// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Verification tests for P8: Recursion boundedness.
//!
//! P8: All recursive calls must have explicit depth limits.

use evorule_tcb::{state::State, state::MAX_PATH_DEPTH, value::Value};

#[test]
fn test_set_path_depth_limit() {
    let state = State::empty();
    let deep_path = (0..(MAX_PATH_DEPTH + 1))
        .map(|i| format!("level{}", i))
        .collect::<Vec<_>>()
        .join(".");

    let result = state.set_path(&deep_path, Value::Integer(42));

    assert!(
        result.is_err(),
        "path depth {} should exceed limit {}",
        MAX_PATH_DEPTH + 1,
        MAX_PATH_DEPTH
    );
}

#[test]
fn test_set_path_depth_equals_max() {
    let state = State::empty();
    let deep_path = (0..MAX_PATH_DEPTH)
        .map(|i| format!("level{}", i))
        .collect::<Vec<_>>()
        .join(".");

    let result = state.set_path(&deep_path, Value::Integer(42));

    assert!(
        result.is_ok(),
        "path depth {} should be within limit",
        MAX_PATH_DEPTH
    );
    let s = result.unwrap();
    assert_eq!(s.get_path(&deep_path), Some(Value::Integer(42)));
}

#[test]
fn test_set_path_shallow_paths_work() {
    let state = State::empty();

    let paths = vec!["a", "a.b", "a.b.c", "a.b.c.d", "items[0].name"];

    for path in paths {
        let result = state.set_path(path, Value::Integer(42));
        assert!(result.is_ok(), "path '{}' should succeed", path);
    }
}

#[test]
fn test_set_path_array_index_depth() {
    let state = State::empty();

    let deep_path = (0..(MAX_PATH_DEPTH + 1))
        .map(|i| format!("arr[{}]", i))
        .collect::<Vec<_>>()
        .join(".");

    let result = state.set_path(&deep_path, Value::Integer(42));

    assert!(result.is_err(), "array path depth should exceed limit");
}

#[test]
#[allow(clippy::assertions_on_constants)] // sanity-check that public constants remain within reasonable bounds
fn test_set_path_depth_constant_is_public() {
    assert!(MAX_PATH_DEPTH > 0, "MAX_PATH_DEPTH should be positive");
    assert!(
        MAX_PATH_DEPTH <= 1024,
        "MAX_PATH_DEPTH should be reasonable"
    );
}
