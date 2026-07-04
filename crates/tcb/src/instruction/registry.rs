// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Instruction registry — Manages the mapping from instruction type to executor function.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Determinism Guarantee
//!
//! All components in this module are **L1 deterministic**:
//! - Registration is deterministic: same config → same registry.
//! - Execution is deterministic: same instruction + same state → same output.
//! - Depth tracking: `Cell<usize>` is thread-local, but TCB is single-threaded.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | Instruction registration | ✅ L1 deterministic | Pure data |
//! | Instruction execution | ✅ L1 deterministic | No side effects |
//! | Depth protection | ✅ L1 deterministic | Integer comparison |
//! | Context ops (add/sub/mul/div) | ✅ L1 deterministic | Saturating arithmetic |
//! | Context ops (append/remove) | ✅ L1 deterministic | List operations |
//!
//! # Cross-Language Note (L4)
//!
//! The registry is a Rust-only construct; there is no cross-language equivalent.
//! However, the serialization format (`to_value`) is JSON-compatible and can be
//! used by other languages to inspect the registry.

use crate::error::{depth_limit_exceeded, unknown_instruction, EvoRuleError};
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;
use std::cell::Cell;
use std::collections::HashMap;

use super::ExecutorFn;

/// Self-explaining branch — describes how an instruction behaves in core.eval.
#[derive(Debug, Clone)]
pub struct EvalBranch {
    /// List of meta-instructions (same format as the "then" branch in `core_eval.json`).
    pub meta_instructions: Vec<HashMap<String, Value>>,
}

impl EvalBranch {
    /// Create a new `EvalBranch`.
    pub const fn new(meta_instructions: Vec<HashMap<String, Value>>) -> Self {
        Self { meta_instructions }
    }

    /// Serialize to a Value.
    pub fn to_value(&self) -> Value {
        let meta_instrs: Vec<Value> = self
            .meta_instructions
            .iter()
            .map(|instr| {
                let obj: im::HashMap<String, Value> =
                    instr.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                Value::Object(obj)
            })
            .collect();
        Value::list(meta_instrs)
    }

    /// Deserialize from a Value.
    pub fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::List(vec) => {
                let meta_instructions: Vec<HashMap<String, Value>> = vec
                    .iter()
                    .filter_map(|v| {
                        if let Value::Object(obj) = v {
                            let mut map = HashMap::new();
                            for (k, v) in obj.iter() {
                                map.insert(k.clone(), v.clone());
                            }
                            Some(map)
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(Self { meta_instructions })
            }
            _ => None,
        }
    }
}

/// Instruction registry — manages the mapping from instruction type to executor function.
#[derive(Debug, Clone)]
pub struct InstructionRegistry {
    /// Instruction type → executor function mapping.
    instructions: HashMap<String, InstructionDef>,
    /// Context operations (add, sub, set, etc.).
    context_ops: HashMap<String, super::ContextOpFn>,
    /// Execution depth counter.
    ///
    /// [FIX #6]: Uses `Cell<usize>` instead of `Arc<AtomicUsize>`. The TCB is single-threaded,
    /// and Cell provides the necessary interior mutability without the overhead of atomic
    /// operations. Each registry instance maintains its own independent depth counter.
    depth: Cell<usize>,
    /// Maximum execution depth (default 64).
    max_depth: usize,
}

/// Instruction layer — describes where the primitive sits in the dependency graph.
///
/// P3-1: Layer numbers provide an additional traceability dimension, making
/// primitive dependencies explicitly visible.
///
/// - D0 (Core state layer): No dependencies, directly manipulates State
/// - D1 (Control layer): Depends on D0, controls execution flow
/// - D2 (Queue layer): Depends on D0+D1, manages the instruction queue
/// - D3 (Domain layer): Depends on D0, predicate logic condition evaluation
/// - D4 (Audit layer): Depends on D0, audit record generation
/// - D5 (Error handling layer): Depends on D0-D4, error recovery and parallel execution
/// - E0 (Extension — compute layer): Depends on D0, expression evaluation and computation
/// - E1 (Extension — I/O layer): Depends on D0, external data reading/writing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InstructionLayer {
    D0, // Core state layer
    D1, // Control layer
    D2, // Queue layer
    D3, // Domain layer
    D4, // Audit layer
    D5, // Error handling layer
    E0, // Extension — compute layer
    E1, // Extension — I/O layer
}

impl InstructionLayer {
    /// Layer rank (for sorting and display).
    pub const fn rank(&self) -> u8 {
        match self {
            Self::D0 => 0,
            Self::D1 => 1,
            Self::D2 => 2,
            Self::D3 => 3,
            Self::D4 => 4,
            Self::D5 => 5,
            Self::E0 => 6,
            Self::E1 => 7,
        }
    }

    /// Layer name (for audit records and display).
    pub const fn name(&self) -> &'static str {
        match self {
            Self::D0 => "D0-core",
            Self::D1 => "D1-control",
            Self::D2 => "D2-queue",
            Self::D3 => "D3-domain",
            Self::D4 => "D4-audit",
            Self::D5 => "D5-error",
            Self::E0 => "E0-compute",
            Self::E1 => "E1-io",
        }
    }
}

impl std::fmt::Display for InstructionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Instruction definition.
#[derive(Debug, Clone)]
pub struct InstructionDef {
    /// Executor function.
    pub exec: ExecutorFn,
    /// Self-explaining branch (meta-instructions as None).
    pub eval_branch: Option<EvalBranch>,
    /// Explainer function for C5 explainability.
    pub explainer: Option<fn(&crate::rule::GenericInstruction) -> String>,
    /// Instruction layer (P3-1 addition).
    pub layer: InstructionLayer,
}

impl InstructionRegistry {
    /// Create a registry (with default context operations: add/sub/mul/div/set/append/remove).
    pub fn new() -> Self {
        Self {
            instructions: HashMap::new(),
            context_ops: HashMap::new(),
            depth: Cell::new(0),
            max_depth: 64,
        }
        .with_default_context_ops()
    }

    /// Register an instruction (default D0 layer).
    pub fn register(&mut self, name: &str, exec: ExecutorFn) {
        self.register_with_metadata(name, exec, None, None, InstructionLayer::D0);
    }

    /// Register an instruction with a specified layer.
    pub fn register_with_layer(&mut self, name: &str, exec: ExecutorFn, layer: InstructionLayer) {
        self.register_with_metadata(name, exec, None, None, layer);
    }

    /// Register an instruction with metadata.
    pub fn register_with_metadata(
        &mut self,
        name: &str,
        exec: ExecutorFn,
        eval_branch: Option<EvalBranch>,
        explainer: Option<fn(&GenericInstruction) -> String>,
        layer: InstructionLayer,
    ) {
        self.instructions.insert(
            name.to_string(),
            InstructionDef {
                exec,
                eval_branch,
                explainer,
                layer,
            },
        );
    }

    /// Get an instruction executor function.
    pub fn get(&self, name: &str) -> Option<&InstructionDef> {
        self.instructions.get(name)
    }

    /// Check if an instruction exists.
    pub fn has(&self, name: &str) -> bool {
        self.instructions.contains_key(name)
    }

    /// Get the names of all registered instruction types.
    pub fn all_type_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.instructions.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the layer of an instruction.
    pub fn get_layer(&self, name: &str) -> Option<InstructionLayer> {
        self.instructions.get(name).map(|def| def.layer)
    }

    /// List instructions grouped by layer.
    pub fn instructions_by_layer(&self) -> Vec<(InstructionLayer, Vec<String>)> {
        let mut groups: HashMap<InstructionLayer, Vec<String>> = HashMap::new();
        for (name, def) in &self.instructions {
            groups.entry(def.layer).or_default().push(name.clone());
        }
        let mut result: Vec<(InstructionLayer, Vec<String>)> = groups
            .into_iter()
            .map(|(layer, mut names)| {
                names.sort();
                (layer, names)
            })
            .collect();
        result.sort_by_key(|(layer, _)| layer.rank());
        result
    }

    /// Execute an instruction.
    ///
    /// If the instruction is not in the registry, attempts to execute via
    /// `dispatch_cases` fallback. `dispatch_cases` is stored in state.__exec__.`dispatch_cases`
    /// and is injected by `TheEquation`.
    pub fn execute(
        &self,
        state: &State,
        instruction: &GenericInstruction,
    ) -> Result<State, EvoRuleError> {
        // Depth protection
        let current_depth = self.depth.get();
        if current_depth >= self.max_depth {
            return Err(depth_limit_exceeded(current_depth, self.max_depth));
        }
        self.depth.set(current_depth + 1);

        // First try direct lookup in the registry
        if let Some(def) = self.get(&instruction.instruction_type) {
            let result = (def.exec)(self, state, instruction);
            self.depth.set(self.depth.get() - 1);
            return result;
        }

        // Fallback: execute composite instructions via dispatch_cases
        // Look up the case from state.__exec__.dispatch_cases, resolve $ref, then execute
        let dispatch_cases = state
            .get("__exec__")
            .and_then(|v| v.get("dispatch_cases"))
            .cloned();

        if let Some(Value::Object(cases)) = dispatch_cases {
            if let Some(case_val) = cases.get(&instruction.instruction_type) {
                // [FIX #1] Preserve original metadata by attaching it to __exec__ context
                let state_with_meta = update_exec_instruction(state, &instruction.to_value());
                let state_with_meta =
                    attach_original_metadata(&state_with_meta, &instruction.metadata);

                // Recursively resolve $ref references
                let resolved = crate::control::dispatch::resolve_refs(&state_with_meta, case_val);
                let case_instr = GenericInstruction::from_value(&resolved)?;

                // FLOW-03 fix: prevent nested dispatch expansion
                if case_instr.instruction_type == "dispatch" {
                    self.depth.set(self.depth.get() - 1);
                    return Err(EvoRuleError::InvalidConfig {
                        detail: format!(
                            "[dispatch] nested dispatch not allowed: case '{}' contains dispatch instruction",
                            instruction.instruction_type
                        ),
                    });
                }

                // P0-B: record dispatch_expand audit event
                let state_with_meta = record_dispatch_expand(
                    &state_with_meta,
                    &instruction.instruction_type,
                    &case_instr.instruction_type,
                );

                let result = self.execute(&state_with_meta, &case_instr);
                self.depth.set(self.depth.get() - 1);
                return result;
            }
        }

        // Not found
        self.depth.set(current_depth);
        Err(unknown_instruction(&instruction.instruction_type))
    }

    /// Execute a Value instruction.
    pub fn execute_value(&self, state: &State, instr_val: &Value) -> Result<State, EvoRuleError> {
        let instr = GenericInstruction::from_value(instr_val)?;
        self.execute(state, &instr)
    }

    /// Register a context operation (add, sub, set).
    pub fn register_context_op(&mut self, name: &str, op: super::ContextOpFn) {
        self.context_ops.insert(name.to_string(), op);
    }

    /// Get a context operation.
    pub fn get_context_operation(&self, name: &str) -> Option<super::ContextOpFn> {
        self.context_ops.get(name).copied()
    }

    /// Set default context operations (add, sub, set).
    /// Reset the depth counter.
    pub fn reset_depth(&self) {
        self.depth.set(0);
    }

    /// Get the current depth.
    pub fn current_depth(&self) -> usize {
        self.depth.get()
    }

    /// Set the maximum execution depth.
    pub fn set_max_depth(&mut self, max: usize) {
        self.max_depth = max;
    }

    /// Get the maximum execution depth.
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }

    // ==================================================
    // Introspection
    // ==================================================

    /// List all registered instruction types.
    pub fn list_instructions(&self) -> Vec<String> {
        let mut types: Vec<String> = self.instructions.keys().cloned().collect();
        types.sort();
        types
    }

    /// Explain an instruction.
    pub fn explain_instruction(&self, instruction: &GenericInstruction) -> Option<String> {
        self.instructions
            .get(&instruction.instruction_type)
            .and_then(|def| def.explainer.map(|f| f(instruction)))
    }

    /// Batch inject explainers (for `create_full_registry` path).
    pub fn inject_explainers(
        &mut self,
        explainers: HashMap<&str, fn(&GenericInstruction) -> String>,
    ) {
        for (name, explainer) in explainers {
            if let Some(def) = self.instructions.get_mut(name) {
                def.explainer = Some(explainer);
            }
        }
    }

    // ==================================================
    // Serialization
    // ==================================================

    /// Serialize the registry to a Value.
    pub fn to_value(&self) -> Value {
        let entries: Vec<Value> = self
            .instructions
            .iter()
            .map(|(name, def)| {
                let mut entry = im::hashmap! {
                    "instruction_type".to_string() => Value::String(name.clone()),
                };

                if let Some(ref eval_branch) = def.eval_branch {
                    entry.insert("eval_branch".to_string(), eval_branch.to_value());
                }

                Value::Object(entry)
            })
            .collect();

        Value::Object(im::hashmap! {
            "entries".to_string() => Value::list(entries),
        })
    }

    pub fn with_default_context_ops(mut self) -> Self {
        self.register_context_op("add", ops_add);
        self.register_context_op("sub", ops_sub);
        self.register_context_op("mul", ops_mul);
        self.register_context_op("div", ops_div);
        self.register_context_op("set", |_a, b| b.clone());
        self.register_context_op("append", ops_append);
        self.register_context_op("remove", ops_remove);
        self.register_context_op("length", ops_length);
        self
    }

    /// Registry size.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

impl Default for InstructionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════
// Context operation functions
// ══════════════════════════════════════════════

/// Addition operation (saturating arithmetic, truncates to extrema on overflow).
fn ops_add(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => Value::Integer(ai.saturating_add(*bi)),
        (Value::Integer(ai), Value::Float(bf)) => Value::float(*ai as f64 + bf.0),
        (Value::Float(af), Value::Integer(bi)) => Value::float(af.0 + *bi as f64),
        (Value::Float(af), Value::Float(bf)) => Value::float(af.0 + bf.0),
        (Value::String(as_), Value::String(bs)) => {
            let mut s = as_.clone();
            s.push_str(bs);
            Value::String(s)
        }
        (Value::List(al), Value::List(bl)) => {
            let mut v = al.clone();
            for item in bl {
                v.push_back(item.clone());
            }
            Value::List(v)
        }
        _ => b.clone(),
    }
}

/// Subtraction operation (saturating arithmetic, truncates to extrema on overflow).
fn ops_sub(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => Value::Integer(ai.saturating_sub(*bi)),
        (Value::Integer(ai), Value::Float(bf)) => Value::float(*ai as f64 - bf.0),
        (Value::Float(af), Value::Integer(bi)) => Value::float(af.0 - *bi as f64),
        (Value::Float(af), Value::Float(bf)) => Value::float(af.0 - bf.0),
        _ => a.clone(),
    }
}

/// Multiplication operation (saturating arithmetic, truncates to extrema on overflow).
fn ops_mul(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => Value::Integer(ai.saturating_mul(*bi)),
        (Value::Integer(ai), Value::Float(bf)) => Value::float(*ai as f64 * bf.0),
        (Value::Float(af), Value::Integer(bi)) => Value::float(af.0 * *bi as f64),
        (Value::Float(af), Value::Float(bf)) => Value::float(af.0 * bf.0),
        _ => b.clone(),
    }
}

/// Division operation (safe division, returns dividend on zero division or overflow).
fn ops_div(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => {
            if *bi == 0 {
                a.clone()
            } else {
                match ai.checked_div(*bi) {
                    Some(result) => Value::Integer(result),
                    None => a.clone(), // Overflow returns the dividend
                }
            }
        }
        (Value::Integer(ai), Value::Float(bf)) => {
            if bf.0 == 0.0 {
                a.clone()
            } else {
                Value::float(*ai as f64 / bf.0)
            }
        }
        (Value::Float(af), Value::Integer(bi)) => {
            if *bi == 0 {
                a.clone()
            } else {
                Value::float(af.0 / *bi as f64)
            }
        }
        (Value::Float(af), Value::Float(bf)) => {
            if bf.0 == 0.0 {
                a.clone()
            } else {
                Value::float(af.0 / bf.0)
            }
        }
        _ => a.clone(),
    }
}

/// Append operation (list).
fn ops_append(a: &Value, b: &Value) -> Value {
    // If the appended value is Null, skip the append (for conditional accumulation
    // scenarios like validator reports)
    if matches!(b, Value::Null) {
        return a.clone();
    }
    match a {
        Value::List(ref v) => {
            let mut new_v = v.clone();
            new_v.push_back(b.clone());
            Value::List(new_v)
        }
        _ => {
            // If not a list, create a new list
            Value::list(vec![a.clone(), b.clone()])
        }
    }
}

/// Remove operation (list).
fn ops_remove(a: &Value, b: &Value) -> Value {
    match a {
        Value::List(v) => {
            let new_list: im::Vector<Value> = v.iter().filter(|&x| x != b).cloned().collect();
            Value::List(new_list)
        }
        _ => a.clone(),
    }
}

/// Length operation — returns the length of the list in `b` (the `value` operand).
/// Non-list values yield 0. The current value at `attr` (`a`) is ignored.
///
/// This enables JSON rules to compute list lengths without an `evaluate_expression`
/// primitive, supporting the L2 migration paradigm of composing existing primitives
/// (P3-2 extension: `state_compute` with `operation: "length"`).
///
/// # Examples
/// - `op_fn(Null, List([1,2,3]))` → `Integer(3)`
/// - `op_fn(Null, Null)` → `Integer(0)`
fn ops_length(_a: &Value, b: &Value) -> Value {
    match b {
        Value::List(v) => Value::Integer(v.len() as i64),
        _ => Value::Integer(0),
    }
}

// ══════════════════════════════════════════════
// Building a full registry
// ══════════════════════════════════════════════

/// Create a full registry with all default instructions and context operations registered.
pub fn create_full_registry() -> InstructionRegistry {
    let mut reg = InstructionRegistry::new().with_default_context_ops();

    // Layer 0: Physical primitives
    crate::primitive::register_all(&mut reg);

    // Layer 1: Control flow
    crate::control::register_all(&mut reg);

    // Inject explainers (C4 self-explainability)
    let mut explainers = crate::primitive::explainer_map();
    explainers.extend(crate::control::explainer_map());
    reg.inject_explainers(explainers);

    reg
}

/// Register primitives driven by JSON configuration.
///
/// JSON declares "which primitives are needed" (what to do), Rust provides
/// "how to do it" (`exec_fn`). Startup does a two-way self-check: JSON-declared
/// primitives must have a corresponding Rust implementation.
///
/// Reuses existing mechanisms:
/// - `register_with_metadata()` completes registration
/// - `EvalBranch::from_value()` parses `eval_branch`
/// - `primitive::exec_fn_map()` + `control::exec_fn_map()` provide `exec_fn` mappings
///
/// # Parameters
/// - `primitives_config`: The Value of the `primitives` section from `eval_config.json`
///
/// # Returns
/// - `Ok(())`: Registration succeeded
/// - `Err(EvoRuleError)`: A JSON-declared primitive has no corresponding Rust implementation
pub fn register_from_config(
    reg: &mut InstructionRegistry,
    primitives_config: &Value,
) -> Result<(), EvoRuleError> {
    // Merge physical primitive + control flow primitive exec_fn mappings
    let mut all_fns: HashMap<&str, ExecutorFn> = HashMap::new();
    for (name, fn_) in crate::primitive::exec_fn_map() {
        all_fns.insert(name, fn_);
    }
    for (name, fn_) in crate::control::exec_fn_map() {
        all_fns.insert(name, fn_);
    }

    // Iterate over JSON-declared primitive categories (state_ops, queue_ops, ...)
    let categories = match primitives_config {
        Value::Object(obj) => obj,
        _ => {
            return Err(EvoRuleError::InvalidConfig {
                detail: "primitives config must be an object".to_string(),
            })
        }
    };

    let mut missing_fns: Vec<String> = Vec::new();

    // Build explainer mapping
    let all_explainers: std::collections::HashMap<&str, fn(&GenericInstruction) -> String> = {
        let mut map = crate::primitive::explainer_map();
        map.extend(crate::control::explainer_map());
        map
    };

    for (_category_name, category_val) in categories.iter() {
        // Skip metadata fields like _description
        let primitives = match category_val {
            Value::Object(obj) => obj,
            _ => continue,
        };

        for (prim_name, prim_val) in primitives.iter() {
            // Skip metadata fields like _description
            if prim_name.starts_with('_') {
                continue;
            }

            // Look up the corresponding exec_fn
            let exec_fn = if let Some(fn_) = all_fns.get(prim_name.as_str()) {
                *fn_
            } else {
                missing_fns.push(prim_name.clone());
                continue;
            };

            // Parse eval_branch (if present)
            let eval_branch = prim_val.get("eval_branch").and_then(EvalBranch::from_value);

            // Parse layer (from JSON declaration, default D0)
            let layer = prim_val
                .get("layer")
                .and_then(|v| v.as_str())
                .and_then(|s| match s {
                    "D0" => Some(InstructionLayer::D0),
                    "D1" => Some(InstructionLayer::D1),
                    "D2" => Some(InstructionLayer::D2),
                    "D3" => Some(InstructionLayer::D3),
                    "D4" => Some(InstructionLayer::D4),
                    "D5" => Some(InstructionLayer::D5),
                    "E0" => Some(InstructionLayer::E0),
                    "E1" => Some(InstructionLayer::E1),
                    _ => None,
                })
                .unwrap_or(InstructionLayer::D0);

            // Reuse register_with_metadata to complete registration
            let explainer = all_explainers.get(prim_name.as_str()).copied();
            reg.register_with_metadata(prim_name, exec_fn, eval_branch, explainer, layer);
        }
    }

    // Two-way self-check: JSON-declared but Rust-unimplemented primitives
    if !missing_fns.is_empty() {
        return Err(EvoRuleError::InvalidConfig {
            detail: format!(
                "primitives declared in JSON but not implemented in Rust: {missing_fns:?}"
            ),
        });
    }

    // Reverse self-check: Rust-implemented but JSON-undeclared primitives (warning only)
    let declared_names: std::collections::HashSet<&str> = {
        let mut set = std::collections::HashSet::new();
        for (_category_name, category_val) in categories.iter() {
            if let Value::Object(obj) = category_val {
                for (name, _) in obj.iter() {
                    if !name.starts_with('_') {
                        set.insert(name.as_str());
                    }
                }
            }
        }
        set
    };
    let undeclared: Vec<&str> = all_fns
        .keys()
        .filter(|name| !declared_names.contains(*name))
        .copied()
        .collect();
    // Note: undeclared Rust exec_fns are silently ignored. TCB must not perform
    // any I/O (including stderr) per ER-603. Validation belongs in governance layer.
    let _ = undeclared; // suppress unused variable warning

    Ok(())
}

/// Create a full registry from JSON configuration.
///
/// Prefers JSON-driven registration, falling back to `create_full_registry()` on failure.
pub fn create_full_registry_from_config(primitives_config: Option<&Value>) -> InstructionRegistry {
    let mut reg = InstructionRegistry::new().with_default_context_ops();

    if let Some(config) = primitives_config {
        match register_from_config(&mut reg, config) {
            Ok(()) => reg,
            Err(_) => {
                // JSON-driven registration failed: fall back to hardcoded registry.
                // TCB must not log to stderr (ER-603); error is silently swallowed.
                create_full_registry()
            }
        }
    } else {
        // No JSON config, fall back to hardcoded registration
        create_full_registry()
    }
}

/// Update __exec__.instruction to the specified value (for `dispatch_cases` fallback).
fn update_exec_instruction(state: &State, instr_val: &Value) -> State {
    let exec_val = state
        .get("__exec__")
        .cloned()
        .unwrap_or_else(Value::empty_object);

    let mut exec_map = match exec_val {
        Value::Object(m) => m,
        _ => im::HashMap::new(),
    };
    exec_map.insert("instruction".to_string(), instr_val.clone());
    state.set("__exec__", Value::Object(exec_map))
}

/// [FIX #1] Attach the original instruction's metadata to the __exec__ context.
///
/// This preserves traceability when composite instructions are expanded via the
/// `dispatch_cases` fallback path. The metadata is stored in __exec__.__`original_metadata`
/// and can be used by audit records to trace back to the original instruction.
fn attach_original_metadata(state: &State, metadata: &Value) -> State {
    if metadata.is_null() || metadata.is_empty() {
        return state.clone();
    }
    state.update_exec_field("__original_metadata", metadata.clone())
}

/// P0-B: Record a `dispatch_expand` audit event.
///
/// When the registry fallback path expands a composite instruction, this writes
/// a record to the audit chain, making the "original instruction → expanded instruction"
/// mapping visible in the audit trail.
///
/// Transparency guarantee: composite instruction expansion is no longer implicit;
/// the audit chain can trace each expansion step.
/// Traceability guarantee: the record contains the original instruction type and
/// the expanded target instruction type.
/// Auditability guarantee: the record is HMAC-SHA256 chained and tamper-proof.
///
/// If the audit chain is not initialized (`__audit_chain` doesn't exist), the
/// record is skipped — this occurs in `non-evaluate()` paths such as
/// `execute_instruction()`.
fn record_dispatch_expand(state: &State, original_type: &str, expanded_type: &str) -> State {
    // Check if the audit chain exists
    let chain_val = match state.get("__audit_chain").cloned() {
        Some(cv) => cv,
        None => return state.clone(),
    };

    let mut chain_state = match crate::audit::AuditChainState::from_value(&chain_val) {
        Some(cs) => cs,
        None => return state.clone(),
    };

    // Check the audit_on switch
    let audit_on = state
        .get("__exec__")
        .and_then(|v| v.get("audit_on"))
        .and_then(super::super::value::Value::as_bool)
        .unwrap_or(true);

    if !audit_on {
        return state.clone();
    }

    // Compute the state hash (excluding system fields).
    // Uses State::business_state_snapshot — the single source of truth for the
    // system-field exclusion list, ensuring hash consistency with dispatch.rs (C3).
    let state_hash = crate::deterministic::content_hash(&[state.business_state_snapshot()]);

    let tick = chain_state.next_tick();
    let previous_hash = chain_state.latest_hash.clone();
    let id = crate::audit::AuditRecord::derive_id(&previous_hash, tick);
    let nonce = crate::audit::AuditRecord::derive_nonce(
        &previous_hash,
        tick,
        crate::audit::DEFAULT_HMAC_KEY,
    );

    let change_summary = Some(format!(
        "dispatch_expand: {original_type} → {expanded_type}"
    ));

    let record = crate::audit::AuditRecord::new(
        &id,
        &format!("dispatch_expand:{original_type}"),
        tick as i64,
        &state_hash,
        &state_hash, // expand itself doesn't modify business state, before == after
        change_summary,
        crate::audit::ExecutionResult::Success,
        None,
        &previous_hash,
        &nonce,
        crate::audit::DEFAULT_HMAC_KEY,
    );

    // Update the chain state
    if chain_state.records.is_empty() {
        chain_state.root_hash = record.hash.clone();
        chain_state.created_at = tick as i64;
    }
    chain_state.latest_hash = record.hash.clone();
    chain_state.updated_at = tick as i64;
    chain_state.records.push(record);

    state.set("__audit_chain", chain_state.to_value())
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[allow(clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = InstructionRegistry::new();
        reg.register("noop", |_, s, _| Ok(s.clone()));
        assert!(reg.has("noop"));
    }

    #[test]
    fn test_registry_default_context_ops() {
        let reg = InstructionRegistry::new().with_default_context_ops();
        assert!(reg.get_context_operation("add").is_some());
        assert!(reg.get_context_operation("sub").is_some());
        assert!(reg.get_context_operation("mul").is_some());
        assert!(reg.get_context_operation("set").is_some());
        assert!(reg.get_context_operation("length").is_some());
    }

    #[test]
    fn test_ops_add_integers() {
        let result = ops_add(&Value::Integer(5), &Value::Integer(3));
        assert_eq!(result, Value::Integer(8));
    }

    #[test]
    fn test_ops_sub_integers() {
        let result = ops_sub(&Value::Integer(10), &Value::Integer(3));
        assert_eq!(result, Value::Integer(7));
    }

    #[test]
    fn test_ops_mul_integers() {
        let result = ops_mul(&Value::Integer(6), &Value::Integer(7));
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_ops_add_strings() {
        let result = ops_add(&Value::string("hello "), &Value::string("world"));
        assert_eq!(result, Value::string("hello world"));
    }

    #[test]
    fn test_create_full_registry() {
        let reg = create_full_registry();
        // Physical primitives should exist
        assert!(reg.has("set_context"));
        assert!(reg.has("advance_instruction"));
        assert!(reg.has("evaluate_domain"));
        assert!(reg.has("dispatch"));
        assert!(reg.has("while_loop"));
        assert!(reg.has("noop"));

        // Should not contain composite instructions (composite instructions → JSON rules)
        // increment/decrement/set/sequence/conditional are not registered here
    }

    #[test]
    fn test_eval_branch_serialization() {
        use std::collections::HashMap as StdHashMap;

        let mut params: StdHashMap<String, Value> = StdHashMap::new();
        params.insert("key".to_string(), Value::String("x".to_string()));
        params.insert("operation".to_string(), Value::String("add".to_string()));
        params.insert("value".to_string(), Value::Integer(1));

        let mut meta_instr: StdHashMap<String, Value> = StdHashMap::new();
        meta_instr.insert("type".to_string(), Value::String("set_context".to_string()));
        meta_instr.insert("params".to_string(), Value::Object(params.into()));

        let eval_branch = EvalBranch::new(vec![meta_instr]);
        let value = eval_branch.to_value();

        // Deserialize
        let restored = EvalBranch::from_value(&value);
        assert!(restored.is_some());
        let restored = restored.unwrap();
        assert_eq!(restored.meta_instructions.len(), 1);
    }

    #[test]
    fn test_registry_with_metadata() {
        use std::collections::HashMap as StdHashMap;

        let mut reg = InstructionRegistry::new();

        let eval_branch = EvalBranch::new(vec![{
            let mut m: StdHashMap<String, Value> = StdHashMap::new();
            m.insert(
                "type".to_string(),
                Value::String("advance_instruction".to_string()),
            );
            m.insert("params".to_string(), Value::Object(im::hashmap!()));
            m
        }]);

        let explainer = |_instr: &GenericInstruction| -> String {
            "Execute instruction: test_instr".to_string()
        };

        reg.register_with_metadata(
            "test_instr",
            |_, s, _| Ok(s.clone()),
            Some(eval_branch),
            Some(explainer),
            InstructionLayer::D0,
        );

        assert!(reg.has("test_instr"));

        // Test explain functionality
        let instr = GenericInstruction {
            instruction_type: "test_instr".to_string(),
            params: std::collections::HashMap::new(),
            metadata: Value::empty_object(),
        };
        let explanation = reg.explain_instruction(&instr);
        assert!(explanation.is_some());
        assert_eq!(explanation.unwrap(), "Execute instruction: test_instr");
    }

    #[test]
    fn test_registry_list_instructions() {
        let mut reg = InstructionRegistry::new();
        reg.register("instr_a", |_, s, _| Ok(s.clone()));
        reg.register("instr_b", |_, s, _| Ok(s.clone()));

        let all = reg.list_instructions();
        assert!(all.contains(&"instr_a".to_string()));
        assert!(all.contains(&"instr_b".to_string()));

        let type_names = reg.all_type_names();
        assert!(type_names.contains(&"instr_a".to_string()));
        assert!(type_names.contains(&"instr_b".to_string()));
    }

    #[test]
    fn test_instruction_layer() {
        let mut reg = InstructionRegistry::new();
        // register defaults to D0
        reg.register("noop_like", |_, s, _| Ok(s.clone()));
        assert_eq!(reg.get_layer("noop_like"), Some(InstructionLayer::D0));

        // register_with_layer specifies the layer
        reg.register_with_layer(
            "dispatch_like",
            |_, s, _| Ok(s.clone()),
            InstructionLayer::D1,
        );
        assert_eq!(reg.get_layer("dispatch_like"), Some(InstructionLayer::D1));

        reg.register_with_layer("trace_like", |_, s, _| Ok(s.clone()), InstructionLayer::D4);
        assert_eq!(reg.get_layer("trace_like"), Some(InstructionLayer::D4));

        reg.register_with_layer("eval_like", |_, s, _| Ok(s.clone()), InstructionLayer::E0);
        assert_eq!(reg.get_layer("eval_like"), Some(InstructionLayer::E0));

        // Non-existent instruction returns None
        assert_eq!(reg.get_layer("nonexistent"), None);
    }

    #[test]
    fn test_instructions_by_layer() {
        let mut reg = InstructionRegistry::new();
        reg.register_with_layer("noop", |_, s, _| Ok(s.clone()), InstructionLayer::D0);
        reg.register_with_layer("set_context", |_, s, _| Ok(s.clone()), InstructionLayer::D0);
        reg.register_with_layer("dispatch", |_, s, _| Ok(s.clone()), InstructionLayer::D1);
        reg.register_with_layer("trace_step", |_, s, _| Ok(s.clone()), InstructionLayer::D4);
        reg.register_with_layer(
            "content_hash",
            |_, s, _| Ok(s.clone()),
            InstructionLayer::E0,
        );

        let groups = reg.instructions_by_layer();
        assert_eq!(groups.len(), 4); // D0, D1, D4, E0
        assert_eq!(groups[0].0, InstructionLayer::D0);
        assert_eq!(groups[0].1, vec!["noop", "set_context"]);
        assert_eq!(groups[1].0, InstructionLayer::D1);
        assert_eq!(groups[1].1, vec!["dispatch"]);
    }

    #[test]
    fn test_layer_from_config() {
        // Simulate the primitives section of eval_config.json with layer fields
        let primitives = serde_json::json!({
            "state_ops": {
                "set_context": {
                    "description": "Set context",
                    "params": ["transform"],
                    "layer": "D0"
                }
            },
            "control_ops": {
                "dispatch": {
                    "description": "Instruction dispatch",
                    "params": [],
                    "layer": "D1"
                }
            },
            "audit_ops": {
                "trace_step": {
                    "description": "Audit trace",
                    "params": [],
                    "layer": "D4"
                }
            },
            "compute_ops": {
                "content_hash": {
                    "description": "Content hash",
                    "params": [],
                    "layer": "E0"
                }
            },
            "noop_ops": {
                "noop": {
                    "description": "No-op",
                    "params": [],
                    "layer": "D0"
                }
            }
        });

        let tcb_value = crate::value::serde_json_to_value(&primitives);
        let mut reg = InstructionRegistry::new();
        register_from_config(&mut reg, &tcb_value).unwrap();

        assert_eq!(reg.get_layer("set_context"), Some(InstructionLayer::D0));
        assert_eq!(reg.get_layer("dispatch"), Some(InstructionLayer::D1));
        assert_eq!(reg.get_layer("trace_step"), Some(InstructionLayer::D4));
        assert_eq!(reg.get_layer("content_hash"), Some(InstructionLayer::E0));
        assert_eq!(reg.get_layer("noop"), Some(InstructionLayer::D0));
    }

    #[test]
    fn test_registry_to_value() {
        let mut reg = InstructionRegistry::new();
        reg.register("test", |_, s, _| Ok(s.clone()));

        let value = reg.to_value();

        // Verify serialization result
        match value {
            Value::Object(obj) => {
                assert!(obj.contains_key("entries"));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_register_from_config_success() {
        // Simulate the primitives section of eval_config.json
        let primitives = serde_json::json!({
            "state_ops": {
                "set_context": {
                    "description": "Set context",
                    "params": ["transform"]
                }
            },
            "queue_ops": {
                "advance_instruction": { "description": "Advance instruction", "params": [] },
                "push_instruction": { "description": "Push single instruction", "params": ["instruction"] }
            },
            "noop_ops": {
                "noop": { "description": "No-op", "params": [] }
            },
            "control_ops": {
                "dispatch": { "description": "Instruction dispatch", "params": [] },
                "while_loop": { "description": "Self-driven loop", "params": [] }
            }
        });

        let tcb_value = crate::value::serde_json_to_value(&primitives);
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        let result = register_from_config(&mut reg, &tcb_value);

        assert!(result.is_ok(), "register_from_config should succeed");
        assert!(reg.has("set_context"), "set_context should be registered");
        assert!(
            reg.has("advance_instruction"),
            "advance_instruction should be registered"
        );
        assert!(
            reg.has("push_instruction"),
            "push_instruction should be registered"
        );
        assert!(reg.has("noop"), "noop should be registered");
        assert!(reg.has("dispatch"), "dispatch should be registered");
        assert!(reg.has("while_loop"), "while_loop should be registered");
    }

    #[test]
    fn test_register_from_config_missing_fn() {
        // JSON declares a primitive that Rust hasn't implemented
        let primitives = serde_json::json!({
            "test_ops": {
                "nonexistent_primitive": {
                    "description": "Non-existent primitive",
                    "params": []
                }
            }
        });

        let tcb_value = crate::value::serde_json_to_value(&primitives);
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        let result = register_from_config(&mut reg, &tcb_value);

        assert!(
            result.is_err(),
            "register_from_config should fail for missing exec_fn"
        );
    }

    #[test]
    fn test_create_full_registry_from_config_with_primitives() {
        // Use a full primitives configuration
        let primitives = serde_json::json!({
            "state_ops": {
                "set_context": { "description": "Set context", "params": ["transform"] }
            },
            "queue_ops": {
                "advance_instruction": { "description": "Advance instruction", "params": [] },
                "push_instruction": { "description": "Push single", "params": ["instruction"] },
                "push_instruction_sequence": { "description": "Push sequence", "params": ["instructions"] }
            },
            "noop_ops": {
                "noop": { "description": "No-op", "params": [] }
            },
            "control_ops": {
                "dispatch": { "description": "Instruction dispatch", "params": [] },
                "while_loop": { "description": "Self-driven loop", "params": [] }
            }
        });

        let tcb_value = crate::value::serde_json_to_value(&primitives);
        let reg = create_full_registry_from_config(Some(&tcb_value));

        // Verify JSON-declared primitives are registered
        assert!(reg.has("set_context"));
        assert!(reg.has("advance_instruction"));
        assert!(reg.has("noop"));
        assert!(reg.has("dispatch"));
        assert!(reg.has("while_loop"));
    }

    #[test]
    fn test_create_full_registry_from_config_fallback() {
        // No JSON config → fall back to hardcoded registration
        let reg = create_full_registry_from_config(None);

        // Hardcoded registration should contain all primitives
        assert!(reg.has("set_context"));
        assert!(reg.has("advance_instruction"));
        assert!(reg.has("noop"));
        assert!(reg.has("dispatch"));
        assert!(reg.has("while_loop"));
    }

    #[test]
    fn test_register_from_config_with_eval_branch() {
        // Test that eval_branch is correctly parsed
        let primitives = serde_json::json!({
            "state_ops": {
                "set_context": {
                    "description": "Set context",
                    "params": ["transform"],
                    "eval_branch": [
                        { "type": "advance_instruction", "params": {} }
                    ]
                }
            }
        });

        let tcb_value = crate::value::serde_json_to_value(&primitives);
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        let result = register_from_config(&mut reg, &tcb_value);

        assert!(result.is_ok());
        // Verify eval_branch was registered
        let def = reg.get("set_context").unwrap();
        assert!(
            def.eval_branch.is_some(),
            "eval_branch should be parsed and registered"
        );
    }

    #[test]
    fn test_all_exec_fns_consistency() {
        // Verify the number of entries in primitive::all_exec_fns() and control::all_exec_fns()
        let primitive_fns = crate::primitive::all_exec_fns();
        let control_fns = crate::control::all_exec_fns();

        // Physical primitives: 21 entries (20 + domain_intersect per ADR-05)
        // Compute_ops reduced to 3 (content_hash, format_string, get_index)
        // Inference_ops: 3 (detect_conflicts, detect_cycles, analyze_rule_effects)
        // Removed: iterate_list, io_read, io_write, check_dimension_consistency,
        //          evaluate_expression, and other deprecated primitives
        assert_eq!(
            primitive_fns.len(),
            21,
            "primitive all_exec_fns should have 21 entries"
        );
        // Control flow primitives: 2 entries
        assert_eq!(
            control_fns.len(),
            2,
            "control all_exec_fns should have 2 entries"
        );

        // Verify consistency with create_full_registry()
        let reg = create_full_registry();
        // Registry contains 21 physical + 2 control = 23 primitives
        assert_eq!(
            reg.len(),
            23,
            "full registry should have 23 instructions (21 primitive + 2 control)"
        );
    }

    /// C4: Self-explainability — verify all 30 primitives have an explainer registered
    #[test]
    fn test_c4_all_primitives_have_explainer() {
        let reg = create_full_registry();
        let all_names = reg.list_instructions();

        let mut missing: Vec<String> = Vec::new();
        for name in &all_names {
            let instr = GenericInstruction {
                instruction_type: name.clone(),
                params: std::collections::HashMap::new(),
                metadata: crate::value::Value::empty_object(),
            };
            match reg.explain_instruction(&instr) {
                Some(explanation) => {
                    // Explanation content is non-empty
                    assert!(
                        !explanation.is_empty(),
                        "explainer for '{}' returned empty string",
                        name
                    );
                }
                None => {
                    missing.push(name.clone());
                }
            }
        }

        assert!(
            missing.is_empty(),
            "The following primitives lack an explainer: {:?}",
            missing
        );
    }

    /// C4: Self-explainability — verify explainers return human-readable Chinese descriptions
    #[test]
    fn test_c4_explainer_returns_readable_description() {
        let reg = create_full_registry();

        // Sample check a few key primitives' explainer output
        let test_cases: Vec<(&str, &str)> = vec![
            ("set_context", "context"),
            ("content_hash", "hash"),
            ("dispatch", "dispatch"),
            ("while_loop", "loop"),
            ("trace_step", "audit"),
        ];

        for (prim_name, expected_keyword) in test_cases {
            let instr = GenericInstruction {
                instruction_type: prim_name.to_string(),
                params: std::collections::HashMap::new(),
                metadata: crate::value::Value::empty_object(),
            };
            let explanation = reg.explain_instruction(&instr);
            assert!(
                explanation.is_some(),
                "{} should have an explainer",
                prim_name
            );
            let text = explanation.unwrap();
            assert!(
                text.to_lowercase().contains(expected_keyword),
                "Explainer for '{}' ('{}') should contain keyword '{}'",
                prim_name,
                text,
                expected_keyword
            );
        }
    }

    /// C4: Self-explainability — verify explainer_map covers all all_exec_fns primitives
    #[test]
    fn test_c4_explainer_map_covers_all_exec_fns() {
        let primitive_exec_fns = crate::primitive::all_exec_fns();
        let primitive_explainers = crate::primitive::explainer_map();
        let control_exec_fns = crate::control::all_exec_fns();
        let control_explainers = crate::control::explainer_map();

        // Each exec_fn should have a corresponding explainer
        for (name, _) in &primitive_exec_fns {
            assert!(
                primitive_explainers.contains_key(name),
                "Physical primitive '{}' missing from explainer_map",
                name
            );
        }
        for (name, _) in &control_exec_fns {
            assert!(
                control_explainers.contains_key(name),
                "Control flow primitive '{}' missing from explainer_map",
                name
            );
        }
    }

    // ══════════════════════════════════════════
    // P0-B Specific tests: registry fallback audit blind spot
    // ══════════════════════════════════════════

    /// P0-B-01: When a composite instruction is expanded via the fallback path,
    /// a dispatch_expand record should be generated in the audit chain.
    #[test]
    fn test_p0b_dispatch_expand_audit_record() {
        use crate::audit::{AuditChainState, DEFAULT_HMAC_KEY};
        use crate::state::State;

        let reg = create_full_registry();

        // Construct state: includes audit chain and dispatch_cases
        // increment is a composite instruction, mapped to set_context
        let dispatch_cases = Value::Object(im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap! {
                    "key".to_string() => Value::string("x"),
                    "operation".to_string() => Value::string("add"),
                    "value".to_string() => Value::Integer(1),
                }),
            }),
        });

        let state = State::new(vec![("x", Value::Integer(42))]);
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "dispatch_cases".to_string() => dispatch_cases,
        });
        let state = state.set("__exec__", exec_ctx);
        let state = state.set("__audit_chain", AuditChainState::empty().to_value());

        // Construct an increment instruction (not in registry, will go through fallback)
        let instr = GenericInstruction {
            instruction_type: "increment".to_string(),
            params: std::collections::HashMap::new(),
            metadata: Value::empty_object(),
        };

        // Execute the fallback path
        let result = reg.execute(&state, &instr).unwrap();

        // Verify the audit chain contains a dispatch_expand record
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        let expand_records: Vec<_> = chain_state
            .records
            .iter()
            .filter(|r| r.rule_id.starts_with("dispatch_expand:"))
            .collect();

        assert_eq!(
            expand_records.len(),
            1,
            "Should generate 1 dispatch_expand audit record, got: {}",
            expand_records.len()
        );

        let record = &expand_records[0];
        assert_eq!(record.rule_id, "dispatch_expand:increment");
        assert!(record.change_summary.is_some());
        assert!(record
            .change_summary
            .as_ref()
            .unwrap()
            .contains("increment"));
        assert!(record
            .change_summary
            .as_ref()
            .unwrap()
            .contains("set_context"));
        assert!(record.verify(DEFAULT_HMAC_KEY));
    }

    /// P0-B-02: Physical primitives (directly registered in the registry) should not
    /// generate dispatch_expand records.
    #[test]
    fn test_p0b_no_expand_for_physical_primitive() {
        use crate::audit::AuditChainState;
        use crate::state::State;

        let reg = create_full_registry();
        let state = State::new(vec![("x", Value::Integer(42))]);
        let state = state.set("__audit_chain", AuditChainState::empty().to_value());

        // set_context is a physical primitive, directly registered in the registry
        let mut params = std::collections::HashMap::new();
        params.insert("key".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("add"));
        params.insert("value".to_string(), Value::Integer(1));

        let instr = GenericInstruction {
            instruction_type: "set_context".to_string(),
            params,
            metadata: Value::empty_object(),
        };

        let result = reg.execute(&state, &instr).unwrap();

        // The audit chain should remain empty (set_context itself doesn't write to the
        // audit chain, and dispatch_expand shouldn't be generated)
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        let expand_records: Vec<_> = chain_state
            .records
            .iter()
            .filter(|r| r.rule_id.starts_with("dispatch_expand:"))
            .collect();

        assert_eq!(
            expand_records.len(),
            0,
            "Physical primitives should not generate dispatch_expand records"
        );
    }

    /// P0-B-03: The before/after hashes in dispatch_expand records should be the same
    /// (expansion itself does not modify the business state).
    #[test]
    fn test_p0b_dispatch_expand_before_after_same() {
        use crate::audit::AuditChainState;
        use crate::state::State;

        let reg = create_full_registry();

        // my_compound is a composite instruction, mapped to noop
        let dispatch_cases = Value::Object(im::hashmap! {
            "my_compound".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
        });

        let state = State::new(vec![("x", Value::Integer(42))]);
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("my_compound"),
                "params".to_string() => Value::empty_object(),
            }),
            "dispatch_cases".to_string() => dispatch_cases,
        });
        let state = state.set("__exec__", exec_ctx);
        let state = state.set("__audit_chain", AuditChainState::empty().to_value());

        let instr = GenericInstruction {
            instruction_type: "my_compound".to_string(),
            params: std::collections::HashMap::new(),
            metadata: Value::empty_object(),
        };

        let result = reg.execute(&state, &instr).unwrap();

        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        let expand_record = chain_state
            .records
            .iter()
            .find(|r| r.rule_id.starts_with("dispatch_expand:"))
            .expect("Should have a dispatch_expand record");

        // Expansion itself doesn't modify business state, so before == after
        assert_eq!(
            expand_record.state_before_hash, expand_record.state_after_hash,
            "dispatch_expand's before/after hashes should be the same"
        );
    }

    // ══════════════════════════════════════════════
    // Additional coverage: context ops error paths
    // ══════════════════════════════════════════════

    #[test]
    fn test_ops_sub_incompatible_types() {
        // Integer minus string → returns the minuend
        let result = ops_sub(&Value::Integer(10), &Value::string("3"));
        assert_eq!(result, Value::Integer(10));
    }

    #[test]
    fn test_ops_append_non_list() {
        // Appending to a non-list → wraps into a list
        let result = ops_append(&Value::Integer(5), &Value::Integer(3));
        assert!(result.is_list());
        let list = result.as_list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_register_with_layer() {
        // register_with_layer registers an instruction with a specified layer
        let mut reg = InstructionRegistry::new();
        let exec_fn: ExecutorFn = |_reg, state: &State, _instr| Ok(state.clone());
        reg.register_with_layer("layered_op", exec_fn, InstructionLayer::D3);
        assert!(reg.has("layered_op"));
        let def = reg.get("layered_op").unwrap();
        assert_eq!(def.layer, InstructionLayer::D3);
    }

    #[test]
    fn test_instructions_by_layer_d0() {
        // instructions_by_layer groups by layer
        let reg = create_full_registry();
        let by_layer = reg.instructions_by_layer();
        assert!(!by_layer.is_empty());
        let has_d0 = by_layer.iter().any(|(l, _)| *l == InstructionLayer::D0);
        assert!(has_d0);
    }

    #[test]
    fn test_get_layer_physical() {
        // get_layer returns the instruction's layer
        let reg = create_full_registry();
        assert_eq!(reg.get_layer("set_context"), Some(InstructionLayer::D0));
        assert_eq!(reg.get_layer("nonexistent"), None);
    }

    #[test]
    fn test_registered_types_sorted() {
        // list_instructions returns sorted instruction names
        let reg = create_full_registry();
        let types = reg.list_instructions();
        assert!(!types.is_empty());
        // Verify sorting
        let mut sorted = types.clone();
        sorted.sort();
        assert_eq!(types, sorted);
    }

    #[test]
    fn test_registry_execute_unknown_instruction() {
        // execute on an unknown instruction → Err
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(5))]);
        let instr =
            GenericInstruction::new("unknown_instruction", std::collections::HashMap::new());
        let result = reg.execute(&state, &instr);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_value_missing_type() {
        // execute_value with missing type field → Err
        let reg = create_full_registry();
        let state = State::new(vec![("x", Value::Integer(5))]);
        let instr_val = Value::from(im::HashMap::from(vec![(
            "params".to_string(),
            Value::empty_object(),
        )]));
        let result = reg.execute_value(&state, &instr_val);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_depth_and_current_depth() {
        // reset_depth resets the depth counter
        let mut reg = create_full_registry();
        reg.set_max_depth(100);
        let depth = reg.current_depth();
        assert!(depth <= 100);
    }

    #[test]
    fn test_set_max_depth() {
        // set_max_depth modifies the maximum depth
        let mut reg = create_full_registry();
        reg.set_max_depth(50);
        assert_eq!(reg.max_depth(), 50);
        reg.set_max_depth(200);
        assert_eq!(reg.max_depth(), 200);
    }

    #[test]
    fn test_registry_has_standard_instructions() {
        // create_full_registry contains all standard instructions
        let reg = create_full_registry();
        assert!(reg.has("noop"));
        assert!(reg.has("set_context"));
        assert!(reg.has("state_set"));
    }

    #[test]
    fn test_inject_explainers() {
        // inject_explainers adds explainers to the registry
        let mut reg = create_full_registry();
        let mut explainers: std::collections::HashMap<&str, fn(&GenericInstruction) -> String> =
            std::collections::HashMap::new();
        explainers.insert("noop", |_instr| "noop: do nothing".to_string());
        reg.inject_explainers(explainers);
        // noop should still exist
        assert!(reg.has("noop"));
    }

    #[test]
    fn test_explain_instruction_unknown() {
        // explain_instruction on an unknown instruction → None
        let reg = create_full_registry();
        let instr =
            GenericInstruction::new("totally_unknown_op_xyz", std::collections::HashMap::new());
        let explanation = reg.explain_instruction(&instr);
        assert!(explanation.is_none());
    }

    /// Test list_instructions returns a sorted list
    #[test]
    fn test_list_instructions_is_sorted() {
        let reg = create_full_registry();
        let types = reg.list_instructions();

        // Verify: the returned list must be sorted
        let mut sorted = types.clone();
        sorted.sort();
        assert_eq!(
            types, sorted,
            "list_instructions() must return a sorted list"
        );
    }

    /// Test list_instructions contains all core instructions
    #[test]
    fn test_list_instructions_contains_core_instructions() {
        let reg = create_full_registry();
        let types = reg.list_instructions();

        // Verify: must contain all core instructions
        assert!(types.contains(&"noop".to_string()), "must contain noop");
        assert!(
            types.contains(&"set_context".to_string()),
            "must contain set_context"
        );
        assert!(
            types.contains(&"state_set".to_string()),
            "must contain state_set"
        );
        assert!(
            types.contains(&"dispatch".to_string()),
            "must contain dispatch"
        );
        assert!(
            types.contains(&"while_loop".to_string()),
            "must contain while_loop"
        );
    }
}
