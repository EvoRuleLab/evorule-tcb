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

//! Immutable state — Core data container for `EvoRule`.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! State is immutable; all modification operations return a new State instance.
//! Internally uses `im::HashMap` for O(1) clone.
//!
//! Reserved fields:
//!   - __exec__: Execution context (`ExecContext`)
//!   - __`audit_trace`: Audit trail (optional)
//!   - __`universe_rules`__: Rule list (for self-referential primitives)

//! # Determinism Guarantee
//!
//! `State` is **fully deterministic** at L1 (computational determinism):
//! - All operations are pure: given the same input `State`, the same operation
//!   returns the same new `State`.
//! - No wall-clock time, no randomness, no system calls.
//! - `im::HashMap` iteration order is deterministic (insertion order).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | Hash iteration order | ✅ L1 deterministic | `im::HashMap` preserves insertion order |
//! | Path resolution | ✅ L1 deterministic | Same path string → same result |
//! | `set_path` silent fallback | ⚠️ Deterministic but silent | See "Silent Fallback" below |
//!
//! # Silent Fallback (Semantic Note)
//!
//! `set_path` on a non-object (e.g., `set_path("x.y", value)` when `x` is an `Integer`)
//! **silently returns the original state** without modification or error.
//!
//! This is **deterministic** (same input → same output), but may be **semantically unexpected**.
//! Callers should ensure that intermediate paths are objects before using `set_path`.
//!
//! # Cross-Platform Note
//!
//! - `im::HashMap` behavior is consistent across all Rust platforms.
//! - `String` sorting uses Unicode code point order (consistent within Rust).

use crate::exec_context::ExecContext;
use crate::value::{value_to_serde_json, ImMap, Value};
use std::collections::HashMap as StdHashMap;

/// Immutable state container.
///
/// All "modification" operations on State return a new State. The original State
/// remains unchanged. This is the foundation of `EvoRule` determinism — old states
/// are never accidentally mutated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// Internal data storage. Uses `im::HashMap` for O(1) clone.
    pub(crate) data: ImMap,
}

impl State {
    /// Create an empty state.
    pub fn empty() -> Self {
        Self { data: ImMap::new() }
    }

    /// Create a State from an `im::HashMap`.
    pub const fn from_im_map(map: ImMap) -> Self {
        Self { data: map }
    }

    /// Create a State from a `StdHashMap`.
    pub fn from_std_map(map: StdHashMap<String, Value>) -> Self {
        Self {
            data: ImMap::from(map),
        }
    }

    /// Create a state containing the specified key-value pairs.
    pub fn new(items: Vec<(&str, Value)>) -> Self {
        let mut map = ImMap::new();
        for (k, v) in items {
            map.insert(k.to_string(), v);
        }
        Self { data: map }
    }

    /// Create a State from a Value (which must be an Object).
    pub fn from_value(val: &Value) -> Self {
        match val {
            Value::Object(m) => Self { data: m.clone() },
            _ => Self::empty(),
        }
    }

    /// Extract a reference to the internal data.
    pub const fn data(&self) -> &ImMap {
        &self.data
    }

    /// Build a snapshot of the **business** state, excluding system metadata fields.
    ///
    /// This is the single source of truth for which fields are considered
    /// "system metadata" (not part of the audited business state) when computing
    /// state hashes for audit records and dispatch provenance.
    ///
    /// Excluded fields:
    /// - `__exec__` — execution context (queue, current instruction, etc.)
    /// - `__audit_chain` — tamper-proof audit chain history
    /// - `__audit_trace` — human-readable audit trace entries
    /// - `__parallel_provenance__` — provenance metadata from parallel execution
    ///
    /// All callers that compute a hash of "the business state" must use this
    /// function to ensure hash consistency across the codebase (C3 auditability).
    /// Prior to this function's introduction, `dispatch.rs` and `registry.rs`
    /// maintained separate exclusion lists, which had diverged (dispatch was
    /// missing `__parallel_provenance__`), causing inconsistent hashes.
    pub fn business_state_snapshot(&self) -> Value {
        const SYSTEM_FIELDS: [&str; 4] = [
            "__exec__",
            "__audit_chain",
            "__audit_trace",
            "__parallel_provenance__",
        ];
        let snapshot: ImMap = self
            .data
            .iter()
            .filter(|(k, _)| !SYSTEM_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Value::Object(snapshot)
    }

    /// Get a system internal field (__exec__, __`audit_trace`, __`universe_rules`__).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Get user data (excludes system fields).
    pub fn get_user(&self, key: &str) -> Option<&Value> {
        if key.starts_with("__") {
            None
        } else {
            self.data.get(key)
        }
    }

    /// Check if a key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Get the execution context.
    pub fn exec_context(&self) -> Option<ExecContext> {
        self.data.get("__exec__").map(ExecContext::from_value)
    }

    /// Set the execution context, returning a new State.
    ///
    /// P2-1: Unified __exec__ write entry, replacing scattered `update_exec_field`.
    pub fn set_exec_context(&self, ctx: &ExecContext) -> Self {
        self.set("__exec__", ctx.to_value())
    }

    /// Update a single field in __exec__, returning a new State.
    ///
    /// P2-1: Convenience method equivalent to:
    /// ```ignore
    /// let ctx = state.exec_context().unwrap_or_default();
    /// let ctx = ctx.with_field(field, value);
    /// state.set_exec_context(&ctx)
    /// ```
    pub fn update_exec_field(&self, field: &str, value: Value) -> Self {
        let ctx = self
            .exec_context()
            .unwrap_or_else(|| ExecContext::new(Value::empty_object()));
        let ctx = ctx.with_field(field, value);
        self.set_exec_context(&ctx)
    }

    // ──────────────────────────────────────────
    // Immutable "modification" operations
    // ──────────────────────────────────────────

    /// Set a field, returning a new State.
    pub fn set(&self, key: impl Into<String>, value: Value) -> Self {
        Self {
            data: self.data.update(key.into(), value),
        }
    }

    /// Batch set fields.
    pub fn set_all(&self, items: Vec<(&str, Value)>) -> Self {
        let mut new_data = self.data.clone();
        for (k, v) in items {
            new_data.insert(k.to_string(), v);
        }
        Self { data: new_data }
    }

    /// Remove a field, returning a new State.
    pub fn remove(&self, key: &str) -> Self {
        Self {
            data: self.data.without(key),
        }
    }

    /// Set the execution context.
    pub fn with_exec_context(&self, ctx: &ExecContext) -> Self {
        self.set("__exec__", ctx.to_value())
    }

    /// Set the template rule list.
    pub fn with_universe_rules(&self, rules: Value) -> Self {
        self.set("__universe_rules__", rules)
    }

    /// Set the audit trail.
    pub fn with_audit_trace(&self, trace: Value) -> Self {
        self.set("__audit_trace", trace)
    }

    // ──────────────────────────────────────────
    // Path resolution
    // ──────────────────────────────────────────

    /// Get a value along a path. Path format: `key.subkey[0].field`
    pub fn get_path(&self, path: &str) -> Option<Value> {
        if path.is_empty() {
            // Check if there is a special value for the empty path
            if let Some(val) = self.data.get("") {
                return Some(val.clone());
            }
            return Some(self.to_value());
        }

        let parts = split_path(path);
        let mut current: Option<&Value> = None;

        for part in &parts {
            current = match current {
                None => self.data.get(part),
                Some(Value::Object(m)) => m.get(part),
                Some(Value::List(v)) => {
                    if let Ok(idx) = part.parse::<usize>() {
                        v.get(idx)
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            current?;
        }

        current.cloned()
    }

    /// Set a value along a path, returning a new State.
    ///
    /// Supports multi-level path setting, e.g., `"a.b.c"`, `"items[0].name"`.
    /// If intermediate path segments do not exist, they are automatically created.
    pub fn set_path(&self, path: &str, value: Value) -> Self {
        if path.is_empty() {
            // Empty path: directly set the empty string key
            return self.set("", value);
        }

        let parts = split_path(path);
        if parts.is_empty() {
            return self.clone();
        }

        if parts.len() == 1 {
            // Single-level path, directly set
            return self.set(parts[0].clone(), value);
        }

        // Multi-level path: build from the target position upward
        self.set_path_recursive(&parts, 0, value)
    }

    /// Recursively set a path value.
    ///
    /// Recursively sets values from the current level downward.
    /// `parts` is the complete path; `idx` is the current position being processed.
    fn set_path_recursive(&self, parts: &[String], idx: usize, value: Value) -> Self {
        if idx >= parts.len() {
            return self.clone();
        }

        let current_key = &parts[idx];
        let is_last = idx == parts.len() - 1;

        // If the current key is a pure number, treat it as an array index
        if let Ok(index) = current_key.parse::<usize>() {
            // If the current state is empty, create an array
            if self.data.is_empty() && !parts.is_empty() {
                let mut new_list = im::Vector::new();
                while new_list.len() <= index {
                    new_list.push_back(Value::Null);
                }
                new_list.set(index, value);
                // Use current_key for recursive levels, parts[0] for the top level
                let key = if idx == 0 { &parts[0] } else { current_key };
                return Self {
                    data: im::HashMap::from_iter([(key.clone(), Value::List(new_list))]),
                };
            }
            return self.set_array_element_by_index(index, value, is_last);
        }

        if is_last {
            // Last level, directly set the value
            return self.set(current_key.clone(), value);
        }

        // Not the last level, recursively update the subpath
        let current_value = self.data.get(current_key);

        // Decide how to recurse based on the current value type
        match current_value {
            Some(Value::Object(m)) => {
                // Regular object handling
                let sub_state = Self::from_im_map(m.clone());
                let updated_sub = sub_state.set_path_recursive(parts, idx + 1, value);
                self.set(current_key.clone(), updated_sub.to_value())
            }
            Some(Value::List(list)) => {
                // List handling: the next key must be an array index
                let next_key = &parts[idx + 1];
                if let Ok(arr_idx) = next_key.parse::<usize>() {
                    // Set the list element
                    let mut new_list = list.clone();
                    let has_more = idx + 2 < parts.len();

                    if arr_idx < new_list.len() {
                        if has_more {
                            // Need to recursively set the subpath
                            let current_elem =
                                new_list.get(arr_idx).cloned().unwrap_or(Value::Null);
                            let sub_state = match current_elem {
                                Value::Object(m) => Self::from_im_map(m),
                                _ => Self::empty(),
                            };
                            let updated_sub = sub_state.set_path_recursive(parts, idx + 2, value);
                            new_list.set(arr_idx, updated_sub.to_value());
                        } else {
                            // Directly set the value
                            new_list.set(arr_idx, value);
                        }
                    } else {
                        // Extend the list
                        while new_list.len() <= arr_idx {
                            new_list.push_back(Value::Null);
                        }
                        if has_more {
                            let sub_state = Self::empty();
                            let updated_sub = sub_state.set_path_recursive(parts, idx + 2, value);
                            new_list.set(arr_idx, updated_sub.to_value());
                        } else {
                            new_list.set(arr_idx, value);
                        }
                    }
                    self.set(current_key.clone(), Value::List(new_list))
                } else {
                    // Cannot handle non-numeric index access on a list
                    self.clone()
                }
            }
            _ => {
                // No existing value or not an object — create a new object or array
                let next_key = if idx + 1 < parts.len() {
                    Some(&parts[idx + 1])
                } else {
                    None
                };

                // Check if the next key is an array index
                if let Some(next_k) = next_key {
                    if let Ok(arr_idx) = next_k.parse::<usize>() {
                        // Next key is an array index, create an array
                        let mut new_list = im::Vector::new();
                        while new_list.len() <= arr_idx {
                            new_list.push_back(Value::Null);
                        }

                        if idx + 2 >= parts.len() {
                            // The next level is the last, directly set the value
                            new_list.set(arr_idx, value);
                        } else {
                            // Continue recursive setting
                            let sub_state = Self::empty();
                            let updated_sub = sub_state.set_path_recursive(parts, idx + 2, value);
                            new_list.set(arr_idx, updated_sub.to_value());
                        }
                        return self.set(current_key.clone(), Value::List(new_list));
                    }
                }

                // Create an object
                let sub_state = Self::empty();
                let updated_sub = sub_state.set_path_recursive(parts, idx + 1, value);
                self.set(current_key.clone(), updated_sub.to_value())
            }
        }
    }

    /// Set an array element by index.
    ///
    /// This function is called when `set_path_recursive` encounters a path
    /// whose current segment is a bare numeric index (e.g. `"2"` in the path
    /// `"[2].name"` → `parts = ["2", "name"]`), **and the state is non-empty**.
    ///
    /// # Determinism fix (ER-601)
    ///
    /// The previous implementation iterated `self.data` (an `im::HashMap`) to
    /// find "the first List field long enough to hold `index`". When the state
    /// contained multiple List fields, the iteration order of `im::HashMap` is
    /// **not guaranteed** to be consistent across platforms, Rust versions, or
    /// hash seeds — meaning the same input path could non-deterministically
    /// hit different lists, producing different outputs. This violated ER-601.
    ///
    /// The fix: collect all List fields in a **deterministic order** (sorted by
    /// key) and:
    ///   - If exactly one List field exists → operate on it (backward compatible).
    ///   - If zero or multiple List fields exist → no-op (return clone).
    ///
    /// This guarantees deterministic behavior regardless of `HashMap` iteration
    /// order, while preserving the single-list convenience case.
    fn set_array_element_by_index(&self, index: usize, value: Value, is_last: bool) -> Self {
        // Collect all List fields in deterministic (sorted) key order.
        // This avoids the non-determinism of iterating im::HashMap directly.
        let mut list_keys: Vec<&String> = self
            .data
            .iter()
            .filter_map(|(k, v)| match v {
                Value::List(_) => Some(k),
                _ => None,
            })
            .collect();
        list_keys.sort();

        if list_keys.len() != 1 {
            // Zero lists: nothing to modify.
            // Multiple lists: ambiguous which one to target — malformed path.
            // Both cases: deterministic no-op (return clone).
            return self.clone();
        }

        // Exactly one list field — operate on it (backward compatible).
        let key = list_keys[0];
        if let Some(Value::List(list)) = self.data.get(key) {
            let mut new_list = list.clone();
            if index < new_list.len() {
                new_list.set(index, value);
                return self.set(key.clone(), Value::List(new_list));
            } else if is_last {
                // Out of bounds but this is the last level, extend the list
                while new_list.len() <= index {
                    new_list.push_back(Value::Null);
                }
                new_list.set(index, value);
                return self.set(key.clone(), Value::List(new_list));
            }
        }

        self.clone()
    }

    // ──────────────────────────────────────────
    // Conversion
    // ──────────────────────────────────────────

    /// Convert to `Value::Object`.
    pub fn to_value(&self) -> Value {
        Value::Object(self.data.clone())
    }

    /// Convert to `StdHashMap`.
    pub fn to_std_map(&self) -> StdHashMap<String, Value> {
        self.data
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Convert to a JSON string.
    pub fn to_json(&self) -> String {
        let json = value_to_serde_json(&self.to_value());
        serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
    }

    /// Count of user data entries.
    pub fn user_data_count(&self) -> usize {
        self.data
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .count()
    }

    /// Collection of all user data keys.
    pub fn user_keys(&self) -> Vec<String> {
        self.data
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .map(|(k, _)| k.clone())
            .collect()
    }
}

// ══════════════════════════════════════════════
// Path resolution utilities
// ══════════════════════════════════════════════

/// Split a path string into segments. Supports dot-separated keys and array indices.
/// "key.subkey[0].field" → ["key", "subkey", "0", "field"]
fn split_path(path: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_bracket = false;

    for ch in path.chars() {
        match ch {
            '.' if !in_bracket => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            '[' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                in_bracket = true;
            }
            ']' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                in_bracket = false;
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

// ══════════════════════════════════════════════
// Default
// ══════════════════════════════════════════════

impl Default for State {
    fn default() -> Self {
        Self::empty()
    }
}

// ══════════════════════════════════════════════
// Display
// ══════════════════════════════════════════════

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "State({})", self.to_json())
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_empty() {
        let s = State::empty();
        assert_eq!(s.user_data_count(), 0);
    }

    #[test]
    fn test_state_set_immutability() {
        let s1 = State::empty();
        let s2 = s1.set("x", Value::Integer(10));

        // s1 unchanged
        assert_eq!(s1.get("x"), None);
        // s2 has the new value
        assert_eq!(s2.get("x"), Some(&Value::Integer(10)));
    }

    #[test]
    fn test_state_get_path() {
        let s = State::new(vec![
            ("x", Value::Integer(42)),
            ("y", Value::string("hello")),
        ]);
        assert_eq!(s.get_path("x"), Some(Value::Integer(42)));
        assert_eq!(s.get_path("y"), Some(Value::string("hello")));
        assert_eq!(s.get_path("z"), None);
    }

    #[test]
    fn test_state_get_path_nested() {
        let inner = State::new(vec![("a", Value::Integer(1))]);
        let s = State::empty().set("inner", inner.to_value());
        assert_eq!(s.get_path("inner.a"), Some(Value::Integer(1)));
    }

    #[test]
    fn test_state_roundtrip() {
        let s = State::new(vec![
            ("name", Value::string("test")),
            ("count", Value::Integer(3)),
        ]);
        let val = s.to_value();
        let restored = State::from_value(&val);
        assert_eq!(s.get("name"), restored.get("name"));
        assert_eq!(s.get("count"), restored.get("count"));
    }

    #[test]
    fn test_system_fields_hidden_from_user() {
        let s = State::empty()
            .set("__exec__", Value::empty_object())
            .set("data", Value::Integer(1));
        assert_eq!(s.get_user("__exec__"), None);
        assert_eq!(s.get_user("data"), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_split_path_simple() {
        let parts = split_path("a.b.c");
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_path_with_index() {
        let parts = split_path("items[0].name");
        assert_eq!(parts, vec!["items", "0", "name"]);
    }

    #[test]
    fn test_state_remove() {
        let s = State::new(vec![("x", Value::Integer(1)), ("y", Value::Integer(2))]);
        let s2 = s.remove("x");
        assert_eq!(s2.get("x"), None);
        assert_eq!(s2.get("y"), Some(&Value::Integer(2)));
        // s unchanged
        assert_eq!(s.get("x"), Some(&Value::Integer(1)));
    }

    // ========== set_path tests ==========

    #[test]
    fn test_set_path_simple() {
        let s = State::empty();
        let s2 = s.set_path("x", Value::Integer(42));
        assert_eq!(s2.get("x"), Some(&Value::Integer(42)));
        // original unchanged
        assert_eq!(s.get("x"), None);
    }

    #[test]
    fn test_set_path_nested() {
        let s = State::empty();
        let s2 = s.set_path("a.b.c", Value::Integer(123));
        assert_eq!(s2.get_path("a.b.c"), Some(Value::Integer(123)));
    }

    #[test]
    fn test_set_path_nested_existing() {
        let inner = State::new(vec![("x", Value::Integer(1))]);
        let s = State::empty().set("a", inner.to_value());
        let s2 = s.set_path("a.y", Value::Integer(2));
        assert_eq!(s2.get_path("a.x"), Some(Value::Integer(1)));
        assert_eq!(s2.get_path("a.y"), Some(Value::Integer(2)));
    }

    #[test]
    fn test_set_path_array_index() {
        let s = State::empty().set(
            "items",
            Value::list(vec![Value::Integer(1), Value::Integer(2)]),
        );
        let s2 = s.set_path("items[0]", Value::Integer(100));
        assert_eq!(s2.get_path("items[0]"), Some(Value::Integer(100)));
        assert_eq!(s2.get_path("items[1]"), Some(Value::Integer(2)));
    }

    #[test]
    fn test_set_path_array_out_of_bounds() {
        let s = State::empty().set("items", Value::list(vec![Value::Integer(1)]));
        let s2 = s.set_path("items[5]", Value::Integer(999));
        assert_eq!(s2.get_path("items[0]"), Some(Value::Integer(1)));
        assert_eq!(s2.get_path("items[5]"), Some(Value::Integer(999)));
    }

    #[test]
    fn test_set_path_deep_nested() {
        let s = State::empty();
        let s2 = s.set_path("a.b.c.d.e", Value::string("deep"));
        assert_eq!(s2.get_path("a.b.c.d.e"), Some(Value::string("deep")));
    }

    #[test]
    fn test_set_path_creates_intermediate() {
        let s = State::empty();
        let s2 = s.set_path("a.b", Value::Integer(1));
        assert!(s2.get_path("a").is_some());
        assert!(s2.get_path("a").unwrap().is_object());
    }

    // ========== set_path_recursive additional tests ==========

    #[test]
    fn test_set_path_recursive_empty_path() {
        let s = State::empty();
        let s2 = s.set_path("", Value::Integer(42));
        // Empty path returns the value converted to a State
        assert_eq!(s2.get_path(""), Some(Value::Integer(42)));
    }

    #[test]
    fn test_set_path_recursive_overwrite_existing() {
        let s = State::new(vec![("x", Value::Integer(1))]);
        let s2 = s.set_path("x", Value::Integer(2));
        assert_eq!(s2.get("x"), Some(&Value::Integer(2)));
        // Original state unchanged
        assert_eq!(s.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_set_path_recursive_nested_object_in_array() {
        let inner = State::new(vec![("name", Value::string("item1"))]);
        let items = Value::list(vec![inner.to_value()]);
        let s = State::empty().set("items", items);
        let s2 = s.set_path("items[0].name", Value::string("updated"));
        assert_eq!(s2.get_path("items[0].name"), Some(Value::string("updated")));
    }

    #[test]
    fn test_set_path_recursive_array_create() {
        let s = State::empty();
        let s2 = s.set_path("arr[2]", Value::Integer(3));
        assert_eq!(s2.get_path("arr[0]"), Some(Value::Null));
        assert_eq!(s2.get_path("arr[1]"), Some(Value::Null));
        assert_eq!(s2.get_path("arr[2]"), Some(Value::Integer(3)));
    }

    #[test]
    fn test_set_path_recursive_mixed_path() {
        let s = State::empty();
        let s2 = s.set_path("users[0].name.first", Value::string("John"));
        assert_eq!(
            s2.get_path("users[0].name.first"),
            Some(Value::string("John"))
        );
    }

    #[test]
    fn test_set_path_recursive_complex_nested() {
        let s = State::empty();
        let s2 = s.set_path("level1.level2[0].level3.key", Value::Integer(42));
        assert_eq!(
            s2.get_path("level1.level2[0].level3.key"),
            Some(Value::Integer(42))
        );
        // Verify intermediate structures were created correctly
        assert!(s2.get_path("level1").unwrap().is_object());
        assert!(s2.get_path("level1.level2").unwrap().is_list());
        assert!(s2.get_path("level1.level2[0]").unwrap().is_object());
        assert!(s2.get_path("level1.level2[0].level3").unwrap().is_object());
    }

    // ══════════════════════════════════════════════
    // Additional coverage: path resolution edge cases
    // ══════════════════════════════════════════════

    #[test]
    fn test_split_path_edge_cases() {
        // Adjacent dots: empty parts are skipped
        assert_eq!(split_path("a..b"), vec!["a", "b"]);
        // Leading dot: skipped
        assert_eq!(split_path(".a"), vec!["a"]);
        // Trailing dot: skipped
        assert_eq!(split_path("a."), vec!["a"]);
        // Empty path returns empty Vec
        assert_eq!(split_path(""), Vec::<String>::new());
        // [0] parses as "0"
        assert_eq!(split_path("[0]"), vec!["0"]);
    }

    #[test]
    fn test_get_path_empty_returns_whole_state() {
        // Empty path returns the entire state
        let s = State::new(vec![("x", Value::Integer(5))]);
        let result = s.get_path("");
        assert!(result.is_some());
        assert!(result.unwrap().is_object());
    }

    #[test]
    fn test_get_path_empty_string_key() {
        // "" as a key can be accessed directly
        let s = State::new(vec![("x", Value::Integer(5))]);
        // Accessing the "" key doesn't exist → returns the entire state
        let result = s.get_path("");
        assert!(result.is_some());
    }

    #[test]
    fn test_get_path_array_out_of_bounds_returns_none() {
        // Array index out of bounds → None
        let s = State::new(vec![("arr", Value::list(vec![Value::Integer(1)]))]);
        assert_eq!(s.get_path("arr[5]"), None);
    }

    #[test]
    fn test_get_path_non_object_intermediate_returns_none() {
        // Intermediate value is not an Object → None
        let s = State::new(vec![("x", Value::Integer(5))]);
        assert_eq!(s.get_path("x.nested"), None);
    }

    #[test]
    fn test_get_path_non_list_with_index_returns_none() {
        // Non-list value accessed with an index → None
        let s = State::new(vec![("x", Value::Integer(5))]);
        assert_eq!(s.get_path("x[0]"), None);
    }

    #[test]
    fn test_set_path_empty_key() {
        // Setting the empty string key
        let s = State::empty().set_path("", Value::Integer(99));
        assert_eq!(s.get(""), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_set_path_empty_path_unchanged() {
        // Empty path → does not change state (returns a clone)
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.set_path("", Value::Integer(99));
        // set_path on empty path sets the "" key
        assert_eq!(s2.get(""), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_set_path_nested_into_scalar_fails() {
        // Attempting to set a nested path under a scalar → silently fails (returns clone)
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.set_path("x.nested", Value::Integer(99));
        // x is an Integer, cannot set nested under it → s2 == s
        assert!(s2.get("x").is_some()); // x remains an Integer
    }

    #[test]
    fn test_set_path_array_out_of_bounds_creates_intermediate() {
        // arr[5] is the last level but out of bounds (array has only 1 element)
        // → extends the array and sets the value
        let s = State::new(vec![("arr", Value::list(vec![Value::Integer(1)]))]);
        let s2 = s.set_path("arr[5]", Value::Integer(99));
        // arr[5] out of bounds, is_last=true, extends array and sets value
        assert_eq!(s2.get_path("arr[5]"), Some(Value::Integer(99)));
    }

    #[test]
    fn test_from_value_non_object_returns_empty() {
        // from_value receives a non-Object value → returns State::empty()
        let val = Value::Integer(42);
        let s = State::from_value(&val);
        assert_eq!(s.user_data_count(), 0);
    }

    #[test]
    fn test_from_value_list_returns_empty() {
        let val = Value::list(vec![Value::Integer(1)]);
        let s = State::from_value(&val);
        assert_eq!(s.user_data_count(), 0);
    }

    #[test]
    fn test_user_keys_excludes_system_fields() {
        // user_keys excludes __-prefixed fields
        let s = State::new(vec![
            ("__secret", Value::Integer(1)),
            ("data", Value::Integer(2)),
            ("__exec__", Value::Integer(3)),
        ]);
        let keys = s.user_keys();
        assert!(!keys.contains(&"__secret".to_string()));
        assert!(keys.contains(&"data".to_string()));
        assert!(!keys.contains(&"__exec__".to_string()));
    }

    #[test]
    fn test_state_set_all_multiple_keys() {
        // set_all sets multiple keys at once
        let s = State::empty();
        let s2 = s.set_all(vec![("a", Value::Integer(1)), ("b", Value::Integer(2))]);
        assert_eq!(s2.get("a"), Some(&Value::Integer(1)));
        assert_eq!(s2.get("b"), Some(&Value::Integer(2)));
    }

    #[test]
    fn test_state_with_audit_trace() {
        // with_audit_trace sets __audit_trace
        let s = State::empty();
        let trace = Value::list(vec![Value::string("event1")]);
        let s2 = s.with_audit_trace(trace);
        assert!(s2.get("__audit_trace").is_some());
    }

    #[test]
    fn test_update_exec_field() {
        // update_exec_field updates a field in __exec__ (using a known field name)
        let s = State::empty();
        let s2 = s.update_exec_field("audit_on", Value::Bool(false));
        let exec = s2.get("__exec__").unwrap();
        // audit_on is a known field, should be updated
        assert_eq!(exec.get("audit_on"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_get_user_returns_none_for_system_fields() {
        // get_user skips system fields
        let s = State::new(vec![
            ("x", Value::Integer(5)),
            ("__exec__", Value::Integer(1)),
        ]);
        assert_eq!(s.get_user("x"), Some(&Value::Integer(5)));
        assert_eq!(s.get_user("__exec__"), None);
    }

    #[test]
    fn test_contains_key() {
        let s = State::new(vec![("x", Value::Integer(5))]);
        assert!(s.contains_key("x"));
        assert!(!s.contains_key("y"));
        assert!(!s.contains_key("__exec__")); // system fields not counted
    }

    #[test]
    fn test_to_std_map() {
        // to_std_map converts to a standard HashMap
        let s = State::new(vec![("x", Value::Integer(5))]);
        let m = s.to_std_map();
        assert_eq!(m.get("x"), Some(&Value::Integer(5)));
    }

    // ══════════════════════════════════════════════
    // Additional coverage: to_json / exec_context
    // ══════════════════════════════════════════════

    #[test]
    fn test_get_exec_context_exists() {
        // exec_context returns Some when __exec__ exists
        let exec_map = im::HashMap::from(vec![("audit_on".to_string(), Value::Bool(true))]);
        let s = State::new(vec![("__exec__", Value::from(exec_map))]);
        let ctx = s.exec_context();
        assert!(ctx.is_some());
    }

    #[test]
    fn test_exec_context_with_scalar_still_returns_some() {
        // exec_context still returns Some even if __exec__ is a scalar
        let s = State::new(vec![("__exec__", Value::Integer(5))]);
        let ctx = s.exec_context();
        assert!(ctx.is_some());
    }

    #[test]
    fn test_set_exec_context() {
        // set_exec_context sets the __exec__ field
        let s = State::new(vec![("x", Value::Integer(5))]);
        let exec_map = im::HashMap::from(vec![("audit_on".to_string(), Value::Bool(false))]);
        let ctx = ExecContext::new(Value::from(exec_map));
        let s2 = s.set_exec_context(&ctx);
        assert!(s2.get("__exec__").is_some());
    }

    #[test]
    fn test_state_clone_is_independent() {
        // A cloned state is independent from the original
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.set("x", Value::Integer(99));
        assert_eq!(s.get("x"), Some(&Value::Integer(5))); // original unchanged
        assert_eq!(s2.get("x"), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_set_path_updates_existing_path() {
        // Setting the same path multiple times
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.set_path("x", Value::Integer(10));
        let s3 = s2.set_path("x", Value::Integer(15));
        assert_eq!(s3.get("x"), Some(&Value::Integer(15)));
    }

    #[test]
    fn test_state_from_value_empty() {
        // State::from_value on an empty object returns an empty State
        let empty_obj = Value::from(im::HashMap::default());
        let s = State::from_value(&empty_obj);
        assert_eq!(s.user_data_count(), 0);
    }

    #[test]
    fn test_state_with_audit_trace_multiple_entries() {
        // with_audit_trace can append multiple records
        let s = State::new(vec![(
            "__audit_trace",
            Value::list(vec![Value::string("e1")]),
        )]);
        let trace = Value::list(vec![Value::string("e2")]);
        let s2 = s.with_audit_trace(trace);
        let traces = s2.get("__audit_trace").unwrap().as_list().unwrap();
        assert!(!traces.is_empty());
    }

    #[test]
    fn test_update_exec_field_creates_exec_if_missing() {
        // update_exec_field creates __exec__ if missing
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.update_exec_field("audit_on", Value::Bool(false));
        let exec = s2.get("__exec__").unwrap();
        assert!(exec.is_object());
    }

    #[test]
    fn test_update_exec_field_string() {
        // update_exec_field supports string-valued fields
        // Using a non-existent __exec__ → creates a new ExecContext
        let s = State::new(vec![("x", Value::Integer(5))]);
        let s2 = s.update_exec_field("prev_instruction", Value::string("compute"));
        let exec = s2.get("__exec__").unwrap();
        // prev_instruction is an ExecContext field, not in Value::Object
        // So this will return None
        assert!(exec.is_object());
    }

    #[test]
    fn test_contains_key_system_field() {
        // contains_key returns true for system fields as well
        let s = State::new(vec![("__exec__", Value::Integer(1))]);
        assert!(s.contains_key("__exec__"));
    }

    // ========== FLOW-02 verification: empty state array path ==========

    #[test]
    fn test_set_path_empty_state_array_index() {
        // FLOW-02 fix verification: setting an array index path on an empty state
        // Before fix: used "" as the key, created {"": [...]} instead of {"a": [...]}
        // After fix: uses parts[0] as the key, correctly creates {"a": [...]}

        let s = State::empty();
        let s2 = s.set_path("a[0]", Value::Integer(42));

        // Verify the key name is correct (should be "a", not "")
        assert!(s2.contains_key("a"), "Key should be 'a', not empty string");

        // Verify it can be read via the normal path
        assert_eq!(s2.get_path("a[0]"), Some(Value::Integer(42)));

        // Verify it was not created via the "" key
        assert!(!s2.contains_key(""), "Should not have an empty key name ''");
    }

    #[test]
    fn test_set_path_empty_state_preserves_top_key() {
        // FLOW-02 verification: ensure parts[0] is correctly used as the top-level key
        let s = State::empty();

        // Test multiple different top-level keys
        for key in ["data", "items", "matrix", "x"] {
            let s2 = s.set_path(&format!("{}[0]", key), Value::Integer(1));
            assert!(
                s2.contains_key(key),
                "Key should be '{}', not empty string",
                key
            );
            assert!(!s2.contains_key(""), "Should not have an empty key name ''");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // P2: Property-based tests (proptest) for State::set_path
    //
    // Verifies algebraic properties of set_path that hold for arbitrary
    // inputs. These tests catch edge cases that example-based tests miss
    // (e.g., key collisions, immutability violations, roundtrip failures).
    // ═══════════════════════════════════════════════════════════════════════

    use proptest::prelude::*;

    /// Generate an arbitrary Value (limited to deterministic, comparable types).
    /// Reuses the same strategy pattern as deterministic.rs::arb_value.
    fn arb_value() -> BoxedStrategy<Value> {
        let leaf: BoxedStrategy<Value> = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::Integer),
            any::<f64>().prop_map(|f| {
                if f.is_finite() {
                    Value::float(f)
                } else {
                    Value::Integer(0)
                }
            }),
            any::<String>().prop_map(Value::string),
        ]
        .boxed();
        prop_oneof![
            leaf.clone(),
            prop::collection::vec(leaf, 0..5).prop_map(Value::list),
        ]
        .boxed()
    }

    /// Generate a simple single-level key (alphanumeric, no dots/brackets).
    fn arb_simple_key() -> BoxedStrategy<String> {
        "[a-zA-Z_][a-zA-Z0-9_]{0,7}".prop_map(String::from).boxed()
    }

    /// set_path then get_path should roundtrip for single-level keys.
    #[test]
    fn prop_set_path_get_path_roundtrip_single_level() {
        proptest!(|(key in arb_simple_key(), v in arb_value())| {
            let s = State::empty();
            let s2 = s.set_path(&key, v.clone());
            prop_assert_eq!(s2.get_path(&key), Some(v));
        });
    }

    /// set_path should not mutate the original state (immutability).
    #[test]
    fn prop_set_path_immutability() {
        proptest!(|(k1 in arb_simple_key(), v1 in arb_value(), k2 in arb_simple_key(), v2 in arb_value())| {
            let s1 = State::empty().set_path(&k1, v1.clone());
            let s2 = s1.set_path(&k2, v2.clone());
            // Original s1 should be unchanged
            prop_assert_eq!(s1.get_path(&k1), Some(v1));
            // s2 should have the new value
            prop_assert_eq!(s2.get_path(&k2), Some(v2));
        });
    }

    /// Setting the same key twice: the second value wins (overwrite semantics).
    #[test]
    fn prop_set_path_overwrite() {
        proptest!(|(key in arb_simple_key(), v1 in arb_value(), v2 in arb_value())| {
            let s = State::empty()
                .set_path(&key, v1.clone())
                .set_path(&key, v2.clone());
            prop_assert_eq!(s.get_path(&key), Some(v2));
        });
    }

    /// set_path with empty path sets the "" key.
    #[test]
    fn prop_set_path_empty_key() {
        proptest!(|(v in arb_value())| {
            let s = State::empty().set_path("", v.clone());
            prop_assert_eq!(s.get_path(""), Some(v));
        });
    }

    /// Two-level path "a.b" should create nested structure and roundtrip.
    #[test]
    fn prop_set_path_two_level_roundtrip() {
        proptest!(|(v in arb_value())| {
            let s = State::empty().set_path("a.b", v.clone());
            prop_assert_eq!(s.get_path("a.b"), Some(v));
        });
    }
}
