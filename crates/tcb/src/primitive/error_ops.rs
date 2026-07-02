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

//! Error handling primitives — try-catch and parallel execution.
//!
//! # Core Functions
//!
//! - `execute_try_catch`: Try-catch exception handling.
//! - `execute_parallel`: Parallel execution (sequential branches with merge).
//!
//! # Design Principles
//!
//! These primitives are control-flow constructs that operate deterministically:
//! - `try_catch` executes the `try` branch; if it fails, executes the `catch` branch
//!   and records the error in a user-specified state field.
//! - `parallel` executes branches sequentially (not truly parallel), collecting
//!   results and merging them according to the configured strategy.
//!
//! # Determinism Guarantee
//!
//! Both primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - Branch execution order is deterministic (sequential).
//! - Merge strategies are deterministic algorithms.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `try_catch` control flow | ✅ L1 deterministic | Pure branching |
//! | `parallel` sequential execution | ✅ L1 deterministic | Fixed branch order |
//! | `last_wins` merge | ✅ L1 deterministic | Last branch wins |
//! | `force` merge | ✅ L1 deterministic | Set union with provenance |
//! | `fail_fast` error strategy | ✅ L1 deterministic | Stop on first error |
//! | `continue` error strategy | ✅ L1 deterministic | Skip failed branches |
//! | `__parallel_provenance__` | ✅ L1 deterministic | Branch index recording |
//! | Error message serialization | ✅ L1 deterministic | `Display` formatting |
//!
//! # Cross-Language Note (L4)
//!
//! These primitives are Rust-only constructs; there is no cross-language equivalent.
//! However, the control flow semantics are defined by the `try`/`catch` branches
//! and the branch list, which are JSON-serializable.

use crate::control::dispatch::resolve_path;
use crate::error::EvoRuleError;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register error handling primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("execute_try_catch", exec_execute_try_catch);
    reg.register("execute_parallel", exec_execute_parallel);
}

/// Try-catch exception handling — executes the try branch, executes the catch branch on failure.
///
/// # Parameters
/// - `try`: The instruction to attempt (resolved via `$ref`).
/// - `catch` (optional): The instruction to execute if `try` fails (resolved via `$ref`).
/// - `error_attr` (optional): State field to store the error message (defaults to `"__error__"`).
///
/// # Behavior
/// - If `try` succeeds, returns the resulting state.
/// - If `try` fails:
///   - Writes the error message to `error_attr`.
///   - If `catch` is provided, executes it and returns the result.
///   - If `catch` fails, returns the state with the error (does not propagate).
/// - If `try` is missing, returns a clone of the original state.
pub(crate) fn exec_execute_try_catch(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let try_raw = match instruction.params.get("try") {
        Some(v) => v.clone(),
        None => return Ok(state.clone()),
    };
    let try_val = resolve_path(state, &try_raw);

    let catch_raw = instruction.params.get("catch").cloned();
    let catch_val = catch_raw.as_ref().map(|v| resolve_path(state, v));

    let error_attr = instruction
        .params
        .get("error_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "__error__".to_string());

    let try_instr = GenericInstruction::from_value(&try_val)?;

    match reg.execute(state, &try_instr) {
        Ok(result) => Ok(result),
        Err(e) => {
            let state_with_error = state.set_path(&error_attr, Value::string(format!("{e}")));

            if let Some(catch_v) = catch_val {
                let catch_instr = GenericInstruction::from_value(&catch_v)?;
                match reg.execute(&state_with_error, &catch_instr) {
                    Ok(result) => Ok(result),
                    Err(_) => Ok(state_with_error),
                }
            } else {
                Ok(state_with_error)
            }
        }
    }
}

/// Parallel execution — executes multiple branches sequentially and merges the results.
///
/// # Parameters
/// - `branches`: A list of instructions to execute (resolved via `$ref`).
/// - `merge` (optional): Merge strategy — `"last_wins"` (default) or `"force"`.
/// - `error_strategy` (optional): Error strategy — `"fail_fast"` (default) or `"continue"`.
///
/// # Merge Strategies
/// - `last_wins`: Returns the state produced by the last successful branch.
/// - `force`: Merges all modifications from all branches. Conflicts are resolved
///   by later branches overwriting earlier ones. Records field provenance in
///   `__parallel_provenance__`.
///
/// # Error Strategies
/// - `fail_fast`: Stops execution on the first error and returns `Err`.
/// - `continue`: Skips failed branches and continues with subsequent branches.
///
/// # Determinism
/// - Branches are executed in the order they appear in the list.
/// - `last_wins` returns the last successful branch's state.
/// - `force` merges modifications in branch order (later branches win conflicts).
pub(crate) fn exec_execute_parallel(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let branches_val = match instruction.params.get("branches") {
        Some(v) => resolve_path(state, v),
        None => return Ok(state.clone()),
    };

    let merge_strategy = instruction
        .params
        .get("merge")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "last_wins".to_string());

    let error_strategy = instruction
        .params
        .get("error_strategy")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "fail_fast".to_string());

    let branches = match branches_val {
        Value::List(ref v) => v.clone(),
        _ => return Ok(state.clone()),
    };

    let mut results: Vec<State> = Vec::new();
    let mut last_error: Option<EvoRuleError> = None;

    for branch_val in &branches {
        let branch_instr = match GenericInstruction::from_value(branch_val) {
            Ok(i) => i,
            Err(e) => {
                last_error = Some(e);
                if error_strategy == "fail_fast" {
                    break;
                }
                continue;
            }
        };

        match reg.execute(state, &branch_instr) {
            Ok(result) => results.push(result),
            Err(e) => {
                last_error = Some(e);
                if error_strategy == "fail_fast" {
                    break;
                }
            }
        }
    }

    if results.is_empty() {
        if let Some(e) = last_error {
            return Err(e);
        }
        return Ok(state.clone());
    }

    match merge_strategy.as_str() {
        "force" => {
            let system_fields = [
                "__exec__",
                "__universe_rules__",
                "__domain_result__",
                "__audit_chain",
                "__audit_trace",
                "__parallel_provenance__",
            ];
            let mut merged = state.clone();
            let mut provenance = im::HashMap::new();

            for (branch_idx, result) in results.iter().enumerate() {
                for (k, v) in result.data().iter() {
                    if system_fields.contains(&k.as_str()) {
                        continue;
                    }
                    let initial_val = state.get(k);
                    if initial_val != Some(v) {
                        merged = merged.set(k, v.clone());
                        // P2-2: Record field source branch
                        provenance.insert(
                            k.clone(),
                            Value::Object(im::hashmap! {
                                "branch".to_string() => Value::Integer(branch_idx as i64),
                                "total_branches".to_string() => Value::Integer(results.len() as i64),
                            }),
                        );
                    }
                }
            }

            // Write source provenance to __parallel_provenance__
            if !provenance.is_empty() {
                merged = merged.set("__parallel_provenance__", Value::Object(provenance));
            }

            Ok(merged)
        }
        // Default: last result wins (results is non-empty, checked above)
        _ => {
            // results.into_iter().last() is guaranteed Some because results.is_empty() was checked above.
            // Using ok_or to satisfy clippy::unwrap_used without changing behavior.
            Ok(results.into_iter().last().ok_or_else(|| {
                crate::error::invalid_config("parallel results unexpectedly empty")
            })?)
        }
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_reg() -> InstructionRegistry {
        // Requires a full primitive registry because execute_try_catch/execute_parallel
        // dispatch branch instructions via reg.execute()
        crate::primitive::make_test_registry()
    }

    fn make_state() -> State {
        State::empty()
            .set("x", Value::Integer(10))
            .set("y", Value::Integer(20))
    }

    // ─── execute_try_catch ───────────────────────────────────────────────

    #[test]
    fn test_try_catch_try_succeeds_returns_result() {
        // Try branch succeeds → returns the try execution result
        let state = make_state();
        let try_instr = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("value".to_string(), Value::Integer(99));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("try".to_string(), try_instr.to_value());
            p.insert("error_attr".to_string(), Value::string("err"));
            p
        };
        let instr = GenericInstruction::new("execute_try_catch", params);

        let result = exec_execute_try_catch(&make_reg(), &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_try_catch_try_fails_executes_catch() {
        // Try branch fails → executes the catch branch, result is modified by catch
        let state = make_state();
        // try: execute an instruction that will fail (invalid compute operation)
        let try_instr = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid_op"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        // catch: set y = 999
        let catch_instr = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("y"));
            p.insert("value".to_string(), Value::Integer(999));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("try".to_string(), try_instr.to_value());
            p.insert("catch".to_string(), catch_instr.to_value());
            p.insert("error_attr".to_string(), Value::string("err"));
            p
        };
        let instr = GenericInstruction::new("execute_try_catch", params);

        let result = exec_execute_try_catch(&make_reg(), &state, &instr).unwrap();
        // catch executed, y set to 999
        assert_eq!(result.get("y"), Some(&Value::Integer(999)));
        // error_attr is set
        assert!(result.get("err").is_some());
    }

    #[test]
    fn test_try_catch_try_fails_no_catch_sets_error_attr() {
        // Try fails with no catch → only sets error_attr
        let state = make_state();
        let try_instr = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid_op"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("try".to_string(), try_instr.to_value());
            // No catch provided
            p.insert("error_attr".to_string(), Value::string("my_err"));
            p
        };
        let instr = GenericInstruction::new("execute_try_catch", params);

        let result = exec_execute_try_catch(&make_reg(), &state, &instr).unwrap();
        assert!(result.get("my_err").is_some());
        // x is unchanged
        assert_eq!(result.get("x"), Some(&Value::Integer(10)));
    }

    #[test]
    fn test_try_catch_missing_try_returns_clone() {
        // No try parameter provided → returns a clone of the original state
        let state = make_state();
        let params = HashMap::new();
        let instr = GenericInstruction::new("execute_try_catch", params);

        let result = exec_execute_try_catch(&make_reg(), &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(10)));
    }

    #[test]
    fn test_try_catch_catch_fails_returns_state_with_error() {
        // Both try and catch fail → returns the state with error_attr set
        let state = make_state();
        let bad_instr = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid_op"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("try".to_string(), bad_instr.to_value());
            p.insert("catch".to_string(), bad_instr.to_value());
            p.insert("error_attr".to_string(), Value::string("err"));
            p
        };
        let instr = GenericInstruction::new("execute_try_catch", params);

        let result = exec_execute_try_catch(&make_reg(), &state, &instr).unwrap();
        // Returns state_with_error (catch failure also returns it)
        assert!(result.get("err").is_some());
    }

    // ─── execute_parallel ────────────────────────────────────────────────

    #[test]
    fn test_parallel_single_branch_last_wins() {
        // Single branch, last_wins strategy → returns that branch's result
        let state = make_state();
        let branch = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("value".to_string(), Value::Integer(55));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("branches".to_string(), Value::list(vec![branch.to_value()]));
            p.insert("merge".to_string(), Value::string("last_wins"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(55)));
    }

    #[test]
    fn test_parallel_multiple_branches_last_wins() {
        // Multiple branches, last_wins → returns the last branch's result
        let state = make_state();
        let branch0 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let branch1 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("value".to_string(), Value::Integer(2));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "branches".to_string(),
                Value::list(vec![branch0.to_value(), branch1.to_value()]),
            );
            p.insert("merge".to_string(), Value::string("last_wins"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(2)));
    }

    #[test]
    fn test_parallel_multiple_branches_force_merges_all() {
        // Multiple branches, force merge → all branch modifications are merged
        let state = make_state();
        let branch0 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let branch1 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("y"));
            p.insert("value".to_string(), Value::Integer(2));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "branches".to_string(),
                Value::list(vec![branch0.to_value(), branch1.to_value()]),
            );
            p.insert("merge".to_string(), Value::string("force"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        // Both x and y were modified
        assert_eq!(result.get("x"), Some(&Value::Integer(1)));
        assert_eq!(result.get("y"), Some(&Value::Integer(2)));
        // __parallel_provenance__ is written
        let prov = result.get("__parallel_provenance__");
        assert!(
            prov.is_some(),
            "force merge should write __parallel_provenance__"
        );
    }

    #[test]
    fn test_parallel_provenance_records_branch_index() {
        // With force merge, __parallel_provenance__ records which branch each field came from
        let state = State::empty();
        let branch0 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("a"));
            p.insert("value".to_string(), Value::Integer(10));
            p
        });
        let branch1 = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("b"));
            p.insert("value".to_string(), Value::Integer(20));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "branches".to_string(),
                Value::list(vec![branch0.to_value(), branch1.to_value()]),
            );
            p.insert("merge".to_string(), Value::string("force"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        let prov = result.get("__parallel_provenance__").unwrap();
        // Verify provenance structure
        assert!(prov.as_object().is_some());
        let prov_obj = prov.as_object().unwrap();
        assert!(prov_obj.get("a").is_some());
        assert!(prov_obj.get("b").is_some());
    }

    #[test]
    fn test_parallel_fail_fast_stops_on_first_error() {
        // fail_fast strategy: stops execution on the first branch error
        let state = make_state();
        let bad_branch = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let good_branch = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("y"));
            p.insert("value".to_string(), Value::Integer(999));
            p
        });
        let params = {
            let mut p = HashMap::new();
            // Bad branch first, fail_fast will stop here
            p.insert(
                "branches".to_string(),
                Value::list(vec![bad_branch.to_value(), good_branch.to_value()]),
            );
            p.insert("error_strategy".to_string(), Value::string("fail_fast"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr);
        // fail_fast returns Err on the first error
        assert!(result.is_err());
    }

    #[test]
    fn test_parallel_continue_skips_bad_branch() {
        // continue strategy: skips failed branches and continues with subsequent branches
        let state = make_state();
        let bad_branch = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let good_branch = GenericInstruction::new("state_set", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("y"));
            p.insert("value".to_string(), Value::Integer(777));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "branches".to_string(),
                Value::list(vec![bad_branch.to_value(), good_branch.to_value()]),
            );
            p.insert("error_strategy".to_string(), Value::string("continue"));
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        // The good branch still executed
        assert_eq!(result.get("y"), Some(&Value::Integer(777)));
    }

    #[test]
    fn test_parallel_empty_branches_returns_original() {
        // Empty branch list → returns the original state
        let state = make_state();
        let params = {
            let mut p = HashMap::new();
            p.insert("branches".to_string(), Value::empty_list());
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &state, &instr).unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(10)));
    }

    #[test]
    fn test_parallel_all_branches_fail_returns_error() {
        // All branches fail → returns the last error
        let bad_branch = GenericInstruction::new("state_compute", {
            let mut p = HashMap::new();
            p.insert("attr".to_string(), Value::string("x"));
            p.insert("operation".to_string(), Value::string("invalid"));
            p.insert("value".to_string(), Value::Integer(1));
            p
        });
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "branches".to_string(),
                Value::list(vec![bad_branch.to_value(), bad_branch.to_value()]),
            );
            p
        };
        let instr = GenericInstruction::new("execute_parallel", params);

        let result = exec_execute_parallel(&make_reg(), &State::empty(), &instr);
        assert!(result.is_err());
    }
}
