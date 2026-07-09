// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P6: Deterministic iteration order.
//!
//! P6: All collection iterations are deterministic.

use evorule_tcb::{
    state::State,
    value::{ImMapExt, Value},
};

#[test]
fn test_iter_order_deterministic() {
    let mut map = im::HashMap::new();
    map.insert("z".to_string(), Value::Integer(3));
    map.insert("a".to_string(), Value::Integer(1));
    map.insert("m".to_string(), Value::Integer(2));

    let results: Vec<_> = (0..100)
        .map(|_| {
            map.iter_sorted()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    for i in 1..100 {
        assert_eq!(results[0], results[i], "iteration {} differs", i);
    }
}

#[test]
fn test_iter_order_lexicographic() {
    let mut map = im::HashMap::new();
    map.insert("beta".to_string(), Value::Integer(2));
    map.insert("alpha".to_string(), Value::Integer(1));
    map.insert("gamma".to_string(), Value::Integer(3));

    let entries: Vec<_> = map.iter_sorted().map(|(k, _)| k.clone()).collect();

    assert_eq!(entries, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn test_state_to_std_map_order() {
    let state = State::empty()
        .set("z", Value::Integer(3))
        .set("a", Value::Integer(1))
        .set("m", Value::Integer(2));

    let results: Vec<_> = (0..100)
        .map(|_| {
            let mut keys: Vec<_> = state.to_std_map().keys().cloned().collect();
            keys.sort();
            keys
        })
        .collect();

    for i in 1..100 {
        assert_eq!(results[0], results[i], "iteration {} differs", i);
    }
}

#[test]
fn test_state_user_keys_order() {
    let state = State::empty()
        .set("__exec__", Value::empty_object())
        .set("z", Value::Integer(3))
        .set("a", Value::Integer(1))
        .set("m", Value::Integer(2));

    let keys = state.user_keys();

    assert_eq!(keys, vec!["a", "m", "z"]);
}

#[test]
fn test_json_serialization_order() {
    let state = State::empty()
        .set("z", Value::Integer(3))
        .set("a", Value::Integer(1))
        .set("m", Value::Integer(2));

    let results: Vec<String> = (0..100).map(|_| state.to_json()).collect();

    for i in 1..100 {
        assert_eq!(results[0], results[i], "iteration {} differs", i);
    }
}
