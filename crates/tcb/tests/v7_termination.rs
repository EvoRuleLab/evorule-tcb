// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P7: Loop termination.
//!
//! P7: All while loops and dispatch calls must terminate.

use evorule_tcb::{
    exec_ctl_ctx::ExecCtlCtx, instruction::registry::create_full_registry,
    rule::GenericInstruction, state::State, value::Value,
};

#[test]
fn test_while_loop_terminates() {
    let reg = create_full_registry();
    let mut params = std::collections::HashMap::new();
    params.insert("body".to_string(), Value::List(im::Vector::new()));
    params.insert("max_steps".to_string(), Value::Integer(10));
    let instr = GenericInstruction::new("while_loop", params);

    let state = State::empty().set("__running", Value::Bool(true)).set(
        "__exec__",
        Value::Object(im::HashMap::from(vec![
            ("running".to_string(), Value::Bool(true)),
            ("step".to_string(), Value::Integer(0)),
        ])),
    );

    let mut ctx = ExecCtlCtx::new();
    let result = reg.execute(&state, &instr, &mut ctx);

    assert!(result.is_ok());
}

#[test]
fn test_dispatch_depth_limit() {
    let reg = create_full_registry();

    let mut params = std::collections::HashMap::new();
    params.insert(
        "instruction".to_string(),
        Value::Object(im::HashMap::from(vec![(
            "type".to_string(),
            Value::string("noop"),
        )])),
    );
    let instr = GenericInstruction::new("dispatch", params);

    let state = State::empty();
    let mut ctx = ExecCtlCtx::new().with_max_depth(1);

    let result = reg.execute(&state, &instr, &mut ctx);

    assert!(result.is_ok());
}

#[test]
fn test_exec_ctx_depth_tracking() {
    let reg = create_full_registry();

    let params = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("x"));
        p.insert("value".to_string(), Value::Integer(1));
        p
    };
    let instr = GenericInstruction::new("state_set", params);

    let state = State::empty();
    let mut ctx = ExecCtlCtx::new();

    assert_eq!(ctx.depth(), 0);

    let _ = reg.execute(&state, &instr, &mut ctx);

    assert_eq!(ctx.depth(), 0);
}

#[test]
fn test_exec_ctx_tick_tracking() {
    let mut ctx = ExecCtlCtx::new();

    assert_eq!(ctx.current_tick(), 0);

    let tick1 = ctx.next_tick();
    assert_eq!(tick1, 0);
    assert_eq!(ctx.current_tick(), 1);

    let tick2 = ctx.next_tick();
    assert_eq!(tick2, 1);
    assert_eq!(ctx.current_tick(), 2);
}
