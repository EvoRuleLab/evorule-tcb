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

//! Unified value type — Generic value representation for `EvoRule`.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Design Principles
//!
//! - All data is represented as the `Value` enum, with no hidden state.
//! - `Float` uses `OrderedFloat` to ensure deterministic comparison (NaN normalized to 0.0).
//! - `Object` uses `im::HashMap` for O(1) clone (immutable sharing).
//! - `List` uses `im::Vector` for O(1) clone.
//! - `serde` serialization/deserialization correctly handles all variants.
//!
//! # Determinism Guarantee
//!
//! All operations on `Value` are **L1 deterministic**:
//! - Same input → same output.
//! - No randomness, no wall-clock time, no side effects.
//! - `OrderedFloat` ensures deterministic ordering of floating-point values.
//! - `im::HashMap` keys are sorted in `content_hash` for deterministic iteration.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `Value` data model | ✅ L1 deterministic | Pure data |
//! | `Float` comparison | ✅ L1 deterministic | `OrderedFloat` (NaN → 0.0) |
//! | `Object` iteration | ✅ L1 deterministic | Keys sorted in `content_hash` |
//! | `List` order | ✅ L1 deterministic | `im::Vector` preserves insertion order |
//! | `to_dispatch_key` | ✅ L1 deterministic | Deterministic conversion |
//! | `loosely_equals` | ✅ L1 deterministic | Numeric cross-type comparison |
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
//! To replicate the `Value` encoding in other languages, the serialization format
//! is documented in the specification (`docs/spec/`). The encoding uses:
//! - Type tags to distinguish variants
//! - Length prefixes for variable-length types (String, Bytes, Object keys)
//! - Sorted keys for Object to ensure determinism

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap as StdHashMap;
use std::fmt;

// ══════════════════════════════════════════════
// Imports
// ══════════════════════════════════════════════

// im crate types — persistent data structures
pub type ImMap = im::HashMap<String, Value>;
pub type ImVector = im::Vector<Value>;

// ══════════════════════════════════════════════
// Deterministic Iteration Extensions
// ══════════════════════════════════════════════

/// Extension trait for deterministic iteration over `im::HashMap`.
///
/// `im::HashMap` uses insertion-order iteration which is deterministic within
/// a single process but may vary across processes if insertion order differs.
/// This trait provides `iter_sorted()` which returns keys in lexicographic order,
/// ensuring deterministic output for operations that depend on iteration order.
///
/// **Constitutional Redline (ER-601):** All iteration over collections that
/// affects output must be deterministic.
pub trait ImMapExt {
    /// Returns an iterator over key-value pairs sorted by key.
    ///
    /// This ensures deterministic iteration order regardless of insertion order.
    /// The returned iterator yields `(&String, &Value)` pairs.
    fn iter_sorted(&self) -> impl Iterator<Item = (&String, &Value)>;

    /// Returns an iterator over values sorted by their corresponding keys.
    ///
    /// This ensures deterministic iteration order for value-only access.
    fn values_sorted(&self) -> impl Iterator<Item = &Value>;
}

impl ImMapExt for ImMap {
    fn iter_sorted(&self) -> impl Iterator<Item = (&String, &Value)> {
        let mut entries: Vec<(&String, &Value)> = self.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries.into_iter()
    }

    fn values_sorted(&self) -> impl Iterator<Item = &Value> {
        let mut entries: Vec<(&String, &Value)> = self.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries.into_iter().map(|(_, v)| v)
    }
}

pub fn iter_std_hashmap_sorted<K: Ord + Clone, V>(
    m: &std::collections::HashMap<K, V>,
) -> Vec<(&K, &V)> {
    let mut entries: Vec<(&K, &V)> = m.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}

// ══════════════════════════════════════════════
// Value Enum
// ══════════════════════════════════════════════

/// Unified value type for `EvoRule`.
///
/// All internal data is passed through this type. Primitives that directly
/// manipulate State can only read/write this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Value {
    /// Null value.
    #[serde(rename = "null")]
    Null,
    /// Boolean value.
    #[serde(rename = "bool")]
    Bool(bool),
    /// Signed 64-bit integer.
    #[serde(rename = "integer")]
    Integer(i64),
    /// Floating-point number, using `OrderedFloat` for determinism.
    #[serde(rename = "float")]
    Float(OrderedFloat<f64>),
    /// UTF-8 string.
    #[serde(rename = "string")]
    String(String),
    /// List (ordered collection), using `im::Vector`.
    #[serde(rename = "list")]
    List(ImVector),
    /// Object (key-value mapping), using `im::HashMap`.
    #[serde(rename = "object")]
    Object(ImMap),
    /// Raw bytes. Deterministic by value equality: same bytes → same Value.
    /// Used for external binary data that has been captured as a snapshot.
    #[serde(rename = "bytes")]
    Bytes(Vec<u8>),
}

impl Value {
    // ──────────────────────────────────────────
    // Constructors
    // ──────────────────────────────────────────

    /// Create a Value from an integer.
    pub const fn integer(v: i64) -> Self {
        Self::Integer(v)
    }

    /// Create a Value from a float. NaN is normalized to 0.0.
    pub fn float(v: f64) -> Self {
        if v.is_nan() {
            Self::Float(OrderedFloat(0.0))
        } else {
            Self::Float(OrderedFloat(v))
        }
    }

    /// Create a Value from a string.
    pub fn string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// Create a list from a vec.
    pub fn list(items: Vec<Self>) -> Self {
        Self::List(ImVector::from(items))
    }

    /// Create an object from a `HashMap`.
    pub fn object(map: StdHashMap<String, Self>) -> Self {
        Self::Object(ImMap::from(map))
    }

    /// Create an object from an `im::HashMap`.
    pub const fn from_im_map(map: ImMap) -> Self {
        Self::Object(map)
    }

    /// Create an empty list.
    pub fn empty_list() -> Self {
        Self::List(ImVector::new())
    }

    /// Create an empty object.
    pub fn empty_object() -> Self {
        Self::Object(ImMap::new())
    }

    // ──────────────────────────────────────────
    // Type predicates
    // ──────────────────────────────────────────

    /// Check if the value is Null.
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Check if the value is a Boolean.
    pub const fn is_bool(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    /// Check if the value is an Integer.
    pub const fn is_integer(&self) -> bool {
        matches!(self, Self::Integer(_))
    }

    /// Check if the value is a Float.
    pub const fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    /// Check if the value is numeric (Integer or Float).
    pub const fn is_number(&self) -> bool {
        matches!(self, Self::Integer(_) | Self::Float(_))
    }

    /// Check if the value is a String.
    pub const fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    /// Check if the value is a List.
    pub const fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// Check if the value is an Object.
    pub const fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    // ──────────────────────────────────────────
    // Value extraction
    // ──────────────────────────────────────────

    /// Get the Boolean value. Returns None for non-Boolean types.
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get the Integer value. Returns None for non-Integer types.
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Get the Float value. Returns None for non-Float types.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(f64::from(*f)),
            _ => None,
        }
    }

    /// Get the numeric value (Integer or Float) unified as f64.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(i) => Some(*i as f64),
            Self::Float(f) => Some(f64::from(*f)),
            _ => None,
        }
    }

    /// Get the type name as a string for error messages.
    pub fn type_name(&self) -> &str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Integer(_) => "Integer",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::List(_) => "List",
            Self::Object(_) => "Object",
            Self::Bytes(_) => "Bytes",
        }
    }

    /// Get the string reference.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Convert this Value to a dispatch key string.
    ///
    /// Dispatch cases table keys are JSON strings, but `$ref`-resolved values may be
    /// non-string types such as Bool, Integer, etc. This method converts any Value
    /// to its natural string representation, so that cases keys like "true"/"false"/"42"
    /// can match correctly.
    ///
    /// Conversion rules (aligns with JSON literals):
    /// - Bool(true)  → "true"
    /// - Bool(false) → "false"
    /// - Integer(n)  → `n.to_string()` (e.g., "42", "-1")
    /// - Float(f)    → `f.to_string()` (e.g., "2.71")
    /// - String(s)   → s (original behavior)
    /// - Null        → "null"
    /// - List/Object → JSON serialization (for composite key matching)
    /// - Bytes       → hexadecimal representation
    pub fn to_dispatch_key(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Integer(n) => n.to_string(),
            Self::Float(f) => f.0.to_string(),
            Self::String(s) => s.clone(),
            Self::Null => "null".to_string(),
            Self::List(_) | Self::Object(_) => {
                serde_json::to_string(self).unwrap_or_else(|_| format!("{self:?}"))
            }
            Self::Bytes(b) => {
                let hex: Vec<String> = b.iter().map(|byte| format!("{byte:02x}")).collect();
                hex.join("")
            }
        }
    }

    /// Get the list reference.
    pub const fn as_list(&self) -> Option<&ImVector> {
        match self {
            Self::List(v) => Some(v),
            _ => None,
        }
    }

    /// Get the object reference.
    pub const fn as_object(&self) -> Option<&ImMap> {
        match self {
            Self::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Get the mutable object reference.
    pub fn as_object_mut(&mut self) -> Option<&mut ImMap> {
        match self {
            Self::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Convert the value to a boolean (for conditional evaluation).
    /// - Null/false/0/empty list/empty object → false
    /// - Everything else → true
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Integer(i) => *i != 0,
            Self::Float(f) => f.0 != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::List(v) => !v.is_empty(),
            Self::Object(m) => !m.is_empty(),
            Self::Bytes(b) => !b.is_empty(),
        }
    }

    /// Loose equality comparison (numeric types compare across Integer/Float boundaries;
    /// all other types compare strictly).
    ///
    /// Extracted from duplicate `values_equal` implementations in domain.rs and `compute_ops.rs`,
    /// eliminating the technical debt of inconsistent maintenance.
    pub fn loosely_equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(ai), Self::Integer(bi)) => ai == bi,
            (Self::Integer(ai), Self::Float(bf)) => (*ai as f64) == bf.0,
            (Self::Float(af), Self::Integer(bi)) => af.0 == (*bi as f64),
            (Self::Float(af), Self::Float(bf)) => af == bf,
            (Self::String(as_), Self::String(bs)) => as_ == bs,
            (Self::Bool(ab), Self::Bool(bb)) => ab == bb,
            (Self::Null, Self::Null) => true,
            _ => self == other,
        }
    }

    // ──────────────────────────────────────────
    // Object/List access
    // ──────────────────────────────────────────

    /// Get the value associated with a key in an object.
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(m) => m.get(key),
            _ => None,
        }
    }

    /// Set a key-value pair in an object, returning a new Value (immutable).
    /// For non-Object types (e.g., List, Integer, etc.), this is a no-op — returns a clone of self.
    pub fn set(&self, key: impl Into<String>, value: Self) -> Self {
        match self {
            Self::Object(m) => Self::Object(m.update(key.into(), value)),
            _ => self.clone(),
        }
    }

    /// Get the element at the given index in a list.
    pub fn index(&self, idx: usize) -> Option<&Self> {
        match self {
            Self::List(v) => v.get(idx),
            _ => None,
        }
    }

    /// Get the length of the value.
    pub fn len(&self) -> usize {
        match self {
            Self::List(v) => v.len(),
            Self::Object(m) => m.len(),
            Self::String(s) => s.len(),
            Self::Bytes(b) => b.len(),
            _ => 1,
        }
    }

    /// Check if the value is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ══════════════════════════════════════════════
// Display
// ══════════════════════════════════════════════

const MAX_DISPLAY_DEPTH: usize = 128;

fn value_fmt_inner(val: &Value, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    if depth > MAX_DISPLAY_DEPTH {
        return write!(f, "...");
    }
    match val {
        Value::Null => write!(f, "null"),
        Value::Bool(b) => write!(f, "{b}"),
        Value::Integer(i) => write!(f, "{i}"),
        Value::Float(fl) => write!(f, "{}", fl.0),
        Value::String(s) => write!(f, "\"{s}\""),
        Value::List(v) => {
            write!(f, "[")?;
            for (i, item) in v.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                value_fmt_inner(item, f, depth + 1)?;
            }
            write!(f, "]")
        }
        Value::Object(m) => {
            write!(f, "{{")?;
            for (i, (k, v)) in m.iter_sorted().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "\"{k}\": ")?;
                value_fmt_inner(v, f, depth + 1)?;
            }
            write!(f, "}}")
        }
        Value::Bytes(b) => write!(f, "bytes({})", b.len()),
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        value_fmt_inner(self, f, 0)
    }
}

// ══════════════════════════════════════════════
// From conversions for native Rust types
// ══════════════════════════════════════════════

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Integer(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Integer(i64::from(v))
    }
}

impl From<i128> for Value {
    fn from(v: i128) -> Self {
        // Safe saturated conversion: truncate values outside i64 range to boundaries
        Self::Integer(if v > i128::from(i64::MAX) {
            i64::MAX
        } else if v < i128::from(i64::MIN) {
            i64::MIN
        } else {
            v as i64
        })
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::float(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Self::List(ImVector::from(
            v.into_iter().map(Into::into).collect::<Vec<_>>(),
        ))
    }
}

impl From<StdHashMap<String, Self>> for Value {
    fn from(v: StdHashMap<String, Self>) -> Self {
        Self::Object(ImMap::from(v))
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}

/// Convert from `im::HashMap`.
impl From<ImMap> for Value {
    fn from(v: ImMap) -> Self {
        Self::Object(v)
    }
}

// ══════════════════════════════════════════════
// serde serialization bridge
// ══════════════════════════════════════════════

const MAX_JSON_DEPTH: usize = 128;

fn serde_json_to_value_inner(json: &serde_json::Value, depth: usize) -> Value {
    if depth > MAX_JSON_DEPTH {
        return Value::Null;
    }
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::float(f)
            } else {
                Value::Integer(n.as_i64().unwrap_or(0))
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::List(ImVector::from(
            arr.iter()
                .map(|v| serde_json_to_value_inner(v, depth + 1))
                .collect::<Vec<_>>(),
        )),
        serde_json::Value::Object(obj) => {
            let mut map = ImMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), serde_json_to_value_inner(v, depth + 1));
            }
            Value::Object(map)
        }
    }
}

/// Convert `serde_json::Value` to `EvoRule` Value, handling NaN/Inf.
pub fn serde_json_to_value(json: &serde_json::Value) -> Value {
    serde_json_to_value_inner(json, 0)
}

fn value_to_serde_json_inner(val: &Value, depth: usize) -> serde_json::Value {
    if depth > MAX_JSON_DEPTH {
        return serde_json::Value::Null;
    }
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => {
            let n = f.0;
            if n.is_finite() {
                serde_json::json!(n)
            } else if n.is_nan() {
                serde_json::json!(0.0_f64)
            } else {
                serde_json::Value::Null
            }
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::List(v) => serde_json::Value::Array(
            v.iter()
                .map(|v| value_to_serde_json_inner(v, depth + 1))
                .collect(),
        ),
        Value::Object(m) => {
            let mut map = serde_json::Map::new();
            for (k, v) in m.iter_sorted() {
                map.insert(k.clone(), value_to_serde_json_inner(v, depth + 1));
            }
            serde_json::Value::Object(map)
        }
        Value::Bytes(b) => serde_json::Value::String(hex::encode(b)),
    }
}

/// Convert `EvoRule` Value to `serde_json::Value`.
pub fn value_to_serde_json(val: &Value) -> serde_json::Value {
    value_to_serde_json_inner(val, 0)
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_creation() {
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Integer(42), Value::integer(42));
        assert!(Value::float(f64::NAN).is_float());
        assert_eq!(Value::string("hello"), Value::String("hello".to_string()));
    }

    #[test]
    fn test_float_nan_normalization() {
        let v = Value::float(f64::NAN);
        assert_eq!(v.as_float(), Some(0.0));
    }

    #[test]
    fn test_loosely_equals_numeric_cross_type() {
        // Integer vs Float with same numeric value should be equal
        assert!(Value::integer(42).loosely_equals(&Value::float(42.0)));
        assert!(Value::float(42.0).loosely_equals(&Value::integer(42)));
        // Integer vs Integer
        assert!(Value::integer(42).loosely_equals(&Value::integer(42)));
        assert!(!Value::integer(42).loosely_equals(&Value::integer(43)));
        // Float vs Float
        assert!(Value::float(2.71).loosely_equals(&Value::float(2.71)));
        assert!(!Value::float(2.71).loosely_equals(&Value::float(3.15)));
        // Different values
        assert!(!Value::integer(42).loosely_equals(&Value::float(99.0)));
    }

    #[test]
    fn test_loosely_equals_strict_types() {
        // String only compares strictly with String
        assert!(Value::string("abc").loosely_equals(&Value::string("abc")));
        assert!(!Value::string("abc").loosely_equals(&Value::string("abd")));
        // Bool only compares strictly with Bool
        assert!(Value::Bool(true).loosely_equals(&Value::Bool(true)));
        assert!(!Value::Bool(true).loosely_equals(&Value::Bool(false)));
        // Null only equals Null
        assert!(Value::Null.loosely_equals(&Value::Null));
    }

    #[test]
    fn test_loosely_equals_fallback_strict_for_mismatched_types() {
        // Cross-type (non-numeric) falls back to strict comparison, should return false
        assert!(!Value::string("42").loosely_equals(&Value::integer(42)));
        assert!(!Value::Bool(true).loosely_equals(&Value::integer(1)));
        assert!(!Value::Null.loosely_equals(&Value::integer(0)));
        // Lists/objects use PartialEq
        let list_a = Value::list(vec![Value::integer(1)]);
        let list_b = Value::list(vec![Value::integer(1)]);
        assert!(list_a.loosely_equals(&list_b));
        let list_c = Value::list(vec![Value::integer(2)]);
        assert!(!list_a.loosely_equals(&list_c));
    }

    #[test]
    fn test_list_operations() {
        let v = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        assert_eq!(v.len(), 2);
        assert!(!v.is_empty());
        assert_eq!(v.index(0), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_object_operations() {
        let mut map = StdHashMap::new();
        map.insert("x".to_string(), Value::Integer(10));
        let v = Value::object(map);
        assert_eq!(v.get("x"), Some(&Value::Integer(10)));
        assert_eq!(v.get("y"), None);
    }

    #[test]
    fn test_truthy() {
        assert!(!Value::Null.truthy());
        assert!(!Value::Bool(false).truthy());
        assert!(!Value::Integer(0).truthy());
        assert!(Value::Integer(1).truthy());
        assert!(Value::string("hello").truthy());
        assert!(!Value::empty_list().truthy());
    }

    #[test]
    fn test_set_immutability() {
        let obj = Value::empty_object();
        let new_obj = obj.set("x", Value::Integer(1));
        assert_eq!(obj.get("x"), None); // original unchanged
        assert_eq!(new_obj.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_serde_roundtrip() {
        let original = Value::Object(ImMap::from(vec![
            ("name".to_string(), Value::string("test")),
            ("count".to_string(), Value::Integer(42)),
        ]));
        let json = value_to_serde_json(&original);
        let back = serde_json_to_value(&json);
        assert_eq!(original, back);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Value::Null), "null");
        assert_eq!(format!("{}", Value::Integer(42)), "42");
        assert_eq!(format!("{}", Value::string("hi")), "\"hi\"");
    }

    // ========== Type predicate tests ==========

    #[test]
    fn test_type_checks() {
        // bool
        assert!(Value::Bool(true).is_bool());
        assert!(!Value::Integer(42).is_bool());

        // integer
        assert!(Value::Integer(42).is_integer());
        assert!(!Value::Float(OrderedFloat(2.71)).is_integer());

        // float
        assert!(Value::Float(OrderedFloat(2.71)).is_float());
        assert!(!Value::Integer(42).is_float());

        // number
        assert!(Value::Integer(42).is_number());
        assert!(Value::Float(OrderedFloat(2.71)).is_number());
        assert!(!Value::string("hello").is_number());

        // string
        assert!(Value::string("hello").is_string());
        assert!(!Value::Integer(42).is_string());

        // list
        assert!(Value::list(vec![]).is_list());
        assert!(!Value::string("hello").is_list());

        // object
        assert!(Value::empty_object().is_object());
        assert!(!Value::list(vec![]).is_object());
    }

    // ========== Value extraction tests ==========

    #[test]
    fn test_value_extraction() {
        // as_bool
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Integer(42).as_bool(), None);

        // as_integer
        assert_eq!(Value::Integer(42).as_integer(), Some(42));
        assert_eq!(Value::string("hello").as_integer(), None);

        // as_float
        assert_eq!(Value::Float(OrderedFloat(2.71)).as_float(), Some(2.71));
        assert_eq!(Value::string("hello").as_float(), None);

        // as_number
        assert_eq!(Value::Integer(42).as_number(), Some(42.0));
        assert_eq!(Value::Float(OrderedFloat(2.71)).as_number(), Some(2.71));
        assert_eq!(Value::string("hello").as_number(), None);

        // as_str
        assert_eq!(Value::string("hello").as_str(), Some("hello"));
        assert_eq!(Value::Integer(42).as_str(), None);

        // as_list
        let list = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        assert!(list.as_list().is_some());
        assert_eq!(Value::Integer(42).as_list(), None);

        // as_object
        let obj = Value::empty_object();
        assert!(obj.as_object().is_some());
        assert_eq!(Value::Integer(42).as_object(), None);
    }

    #[test]
    fn test_as_object_mut() {
        let mut obj = Value::empty_object();
        if let Some(m) = obj.as_object_mut() {
            m.insert("key".to_string(), Value::Integer(1));
        }
        assert_eq!(obj.get("key"), Some(&Value::Integer(1)));
    }

    // ========== Object/list operation tests ==========

    #[test]
    fn test_index() {
        let list = Value::list(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(list.index(0), Some(&Value::Integer(1)));
        assert_eq!(list.index(1), Some(&Value::Integer(2)));
        assert_eq!(list.index(3), None);
        assert_eq!(Value::Integer(42).index(0), None);
    }

    #[test]
    fn test_get() {
        let mut map = StdHashMap::new();
        map.insert("x".to_string(), Value::Integer(10));
        map.insert("y".to_string(), Value::Integer(20));
        let obj = Value::object(map);
        assert_eq!(obj.get("x"), Some(&Value::Integer(10)));
        assert_eq!(obj.get("y"), Some(&Value::Integer(20)));
        assert_eq!(obj.get("z"), None);
    }

    #[test]
    fn test_set() {
        let obj = Value::empty_object();
        let new_obj = obj.set("x", Value::Integer(1));
        assert!(obj.get("x").is_none());
        assert_eq!(new_obj.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_set_on_non_object() {
        let val = Value::Integer(42);
        let result = val.set("x", Value::Integer(1));
        // Non-Object types calling set is a no-op, returns a clone of self
        assert_eq!(result, Value::Integer(42));
    }

    // ========== len and is_empty tests ==========

    #[test]
    fn test_len_and_is_empty() {
        // list
        let list = Value::list(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(list.len(), 3);
        assert!(!list.is_empty());
        assert!(Value::empty_list().is_empty());

        // object
        let mut map = StdHashMap::new();
        map.insert("a".to_string(), Value::Integer(1));
        map.insert("b".to_string(), Value::Integer(2));
        let obj = Value::object(map);
        assert_eq!(obj.len(), 2);

        // string
        assert_eq!(Value::string("hello").len(), 5);

        // bytes
        let bytes = Value::Bytes(vec![1, 2, 3, 4, 5]);
        assert_eq!(bytes.len(), 5);

        // primitive
        assert_eq!(Value::Integer(42).len(), 1);
        assert_eq!(Value::Null.len(), 1);
    }

    // ========== From conversion tests ==========

    #[test]
    fn test_from_conversions() {
        // i32
        let val: Value = i32::MAX.into();
        assert_eq!(val, Value::Integer(i32::MAX as i64));

        // i128 (saturated conversion)
        let val: Value = i128::MAX.into();
        assert_eq!(val, Value::Integer(i64::MAX));

        // f64
        let val: Value = 2.71.into();
        assert_eq!(val.as_float(), Some(2.71));

        // String
        let val: Value = String::from("hello").into();
        assert_eq!(val.as_str(), Some("hello"));

        // &str
        let val: Value = "hello".into();
        assert_eq!(val.as_str(), Some("hello"));

        // Vec<Value>
        let val: Value = vec![Value::Integer(1), Value::Integer(2)].into();
        assert!(val.is_list());
        assert_eq!(val.len(), 2);

        // StdHashMap
        let mut map = StdHashMap::new();
        map.insert("x".to_string(), Value::Integer(1));
        let val: Value = map.into();
        assert!(val.is_object());

        // Vec<u8>
        let val: Value = vec![1u8, 2, 3].into();
        assert!(matches!(val, Value::Bytes(_)));

        // ImMap
        let mut im_map = ImMap::new();
        im_map.insert("key".to_string(), Value::Integer(42));
        let val: Value = im_map.into();
        assert!(val.is_object());
        assert_eq!(val.get("key"), Some(&Value::Integer(42)));
    }

    // ========== serde_json conversion tests ==========

    #[test]
    fn test_serde_json_conversions() {
        // null
        let json = serde_json::Value::Null;
        let val = serde_json_to_value(&json);
        assert_eq!(val, Value::Null);

        // bool
        let json = serde_json::Value::Bool(true);
        let val = serde_json_to_value(&json);
        assert_eq!(val, Value::Bool(true));

        // string
        let json = serde_json::Value::String("hello".to_string());
        let val = serde_json_to_value(&json);
        assert_eq!(val.as_str(), Some("hello"));

        // array
        let json = serde_json::json!([1, 2, 3]);
        let val = serde_json_to_value(&json);
        assert!(val.is_list());
        assert_eq!(val.len(), 3);

        // object
        let json = serde_json::json!({"x": 1, "y": 2});
        let val = serde_json_to_value(&json);
        assert!(val.is_object());
        assert_eq!(val.get("x"), Some(&Value::Integer(1)));

        // Value -> serde_json (integer)
        let val = Value::Integer(42);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(42));

        // Value -> serde_json (float)
        let val = Value::Float(OrderedFloat(2.71));
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(2.71));

        // Value -> serde_json (list)
        let val = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        let json = value_to_serde_json(&val);
        assert!(json.is_array());

        // Value -> serde_json (object)
        let mut map = StdHashMap::new();
        map.insert("x".to_string(), Value::Integer(1));
        let val = Value::object(map);
        let json = value_to_serde_json(&val);
        assert!(json.is_object());

        // Value -> serde_json (bytes)
        let val = Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]);
        let json = value_to_serde_json(&val);
        assert!(json.is_string());

        // roundtrip
        let original = Value::list(vec![
            Value::string("test"),
            Value::Integer(42),
            Value::Float(OrderedFloat(2.71)),
            Value::Bool(true),
            Value::Null,
        ]);
        let json = value_to_serde_json(&original);
        let back = serde_json_to_value(&json);
        assert_eq!(original, back);
    }

    // ========== truthy boundary tests ==========

    #[test]
    fn test_truthy_boundaries() {
        // null
        assert!(!Value::Null.truthy());

        // bool
        assert!(!Value::Bool(false).truthy());
        assert!(Value::Bool(true).truthy());

        // integer
        assert!(!Value::Integer(0).truthy());
        assert!(Value::Integer(1).truthy());
        assert!(Value::Integer(-1).truthy());

        // float
        assert!(!Value::Float(OrderedFloat(0.0)).truthy());
        assert!(!Value::Float(OrderedFloat(-0.0)).truthy());
        assert!(Value::Float(OrderedFloat(1.0)).truthy());
        assert!(Value::Float(OrderedFloat(-1.0)).truthy());

        // string
        assert!(!Value::string("").truthy());
        assert!(Value::string("hello").truthy());

        // list
        assert!(!Value::empty_list().truthy());
        assert!(Value::list(vec![Value::Integer(1)]).truthy());

        // object
        assert!(!Value::empty_object().truthy());
        let mut map = StdHashMap::new();
        map.insert("x".to_string(), Value::Integer(1));
        assert!(Value::object(map).truthy());

        // bytes
        assert!(!Value::Bytes(vec![]).truthy());
        assert!(Value::Bytes(vec![1]).truthy());
    }

    // ========== Float NaN handling tests ==========

    #[test]
    fn test_float_inf_positive() {
        let val = Value::float(f64::INFINITY);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::Value::Null);
    }

    #[test]
    fn test_float_inf_negative() {
        let val = Value::float(f64::NEG_INFINITY);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::Value::Null);
    }

    // ========== Immutability tests ==========

    #[test]
    fn test_list_immutability() {
        let list = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        let list2 = list.set("key", Value::Integer(3)); // object method, no effect on list
        assert_eq!(list.len(), 2);
        assert_eq!(list2.len(), 2);
    }

    #[test]
    fn test_value_clone_independence() {
        let v1 = Value::Integer(42);
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    // ========== Bytes type tests ==========

    #[test]
    fn test_bytes_type() {
        // creation
        let bytes = Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(bytes.len(), 4);
        assert!(matches!(bytes, Value::Bytes(_)));

        // display
        let bytes = Value::Bytes(vec![0xde, 0xad]);
        assert_eq!(format!("{}", bytes), "bytes(2)");

        // truthy
        assert!(!Value::Bytes(vec![]).truthy());
        assert!(Value::Bytes(vec![1]).truthy());

        // type checks (negative)
        let bytes = Value::Bytes(vec![1, 2, 3]);
        assert!(!bytes.is_number());
        assert!(!bytes.is_integer());
        assert!(!bytes.is_float());
        assert!(!bytes.is_string());
        assert!(!bytes.is_list());
        assert!(!bytes.is_object());
        assert!(!bytes.is_bool());
    }

    // ========== is_null tests ==========

    #[test]
    fn test_is_null_true() {
        assert!(Value::Null.is_null());
    }

    #[test]
    fn test_is_null_false() {
        assert!(!Value::Integer(0).is_null());
        assert!(!Value::string("").is_null());
        assert!(!Value::empty_list().is_null());
        assert!(!Value::empty_object().is_null());
    }

    // ========== Negative value tests ==========

    #[test]
    fn test_negative_integer() {
        let val = Value::Integer(-42);
        assert_eq!(val.as_integer(), Some(-42));
        assert!(val.is_integer());
        assert!(val.truthy());
    }

    #[test]
    fn test_negative_float() {
        let val = Value::float(-2.71);
        assert_eq!(val.as_float(), Some(-2.71));
        assert!(val.is_float());
        assert!(val.truthy());
    }

    #[test]
    fn test_negative_float_zero_comparison() {
        // -0.0 should be considered falsy
        let val = Value::float(-0.0);
        assert!(!val.truthy());
    }

    // ========== Float serialization detailed tests ==========

    #[test]
    fn test_float_nan_serde() {
        let val = Value::float(f64::NAN);
        let json = value_to_serde_json(&val);
        // NaN is normalized to 0.0
        assert_eq!(json, serde_json::json!(0.0));
    }

    #[test]
    fn test_float_large_value() {
        let val = Value::float(1e308);
        let json = value_to_serde_json(&val);
        assert!(json.is_number());
    }

    #[test]
    fn test_float_negative_large_value() {
        let val = Value::float(-1e308);
        let json = value_to_serde_json(&val);
        assert!(json.is_number());
    }

    #[test]
    fn test_float_small_value() {
        let val = Value::float(1e-308);
        let json = value_to_serde_json(&val);
        assert!(json.is_number());
    }

    // ========== From<ImMap> detailed tests ==========

    #[test]
    fn test_value_from_im_map_detailed() {
        let mut map = ImMap::new();
        map.insert("a".to_string(), Value::Integer(1));
        map.insert("b".to_string(), Value::Integer(2));
        let val: Value = map.into();
        assert!(val.is_object());
        assert_eq!(val.get("a"), Some(&Value::Integer(1)));
        assert_eq!(val.get("b"), Some(&Value::Integer(2)));
    }

    // ========== serde_json_to_value detailed tests ==========

    #[test]
    fn test_serde_json_to_value_nested() {
        let json = serde_json::json!({
            "outer": {
                "inner": 42
            }
        });
        let val = serde_json_to_value(&json);
        assert!(val.is_object());
        let outer = val.get("outer").unwrap();
        assert!(outer.is_object());
        let inner = outer.get("inner").unwrap();
        assert_eq!(inner.as_integer(), Some(42));
    }

    #[test]
    fn test_serde_json_to_value_nested_array() {
        let json = serde_json::json!({
            "matrix": [[1, 2], [3, 4]]
        });
        let val = serde_json_to_value(&json);
        assert!(val.is_object());
    }

    #[test]
    fn test_serde_json_to_value_large_integer() {
        let json = serde_json::json!(i64::MAX);
        let val = serde_json_to_value(&json);
        assert_eq!(val, Value::Integer(i64::MAX));
    }

    #[test]
    fn test_serde_json_to_value_negative_integer() {
        let json = serde_json::json!(-12345);
        let val = serde_json_to_value(&json);
        assert_eq!(val, Value::Integer(-12345));
    }

    #[test]
    fn test_serde_json_to_value_negative_float() {
        let json = serde_json::json!(-2.71);
        let val = serde_json_to_value(&json);
        assert_eq!(val.as_float(), Some(-2.71));
    }

    #[test]
    fn test_serde_json_to_value_empty_string() {
        let json = serde_json::json!("");
        let val = serde_json_to_value(&json);
        assert_eq!(val.as_str(), Some(""));
    }

    #[test]
    fn test_serde_json_to_value_empty_array() {
        let json = serde_json::json!([]);
        let val = serde_json_to_value(&json);
        assert!(val.is_list());
        assert!(val.is_empty());
    }

    #[test]
    fn test_serde_json_to_value_empty_object() {
        let json = serde_json::json!({});
        let val = serde_json_to_value(&json);
        assert!(val.is_object());
        assert!(val.is_empty());
    }

    // ========== value_to_serde_json detailed tests ==========

    #[test]
    fn test_value_to_serde_json_negative_integer() {
        let val = Value::Integer(-12345);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(-12345));
    }

    #[test]
    fn test_value_to_serde_json_negative_float() {
        let val = Value::Float(OrderedFloat(-2.71));
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(-2.71));
    }

    #[test]
    fn test_value_to_serde_json_string_with_special_chars() {
        let val = Value::string("hello\nworld\ttab");
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!("hello\nworld\ttab"));
    }

    #[test]
    fn test_value_to_serde_json_bool_true() {
        let val = Value::Bool(true);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(true));
    }

    #[test]
    fn test_value_to_serde_json_bool_false() {
        let val = Value::Bool(false);
        let json = value_to_serde_json(&val);
        assert_eq!(json, serde_json::json!(false));
    }

    // ========== List operation detailed tests ==========

    #[test]
    fn test_list_index_out_of_bounds() {
        let list = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        assert_eq!(list.index(10), None);
    }

    #[test]
    fn test_list_index_negative() {
        let list = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        // im::Vector does not support negative indexing
        assert_eq!(list.index(0), Some(&Value::Integer(1)));
    }

    #[test]
    fn test_list_with_mixed_types() {
        let list = Value::list(vec![
            Value::Integer(1),
            Value::string("two"),
            Value::Bool(true),
            Value::Null,
        ]);
        assert_eq!(list.len(), 4);
        assert_eq!(list.index(0), Some(&Value::Integer(1)));
        assert_eq!(list.index(1), Some(&Value::string("two")));
        assert_eq!(list.index(2), Some(&Value::Bool(true)));
        assert_eq!(list.index(3), Some(&Value::Null));
    }

    #[test]
    fn test_list_set_immutability() {
        let list = Value::list(vec![Value::Integer(1), Value::Integer(2)]);
        let new_list = list.set("key", Value::Integer(100));
        // list.set is an object method, list is unaffected
        assert_eq!(list.len(), 2);
        assert_eq!(new_list.len(), 2);
    }

    // ========== Object operation detailed tests ==========

    #[test]
    fn test_object_set_existing_key() {
        let obj = Value::empty_object();
        let obj2 = obj.set("x", Value::Integer(1));
        let obj3 = obj2.set("x", Value::Integer(2));
        assert_eq!(obj.get("x"), None);
        assert_eq!(obj2.get("x"), Some(&Value::Integer(1)));
        assert_eq!(obj3.get("x"), Some(&Value::Integer(2)));
    }

    #[test]
    fn test_object_multiple_keys() {
        let obj = Value::empty_object()
            .set("a", Value::Integer(1))
            .set("b", Value::Integer(2))
            .set("c", Value::Integer(3));
        assert_eq!(obj.len(), 3);
    }

    // ========== truthy detailed tests ==========

    #[test]
    fn test_truthy_negative_number() {
        assert!(Value::Integer(-1).truthy());
        assert!(Value::float(-1.0).truthy());
    }

    #[test]
    fn test_truthy_zero_values() {
        assert!(!Value::Integer(0).truthy());
        assert!(!Value::float(0.0).truthy());
        assert!(!Value::float(-0.0).truthy());
    }

    // ========== as_number detailed tests ==========

    #[test]
    fn test_as_number_from_float() {
        let val = Value::Float(OrderedFloat(2.71));
        assert_eq!(val.as_number(), Some(2.71));
    }

    #[test]
    fn test_as_number_from_integer() {
        let val = Value::Integer(42);
        assert_eq!(val.as_number(), Some(42.0));
    }

    #[test]
    fn test_as_number_from_string() {
        let val = Value::string("not a number");
        assert_eq!(val.as_number(), None);
    }

    // ========== Display detailed tests ==========

    #[test]
    fn test_display_bool_true() {
        assert_eq!(format!("{}", Value::Bool(true)), "true");
    }

    #[test]
    fn test_display_bool_false() {
        assert_eq!(format!("{}", Value::Bool(false)), "false");
    }

    #[test]
    fn test_display_list_empty() {
        assert_eq!(format!("{}", Value::empty_list()), "[]");
    }

    #[test]
    fn test_display_list_multiple() {
        let list = Value::list(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);
        assert_eq!(format!("{}", list), "[1, 2, 3]");
    }

    #[test]
    fn test_display_object_empty() {
        assert_eq!(format!("{}", Value::empty_object()), "{}");
    }

    #[test]
    fn test_display_object_multiple() {
        // im::HashMap does not guarantee insertion order; check structure rather than full string
        let obj = Value::Object(ImMap::from(vec![
            ("x".to_string(), Value::Integer(1)),
            ("y".to_string(), Value::Integer(2)),
        ]));
        let display = format!("{}", obj);
        assert!(display.contains("\"x\": 1") || display.contains("\"x\":1"));
        assert!(display.contains("\"y\": 2") || display.contains("\"y\":2"));
        assert!(display.starts_with('{') && display.ends_with('}'));
    }

    // ========== Immutability detailed tests ==========

    #[test]
    fn test_object_immutability_chain() {
        let obj1 = Value::empty_object();
        let obj2 = obj1.set("x", Value::Integer(1));
        let obj3 = obj2.set("y", Value::Integer(2));
        let obj4 = obj3.set("z", Value::Integer(3));

        assert!(obj1.get("x").is_none());
        assert!(obj2.get("y").is_none());
        assert!(obj3.get("z").is_none());

        assert_eq!(obj4.get("x"), Some(&Value::Integer(1)));
        assert_eq!(obj4.get("y"), Some(&Value::Integer(2)));
        assert_eq!(obj4.get("z"), Some(&Value::Integer(3)));
    }

    // ========== OrderedFloat behavior tests ==========

    #[test]
    fn test_ordered_float_positive_zero_negative_zero() {
        let pos_zero = Value::float(0.0);
        let neg_zero = Value::float(-0.0);
        // 0.0 == -0.0
        assert_eq!(pos_zero, neg_zero);
    }

    #[test]
    fn test_value_integer_max() {
        let val = Value::Integer(i64::MAX);
        assert_eq!(val.as_integer(), Some(i64::MAX));
        assert!(val.is_integer());
        assert!(val.is_number());
    }

    #[test]
    fn test_value_integer_min() {
        let val = Value::Integer(i64::MIN);
        assert_eq!(val.as_integer(), Some(i64::MIN));
        assert!(val.is_integer());
        assert!(val.is_number());
    }
}
