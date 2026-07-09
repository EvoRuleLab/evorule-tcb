// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P1-P3: Faithfulness properties.
//!
//! P1: No hidden inputs (TCB reads no external state)
//! P2: No hidden outputs (TCB modifies no external state)
//! P3: No hidden state (all state affecting computation is explicit)

use evorule_tcb::{
    exec_ctl_ctx::ExecCtlCtx,
    instruction::registry::{create_full_registry, InstructionRegistry},
    rule::GenericInstruction,
    state::State,
    value::Value,
};

#[test]
fn test_no_hidden_state() {
    let reg = create_full_registry();
    let state = State::empty().set("x", Value::Integer(42));
    let params = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("y"));
        p.insert("value".to_string(), Value::Integer(100));
        p
    };
    let instr = GenericInstruction::new("state_set", params);

    let mut ctx1 = ExecCtlCtx::new();
    let mut ctx2 = ExecCtlCtx::new();

    let result1 = reg.execute(&state, &instr, &mut ctx1);
    let result2 = reg.execute(&state, &instr, &mut ctx2);

    assert_eq!(result1, result2);
    assert_eq!(ctx1.depth(), ctx2.depth());
}

#[test]
fn test_execute_is_reentrant() {
    let reg = create_full_registry();
    let state = State::empty().set("counter", Value::Integer(0));

    let params1 = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("a"));
        p.insert("value".to_string(), Value::Integer(1));
        p
    };
    let params2 = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("b"));
        p.insert("value".to_string(), Value::Integer(2));
        p
    };

    let instr1 = GenericInstruction::new("state_set", params1);
    let instr2 = GenericInstruction::new("state_set", params2);

    let mut ctx1 = ExecCtlCtx::new();
    let mut ctx2 = ExecCtlCtx::new();

    let _ = reg.execute(&state, &instr1, &mut ctx1);
    let _ = reg.execute(&state, &instr1, &mut ctx2);
    let _ = reg.execute(&state, &instr2, &mut ctx1);
    let _ = reg.execute(&state, &instr2, &mut ctx2);

    assert_eq!(ctx1.depth(), ctx2.depth());
}

#[test]
#[cfg(not(debug_assertions))]
fn test_registry_frozen_cannot_register_release() {
    let mut reg = InstructionRegistry::new();
    reg.freeze();

    reg.register("test_primitive", |_reg, _state, _instr, _ctx| {
        Ok(State::empty())
    });

    assert!(!reg.has("test_primitive"));
}

#[test]
#[cfg(debug_assertions)]
fn test_registry_frozen_cannot_register_debug() {
    let mut reg = InstructionRegistry::new();
    reg.freeze();

    let result = std::panic::catch_unwind(move || {
        reg.register("test_primitive", |_reg, _state, _instr, _ctx| {
            Ok(State::empty())
        });
    });

    assert!(result.is_err());
}

#[test]
fn test_exec_ctx_is_explicit() {
    let reg = create_full_registry();
    let state = State::empty().set("x", Value::Integer(1));

    let params = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("x"));
        p.insert("operation".to_string(), Value::string("add"));
        p.insert("value".to_string(), Value::Integer(1));
        p
    };
    let instr = GenericInstruction::new("state_compute", params);

    let mut ctx1 = ExecCtlCtx::new();
    let mut ctx2 = ExecCtlCtx::new();

    let _ = reg.execute(&state, &instr, &mut ctx1);
    let _ = reg.execute(&state, &instr, &mut ctx2);

    assert_eq!(ctx1.current_tick(), ctx2.current_tick());
}
