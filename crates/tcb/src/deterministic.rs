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

//! Deterministic components — Clock, hash, and random number generator.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! All components guarantee determinism: same input → same output.
//! Determinism is the core promise of `EvoRule`.

//! Deterministic components — Clock, hash, and random number generator.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Determinism Guarantee
//!
//! All components in this module are **L1 deterministic**:
//! - `content_hash`: SHA-256 (FIPS 180-4).
//! - `LogicalClock`: Monotonic integer counter.
//! - `DeterministicRNG`: LCG (Linear Congruential Generator) — same seed → same sequence.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `content_hash` SHA-256 | ✅ L1 deterministic | FIPS 180-4 |
//! | `content_hash` byte serialization | ✅ L1 deterministic | All types use fixed encoding |
//! | `LogicalClock` | ✅ L1 deterministic | u64 increment |
//! | `DeterministicRNG` | ✅ L1 deterministic | LCG parameters fixed |
//! | Float `to_le_bytes()` | ✅ L1 + L3 conditional | See "Float Serialization" below |
//!
//! # Float Serialization (Cross-Platform Note)
//!
//! `content_hash` serializes `Float` values using `f64.to_le_bytes()`.
//! This produces **little-endian byte order on all platforms** (guaranteed by Rust).
//!
//! While the byte order is fixed, the **floating-point bit pattern** may differ
//! between platforms if the value was produced by platform-dependent arithmetic.
//! For values that originate from serialized JSON, the bit pattern is consistent
//! across platforms.
//!
//! # Cross-Language Note (L4)
//!
//! To replicate `content_hash` in other languages, implement the following encoding:
//! - Null: `0x00`
//! - Bool: `0x01` + `0x01` (true) or `0x00` (false)
//! - Integer: `0x02` + 8-byte little-endian
//! - Float: `0x03` + 8-byte little-endian (NaN normalized to 0.0)
//! - String: `0x04` + 8-byte length prefix (big-endian) + UTF-8 bytes
//! - List: `0x05` + 8-byte length prefix (big-endian) + each element
//! - Object: `0x06` + 8-byte length prefix (big-endian) + sorted keys (length-prefixed) + values
//! - Bytes: `0x08` + 8-byte length prefix (big-endian) + raw bytes

use crate::value::{ImMapExt, Value};
use sha2::{Digest, Sha256};

/// Content hash — `EvoRule`'s deterministic hash function.
///
/// Based on SHA-256, consistent across languages. Used for State content verification,
/// audit chain linking, and deterministic seed generation.
pub fn content_hash(values: &[Value]) -> String {
    let mut hasher = Sha256::new();
    for val in values {
        let bytes = value_to_bytes(val);
        hasher.update(&bytes);
    }
    hex::encode(hasher.finalize())
}

const MAX_HASH_DEPTH: usize = 128;

/// Convert a Value to bytes (for hashing).
///
/// Each variant has a unique type tag prefix to prevent collisions between
/// different types. Variable-length types (String, Bytes, Object keys) also
/// include a length prefix (big-endian u64) to prevent boundary ambiguity
/// when concatenated in List/Object.
///
/// # Depth Limit
///
/// Recursive processing of List and Object values is limited to `MAX_HASH_DEPTH`
/// to prevent infinite recursion from deeply nested or cyclic data structures.
/// Exceeding the depth limit returns a placeholder (0x07) instead of the actual bytes.
fn value_to_bytes(val: &Value) -> Vec<u8> {
    value_to_bytes_inner(val, 0)
}

fn value_to_bytes_inner(val: &Value, depth: usize) -> Vec<u8> {
    if depth > MAX_HASH_DEPTH {
        return vec![0x07];
    }
    match val {
        Value::Null => vec![0x00],
        Value::Bool(b) => vec![0x01, u8::from(*b)],
        Value::Integer(i) => {
            let mut bytes = vec![0x02];
            bytes.extend_from_slice(&i.to_le_bytes());
            bytes
        }
        Value::Float(f) => {
            debug_assert!(
                !f.0.is_nan(),
                "Float NaN should have been normalized by Value::float()"
            );
            let mut bytes = vec![0x03];
            bytes.extend_from_slice(&f.0.to_le_bytes());
            bytes
        }
        Value::String(s) => {
            let mut bytes = vec![0x04];
            bytes.extend_from_slice(&(s.len() as u64).to_be_bytes());
            bytes.extend_from_slice(s.as_bytes());
            bytes
        }
        Value::List(v) => {
            let mut bytes = vec![0x05];
            bytes.extend_from_slice(&(v.len() as u64).to_be_bytes());
            for item in v {
                bytes.extend_from_slice(&value_to_bytes_inner(item, depth + 1));
            }
            bytes
        }
        Value::Object(m) => {
            let mut bytes = vec![0x06];
            bytes.extend_from_slice(&(m.len() as u64).to_be_bytes());
            for (key, value) in m.iter_sorted() {
                bytes.extend_from_slice(&(key.len() as u64).to_be_bytes());
                bytes.extend_from_slice(key.as_bytes());
                bytes.extend_from_slice(&value_to_bytes_inner(value, depth + 1));
            }
            bytes
        }
        Value::Bytes(b) => {
            let mut bytes = vec![0x08];
            bytes.extend_from_slice(&(b.len() as u64).to_be_bytes());
            bytes.extend_from_slice(b);
            bytes
        }
    }
}

// ══════════════════════════════════════════════
// LogicalClock
// ══════════════════════════════════════════════

/// Logical clock — deterministic tick generator.
///
/// Each call to `tick()` returns the next integer, guaranteed to be strictly monotonic.
/// Consistent across languages: same call sequence produces the same sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalClock {
    tick_count: u64,
}

impl LogicalClock {
    /// Create a new logical clock, starting from 0.
    pub const fn new() -> Self {
        Self { tick_count: 0 }
    }

    /// Create a clock with a specific tick count.
    pub const fn from_tick(tick: u64) -> Self {
        Self { tick_count: tick }
    }

    /// Get the next tick.
    pub fn tick(&mut self) -> u64 {
        let current = self.tick_count;
        self.tick_count += 1;
        current
    }

    /// Get the current tick count (without incrementing).
    pub const fn current_tick(&self) -> u64 {
        self.tick_count
    }

    /// Reset the clock.
    pub fn reset(&mut self) {
        self.tick_count = 0;
    }

    /// Get the current state (for serialization).
    pub fn state(&self) -> Value {
        Value::Object(im::hashmap! {
            "tick_count".to_string() => Value::Integer(self.tick_count as i64),
        })
    }

    pub fn from_state(state: &Value) -> Self {
        let tick = state
            .get("tick_count")
            .and_then(super::value::Value::as_integer)
            .unwrap_or(0) as u64;
        Self::from_tick(tick)
    }
}

impl Default for LogicalClock {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════
// DeterministicRNG
// ══════════════════════════════════════════════

/// Deterministic random number generator — Linear Congruential Generator (LCG).
///
/// Not cryptographically secure; only guarantees determinism — same seed always
/// produces the same sequence. Used for scenarios that require reproducibility
/// rather than security.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicRNG {
    state: u64,
}

impl DeterministicRNG {
    /// Create an RNG with the specified seed.
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    /// Create a seed from a content hash.
    pub fn from_content_hash(hash: &str) -> Self {
        let seed = hash.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(b))
        });
        Self::new(seed)
    }

    /// Generate the next u64 random number.
    pub fn next_u64(&mut self) -> u64 {
        // LCG parameters (Numerical Recipes)
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    /// Generate a random f64 in the range [0, 1).
    pub fn random(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Generate a random integer in the range [min, max].
    pub fn randint(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        let range = (max - min + 1) as u64;
        min + (self.next_u64() % range) as i64
    }

    /// Randomly select an item from a list.
    pub fn choice<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            return None;
        }
        // Use u64 arithmetic to avoid platform-dependent usize truncation.
        // Casting next_u64() to usize on 32-bit platforms would lose the high
        // 32 bits, producing different sequences than on 64-bit platforms.
        let idx = (self.next_u64() % (items.len() as u64)) as usize;
        Some(items[idx].clone())
    }

    /// Shuffle a list (Fisher-Yates).
    pub fn shuffle<T: Clone>(&mut self, items: &mut [T]) {
        let len = items.len();
        for i in (1..len).rev() {
            // Use u64 arithmetic to avoid platform-dependent usize truncation.
            let j = (self.next_u64() % ((i + 1) as u64)) as usize;
            items.swap(i, j);
        }
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    #![allow(clippy::cloned_ref_to_slice_refs)] // content_hash takes &[Value] (owned); clone is required
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let val = Value::Integer(42);
        let h1 = content_hash(&[val.clone()]);
        let h2 = content_hash(&[val.clone()]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different() {
        let h1 = content_hash(&[Value::Integer(42)]);
        let h2 = content_hash(&[Value::Integer(43)]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_content_hash_string() {
        let h = content_hash(&[Value::string("hello")]);
        assert_eq!(h.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_content_hash_object_deterministic() {
        let map = im::HashMap::from(vec![
            ("b".to_string(), Value::Integer(2)),
            ("a".to_string(), Value::Integer(1)),
        ]);
        let h = content_hash(&[Value::Object(map)]);
        // Same hash regardless of insertion order
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn test_logical_clock() {
        let mut clock = LogicalClock::new();
        assert_eq!(clock.tick(), 0);
        assert_eq!(clock.tick(), 1);
        assert_eq!(clock.tick(), 2);
        assert_eq!(clock.current_tick(), 3);
    }

    #[test]
    fn test_logical_clock_state_roundtrip() {
        let mut clock = LogicalClock::new();
        clock.tick();
        clock.tick();
        let state = clock.state();
        let restored = LogicalClock::from_state(&state);
        assert_eq!(restored.current_tick(), 2);
    }

    #[test]
    fn test_rng_deterministic() {
        let mut rng1 = DeterministicRNG::new(42);
        let mut rng2 = DeterministicRNG::new(42);
        let nums1: Vec<f64> = (0..5).map(|_| rng1.random()).collect();
        let nums2: Vec<f64> = (0..5).map(|_| rng2.random()).collect();
        assert_eq!(nums1, nums2);
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut rng1 = DeterministicRNG::new(42);
        let mut rng2 = DeterministicRNG::new(99);
        let n1 = rng1.random();
        let n2 = rng2.random();
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_rng_randint() {
        let mut rng = DeterministicRNG::new(42);
        for _ in 0..100 {
            let n = rng.randint(1, 6);
            assert!((1..=6).contains(&n));
        }
    }

    #[test]
    fn test_rng_choice() {
        let mut rng = DeterministicRNG::new(42);
        let items = vec![1, 2, 3];
        let c = rng.choice(&items);
        assert!(c.is_some());
        assert!(items.contains(&c.unwrap()));
    }

    /// Test content_hash: nested object determinism
    #[test]
    fn test_content_hash_nested_object() {
        let nested = Value::from(im::hashmap! {
            "outer".to_string() => Value::from(im::hashmap! {
                "inner".to_string() => Value::Integer(42),
            }),
        });
        let h1 = content_hash(&[nested.clone()]);
        let h2 = content_hash(&[nested.clone()]);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    /// Test content_hash: list order sensitivity
    #[test]
    fn test_content_hash_list_order_sensitive() {
        let list1 = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        let list2 = Value::list(vec![Value::Integer(2), Value::Integer(1)]);
        let h1 = content_hash(&[list1]);
        let h2 = content_hash(&[list2]);
        assert_ne!(h1, h2); // Different order should yield different hashes
    }

    /// Test content_hash: Bytes type
    #[test]
    fn test_content_hash_bytes() {
        let bytes = Value::Bytes(vec![0x01, 0x02, 0x03]);
        let h = content_hash(&[bytes]);
        assert_eq!(h.len(), 64);
    }

    /// Test content_hash: multiple values combined
    #[test]
    fn test_content_hash_multiple_values() {
        let vals = vec![
            Value::Integer(42),
            Value::string("hello"),
            Value::Bool(true),
        ];
        let h = content_hash(&vals);
        assert_eq!(h.len(), 64);
    }

    /// Test content_hash: empty list and empty object
    #[test]
    fn test_content_hash_empty_collections() {
        let empty_list = Value::empty_list();
        let empty_obj = Value::empty_object();
        let h1 = content_hash(&[empty_list]);
        let h2 = content_hash(&[empty_obj]);
        assert_ne!(h1, h2); // Different types should yield different hashes
    }

    /// Test rng.choice: empty list returns None
    #[test]
    fn test_rng_choice_empty() {
        let mut rng = DeterministicRNG::new(42);
        let items: Vec<i32> = vec![];
        let c = rng.choice(&items);
        assert!(c.is_none());
    }

    /// Test rng.shuffle: deterministic shuffling
    #[test]
    fn test_rng_shuffle_deterministic() {
        let mut rng1 = DeterministicRNG::new(42);
        let mut rng2 = DeterministicRNG::new(42);
        let mut items1 = vec![1, 2, 3, 4, 5];
        let mut items2 = vec![1, 2, 3, 4, 5];
        rng1.shuffle(&mut items1);
        rng2.shuffle(&mut items2);
        assert_eq!(items1, items2);
    }

    /// Test rng.from_content_hash: seeding from hash
    #[test]
    fn test_rng_from_content_hash() {
        let hash = "abc123def456";
        let mut rng1 = DeterministicRNG::from_content_hash(hash);
        let mut rng2 = DeterministicRNG::from_content_hash(hash);
        let n1 = rng1.random();
        let n2 = rng2.random();
        assert_eq!(n1, n2);
    }

    /// Test LogicalClock: reset functionality
    #[test]
    fn test_logical_clock_reset() {
        let mut clock = LogicalClock::new();
        clock.tick();
        clock.tick();
        assert_eq!(clock.current_tick(), 2);
        clock.reset();
        assert_eq!(clock.current_tick(), 0);
    }

    /// Test LogicalClock: from_tick creation
    #[test]
    fn test_logical_clock_from_tick() {
        let mut clock = LogicalClock::from_tick(100);
        assert_eq!(clock.current_tick(), 100);
        assert_eq!(clock.tick(), 100); // Next tick is 100
    }

    // ═══════════════════════════════════════════════════════════════════════
    // P2: Property-based tests (proptest)
    // ═══════════════════════════════════════════════════════════════════════
    // Collision regression tests — verify NO hash collisions exist between
    // structurally different values that produce similar byte patterns.
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_no_collision_string_in_list() {
        // String "a\x04b" in a single-element list vs two strings "a","b"
        // \x04 is the String type tag; without length prefix, boundary is ambiguous
        let single = Value::list(vec![Value::string("a\u{0004}b")]);
        let double = Value::list(vec![Value::string("a"), Value::string("b")]);
        assert_ne!(
            content_hash(&[single]),
            content_hash(&[double]),
            "Collision: List([String(\"a\\x04b\")]) == List([String(\"a\"), String(\"b\")])"
        );
    }

    #[test]
    fn test_no_collision_bytes_in_list() {
        // Bytes [0x08, 0x61] in one element vs split across two elements
        // 0x08 is the Bytes type tag; without length prefix, boundary is ambiguous
        let single = Value::list(vec![Value::Bytes(vec![0x08, 0x61])]);
        let double = Value::list(vec![Value::Bytes(vec![]), Value::Bytes(vec![0x61])]);
        assert_ne!(
            content_hash(&[single]),
            content_hash(&[double]),
            "Collision: List([Bytes([0x08,0x61])]) == List([Bytes([]), Bytes([0x61])])"
        );
    }

    #[test]
    fn test_no_collision_object_key_with_separator() {
        // Key containing 0x00 (the key-value separator) vs key + value boundary
        // \x00 is the separator; keys can contain U+0000 in Rust strings
        let key_with_null = Value::from({
            let mut m = im::HashMap::new();
            m.insert("a\u{0000}".to_string(), Value::Null);
            m
        });
        let normal_key = Value::from({
            let mut m = im::HashMap::new();
            m.insert("a".to_string(), Value::Null);
            m
        });
        assert_ne!(
            content_hash(&[key_with_null]),
            content_hash(&[normal_key]),
            "Collision: Object(key with 0x00) == Object(normal key)"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Verify algebraic properties of content_hash that hold for arbitrary
    // inputs. These tests catch edge cases that example-based tests miss
    // (e.g., collisions on structurally different values, order sensitivity).

    use proptest::prelude::*;

    /// Generate an arbitrary Value (limited to deterministic, comparable types).
    /// Uses BoxedStrategy to allow limited recursion (lists of depth 1).
    fn arb_value() -> BoxedStrategy<Value> {
        let leaf: BoxedStrategy<Value> = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::Integer),
            any::<f64>().prop_map(|f| {
                // Avoid NaN/Inf which are normalized away by Value::float
                if f.is_finite() {
                    Value::float(f)
                } else {
                    Value::Integer(0)
                }
            }),
            any::<String>().prop_map(Value::string),
        ]
        .boxed();
        // Small lists of leaves only (depth 1) to avoid infinite recursion.
        // Clone leaf boxed strategy (cheap Arc clone).
        prop_oneof![
            leaf.clone(),
            prop::collection::vec(leaf, 0..5).prop_map(Value::list),
        ]
        .boxed()
    }

    #[test]
    fn prop_content_hash_idempotent() {
        proptest!(|(v in arb_value())| {
            let h1 = content_hash(&[v.clone()]);
            let h2 = content_hash(&[v.clone()]);
            prop_assert_eq!(h1, h2);
        });
    }

    #[test]
    fn prop_content_hash_list_idempotent() {
        proptest!(|(a in arb_value(), b in arb_value(), c in arb_value())| {
            let list1 = vec![a.clone(), b.clone(), c.clone()];
            let list2 = vec![a, b, c];
            prop_assert_eq!(content_hash(&list1), content_hash(&list2));
        });
    }

    #[test]
    fn prop_content_hash_order_sensitive_distinct() {
        proptest!(|(a in arb_value(), b in arb_value())| {
            let h1 = content_hash(&[a.clone(), b.clone()]);
            let h2 = content_hash(&[b.clone(), a.clone()]);
            let values_equal = values_structurally_equal(&a, &b);
            let hashes_equal = h1 == h2;
            prop_assert!(hashes_equal == values_equal);
        });
    }

    #[test]
    fn prop_content_hash_empty_constant() {
        let h1 = content_hash(&[]);
        let h2 = content_hash(&[]);
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn prop_content_hash_object_key_order_independent() {
        proptest!(|(k1 in any::<String>(), k2 in any::<String>(), v1 in arb_value(), v2 in arb_value())| {
            prop_assume!(k1 != k2);

            let mut m1 = im::HashMap::new();
            m1.insert(k1.clone(), v1.clone());
            m1.insert(k2.clone(), v2.clone());

            let mut m2 = im::HashMap::new();
            m2.insert(k2, v2);
            m2.insert(k1, v1);

            let h1 = content_hash(&[Value::from(m1)]);
            let h2 = content_hash(&[Value::from(m2)]);
            prop_assert_eq!(h1, h2);
        });
    }

    #[test]
    fn prop_content_hash_repeated_stable() {
        proptest!(|(v in arb_value())| {
            let h1 = content_hash(&[v.clone()]);
            let h2 = content_hash(&[v.clone()]);
            let h3 = content_hash(&[v]);
            prop_assert_eq!(h1, h2.clone());
            prop_assert_eq!(h2, h3);
        });
    }

    /// Helper: structural equality for property 3.
    /// (Cannot use PartialEq on Value directly because f64 NaN != NaN; we use
    ///  the same canonicalization that content_hash uses.)
    fn values_structurally_equal(a: &Value, b: &Value) -> bool {
        content_hash(&[a.clone()]) == content_hash(&[b.clone()])
    }

    /// Property 7: No collision — List([s]) vs List split of s at any byte
    /// that equals a type tag. Catches boundary ambiguity in variable-length
    /// types (String, Bytes) without length prefixes.
    #[test]
    fn prop_no_collision_list_split() {
        proptest!(|(s in any::<String>())| {
            let single = Value::list(vec![Value::string(s.clone())]);
            let bytes = s.as_bytes();
            for i in 1..bytes.len() {
                let left = String::from_utf8_lossy(&bytes[..i]).to_string();
                let right = String::from_utf8_lossy(&bytes[i..]).to_string();
                // Skip invalid UTF-8 splits
                if left.contains('\u{FFFD}') || right.contains('\u{FFFD}') {
                    continue;
                }
                let double = Value::list(vec![Value::string(left), Value::string(right)]);
                let h1 = content_hash(&[single.clone()]);
                let h2 = content_hash(&[double]);
                prop_assert_ne!(h1, h2, "collision at split position {}", i);
            }
        });
    }

    /// Property 8: No collision — Bytes([tag, ...]) vs Bytes([], Bytes([...]))
    /// Catches boundary ambiguity in Bytes without length prefix.
    #[test]
    fn prop_no_collision_bytes_split() {
        proptest!(|(b in prop::collection::vec(any::<u8>(), 2..10))| {
            let single = Value::list(vec![Value::Bytes(b.clone())]);
            for i in 1..b.len() {
                let left = b[..i].to_vec();
                let right = b[i..].to_vec();
                let double = Value::list(vec![Value::Bytes(left), Value::Bytes(right)]);
                let h1 = content_hash(&[single.clone()]);
                let h2 = content_hash(&[double]);
                prop_assert_ne!(h1, h2, "bytes collision at split position {}", i);
            }
        });
    }

    /// Property 9: State data iteration order is deterministic (P6 compliance).
    /// Verify that iterating over state.data() produces consistent order across multiple runs.
    #[test]
    fn prop_state_data_iter_order_deterministic() {
        proptest!(|(size in 0..20)| {
            use crate::state::State;
            let mut map = im::HashMap::new();
            for i in 0..size {
                map.insert(format!("key_{}", i), Value::Integer(i as i64));
            }
            let state = State::from_im_map(map);

            let results: Vec<Vec<String>> = (0..100).map(|_| {
                state.data().iter_sorted().map(|(k, _)| k.clone()).collect()
            }).collect();

            for i in 1..100 {
                prop_assert_eq!(&results[0], &results[i], "State iteration order changed at run {}", i);
            }
        });
    }

    /// Property 10: Value::Object Display format is deterministic (P6 compliance).
    /// Verify that Display for Object values produces consistent output across multiple runs.
    #[test]
    fn prop_value_object_display_deterministic() {
        proptest!(|(size in 0..20)| {
            let mut map = im::HashMap::new();
            for i in 0..size {
                map.insert(format!("key_{}", i), Value::Integer(i as i64));
            }
            let value = Value::Object(map);

            let results: Vec<String> = (0..100).map(|_| format!("{}", value)).collect();

            for i in 1..100 {
                prop_assert_eq!(&results[0], &results[i], "Object display format changed at run {}", i);
            }
        });
    }

    /// Property 11: iter_std_hashmap_sorted produces deterministic order (P6 compliance).
    #[test]
    fn prop_std_hashmap_iter_sorted_deterministic() {
        proptest!(|(size in 0..20)| {
            let mut map = std::collections::HashMap::new();
            for i in 0..size {
                map.insert(format!("key_{}", i), Value::Integer(i as i64));
            }

            let results: Vec<Vec<(String, Value)>> = (0..100).map(|_| {
                crate::value::iter_std_hashmap_sorted(&map)
                    .into_iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            }).collect();

            for i in 1..100 {
                prop_assert_eq!(&results[0], &results[i], "StdHashMap sorted iteration order changed at run {}", i);
            }
        });
    }
}
