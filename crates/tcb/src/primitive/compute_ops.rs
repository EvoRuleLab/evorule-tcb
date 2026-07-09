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

//! Compute primitives — Pure computation operations.
//!
//! # Core Functions
//!
//! - `content_hash`: Content hash computation (SHA-256, deterministic).
//! - `get_index`: List index access (supports negative indices).
//! - `object_keys`: Extract sorted keys from an object.
//! - `set_intersection`: Compute intersection of two string lists.
//! - `set_diff`: Compute set difference (list_a minus list_b).
//! - `set_union`: Compute union of two string lists.
//!
//! # Design Principles
//!
//! ## `get_index`: Negative Index Support
//!
//! Negative indices count from the end of the list: `-1` → last element, `-2` → second last.
//! Out-of-bounds indices return `Value::Null` (no panic).
//!
//! # Determinism Guarantee
//!
//! All compute primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - `content_hash`: SHA-256 (FIPS 180-4) with deterministic serialization.
//! - `get_index`: Deterministic list access (pure indexing).
//! - `object_keys`: Deterministic sorted key extraction (ER-601 compliant).
//! - Set operations: Deterministic sorted output (ER-601 compliant).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `content_hash` SHA-256 | ✅ L1 deterministic | FIPS 180-4 |
//! | `content_hash` key selection | ✅ L1 deterministic | Pure filtering |
//! | `get_index` index resolution | ✅ L1 deterministic | Integer arithmetic |
//! | `get_index` negative index | ✅ L1 deterministic | List length + index |
//! | `get_index` out-of-bounds | ✅ L1 deterministic | Returns `Value::Null` |
//! | `object_keys` sorting | ✅ L1 deterministic | Lexicographic sort |
//! | Set operations sorting | ✅ L1 deterministic | Lexicographic sort |
//!
//! # Cross-Language Note (L4)
//!
//! To replicate `content_hash` in other languages, see the encoding specification
//! in `deterministic.rs`.
//!
//! # Moved Out of TCB
//!
//! `format_string` was moved to the Governance layer because it performs text
//! template replacement, which violates the TCB's JSON-structured-data-only
//! design principle. See `governance-core/src/io/primitives.rs` for the migration.

use crate::control::dispatch::resolve_path;
use crate::deterministic::content_hash;
use crate::error::EvoRuleError;
use crate::exec_ctl_ctx::ExecCtlCtx;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register computation primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("content_hash", exec_content_hash);
    reg.register("get_index", exec_get_index);
    reg.register("object_keys", exec_object_keys);
    reg.register("set_intersection", exec_set_intersection);
    reg.register("set_diff", exec_set_diff);
    reg.register("set_union", exec_set_union);
}

// ══════════════════════════════════════════════
// content_hash
// ══════════════════════════════════════════════

/// Content hash — compute a deterministic hash of state content.
///
/// # Parameters
/// - `keys` (optional): List of keys to hash. If empty, hashes the entire state.
/// - `store_as` (optional): Attribute to store the hash (default: `"__hash__"`).
///
/// # Behavior
/// - If `keys` is empty, hashes the entire `State`.
/// - If `keys` is provided, hashes only the specified keys (missing keys use `Value::Null`).
/// - Non-String keys are filtered out.
/// - The hash is a SHA-256 hex string (64 characters).
///
/// # Example
/// ```json
/// { "type": "content_hash", "params": { "keys": ["x", "y"], "store_as": "hash" } }
/// ```
pub(crate) fn exec_content_hash(
    _reg: &crate::instruction::registry::InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let keys = instruction
        .params
        .get("keys")
        .and_then(|v| v.as_list())
        .cloned()
        .unwrap_or(im::Vector::new());

    let store_as = instruction
        .params
        .get("store_as")
        .and_then(|v| v.as_str())
        .unwrap_or("__hash__");

    let values: Vec<Value> = if keys.is_empty() {
        // Hash the entire State
        vec![state.to_value()]
    } else {
        // Hash only the specified keys
        keys.iter()
            .filter_map(|k| k.as_str())
            .map(|k| state.get(k).cloned().unwrap_or(Value::Null))
            .collect()
    };

    let hash = content_hash(&values);
    state.set_path(store_as, Value::string(hash))
}

// ══════════════════════════════════════════════
// get_index
// ══════════════════════════════════════════════

/// List index access — gets the element at the specified index in a list.
///
/// # Parameters
/// - `ref`: Reference to the list (resolved via `$ref`).
/// - `index`: Index to access (supports negative indices).
/// - `result_attr` (optional): Attribute to store the result (default: `"item"`).
///
/// # Behavior
/// - Positive indices count from the start (0-based).
/// - Negative indices count from the end: `-1` → last element.
/// - Out-of-bounds indices return `Value::Null`.
/// - If `ref` does not point to a list, returns `Value::Null`.
///
/// # Example
/// ```json
/// {
///   "type": "get_index",
///   "params": {
///     "ref": "items",
///     "index": 0,
///     "result_attr": "first_item"
///   }
/// }
/// ```
pub(crate) fn exec_get_index(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let ref_val = instruction
        .params
        .get("ref")
        .cloned()
        .unwrap_or(Value::Null);
    let ref_str = resolve_path(state, &ref_val);

    let index = instruction
        .params
        .get("index")
        .and_then(|v| {
            let resolved = resolve_path(state, v);
            if resolved.is_integer() {
                resolved.as_integer()
            } else if let Some(s) = resolved.as_str() {
                // Supports "$attr" format references
                let path = s.strip_prefix('$').unwrap_or(s);
                state.get_path(path).and_then(|val| val.as_integer())
            } else {
                None
            }
        })
        .unwrap_or(0);

    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "item".to_string());

    let list = match ref_str {
        Value::String(ref s) => {
            let path = s.strip_prefix('$').unwrap_or(s);
            state.get_path(path)
        }
        _ => None,
    };

    let item = list
        .and_then(|v| {
            v.as_list().and_then(|list| {
                let idx = if index < 0 {
                    (list.len() as i64 + index) as usize
                } else {
                    index as usize
                };
                list.get(idx).cloned()
            })
        })
        .unwrap_or(Value::Null);

    state.set_path(&result_attr, item)
}

// ══════════════════════════════════════════════
// object_keys
// ══════════════════════════════════════════════

/// Extract keys from a Value::Object into a sorted list.
///
/// # Parameters
/// - `ref`: Reference to the object (resolved via `$ref`).
/// - `result_attr` (optional): Attribute to store the result (default: `"keys"`).
///
/// # Behavior
/// - Returns a sorted list of keys from the object.
/// - ER-601: Keys are sorted lexicographically for deterministic output order.
/// - Non-object values (List, Integer, String, Null) return an empty list.
/// - Empty objects return an empty list.
///
/// # Example
/// ```json
/// {
///   "type": "object_keys",
///   "params": {
///     "ref": "_transform",
///     "result_attr": "_field_list"
///   }
/// }
/// ```
pub(crate) fn exec_object_keys(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let ref_val = instruction
        .params
        .get("ref")
        .cloned()
        .unwrap_or(Value::Null);
    let ref_str = resolve_path(state, &ref_val);

    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "keys".to_string());

    let obj_value = match ref_str {
        Value::String(ref s) => {
            let path = s.strip_prefix('$').unwrap_or(s);
            state.get_path(path).unwrap_or(Value::Null)
        }
        _ => ref_str,
    };

    let keys = match obj_value {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            // ER-601: Sort keys for deterministic output order.
            keys.sort();
            Value::list(keys.into_iter().map(Value::string).collect())
        }
        _ => Value::list(vec![]),
    };

    state.set_path(&result_attr, keys)
}

// ══════════════════════════════════════════════
// set_intersection
// ══════════════════════════════════════════════

pub(crate) fn exec_set_intersection(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let list_a = extract_string_list(state, instruction, "list_a");
    let list_b = extract_string_list(state, instruction, "list_b");
    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "intersection".to_string());

    let set_a: std::collections::HashSet<_> = list_a.into_iter().collect();
    let intersection: Vec<String> = list_b.into_iter().filter(|x| set_a.contains(x)).collect();

    // ER-601: Sort for deterministic output order.
    let mut result = intersection;
    result.sort();

    state.set_path(
        &result_attr,
        Value::list(result.into_iter().map(Value::string).collect()),
    )
}

// ══════════════════════════════════════════════
// set_diff
// ══════════════════════════════════════════════

pub(crate) fn exec_set_diff(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let list_a = extract_string_list(state, instruction, "list_a");
    let list_b = extract_string_list(state, instruction, "list_b");
    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "diff".to_string());

    let set_b: std::collections::HashSet<_> = list_b.into_iter().collect();
    let diff: Vec<String> = list_a.into_iter().filter(|x| !set_b.contains(x)).collect();

    // ER-601: Sort for deterministic output order.
    let mut result = diff;
    result.sort();

    state.set_path(
        &result_attr,
        Value::list(result.into_iter().map(Value::string).collect()),
    )
}

// ══════════════════════════════════════════════
// set_union
// ══════════════════════════════════════════════

pub(crate) fn exec_set_union(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let list_a = extract_string_list(state, instruction, "list_a");
    let list_b = extract_string_list(state, instruction, "list_b");
    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "union".to_string());

    let mut set: std::collections::HashSet<_> = list_a.into_iter().collect();
    set.extend(list_b);

    // ER-601: Sort for deterministic output order.
    let mut result: Vec<String> = set.into_iter().collect();
    result.sort();

    state.set_path(
        &result_attr,
        Value::list(result.into_iter().map(Value::string).collect()),
    )
}

// ══════════════════════════════════════════════
// Helper: extract_string_list
// ══════════════════════════════════════════════

fn extract_string_list(
    state: &State,
    instruction: &GenericInstruction,
    param_name: &str,
) -> Vec<String> {
    let ref_val = instruction
        .params
        .get(param_name)
        .cloned()
        .unwrap_or(Value::Null);
    let resolved = resolve_path(state, &ref_val);

    match resolved {
        Value::String(ref s) => {
            let path = s.strip_prefix('$').unwrap_or(s);
            match state.get_path(path) {
                Some(Value::List(list)) => list
                    .iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect(),
                _ => Vec::new(),
            }
        }
        Value::List(list) => list
            .iter()
            .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_exec_content_hash() {
        let _reg = crate::instruction::registry::InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);
        let instr = GenericInstruction::simple("content_hash");
        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(&_reg, &state, &instr, &mut ctx).unwrap();
        let hash = result.get("__hash__").and_then(|v| v.as_str()).unwrap();
        assert_eq!(hash.len(), 64); // SHA-256
    }

    #[test]
    fn test_exec_content_hash_with_keys() {
        let state = State::new(vec![
            ("name", Value::string("test")),
            ("value", Value::Integer(42)),
        ]);
        let params = HashMap::from([
            ("keys".to_string(), Value::list(vec![Value::string("name")])),
            ("store_as".to_string(), Value::string("hash")),
        ]);
        let instr = GenericInstruction::new("content_hash", params);
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(&reg, &state, &instr, &mut ctx).unwrap();
        assert!(result.get("hash").is_some());
        let hash = result.get("hash").unwrap().as_str().unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_get_index_negative_index_from_end() {
        let state = State::new(vec![(
            "arr",
            Value::list(vec![
                Value::Integer(10),
                Value::Integer(20),
                Value::Integer(30),
            ]),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("arr"));
            p.insert("index".to_string(), Value::Integer(-1));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(result.get("item"), Some(&Value::Integer(30)));
    }

    #[test]
    fn test_get_index_out_of_bounds_returns_null() {
        let state = State::new(vec![(
            "arr",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("arr"));
            p.insert("index".to_string(), Value::Integer(100));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(result.get("item"), Some(&Value::Null));
    }

    #[test]
    fn test_get_index_default_result_attr() {
        let state = State::new(vec![("arr", Value::list(vec![Value::Integer(42)]))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("arr"));
            p.insert("index".to_string(), Value::Integer(0));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(result.get("item"), Some(&Value::Integer(42)));
    }

    // ─── P1: Error path and boundary tests ─────────────────────────────────
    // Verify graceful handling of missing params, out-of-bounds indices, and
    // type mismatches. Aligns with ER-600 (no runtime panics).

    #[test]
    fn test_get_index_out_of_bounds_positive_returns_null() {
        // Index beyond list length → returns Null (no panic)
        let state = State::new(vec![(
            "arr",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("arr"));
            p.insert("index".to_string(), Value::Integer(10));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(
            result.get("item"),
            Some(&Value::Null),
            "out-of-bounds positive index should return Null, not panic"
        );
    }

    #[test]
    fn test_get_index_out_of_bounds_negative_returns_null() {
        // Negative index beyond list length → returns Null (no panic)
        let state = State::new(vec![(
            "arr",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("arr"));
            p.insert("index".to_string(), Value::Integer(-100));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(
            result.get("item"),
            Some(&Value::Null),
            "out-of-bounds negative index should return Null, not panic"
        );
    }

    #[test]
    fn test_get_index_ref_points_to_non_list_returns_null() {
        // ref points to a non-list value (Integer) → returns Null (no panic)
        let state = State::new(vec![("not_arr", Value::Integer(42))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("ref".to_string(), Value::string("not_arr"));
            p.insert("index".to_string(), Value::Integer(0));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(
            result.get("item"),
            Some(&Value::Null),
            "ref pointing to non-list should return Null, not panic"
        );
    }

    #[test]
    fn test_get_index_missing_ref_returns_null() {
        // Missing "ref" param → returns Null (no panic)
        let state = State::empty();
        let params = {
            let mut p = HashMap::new();
            p.insert("index".to_string(), Value::Integer(0));
            p
        };
        let instr = GenericInstruction::new("get_index", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_get_index(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(
            result.get("item"),
            Some(&Value::Null),
            "missing ref should return Null, not panic"
        );
    }

    #[test]
    fn test_content_hash_missing_keys_hashes_entire_state() {
        // Missing "keys" param → hashes entire state (no panic)
        let state = State::new(vec![("x", Value::Integer(10))]);
        let instr = GenericInstruction::new("content_hash", HashMap::new());

        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        // Should produce a hash string at default __hash__ attr
        let hash = result.get("__hash__").and_then(|v| v.as_str());
        assert!(
            hash.is_some() && !hash.unwrap().is_empty(),
            "missing keys should hash entire state and produce non-empty hash"
        );
    }

    #[test]
    fn test_content_hash_keys_with_non_string_elements_filters_them() {
        // keys contains non-String elements → should be filtered out, no panic
        let state = State::new(vec![
            ("a", Value::string("hello")),
            ("b", Value::Integer(42)),
        ]);
        let params = {
            let mut p = HashMap::new();
            // Mixed: "a" (valid), 123 (invalid), "b" (valid)
            p.insert(
                "keys".to_string(),
                Value::list(vec![
                    Value::string("a"),
                    Value::Integer(123),
                    Value::string("b"),
                ]),
            );
            p
        };
        let instr = GenericInstruction::new("content_hash", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        let hash = result.get("__hash__").and_then(|v| v.as_str());
        assert!(
            hash.is_some() && !hash.unwrap().is_empty(),
            "non-String keys should be filtered, still producing valid hash"
        );
    }

    #[test]
    fn test_content_hash_keys_as_non_list_treats_as_empty() {
        // keys is a non-List value (Integer) → treated as empty, hashes entire state
        let state = State::new(vec![("x", Value::Integer(10))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("keys".to_string(), Value::Integer(42));
            p
        };
        let instr = GenericInstruction::new("content_hash", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        let hash = result.get("__hash__").and_then(|v| v.as_str());
        assert!(
            hash.is_some() && !hash.unwrap().is_empty(),
            "non-List keys should be treated as empty, hashing entire state"
        );
    }

    #[test]
    fn test_content_hash_missing_key_in_state_uses_null() {
        // keys references a key not in state → uses Null for that key
        let state = State::new(vec![("a", Value::string("hello"))]);
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "keys".to_string(),
                Value::list(vec![
                    Value::string("a"),
                    Value::string("missing_key"), // not in state
                ]),
            );
            p
        };
        let instr = GenericInstruction::new("content_hash", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_content_hash(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        let hash = result.get("__hash__").and_then(|v| v.as_str());
        assert!(
            hash.is_some() && !hash.unwrap().is_empty(),
            "missing key in state should use Null, still producing valid hash"
        );
    }

    #[test]
    fn test_exec_object_keys() {
        let state = State::new(vec![(
            "_transform",
            Value::from(im::HashMap::from(vec![
                ("field_b".to_string(), Value::Null),
                ("field_a".to_string(), Value::Null),
                ("field_c".to_string(), Value::Null),
            ])),
        )]);
        let params = HashMap::from([
            ("ref".to_string(), Value::string("_transform")),
            ("result_attr".to_string(), Value::string("_keys")),
        ]);
        let instr = GenericInstruction::new("object_keys", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_object_keys(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let keys = result.get("_keys").unwrap();
        assert!(keys.is_list());
        let key_list = keys.as_list().unwrap();
        assert_eq!(key_list.len(), 3);
        assert_eq!(key_list[0].as_str(), Some("field_a"));
        assert_eq!(key_list[1].as_str(), Some("field_b"));
        assert_eq!(key_list[2].as_str(), Some("field_c"));
    }

    #[test]
    fn test_exec_object_keys_empty_object() {
        let state = State::new(vec![("_empty", Value::from(im::HashMap::new()))]);
        let params = HashMap::from([
            ("ref".to_string(), Value::string("_empty")),
            ("result_attr".to_string(), Value::string("_keys")),
        ]);
        let instr = GenericInstruction::new("object_keys", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_object_keys(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let keys = result.get("_keys").unwrap();
        assert!(keys.is_list());
        assert!(keys.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_exec_object_keys_non_object() {
        let state = State::new(vec![(
            "_list",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        )]);
        let params = HashMap::from([
            ("ref".to_string(), Value::string("_list")),
            ("result_attr".to_string(), Value::string("_keys")),
        ]);
        let instr = GenericInstruction::new("object_keys", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_object_keys(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let keys = result.get("_keys").unwrap();
        assert!(keys.is_list());
        assert!(keys.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_exec_object_keys_null() {
        let state = State::new(vec![("_null", Value::Null)]);
        let params = HashMap::from([
            ("ref".to_string(), Value::string("_null")),
            ("result_attr".to_string(), Value::string("_keys")),
        ]);
        let instr = GenericInstruction::new("object_keys", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_object_keys(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let keys = result.get("_keys").unwrap();
        assert!(keys.is_list());
        assert!(keys.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_exec_set_intersection() {
        let state = State::new(vec![
            (
                "_a",
                Value::list(vec![
                    Value::string("x"),
                    Value::string("y"),
                    Value::string("z"),
                ]),
            ),
            (
                "_b",
                Value::list(vec![
                    Value::string("y"),
                    Value::string("z"),
                    Value::string("w"),
                ]),
            ),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
            ("result_attr".to_string(), Value::string("_intersection")),
        ]);
        let instr = GenericInstruction::new("set_intersection", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_intersection(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let intersection = result.get("_intersection").unwrap();
        assert!(intersection.is_list());
        let list = intersection.as_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].as_str(), Some("y"));
        assert_eq!(list[1].as_str(), Some("z"));
    }

    #[test]
    fn test_exec_set_diff() {
        let state = State::new(vec![
            (
                "_a",
                Value::list(vec![
                    Value::string("x"),
                    Value::string("y"),
                    Value::string("z"),
                ]),
            ),
            (
                "_b",
                Value::list(vec![
                    Value::string("y"),
                    Value::string("z"),
                    Value::string("w"),
                ]),
            ),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
            ("result_attr".to_string(), Value::string("_diff")),
        ]);
        let instr = GenericInstruction::new("set_diff", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_diff(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let diff = result.get("_diff").unwrap();
        assert!(diff.is_list());
        let list = diff.as_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].as_str(), Some("x"));
    }

    #[test]
    fn test_exec_set_union() {
        let state = State::new(vec![
            (
                "_a",
                Value::list(vec![
                    Value::string("x"),
                    Value::string("y"),
                    Value::string("z"),
                ]),
            ),
            (
                "_b",
                Value::list(vec![
                    Value::string("y"),
                    Value::string("z"),
                    Value::string("w"),
                ]),
            ),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
            ("result_attr".to_string(), Value::string("_union")),
        ]);
        let instr = GenericInstruction::new("set_union", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_union(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let union = result.get("_union").unwrap();
        assert!(union.is_list());
        let list = union.as_list().unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].as_str(), Some("w"));
        assert_eq!(list[1].as_str(), Some("x"));
        assert_eq!(list[2].as_str(), Some("y"));
        assert_eq!(list[3].as_str(), Some("z"));
    }

    #[test]
    fn test_exec_set_intersection_empty() {
        let state = State::new(vec![
            ("_a", Value::list(vec![Value::string("x")])),
            ("_b", Value::list(vec![Value::string("y")])),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
        ]);
        let instr = GenericInstruction::new("set_intersection", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_intersection(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let intersection = result.get("intersection").unwrap();
        assert!(intersection.is_list());
        assert!(intersection.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_exec_set_diff_empty_list_a() {
        let state = State::new(vec![
            ("_a", Value::list(vec![])),
            ("_b", Value::list(vec![Value::string("y")])),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
        ]);
        let instr = GenericInstruction::new("set_diff", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_diff(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let diff = result.get("diff").unwrap();
        assert!(diff.is_list());
        assert!(diff.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_exec_set_union_empty_lists() {
        let state = State::new(vec![
            ("_a", Value::list(vec![])),
            ("_b", Value::list(vec![])),
        ]);
        let params = HashMap::from([
            ("list_a".to_string(), Value::string("_a")),
            ("list_b".to_string(), Value::string("_b")),
        ]);
        let instr = GenericInstruction::new("set_union", params);

        let mut ctx = ExecCtlCtx::new();
        let result = exec_set_union(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();

        let union = result.get("union").unwrap();
        assert!(union.is_list());
        assert!(union.as_list().unwrap().is_empty());
    }
}
