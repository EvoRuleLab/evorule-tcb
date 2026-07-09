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

//! Control flow primitive — while loop.
//!
//! while_loop: Self-driven execution loop (core control flow)

use crate::domain::Domain;
use crate::error::EvoRuleError;
use crate::exec_ctl_ctx::ExecCtlCtx;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register control flow primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("while_loop", exec_while_loop);
}

/// While loop — self-driven execution loop.
///
/// TCB core loop (aligned with v2):
///   while condition:
///       exec body (dispatch + trace_step)
///       drain queue: meta-instructions execute immediately,
///                    business instructions become current then break
///
/// Meta-instruction types are read from `__exec__.meta_instruction_types`
/// (injected by evaluate() during initialization, sourced from eval_config.json).
///
/// Parameters:
///   - condition: condition (domain expression or boolean reference, defaults to __running)
///   - max_steps: maximum step limit (optional, defaults to 10000)
///   - body: loop body (instruction sequence to repeat)
pub fn exec_while_loop(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let condition = instruction.params.get("condition").cloned();
    let max_steps = instruction
        .params
        .get("max_steps")
        .and_then(|v| v.as_integer())
        .unwrap_or(10000)
        .min(100000) as usize;
    let body = instruction.params.get("body").cloned();

    // Read configuration from __exec__ (injected by evaluate())
    // [FIX #5] Cache meta_types to avoid recomputing on each drain iteration
    let meta_types = load_meta_types(state);
    let audit_on = load_audit_on(state);
    let drain_meta_trace = load_drain_meta_trace(state);

    let mut current_state = state.clone();
    let mut last_step: usize = 0;
    let mut condition_holds_at_exit = false;

    for step in 0..max_steps {
        last_step = step;

        // ── 1. Check condition ──
        let should_continue = check_condition(&current_state, &condition)?;
        if !should_continue {
            #[cfg(feature = "tracing")]
            log::trace!(
                "[while_loop] step={} condition not satisfied, exiting loop",
                step
            );
            condition_holds_at_exit = false;
            break;
        }

        #[cfg(feature = "tracing")]
        log::trace!(
            "[while_loop] step={} executing body, current instruction type='{}'",
            step,
            current_state
                .get("__exec__")
                .and_then(|v| v.get("instruction"))
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        );

        // ── 2. Execute body (dispatch + trace_step) ──
        match &body {
            Some(Value::List(instructions)) => {
                for instr_val in instructions.iter() {
                    let instr = GenericInstruction::from_value(instr_val)?;
                    current_state = reg.execute(&current_state, &instr, ctx)?;
                }
            }
            Some(body_instr) => {
                let instr = GenericInstruction::from_value(body_instr)?;
                current_state = reg.execute(&current_state, &instr, ctx)?;
            }
            None => {
                condition_holds_at_exit = false;
                break;
            }
        }

        // ── 3. Drain queue (aligned with v2) ──
        current_state = drain_queue(
            reg,
            &current_state,
            &meta_types,
            audit_on,
            drain_meta_trace,
            ctx,
        )?;

        // Check __running again after drain
        if !is_running(&current_state) {
            #[cfg(feature = "tracing")]
            log::trace!(
                "[while_loop] step={} __running=false after drain, exiting loop",
                step
            );
            condition_holds_at_exit = false;
            break;
        }

        condition_holds_at_exit = true;
    }

    // ── 4. Truncation detection ──
    // Reached max_steps with condition still true → silent truncation,
    // writes a flag for the upper layer to observe.
    let terminated_by_max_steps =
        last_step == max_steps - 1 && condition_holds_at_exit && is_running(&current_state);

    if terminated_by_max_steps {
        #[cfg(feature = "tracing")]
        log::warn!(
            "[while_loop] forcibly truncated at max_steps={}, condition still true \
             (possible infinite loop or rule logic error)",
            max_steps
        );
    } else {
        #[cfg(feature = "tracing")]
        log::trace!(
            "[while_loop] normal exit: last_step={}, condition_holds={}, running={}",
            last_step,
            condition_holds_at_exit,
            is_running(&current_state)
        );
    }

    // Explicitly write the flag to avoid dirty state residue from previous runs
    current_state = current_state.update_exec_field(
        "__terminated_by_max_steps",
        Value::Bool(terminated_by_max_steps),
    );

    Ok(current_state)
}

/// Drain the queue — v2's core drain logic.
///
/// Strategy:
///   - Queue has meta-instruction → execute directly, continue draining
///   - Queue has business instruction → set as current instruction + audit, break to outer dispatch
///   - Queue empty + current instruction is meta → break to outer dispatch (let dispatch handle it)
///   - Queue empty + current instruction is business → break to outer dispatch
///   - Queue empty + current instruction is noop (or empty) → execute advance_instruction + audit, break
///
/// Note: "meta" vs "business" is entirely data-driven via `meta_types` (read from __exec__).
///       `noop` is deliberately included in `meta_types` so it is handled correctly.
fn drain_queue(
    reg: &InstructionRegistry,
    state: &State,
    meta_types: &[String],
    audit_on: bool,
    drain_meta_trace: bool,
    ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let mut current_state = state.clone();

    loop {
        let exec_ctx = current_state.get("__exec__").cloned();
        let queue_val = exec_ctx
            .as_ref()
            .and_then(|v| v.get("queue"))
            .cloned()
            .unwrap_or(Value::empty_list());

        let queue = match &queue_val {
            Value::List(v) => v.clone(),
            _ => break,
        };

        if !queue.is_empty() {
            // Check __running flag
            if !is_running(&current_state) {
                #[cfg(feature = "tracing")]
                log::trace!("[drain] __running=false, stopping drain");
                break;
            }

            // Pop the front element
            let (first, remaining) = queue.split_at(1);
            let next_dict = first[0].clone();
            #[cfg(feature = "tracing")]
            let remaining_len = remaining.len();

            // Update the queue
            current_state = current_state.update_exec_field("queue", Value::List(remaining));

            let next_type = next_dict
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if meta_types.contains(&next_type) {
                // Meta-instruction: execute directly, continue draining
                #[cfg(feature = "tracing")]
                log::trace!(
                    "[drain] popped meta-instruction: type='{}', remaining queue length={}",
                    next_type,
                    remaining_len
                );
                current_state =
                    set_trace_source(&current_state, &format!("drain_meta:{}", next_type));
                let next_instr = GenericInstruction::from_value(&next_dict)?;
                current_state = reg.execute(&current_state, &next_instr, ctx)?;

                if audit_on && drain_meta_trace {
                    current_state = reg.execute(
                        &current_state,
                        &GenericInstruction::simple("trace_step"),
                        ctx,
                    )?;
                }
                // Continue draining
            } else {
                // Business instruction: set as current instruction + audit + break to dispatch
                #[cfg(feature = "tracing")]
                log::trace!(
                    "[drain] popped business instruction: type='{}', setting as current and breaking",
                    next_type
                );
                current_state = current_state.update_exec_field("instruction", next_dict);
                current_state = set_trace_source(&current_state, "queue_pop");
                if audit_on {
                    current_state = reg.execute(
                        &current_state,
                        &GenericInstruction::simple("trace_step"),
                        ctx,
                    )?;
                }
                break;
            }
        } else {
            // Queue empty → check current instruction
            let cur_type = exec_ctx
                .as_ref()
                .and_then(|v| v.get("instruction"))
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // If current instruction is a business instruction (not in meta_types),
            // break to dispatch. This includes cases where it's a non-noop business
            // instruction that was already set by a previous drain.
            if !cur_type.is_empty() && !meta_types.contains(&cur_type) {
                #[cfg(feature = "tracing")]
                log::trace!(
                    "[drain] queue empty, current instruction type='{}' is business, breaking to dispatch",
                    cur_type
                );
                if audit_on {
                    current_state = reg.execute(
                        &current_state,
                        &GenericInstruction::simple("trace_step"),
                        ctx,
                    )?;
                }
                break;
            }

            // Current instruction is meta (including noop) or empty → auto-advance + audit.
            // This will trigger advance_instruction, which checks termination_domain
            // and injects default_instruction (noop) when queue is empty, eventually
            // setting __running=false when termination condition is met.
            #[cfg(feature = "tracing")]
            log::trace!(
                "[drain] queue empty, current instruction type='{}', executing advance_instruction",
                cur_type
            );
            current_state = set_trace_source(&current_state, "advance");
            current_state = reg.execute(
                &current_state,
                &GenericInstruction::simple("advance_instruction"),
                ctx,
            )?;
            if audit_on {
                current_state = reg.execute(
                    &current_state,
                    &GenericInstruction::simple("trace_step"),
                    ctx,
                )?;
            }
            break;
        }
    }

    Ok(current_state)
}

/// Check the __running flag.
fn is_running(state: &State) -> bool {
    state
        .get("__exec__")
        .and_then(|v| v.get("__running"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Check the loop condition.
///
/// [FIX #2]: Resolve $ref references before evaluating truthy.
/// [P0 FIX]: Bind the inner value directly in the match arm instead of
/// calling `.as_ref().unwrap()`, eliminating runtime panic risk
/// (ER-600: no runtime panics in production).
fn check_condition(state: &State, condition: &Option<Value>) -> Result<bool, EvoRuleError> {
    match condition {
        None => Ok(is_running(state)),
        Some(cond @ Value::Object(m)) if m.contains_key("$ref") => {
            // Resolve $ref first, then evaluate truthy.
            // `cond` is bound directly by the match pattern — no unwrap needed.
            let resolved = crate::control::dispatch::resolve_refs(state, cond);
            Ok(resolved.truthy())
        }
        Some(cond @ Value::Object(m)) if m.contains_key("type") => {
            // Domain expression. `cond` is bound directly — no unwrap needed.
            let domain = Domain::from_value(cond)?;
            Ok(domain.contains(state))
        }
        Some(other) => Ok(other.truthy()),
    }
}

/// Read meta-instruction types from __exec__.
///
/// [FIX #5]: Caches meta_types to avoid recomputing on each drain iteration.
/// [FIX #6]: Fallback to empty list and log a warning instead of treating all
/// registered primitives as meta-instructions. This preserves the intended
/// separation of meta vs business instructions.
fn load_meta_types(state: &State) -> Vec<String> {
    match state
        .get("__exec__")
        .and_then(|v| v.get("meta_instruction_types"))
        .and_then(|v| v.as_list())
    {
        Some(list) => list
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        None => {
            #[cfg(feature = "tracing")]
            log::warn!(
                "load_meta_types: '__exec__.meta_instruction_types' not found or invalid; \
                 using empty list (only built-in fallbacks will be treated as meta)"
            );
            Vec::new()
        }
    }
}

/// Read the audit switch from __exec__.
fn load_audit_on(state: &State) -> bool {
    state
        .get("__exec__")
        .and_then(|v| v.get("audit_on"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the drain meta-trace switch from __exec__.
fn load_drain_meta_trace(state: &State) -> bool {
    state
        .get("__exec__")
        .and_then(|v| v.get("drain_meta_trace"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Set __trace_source (for audit tracing).
fn set_trace_source(state: &State, source: &str) -> State {
    state.update_exec_field("__trace_source", Value::string(source))
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_registry() -> InstructionRegistry {
        let mut reg = InstructionRegistry::new().with_default_context_ops();
        crate::primitive::register_all(&mut reg);
        crate::control::register_all(&mut reg);
        reg
    }

    fn make_exec_context() -> Value {
        Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("push_instruction"),
                Value::string("push_instruction_sequence"),
                Value::string("evaluate_domain"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(true),
            "drain_meta_trace".to_string() => Value::Bool(false),
        })
    }

    #[test]
    fn test_while_loop_with_dispatch_and_advance() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let dispatch_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("dispatch"),
            "params".to_string() => Value::Object(im::hashmap!{
                "key".to_string() => Value::Object(im::hashmap!{
                    "$ref".to_string() => Value::string("__exec__.instruction.type"),
                }),
                "cases".to_string() => Value::Object(im::hashmap!{
                    "increment".to_string() => Value::Object(im::hashmap!{
                        "type".to_string() => Value::string("set_context"),
                        "params".to_string() => Value::Object(im::hashmap!{
                            "transform".to_string() => Value::Object(im::hashmap!{
                                "attr".to_string() => Value::string("x"),
                                "operation".to_string() => Value::string("add"),
                                "value".to_string() => Value::Integer(1),
                            }),
                        }),
                    }),
                }),
                "default".to_string() => Value::Object(im::hashmap!{
                    "type".to_string() => Value::string("advance_instruction"),
                }),
            }),
        });

        let trace_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("trace_step"),
            "params".to_string() => Value::Object(im::hashmap!{
                "label".to_string() => Value::string("step"),
            }),
        });

        let mut params = HashMap::new();
        params.insert(
            "body".to_string(),
            Value::list(vec![dispatch_instr, trace_instr]),
        );
        params.insert("max_steps".to_string(), Value::Integer(20));

        let instr = GenericInstruction::new("while_loop", params);

        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();
        // increment → set_context(x+=1) → advance → queue empty → noop → advance → stop
        assert_eq!(result.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_while_loop_zero_iterations() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();
        let exec = make_exec_context();
        let state_with_exec = State::empty().set("__exec__", exec);
        let state_with_exec = state_with_exec.update_exec_field("__running", Value::Bool(false));
        let state = State::empty().set(
            "__exec__",
            state_with_exec.data().get("__exec__").cloned().unwrap(),
        );

        let mut params = HashMap::new();
        params.insert("body".to_string(), Value::empty_list());
        let instr = GenericInstruction::new("while_loop", params);

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();
        assert!(!is_running(&result));
        assert_eq!(
            result
                .get("__exec__")
                .and_then(|v| v.get("__terminated_by_max_steps"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "Zero-iteration scenario __terminated_by_max_steps should be false"
        );
    }

    // ══════════════════════════════════════════════════════════════
    // Truncation detection tests (fix for silent truncation)
    // ══════════════════════════════════════════════════════════════

    /// Construct a never-stopping dead-loop body: only noop, does not change __running.
    /// With a small max_steps, verify that the truncation flag is correctly set.
    #[test]
    fn test_while_loop_terminated_by_max_steps_dead_loop() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // Initial __running=true, body is noop (won't set __running to false)
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
        });
        let state = State::empty().set("__exec__", exec);

        // body=empty list + no condition (default checks __running=true)
        // → infinite loop, will be truncated by max_steps
        let mut params = HashMap::new();
        params.insert("body".to_string(), Value::empty_list());
        params.insert("max_steps".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("while_loop", params);

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();

        // Truncation flag should be true
        assert_eq!(
            result
                .get("__exec__")
                .and_then(|v| v.get("__terminated_by_max_steps"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "__terminated_by_max_steps should be true when max_steps reached and __running still true"
        );
        // __running should still be true (truncated, not exited normally)
        assert!(
            is_running(&result),
            "Truncated loop should still have __running=true"
        );
    }

    #[test]
    fn test_while_loop_normal_exit_clears_truncation_flag() {
        // Verify: normal exit explicitly sets __terminated_by_max_steps to false
        // Even if a dirty state (true) was left from a previous run, this normal exit
        // should override it to false.
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // Pre-write a dirty truncation flag (true) in __exec__
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(false), // terminate immediately
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("advance_instruction"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "__terminated_by_max_steps".to_string() => Value::Bool(true), // dirty state
        });
        let state = State::empty().set("__exec__", exec);

        let mut params = HashMap::new();
        params.insert("body".to_string(), Value::empty_list());
        params.insert("max_steps".to_string(), Value::Integer(10));
        let instr = GenericInstruction::new("while_loop", params);

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();

        // Normal exit (__running=false), dirty state should be cleared to false
        assert_eq!(
            result
                .get("__exec__")
                .and_then(|v| v.get("__terminated_by_max_steps"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "__terminated_by_max_steps should be explicitly set to false on normal exit, clearing dirty state"
        );
    }

    // ══════════════════════════════════════════════════════════════
    // while_loop self-driven model deep tests
    // Verify that after evaluate() does initialization + one reg.execute(while_loop),
    // the JSON rule-driven model takes over the entire execution flow.
    // ══════════════════════════════════════════════════════════════

    /// Construct the standard __exec__ context (simulating state after evaluate() initialization).
    /// Includes termination_domain: terminates when instruction type becomes noop.
    fn make_self_driving_exec(current_type: &str, current_params: Value) -> Value {
        Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string(current_type),
                "params".to_string() => current_params,
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("push_instruction"),
                Value::string("push_instruction_sequence"),
                Value::string("evaluate_domain"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(true),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        })
    }

    /// Construct the standard while_loop instruction (matching core_eval.json structure).
    fn make_core_eval_while_loop(
        cases: im::HashMap<String, Value>,
        default: Value,
    ) -> GenericInstruction {
        let dispatch_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("dispatch"),
            "params".to_string() => Value::Object(im::hashmap!{
                "key".to_string() => Value::Object(im::hashmap!{
                    "$ref".to_string() => Value::string("__exec__.instruction.type"),
                }),
                "cases".to_string() => Value::Object(cases),
                "default".to_string() => default,
            }),
        });
        let trace_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("trace_step"),
            "params".to_string() => Value::Object(im::hashmap!{
                "record_hash".to_string() => Value::Bool(true),
            }),
        });
        let mut params = HashMap::new();
        params.insert(
            "condition".to_string(),
            Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("__exec__.__running"),
            }),
        );
        params.insert("max_steps".to_string(), Value::Integer(100));
        params.insert(
            "body".to_string(),
            Value::list(vec![dispatch_instr, trace_instr]),
        );
        GenericInstruction::new("while_loop", params)
    }

    // ── Test 1: Single-instruction self-driven lifecycle ──
    // Verify: dispatch → case body → advance → termination
    #[test]
    fn test_self_driving_single_instruction_lifecycle() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let cases = im::hashmap! {
            "set".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("score"),
                        "operation".to_string() => Value::string("set"),
                        "value".to_string() => Value::Integer(100),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction is "set", score=0
        let exec = make_self_driving_exec("set", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("score", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: set → set_context(score=100) → advance → noop → advance → stop
        assert_eq!(result.get("score"), Some(&Value::Integer(100)));
        assert!(!is_running(&result), "should terminate after noop advance");
    }

    // ── Test 2: Multi-instruction queue self-driven ──
    // Verify: drain queue sets business instructions as current, while_loop processes them
    // in the next iteration via dispatch.
    #[test]
    fn test_self_driving_multi_instruction_queue() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // case "increment": x += 1
        let cases = im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction increment, queue has a second increment
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::list(vec![
                Value::Object(im::hashmap!{
                    "type".to_string() => Value::string("increment"),
                    "params".to_string() => Value::empty_object(),
                }),
            ]),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: both increments executed, x = 0 + 1 + 1 = 2
        assert_eq!(result.get("x"), Some(&Value::Integer(2)));
        assert!(!is_running(&result));
    }

    // ── Test 3: Meta-instructions in drain queue execute immediately ──
    // Verify: meta-instructions execute directly in drain without returning to while_loop
    #[test]
    fn test_self_driving_drain_meta_immediate_execution() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // case "set": set y=10
        let cases = im::hashmap! {
            "set".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("y"),
                        "operation".to_string() => Value::string("set"),
                        "value".to_string() => Value::Integer(10),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction set, queue has a set_context meta-instruction (set z=5)
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("set"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::list(vec![
                Value::Object(im::hashmap!{
                    "type".to_string() => Value::string("set_context"),
                    "params".to_string() => Value::Object(im::hashmap!{
                        "transform".to_string() => Value::Object(im::hashmap!{
                            "attr".to_string() => Value::string("z"),
                            "operation".to_string() => Value::string("set"),
                            "value".to_string() => Value::Integer(5),
                        }),
                    }),
                }),
            ]),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("y", Value::Integer(0))
            .set("z", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: case body sets y=10, drain meta-instruction set_context sets z=5
        assert_eq!(result.get("y"), Some(&Value::Integer(10)));
        assert_eq!(result.get("z"), Some(&Value::Integer(5)));
        assert!(!is_running(&result));
    }

    // ── Test 4: default branch pushes advance_instruction then terminates ──
    // Verify: unknown instruction → default(advance) → noop → termination_domain → stop
    #[test]
    fn test_self_driving_default_branch_termination() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let cases = im::hashmap! {};
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction unknown
        let exec = make_self_driving_exec("unknown", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(42));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: unknown → default(advance) → noop → advance → stop
        assert_eq!(
            result.get("x"),
            Some(&Value::Integer(42)),
            "state unchanged"
        );
        assert!(!is_running(&result), "should terminate");
    }

    // ── Test 5: termination_domain early termination ──
    // Verify: advance_instruction checks termination_domain, sets __running=false when matched
    #[test]
    fn test_self_driving_termination_domain() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let cases = im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction increment, termination_domain = InstructionDomain("noop")
        // → terminates when instruction becomes noop
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: increment → set_context(x+=1) → advance → noop → termination_domain matches → stop
        assert_eq!(result.get("x"), Some(&Value::Integer(1)));
        assert!(
            !is_running(&result),
            "should terminate when instruction becomes noop"
        );
    }

    // ── Test 6: Audit chain correctly built in self-driven model ──
    // Verify: with audit_on=true, both trace_step in body and trace_step in drain are recorded
    #[test]
    fn test_self_driving_audit_chain_construction() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let cases = im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // audit_on=true
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(true),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: audit chain exists and has records
        let chain_val = result.get("__audit_chain").expect("audit chain must exist");
        let chain = crate::audit::AuditChainState::from_value(chain_val)
            .expect("audit chain must be parseable");
        assert!(!chain.records.is_empty(), "audit chain must have records");

        // Verify: each record's HMAC is valid
        for record in &chain.records {
            assert!(
                record.verify(crate::audit::DEFAULT_HMAC_KEY),
                "HMAC must be valid for record {}",
                record.id
            );
        }

        // Verify: chain structure is intact (previous_hash links correctly)
        for i in 1..chain.records.len() {
            assert_eq!(
                chain.records[i].previous_hash,
                chain.records[i - 1].hash,
                "chain link at position {} must be valid",
                i
            );
        }

        assert!(!is_running(&result));
    }

    // ── Test 7: audit_on=false skips audit chain construction ──
    // Verify: trace_step checks audit_on, doesn't write audit records when disabled
    #[test]
    fn test_self_driving_audit_disabled() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let cases = im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // audit_on=false
        let exec = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("increment"),
                "params".to_string() => Value::empty_object(),
            }),
            "queue".to_string() => Value::empty_list(),
            "__running".to_string() => Value::Bool(true),
            "default_instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "meta_instruction_types".to_string() => Value::list(vec![
                Value::string("noop"),
                Value::string("set_context"),
                Value::string("advance_instruction"),
                Value::string("trace_step"),
                Value::string("dispatch"),
                Value::string("while_loop"),
            ]),
            "audit_on".to_string() => Value::Bool(false),
            "drain_meta_trace".to_string() => Value::Bool(false),
            "termination_domain".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string("__exec__.instruction.type"),
                "op".to_string() => Value::string("eq"),
                "value".to_string() => Value::string("noop"),
            }),
        });
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: functionality works but no audit chain
        assert_eq!(result.get("x"), Some(&Value::Integer(1)));
        assert!(
            result.get("__audit_chain").is_none(),
            "audit chain must NOT exist when audit disabled"
        );
    }

    // ── Test 8: max_steps safety valve ──
    // Verify: even if the condition is always true, max_steps prevents infinite loops
    #[test]
    fn test_self_driving_max_steps_safety() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // case "increment": x += 1, but the queue pushes another increment (simulating infinite loop)
        // Actually dispatch case pushes advance_instruction, advance → noop → stop.
        // So it won't infinite loop. But the max_steps limit should still be effective.
        let cases = im::hashmap! {
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let mut params = HashMap::new();
        params.insert(
            "condition".to_string(),
            Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("__exec__.__running"),
            }),
        );
        params.insert("max_steps".to_string(), Value::Integer(3)); // only allow 3 steps
        let dispatch_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("dispatch"),
            "params".to_string() => Value::Object(im::hashmap!{
                "key".to_string() => Value::Object(im::hashmap!{
                    "$ref".to_string() => Value::string("__exec__.instruction.type"),
                }),
                "cases".to_string() => Value::Object(cases),
                "default".to_string() => default,
            }),
        });
        let trace_instr = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("trace_step"),
            "params".to_string() => Value::Object(im::hashmap!{
                "record_hash".to_string() => Value::Bool(true),
            }),
        });
        params.insert(
            "body".to_string(),
            Value::list(vec![dispatch_instr, trace_instr]),
        );
        let while_loop = GenericInstruction::new("while_loop", params);

        let exec = make_self_driving_exec("increment", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: max_steps limits execution, but normal flow should terminate within 3 steps
        // increment → set_context → advance → noop → stop (1 iteration is enough)
        assert!(!is_running(&result), "should terminate within max_steps");
    }

    // ── Test 9: evaluate_domain branch pushes to queue ──
    // Verify: evaluate_domain's on_true/on_false are pushed to the queue, drain handles them
    #[test]
    fn test_self_driving_evaluate_domain_branch() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // case "check": evaluate_domain checks x > 0, on_true pushes increment
        let cases = im::hashmap! {
            "check".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("evaluate_domain"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "domain".to_string() => Value::Object(im::hashmap!{
                        "type".to_string() => Value::string("atom"),
                        "attribute".to_string() => Value::string("x"),
                        "op".to_string() => Value::string("gt"),
                        "value".to_string() => Value::Integer(0),
                    }),
                    "on_true".to_string() => Value::Object(im::hashmap!{
                        "type".to_string() => Value::string("increment"),
                        "params".to_string() => Value::empty_object(),
                    }),
                    "on_false".to_string() => Value::Object(im::hashmap!{
                        "type".to_string() => Value::string("advance_instruction"),
                        "params".to_string() => Value::empty_object(),
                    }),
                }),
            }),
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: x=5 > 0, current instruction check
        let exec = make_self_driving_exec("check", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(5));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: check → evaluate_domain(x>0=true) → pushes increment to queue
        // → drain: increment is a business instruction → set as current → next loop dispatch
        // → increment → set_context(x+=1) → advance → noop → stop
        assert_eq!(result.get("x"), Some(&Value::Integer(6)));
        assert!(!is_running(&result));
    }

    // ── Test 10: push_instruction_sequence batch enqueue ──
    // Verify: push_instruction_sequence pushes multiple instructions, drain processes them one by one
    #[test]
    fn test_self_driving_push_instruction_sequence() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // case "batch": push two increments to the queue
        let cases = im::hashmap! {
            "batch".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("push_instruction_sequence"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "instructions".to_string() => Value::list(vec![
                        Value::Object(im::hashmap!{
                            "type".to_string() => Value::string("increment"),
                            "params".to_string() => Value::empty_object(),
                        }),
                        Value::Object(im::hashmap!{
                            "type".to_string() => Value::string("increment"),
                            "params".to_string() => Value::empty_object(),
                        }),
                    ]),
                }),
            }),
            "increment".to_string() => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::Object(im::hashmap!{
                    "transform".to_string() => Value::Object(im::hashmap!{
                        "attr".to_string() => Value::string("x"),
                        "operation".to_string() => Value::string("add"),
                        "value".to_string() => Value::Integer(1),
                    }),
                }),
            }),
        };
        let default = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
        });

        let while_loop = make_core_eval_while_loop(cases, default);

        // Initial state: current instruction batch
        let exec = make_self_driving_exec("batch", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("x", Value::Integer(0));

        let result = exec_while_loop(&reg, &state, &while_loop, &mut ctx).unwrap();

        // Verify: batch → push_instruction_sequence(2 increments) → advance
        // → drain: increment(business) → next loop dispatch → x+=1 → advance
        // → drain: increment(business) → next loop dispatch → x+=1 → advance → noop → stop
        assert_eq!(result.get("x"), Some(&Value::Integer(2)));
        assert!(!is_running(&result));
    }

    // ── [FIX #2] Specific test: condition $ref object resolution ──

    #[test]
    fn test_condition_ref_object_resolved() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        // condition = {"$ref": "flag"}, flag in state is Bool(false)
        let mut params = HashMap::new();
        params.insert(
            "condition".to_string(),
            Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("flag"),
            }),
        );
        params.insert("body".to_string(), Value::empty_list());
        params.insert("max_steps".to_string(), Value::Integer(10));
        let instr = GenericInstruction::new("while_loop", params);

        let exec = make_self_driving_exec("noop", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("flag", Value::Bool(false));

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();
        // Condition is false, 0 iterations, __running remains true (loop didn't start)
        assert!(is_running(&result));
    }

    #[test]
    fn test_condition_ref_object_true() {
        let reg = make_registry();
        let mut ctx = ExecCtlCtx::new();

        let mut params = HashMap::new();
        params.insert(
            "condition".to_string(),
            Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("flag"),
            }),
        );
        params.insert(
            "body".to_string(),
            Value::list(vec![Value::Object(im::hashmap! {
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            })]),
        );
        params.insert("max_steps".to_string(), Value::Integer(3));
        let instr = GenericInstruction::new("while_loop", params);

        let exec = make_self_driving_exec("noop", Value::empty_object());
        let state = State::empty()
            .set("__exec__", exec)
            .set("flag", Value::Bool(true));

        let result = exec_while_loop(&reg, &state, &instr, &mut ctx).unwrap();
        // Condition is true, enters loop body. Body is noop, drain_queue detects
        // queue empty + current instruction is noop, triggers advance_instruction,
        // which hits termination_domain and sets __running=false → normal exit.
        // Verifies that $ref condition was correctly resolved to true (loop entered).
        assert!(!is_running(&result));
        assert_eq!(
            result
                .get("__exec__")
                .and_then(|v| v.get("__terminated_by_max_steps"))
                .and_then(|v| v.as_bool()),
            Some(false)
        );
    }
}
