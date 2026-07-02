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

//! Execution context — auxiliary data structure for instruction execution.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Design Principles
//!
//! `ExecContext` wraps the `__exec__` internal state, providing contextual information
//! for instruction execution. Includes the current instruction, instruction queue,
//! running flag, dispatch table, audit switch, etc.
//!
//! P2-1: Extended to a fully typed accessor for `__exec__`, unifying the scattered
//! `update_exec_field` functions across 3 files and eliminating the risk of typos
//! from manual path resolution.
//!
//! # Determinism Guarantee
//!
//! All operations on `ExecContext` are **L1 deterministic**:
//! - Same input → same output.
//! - No randomness, no wall-clock time, no side effects.
//! - The `running` flag and `terminated_by_max_steps` are simple booleans.
//! - All fields are pure data, not behavior.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `ExecContext` data model | ✅ L1 deterministic | Pure data |
//! | `to_value` / `from_value` | ✅ L1 deterministic | Deterministic conversion |
//! | `with_*` builder methods | ✅ L1 deterministic | Pure data transformation |
//! | `with_field` string dispatch | ✅ L1 deterministic | Deterministic field update |
//! | `params()` / `instruction_type()` | ✅ L1 deterministic | Pure data access |
//! | `before_hash()` / `after_hash()` | ✅ L1 deterministic | Pure data access |
//! | `next_tick` (in audit chain) | ✅ L1 deterministic | `u64` increment |
//! | Logical clock tick | ✅ L1 deterministic | Monotonic counter |
//!
//! # Cross-Language Note (L4)
//!
//! `ExecContext` is a Rust-only construct; there is no cross-language equivalent.
//! However, the serialization format (`to_value`) is JSON-compatible and can be
//! used by other languages to inspect the execution context.

use crate::value::Value;

/// Execution context, corresponding to the __exec__ field.
///
/// Wraps the internal state required by the execution engine, excluding business data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecContext {
    // ── Core fields ──
    /// Current instruction being executed.
    pub instruction: Value,
    /// Instruction queue (list of instructions waiting to be executed).
    pub queue: Value,
    /// Whether the engine is currently running.
    pub running: bool,
    /// Additional metadata.
    pub metadata: Value,

    // ── Configuration fields (injected by governance layer, TCB read-only) ──
    /// Default instruction (executed when the queue is empty).
    pub default_instruction: Value,
    /// Termination domain (stops the loop when satisfied).
    pub termination_domain: Value,
    /// List of meta-instruction types.
    pub meta_instruction_types: Value,
    /// Audit switch.
    pub audit_on: bool,
    /// Drain meta-trace switch.
    pub drain_meta_trace: bool,
    /// Dispatch table (cases table).
    pub dispatch_cases: Value,

    // ── Dynamic fields (written by TCB at runtime) ──
    /// Last dispatch before/after hashes (P0-A addition).
    pub last_dispatch_hashes: Value,
    /// Trace source marker.
    pub trace_source: Value,
    /// Whether the `while_loop` was forcibly truncated due to reaching `max_steps`
    /// (traceability fix).
    pub terminated_by_max_steps: bool,
}

impl ExecContext {
    /// Create a new execution context.
    pub fn new(instruction: Value) -> Self {
        Self {
            instruction,
            queue: Value::empty_list(),
            running: true,
            metadata: Value::empty_object(),
            default_instruction: Value::Null,
            termination_domain: Value::Null,
            meta_instruction_types: Value::empty_list(),
            audit_on: true,
            drain_meta_trace: false,
            dispatch_cases: Value::empty_object(),
            last_dispatch_hashes: Value::Null,
            trace_source: Value::Null,
            terminated_by_max_steps: false,
        }
    }

    /// Create an `ExecContext` from a Value (typically an object).
    ///
    /// This is the typed parsing entry point for __exec__, replacing the scattered
    /// `state.get("__exec__").and_then(|v| v.get("xxx"))` pattern.
    pub fn from_value(val: &Value) -> Self {
        let instruction = val
            .get("instruction")
            .cloned()
            .unwrap_or(Value::empty_object());
        let queue = val.get("queue").cloned().unwrap_or(Value::empty_list());
        let running = val
            .get("__running")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(true);
        let metadata = val
            .get("metadata")
            .cloned()
            .unwrap_or(Value::empty_object());
        let default_instruction = val
            .get("default_instruction")
            .cloned()
            .unwrap_or(Value::Null);
        let termination_domain = val
            .get("termination_domain")
            .cloned()
            .unwrap_or(Value::Null);
        let meta_instruction_types = val
            .get("meta_instruction_types")
            .cloned()
            .unwrap_or(Value::empty_list());
        let audit_on = val
            .get("audit_on")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(true);
        let drain_meta_trace = val
            .get("drain_meta_trace")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(false);
        let dispatch_cases = val
            .get("dispatch_cases")
            .cloned()
            .unwrap_or(Value::empty_object());
        let last_dispatch_hashes = val
            .get("last_dispatch_hashes")
            .cloned()
            .unwrap_or(Value::Null);
        let trace_source = val.get("__trace_source").cloned().unwrap_or(Value::Null);
        let terminated_by_max_steps = val
            .get("__terminated_by_max_steps")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(false);

        Self {
            instruction,
            queue,
            running,
            metadata,
            default_instruction,
            termination_domain,
            meta_instruction_types,
            audit_on,
            drain_meta_trace,
            dispatch_cases,
            last_dispatch_hashes,
            trace_source,
            terminated_by_max_steps,
        }
    }

    /// Convert the `ExecContext` to a Value (object).
    ///
    /// The output format is fully consistent with the Value structure of __exec__,
    /// ensuring that `$ref` paths in cases tables remain unchanged.
    pub fn to_value(&self) -> Value {
        Value::Object(im::HashMap::from(vec![
            ("instruction".to_string(), self.instruction.clone()),
            ("queue".to_string(), self.queue.clone()),
            ("__running".to_string(), Value::Bool(self.running)),
            ("metadata".to_string(), self.metadata.clone()),
            (
                "default_instruction".to_string(),
                self.default_instruction.clone(),
            ),
            (
                "termination_domain".to_string(),
                self.termination_domain.clone(),
            ),
            (
                "meta_instruction_types".to_string(),
                self.meta_instruction_types.clone(),
            ),
            ("audit_on".to_string(), Value::Bool(self.audit_on)),
            (
                "drain_meta_trace".to_string(),
                Value::Bool(self.drain_meta_trace),
            ),
            ("dispatch_cases".to_string(), self.dispatch_cases.clone()),
            (
                "last_dispatch_hashes".to_string(),
                self.last_dispatch_hashes.clone(),
            ),
            ("__trace_source".to_string(), self.trace_source.clone()),
            (
                "__terminated_by_max_steps".to_string(),
                Value::Bool(self.terminated_by_max_steps),
            ),
        ]))
    }

    // ══════════════════════════════════════════
    // Read methods (typed accessors)
    // ══════════════════════════════════════════

    /// Get the parameters of the current instruction.
    pub fn params(&self) -> Value {
        self.instruction
            .get("params")
            .cloned()
            .unwrap_or(Value::empty_object())
    }

    /// Get the type of the current instruction.
    pub fn instruction_type(&self) -> String {
        self.instruction
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Get the `before_hash` from `last_dispatch_hashes`.
    pub fn before_hash(&self) -> Option<String> {
        self.last_dispatch_hashes
            .get("before_hash")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string)
    }

    /// Get the `after_hash` from `last_dispatch_hashes`.
    pub fn after_hash(&self) -> Option<String> {
        self.last_dispatch_hashes
            .get("after_hash")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string)
    }

    // ══════════════════════════════════════════
    // Write methods (builder pattern, return new instance)
    // ══════════════════════════════════════════

    /// Set the current instruction.
    pub fn with_instruction(&self, instruction: Value) -> Self {
        Self {
            instruction,
            ..self.clone()
        }
    }

    /// Enqueue an instruction (append to the end of the queue).
    pub fn with_enqueued(&self, instruction: Value) -> Self {
        let mut q = match &self.queue {
            Value::List(v) => v.clone(),
            _ => im::Vector::new(),
        };
        q.push_back(instruction);
        Self {
            queue: Value::List(q),
            ..self.clone()
        }
    }

    /// Enqueue a sequence of instructions (append all to the end of the queue).
    pub fn with_enqueued_sequence(&self, instructions: &[Value]) -> Self {
        let mut q = match &self.queue {
            Value::List(v) => v.clone(),
            _ => im::Vector::new(),
        };
        for instr in instructions {
            q.push_back(instr.clone());
        }
        Self {
            queue: Value::List(q),
            ..self.clone()
        }
    }

    /// Set the running flag.
    pub fn with_running(&self, running: bool) -> Self {
        Self {
            running,
            ..self.clone()
        }
    }

    /// Stop execution.
    pub fn stop(&self) -> Self {
        self.with_running(false)
    }

    /// Set `last_dispatch_hashes`.
    pub fn with_dispatch_hashes(&self, before: &str, after: &str) -> Self {
        Self {
            last_dispatch_hashes: Value::Object(im::hashmap! {
                "before_hash".to_string() => Value::string(before),
                "after_hash".to_string() => Value::string(after),
            }),
            ..self.clone()
        }
    }

    /// Set __`trace_source`.
    pub fn with_trace_source(&self, source: &str) -> Self {
        Self {
            trace_source: Value::string(source),
            ..self.clone()
        }
    }

    // ══════════════════════════════════════════
    // Type-safe field setters (preferred over with_field)
    // ══════════════════════════════════════════

    /// Set the queue.
    pub fn with_queue(&self, queue: Value) -> Self {
        Self {
            queue,
            ..self.clone()
        }
    }

    /// Set the metadata.
    pub fn with_metadata(&self, metadata: Value) -> Self {
        Self {
            metadata,
            ..self.clone()
        }
    }

    /// Set the default instruction.
    pub fn with_default_instruction(&self, default_instruction: Value) -> Self {
        Self {
            default_instruction,
            ..self.clone()
        }
    }

    /// Set the termination domain.
    pub fn with_termination_domain(&self, termination_domain: Value) -> Self {
        Self {
            termination_domain,
            ..self.clone()
        }
    }

    /// Set the meta-instruction types list.
    pub fn with_meta_instruction_types(&self, meta_instruction_types: Value) -> Self {
        Self {
            meta_instruction_types,
            ..self.clone()
        }
    }

    /// Set the audit switch.
    pub fn with_audit_on(&self, audit_on: bool) -> Self {
        Self {
            audit_on,
            ..self.clone()
        }
    }

    /// Set the drain meta-trace switch.
    pub fn with_drain_meta_trace(&self, drain_meta_trace: bool) -> Self {
        Self {
            drain_meta_trace,
            ..self.clone()
        }
    }

    /// Set the dispatch cases table.
    pub fn with_dispatch_cases(&self, dispatch_cases: Value) -> Self {
        Self {
            dispatch_cases,
            ..self.clone()
        }
    }

    /// Set the `terminated_by_max_steps` flag.
    pub fn with_terminated_by_max_steps(&self, terminated_by_max_steps: bool) -> Self {
        Self {
            terminated_by_max_steps,
            ..self.clone()
        }
    }

    /// Update a single field in __exec__ (generic, string-based).
    ///
    /// # Warning
    ///
    /// This method uses string matching for field names. Typos will cause the
    /// update to be silently ignored. For compile-time safety, prefer the
    /// type-safe setters: `with_queue()`, `with_metadata()`, `with_audit_on()`,
    /// etc.
    ///
    /// Supported field names:
    /// `instruction`, `queue`, `__running`, `metadata`, `default_instruction`,
    /// `termination_domain`, `meta_instruction_types`, `audit_on`,
    /// `drain_meta_trace`, `dispatch_cases`, `last_dispatch_hashes`,
    /// `__trace_source`, `__terminated_by_max_steps`.
    ///
    /// Unknown fields are ignored.
    pub fn with_field(&self, field: &str, value: Value) -> Self {
        match field {
            "instruction" => self.with_instruction(value),
            "queue" => self.with_queue(value),
            "__running" => self.with_running(value.as_bool().unwrap_or(false)),
            "metadata" => self.with_metadata(value),
            "default_instruction" => self.with_default_instruction(value),
            "termination_domain" => self.with_termination_domain(value),
            "meta_instruction_types" => self.with_meta_instruction_types(value),
            "audit_on" => self.with_audit_on(value.as_bool().unwrap_or(true)),
            "drain_meta_trace" => self.with_drain_meta_trace(value.as_bool().unwrap_or(false)),
            "dispatch_cases" => self.with_dispatch_cases(value),
            "last_dispatch_hashes" => Self {
                last_dispatch_hashes: value,
                ..self.clone()
            },
            "__trace_source" => self.with_trace_source(value.as_str().unwrap_or("")),
            "__terminated_by_max_steps" => {
                self.with_terminated_by_max_steps(value.as_bool().unwrap_or(false))
            }
            _ => self.clone(), // Unknown fields are ignored (typo safety)
        }
    }
}

impl Default for ExecContext {
    fn default() -> Self {
        Self::new(Value::empty_object())
    }
}

#[allow(clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_context_creation() {
        let ctx = ExecContext::new(Value::empty_object());
        assert!(ctx.running);
        assert_eq!(ctx.instruction_type(), "unknown");
    }

    #[test]
    fn test_exec_context_to_from_value() {
        let ctx = ExecContext::new(Value::empty_object());
        let val = ctx.to_value();
        let restored = ExecContext::from_value(&val);
        assert_eq!(ctx.running, restored.running);
    }

    #[test]
    fn test_enqueue() {
        let ctx = ExecContext::new(Value::empty_object());
        let instr = Value::from(im::HashMap::from(vec![(
            "type".to_string(),
            Value::string("advance"),
        )]));
        let ctx2 = ctx.with_enqueued(instr.clone());
        match &ctx2.queue {
            Value::List(v) => assert_eq!(v.len(), 1),
            _ => panic!("queue should be a list"),
        }
    }

    #[test]
    fn test_stop() {
        let ctx = ExecContext::new(Value::empty_object());
        assert!(ctx.running);
        assert!(!ctx.stop().running);
    }

    #[test]
    fn test_params() {
        let instr = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("set")),
            (
                "params".to_string(),
                Value::from(im::HashMap::from(vec![(
                    "attr".to_string(),
                    Value::string("x"),
                )])),
            ),
        ]));
        let ctx = ExecContext::new(instr);
        assert_eq!(ctx.params().get("attr"), Some(&Value::string("x")));
    }

    #[test]
    fn test_p21_extended_fields_roundtrip() {
        let ctx = ExecContext::new(Value::empty_object())
            .with_dispatch_hashes("abc123", "def456")
            .with_trace_source("dispatch:observe");
        let val = ctx.to_value();
        let restored = ExecContext::from_value(&val);
        assert_eq!(restored.before_hash(), Some("abc123".to_string()));
        assert_eq!(restored.after_hash(), Some("def456".to_string()));
        assert_eq!(restored.trace_source.as_str(), Some("dispatch:observe"));
    }

    #[test]
    fn test_p21_with_field() {
        let ctx = ExecContext::new(Value::empty_object());
        let ctx2 = ctx.with_field("audit_on", Value::Bool(false));
        assert!(!ctx2.audit_on);

        let ctx3 = ctx2.with_field("__running", Value::Bool(false));
        assert!(!ctx3.running);

        // Unknown fields should be ignored
        let ctx4 = ctx3.with_field("unknown_field", Value::Integer(42));
        assert_eq!(ctx3, ctx4);
    }

    /// Test from_value: uses default values for empty Value
    #[test]
    fn test_from_value_empty() {
        let ctx = ExecContext::from_value(&Value::Null);
        assert!(ctx.running); // default true
        assert_eq!(ctx.instruction_type(), "unknown"); // default empty object
        assert!(ctx.queue.as_list().unwrap().is_empty()); // default empty list
    }

    /// Test from_value: missing fields use default values
    #[test]
    fn test_from_value_partial_missing() {
        // Only provide instruction, other fields missing
        let val = Value::from(im::hashmap! {
            "instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("test"),
            }),
        });
        let ctx = ExecContext::from_value(&val);
        assert_eq!(ctx.instruction_type(), "test");
        assert!(ctx.running); // default true
        assert!(ctx.audit_on); // default true
        assert!(!ctx.drain_meta_trace); // default false
    }

    /// Test with_enqueued_sequence: multiple instructions enqueued
    #[test]
    fn test_enqueue_sequence() {
        let ctx = ExecContext::new(Value::empty_object());
        let instrs = vec![
            Value::from(im::hashmap! { "type".to_string() => Value::string("a") }),
            Value::from(im::hashmap! { "type".to_string() => Value::string("b") }),
            Value::from(im::hashmap! { "type".to_string() => Value::string("c") }),
        ];
        let ctx2 = ctx.with_enqueued_sequence(&instrs);
        match &ctx2.queue {
            Value::List(v) => assert_eq!(v.len(), 3),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test before_hash/after_hash: returns None when missing
    #[test]
    fn test_hash_methods_missing() {
        let ctx = ExecContext::new(Value::empty_object());
        assert!(ctx.before_hash().is_none());
        assert!(ctx.after_hash().is_none());
    }

    /// Test with_field: setting dispatch_cases
    #[test]
    fn test_with_field_dispatch_cases() {
        let ctx = ExecContext::new(Value::empty_object());
        let cases = Value::from(im::hashmap! {
            "a".to_string() => Value::from(im::hashmap! { "type".to_string() => Value::string("noop") }),
        });
        let ctx2 = ctx.with_field("dispatch_cases", cases.clone());
        assert_eq!(ctx2.dispatch_cases, cases);
    }

    /// Test terminated_by_max_steps field
    #[test]
    fn test_terminated_by_max_steps() {
        let ctx = ExecContext::new(Value::empty_object());
        assert!(!ctx.terminated_by_max_steps);

        let ctx2 = ctx.with_field("__terminated_by_max_steps", Value::Bool(true));
        assert!(ctx2.terminated_by_max_steps);
    }

    /// Test type-safe setters
    #[test]
    fn test_type_safe_setters() {
        let ctx = ExecContext::new(Value::empty_object());

        // with_audit_on
        let ctx2 = ctx.with_audit_on(false);
        assert!(!ctx2.audit_on);
        assert!(ctx.audit_on); // original unchanged

        // with_running
        let ctx3 = ctx2.with_running(false);
        assert!(!ctx3.running);

        // with_terminated_by_max_steps
        let ctx4 = ctx3.with_terminated_by_max_steps(true);
        assert!(ctx4.terminated_by_max_steps);

        // with_queue
        let queue = Value::list(vec![Value::string("a"), Value::string("b")]);
        let ctx5 = ctx4.with_queue(queue.clone());
        assert_eq!(ctx5.queue, queue);
    }
}
