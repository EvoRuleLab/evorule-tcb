// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P9: Audit chain integrity.
//!
//! P9: Audit chain hash is correctly linked and cannot be tampered with.

use evorule_tcb::{
    audit::{AuditChainState, DEFAULT_HMAC_KEY},
    deterministic::content_hash,
    exec_ctl_ctx::ExecCtlCtx,
    instruction::registry::create_full_registry,
    rule::GenericInstruction,
    state::State,
    value::Value,
};

#[test]
fn test_audit_chain_integrity() {
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

    let mut ctx = ExecCtlCtx::new();
    let result = reg.execute(&state, &instr, &mut ctx);

    assert!(result.is_ok());
}

#[test]
fn test_content_hash_deterministic() {
    let values = vec![Value::Integer(1), Value::string("hello"), Value::Bool(true)];

    let results: Vec<String> = (0..100).map(|_| content_hash(&values)).collect();

    for i in 1..100 {
        assert_eq!(results[0], results[i], "hash {} differs", i);
    }
}

#[test]
fn test_content_hash_object_deterministic() {
    let mut map = im::HashMap::new();
    map.insert("z".to_string(), Value::Integer(3));
    map.insert("a".to_string(), Value::Integer(1));
    map.insert("m".to_string(), Value::Integer(2));

    let obj = Value::Object(map);

    let results: Vec<String> = (0..100).map(|_| content_hash(&[obj.clone()])).collect();

    for i in 1..100 {
        assert_eq!(results[0], results[i], "hash {} differs", i);
    }
}

#[test]
fn test_content_hash_empty() {
    let result = content_hash(&[]);
    assert!(!result.is_empty());

    let result2 = content_hash(&[]);
    assert_eq!(result, result2);
}

#[test]
fn test_state_hash_consistency() {
    let state1 = State::empty()
        .set("a", Value::Integer(1))
        .set("b", Value::Integer(2));
    let state2 = State::empty()
        .set("b", Value::Integer(2))
        .set("a", Value::Integer(1));

    let hash1 = content_hash(&[state1.to_value()]);
    let hash2 = content_hash(&[state2.to_value()]);

    assert_eq!(hash1, hash2, "State hash should be order-independent");
}

// ========================================================================
// Property-based audit chain tests (P9) — augment the hand-written tests above.
// ========================================================================

use proptest::prelude::*;

proptest! {
    /// Property: content_hash of an Object is independent of insertion order.
    /// `value_to_bytes` sorts Object keys before hashing (see deterministic.rs §Object),
    /// so the same set of (k, v) entries always produces the same hash regardless of
    /// the order keys were inserted.
    #[test]
    fn prop_content_hash_object_order_independent(
        keys in proptest::collection::hash_set("[a-z][a-z0-9_]{0,7}", 1..8),
        vals in proptest::collection::vec(-1000i64..1000i64, 1..8),
    ) {
        // Align key/value counts (proptest strategies don't guarantee equal sizes)
        let n = keys.len().min(vals.len());
        let keys: Vec<String> = keys.into_iter().take(n).collect();
        let vals: Vec<i64> = vals.into_iter().take(n).collect();

        // Insertion order #1: forward
        let mut map_fwd = std::collections::HashMap::new();
        for (k, v) in keys.iter().zip(vals.iter()) {
            map_fwd.insert(k.clone(), Value::Integer(*v));
        }
        let obj_fwd = Value::object(map_fwd);

        // Insertion order #2: reverse
        let mut map_rev = std::collections::HashMap::new();
        for (k, v) in keys.iter().rev().zip(vals.iter().rev()) {
            map_rev.insert(k.clone(), Value::Integer(*v));
        }
        let obj_rev = Value::object(map_rev);

        let h_fwd = content_hash(&[obj_fwd]);
        let h_rev = content_hash(&[obj_rev]);
        prop_assert_eq!(h_fwd, h_rev,
            "content_hash must be order-independent for Object (P9)");
    }

    /// Property: State.set() calls in different orders produce the same content_hash.
    /// This is the State-level counterpart to the Object property test — a stronger
    /// guarantee that covers real-world State construction patterns.
    #[test]
    fn prop_state_hash_order_independent(
        keys in proptest::collection::hash_set("[a-z][a-z0-9_]{0,7}", 1..6),
        vals in proptest::collection::vec(-1000i64..1000i64, 1..6),
    ) {
        let n = keys.len().min(vals.len());
        let keys: Vec<String> = keys.into_iter().take(n).collect();
        let vals: Vec<i64> = vals.into_iter().take(n).collect();

        // Order #1
        let mut s1 = State::empty();
        for (k, v) in keys.iter().zip(vals.iter()) {
            s1 = s1.set(k.as_str(), Value::Integer(*v));
        }
        // Order #2 (reversed)
        let mut s2 = State::empty();
        for (k, v) in keys.iter().rev().zip(vals.iter().rev()) {
            s2 = s2.set(k.as_str(), Value::Integer(*v));
        }

        let h1 = content_hash(&[s1.to_value()]);
        let h2 = content_hash(&[s2.to_value()]);
        prop_assert_eq!(h1, h2,
            "State content_hash must be insertion-order-independent (P9)");
    }
}

proptest! {
    /// Property: After N trace_step instructions, the audit chain has exactly N records,
    /// and every record's logical tick is strictly increasing.
    /// This is the structural monotonicity invariant the governance layer relies on
    /// when reconstructing execution paths.
    #[test]
    fn prop_audit_chain_monotonic(n in 1usize..12) {
        use std::collections::HashMap;
        let reg = create_full_registry();
        let mut state = State::new(vec![("x", Value::integer(0))]);
        let mut ctx = ExecCtlCtx::new();

        for i in 0..n {
            let mut params = HashMap::new();
            params.insert("label".to_string(), Value::string(format!("step_{}", i)));
            params.insert("rule_id".to_string(), Value::string(format!("rule.step_{}", i)));
            let instr = GenericInstruction::new("trace_step", params);
            state = reg.execute(&state, &instr, &mut ctx).expect("trace_step must succeed");
        }

        let chain_val = state.get("__audit_chain").cloned().expect("__audit_chain must exist");
        let chain = AuditChainState::from_value(&chain_val).expect("chain must parse");

        // Exactly N records
        prop_assert_eq!(chain.records.len(), n, "chain must have exactly n records");

        // Logical tick monotonically increasing (0, 1, 2, ..., n-1)
        for i in 0..n {
            let expected_tick = i as i64;
            let actual_tick = chain.records[i].timestamp;
            prop_assert_eq!(actual_tick, expected_tick, "tick at index {} must equal i", i);
        }

        // Hash chain link: each record's previous_hash == previous record's hash
        for i in 1..n {
            prop_assert_eq!(
                &chain.records[i].previous_hash,
                &chain.records[i - 1].hash,
                "chain link broken at index {}",
                i
            );
        }

        // All records verify under default HMAC key
        for rec in &chain.records {
            prop_assert!(rec.verify(DEFAULT_HMAC_KEY), "record {} must verify", rec.id);
        }
    }
}
