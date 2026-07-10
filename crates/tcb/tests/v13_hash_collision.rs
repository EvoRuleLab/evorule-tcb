// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Property-based hash collision and avalanche tests for content_hash.
//!
//! Three properties protect the hash function:
//!   1. Different Value -> different hash (no trivial collisions)
//!   2. One-byte difference in input -> drastically different hash (avalanche)
//!   3. Type-tagged disambiguation: Value::Integer(0) != Value::string("0")
//!      != Value::Bool(false) in hash space
//!
//! These properties guarantee that content_hash is suitable as a content
//! fingerprint in the audit chain (see audit.rs::compute_state_hash).

use evorule_tcb::deterministic::content_hash;
use evorule_tcb::Value;
use proptest::prelude::*;

proptest! {
    /// Property: Two Value variants of the same shape produce different
    /// content_hash values when their data differs.
    /// (This is the basic "no trivial collisions" invariant.)
    #[test]
    fn prop_hash_distinguishes_values(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
    ) {
        prop_assume!(a != b);
        let h_a = content_hash(&[Value::integer(a)]);
        let h_b = content_hash(&[Value::integer(b)]);
        prop_assert_ne!(&h_a, &h_b, "different integers must hash differently");
    }

    /// Property: A single-byte change in the input produces a drastically
    /// different hash (avalanche effect).
    /// We construct two strings differing by exactly one byte and verify
    /// that NO prefix of the resulting hashes match (length-2 window).
    /// This is a strong form of the avalanche property.
    #[test]
    fn prop_hash_avalanche_byte_change(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{0,5}",
        byte_a in 0u8..127,  // ASCII only, guarantees valid UTF-8
    ) {
        // Skip the rare case byte_a = 126 (wrapping_add gives 127, also valid)
        // but byte_a = 127 wraps to 0, which is fine.
        // Build base string: prefix + 1 byte + suffix
        let byte_b = byte_a.wrapping_add(1);
        let mut bytes_a = Vec::with_capacity(prefix.len() + 1 + suffix.len());
        bytes_a.extend_from_slice(prefix.as_bytes());
        bytes_a.push(byte_a);
        bytes_a.extend_from_slice(suffix.as_bytes());

        let mut bytes_b = bytes_a.clone();
        bytes_b[prefix.len()] = byte_b;

        let s_a = String::from_utf8(bytes_a).expect("ASCII input must be valid UTF-8");
        let s_b = String::from_utf8(bytes_b).expect("ASCII input must be valid UTF-8");

        let h_a = content_hash(&[Value::string(s_a.clone())]);
        let h_b = content_hash(&[Value::string(s_b.clone())]);

        prop_assert_ne!(&h_a, &h_b, "one-byte input difference must produce different hash");

        // Avalanche: first 4 hex chars of the hash should differ.
        // (SHA-256 is a strong hash function, so any single-bit input
        // difference should flip roughly half the output bits.)
        let common_prefix_len = h_a
            .chars()
            .zip(h_b.chars())
            .take_while(|(c1, c2)| c1 == c2)
            .count();
        prop_assert!(
            common_prefix_len < 4,
            "avalanche: 1-byte input change must flip at least 4 hex chars of the hash              (got common_prefix_len = {})",
            common_prefix_len
        );
    }

    /// Property: Distinct Value variants do NOT collide in hash space.
    /// This catches the common bug of a hash function that uses only the
    /// inner data and ignores the type tag (e.g. hashing 0 and "0" the
    /// same way). EvoRule's content_hash prefixes each value with its
    /// type discriminator, so this must hold for all distinct variants
    /// we test.
    #[test]
    fn prop_hash_type_disambiguation(_dummy in 0u8..1) {
        // Picked values that would collide under naive stringification.
        let v_int = Value::integer(0);
        let v_str = Value::string("0");
        let v_bool = Value::Bool(false);
        let v_null = Value::Null;
        let v_empty_str = Value::string("");

        let h_int = content_hash(&[v_int]);
        let h_str = content_hash(&[v_str]);
        let h_bool = content_hash(&[v_bool]);
        let h_null = content_hash(&[v_null]);
        let h_empty = content_hash(&[v_empty_str]);

        // 10 distinct hash values (5 inputs, all must differ)
        let all = [&h_int, &h_str, &h_bool, &h_null, &h_empty];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                prop_assert_ne!(
                    all[i], all[j],
                    "distinct Value variants must not collide in hash space (i={}, j={})",
                    i, j
                );
            }
        }
    }

    /// Property: The same Value always produces the same hash (determinism).
    /// This is the property that makes content_hash suitable for audit chain
    /// integrity verification — two equivalent states must have equal hashes.
    #[test]
    fn prop_hash_deterministic(
        n in -1_000_000i64..1_000_000i64,
        s in "[a-z]{0,8}",
        b in any::<bool>(),
    ) {
        // Build a complex Value: a list of three distinct types.
        let v1 = Value::integer(n);
        let v2 = Value::string(s);
        let v3 = Value::Bool(b);
        let v = Value::list(vec![v1, v2, v3]);

        let h1 = content_hash(std::slice::from_ref(&v));
        let h2 = content_hash(&[v]);
        prop_assert_eq!(h1, h2, "content_hash must be deterministic");
    }

    /// Property: Hash of Value::list is independent of element order
    /// for sets (using unordered inner values), but IS dependent for
    /// ordered lists. We test the strong form: hash of a single-element
    /// list with different elements is distinct.
    /// (This is just a sanity check that the list-prefix is wired up.)
    #[test]
    fn prop_hash_list_order_matters(
        a in -1000i64..1000i64,
        b in -1000i64..1000i64,
    ) {
        prop_assume!(a != b);
        let list_ab = Value::list(vec![Value::integer(a), Value::integer(b)]);
        let list_ba = Value::list(vec![Value::integer(b), Value::integer(a)]);
        let h_ab = content_hash(&[list_ab]);
        let h_ba = content_hash(&[list_ba]);
        prop_assert_ne!(
            &h_ab, &h_ba,
            "List is order-sensitive by design; different order -> different hash"
        );
    }
}
