// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Property-based set-operation commutativity tests.
//!
//! Verifies the algebraic properties of compute_ops::set_union / set_intersection /
//! set_diff over the State path-resolution bridge:
//!   - set_union(A, B) == set_union(B, A)         (commutative)
//!   - set_intersection(A, B) == set_intersection(B, A)   (commutative)
//!   - set_diff(A, B) != set_diff(B, A) in general (anti-commutative)
//!
//! The anti-commutative property for set_diff is the **stronger** assertion —
//! it would catch a "buggy" implementation that quietly aliases set_union
//! to set_diff, since union IS commutative.

use evorule_tcb::instruction::registry::create_full_registry;
use evorule_tcb::rule::GenericInstruction;
use evorule_tcb::{ExecCtlCtx, State, Value};
use proptest::prelude::*;
use std::collections::HashMap;

/// Helper: build a State containing two random string lists at paths
/// "set_a" and "set_b", with both lists deduplicated and sorted for stable
/// comparison.
fn build_state(set_a: Vec<String>, set_b: Vec<String>) -> State {
    let mut state = State::empty();
    let mut a_sorted = set_a;
    a_sorted.sort();
    a_sorted.dedup();
    let mut b_sorted = set_b;
    b_sorted.sort();
    b_sorted.dedup();
    state = state.set(
        "set_a",
        Value::list(a_sorted.into_iter().map(Value::string).collect()),
    );
    state = state.set(
        "set_b",
        Value::list(b_sorted.into_iter().map(Value::string).collect()),
    );
    state
}

/// Helper: execute a set-op instruction (name = "set_union" | "set_intersection" |
/// "set_diff") and return the new state along with the result list stored at
/// `result_attr`. Returning the state allows chained operations that depend on
/// previously-computed result_attrs.
fn run_set_op(
    name: &str,
    state: &State,
    list_a: &str,
    list_b: &str,
    result_attr: &str,
) -> (State, Vec<String>) {
    let reg = create_full_registry();
    let mut ctx = ExecCtlCtx::new();
    let mut params = HashMap::new();
    params.insert("list_a".to_string(), Value::string(list_a.to_string()));
    params.insert("list_b".to_string(), Value::string(list_b.to_string()));
    params.insert(
        "result_attr".to_string(),
        Value::string(result_attr.to_string()),
    );
    let instr = GenericInstruction::new(name, params);
    let new_state = reg
        .execute(state, &instr, &mut ctx)
        .expect("set op must succeed");
    let result = new_state
        .get(result_attr)
        .cloned()
        .expect("result_attr must exist");
    let list = match result {
        Value::List(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => panic!("result_attr must be a List, got {:?}", result),
    };
    (new_state, list)
}

proptest! {
    /// Property: set_union(A, B) == set_union(B, A).
    /// The result is the union of two sets, which is order-independent by
    /// definition. This property protects against asymmetric implementations
    /// (e.g. one that prefers list_a over list_b).
    #[test]
    fn prop_set_union_commutative(
        set_a in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
        set_b in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
    ) {
        let state = build_state(set_a.iter().cloned().collect(), set_b.iter().cloned().collect());
        let (_, union_ab) = run_set_op("set_union", &state, "$set_a", "$set_b", "$union_ab");
        let (_, union_ba) = run_set_op("set_union", &state, "$set_b", "$set_a", "$union_ba");
        prop_assert_eq!(union_ab, union_ba, "set_union must be commutative");
    }

    /// Property: set_intersection(A, B) == set_intersection(B, A).
    #[test]
    fn prop_set_intersection_commutative(
        set_a in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
        set_b in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
    ) {
        let state = build_state(set_a.iter().cloned().collect(), set_b.iter().cloned().collect());
        let (_, inter_ab) = run_set_op("set_intersection", &state, "$set_a", "$set_b", "$inter_ab");
        let (_, inter_ba) = run_set_op("set_intersection", &state, "$set_b", "$set_a", "$inter_ba");
        prop_assert_eq!(inter_ab, inter_ba, "set_intersection must be commutative");
    }

    /// Property: set_diff(A, B) != set_diff(B, A) for non-trivial inputs.
    /// This is the **anti-commutative** assertion that distinguishes set_diff
    /// from set_union. A buggy implementation that aliases them would FAIL this
    /// test (because set_union IS commutative and would produce equal results).
    /// We require that at least one of {set_a, set_b} is non-empty AND they
    /// differ, so the test exercises asymmetric inputs.
    #[test]
    fn prop_set_diff_anti_commutative(
        set_a in proptest::collection::hash_set("[a-z]{1,4}", 1..6),
        set_b in proptest::collection::hash_set("[a-z]{1,4}", 1..6),
    ) {
        // Only run when set_a != set_b (otherwise the diff is empty in both
        // directions, which trivially satisfies anti-commutativity vacuously).
        prop_assume!(set_a != set_b);
        let state = build_state(set_a.iter().cloned().collect(), set_b.iter().cloned().collect());
        let (_, diff_ab) = run_set_op("set_diff", &state, "$set_a", "$set_b", "$diff_ab");
        let (_, diff_ba) = run_set_op("set_diff", &state, "$set_b", "$set_a", "$diff_ba");
        prop_assert_ne!(diff_ab, diff_ba, "set_diff must be anti-commutative when set_a != set_b");
    }

    /// Property: set_diff is consistent with set_union and set_intersection:
    /// A == (A intersect B) union (A diff B), and this identity holds for
    /// random inputs. This protects the algebraic structure of the three
    /// set operations.
    #[test]
    fn prop_set_decomposition_identity(
        set_a in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
        set_b in proptest::collection::hash_set("[a-z]{1,4}", 0..6),
    ) {
        let state = build_state(set_a.iter().cloned().collect(), set_b.iter().cloned().collect());

        let (s1, inter) = run_set_op("set_intersection", &state, "$set_a", "$set_b", "inter");
        let (s2, diff) = run_set_op("set_diff", &s1, "$set_a", "$set_b", "diff");
        let (_, reconstructed) = run_set_op("set_union", &s2, "$inter", "$diff", "reconstructed");

        let mut expected_a: Vec<String> = set_a.iter().cloned().collect();
        expected_a.sort();
        expected_a.dedup();
        prop_assert_eq!(
            reconstructed, expected_a,
            "set_a must equal union(intersect(a,b), diff(a,b))"
        );
        let _ = (inter, diff); // silence unused warning if any
    }
}
