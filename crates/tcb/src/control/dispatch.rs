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

//! Control flow primitive — Instruction dispatch and path reference resolution.
//!
//! # Core Functions
//!
//! - `dispatch`: O(1) instruction dispatch via cases table.
//! - `resolve_path` / `resolve_refs`: `$ref` path reference resolution.
//! - `resolve_pass`: `$pass` explicit passthrough declaration.
//!
//! # Design Principles
//!
//! All dispatch must go through the cases table — this is the core commitment
//! to data-driven execution. Constitutional principles C1 (Transparency), C2 (Traceability),
//! and C3 (Auditability) require that every instruction's execution path be
//! recorded in the cases table, with no implicit paths.
//!
//! `$ref` references in case entries are recursively resolved before execution,
//! ensuring that physical primitives receive fully resolved parameter values.
//! `$pass` is syntactic sugar for `$ref`, specifically for passing through
//! the current instruction's parameters.
//!
//! # Determinism Guarantee
//!
//! The `dispatch` primitive itself is **L1 deterministic**:
//! - Given the same input state and instruction, it produces the same output state.
//! - It uses no randomness, wall-clock time, or side effects.
//! - Cases table lookup is deterministic (hash map lookup with string keys).
//! - `$ref` resolution is deterministic (pure path traversal).
//! - `$pass` resolution is deterministic (pure field access).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | Cases table matching | ✅ L1 deterministic | `to_dispatch_key` conversion |
//! | `$ref` resolution | ✅ L1 deterministic | Pure path traversal |
//! | `$pass` resolution | ✅ L1 deterministic | Pure field access |
//! | `resolve_refs` recursion | ✅ L1 deterministic | Depth-limited (64) |
//! | Dispatch hashes (`last_dispatch_hashes`) | ✅ L1 deterministic | SHA-256 content hash |
//! | `compute_dispatch_state_hash` | ✅ L1 deterministic | Delegates to `business_state_snapshot` |
//! | `set_path_value` / `build_nested_state` | ✅ L1 deterministic | Pure path building |
//!
//! # Compositional Determinism
//!
//! While the `dispatch` primitive is L1 deterministic, the **overall determinism**
//! of a dispatch execution depends on the determinism of the instruction being
//! dispatched (the `case` body or `default` branch). If the dispatched instruction
//! is non-deterministic, the composition becomes non-deterministic.
//!
//! The TCB does not enforce instruction determinism white‑listing; callers are
//! responsible for ensuring that all instructions dispatched via the cases table
//! are deterministic. This is a **conditional deterministic** guarantee.
//!
//! # Cross-Language Note (L4)
//!
//! `dispatch` is a Rust-only construct; there is no cross-language equivalent.
//! However, the cases table format (`core_eval.json`) is JSON-compatible and can be
//! inspected by other languages.

use crate::audit::compute_state_hash;
use crate::error::EvoRuleError;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// O(1) instruction dispatch.
///
/// All dispatch must go through the cases table — this is the core commitment
/// to data-driven execution.
/// Dispatch instructions are dispatched by key/cases.
/// Constitutional principles C1 (Transparency), C2 (Traceability), and
/// C3 (Auditability) require that every instruction's execution path be
/// recorded in the cases table, with no implicit paths.
///
/// Note: This function is registered as the `exec_fn` for the "dispatch" type,
/// so `instruction.instruction_type` is necessarily "dispatch".
pub fn exec_dispatch(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    dispatch_by_cases(reg, state, instruction)
}

/// Dispatch by cases table (with recursive $ref resolution).
///
/// The cases table is the sole dispatch source. $ref references in case entries
/// are recursively resolved before execution, ensuring that physical primitives
/// receive fully resolved parameter values.
fn dispatch_by_cases(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    if let Some(key_val) = instruction.params.get("key") {
        let resolved_key = resolve_path(state, key_val);
        let key_str = resolved_key.to_dispatch_key();

        let cases = instruction
            .params
            .get("cases")
            .cloned()
            .unwrap_or(Value::empty_object());

        if let Value::Object(case_map) = &cases {
            // Exact match
            if let Some(case_val) = case_map.get(&key_str) {
                // ── P0-A: Record state hash before execution ──
                let state_before_hash = compute_dispatch_state_hash(state);

                // Recursively resolve $ref references — {"$ref": "__exec__.instruction.params.attr"}
                // in cases must be resolved to actual values before constructing GenericInstruction
                let resolved = resolve_refs(state, case_val);
                let case_instr = GenericInstruction::from_value(&resolved)?;
                let result = reg.execute(state, &case_instr)?;

                // ── P0-A: Record state hash after execution, write to __exec__.last_dispatch_hashes ──
                let state_after_hash = compute_dispatch_state_hash(&result);
                let result = record_dispatch_hashes(
                    &result,
                    &state_before_hash,
                    &state_after_hash,
                    &key_str,
                );

                // Aligns with v2: push advance_instruction to the queue after case body execution.
                // The while_loop drain logic will pop and execute it in the next iteration.
                // [FIX #4] Only push advance_instruction if __running is still true.
                if is_running(&result) {
                    let mut queue = get_queue(&result);
                    queue.push_back(Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("advance_instruction"),
                        "params".to_string() => Value::empty_object(),
                    }));
                    return Ok(result.update_exec_field("queue", Value::List(queue)));
                }
                return Ok(result);
            }

            // Default branch
            if let Some(default_val) = instruction.params.get("default") {
                // ── P0-A: Record state hash before execution ──
                let state_before_hash = compute_dispatch_state_hash(state);

                let resolved = resolve_refs(state, default_val);
                let default_instr = GenericInstruction::from_value(&resolved)?;
                let result = reg.execute(state, &default_instr)?;

                // ── P0-A: Record state hash after execution ──
                let state_after_hash = compute_dispatch_state_hash(&result);
                let result = record_dispatch_hashes(
                    &result,
                    &state_before_hash,
                    &state_after_hash,
                    &format!("default:{key_str}"),
                );

                // [FIX #4] Only push advance_instruction if __running is still true.
                if is_running(&result) {
                    let mut queue = get_queue(&result);
                    queue.push_back(Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("advance_instruction"),
                        "params".to_string() => Value::empty_object(),
                    }));
                    return Ok(result.update_exec_field("queue", Value::List(queue)));
                }
                return Ok(result);
            }
        }
    }

    Ok(state.clone())
}

/// Get the current execution queue.
fn get_queue(state: &State) -> im::Vector<Value> {
    state
        .get("__exec__")
        .and_then(|v| v.get("queue"))
        .cloned()
        .and_then(|v| match v {
            Value::List(vec) => Some(vec),
            _ => None,
        })
        .unwrap_or_default()
}

/// Check the __running flag.
fn is_running(state: &State) -> bool {
    state
        .get("__exec__")
        .and_then(|v| v.get("__running"))
        .and_then(super::super::value::Value::as_bool)
        .unwrap_or(true)
}

/// Compute the dispatch state hash (excluding system fields for determinism).
///
/// Delegates to [`State::business_state_snapshot`], the single source of truth
/// for which fields are excluded from business-state hashing. This ensures
/// the before/after hashes recorded in `__exec__.last_dispatch_hashes` are
/// computed with the **same** exclusion list as `trace_step`'s audit records
/// (C3 auditability: hash consistency across the codebase).
fn compute_dispatch_state_hash(state: &State) -> String {
    compute_state_hash(&state.business_state_snapshot())
}

/// Write the before/after dispatch hashes to `__exec__.last_dispatch_hashes`.
///
/// `trace_step` reads from this field to obtain before/after hashes, enabling
/// auditability: audit records can distinguish "what dispatch changed".
///
/// Field structure:
/// ```json
/// {
///   "before_hash": "sha256...",
///   "after_hash": "sha256...",
///   "case_key": "increment"
/// }
/// ```
fn record_dispatch_hashes(
    state: &State,
    before_hash: &str,
    after_hash: &str,
    case_key: &str,
) -> State {
    let hashes = Value::Object(im::hashmap! {
        "before_hash".to_string() => Value::string(before_hash),
        "after_hash".to_string() => Value::string(after_hash),
        "case_key".to_string() => Value::string(case_key),
    });
    state.update_exec_field("last_dispatch_hashes", hashes)
}

/// Maximum recursion depth for `resolve_refs`.
///
/// This is a safety valve against stack overflow caused by deeply nested input
/// values or circular `$ref` chains (e.g. `state.x = {"$ref": "y"}` and
/// `state.y = {"$ref": "x"}`). Without this limit, a pathological input could
/// trigger a stack-overflow panic, violating ER-602 (no panic in production).
///
/// 64 is chosen as a generous upper bound: legitimate cases tables rarely
/// nest more than a few levels deep, and a chain of 64 `$ref` hops is almost
/// certainly a configuration error rather than an intentional design.
const MAX_REF_DEPTH: usize = 64;

/// Recursively resolve $ref references in a Value.
///
/// Performs a deep traversal of nested Objects and Lists, replacing each
/// {"$ref": "path"} with the value from the state at that path.
/// This is the key to data-driven dispatch via the cases table:
/// $ref/$pass in cases are resolved from the state at execution time,
/// enabling parameter injection.
///
/// # Depth limit & chain resolution
///
/// - `$ref` is resolved **chain-style**: if the value at the referenced path
///   is itself a `{"$ref": "..."}`, resolution continues recursively until a
///   non-`$ref` value is reached or the `MAX_REF_DEPTH` limit is exceeded.
/// - If the depth limit is exceeded (indicating a circular reference or
///   pathological nesting), resolution stops and returns [`Value::Null`],
///   preventing a stack-overflow panic. This is the safe fallback per ER-602.
///
/// Examples:
///   {"$ref": "__exec__.instruction.params.attr"} → the value at that path in state
///   {"$pass": ""} → passthrough the entire __exec__.instruction.params
///   {"$pass": "attr"} → passthrough __exec__.instruction.params.attr
///   {"type": "`set_context`", "params": {"transform": {"$ref": "..."}}}
///     → recursively resolves $ref inside transform
pub fn resolve_refs(state: &State, value: &Value) -> Value {
    resolve_refs_inner(state, value, 0)
}

fn resolve_refs_inner(state: &State, value: &Value, depth: usize) -> Value {
    if depth > MAX_REF_DEPTH {
        // Depth exceeded — likely a circular $ref chain or pathological nesting.
        // Return Null to prevent stack overflow (ER-602: no panic in production).
        return Value::Null;
    }
    match value {
        // Single-field $ref → directly resolve, then chain-resolve the result
        Value::Object(m) if m.contains_key("$ref") && m.len() == 1 => {
            let resolved = resolve_path(state, value);
            // Chain resolution: if the resolved value is itself a $ref, keep resolving.
            // Increment depth to guard against circular references.
            resolve_refs_inner(state, &resolved, depth + 1)
        }
        // Single-field $pass → passthrough instruction parameters (no chain needed)
        Value::Object(m) if m.contains_key("$pass") && m.len() == 1 => resolve_pass(state, value),
        // Multi-field Object → recursively resolve each value
        Value::Object(m) => {
            let resolved: im::HashMap<String, Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), resolve_refs_inner(state, v, depth + 1)))
                .collect();
            Value::Object(resolved)
        }
        // List → recursively resolve each element
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|v| resolve_refs_inner(state, v, depth + 1))
                .collect(),
        ),
        // Atomic value → return as-is
        other => other.clone(),
    }
}

// ══════════════════════════════════════════════
// $ref path reference resolution
// ══════════════════════════════════════════════

/// Resolve a $ref path reference.
///
/// If the value contains a "$ref" field, reads the value at that path from the state.
/// Otherwise, returns the value itself.
pub fn resolve_path(state: &State, value: &Value) -> Value {
    match value {
        Value::Object(m) if m.contains_key("$ref") => {
            if let Some(path) = m.get("$ref").and_then(|v| v.as_str()) {
                resolve_ref_path(state, path)
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

/// Full path reference resolution.
///
/// Supports path formats:
///   - "x" → state.x
///   - "x.y" → state.x.y
///   - `x[0]` → `state.x[0]`
///   - `__exec__.instruction.params.attr` → instruction parameter
///   - `results[0].score` → nested field in an object array
pub fn resolve_ref_path(state: &State, path: &str) -> Value {
    if path.is_empty() {
        return state.to_value();
    }

    // Use State::get_path for unified path resolution,
    // supporting .-separated keys, [index] array indices, mixed paths, and all other formats.
    state.get_path(path).unwrap_or(Value::Null)
}

// ══════════════════════════════════════════════
// $pass explicit passthrough declaration
// ══════════════════════════════════════════════

/// Resolve a $pass explicit passthrough declaration.
///
/// `$pass` is syntactic sugar for `$ref`, specifically for passing through
/// the current instruction's parameters:
/// - `{"$pass": ""}` → passthrough the entire `__exec__.instruction.params`
/// - `{"$pass": "attr"}` → passthrough `__exec__.instruction.params.attr`
///
/// Difference from `$ref`:
/// - `$ref` is a general-purpose path reference that can reference any path in state
/// - `$pass` can only reference the current instruction's parameters, but is more concise
///   and more transparent
///
/// Transparency guarantee: `{"$pass": "attr"}` in the cases table explicitly declares
/// "this parameter comes from the instruction's attr field", which is more traceable
/// than implicit passthrough.
pub fn resolve_pass(state: &State, value: &Value) -> Value {
    let field = value.get("$pass").and_then(|v| v.as_str()).unwrap_or("");

    if field.is_empty() {
        // Passthrough entire __exec__.instruction.params
        state
            .get("__exec__")
            .and_then(|v| v.get("instruction"))
            .and_then(|v| v.get("params"))
            .cloned()
            .unwrap_or(Value::empty_object())
    } else {
        // Passthrough __exec__.instruction.params.<field>
        state
            .get("__exec__")
            .and_then(|v| v.get("instruction"))
            .and_then(|v| v.get("params"))
            .and_then(|p| p.get(field))
            .cloned()
            .unwrap_or(Value::Null)
    }
}

/// Write a value along a path in the State.
pub fn set_path_value(state: &State, path: &str, value: Value) -> Result<State, EvoRuleError> {
    if path.is_empty() {
        return Ok(State::from_value(&value));
    }

    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() == 1 {
        return Ok(state.set(parts[0], value));
    }

    // Deep path: need to build intermediate objects
    build_nested_state(state, &parts, &value)
}

/// Build a nested State (for deep path writes).
fn build_nested_state(state: &State, parts: &[&str], value: &Value) -> Result<State, EvoRuleError> {
    if parts.len() == 1 {
        return Ok(state.set(parts[0], value.clone()));
    }

    // Get existing child state or create a new one
    let child = state
        .get(parts[0])
        .cloned()
        .unwrap_or(Value::empty_object());
    let child_state = State::from_value(&child);
    let updated_child = build_nested_state(&child_state, &parts[1..], value)?;
    Ok(state.set(parts[0], updated_child.to_value()))
}

// ══════════════════════════════════════════════
// Registration
// ══════════════════════════════════════════════

/// Register control flow primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("dispatch", exec_dispatch);
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[allow(clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_resolve_ref_path_simple() {
        let state = State::new(vec![("x", Value::Integer(42))]);
        assert_eq!(resolve_ref_path(&state, "x"), Value::Integer(42));
    }

    #[test]
    fn test_resolve_ref_path_null() {
        let state = State::new(vec![("x", Value::Integer(42))]);
        assert_eq!(resolve_ref_path(&state, "y"), Value::Null);
    }

    #[test]
    fn test_resolve_path_ref_handling() {
        let state = State::new(vec![("x", Value::Integer(10))]);

        // Value with $ref
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_path(&state, &ref_val), Value::Integer(10));

        // Value without $ref should be returned as-is
        assert_eq!(resolve_path(&state, &Value::Integer(5)), Value::Integer(5));
    }

    #[test]
    fn test_set_path_value_top_level() {
        let state = State::empty();
        let result = set_path_value(&state, "x", Value::Integer(42)).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(42)));
    }

    #[test]
    fn test_dispatch_unknown_type() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(1))]);
        let instr = GenericInstruction::simple("unknown_type");
        let result = exec_dispatch(&reg, &state, &instr);
        // dispatch_by_cases with unknown key, no match, no default → returns original state (no error)
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_refs_simple_ref() {
        // {"$ref": "x"} → state.x value
        let state = State::new(vec![("x", Value::Integer(42))]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Integer(42));
    }

    #[test]
    fn test_resolve_refs_nested_object() {
        // {"type": "set_context", "params": {"attr": {"$ref": "x"}}}
        // → {"type": "set_context", "params": {"attr": 42}}
        let state = State::new(vec![("x", Value::Integer(42))]);
        let nested = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("set_context")),
            (
                "params".to_string(),
                Value::Object(im::HashMap::from(vec![(
                    "attr".to_string(),
                    Value::Object(im::HashMap::from(vec![(
                        "$ref".to_string(),
                        Value::string("x"),
                    )])),
                )])),
            ),
        ]));
        let resolved = resolve_refs(&state, &nested);
        // Verify $ref was resolved
        if let Value::Object(m) = &resolved {
            if let Some(Value::Object(params)) = m.get("params") {
                assert_eq!(params.get("attr"), Some(&Value::Integer(42)));
            } else {
                panic!("params should be Object");
            }
        } else {
            panic!("resolved should be Object");
        }
    }

    #[test]
    fn test_resolve_refs_no_ref_passthrough() {
        // Values without $ref should be returned unchanged
        let state = State::new(vec![("x", Value::Integer(42))]);
        assert_eq!(resolve_refs(&state, &Value::Integer(5)), Value::Integer(5));
        assert_eq!(
            resolve_refs(&state, &Value::string("hello")),
            Value::string("hello")
        );
    }

    #[test]
    fn test_resolve_refs_list() {
        // $ref inside a List should be recursively resolved
        let state = State::new(vec![("x", Value::Integer(10))]);
        let list = Value::List(
            vec![
                Value::Object(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("x"),
                )])),
                Value::Integer(20),
            ]
            .into(),
        );
        let resolved = resolve_refs(&state, &list);
        assert_eq!(
            resolved,
            Value::List(vec![Value::Integer(10), Value::Integer(20)].into())
        );
    }

    // ══════════════════════════════════════════
    // Chain resolution tests (P3 enhancement)
    // Verifies behavior change introduced by resolve_refs_inner chain logic.
    // ══════════════════════════════════════════

    #[test]
    fn test_resolve_refs_chain_two_hops() {
        // state.x = {"$ref": "y"}, state.y = 42
        // {"$ref": "x"} should chain-resolve to 42 (not stop at {"$ref": "y"})
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Integer(42)),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Integer(42));
    }

    #[test]
    fn test_resolve_refs_chain_three_hops() {
        // state.a = {"$ref": "b"}, state.b = {"$ref": "c"}, state.c = "done"
        let state = State::new(vec![
            (
                "a",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b"),
                )])),
            ),
            (
                "b",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("c"),
                )])),
            ),
            ("c", Value::string("done")),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("a"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::string("done"));
    }

    #[test]
    fn test_resolve_refs_chain_to_list() {
        // state.x = {"$ref": "y"}, state.y = [1, 2, 3]
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            (
                "y",
                Value::List(vec![Value::Integer(1), Value::Integer(2)].into()),
            ),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(
            resolve_refs(&state, &ref_val),
            Value::List(vec![Value::Integer(1), Value::Integer(2)].into())
        );
    }

    #[test]
    fn test_resolve_refs_chain_to_object_without_ref() {
        // Chain resolves to a plain object (no $ref) — should stop and return it
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            (
                "y",
                Value::from(im::HashMap::from(vec![
                    ("name".to_string(), Value::string("alice")),
                    ("age".to_string(), Value::Integer(30)),
                ])),
            ),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        let resolved = resolve_refs(&state, &ref_val);
        if let Value::Object(m) = &resolved {
            assert_eq!(m.get("name"), Some(&Value::string("alice")));
            assert_eq!(m.get("age"), Some(&Value::Integer(30)));
        } else {
            panic!("expected Object, got {:?}", resolved);
        }
    }

    #[test]
    fn test_resolve_refs_chain_self_cycle() {
        // state.x = {"$ref": "x"} — direct self-cycle
        let state = State::new(vec![(
            "x",
            Value::from(im::HashMap::from(vec![(
                "$ref".to_string(),
                Value::string("x"),
            )])),
        )]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        // Should return Null after MAX_REF_DEPTH exceeded, not stack overflow
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_resolve_refs_chain_mutual_cycle() {
        // state.a = {"$ref": "b"}, state.b = {"$ref": "a"} — mutual cycle
        let state = State::new(vec![
            (
                "a",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b"),
                )])),
            ),
            (
                "b",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("a"),
                )])),
            ),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("a"),
        )]));
        // Should return Null after depth exceeded, not stack overflow
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_resolve_refs_chain_three_way_cycle() {
        // state.a = {"$ref": "b"}, state.b = {"$ref": "c"}, state.c = {"$ref": "a"}
        let state = State::new(vec![
            (
                "a",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b"),
                )])),
            ),
            (
                "b",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("c"),
                )])),
            ),
            (
                "c",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("a"),
                )])),
            ),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("a"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_resolve_refs_chain_depth_boundary_exactly_64() {
        // Chain of exactly 64 hops should resolve successfully (depth=0..64, 64 ≤ 64).
        // Build: state.k0 = {"$ref": "k1"}, k1 = {"$ref": "k2"}, ... k63 = "end"
        let mut state = State::empty();
        for i in 0..63 {
            let ref_val = Value::from(im::HashMap::from(vec![(
                "$ref".to_string(),
                Value::string(format!("k{}", i + 1)),
            )]));
            state = state.set(format!("k{}", i), ref_val);
        }
        state = state.set("k63", Value::string("end"));
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("k0"),
        )]));
        // 63 hops + initial call = depth 64 at "end", which is ≤ MAX_REF_DEPTH (64)
        assert_eq!(resolve_refs(&state, &ref_val), Value::string("end"));
    }

    #[test]
    fn test_resolve_refs_chain_depth_exceeded_65() {
        // Chain of 65 hops should hit depth limit and return Null
        let mut state = State::empty();
        for i in 0..64 {
            let ref_val = Value::from(im::HashMap::from(vec![(
                "$ref".to_string(),
                Value::string(format!("k{}", i + 1)),
            )]));
            state = state.set(format!("k{}", i), ref_val);
        }
        state = state.set("k64", Value::string("end"));
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("k0"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_resolve_refs_chain_inside_nested_object() {
        // Nested object containing a $ref that chains: {"wrapper": {"$ref": "x"}} where x = {"$ref": "y"}, y = 99
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Integer(99)),
        ]);
        let nested = Value::from(im::HashMap::from(vec![(
            "wrapper".to_string(),
            Value::from(im::HashMap::from(vec![(
                "$ref".to_string(),
                Value::string("x"),
            )])),
        )]));
        let resolved = resolve_refs(&state, &nested);
        if let Value::Object(m) = &resolved {
            assert_eq!(m.get("wrapper"), Some(&Value::Integer(99)));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn test_resolve_refs_chain_inside_list() {
        // List element is a $ref that chains: [{"$ref": "x"}, 2] where x = {"$ref": "y"}, y = 1
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Integer(1)),
        ]);
        let list = Value::List(
            vec![
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("x"),
                )])),
                Value::Integer(2),
            ]
            .into(),
        );
        let resolved = resolve_refs(&state, &list);
        assert_eq!(
            resolved,
            Value::List(vec![Value::Integer(1), Value::Integer(2)].into())
        );
    }

    #[test]
    fn test_resolve_refs_chain_to_null_target() {
        // state.x = {"$ref": "y"}, y does not exist → resolve_path returns Null
        // Chain should stop at Null (not a $ref object)
        let state = State::new(vec![(
            "x",
            Value::from(im::HashMap::from(vec![(
                "$ref".to_string(),
                Value::string("y"),
            )])),
        )]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_resolve_refs_no_chain_for_pass() {
        // $pass should NOT chain-resolve even if result contains $ref.
        // $pass is a passthrough of instruction params, not a state lookup.
        // Build __exec__.instruction.params.attr = {"$ref": "nonexistent"}
        let inner_ref = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("nonexistent"),
        )]));
        let params = im::HashMap::from(vec![("attr".to_string(), inner_ref.clone())]);
        let instruction = im::HashMap::from(vec![
            ("type".to_string(), Value::string("set_context")),
            ("params".to_string(), Value::Object(params)),
        ]);
        let exec = im::HashMap::from(vec![(
            "instruction".to_string(),
            Value::Object(instruction),
        )]);
        let state = State::new(vec![("__exec__", Value::Object(exec))]);

        let pass_val = Value::from(im::HashMap::from(vec![(
            "$pass".to_string(),
            Value::string("attr"),
        )]));
        // $pass returns the raw param value {"$ref": "nonexistent"} — no chain resolution
        let resolved = resolve_refs(&state, &pass_val);
        if let Value::Object(m) = &resolved {
            assert!(m.contains_key("$ref"));
        } else {
            panic!(
                "expected $pass to return raw value without chain resolution, got {:?}",
                resolved
            );
        }
    }

    #[test]
    fn test_resolve_refs_chain_to_integer_target() {
        // Chain ends at an integer — verify type is preserved
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Integer(i64::MAX)),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Integer(i64::MAX));
    }

    #[test]
    fn test_resolve_refs_chain_to_bool_target() {
        // Chain ends at a boolean
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Bool(true)),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Bool(true));
    }

    #[test]
    fn test_resolve_refs_multiple_independent_chains_in_one_object() {
        // Two $ref fields in a multi-field object, each with different chain lengths
        let state = State::new(vec![
            (
                "a1",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("a2"),
                )])),
            ),
            ("a2", Value::Integer(100)),
            (
                "b1",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b2"),
                )])),
            ),
            (
                "b2",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b3"),
                )])),
            ),
            ("b3", Value::string("final")),
        ]);
        // Multi-field object with two $ref — note: $ref with len==1 triggers chain,
        // but here we embed $ref inside nested objects to keep them as regular fields.
        let input = Value::from(im::HashMap::from(vec![
            (
                "first".to_string(),
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("a1"),
                )])),
            ),
            (
                "second".to_string(),
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("b1"),
                )])),
            ),
        ]));
        let resolved = resolve_refs(&state, &input);
        if let Value::Object(m) = &resolved {
            assert_eq!(m.get("first"), Some(&Value::Integer(100)));
            assert_eq!(m.get("second"), Some(&Value::string("final")));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn test_resolve_refs_chain_to_empty_list() {
        // Chain resolves to an empty list — should not panic
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::List(im::Vector::new())),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(
            resolve_refs(&state, &ref_val),
            Value::List(im::Vector::new())
        );
    }

    #[test]
    fn test_resolve_refs_chain_preserves_null_value() {
        // state.x = {"$ref": "y"}, state.y = Value::Null (explicitly set)
        let state = State::new(vec![
            (
                "x",
                Value::from(im::HashMap::from(vec![(
                    "$ref".to_string(),
                    Value::string("y"),
                )])),
            ),
            ("y", Value::Null),
        ]);
        let ref_val = Value::from(im::HashMap::from(vec![(
            "$ref".to_string(),
            Value::string("x"),
        )]));
        assert_eq!(resolve_refs(&state, &ref_val), Value::Null);
    }

    #[test]
    fn test_dispatch_by_cases_with_ref_resolution() {
        // Simulate core_eval.json dispatch flow:
        // key = "increment", cases increment → set_context with $ref
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // Construct state: x=5, __exec__.instruction.type="increment",
        // __exec__.instruction.params.attr="x", __exec__.instruction.params.delta=3
        let exec_inner = im::HashMap::from(vec![
            ("type".to_string(), Value::string("increment")),
            (
                "params".to_string(),
                Value::Object(im::HashMap::from(vec![
                    ("attr".to_string(), Value::string("x")),
                    ("delta".to_string(), Value::Integer(3)),
                ])),
            ),
        ]);
        let state = State::new(vec![
            ("x", Value::Integer(5)),
            (
                "__exec__",
                Value::Object(im::HashMap::from(vec![(
                    "instruction".to_string(),
                    Value::Object(exec_inner),
                )])),
            ),
        ]);

        // Construct dispatch instruction (params use std::collections::HashMap)
        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::HashMap::from(vec![(
                        "$ref".to_string(),
                        Value::string("__exec__.instruction.type"),
                    )])),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::HashMap::from(vec![(
                        "increment".to_string(),
                        Value::Object(im::HashMap::from(vec![
                            ("type".to_string(), Value::string("set_context")),
                            (
                                "params".to_string(),
                                Value::Object(im::HashMap::from(vec![(
                                    "transform".to_string(),
                                    Value::Object(im::HashMap::from(vec![
                                        (
                                            "attr".to_string(),
                                            Value::Object(im::HashMap::from(vec![(
                                                "$ref".to_string(),
                                                Value::string("__exec__.instruction.params.attr"),
                                            )])),
                                        ),
                                        ("operation".to_string(), Value::string("add")),
                                        (
                                            "value".to_string(),
                                            Value::Object(im::HashMap::from(vec![(
                                                "$ref".to_string(),
                                                Value::string("__exec__.instruction.params.delta"),
                                            )])),
                                        ),
                                    ])),
                                )])),
                            ),
                        ])),
                    )])),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr).unwrap();
        // x should change from 5 to 8 (5 + 3)
        assert_eq!(result.get("x"), Some(&Value::Integer(8)));
    }

    // ══════════════════════════════════════════
    // P1-2 Specific tests: $pass explicit passthrough declaration
    // ══════════════════════════════════════════

    #[test]
    fn test_p12_pass_single_field() {
        // {"$pass": "attr"} is equivalent to {"$ref": "__exec__.instruction.params.attr"}
        let state = State::new(vec![
            ("x", Value::Integer(42)),
            (
                "__exec__",
                Value::Object(im::hashmap! {
                    "instruction".to_string() => Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("test"),
                        "params".to_string() => Value::Object(im::hashmap! {
                            "attr".to_string() => Value::string("x"),
                            "delta".to_string() => Value::Integer(3),
                        }),
                    }),
                }),
            ),
        ]);

        let pass_val = Value::Object(im::hashmap! {
            "$pass".to_string() => Value::string("attr"),
        });
        let ref_val = Value::Object(im::hashmap! {
            "$ref".to_string() => Value::string("__exec__.instruction.params.attr"),
        });

        // $pass and $ref should produce the same result
        assert_eq!(
            resolve_refs(&state, &pass_val),
            resolve_refs(&state, &ref_val)
        );
        assert_eq!(resolve_refs(&state, &pass_val), Value::string("x"));
    }

    #[test]
    fn test_p12_pass_whole_params() {
        // {"$pass": ""} passthrough entire __exec__.instruction.params
        let state = State::new(vec![
            ("x", Value::Integer(42)),
            (
                "__exec__",
                Value::Object(im::hashmap! {
                    "instruction".to_string() => Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("test"),
                        "params".to_string() => Value::Object(im::hashmap! {
                            "key1".to_string() => Value::string("val1"),
                            "key2".to_string() => Value::Integer(99),
                        }),
                    }),
                }),
            ),
        ]);

        let pass_val = Value::Object(im::hashmap! {
            "$pass".to_string() => Value::string(""),
        });

        let result = resolve_refs(&state, &pass_val);
        assert_eq!(result.get("key1"), Some(&Value::string("val1")));
        assert_eq!(result.get("key2"), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_p12_pass_in_dispatch() {
        // Use $pass instead of $ref in dispatch
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        let exec_inner = im::HashMap::from(vec![
            ("type".to_string(), Value::string("increment")),
            (
                "params".to_string(),
                Value::Object(im::HashMap::from(vec![
                    ("attr".to_string(), Value::string("x")),
                    ("delta".to_string(), Value::Integer(3)),
                ])),
            ),
        ]);
        let state = State::new(vec![
            ("x", Value::Integer(5)),
            (
                "__exec__",
                Value::Object(im::HashMap::from(vec![(
                    "instruction".to_string(),
                    Value::Object(exec_inner),
                )])),
            ),
        ]);

        // Use $pass instead of $ref
        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("__exec__.instruction.type"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "increment".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("set_context"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "transform".to_string() => Value::Object(im::hashmap! {
                                    "attr".to_string() => Value::Object(im::hashmap! {
                                        "$pass".to_string() => Value::string("attr"),
                                    }),
                                    "operation".to_string() => Value::string("add"),
                                    "value".to_string() => Value::Object(im::hashmap! {
                                        "$pass".to_string() => Value::string("delta"),
                                    }),
                                }),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr).unwrap();
        // x should change from 5 to 8 (5 + 3), same as the $ref version
        assert_eq!(result.get("x"), Some(&Value::Integer(8)));
    }

    #[test]
    fn test_p12_pass_missing_field() {
        // $pass referencing a non-existent field should return Null
        let state = State::new(vec![
            ("x", Value::Integer(42)),
            (
                "__exec__",
                Value::Object(im::hashmap! {
                    "instruction".to_string() => Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("test"),
                        "params".to_string() => Value::empty_object(),
                    }),
                }),
            ),
        ]);

        let pass_val = Value::Object(im::hashmap! {
            "$pass".to_string() => Value::string("nonexistent"),
        });

        assert_eq!(resolve_refs(&state, &pass_val), Value::Null);
    }

    /// Test dispatch with Bool key matching.
    ///
    /// Verifies that when the $ref-resolved key is Bool(true),
    /// the "true" key in the cases table matches correctly
    /// (fixing the Bool→String conversion vulnerability).
    #[test]
    fn test_dispatch_bool_key_matching() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // Construct state: flag = Bool(true)
        let state = State::new(vec![("flag", Value::Bool(true)), ("x", Value::Integer(0))]);

        // dispatch: key = $ref(flag), cases = { "true": set x=1, "false": set x=2 }
        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("flag"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "true".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(1),
                            }),
                        }),
                        "false".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(2),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr)
            .expect("dispatch with Bool key should not panic");

        // flag=true → should match the "true" case → x=1
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(1)),
            "Bool(true) key should match 'true' case in cases"
        );
    }

    /// Test dispatch with Bool(false) key matching.
    #[test]
    fn test_dispatch_bool_false_key_matching() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // Construct state: flag = Bool(false)
        let state = State::new(vec![("flag", Value::Bool(false)), ("x", Value::Integer(0))]);

        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("flag"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "true".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(1),
                            }),
                        }),
                        "false".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(2),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr)
            .expect("dispatch with Bool(false) key should not panic");

        // flag=false → should match the "false" case → x=2
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(2)),
            "Bool(false) key should match 'false' case in cases"
        );
    }

    /// Test dispatch with Integer key matching.
    #[test]
    fn test_dispatch_integer_key_matching() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // Construct state: level = Integer(2)
        let state = State::new(vec![("level", Value::Integer(2)), ("x", Value::Integer(0))]);

        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("level"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "1".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(10),
                            }),
                        }),
                        "2".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(20),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr)
            .expect("dispatch with Integer key should not panic");

        // level=2 → should match the "2" case → x=20
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(20)),
            "Integer(2) key should match '2' case in cases"
        );
    }

    /// Test dispatch with Null key matching.
    #[test]
    fn test_dispatch_null_key_matching() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // Construct state: missing_key does not exist → $ref resolves to Null
        let state = State::new(vec![("x", Value::Integer(0))]);

        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("missing_key"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "null".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(-1),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr)
            .expect("dispatch with Null key should not panic");

        // missing_key → Null → should match the "null" case → x=-1
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(-1)),
            "Null key should match 'null' case in cases"
        );
    }

    // ── [FIX #4] Specific tests: skip advance_instruction when __running=false ──

    #[test]
    fn test_dispatch_skips_advance_when_running_false() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        // __running=false, state_set executes but does not push advance_instruction
        let state = State::new(vec![("x", Value::Integer(0))]);
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(false),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("set_context"),
                Value::string("advance_instruction"),
            ]),
        });
        let state = state.set("__exec__", exec_ctx);

        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("__exec__.instruction.type"),
                    }),
                ),
                (
                    "cases".to_string(),
                    Value::Object(im::hashmap! {
                        "set".to_string() => Value::Object(im::hashmap! {
                            "type".to_string() => Value::string("state_set"),
                            "params".to_string() => Value::Object(im::hashmap! {
                                "attr".to_string() => Value::string("x"),
                                "value".to_string() => Value::Integer(5),
                            }),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(5)));

        // When __running=false, the queue should not contain advance_instruction
        let queue = get_queue(&result);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_dispatch_skips_advance_on_default_when_running_false() {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);

        let state = State::new(vec![("x", Value::Integer(0))]);
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("unknown"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(false),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("advance_instruction"),
            ]),
        });
        let state = state.set("__exec__", exec_ctx);

        let dispatch_instr = GenericInstruction::new(
            "dispatch",
            HashMap::from([
                (
                    "key".to_string(),
                    Value::Object(im::hashmap! {
                        "$ref".to_string() => Value::string("__exec__.instruction.type"),
                    }),
                ),
                ("cases".to_string(), Value::Object(im::hashmap! {})),
                (
                    "default".to_string(),
                    Value::Object(im::hashmap! {
                        "type".to_string() => Value::string("state_set"),
                        "params".to_string() => Value::Object(im::hashmap! {
                            "attr".to_string() => Value::string("x"),
                            "value".to_string() => Value::Integer(99),
                        }),
                    }),
                ),
            ]),
        );

        let result = exec_dispatch(&reg, &state, &dispatch_instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(99)));

        let queue = get_queue(&result);
        assert_eq!(queue.len(), 0);
    }
}
