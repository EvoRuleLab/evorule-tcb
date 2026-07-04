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

//! State primitives — Direct modification of top-level State attributes.
//!
//! # Core Functions
//!
//! - `set_context`: Compatibility alias, dispatches to `state_set` or `state_compute`.
//! - `state_set`: Pure assignment (`set` operation).
//! - `state_compute`: Compute + assignment (add/sub/mul/div/append/remove/length).
//!
//! # Design Principles
//!
//! P3-2: Split `set_context` into `state_set` + `state_compute`:
//! - `state_set`: Pure assignment (corresponds to `set_context`'s `"set"` operation)
//! - `state_compute`: Compute + assignment (corresponds to `set_context`'s
//!   add/sub/mul/div/append/remove operations)
//! - `set_context`: Retained as a compatibility alias, internally dispatches
//!   to `state_set` or `state_compute`
//!
//! # Determinism Guarantee
//!
//! All state primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - Operations are pure functions (add/sub/mul/div/append/remove).
//! - `$ref` resolution is deterministic (pure path traversal).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `state_set` assignment | ✅ L1 deterministic | Pure assignment |
//! | `state_compute` arithmetic | ✅ L1 deterministic | Saturating arithmetic |
//! | `state_compute` list ops | ✅ L1 deterministic | Append/remove/length |
//! | `set_context` dispatch | ✅ L1 deterministic | Deterministic branch |
//! | Operation validation | ✅ L1 deterministic | Static allowlist |
//! | Silent fallthrough (non-String attr) | ✅ L1 deterministic | Returns original state |
//!
//! # Silent Fallthrough Contract
//!
//! When `attr` resolves to a non-String value (e.g., `Value::Null`, `Value::Integer`),
//! all state primitives **silently return the original state** without modification.
//!
//! This is intentional design: failing gracefully when `$ref` resolution fails
//! prevents panic and preserves TCB liveness (ER-600: no runtime panics).
//!
//! # Cross-Language Note (L4)
//!
//! These primitives are Rust-only constructs; there is no cross-language equivalent.
//! The operation semantics (add/sub/mul/div/append/remove/length) are defined by the
//! `ContextOpFn` type and are deterministic.

use crate::control::dispatch::resolve_path;
use crate::error::{invalid_config, EvoRuleError};
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// List of valid context operations (for `set_context` compatibility).
const VALID_OPERATIONS: &[&str] = &["set", "add", "sub", "mul", "div", "append", "remove"];

/// List of valid compute operations (specific to `state_compute`).
/// Note: "set" is NOT included here — use `state_set` for pure assignment.
const COMPUTE_OPERATIONS: &[&str] = &["add", "sub", "mul", "div", "append", "remove", "length"];

/// Register state primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("set_context", exec_set_context);
    reg.register("state_set", exec_state_set);
    reg.register("state_compute", exec_state_compute);
}

/// Pure assignment — writes `value` to `attr`.
///
/// # Parameters
/// - `attr`: The attribute name (resolved via `$ref`).
/// - `value`: The value to assign (resolved via `$ref`).
///
/// # Silent Fallthrough
/// - If `attr` resolves to a non-String value, returns the original state unchanged.
/// - Missing `value` defaults to `Value::Null`.
///
/// # Example
/// ```json
/// { "type": "state_set", "params": { "attr": "x", "value": 42 } }
/// ```
pub(crate) fn exec_state_set(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let attr = instruction
        .params
        .get("attr")
        .cloned()
        .unwrap_or(Value::Null);
    let val = instruction
        .params
        .get("value")
        .cloned()
        .unwrap_or(Value::Null);

    let resolved_attr = resolve_path(state, &attr);
    let resolved_val = resolve_path(state, &val);

    let attr_str = match &resolved_attr {
        Value::String(s) => s.clone(),
        _ => return Ok(state.clone()),
    };

    Ok(state.set_path(&attr_str, resolved_val))
}

/// Compute + assignment — executes a computation on `attr` and writes the result.
///
/// # Parameters
/// - `attr`: The attribute name (resolved via `$ref`).
/// - `operation`: The operation to perform — `add`, `sub`, `mul`, `div`, `append`, `remove`, `length`.
/// - `value`: The operand (resolved via `$ref`). For `length`, this is the list to measure.
///
/// # Operations
/// - `add` / `sub` / `mul` / `div`: Arithmetic operations (saturating).
/// - `append`: Appends to a list.
/// - `remove`: Removes from a list (first occurrence).
/// - `length`: Returns the length of the list in `value` (ignores the current value at `attr`).
///
/// # Silent Fallthrough
/// - If `attr` resolves to a non-String value, returns the original state unchanged.
/// - If `operation` is non-String, defaults to `"add"`.
/// - Missing `value` defaults to `Value::Null`.
///
/// # Errors
/// - Returns `Err` if `operation` is not in `COMPUTE_OPERATIONS`.
///
/// # Example
/// ```json
/// { "type": "state_compute", "params": { "attr": "x", "operation": "add", "value": 5 } }
/// ```
pub(crate) fn exec_state_compute(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let attr = instruction
        .params
        .get("attr")
        .cloned()
        .unwrap_or(Value::Null);
    let op = instruction
        .params
        .get("operation")
        .cloned()
        .unwrap_or(Value::string("add"));
    let val = instruction
        .params
        .get("value")
        .cloned()
        .unwrap_or(Value::Null);

    // Validate operation legality
    if let Value::String(op_str) = &op {
        if !COMPUTE_OPERATIONS.contains(&op_str.as_str()) {
            return Err(invalid_config(format!(
                "state_compute has invalid operation '{op_str}', expected one of {COMPUTE_OPERATIONS:?}"
            )));
        }
    }

    let resolved_attr = resolve_path(state, &attr);
    let resolved_val = resolve_path(state, &val);

    let attr_str = match &resolved_attr {
        Value::String(s) => s.clone(),
        _ => return Ok(state.clone()),
    };
    let op_str = match &op {
        Value::String(s) => s.clone(),
        _ => "add".to_string(),
    };

    let old_val = state.get_path(&attr_str).unwrap_or(Value::Null);
    let new_val = reg
        .get_context_operation(op_str.as_str())
        .map_or(resolved_val.clone(), |op_fn| op_fn(&old_val, &resolved_val));

    Ok(state.set_path(&attr_str, new_val))
}

/// Set context — directly modifies top-level State attributes.
///
/// # Parameters
/// - `transform`: An object with `attr`, `operation`, and `value` fields.
///
/// This is a **compatibility alias** that internally dispatches to:
/// - `state_set` when `operation` is `"set"`
/// - `state_compute` when `operation` is one of `add/sub/mul/div/append/remove`
///
/// # Startup Validation
/// - The `operation` must be one of `VALID_OPERATIONS`.
/// - If `operation` is invalid, returns `Err`.
///
/// # Silent Fallthrough
/// - If `transform` is missing or not an Object, returns the original state unchanged.
/// - If `attr` resolves to a non-String value, returns the original state unchanged.
///
/// # Example
/// ```json
/// {
///   "type": "set_context",
///   "params": {
///     "transform": { "attr": "x", "operation": "add", "value": 5 }
///   }
/// }
/// ```
pub(crate) fn exec_set_context(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let spec_raw = instruction
        .params
        .get("transform")
        .cloned()
        .unwrap_or(Value::Object(im::HashMap::new()));

    let spec_map = match &spec_raw {
        Value::Object(m) => m,
        _ => return Ok(state.clone()),
    };

    let attr = spec_map.get("attr").cloned().unwrap_or(Value::Null);
    let op = spec_map
        .get("operation")
        .cloned()
        .unwrap_or(Value::string("set"));
    let val_val = spec_map.get("value").cloned().unwrap_or(Value::Null);

    // Validate operation legality (only for literals; $ref references are skipped)
    if let Value::String(op_str) = &op {
        if !VALID_OPERATIONS.contains(&op_str.as_str()) {
            return Err(invalid_config(format!(
                "set_context transform has invalid operation '{}', expected one of {:?} \
                 [at {}:{}]\n  transform: {}",
                op_str,
                VALID_OPERATIONS,
                file!(),
                line!(),
                serde_json::to_string(&spec_raw).unwrap_or_else(|_| format!("{spec_raw:?}"))
            )));
        }
    }

    let resolved_attr = resolve_path(state, &attr);
    let resolved_val = resolve_path(state, &val_val);

    let attr_str = match &resolved_attr {
        Value::String(s) => s.clone(),
        _ => return Ok(state.clone()),
    };
    let op_str = match &op {
        Value::String(s) => s.clone(),
        _ => "set".to_string(),
    };

    let old_val = state.get_path(&attr_str).unwrap_or(Value::Null);
    let new_val = match op_str.as_str() {
        "add" | "sub" | "mul" | "div" | "append" | "remove" => reg
            .get_context_operation(op_str.as_str())
            .map_or(resolved_val.clone(), |op_fn| op_fn(&old_val, &resolved_val)),
        _ => resolved_val,
    };

    Ok(state.set_path(&attr_str, new_val))
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[allow(clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_test_state() -> State {
        State::empty().set("x", Value::Integer(10))
    }

    #[test]
    fn test_set_context_set() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("set")),
                ("value".to_string(), Value::Integer(42)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(42)));
    }

    #[test]
    fn test_set_context_add() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("add")),
                ("value".to_string(), Value::Integer(5)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(15)));
    }

    // The following 4 tests cover set_context's support for mul/div/append/remove.
    // These ops are already registered via with_default_context_ops() in the registry.
    // These tests verify that set_context's startup validation passes these ops
    // and correctly dispatches them to the corresponding ContextOpFn.

    #[test]
    fn test_set_context_mul() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("mul")),
                ("value".to_string(), Value::Integer(3)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(30)),
            "mul: 10 * 3 should equal 30"
        );
    }

    #[test]
    fn test_set_context_div() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("div")),
                ("value".to_string(), Value::Integer(2)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(5)),
            "div: 10 / 2 should equal 5"
        );

        // div by zero should return the original value (consistent with README)
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("div")),
                ("value".to_string(), Value::Integer(0)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "div: division by zero should return the original value"
        );
    }

    #[test]
    fn test_set_context_append() {
        let state = State::empty().set("xs", Value::empty_list());
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("xs")),
                ("operation".to_string(), Value::string("append")),
                ("value".to_string(), Value::Integer(42)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        match result.get("xs") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 1, "list should have 1 element after append");
                assert_eq!(items.front(), Some(&Value::Integer(42)));
            }
            other => panic!("Expected List, got: {:?}", other),
        }
    }

    #[test]
    fn test_set_context_remove() {
        let state = State::empty().set(
            "xs",
            Value::list(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("xs")),
                ("operation".to_string(), Value::string("remove")),
                ("value".to_string(), Value::Integer(2)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        match result.get("xs") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 2, "list should have 2 elements after remove");
                // Verify that 2 has been removed
                let has_two = items.iter().any(|v| matches!(v, Value::Integer(2)));
                assert!(
                    !has_two,
                    "remove should have removed the element with value 2"
                );
            }
            other => panic!("Expected List, got: {:?}", other),
        }
    }

    // ══════════════════════════════════════════════
    // P3-2: state_set / state_compute tests
    // ══════════════════════════════════════════════

    #[test]
    fn test_state_set_basic() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("value".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("state_set", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_set(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(42)));
    }

    #[test]
    fn test_state_set_new_attr() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("y"));
        params.insert("value".to_string(), Value::string("hello"));
        let instr = GenericInstruction::new("state_set", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_set(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("y"), Some(&Value::string("hello")));
        // Original attribute unchanged
        assert_eq!(result.get("x"), Some(&Value::Integer(10)));
    }

    #[test]
    fn test_state_compute_add() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("add"));
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(15)));
    }

    #[test]
    fn test_state_compute_sub() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("sub"));
        params.insert("value".to_string(), Value::Integer(3));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(7)));
    }

    #[test]
    fn test_state_compute_mul() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("mul"));
        params.insert("value".to_string(), Value::Integer(3));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(30)));
    }

    #[test]
    fn test_state_compute_invalid_operation() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("invalid_op"));
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr);
        assert!(
            result.is_err(),
            "state_compute should reject illegal operation"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // P3-2 Full fix (2026-06-26): Full op coverage + silent fallthrough contract
    //
    // The following tests solidify three types of contracts, eliminating technical debt:
    //   A. state_compute full coverage of all 6 legal operations (this block)
    //   B. Silent fallthrough contract: attr non-String returns original state silently
    //      (This is intentional design: fail gracefully when $ref resolution fails,
    //       per module decision 3)
    //   C. set_context startup validation consistent with state_compute
    // ════════════════════════════════════════════════════════════════════

    // ── A. state_compute full op coverage ──────────────────────────────────

    #[test]
    fn test_state_compute_div() {
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("div"));
        params.insert("value".to_string(), Value::Integer(2));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(5)),
            "div: 10 / 2 should equal 5"
        );

        // div by zero should return the original value (consistent with README and set_context_div)
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("div"));
        params.insert("value".to_string(), Value::Integer(0));
        let instr = GenericInstruction::new("state_compute", params);
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "div: division by zero should return the original value"
        );
    }

    #[test]
    fn test_state_compute_append() {
        let state = State::empty().set("xs", Value::empty_list());
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("xs"));
        params.insert("operation".to_string(), Value::string("append"));
        params.insert("value".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        match result.get("xs") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 1, "list should have 1 element after append");
                assert_eq!(items.front(), Some(&Value::Integer(42)));
            }
            other => panic!("Expected List, got: {:?}", other),
        }
    }

    #[test]
    fn test_state_compute_remove() {
        let state = State::empty().set(
            "xs",
            Value::list(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("xs"));
        params.insert("operation".to_string(), Value::string("remove"));
        params.insert("value".to_string(), Value::Integer(2));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        match result.get("xs") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 2, "list should have 2 elements after remove");
                let has_two = items.iter().any(|v| matches!(v, Value::Integer(2)));
                assert!(
                    !has_two,
                    "remove should have removed the element with value 2"
                );
            }
            other => panic!("Expected List, got: {:?}", other),
        }
    }

    #[test]
    fn test_state_compute_length() {
        // length operation: returns the length of the list in `value`, writes to `attr`.
        // The current value at `attr` (Null here) is ignored.
        let state = State::empty(); // attr "n" does not exist yet
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("n"));
        params.insert("operation".to_string(), Value::string("length"));
        params.insert(
            "value".to_string(),
            Value::list(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("n"),
            Some(&Value::Integer(3)),
            "length of a 3-element list should be 3"
        );
    }

    #[test]
    fn test_state_compute_length_empty_list() {
        let state = State::empty();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("n"));
        params.insert("operation".to_string(), Value::string("length"));
        params.insert("value".to_string(), Value::empty_list());
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("n"),
            Some(&Value::Integer(0)),
            "length of an empty list should be 0"
        );
    }

    #[test]
    fn test_state_compute_length_non_list_yields_zero() {
        // Non-list `value` should yield 0 (graceful fallback, no panic).
        let state = State::empty();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("n"));
        params.insert("operation".to_string(), Value::string("length"));
        params.insert("value".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("n"),
            Some(&Value::Integer(0)),
            "length of a non-list value should be 0"
        );
    }

    // ── B. Silent fallthrough contract (attr non-String → silently return original state) ──

    #[test]
    fn test_state_set_null_attr_returns_state_unchanged() {
        // attr = Value::Null should silently return original state, no modification
        let state = make_test_state(); // x = 10
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::Null);
        params.insert("value".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("state_set", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_set(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "state_set with attr=Null should silently retain original x value"
        );
    }

    #[test]
    fn test_state_compute_null_attr_returns_state_unchanged() {
        // attr = Value::Null should silently return original state, even if operation is valid
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::Null);
        params.insert("operation".to_string(), Value::string("add"));
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "state_compute with attr=Null should silently retain original x value, not apply add"
        );
    }

    #[test]
    fn test_set_context_null_attr_returns_state_unchanged() {
        // attr = Value::Null (inside transform) should silently return original state
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::Null),
                ("operation".to_string(), Value::string("add")),
                ("value".to_string(), Value::Integer(5)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "set_context with transform.attr=Null should silently retain original x value"
        );
    }

    // ── C. set_context startup validation ──────────────────────────────────────

    #[test]
    fn test_set_context_invalid_operation() {
        // Aligns with test_state_compute_invalid_operation:
        // set_context should also validate operation legality at startup
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("attr".to_string(), Value::string("x")),
                ("operation".to_string(), Value::string("invalid_op")),
                ("value".to_string(), Value::Integer(5)),
            ])),
        );
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr);
        assert!(
            result.is_err(),
            "set_context should reject illegal operation"
        );
    }

    // ─── P1: Error path and boundary tests ─────────────────────────────────
    // Verify graceful handling of type mismatches and edge cases.
    // Aligns with ER-600 (no runtime panics).

    #[test]
    fn test_state_compute_non_string_operation_defaults_to_add() {
        // operation is a non-String value (Integer) → defaults to "add", no panic
        let state = make_test_state(); // x = 10
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::Integer(999)); // invalid type
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        // Non-String op falls through to "add" default
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(15)),
            "non-String operation should default to 'add' (10 + 5 = 15)"
        );
    }

    #[test]
    fn test_state_compute_null_operation_defaults_to_add() {
        // operation is Value::Null → defaults to "add", no panic
        let state = make_test_state(); // x = 10
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::Null);
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(15)),
            "Null operation should default to 'add' (10 + 5 = 15)"
        );
    }

    #[test]
    fn test_state_compute_non_string_attr_returns_state_unchanged() {
        // attr is a non-String value (Integer) → returns state unchanged, no panic
        let state = make_test_state(); // x = 10
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::Integer(42)); // invalid type
        params.insert("operation".to_string(), Value::string("add"));
        params.insert("value".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "non-String attr should return state unchanged"
        );
    }

    #[test]
    fn test_state_compute_missing_value_uses_null() {
        // Missing "value" param → uses Value::Null, no panic
        // Note: "add" is used because state_compute only allows
        // add/sub/mul/div/append/remove (not "set")
        let state = make_test_state(); // x = 10
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("operation".to_string(), Value::string("add"));
        let instr = GenericInstruction::new("state_compute", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_state_compute(&reg, &state, &instr).unwrap();
        // add with Null value → depends on add op behavior with Null
        // (verify no panic; exact value depends on op_fn implementation)
        assert!(
            result.get("x").is_some(),
            "missing value should not panic; state should still contain x"
        );
    }

    #[test]
    fn test_set_context_missing_transform_returns_state_unchanged() {
        // Missing "transform" param → spec_raw is empty Object → returns state unchanged
        let state = make_test_state();
        let instr = GenericInstruction::new("set_context", HashMap::new());

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "missing transform should return state unchanged"
        );
    }

    #[test]
    fn test_set_context_non_object_transform_returns_state_unchanged() {
        // transform is a non-Object value (Integer) → returns state unchanged
        let state = make_test_state();
        let mut params = HashMap::new();
        params.insert("transform".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("set_context", params);

        let reg = InstructionRegistry::new().with_default_context_ops();
        let result = exec_set_context(&reg, &state, &instr).unwrap();
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(10)),
            "non-Object transform should return state unchanged"
        );
    }
}
