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

//! Physical Primitive Layer (Layer 0) — module entry point.
//!
//! # Module Structure
//!
//! This module aggregates all indivisible deterministic computation operations,
//! organized by responsibility:
//!
//! - `state_ops`: State modification primitives
//! - `queue_ops`: Queue operation primitives
//! - `domain_ops`: Domain operation primitives
//! - `rule_ops`: Rule operation primitives
//! - `compute_ops`: Computation primitives (`content_hash`, `format_string`, `get_index`)
//! - `audit_ops`: Audit primitives
//! - `error_ops`: Error handling primitives
//! - `noop_ops`: No-op primitive
//!
//! # Moved Out of TCB
//!
//! The following modules have been moved to the Governance layer:
//! - `io_ops` → Governance layer (I/O channels)
//! - `inference_ops` → Governance layer (conflict/cycle detection, dimension check)
//! - `algebra_ops` → Governance layer
//! - `solver_ops` → Governance layer
//! - `dsl_parser` → Governance layer
//!
//! # Determinism Guarantee
//!
//! This module itself is **L1 deterministic** — it is a pure aggregation
//! of deterministic primitives. The registration and mapping functions have
//! no runtime behavior that could introduce non-determinism.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `register_all()` | ✅ L1 deterministic | Pure registration |
//! | `all_exec_fns()` | ✅ L1 deterministic | Static mapping table |
//! | `exec_fn_map()` | ✅ L1 deterministic | `HashMap` construction |
//! | `all_explainers()` | ✅ L1 deterministic | Static explainer table |
//! | `explainer_map()` | ✅ L1 deterministic | `HashMap` construction |
//! | `validate_instruction_type` | ✅ L1 deterministic | Pure validation |
//! | `make_test_registry()` | ✅ L1 deterministic | Test-only construction |
//! | All submodule primitives | ✅ L1 deterministic | See individual modules |
//!
//! # Cross-Language Note (L4)
//!
//! This module is a Rust-only construct; there is no cross-language equivalent.
//! The primitives it aggregates are defined entirely within the TCB.

pub mod audit_ops;
pub mod compute_ops;
pub mod domain_ops;
pub mod error_ops;
pub mod noop_ops;
pub mod queue_ops;
pub mod rule_ops;
pub mod state_ops;

use crate::error::{invalid_config, EvoRuleError};
use crate::instruction::registry::InstructionRegistry;
use crate::instruction::ExecutorFn;
use crate::state::State;
use crate::value::Value;
use std::collections::HashMap;

/// Register all physical primitives.
pub fn register_all(reg: &mut InstructionRegistry) {
    state_ops::register(reg);
    queue_ops::register(reg);
    domain_ops::register(reg);
    rule_ops::register(reg);
    compute_ops::register(reg);
    audit_ops::register(reg);
    error_ops::register(reg);
    noop_ops::register(reg);
}

/// Static mapping table: primitive name → executor function.
///
/// This is the single source of truth for "how to do it". JSON declares "what to do",
/// and this table provides "how to do it" — each primitive name maps to a Rust function pointer.
///
/// When adding a new primitive, simply add one line to this table.
pub fn all_exec_fns() -> Vec<(&'static str, ExecutorFn)> {
    vec![
        // state_ops
        ("set_context", state_ops::exec_set_context),
        ("state_set", state_ops::exec_state_set),
        ("state_compute", state_ops::exec_state_compute),
        // queue_ops
        ("advance_instruction", queue_ops::exec_advance_instruction),
        ("push_instruction", queue_ops::exec_push_instruction),
        (
            "push_instruction_sequence",
            queue_ops::exec_push_instruction_sequence,
        ),
        ("instruction_sequence", queue_ops::exec_instruction_sequence),
        // domain_ops
        ("evaluate_domain", domain_ops::exec_evaluate_domain),
        ("match_domain", domain_ops::exec_match_domain),
        // rule_ops
        ("apply_rule", rule_ops::exec_apply_rule),
        ("observe_rules", rule_ops::exec_observe_rules),
        ("filter_rules", rule_ops::exec_filter_rules),
        ("inject_rule", rule_ops::exec_inject_rule),
        // compute_ops
        ("content_hash", compute_ops::exec_content_hash),
        ("format_string", compute_ops::exec_format_string),
        ("get_index", compute_ops::exec_get_index),
        // audit_ops
        ("trace_step", audit_ops::exec_trace_step),
        // error_ops
        ("execute_try_catch", error_ops::exec_execute_try_catch),
        ("execute_parallel", error_ops::exec_execute_parallel),
        // noop_ops
        ("noop", noop_ops::exec_noop),
    ]
}

/// Build a `HashMap` from the executor function mapping table.
pub fn exec_fn_map() -> HashMap<&'static str, ExecutorFn> {
    all_exec_fns().into_iter().collect()
}

// ══════════════════════════════════════════════
// Shared validation logic (eliminating duplicate code)
// ══════════════════════════════════════════════

/// Validate that an instruction type is dispatchable (present in the registry or cases table).
///
/// This validation only applies when `__exec__.dispatch_cases` exists (production environment).
/// In test environments without `dispatch_cases`, validation is skipped.
///
/// - `context`: Call-site identifier (e.g., "`evaluate_domain` `on_true`", "`push_instruction`")
/// - `instr_val`: The instruction value to validate (from which the `type` field is extracted)
pub fn validate_instruction_type(
    reg: &InstructionRegistry,
    state: &State,
    instr_val: &Value,
    context: &str,
) -> Result<(), EvoRuleError> {
    let dispatch_cases = state
        .get("__exec__")
        .and_then(|v| v.get("dispatch_cases"))
        .and_then(|v| v.as_object());
    if let Some(cases_map) = dispatch_cases {
        if let Some(instr_type) = instr_val.get("type").and_then(|v| v.as_str()) {
            let in_registry = reg.has(instr_type);
            let in_cases = cases_map.contains_key(instr_type);
            if !in_registry && !in_cases {
                return Err(invalid_config(format!(
                    "{} references unregistered instruction type '{}' [at {}:{}]\n  instruction: {}",
                    context,
                    instr_type,
                    file!(),
                    line!(),
                    serde_json::to_string(instr_val).unwrap_or_else(|_| format!("{instr_val:?}"))
                )));
            }
        }
    }
    Ok(())
}

// ══════════════════════════════════════════════
// Test helpers
// ══════════════════════════════════════════════

/// Create a registry with all primitives and context operations registered.
/// Used for unit tests.
#[cfg(test)]
pub fn make_test_registry() -> InstructionRegistry {
    let mut reg = InstructionRegistry::new().with_default_context_ops();
    register_all(&mut reg);
    reg
}

/// Static mapping table: primitive name → explainer function.
///
/// Each primitive's self-explaining function, fulfilling the C4 constitutional promise.
/// The explainer receives instruction parameters and returns a human/AI-readable
/// description of the primitive's behavior.
pub type ExplainerFn = fn(&crate::rule::GenericInstruction) -> String;

pub fn all_explainers() -> Vec<(&'static str, ExplainerFn)> {
    vec![
        // state_ops
        ("set_context", |instr| {
            let op = instr
                .params
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("set");
            let key = instr
                .params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Modify context: execute '{op}' operation on '{key}'")
        }),
        ("state_set", |instr| {
            let key = instr
                .params
                .get("attr")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Assignment: set value of '{key}'")
        }),
        ("state_compute", |instr| {
            let op = instr
                .params
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("compute");
            let key = instr
                .params
                .get("attr")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Compute assignment: execute '{op}' operation on '{key}'")
        }),
        // queue_ops
        ("advance_instruction", |_| {
            "Advance instruction pointer to the next instruction".to_string()
        }),
        ("push_instruction", |instr| {
            let t = instr
                .params
                .get("instruction_type")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Push an instruction of type '{t}' to the queue")
        }),
        ("push_instruction_sequence", |instr| {
            let n = instr
                .params
                .get("instructions")
                .and_then(|v| v.as_list())
                .map_or(0, im::Vector::len);
            format!("Push a sequence of {n} instructions to the queue")
        }),
        ("instruction_sequence", |instr| {
            let n = instr
                .params
                .get("instructions")
                .and_then(|v| v.as_list())
                .map_or(0, im::Vector::len);
            format!("Execute a sequence of {n} instructions")
        }),
        // domain_ops
        ("evaluate_domain", |instr| {
            let domain = instr
                .params
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Evaluate domain '{domain}'")
        }),
        ("match_domain", |instr| {
            let domain = instr
                .params
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Match rules against domain '{domain}'")
        }),
        // rule_ops
        ("apply_rule", |instr| {
            let rule = instr
                .params
                .get("rule_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Apply rule '{rule}'")
        }),
        ("observe_rules", |_| {
            "Observe all currently applicable rules".to_string()
        }),
        ("filter_rules", |instr| {
            let domain = instr
                .params
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Filter rules by domain '{domain}'")
        }),
        ("inject_rule", |instr| {
            let remove = instr
                .params
                .get("remove_rule_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let add = instr
                .params
                .get("add_rule")
                .and_then(|v| v.get("rule_id").and_then(|r| r.as_str()))
                .unwrap_or("");
            match (remove.is_empty(), add.is_empty()) {
                (true, true) => "View rule set status".to_string(),
                (false, true) => format!("Remove rule '{remove}' from rule set"),
                (true, false) => format!("Inject rule '{add}' into rule set"),
                (false, false) => format!("Replace rule: remove '{remove}' → inject '{add}'"),
            }
        }),
        // compute_ops
        ("content_hash", |instr| {
            let key = instr
                .params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("state");
            format!("Compute SHA-256 content hash for '{key}'")
        }),
        ("format_string", |instr| {
            let fmt = instr
                .params
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Format string: '{fmt}'")
        }),
        ("get_index", |instr| {
            let key = instr
                .params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Get index value from '{key}'")
        }),
        // audit_ops
        ("trace_step", |instr| {
            let label = instr
                .params
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("step");
            format!("Audit trace step: '{label}'")
        }),
        // error_ops
        ("execute_try_catch", |_| {
            "Execute try-catch error handling".to_string()
        }),
        ("execute_parallel", |instr| {
            let n = instr
                .params
                .get("branches")
                .and_then(|v| v.as_list())
                .map_or(0, im::Vector::len);
            format!("Execute {n} branches in parallel")
        }),
        // noop_ops
        ("noop", |_| "No operation, does nothing".to_string()),
    ]
}

/// Build a `HashMap` from the explainer mapping table.
pub fn explainer_map() -> HashMap<&'static str, fn(&crate::rule::GenericInstruction) -> String> {
    all_explainers().into_iter().collect()
}
