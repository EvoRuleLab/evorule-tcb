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

//! Queue primitives — Instruction queue operations.
//!
//! # Core Functions
//!
//! - `advance_instruction`: Instruction advancement + termination detection.
//! - `push_instruction`: Single instruction enqueue.
//! - `push_instruction_sequence`: Batch enqueue (atomic validation).
//! - `instruction_sequence`: Sequential execution (direct execution, no enqueue).
//!
//! # Design Principles
//!
//! ## `advance_instruction`: The Core of v2 Drain Logic
//!
//! This primitive implements the queue advancement logic used by `while_loop`'s
//! `drain_queue` function. It handles:
//! - Non-empty queue: Pop the front instruction, set as current.
//! - Empty queue: Inject `default_instruction` (typically `noop`) and check
//!   `termination_domain` to determine if the loop should terminate.
//!
//! This design aligns precisely with the v2 self-driving model documented in
//! `while_loop_self_driving_model.md` §4.4.
//!
//! ## Atomicity Guarantee (FLOW-04)
//!
//! `push_instruction_sequence` validates **all** instructions before enqueuing
//! any of them. If any instruction is invalid, the entire operation fails and
//! the queue remains unchanged. This prevents partial enqueue that could lead
//! to state inconsistency.
//!
//! # Determinism Guarantee
//!
//! All queue primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - Queue operations are pure data transformations.
//! - Termination detection is a pure domain evaluation.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `advance_instruction` queue pop | ✅ L1 deterministic | `im::Vector::split_at` |
//! | `advance_instruction` termination check | ✅ L1 deterministic | `Domain::contains` |
//! | `push_instruction` enqueue | ✅ L1 deterministic | `im::Vector::push_back` |
//! | `push_instruction_sequence` batch enqueue | ✅ L1 deterministic | Atomic validation |
//! | `instruction_sequence` sequential execution | ✅ L1 deterministic | Pure sequential |
//! | `instruction_sequence` missing type skip | ✅ L1 deterministic | Deterministic skip |
//!
//! # Cross-Language Note (L4)
//!
//! These primitives are Rust-only constructs; there is no cross-language equivalent.
//! The queue semantics are defined by the TCB and are not language-agnostic.

use crate::domain::Domain;
use crate::error::{type_error, EvoRuleError};
use crate::exec_ctl_ctx::ExecCtlCtx;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register queue primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("advance_instruction", exec_advance_instruction);
    reg.register("push_instruction", exec_push_instruction);
    reg.register("push_instruction_sequence", exec_push_instruction_sequence);
    reg.register("instruction_sequence", exec_instruction_sequence);
}

/// Get the current execution queue.
pub(crate) fn get_queue(state: &State) -> im::Vector<Value> {
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

/// Advance the instruction pointer — pop the next instruction from the queue.
///
/// # Behavior
///
/// Aligned with v2 self-driving model:
/// - Queue non-empty: pop the front instruction, set as current instruction.
/// - Queue empty: inject `default_instruction` (defaults to `noop`).
/// - After injecting `default_instruction`, immediately check `termination_domain`.
/// - If `termination_domain` is satisfied, set `__running = false`.
///
/// # Parameters
///
/// This primitive takes no parameters from the instruction — it operates
/// entirely on the `__exec__` state context.
///
/// # Termination Detection
///
/// The termination check is performed **after** `default_instruction` is injected,
/// because this is the only moment when the queue is known to be empty and the
/// instruction type is deterministic (typically `noop`). This matches the design
/// in `while_loop_self_driving_model.md` §4.4.
pub(crate) fn exec_advance_instruction(
    _reg: &InstructionRegistry,
    state: &State,
    _instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let queue = get_queue(state);

    if queue.is_empty() {
        // Queue empty → inject default_instruction
        let default_instr = state
            .get("__exec__")
            .and_then(|v| v.get("default_instruction"))
            .cloned()
            .unwrap_or_else(|| {
                Value::Object(im::hashmap! {
                    "type".to_string() => Value::string("noop"),
                    "params".to_string() => Value::empty_object(),
                })
            });

        let exec_ctx = state.update_exec_field("instruction", default_instr);

        // After injecting default_instruction, immediately check termination_domain
        let should_terminate = state
            .get("__exec__")
            .and_then(|v| v.get("termination_domain"))
            .and_then(|td| Domain::from_value(td).ok())
            .is_some_and(|domain| domain.contains(&exec_ctx));

        if should_terminate {
            Ok(exec_ctx.update_exec_field("__running", Value::Bool(false)))
        } else {
            Ok(exec_ctx)
        }
    } else {
        let (first, remaining) = queue.split_at(1);
        let next_instr = first[0].clone();
        let exec_ctx = state.update_exec_field("queue", Value::List(remaining));
        Ok(exec_ctx.update_exec_field("instruction", next_instr))
    }
}

/// Enqueue a single instruction.
///
/// # Parameters
/// - `instruction`: The instruction to enqueue (resolved via `$ref`).
///
/// # Startup Validation
///
/// The enqueued instruction type must be registered in the registry or cases table.
/// This validation is skipped when `__exec__.dispatch_cases` is absent (test environments).
///
/// # Behavior
/// - Appends the instruction to the end of the queue.
/// - Does not execute the instruction immediately.
pub(crate) fn exec_push_instruction(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let new_instr = instruction
        .params
        .get("instruction")
        .cloned()
        .unwrap_or(Value::empty_object());

    // Validate that the instruction type is dispatchable
    super::validate_instruction_type(reg, state, &new_instr, "push_instruction")?;

    let mut queue = get_queue(state);
    queue.push_back(new_instr);
    Ok(state.update_exec_field("queue", Value::List(queue)))
}

/// Enqueue a sequence of instructions.
///
/// # Parameters
/// - `instructions`: List of instructions to enqueue (resolved via `$ref`).
///   If a single non-List value is provided, it is treated as a single instruction.
///
/// # Startup Validation
///
/// Every enqueued instruction type must be registered in the registry or cases table.
///
/// # Atomicity Guarantee (FLOW-04)
///
/// All validations are performed **before** any enqueue operation. If any instruction
/// is invalid, the entire operation fails and the queue remains unchanged.
/// This prevents partial enqueue that could lead to state inconsistency.
pub(crate) fn exec_push_instruction_sequence(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let instructions = instruction
        .params
        .get("instructions")
        .cloned()
        .unwrap_or(Value::empty_list());

    // ── FLOW-04 fix: Unified validation logic ──
    // Collect all instructions (including single non-List values) into a Vec,
    // validate them all first, then batch enqueue them all — preventing partial
    // enqueue that would cause state inconsistency.
    let instructions_to_validate: Vec<Value> = match &instructions {
        Value::List(v) => v.iter().cloned().collect(),
        _ => vec![instructions.clone()], // Non-List value treated as a single instruction
    };

    // Validate every instruction type (all before any enqueue)
    for (i, item) in instructions_to_validate.iter().enumerate() {
        super::validate_instruction_type(
            reg,
            state,
            item,
            &format!("push_instruction_sequence[{i}]"),
        )?;
    }

    // After all validations pass, perform the enqueue
    let mut queue = get_queue(state);
    queue.extend(instructions_to_validate);
    Ok(state.update_exec_field("queue", Value::List(queue)))
}

/// Execute a sequence of instructions sequentially.
///
/// # Parameters
/// - `instructions`: List of instructions to execute (resolved via `$ref`).
///
/// # Behavior
/// - Executes multiple instructions directly (rather than enqueuing them).
/// - The output State of each instruction serves as the input for the next.
/// - This is the core primitive for JSON rule orchestration, implementing the
///   `instruction_sequence` transform.
///
/// # Fail-Fast
/// - If any child instruction fails, the sequence stops and returns the error.
/// - Instructions missing the `type` field are silently skipped.
///
/// # Example
/// ```json
/// {
///   "type": "instruction_sequence",
///   "params": {
///     "instructions": [
///       { "type": "state_set", "params": { "attr": "x", "value": 1 } },
///       { "type": "state_set", "params": { "attr": "y", "value": 2 } }
///     ]
///   }
/// }
/// ```
pub(crate) fn exec_instruction_sequence(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let instructions = instruction
        .params
        .get("instructions")
        .cloned()
        .unwrap_or(Value::empty_list());

    let instr_list = match &instructions {
        Value::List(v) => v,
        _ => return Err(type_error("List", instructions.type_name())),
    };

    let mut current_state = state.clone();

    for item in instr_list {
        let instr_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if instr_type.is_empty() {
            // instruction missing type field, skipping
            continue;
        }

        // Construct GenericInstruction from Value
        let child_instr = GenericInstruction::from_value(item)?;

        // Execute the child instruction
        match reg.execute(&current_state, &child_instr, ctx) {
            Ok(new_state) => {
                current_state = new_state;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(current_state)
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
    fn test_advance_instruction() {
        let reg = InstructionRegistry::new();

        let exec = Value::from(im::HashMap::from(vec![
            (
                "instruction".to_string(),
                Value::from(im::HashMap::from(vec![(
                    "type".to_string(),
                    Value::string("current"),
                )])),
            ),
            (
                "queue".to_string(),
                Value::list(vec![Value::from(im::HashMap::from(vec![(
                    "type".to_string(),
                    Value::string("next"),
                )]))]),
            ),
            ("__running".to_string(), Value::Bool(true)),
        ]));
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        let new_exec = result
            .get("__exec__")
            .cloned()
            .unwrap_or(Value::empty_object());
        let new_instr = new_exec
            .get("instruction")
            .cloned()
            .unwrap_or(Value::empty_object());
        assert_eq!(new_instr.get("type"), Some(&Value::string("next")));

        let queue = new_exec
            .get("queue")
            .cloned()
            .unwrap_or(Value::empty_list());
        assert!(queue.is_empty());
    }

    #[test]
    fn test_push_instruction() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::HashMap::from(vec![
            ("queue".to_string(), Value::empty_list()),
            ("__running".to_string(), Value::Bool(true)),
        ]));
        let state = State::empty().set("__exec__", exec);

        let mut params = HashMap::new();
        params.insert(
            "instruction".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("test_instr"),
            )])),
        );
        let instr = GenericInstruction::new("push_instruction", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap_or(Value::empty_list());
        match queue {
            Value::List(v) => assert_eq!(v.len(), 1),
            _ => panic!("queue should be a list"),
        }
    }

    // ══════════════════════════════════════════════
    // Additional queue_ops tests
    // ══════════════════════════════════════════════

    /// Test advance_instruction: empty queue injects default_instruction (noop)
    #[test]
    fn test_advance_instruction_empty_queue_injects_noop() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        let new_instr = result
            .get("__exec__")
            .and_then(|v| v.get("instruction"))
            .cloned()
            .unwrap();
        // Verify noop default instruction was injected
        assert_eq!(new_instr.get("type"), Some(&Value::string("noop")));
    }

    /// Test advance_instruction: empty queue + termination_domain satisfied → terminate
    #[test]
    fn test_advance_instruction_with_termination_domain_satisfied() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            // termination_domain: checks __exec__.__running eq true → should terminate
            "termination_domain".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.__running"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::Bool(true),
            }),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        // Verify __running was set to false
        let running = result
            .get("__exec__")
            .and_then(|v| v.get("__running"))
            .cloned();
        assert_eq!(running, Some(Value::Bool(false)));
    }

    /// Test advance_instruction: empty queue + termination_domain not satisfied → continue
    #[test]
    fn test_advance_instruction_with_termination_domain_not_satisfied() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            // termination_domain: checks __exec__.__running eq false, but it's true → not satisfied
            "termination_domain".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.__running"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::Bool(false),
            }),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        // Verify __running remains true (not terminated)
        let running = result
            .get("__exec__")
            .and_then(|v| v.get("__running"))
            .cloned();
        assert_eq!(running, Some(Value::Bool(true)));
    }

    /// Test push_instruction_sequence: batch enqueue multiple instructions
    #[test]
    fn test_push_instruction_sequence_multiple_instructions() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("instr_a"),
            }),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("instr_b"),
            }),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("instr_c"),
            }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("push_instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 3),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test push_instruction_sequence: single non-List instruction can also be enqueued
    #[test]
    fn test_push_instruction_sequence_single_non_list() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        // Pass a single instruction (not a list)
        let single_instr = Value::from(im::hashmap! {
            "type".to_string() => Value::string("single"),
        });
        let params = std::collections::HashMap::from([("instructions".to_string(), single_instr)]);
        let instr = GenericInstruction::new("push_instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 1),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test instruction_sequence: chain execution with state passing
    #[test]
    fn test_instruction_sequence_executes_chain() {
        let mut reg = InstructionRegistry::new();
        reg.register("state_set", crate::primitive::state_ops::exec_state_set);

        let exec = Value::from(im::hashmap! {
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("state_set"),
                "params".to_string() => Value::from(im::hashmap! {
                    "attr".to_string() => Value::string("a"),
                    "value".to_string() => Value::Integer(1),
                }),
            }),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("state_set"),
                "params".to_string() => Value::from(im::hashmap! {
                    "attr".to_string() => Value::string("b"),
                    "value".to_string() => Value::Integer(2),
                }),
            }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();

        // Verify both instructions executed; state contains a and b
        assert_eq!(result.get("a"), Some(&Value::Integer(1)));
        assert_eq!(result.get("b"), Some(&Value::Integer(2)));
    }

    /// Test instruction_sequence: child instruction failure fails fast with error
    #[test]
    fn test_instruction_sequence_fail_fast_on_error() {
        let reg = InstructionRegistry::new();
        // No instructions registered, causing execution to fail
        let exec = Value::from(im::hashmap! {
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("unregistered_instruction"),
                "params".to_string() => Value::empty_object(),
            }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_instruction_sequence(&reg, &state, &instr, &mut ctx);
        // Second instruction is unregistered, should return an error
        assert!(result.is_err());
    }

    /// Test get_queue: returns empty vector when no queue exists
    #[test]
    fn test_get_queue_empty_when_no_exec() {
        let state = State::empty();
        let queue = get_queue(&state);
        assert!(queue.is_empty());
    }

    /// Test get_queue: queue field exists but is not a list → returns empty vector
    #[test]
    fn test_get_queue_returns_empty_for_non_list() {
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::string("not_a_list"),
        });
        let state = State::empty().set("__exec__", exec);
        let queue = get_queue(&state);
        assert!(queue.is_empty());
    }

    /// Test get_queue: empty list queue returns empty vector
    #[test]
    fn test_get_queue_empty_list_returns_empty() {
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
        });
        let state = State::empty().set("__exec__", exec);
        let queue = get_queue(&state);
        assert!(queue.is_empty());
    }

    /// Test advance_instruction: non-empty queue pops front, remaining length decreases by 1
    #[test]
    fn test_advance_instruction_queue_length_decrements() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("current"),
            }),
            "queue".to_string() => Value::list(vec![
                Value::from(im::hashmap! { "type".to_string() => Value::string("next1") }),
                Value::from(im::hashmap! { "type".to_string() => Value::string("next2") }),
                Value::from(im::hashmap! { "type".to_string() => Value::string("next3") }),
            ]),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 2), // 3 → 2
            _ => panic!("queue should be a list"),
        }
    }

    /// Test advance_instruction: popped instruction content is correct
    #[test]
    fn test_advance_instruction_pops_correct_instruction() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("current"),
            }),
            "queue".to_string() => Value::list(vec![
                Value::from(im::hashmap! {
                    "type".to_string() => Value::string("first"),
                    "params".to_string() => Value::from(im::hashmap! {
                        "key".to_string() => Value::string("value"),
                    }),
                }),
            ]),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        let new_instr = result
            .get("__exec__")
            .and_then(|v| v.get("instruction"))
            .cloned()
            .unwrap();
        assert_eq!(new_instr.get("type"), Some(&Value::string("first")));
        assert_eq!(
            new_instr.get("params").and_then(|p| p.get("key")),
            Some(&Value::string("value"))
        );
    }

    /// Test push_instruction_sequence: empty list does not error
    #[test]
    fn test_push_instruction_sequence_empty_list() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let params =
            std::collections::HashMap::from([("instructions".to_string(), Value::empty_list())]);
        let instr = GenericInstruction::new("push_instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert!(v.is_empty()),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test instruction_sequence: empty list returns original state
    #[test]
    fn test_instruction_sequence_empty_list() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let params =
            std::collections::HashMap::from([("instructions".to_string(), Value::empty_list())]);
        let instr = GenericInstruction::new("instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        // Empty list should return a clone of the original state (including __exec__)
        assert_eq!(result.get("__exec__"), state.get("__exec__"));
    }

    /// Test instruction_sequence: skips instructions missing the type field
    #[test]
    fn test_instruction_sequence_skips_missing_type() {
        let mut reg = InstructionRegistry::new();
        reg.register("state_set", crate::primitive::state_ops::exec_state_set);

        let exec = Value::from(im::hashmap! {
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("state_set"),
                "params".to_string() => Value::from(im::hashmap! {
                    "attr".to_string() => Value::string("a"),
                    "value".to_string() => Value::Integer(1),
                }),
            }),
            Value::from(im::hashmap! {
                // missing type field
                "params".to_string() => Value::empty_object(),
            }),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("state_set"),
                "params".to_string() => Value::from(im::hashmap! {
                    "attr".to_string() => Value::string("b"),
                    "value".to_string() => Value::Integer(2),
                }),
            }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        // First and third should execute, second is skipped
        assert_eq!(result.get("a"), Some(&Value::Integer(1)));
        assert_eq!(result.get("b"), Some(&Value::Integer(2)));
    }

    /// Test push_instruction: validation failure returns error
    #[test]
    fn test_push_instruction_validation_failure() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "dispatch_cases".to_string() => Value::empty_object(),
        });
        let state = State::empty().set("__exec__", exec);

        let params = std::collections::HashMap::from([(
            "instruction".to_string(),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("unregistered_type"),
            }),
        )]);
        let instr = GenericInstruction::new("push_instruction", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction(&reg, &state, &instr, &mut ctx);
        assert!(result.is_err());
    }

    /// Test push_instruction_sequence: partial validation failure causes entire operation to fail
    #[test]
    fn test_push_instruction_sequence_atomic_on_validation_failure() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::list(vec![
                Value::from(im::hashmap! { "type".to_string() => Value::string("existing") }),
            ]),
            "__running".to_string() => Value::Bool(true),
            "dispatch_cases".to_string() => Value::empty_object(),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! { "type".to_string() => Value::string("valid_type") }),
            Value::from(im::hashmap! { "type".to_string() => Value::string("unregistered_type") }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("push_instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction_sequence(&reg, &state, &instr, &mut ctx);
        assert!(result.is_err());

        // Verify the queue was not modified
        let queue = state
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 1),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test push_instruction_sequence: all validations pass before enqueue
    #[test]
    fn test_push_instruction_sequence_all_valid() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let instructions = Value::list(vec![
            Value::from(im::hashmap! { "type".to_string() => Value::string("noop") }),
            Value::from(im::hashmap! { "type".to_string() => Value::string("noop") }),
        ]);
        let params = std::collections::HashMap::from([("instructions".to_string(), instructions)]);
        let instr = GenericInstruction::new("push_instruction_sequence", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction_sequence(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 2),
            _ => panic!("queue should be a list"),
        }
    }

    /// Test advance_instruction: custom default_instruction when queue is empty
    #[test]
    fn test_advance_instruction_custom_default_instruction() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::from(im::hashmap! {
                "type".to_string() => Value::string("custom_default"),
                "params".to_string() => Value::from(im::hashmap! {
                    "source".to_string() => Value::string("test"),
                }),
            }),
        });
        let state = State::empty().set("__exec__", exec);

        let instr = GenericInstruction::simple("advance_instruction");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_advance_instruction(&reg, &state, &instr, &mut ctx).unwrap();

        let new_instr = result
            .get("__exec__")
            .and_then(|v| v.get("instruction"))
            .cloned()
            .unwrap();
        assert_eq!(
            new_instr.get("type"),
            Some(&Value::string("custom_default"))
        );
    }

    /// Test push_instruction: successful enqueue increases queue length by 1
    #[test]
    fn test_push_instruction_increments_queue() {
        let reg = InstructionRegistry::new();
        let exec = Value::from(im::hashmap! {
            "queue".to_string() => Value::list(vec![
                Value::from(im::hashmap! { "type".to_string() => Value::string("existing") }),
            ]),
            "__running".to_string() => Value::Bool(true),
        });
        let state = State::empty().set("__exec__", exec);

        let params = std::collections::HashMap::from([(
            "instruction".to_string(),
            Value::from(im::hashmap! {
                "type".to_string() => Value::string("new_instr"),
            }),
        )]);
        let instr = GenericInstruction::new("push_instruction", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_push_instruction(&reg, &state, &instr, &mut ctx).unwrap();
        let queue = result
            .get("__exec__")
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap();
        match queue {
            Value::List(v) => assert_eq!(v.len(), 2),
            _ => panic!("queue should be a list"),
        }
    }
}
