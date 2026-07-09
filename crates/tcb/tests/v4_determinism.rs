// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P4: Determinism.
//!
//! P4: Same input → same output across all runs.

use evorule_tcb::{
    exec_ctl_ctx::ExecCtlCtx, instruction::registry::create_full_registry,
    rule::GenericInstruction, state::State, value::Value,
};

#[test]
fn test_determinism_basic() {
    let reg = create_full_registry();
    let state = State::empty()
        .set("a", Value::Integer(1))
        .set("b", Value::Integer(2));

    let params = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("c"));
        p.insert("value".to_string(), Value::Integer(3));
        p
    };
    let instr = GenericInstruction::new("state_set", params);

    let mut ctx1 = ExecCtlCtx::new();
    let mut ctx2 = ExecCtlCtx::new();

    let result1 = reg.execute(&state, &instr, &mut ctx1);
    let result2 = reg.execute(&state, &instr, &mut ctx2);

    assert_eq!(result1, result2);
}

#[test]
fn test_determinism_multiple_executions() {
    let reg = create_full_registry();
    let state = State::empty().set("x", Value::Integer(10));

    let params = {
        let mut p = std::collections::HashMap::new();
        p.insert("attr".to_string(), Value::string("x"));
        p.insert("operation".to_string(), Value::string("add"));
        p.insert("value".to_string(), Value::Integer(5));
        p
    };
    let instr = GenericInstruction::new("state_compute", params);

    let mut results = Vec::with_capacity(100);
    for _ in 0..100 {
        let mut ctx = ExecCtlCtx::new();
        results.push(reg.execute(&state, &instr, &mut ctx));
    }

    for i in 1..100 {
        assert_eq!(
            results[0], results[i],
            "execution {} differs from execution 0",
            i
        );
    }
}

#[test]
fn test_determinism_state_set_multiple() {
    let reg = create_full_registry();
    let state = State::empty().set("counter", Value::Integer(0));

    for iteration in 0..10 {
        let params = {
            let mut p = std::collections::HashMap::new();
            p.insert("attr".to_string(), Value::string("counter"));
            p.insert("value".to_string(), Value::Integer(iteration + 1));
            p
        };
        let instr = GenericInstruction::new("state_set", params);

        let mut ctx = ExecCtlCtx::new();
        let result = reg.execute(&state, &instr, &mut ctx).unwrap();

        assert_eq!(result.get("counter"), Some(&Value::Integer(iteration + 1)));
    }
}

#[test]
fn test_determinism_set_path() {
    let state = State::empty();

    for _ in 0..10 {
        let result = state.set_path("a.b.c", Value::Integer(42));
        assert!(result.is_ok());
        let s = result.unwrap();
        assert_eq!(s.get_path("a.b.c"), Some(Value::Integer(42)));
    }
}

// ========================================================================
// Property-based determinism tests (P4) — augment the hand-written tests above.
// Auto-shrinks failing inputs so we get minimal counterexamples for free.
// ========================================================================

use proptest::prelude::*;

proptest! {
    /// Property: state_set with same (attr, value) is deterministic across runs.
    /// Mirrors `test_determinism_basic` but parameterises initial state, attr name, and new value.
    #[test]
    fn prop_determinism_state_set(
        attr_name in "[a-z][a-z0-9_]{0,7}",
        initial_value in -1000i64..1000i64,
        new_value in -1000i64..1000i64,
    ) {
        let reg = create_full_registry();
        let state = State::empty().set(attr_name.as_str(), Value::Integer(initial_value));

        let mut params = std::collections::HashMap::new();
        params.insert("attr".to_string(), Value::string(attr_name.as_str()));
        params.insert("value".to_string(), Value::Integer(new_value));
        let instr = GenericInstruction::new("state_set", params);

        let mut ctx1 = ExecCtlCtx::new();
        let mut ctx2 = ExecCtlCtx::new();

        let r1 = reg.execute(&state, &instr, &mut ctx1);
        let r2 = reg.execute(&state, &instr, &mut ctx2);
        prop_assert_eq!(r1, r2, "state_set must be deterministic (P4)");
    }

    /// Property: state_set with same input is idempotent over many executions.
    /// Mirrors `test_determinism_multiple_executions` but parameterises attr + value.
    #[test]
    fn prop_determinism_state_set_idempotent(
        attr_name in "[a-z][a-z0-9_]{0,7}",
        value in -100_000i64..100_000i64,
        trial_count in 1usize..32,
    ) {
        let reg = create_full_registry();
        let state = State::empty();

        let mut params = std::collections::HashMap::new();
        params.insert("attr".to_string(), Value::string(attr_name.as_str()));
        params.insert("value".to_string(), Value::Integer(value));
        let instr = GenericInstruction::new("state_set", params);

        let mut results: Vec<_> = Vec::with_capacity(trial_count);
        for _ in 0..trial_count {
            let mut ctx = ExecCtlCtx::new();
            results.push(reg.execute(&state, &instr, &mut ctx));
        }
        for i in 1..trial_count {
            prop_assert_eq!(&results[0], &results[i],
                "execution {} differs from execution 0 (P4 idempotency)", i);
        }
    }

    /// Property: state_set updates the named attr to the supplied value.
    /// Mirrors `test_determinism_state_set_multiple` but with generated attr + value.
    #[test]
    fn prop_determinism_state_set_writes_back(
        attr_name in "[a-z][a-z0-9_]{0,7}",
        value in i64::MIN/2..i64::MAX/2,
    ) {
        let reg = create_full_registry();
        let state = State::empty();

        let mut params = std::collections::HashMap::new();
        params.insert("attr".to_string(), Value::string(attr_name.as_str()));
        params.insert("value".to_string(), Value::Integer(value));
        let instr = GenericInstruction::new("state_set", params);

        let mut ctx = ExecCtlCtx::new();
        let result = reg.execute(&state, &instr, &mut ctx).unwrap();
        let expected = Value::Integer(value);
        prop_assert_eq!(result.get(attr_name.as_str()), Some(&expected));
    }
}
