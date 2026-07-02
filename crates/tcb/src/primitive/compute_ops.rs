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
//! - `format_string`: Template string replacement with `{key}` placeholders.
//! - `get_index`: List index access (supports negative indices).
//!
//! # Design Principles
//!
//! ## `format_string`: Strict Preservation Redline
//!
//! Only `Value::String` typed values are substituted. For any other case:
//! - Non-String value types (Integer, Bool, Float, List, Object)
//! - Missing keys
//! - `Value::Null`
//!
//! The **original placeholder bytes** are preserved verbatim (e.g., `{age}` stays `{age}`).
//!
//! This guarantees:
//! 1. No implicit type coercion (Integer/Bool/List never become `"42"` or `"true"`)
//! 2. No information loss from Debug formatting
//! 3. Original placeholder bytes are preserved exactly (safe for keys containing
//!    special characters like `{` or `}`)
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
//! - `format_string`: Deterministic string replacement (pure algorithm).
//! - `get_index`: Deterministic list access (pure indexing).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `content_hash` SHA-256 | ✅ L1 deterministic | FIPS 180-4 |
//! | `content_hash` key selection | ✅ L1 deterministic | Pure filtering |
//! | `format_string` replacement | ✅ L1 deterministic | Deterministic algorithm |
//! | `format_string` strict preservation | ✅ L1 deterministic | Type-based branch |
//! | `get_index` index resolution | ✅ L1 deterministic | Integer arithmetic |
//! | `get_index` negative index | ✅ L1 deterministic | List length + index |
//! | `get_index` out-of-bounds | ✅ L1 deterministic | Returns `Value::Null` |
//!
//! # Cross-Language Note (L4)
//!
//! To replicate `content_hash` in other languages, see the encoding specification
//! in `deterministic.rs`. `format_string` and `get_index` are Rust-only constructs
//! but their semantics are JSON-compatible.

use crate::control::dispatch::resolve_path;
use crate::deterministic::content_hash;
use crate::error::EvoRuleError;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register computation primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("content_hash", exec_content_hash);
    reg.register("format_string", exec_format_string);
    reg.register("get_index", exec_get_index);
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
    Ok(state.set_path(store_as, Value::string(hash)))
}

// ══════════════════════════════════════════════
// format_string
// ══════════════════════════════════════════════

/// Template string replacement — parses `{key}` placeholders and replaces them.
///
/// # Parameters
/// - `template`: The template string with `{key}` placeholders.
/// - `store_as` (optional): Attribute to store the result (default: `"formatted"`).
/// - `source` (optional): Object to use as the value source (overrides state).
///
/// # Strict Preservation Redline
///
/// Only `Value::String` typed values are substituted. For any other case
/// (non-String value types, missing keys, or `Value::Null`), the **original
/// placeholder bytes** are preserved verbatim (e.g., `{age}` stays as `{age}`).
///
/// This guarantees:
/// 1. No implicit type coercion (Integer/Bool/List never become `"Integer(42)"` etc.)
/// 2. No information loss from Debug formatting
/// 3. Original placeholder bytes are preserved exactly (safe for keys containing
///    special characters like `{` or `}`)
///
/// # Example
/// ```json
/// {
///   "type": "format_string",
///   "params": {
///     "template": "Hello {name}, score={score}",
///     "store_as": "message"
///   }
/// }
/// ```
pub(crate) fn exec_format_string(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    let template = instruction
        .params
        .get("template")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_default();

    let store_as = instruction
        .params
        .get("store_as")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "formatted".to_string());

    let source = instruction.params.get("source").cloned();

    let mut result = template;
    let mut start = 0;
    let mut replacements = Vec::new();
    while let Some(open) = result[start..].find('{') {
        let abs_open = start + open;
        if let Some(close) = result[abs_open..].find('}') {
            let key = &result[abs_open + 1..abs_open + close];
            let value = if let Some(ref src) = source {
                src.get(key).cloned()
            } else {
                state.get(key).cloned()
            };
            // Strict preservation: only String values are substituted.
            // Non-String types (Integer/Bool/Float/List/Object/Null) and missing keys
            // preserve the original placeholder bytes verbatim.
            let original_placeholder = &result[abs_open..=(abs_open + close)];
            let replacement = match value {
                Some(Value::String(s)) => s,
                _ => original_placeholder.to_string(),
            };
            replacements.push((abs_open, abs_open + close + 1, replacement));
            start = abs_open + close + 1;
        } else {
            break;
        }
    }

    for (from, to, replacement) in replacements.into_iter().rev() {
        result.replace_range(from..to, &replacement);
    }

    Ok(state.set_path(&store_as, Value::string(&result)))
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

    Ok(state.set_path(&result_attr, item))
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
        let result = exec_content_hash(&_reg, &state, &instr).unwrap();
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
        let result = exec_content_hash(&reg, &state, &instr).unwrap();
        assert!(result.get("hash").is_some());
        let hash = result.get("hash").unwrap().as_str().unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_format_string_basic_placeholders() {
        let state = State::new(vec![
            ("name", Value::string("Alice")),
            ("score", Value::string("95")),
        ]);
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "template".to_string(),
                Value::string("Hello {name}, score={score}"),
            );
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("Hello Alice, score=95"))
        );
    }

    #[test]
    fn test_format_string_missing_key_preserves_placeholder() {
        let state = State::new(vec![("name", Value::string("Bob"))]);
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "template".to_string(),
                Value::string("Hello {name}, age={age}"),
            );
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("Hello Bob, age={age}"))
        );
    }

    #[test]
    fn test_format_string_custom_store_attr() {
        let state = State::new(vec![("x", Value::string("42"))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("val={x}"));
            p.insert("store_as".to_string(), Value::string("output"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("output"), Some(&Value::string("val=42")));
    }

    #[test]
    fn test_format_string_source_object_overrides_state() {
        let state = State::new(vec![("name", Value::string("StateName"))]);
        let source = Value::from(im::HashMap::from(vec![(
            "name".to_string(),
            Value::string("SourceName"),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("name={name}"));
            p.insert("source".to_string(), source);
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("name=SourceName"))
        );
    }

    // ─── Strict preservation redline tests (ER-600 series) ───────────────
    // Only Value::String is substituted; all other types preserve {key} verbatim.

    #[test]
    fn test_format_string_integer_value_preserves_placeholder() {
        // Integer in state → {count} preserved, not "42" or "Integer(42)"
        let state = State::new(vec![("count", Value::Integer(42))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("count={count}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("count={count}"))
        );
    }

    #[test]
    fn test_format_string_float_value_preserves_placeholder() {
        let state = State::new(vec![("pi", Value::float(2.71))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("pi={pi}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("formatted"), Some(&Value::string("pi={pi}")));
    }

    #[test]
    fn test_format_string_bool_value_preserves_placeholder() {
        let state = State::new(vec![("flag", Value::Bool(true))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("flag={flag}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("formatted"), Some(&Value::string("flag={flag}")));
    }

    #[test]
    fn test_format_string_list_value_preserves_placeholder() {
        let state = State::new(vec![(
            "items",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("items={items}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("items={items}"))
        );
    }

    #[test]
    fn test_format_string_object_value_preserves_placeholder() {
        let state = State::new(vec![(
            "config",
            Value::from(im::HashMap::from(vec![(
                "k".to_string(),
                Value::string("v"),
            )])),
        )]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("cfg={config}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("cfg={config}"))
        );
    }

    #[test]
    fn test_format_string_null_value_preserves_placeholder() {
        // Explicit Null in state → {key} preserved (not "Null" or "null")
        let state = State::new(vec![("missing", Value::Null)]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("v={missing}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("formatted"), Some(&Value::string("v={missing}")));
    }

    #[test]
    fn test_format_string_mixed_string_and_non_string_in_same_template() {
        // Mixed: String key substitutes, non-String key preserves
        let state = State::new(vec![
            ("name", Value::string("Alice")),
            ("count", Value::Integer(42)),
            ("flag", Value::Bool(true)),
        ]);
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "template".to_string(),
                Value::string("{name}: count={count}, flag={flag}"),
            );
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("Alice: count={count}, flag={flag}"))
        );
    }

    #[test]
    fn test_format_string_source_null_value_preserves_placeholder() {
        // source provided with explicit Null → {key} preserved (not "Null")
        let state = State::empty();
        let source = Value::from(im::HashMap::from(vec![("name".to_string(), Value::Null)]));
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("name={name}"));
            p.insert("source".to_string(), source);
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("formatted"), Some(&Value::string("name={name}")));
    }

    #[test]
    fn test_format_string_source_integer_value_preserves_placeholder() {
        // source with non-String type → {key} preserved
        let state = State::empty();
        let source = Value::from(im::HashMap::from(vec![(
            "count".to_string(),
            Value::Integer(99),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("count={count}"));
            p.insert("source".to_string(), source);
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("count={count}"))
        );
    }

    #[test]
    fn test_format_string_original_placeholder_bytes_preserved_exactly() {
        // Verify original placeholder bytes are preserved (not reconstructed via format!)
        // Using a key that would behave differently if reconstructed
        let state = State::empty();
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("value={key}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        // Must be exactly "{key}" not "{key}" reconstructed with different bytes
        assert_eq!(result.get("formatted"), Some(&Value::string("value={key}")));
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(result.get("item"), Some(&Value::Integer(42)));
    }

    // ─── P1: Error path and boundary tests ─────────────────────────────────
    // Verify graceful handling of missing params, out-of-bounds indices, and
    // type mismatches. Aligns with ER-600 (no runtime panics).

    #[test]
    fn test_format_string_missing_template_returns_empty_string() {
        // Missing "template" param → unwrap_or_default() yields empty string
        let state = State::empty();
        let instr = GenericInstruction::new("format_string", HashMap::new());

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("")),
            "missing template should yield empty formatted string, not Err"
        );
    }

    #[test]
    fn test_format_string_missing_store_as_defaults_to_formatted() {
        // Missing "store_as" → defaults to "formatted"
        let state = State::new(vec![("name", Value::string("Alice"))]);
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("hi {name}"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("hi Alice")),
            "missing store_as should default to 'formatted'"
        );
    }

    #[test]
    fn test_format_string_unclosed_brace_preserved() {
        // Unclosed { brace → no replacement, brace preserved as-is
        let state = State::empty();
        let params = {
            let mut p = HashMap::new();
            p.insert("template".to_string(), Value::string("hello {world"));
            p
        };
        let instr = GenericInstruction::new("format_string", params);

        let result =
            exec_format_string(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        assert_eq!(
            result.get("formatted"),
            Some(&Value::string("hello {world")),
            "unclosed brace should be preserved verbatim"
        );
    }

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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_get_index(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_content_hash(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_content_hash(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_content_hash(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
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

        let result =
            exec_content_hash(&crate::primitive::make_test_registry(), &state, &instr).unwrap();
        let hash = result.get("__hash__").and_then(|v| v.as_str());
        assert!(
            hash.is_some() && !hash.unwrap().is_empty(),
            "missing key in state should use Null, still producing valid hash"
        );
    }
}
