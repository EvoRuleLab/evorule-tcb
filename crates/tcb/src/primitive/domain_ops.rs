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

//! Domain primitives — Domain evaluation and domain matching.
//!
//! # Core Functions
//!
//! - `evaluate_domain`: Domain evaluation + branch push to queue.
//! - `match_domain`: Domain matching + result storage.
//!
//! # Design Principles
//!
//! ## `evaluate_domain`: Conditional Branching
//!
//! This primitive evaluates a domain condition and pushes the selected branch
//! (`on_true` or `on_false`) to the front of the execution queue. The branch
//! is then executed by `while_loop`'s drain logic in the next iteration.
//!
//! This design aligns with v2's `evaluate_domain` semantics — the branch is
//! **pushed to the queue** rather than executed immediately, ensuring that
//! dispatch remains the sole source of instruction execution.
//!
//! ## Audit Traceability
//!
//! `evaluate_domain` writes a `__trace_branch` field to `__exec__` containing:
//! - `domain_result`: The boolean result of the domain evaluation.
//! - `chosen_branch`: `"then"`, `"else"`, or `"none"`.
//!
//! This enables audit trails to track which branch was selected, satisfying
//! C1 (Transparency) and C3 (Traceability) constitutional requirements.
//!
//! # Determinism Guarantee
//!
//! Both primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - Domain evaluation is a pure function (`Domain::contains`).
//! - Branch selection is deterministic (based on domain result).
//! - Queue push order is deterministic (front insertion).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `evaluate_domain` domain evaluation | ✅ L1 deterministic | `Domain::contains` |
//! | `evaluate_domain` branch selection | ✅ L1 deterministic | Boolean branch |
//! | `evaluate_domain` queue push | ✅ L1 deterministic | Front insertion |
//! | `evaluate_domain` branch validation | ✅ L1 deterministic | Registry/cases check |
//! | `match_domain` domain evaluation | ✅ L1 deterministic | `Domain::contains` |
//! | `match_domain` result storage | ✅ L1 deterministic | Pure assignment |
//! | `__trace_branch` recording | ✅ L1 deterministic | Deterministic record |
//!
//! # Cross-Language Note (L4)
//!
//! These primitives are Rust-only constructs; there is no cross-language equivalent.
//! However, the domain format (JSON) is language-agnostic and can be inspected
//! by other languages.

use crate::control::dispatch::resolve_path;
use crate::domain::Domain;
use crate::error::{missing_param, EvoRuleError};
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register domain primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("evaluate_domain", exec_evaluate_domain);
    reg.register("match_domain", exec_match_domain);
    reg.register("domain_intersect", exec_domain_intersect);
}

/// Evaluate a domain — determines whether the current state satisfies a given Domain.
///
/// After evaluating the domain, pushes the `on_true`/`on_false` branch to the queue,
/// to be popped and executed by `while_loop`'s drain logic in the next iteration.
///
/// # Parameters
/// - `domain`: The domain to evaluate (required).
/// - `on_true`: Instruction to execute if the domain evaluates to `true` (optional).
/// - `on_false`: Instruction to execute if the domain evaluates to `false` (optional).
///
/// # Behavior
/// - Evaluates `domain` against the current state.
/// - If `domain` is `true` and `on_true` is provided, pushes `on_true` to the queue front.
/// - If `domain` is `false` and `on_false` is provided, pushes `on_false` to the queue front.
/// - If the selected branch is not provided, nothing is pushed to the queue.
/// - Writes `__trace_branch` to `__exec__` for audit traceability.
/// - Writes `__domain_result__` to the state.
///
/// # Startup Validation
/// - When `__exec__.dispatch_cases` exists (production environment), validates that
///   the branch instruction types are registered in the registry or cases table.
/// - Test environments without `dispatch_cases` skip validation.
///
/// # Errors
/// - `MissingParam`: `domain` parameter is missing.
/// - `EvoRuleError`: Domain parsing fails.
/// - `InvalidConfig`: Branch instruction type is unregistered.
///
/// # Audit Trace (`__trace_branch`)
/// - `chosen_branch`: `"then"` (domain true, `on_true` selected), `"else"`
///   (domain false, `on_false` selected), or `"none"` (branch missing).
/// - `domain_result`: The boolean result of the domain evaluation.
pub(crate) fn exec_evaluate_domain(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let domain_val = instruction
        .params
        .get("domain")
        .cloned()
        .ok_or_else(|| missing_param("evaluate_domain", "domain"))?;

    let domain = Domain::from_value(&domain_val)?;
    let result = domain.contains(state);

    let on_true = instruction.params.get("on_true").cloned();
    let on_false = instruction.params.get("on_false").cloned();

    // Validate that branch instruction types are dispatchable (in registry or cases table)
    // Only validate when __exec__.dispatch_cases exists (production environment)
    // Test environments may not have dispatch_cases, so validation is skipped.
    let dispatch_cases = state
        .get("__exec__")
        .and_then(|v| v.get("dispatch_cases"))
        .and_then(|v| v.as_object());
    if dispatch_cases.is_some() {
        for (label, branch) in [("on_true", &on_true), ("on_false", &on_false)] {
            if let Some(ref val) = branch {
                super::validate_instruction_type(
                    reg,
                    state,
                    val,
                    &format!("evaluate_domain {label}"),
                )?;
            }
        }
    }
    let chosen = if result { on_true } else { on_false };

    // chosen_branch identifies the branch selected by the domain evaluation result:
    // - "then": domain is true, on_true branch selected
    // - "else": domain is false, on_false branch selected
    // - "none": domain evaluated to true/false but the corresponding branch instruction is None
    //
    // Consumers (e.g., trace_step or audit logs) can use chosen_branch to determine
    // the actual execution path. "none" indicates that although the domain evaluated
    // to a result, the branch was undefined (skipped), which is meaningful when
    // conditional branching is optional.
    let chosen_branch = if chosen.is_some() {
        if result {
            "then"
        } else {
            "else"
        }
    } else {
        "none"
    };

    let exec_val = state
        .get("__exec__")
        .cloned()
        .unwrap_or(Value::empty_object());
    let mut exec_map = match &exec_val {
        Value::Object(m) => m.clone(),
        _ => im::HashMap::new(),
    };
    exec_map.insert(
        "__trace_branch".to_string(),
        Value::Object(im::hashmap! {
            "domain_result".to_string() => Value::Bool(result),
            "chosen_branch".to_string() => Value::string(chosen_branch),
        }),
    );

    // Push the selected branch to the queue
    if let Some(chosen_val) = chosen {
        let mut queue = super::queue_ops::get_queue(state);
        queue.insert(0, chosen_val);
        exec_map.insert("queue".to_string(), Value::List(queue));
    }

    let new_state = state.set("__exec__", Value::Object(exec_map));
    Ok(new_state.set("__domain_result__", Value::Bool(result)))
}

/// Domain matching — determines whether a given state satisfies a specific Domain.
///
/// # Parameters
/// - `domain`: The domain to evaluate (required).
/// - `state_ref` (optional): External state to evaluate against (resolved via `$ref`).
///   If not provided, uses the current state.
/// - `state_data` (legacy alias for `state_ref`): Supported for backward compatibility.
/// - `result_attr` (optional): Attribute to store the result (default: `"__matched__"`).
/// - `store_as` (legacy alias for `result_attr`): Supported for backward compatibility.
///
/// # Behavior
/// - Evaluates `domain` against the target state.
/// - Stores the boolean result in `result_attr`.
/// - If `state_ref` is provided, evaluates against that state instead of the current state.
///
/// # Errors
/// - `MissingParam`: `domain` parameter is missing.
/// - `EvoRuleError`: Domain parsing fails.
///
/// # Example
/// ```json
/// {
///   "type": "match_domain",
///   "params": {
///     "domain": { "type": "atom", "attribute": "x", "op": "eq", "value": 42 },
///     "result_attr": "x_is_42"
///   }
/// }
/// ```
pub(crate) fn exec_match_domain(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let domain_raw = instruction
        .params
        .get("domain")
        .ok_or_else(|| missing_param("match_domain", "domain"))?;
    let domain_val = resolve_path(state, domain_raw);
    let domain = Domain::from_value(&domain_val)?;

    // Supports state_ref (core_eval.json) and state_data (legacy interface)
    let target_data = instruction
        .params
        .get("state_ref")
        .or_else(|| instruction.params.get("state_data"))
        .map(|v| resolve_path(state, v))
        .filter(|v| !v.is_null())
        .unwrap_or(state.to_value());

    let target_state = State::from_value(&target_data);
    let matched = domain.contains(&target_state);

    // Supports result_attr (core_eval.json) and store_as (legacy interface)
    let store_as = instruction
        .params
        .get("result_attr")
        .or_else(|| instruction.params.get("store_as"))
        .map(|v| resolve_path(state, v))
        .and_then(|v| v.as_str().map(std::string::ToString::to_string))
        .unwrap_or_else(|| "__matched__".to_string());

    Ok(state.set_path(&store_as, Value::Bool(matched)))
}

/// Domain intersection — determines whether two domain trees *may* have a
/// non-empty intersection (conservative over-approximation).
///
/// This is a pure structural judgment over two domain Value trees; it does
/// NOT depend on the current state and does NOT execute any rule. It is the
/// sole primitive that unlocks composition-based rewrites of v4's
/// `detect_conflicts` / `detect_cycles` / `analyze_rule_effects`
/// (see ADR-05; business logic stays in the rule layer via Recipe G).
///
/// # Parameters
/// - `domain1` (required): first domain Value (supports `$ref`).
/// - `domain2` (required): second domain Value (supports `$ref`).
/// - `result_attr` (required): state path where the boolean result is written.
///
/// # Semantics (matches v4 `domain_values_overlap`)
/// | `(type1, type2)`                              | Result                             |
/// | --------------------------------------------- | ---------------------------------- |
/// | `(universal, *)` or `(*, universal)`          | `true`                             |
/// | `(empty, *)` or `(*, empty)`                  | `false`                            |
/// | `(atom, atom)`                                | `true` iff `attr1 == attr2`        |
/// | `(and, X)`                                    | `true` iff ALL children intersect X |
/// | `(or, X)`                                     | `true` iff ANY child intersects X   |
/// | `(not, X)`                                    | `true` (conservative; undecidable) |
/// | `(atom, and/or)`                              | recurse with compound side primary |
/// | otherwise                                     | `true` (conservative default)      |
///
/// # Determinism
/// L1 deterministic: same `(domain1, domain2)` → same result. No I/O, no
/// randomness, no state read beyond the two domain params.
///
/// # Errors
/// - `MissingParam`: `domain1`, `domain2`, or `result_attr` is missing.
///
/// # Example
/// ```json
/// {
///   "type": "domain_intersect",
///   "params": {
///     "domain1": { "$ref": "_r1.domain" },
///     "domain2": { "$ref": "_r2.domain" },
///     "result_attr": "_overlap"
///   }
/// }
/// ```
pub(crate) fn exec_domain_intersect(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let d1_raw = instruction
        .params
        .get("domain1")
        .ok_or_else(|| missing_param("domain_intersect", "domain1"))?;
    let d2_raw = instruction
        .params
        .get("domain2")
        .ok_or_else(|| missing_param("domain_intersect", "domain2"))?;
    let d1 = resolve_path(state, d1_raw);
    let d2 = resolve_path(state, d2_raw);

    let result_attr = instruction
        .params
        .get("result_attr")
        .map(|v| resolve_path(state, v))
        .and_then(|v| v.as_str().map(std::string::ToString::to_string))
        .ok_or_else(|| missing_param("domain_intersect", "result_attr"))?;

    let intersects = domain_values_overlap(&d1, &d2);
    Ok(state.set_path(&result_attr, Value::Bool(intersects)))
}

/// Recursive check whether two domain Value trees may overlap.
///
/// Conservative over-approximation: false negatives are forbidden, false
/// positives are tolerable. Direct port of v4's `domain_values_overlap`
/// (`evorule-v4/crates/tcb/src/primitive/inference_ops.rs:376`).
///
/// **Note on `and` semantics**: v4 uses `any` for `domains` list form and `&&`
/// for `left/right` binary form. This inconsistency is preserved for behavioral
/// compatibility. The `domains` list form (`any`) is the correct
/// over-approximation; the `left/right` form (`&&`) is technically
/// under-approximation but retained to match v4 exactly.
fn domain_values_overlap(d1: &Value, d2: &Value) -> bool {
    let type1 = d1
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let type2 = d2
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Universal overlaps with anything.
    if type1 == "universal" || type2 == "universal" {
        return true;
    }
    // Empty overlaps with nothing.
    if type1 == "empty" || type2 == "empty" {
        return false;
    }
    // Two atoms: same attribute → maybe overlap (conservative; ignores op/value).
    if type1 == "atom" && type2 == "atom" {
        let attr1 = d1.get("attribute").and_then(|v| v.as_str()).unwrap_or("");
        let attr2 = d2.get("attribute").and_then(|v| v.as_str()).unwrap_or("");
        return attr1 == attr2;
    }
    // `and`/`or` compound: recurse against the other domain.
    // Matches v4: `domains` list (or `left` as list) → `any` for both and/or.
    if type1 == "and" || type1 == "or" {
        if let Some(children) = d1
            .get("domains")
            .or_else(|| d1.get("left"))
            .and_then(|v| v.as_list())
        {
            return children.iter().any(|c| domain_values_overlap(c, d2));
        }
        // Binary `left/right` form: `and` → `&&`, `or` → `||` (matches v4).
        if let (Some(left), Some(right)) = (d1.get("left"), d1.get("right")) {
            if type1 == "and" {
                return domain_values_overlap(left, d2) && domain_values_overlap(right, d2);
            } else {
                return domain_values_overlap(left, d2) || domain_values_overlap(right, d2);
            }
        }
        // Malformed `and`/`or` (no children): conservative true.
        return true;
    }
    // d2 is compound, d1 is not: recurse with d2 as primary (symmetric).
    if type2 == "and" || type2 == "or" {
        if let Some(children) = d2
            .get("domains")
            .or_else(|| d2.get("left"))
            .and_then(|v| v.as_list())
        {
            return children.iter().any(|c| domain_values_overlap(d1, c));
        }
        if let (Some(left), Some(right)) = (d2.get("left"), d2.get("right")) {
            if type2 == "and" {
                return domain_values_overlap(d1, left) && domain_values_overlap(d1, right);
            } else {
                return domain_values_overlap(d1, left) || domain_values_overlap(d1, right);
            }
        }
        return true;
    }
    // `not` makes static analysis undecidable in general → conservative true.
    if type1 == "not" || type2 == "not" {
        return true;
    }
    // Unknown types: conservative true.
    true
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
    fn test_evaluate_domain() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        let domain = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(10)),
        ]));

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_match_domain_true() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);

        let domain = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(42)),
        ]));

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__matched__"), Some(&Value::Bool(true)));
    }

    // ══════════════════════════════════════════════
    // Additional domain_ops tests
    // ══════════════════════════════════════════════

    /// Test evaluate_domain: domain evaluates to false, selects the else branch
    #[test]
    fn test_evaluate_domain_false_selects_else_branch() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(99), // x=10 is not equal to 99, so false
        });

        let on_true = Value::from(im::hashmap! {
            "type".to_string() => Value::string("noop"),
        });
        let on_false = Value::from(im::hashmap! {
            "type".to_string() => Value::string("noop"),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        params.insert("on_true".to_string(), on_true);
        params.insert("on_false".to_string(), on_false);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();

        // Verify __domain_result__ is false
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(false)));

        // Verify the else branch was selected
        let trace = result
            .get("__exec__")
            .and_then(|v| v.get("__trace_branch"))
            .cloned()
            .unwrap();
        let chosen = trace.get("chosen_branch").and_then(|v| v.as_str());
        assert_eq!(chosen, Some("else"));
    }

    /// Test evaluate_domain: domain evaluates to true, selects the then branch
    #[test]
    fn test_evaluate_domain_true_selects_then_branch() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(10), // x=10 equals 10
        });

        let on_true = Value::from(im::hashmap! {
            "type".to_string() => Value::string("noop"),
        });
        let on_false = Value::from(im::hashmap! {
            "type".to_string() => Value::string("noop"),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        params.insert("on_true".to_string(), on_true);
        params.insert("on_false".to_string(), on_false);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();

        // Verify __domain_result__ is true
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(true)));

        // Verify the then branch was selected
        let trace = result
            .get("__exec__")
            .and_then(|v| v.get("__trace_branch"))
            .cloned()
            .unwrap();
        let chosen = trace.get("chosen_branch").and_then(|v| v.as_str());
        assert_eq!(chosen, Some("then"));
    }

    /// Test evaluate_domain: no on_true/on_false branches does not error
    #[test]
    fn test_evaluate_domain_no_branch_no_error() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(10),
        });

        // Only domain, no on_true/on_false
        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("evaluate_domain", params);

        // Should not error, only return __domain_result__
        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(true)));
    }

    /// Test match_domain: domain does not match, returns false
    #[test]
    fn test_match_domain_false() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(99), // x=10 is not equal to 99
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__matched__"), Some(&Value::Bool(false)));
    }

    /// Test match_domain: nested path
    #[test]
    fn test_match_domain_nested_path() {
        let reg = InstructionRegistry::new();
        // Create nested structure: { outer: { inner: 42 } }
        let state = State::new(vec![(
            "outer",
            Value::Object(im::hashmap! {
                "inner".to_string() => Value::Integer(42),
            }),
        )]);

        // Check outer.inner == 42
        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("outer.inner"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(42),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__matched__"), Some(&Value::Bool(true)));
    }

    /// Test match_domain: using state_ref to check external state
    #[test]
    fn test_match_domain_state_ref() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        // Create external state data
        let external_state = Value::from(im::hashmap! {
            "x".to_string() => Value::Integer(99),
        });

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(99),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        params.insert("state_ref".to_string(), external_state);
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr).unwrap();
        // External state has x=99, domain checks x=99, so it matches
        assert_eq!(result.get("__matched__"), Some(&Value::Bool(true)));
    }

    /// Test match_domain: custom result_attr
    #[test]
    fn test_match_domain_custom_result_attr() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);

        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string("x"),
            "op".to_string() => Value::string("eq"),
            "value".to_string() => Value::Integer(42),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        params.insert("result_attr".to_string(), Value::string("my_match_result"));
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr).unwrap();
        // Using custom result_attr
        assert_eq!(result.get("my_match_result"), Some(&Value::Bool(true)));
        // Default __matched__ does not exist
        assert!(result.get("__matched__").is_none());
    }

    // ══════════════════════════════════════════════
    // domain_intersect tests (ADR-05)
    // ══════════════════════════════════════════════

    fn atom_domain(attr: &str, op: &str, value: i64) -> Value {
        Value::from(im::hashmap! {
            "type".to_string() => Value::string("atom"),
            "attribute".to_string() => Value::string(attr),
            "op".to_string() => Value::string(op),
            "value".to_string() => Value::Integer(value),
        })
    }

    fn run_domain_intersect(d1: Value, d2: Value) -> State {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![]);
        let mut params = HashMap::new();
        params.insert("domain1".to_string(), d1);
        params.insert("domain2".to_string(), d2);
        params.insert("result_attr".to_string(), Value::string("_overlap"));
        let instr = GenericInstruction::new("domain_intersect", params);
        exec_domain_intersect(&reg, &state, &instr).unwrap()
    }

    #[test]
    fn test_domain_intersect_same_attribute_atoms() {
        // Two atoms on the same attribute → conservative true (may overlap).
        let d1 = atom_domain("x", "eq", 10);
        let d2 = atom_domain("x", "eq", 99);
        let result = run_domain_intersect(d1, d2);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_different_attribute_atoms() {
        // Two atoms on different attributes → no overlap.
        let d1 = atom_domain("x", "eq", 10);
        let d2 = atom_domain("y", "eq", 10);
        let result = run_domain_intersect(d1, d2);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_domain_intersect_universal_overlaps_anything() {
        let universal = Value::from(im::hashmap! {
            "type".to_string() => Value::string("universal"),
        });
        let atom = atom_domain("x", "eq", 10);
        let result = run_domain_intersect(universal.clone(), atom.clone());
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
        // Symmetric direction.
        let result = run_domain_intersect(atom, universal);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_empty_overlaps_nothing() {
        let empty = Value::from(im::hashmap! {
            "type".to_string() => Value::string("empty"),
        });
        let atom = atom_domain("x", "eq", 10);
        let result = run_domain_intersect(empty.clone(), atom.clone());
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(false)));
        let result = run_domain_intersect(atom, empty);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_domain_intersect_and_uses_any_matching_v4_semantics() {
        // v4 uses `any` for `and` with `domains` list (over-approximation).
        // d1 = and(x, y), d2 = atom(x) → x overlaps → true (conservative).
        let and_domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("and"),
            "domains".to_string() => Value::list(vec![
                atom_domain("x", "eq", 1),
                atom_domain("y", "eq", 2),
            ]),
        });
        let atom_x = atom_domain("x", "eq", 99);
        let result = run_domain_intersect(and_domain, atom_x);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_and_left_right_uses_and_matching_v4_semantics() {
        // v4 uses `&&` for `and` with `left/right` binary form.
        // d1 = and(left=x, right=y), d2 = atom(x) → left overlaps, right does not → false.
        let and_domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("and"),
            "left".to_string() => atom_domain("x", "eq", 1),
            "right".to_string() => atom_domain("y", "eq", 2),
        });
        let atom_x = atom_domain("x", "eq", 99);
        let result = run_domain_intersect(and_domain, atom_x);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_domain_intersect_or_left_right_uses_or_matching_v4_semantics() {
        // v4 uses `||` for `or` with `left/right` binary form.
        // d1 = or(left=x, right=y), d2 = atom(x) → left overlaps → true.
        let or_domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("or"),
            "left".to_string() => atom_domain("x", "eq", 1),
            "right".to_string() => atom_domain("y", "eq", 2),
        });
        let atom_x = atom_domain("x", "eq", 99);
        let result = run_domain_intersect(or_domain, atom_x);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_or_any_child_overlaps() {
        // d1 = or(x, y), d2 = atom(x) → x overlaps → true.
        let or_domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("or"),
            "domains".to_string() => Value::list(vec![
                atom_domain("x", "eq", 1),
                atom_domain("y", "eq", 2),
            ]),
        });
        let atom_x = atom_domain("x", "eq", 99);
        let result = run_domain_intersect(or_domain, atom_x);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_not_is_conservative_true() {
        // not vs atom → conservative true (undecidable).
        let not_domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("not"),
            "inner".to_string() => atom_domain("x", "eq", 1),
        });
        let atom_y = atom_domain("y", "eq", 99);
        let result = run_domain_intersect(not_domain, atom_y);
        assert_eq!(result.get("_overlap"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_domain_intersect_missing_params_errors() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![]);
        // Missing domain2.
        let mut params = HashMap::new();
        params.insert("domain1".to_string(), atom_domain("x", "eq", 1));
        params.insert("result_attr".to_string(), Value::string("_o"));
        let instr = GenericInstruction::new("domain_intersect", params);
        let err = exec_domain_intersect(&reg, &state, &instr).unwrap_err();
        assert!(err.to_string().contains("domain2"));
    }

    /// Test evaluate_domain with Not domain
    #[test]
    fn test_evaluate_domain_not() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);

        // NOT (x == 99) -> true, because x=10 is not equal to 99
        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("not"),
            "inner".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("x"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::Integer(99),
            }),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(true)));
    }

    /// Test evaluate_domain with And domain
    #[test]
    fn test_evaluate_domain_and() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10)), ("y", Value::Integer(20))]);

        // x == 10 AND y == 20 -> true
        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("and"),
            "domains".to_string() => Value::list(vec![
                Value::from(im::hashmap! {
                    "type".to_string() => Value::string("atom"),
                    "attribute".to_string() => Value::string("x"),
                    "op".to_string() => Value::string("eq"),
                    "value".to_string() => Value::Integer(10),
                }),
                Value::from(im::hashmap! {
                    "type".to_string() => Value::string("atom"),
                    "attribute".to_string() => Value::string("y"),
                    "op".to_string() => Value::string("eq"),
                    "value".to_string() => Value::Integer(20),
                }),
            ]),
        });

        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr).unwrap();
        assert_eq!(result.get("__domain_result__"), Some(&Value::Bool(true)));
    }

    // ─── P1: Error path tests ───────────────────────────────────────────────
    // Verify that invalid inputs return Err rather than panicking or silently
    // producing wrong results. Aligns with ER-600 (no runtime panics).

    #[test]
    fn test_evaluate_domain_missing_domain_param_returns_err() {
        // Missing "domain" param → should return Err(MissingParam)
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);
        let instr = GenericInstruction::new("evaluate_domain", HashMap::new());

        let result = exec_evaluate_domain(&reg, &state, &instr);
        assert!(result.is_err(), "missing domain param should return Err");
        let err = result.unwrap_err();
        match err {
            crate::error::EvoRuleError::MissingParam { param, .. } => {
                assert_eq!(param, "domain");
            }
            other => panic!("expected MissingParam, got {:?}", other),
        }
    }

    #[test]
    fn test_evaluate_domain_non_object_domain_returns_err() {
        // domain is a non-Object value (Integer) → Domain::from_value should fail
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);
        let mut params = HashMap::new();
        params.insert("domain".to_string(), Value::Integer(42));
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr);
        assert!(
            result.is_err(),
            "non-Object domain should return Err from Domain::from_value"
        );
    }

    #[test]
    fn test_evaluate_domain_invalid_domain_type_returns_err() {
        // domain has unknown "type" → Domain::from_value should fail
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);
        let domain = Value::from(im::hashmap! {
            "type".to_string() => Value::string("unknown_type"),
        });
        let mut params = HashMap::new();
        params.insert("domain".to_string(), domain);
        let instr = GenericInstruction::new("evaluate_domain", params);

        let result = exec_evaluate_domain(&reg, &state, &instr);
        assert!(result.is_err(), "invalid domain type should return Err");
    }

    #[test]
    fn test_match_domain_missing_domain_param_returns_err() {
        // Missing "domain" param → should return Err(MissingParam)
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);
        let instr = GenericInstruction::new("match_domain", HashMap::new());

        let result = exec_match_domain(&reg, &state, &instr);
        assert!(result.is_err(), "missing domain param should return Err");
        let err = result.unwrap_err();
        match err {
            crate::error::EvoRuleError::MissingParam { param, .. } => {
                assert_eq!(param, "domain");
            }
            other => panic!("expected MissingParam, got {:?}", other),
        }
    }

    #[test]
    fn test_match_domain_non_object_domain_returns_err() {
        // domain is a non-Object value (List) → Domain::from_value should fail
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(10))]);
        let mut params = HashMap::new();
        params.insert(
            "domain".to_string(),
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        );
        let instr = GenericInstruction::new("match_domain", params);

        let result = exec_match_domain(&reg, &state, &instr);
        assert!(
            result.is_err(),
            "non-Object domain should return Err from Domain::from_value"
        );
    }
}
