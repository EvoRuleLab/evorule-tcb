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

//! Domain matching — `EvoRule`'s conditional decision system.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! Domain is the "condition part" of an `EvoRule`. Rule conditions are expressed via Domain.
//! The core method `contains(state)` determines whether the state satisfies the domain.
//!
//! Variants: Atom (atomic condition), And, Or, Not (logical combinations),
//! Instruction (instruction type), Universal (always matches), Empty (never matches).

//! # Determinism Guarantee
//!
//! `Domain` evaluation is **fully deterministic** at L1:
//! - All operators (`Eq`, `Gt`, `Contains`, etc.) are pure functions.
//! - `And`/`Or`/`Not` follow standard Boolean logic.
//! - `Universal` always returns `true`; `Empty` always returns `false`.
//! - `Contains` / `NotContains` only work with list elements (not strings).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | Float comparisons (`OrderedFloat`) | ✅ L1 deterministic | NaN normalized to 0.0 |
//!
//! # Cross-Platform Note
//!
//! - `OrderedFloat` behavior is consistent across all Rust platforms.
//! - Regex matching (`Matches`/`NotMatches`) has been moved to Governance layer.

use crate::state::State;
use crate::value::Value;

/// Relational operators.
///
/// # Moved Out of TCB
///
/// The following operators were moved to the Governance layer because they perform
/// text processing, which violates the TCB's JSON-structured-data-only design principle:
/// - `Matches` / `NotMatches` (regex matching)
/// - `StartsWith` / `EndsWith` (string prefix/suffix matching)
/// - String substring `Contains` / `NotContains`
///
/// In TCB, `Contains` / `NotContains` only work with list elements (not strings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelOp {
    /// Equal to
    Eq,
    /// Not equal to
    Ne,
    /// Greater than
    Gt,
    /// Greater than or equal to
    Ge,
    /// Less than
    Lt,
    /// Less than or equal to
    Le,
    /// Contains (list element check only)
    Contains,
    /// Does not contain
    NotContains,
    /// In a set (list membership only)
    In,
    /// Not in a set
    NotIn,
    /// In a numeric range (closed interval [min, max])
    Between,
    /// Not in a numeric range
    NotBetween,
    /// Attribute exists
    Exists,
    /// Attribute does not exist
    NotExists,
}

impl RelOp {
    /// Parse a `RelOp` from a string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "eq" => Some(Self::Eq),
            "ne" => Some(Self::Ne),
            "gt" => Some(Self::Gt),
            "ge" => Some(Self::Ge),
            "lt" => Some(Self::Lt),
            "le" => Some(Self::Le),
            "contains" | "has" => Some(Self::Contains),
            "not_contains" | "lacks" => Some(Self::NotContains),
            "in" => Some(Self::In),
            "not_in" => Some(Self::NotIn),
            "between" => Some(Self::Between),
            "not_between" => Some(Self::NotBetween),
            "exists" => Some(Self::Exists),
            "not_exists" => Some(Self::NotExists),
            _ => None,
        }
    }

    /// Convert to a string representation.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Contains => "contains",
            Self::NotContains => "not_contains",
            Self::In => "in",
            Self::NotIn => "not_in",
            Self::Between => "between",
            Self::NotBetween => "not_between",
            Self::Exists => "exists",
            Self::NotExists => "not_exists",
        }
    }

    /// Apply the relational operator to compare two values.
    ///
    /// Note: `Exists` and `NotExists` are short-circuited in `Domain::contains`
    /// and never reach here under normal control flow. As a defensive measure
    /// against contract violations, this function returns `false` (rather than
    /// panicking) if they are ever invoked directly, preserving TCB liveness
    /// (ER-600: no runtime panics in production).
    pub fn apply(&self, actual: &Value, expected: &Value) -> bool {
        match self {
            Self::Eq => values_equal(actual, expected),
            Self::Ne => !values_equal(actual, expected),
            Self::Gt => compare_values(actual, expected) == Some(std::cmp::Ordering::Greater),
            Self::Ge => {
                matches!(
                    compare_values(actual, expected),
                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                )
            }
            Self::Lt => compare_values(actual, expected) == Some(std::cmp::Ordering::Less),
            Self::Le => {
                matches!(
                    compare_values(actual, expected),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            }
            Self::Contains => value_contains(actual, expected),
            Self::NotContains => !value_contains(actual, expected),
            Self::In => value_in(actual, expected),
            Self::NotIn => !value_in(actual, expected),
            Self::Between => value_between(actual, expected),
            Self::NotBetween => !value_between(actual, expected),
            // Defensive fallback: Exists/NotExists are short-circuited in
            // `Domain::contains`. If they reach here due to a contract
            // violation, return false rather than panicking to preserve TCB
            // liveness (ER-600: no runtime panics in production).
            Self::Exists | Self::NotExists => false,
        }
    }
}

/// Domain type — the conditional part of a rule.
///
/// Every rule contains a domain that determines whether the rule applies to the current state.
#[derive(Debug, Clone, PartialEq)]
pub enum Domain {
    /// Atomic domain — a single condition.
    /// Checks whether a state attribute satisfies a relational operator.
    Atom {
        attribute: String,
        op: RelOp,
        value: Value,
    },
    /// And — all subdomains must match.
    And(Vec<Self>),
    /// Or — any subdomain must match.
    Or(Vec<Self>),
    /// Not — the subdomain must not match.
    Not(Box<Self>),
    /// Instruction domain — matches a specific instruction type.
    Instruction(String),
    /// Universal — unconditionally matches.
    Universal,
    /// Empty — never matches.
    Empty,
}

const MAX_DOMAIN_DEPTH: usize = 128;

impl Domain {
    /// Determine whether the state satisfies this domain.
    pub fn contains(&self, state: &State) -> bool {
        self.contains_inner(state, 0)
    }

    fn contains_inner(&self, state: &State, depth: usize) -> bool {
        if depth > MAX_DOMAIN_DEPTH {
            return false;
        }
        match self {
            Self::Atom {
                attribute,
                op,
                value,
            } => {
                let actual = state.get_path(attribute);
                match actual {
                    Some(actual_value) => match op {
                        RelOp::Exists => true,
                        RelOp::NotExists => false,
                        _ => op.apply(&actual_value, value),
                    },
                    // Attribute does not exist
                    None => match op {
                        RelOp::Eq => false,
                        RelOp::Ne => true,
                        RelOp::Exists => false,
                        RelOp::NotExists => true,
                        _ => false,
                    },
                }
            }
            Self::And(domains) => domains.iter().all(|d| d.contains_inner(state, depth + 1)),
            Self::Or(domains) => domains.iter().any(|d| d.contains_inner(state, depth + 1)),
            Self::Not(inner) => !inner.contains_inner(state, depth + 1),
            Self::Instruction(instr_type) => {
                // Aligns with v2: check __exec__.instruction.type
                match state
                    .get("__exec__")
                    .and_then(|v| v.get("instruction"))
                    .and_then(|v| v.get("type"))
                {
                    Some(actual_type) => values_equal(actual_type, &Value::string(instr_type)),
                    None => false,
                }
            }
            Self::Universal => true,
            Self::Empty => false,
        }
    }

    /// Create a Domain from a dictionary.
    pub fn from_dict(
        data: &std::collections::HashMap<String, Value>,
    ) -> Result<Self, crate::error::EvoRuleError> {
        use crate::error::{invalid_domain_type, invalid_param, missing_param};

        let dtype = data
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| missing_param("Domain::from_dict", "type"))?;

        match dtype {
            "atom" => {
                let attribute = data
                    .get("attribute")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| missing_param("Domain::from_dict", "attribute"))?
                    .to_string();
                let op_str = data
                    .get("op")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| missing_param("Domain::from_dict", "op"))?;
                let op = RelOp::parse(op_str).ok_or_else(|| {
                    invalid_param(
                        "Domain::from_dict",
                        "op",
                        &format!("Unknown operator: {op_str}"),
                    )
                })?;
                let value = match op {
                    RelOp::Exists | RelOp::NotExists => {
                        // exists/not_exists do not require a value; default to Null
                        data.get("value").cloned().unwrap_or(Value::Null)
                    }
                    _ => data
                        .get("value")
                        .cloned()
                        .ok_or_else(|| missing_param("Domain::from_dict", "value"))?,
                };
                Ok(Self::Atom {
                    attribute,
                    op,
                    value,
                })
            }
            "and" => {
                // Prefer the `domains` list format, fall back to binary `left`/`right` format
                if data.contains_key("domains") {
                    let domains = parse_domain_list(data, "domains")?;
                    Ok(Self::And(domains))
                } else if let (Some(left_val), Some(right_val)) =
                    (data.get("left"), data.get("right"))
                {
                    let left = domain_from_value(left_val)?;
                    let right = domain_from_value(right_val)?;
                    Ok(Self::And(vec![left, right]))
                } else {
                    let domains = parse_domain_list(data, "domains")?;
                    Ok(Self::And(domains))
                }
            }
            "or" => {
                // Prefer the `domains` list format, fall back to binary `left`/`right` format
                if data.contains_key("domains") {
                    let domains = parse_domain_list(data, "domains")?;
                    Ok(Self::Or(domains))
                } else if let (Some(left_val), Some(right_val)) =
                    (data.get("left"), data.get("right"))
                {
                    let left = domain_from_value(left_val)?;
                    let right = domain_from_value(right_val)?;
                    Ok(Self::Or(vec![left, right]))
                } else {
                    let domains = parse_domain_list(data, "domains")?;
                    Ok(Self::Or(domains))
                }
            }
            "not" => {
                let inner_data = data
                    .get("inner")
                    .ok_or_else(|| missing_param("Domain::from_dict", "inner"))?;
                let inner = domain_from_value(inner_data)?;
                Ok(Self::Not(Box::new(inner)))
            }
            "instruction" => {
                // Compatibility: v2 uses "instruction_type", v4 uses "value"
                let instr = data
                    .get("instruction_type")
                    .or_else(|| data.get("value"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| missing_param("Domain::from_dict", "instruction_type"))?
                    .to_string();
                Ok(Self::Instruction(instr))
            }
            "universal" => Ok(Self::Universal),
            "empty" | "never" => Ok(Self::Empty),
            _ => Err(invalid_domain_type(dtype)),
        }
    }

    /// Convert to a dictionary.
    pub fn to_dict(&self) -> std::collections::HashMap<String, Value> {
        use std::collections::HashMap;
        match self {
            Self::Atom {
                attribute,
                op,
                value,
            } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), Value::string("atom"));
                map.insert("attribute".to_string(), Value::string(attribute));
                map.insert("op".to_string(), Value::string(op.as_str()));
                map.insert("value".to_string(), value.clone());
                map
            }
            Self::And(domains) => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), Value::string("and"));
                let list: Vec<Value> = domains
                    .iter()
                    .map(|d| {
                        let m = d.to_dict();
                        Value::Object(im::HashMap::from(m))
                    })
                    .collect();
                map.insert("domains".to_string(), Value::list(list));
                map
            }
            Self::Or(domains) => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), Value::string("or"));
                let list: Vec<Value> = domains
                    .iter()
                    .map(|d| {
                        let m = d.to_dict();
                        Value::Object(im::HashMap::from(m))
                    })
                    .collect();
                map.insert("domains".to_string(), Value::list(list));
                map
            }
            Self::Not(inner) => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), Value::string("not"));
                let inner_map = inner.to_dict();
                map.insert(
                    "inner".to_string(),
                    Value::Object(im::HashMap::from(inner_map)),
                );
                map
            }
            Self::Instruction(instr) => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), Value::string("instruction"));
                map.insert("value".to_string(), Value::string(instr));
                map
            }
            Self::Universal => {
                let mut map = std::collections::HashMap::new();
                map.insert("type".to_string(), Value::string("universal"));
                map
            }
            Self::Empty => {
                let mut map = std::collections::HashMap::new();
                map.insert("type".to_string(), Value::string("empty"));
                map
            }
        }
    }

    /// Create a Domain from a Value (supports both Object and String).
    pub fn from_value(val: &Value) -> Result<Self, crate::error::EvoRuleError> {
        domain_from_value(val)
    }

    /// Serialize the Domain to a Value (for injection into the __exec__ context).
    pub fn to_value(&self) -> Value {
        match self {
            Self::Atom {
                attribute,
                op,
                value,
            } => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("atom"),
                "attribute".to_string() => Value::string(attribute),
                "op".to_string() => Value::string(op.as_str()),
                "value".to_string() => value.clone(),
            }),
            Self::And(domains) => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("and"),
                "domains".to_string() => Value::List(domains.iter().map(Self::to_value).collect()),
            }),
            Self::Or(domains) => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("or"),
                "domains".to_string() => Value::List(domains.iter().map(Self::to_value).collect()),
            }),
            Self::Not(inner) => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("not"),
                "inner".to_string() => inner.to_value(),  // Consistent with from_value() "inner" field name
            }),
            Self::Instruction(instr) => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("instruction"),
                "instruction_type".to_string() => Value::string(instr),
                "op".to_string() => Value::string("eq"),
            }),
            Self::Universal => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("universal"),
            }),
            Self::Empty => Value::Object(im::hashmap! {
                "type".to_string() => Value::string("empty"),
            }),
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Atom {
                attribute,
                op,
                value,
            } => write!(f, "{} {} {}", attribute, op.as_str(), value),
            Self::And(domains) => {
                let parts: Vec<String> = domains.iter().map(|d| format!("{d}")).collect();
                write!(f, "({})", parts.join(" AND "))
            }
            Self::Or(domains) => {
                let parts: Vec<String> = domains.iter().map(|d| format!("{d}")).collect();
                write!(f, "({})", parts.join(" OR "))
            }
            Self::Not(inner) => write!(f, "NOT ({inner})"),
            Self::Instruction(instr) => write!(f, "instruction:{instr}"),
            Self::Universal => write!(f, "*"),
            Self::Empty => write!(f, "∅"),
        }
    }
}

// ══════════════════════════════════════════════
// Helper functions
// ══════════════════════════════════════════════

/// Determine whether two Values are equal (including loose numeric comparison).
///
/// Delegates to `Value::loosely_equals` to avoid duplication with `compute_ops.rs`.
fn values_equal(a: &Value, b: &Value) -> bool {
    a.loosely_equals(b)
}

/// Compare the order of two values.
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a.as_number(), b.as_number()) {
        (Some(af), Some(bf)) => {
            if af < bf {
                Some(std::cmp::Ordering::Less)
            } else if af > bf {
                Some(std::cmp::Ordering::Greater)
            } else {
                Some(std::cmp::Ordering::Equal)
            }
        }
        _ => {
            // String comparison
            match (a.as_str(), b.as_str()) {
                (Some(as_), Some(bs)) => Some(as_.cmp(bs)),
                _ => None,
            }
        }
    }
}

/// Check whether `actual` contains `expected` (list element check only).
///
/// Note: String substring contains was moved to the Governance layer as
/// `contains_substring` primitive. In TCB, `contains` only works with lists.
fn value_contains(actual: &Value, expected: &Value) -> bool {
    match actual {
        Value::List(v) => v.iter().any(|item| values_equal(item, expected)),
        _ => false,
    }
}

/// Check whether `actual` is in the `expected` set (list membership only).
///
/// Note: String substring membership was moved to the Governance layer.
/// In TCB, `in` only works with lists.
fn value_in(actual: &Value, expected: &Value) -> bool {
    match expected {
        Value::List(v) => v.iter().any(|item| values_equal(item, actual)),
        _ => false,
    }
}

/// Check whether a number is in a closed interval [min, max].
/// `expected` must be a list containing min and max: [min, max]
fn value_between(actual: &Value, expected: &Value) -> bool {
    let list = match expected {
        Value::List(v) if v.len() >= 2 => v.clone(),
        _ => return false,
    };

    let min = match &list[0] {
        Value::Integer(i) => *i as f64,
        Value::Float(f) => f.0,
        _ => return false,
    };

    let max = match &list[1] {
        Value::Integer(i) => *i as f64,
        Value::Float(f) => f.0,
        _ => return false,
    };

    let actual_num = match actual.as_number() {
        Some(n) => n,
        None => return false,
    };

    actual_num >= min && actual_num <= max
}

/// Parse a list of domains from a Value object.
fn parse_domain_list(
    data: &std::collections::HashMap<String, Value>,
    key: &str,
) -> Result<Vec<Domain>, crate::error::EvoRuleError> {
    use crate::error::{invalid_param, missing_param};

    let list_val = data
        .get(key)
        .ok_or_else(|| missing_param("Domain::from_dict", key))?;

    match list_val {
        Value::List(v) => {
            let mut domains = Vec::new();
            for item in v {
                domains.push(domain_from_value(item)?);
            }
            Ok(domains)
        }
        _ => Err(invalid_param("Domain::from_dict", key, "must be a list")),
    }
}

/// Create a Domain from a Value.
fn domain_from_value(val: &Value) -> Result<Domain, crate::error::EvoRuleError> {
    use crate::error::invalid_param;

    match val {
        Value::Object(map) => {
            let mut std_map = std::collections::HashMap::new();
            for (k, v) in map.iter() {
                std_map.insert(k.clone(), v.clone());
            }
            Domain::from_dict(&std_map)
        }
        Value::String(s) if s == "universal" => Ok(Domain::Universal),
        Value::String(s) if s == "empty" => Ok(Domain::Empty),
        Value::String(s) => Ok(Domain::Instruction(s.clone())),
        _ => Err(invalid_param(
            "Domain::from_value",
            "value",
            &format!("Cannot create Domain from {val:?}"),
        )),
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[allow(clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_eq_true() {
        let state = State::new(vec![("x", Value::Integer(10))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Eq,
            value: Value::Integer(10),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_atom_eq_false() {
        let state = State::new(vec![("x", Value::Integer(10))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Eq,
            value: Value::Integer(20),
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_and_all_true() {
        let state = State::new(vec![("x", Value::Integer(10)), ("y", Value::string("hi"))]);
        let domain = Domain::And(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Gt,
                value: Value::Integer(5),
            },
            Domain::Atom {
                attribute: "y".to_string(),
                op: RelOp::Eq,
                value: Value::string("hi"),
            },
        ]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_and_one_false() {
        let state = State::new(vec![("x", Value::Integer(1))]);
        let domain = Domain::And(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Eq,
                value: Value::Integer(1),
            },
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Eq,
                value: Value::Integer(2),
            },
        ]);
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_or_one_true() {
        let state = State::new(vec![("x", Value::Integer(1))]);
        let domain = Domain::Or(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Eq,
                value: Value::Integer(1),
            },
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Eq,
                value: Value::Integer(2),
            },
        ]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_not() {
        let state = State::new(vec![("x", Value::Integer(1))]);
        let domain = Domain::Not(Box::new(Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Eq,
            value: Value::Integer(2),
        }));
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_universal() {
        let state = State::new(vec![("x", Value::Integer(1))]);
        assert!(Domain::Universal.contains(&state));
    }

    #[test]
    fn test_empty() {
        let state = State::new(vec![("x", Value::Integer(1))]);
        assert!(!Domain::Empty.contains(&state));
    }

    #[test]
    fn test_from_dict_atom() {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), Value::string("atom"));
        data.insert("attribute".to_string(), Value::string("x"));
        data.insert("op".to_string(), Value::string("gt"));
        data.insert("value".to_string(), Value::Integer(5));
        let domain = Domain::from_dict(&data).unwrap();
        let state = State::new(vec![("x", Value::Integer(10))]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_from_dict_and() {
        let mut and_data = std::collections::HashMap::new();
        and_data.insert("type".to_string(), Value::string("and"));

        let sub_domain1 = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(1)),
        ]));
        let sub_domain2 = Value::from(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("y")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(2)),
        ]));
        and_data.insert(
            "domains".to_string(),
            Value::list(vec![sub_domain1, sub_domain2]),
        );

        let domain = Domain::from_dict(&and_data).unwrap();
        let state = State::new(vec![("x", Value::Integer(1)), ("y", Value::Integer(2))]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_roundtrip_to_dict() {
        let original = Domain::And(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Gt,
                value: Value::Integer(5),
            },
            Domain::Or(vec![
                Domain::Atom {
                    attribute: "y".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("a"),
                },
                Domain::Atom {
                    attribute: "y".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("b"),
                },
            ]),
        ]);
        let dict = original.to_dict();
        let reconstructed = Domain::from_dict(&dict).unwrap();
        assert_eq!(format!("{}", original), format!("{}", reconstructed));
    }

    #[test]
    fn test_relop_parse() {
        assert_eq!(RelOp::parse("eq"), Some(RelOp::Eq));
        assert_eq!(RelOp::parse("ne"), Some(RelOp::Ne));
        assert_eq!(RelOp::parse("gt"), Some(RelOp::Gt));
        assert_eq!(RelOp::parse("unknown"), None);
    }

    #[test]
    fn test_contains_operator() {
        let state = State::new(vec![(
            "tags",
            Value::list(vec![Value::string("admin"), Value::string("user")]),
        )]);
        let domain = Domain::Atom {
            attribute: "tags".to_string(),
            op: RelOp::Contains,
            value: Value::string("admin"),
        };
        assert!(domain.contains(&state));
    }

    // ========== Additional RelOp tests ==========

    #[test]
    fn test_between_true() {
        let state = State::new(vec![("x", Value::Integer(5))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_between_false_out_of_range() {
        let state = State::new(vec![("x", Value::Integer(15))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_between_false_below_range() {
        let state = State::new(vec![("x", Value::Integer(0))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_between_edge_cases() {
        // Test boundary values (closed interval)
        let state_min = State::new(vec![("x", Value::Integer(1))]);
        let state_max = State::new(vec![("x", Value::Integer(10))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(domain.contains(&state_min));
        assert!(domain.contains(&state_max));
    }

    #[test]
    fn test_not_between_true() {
        let state = State::new(vec![("x", Value::Integer(15))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::NotBetween,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_not_between_false_in_range() {
        let state = State::new(vec![("x", Value::Integer(5))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::NotBetween,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_in_operator() {
        let state = State::new(vec![("color", Value::string("red"))]);
        let domain = Domain::Atom {
            attribute: "color".to_string(),
            op: RelOp::In,
            value: Value::list(vec![
                Value::string("red"),
                Value::string("green"),
                Value::string("blue"),
            ]),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_not_in_operator() {
        let state = State::new(vec![("color", Value::string("yellow"))]);
        let domain = Domain::Atom {
            attribute: "color".to_string(),
            op: RelOp::NotIn,
            value: Value::list(vec![
                Value::string("red"),
                Value::string("green"),
                Value::string("blue"),
            ]),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_not_contains() {
        let state = State::new(vec![("msg", Value::string("hello"))]);
        let domain = Domain::Atom {
            attribute: "msg".to_string(),
            op: RelOp::NotContains,
            value: Value::string("world"),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_between_float() {
        let state = State::new(vec![("temp", Value::float(25.5))]);
        let domain = Domain::Atom {
            attribute: "temp".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::float(20.0), Value::float(30.0)]),
        };
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_complex_domain_nested() {
        // Test a complex nested Domain
        let state = State::new(vec![
            ("age", Value::Integer(25)),
            ("name", Value::string("John")),
            ("email", Value::string("john@example.com")),
        ]);
        let domain = Domain::And(vec![Domain::Atom {
            attribute: "age".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(18), Value::Integer(100)]),
        }]);
        assert!(domain.contains(&state));
    }

    // ========== Domain type conversion tests ==========

    #[test]
    fn test_from_dict_universal() {
        let data =
            std::collections::HashMap::from([("type".to_string(), Value::string("universal"))]);
        let domain = Domain::from_dict(&data).unwrap();
        assert!(matches!(domain, Domain::Universal));
    }

    #[test]
    fn test_from_dict_empty_alias() {
        // "empty" and "never" are aliases for Domain::Empty
        let data1 = std::collections::HashMap::from([("type".to_string(), Value::string("empty"))]);
        let data2 = std::collections::HashMap::from([("type".to_string(), Value::string("never"))]);
        assert!(matches!(Domain::from_dict(&data1).unwrap(), Domain::Empty));
        assert!(matches!(Domain::from_dict(&data2).unwrap(), Domain::Empty));
    }

    #[test]
    fn test_from_dict_instruction() {
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("instruction")),
            ("value".to_string(), Value::string("eval")),
        ]);
        let domain = Domain::from_dict(&data).unwrap();
        assert!(matches!(domain, Domain::Instruction(s) if s == "eval"));
    }

    #[test]
    fn test_from_dict_not() {
        let inner_data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(0)),
        ]);
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("not")),
            ("inner".to_string(), Value::from(inner_data)),
        ]);
        let domain = Domain::from_dict(&data).unwrap();
        assert!(matches!(domain, Domain::Not(_)));

        // Verify Not logic: x == 0 is false, Not should be true (when x != 0)
        let state = State::new(vec![("x", Value::Integer(5))]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_from_dict_or() {
        let sub1 = Value::Object(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(1)),
        ]));
        let sub2 = Value::Object(im::HashMap::from(vec![
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(2)),
        ]));
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("or")),
            ("domains".to_string(), Value::list(vec![sub1, sub2])),
        ]);
        let domain = Domain::from_dict(&data).unwrap();
        assert!(matches!(domain, Domain::Or(_)));

        let state1 = State::new(vec![("x", Value::Integer(1))]);
        let state2 = State::new(vec![("x", Value::Integer(2))]);
        let state3 = State::new(vec![("x", Value::Integer(3))]);
        assert!(domain.contains(&state1));
        assert!(domain.contains(&state2));
        assert!(!domain.contains(&state3));
    }

    #[test]
    fn test_to_dict_roundtrip_universal() {
        let original = Domain::Universal;
        let dict = original.to_dict();
        let reconstructed = Domain::from_dict(&dict).unwrap();
        assert!(matches!(reconstructed, Domain::Universal));
    }

    #[test]
    fn test_to_dict_roundtrip_empty() {
        let original = Domain::Empty;
        let dict = original.to_dict();
        let reconstructed = Domain::from_dict(&dict).unwrap();
        assert!(matches!(reconstructed, Domain::Empty));
    }

    #[test]
    fn test_to_dict_roundtrip_instruction() {
        let original = Domain::Instruction("test_instr".to_string());
        let dict = original.to_dict();
        let reconstructed = Domain::from_dict(&dict).unwrap();
        assert!(matches!(reconstructed, Domain::Instruction(s) if s == "test_instr"));
    }

    #[test]
    fn test_to_dict_roundtrip_not() {
        let original = Domain::Not(Box::new(Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Gt,
            value: Value::Integer(0),
        }));
        let dict = original.to_dict();
        let reconstructed = Domain::from_dict(&dict).unwrap();
        assert!(matches!(reconstructed, Domain::Not(_)));
    }

    // ========== RelOp alias tests ==========

    #[test]
    fn test_relop_aliases() {
        // contains / has
        assert_eq!(RelOp::parse("has"), Some(RelOp::Contains));
        assert_eq!(RelOp::parse("contains"), Some(RelOp::Contains));

        // not_contains / lacks
        assert_eq!(RelOp::parse("lacks"), Some(RelOp::NotContains));
        assert_eq!(RelOp::parse("not_contains"), Some(RelOp::NotContains));
    }

    #[test]
    fn test_relop_as_str_all_variants() {
        // Verify all RelOp variants can convert to strings
        let variants = [
            (RelOp::Eq, "eq"),
            (RelOp::Ne, "ne"),
            (RelOp::Gt, "gt"),
            (RelOp::Ge, "ge"),
            (RelOp::Lt, "lt"),
            (RelOp::Le, "le"),
            (RelOp::Contains, "contains"),
            (RelOp::NotContains, "not_contains"),
            (RelOp::In, "in"),
            (RelOp::NotIn, "not_in"),
            (RelOp::Between, "between"),
            (RelOp::NotBetween, "not_between"),
        ];

        for (op, expected_str) in variants {
            assert_eq!(op.as_str(), expected_str);
            // Verify roundtrip
            assert_eq!(RelOp::parse(expected_str), Some(op.clone()));
        }
    }

    // ========== Error handling tests ==========

    #[test]
    fn test_from_dict_missing_type() {
        let data = std::collections::HashMap::from([("attribute".to_string(), Value::string("x"))]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_invalid_type() {
        let data =
            std::collections::HashMap::from([("type".to_string(), Value::string("invalid_type"))]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_atom_missing_attribute() {
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("atom")),
            ("op".to_string(), Value::string("eq")),
            ("value".to_string(), Value::Integer(5)),
        ]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_atom_missing_op() {
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("value".to_string(), Value::Integer(5)),
        ]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_atom_invalid_op() {
        let data = std::collections::HashMap::from([
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("x")),
            ("op".to_string(), Value::string("invalid_op")),
            ("value".to_string(), Value::Integer(5)),
        ]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_and_missing_domains() {
        let data = std::collections::HashMap::from([("type".to_string(), Value::string("and"))]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_not_missing_inner() {
        let data = std::collections::HashMap::from([("type".to_string(), Value::string("not"))]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_instruction_missing_value() {
        let data =
            std::collections::HashMap::from([("type".to_string(), Value::string("instruction"))]);
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_relop_case_sensitivity() {
        // RelOp string matching should be case-sensitive
        assert_eq!(RelOp::parse("GT"), None);
        assert_eq!(RelOp::parse("Eq"), None);
        assert_eq!(RelOp::parse("EQ"), None);
        assert_eq!(RelOp::parse("gt"), Some(RelOp::Gt));
        assert_eq!(RelOp::parse("eq"), Some(RelOp::Eq));
    }

    // ══════════════════════════════════════════════
    // Additional coverage: helper function error paths
    // ══════════════════════════════════════════════

    #[test]
    fn test_compare_values_incompatible_types_returns_none() {
        // compare_values on incompatible types returns None (Integer vs String)
        use crate::domain::Domain;
        let state = State::new(vec![("x", Value::Integer(5))]);
        let domain = Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Gt,
            value: Value::string("abc"), // Integer vs String → None → Gt → false
        };
        // Should not panic; Gt comparison returns false
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_compare_values_bool_vs_number_returns_none() {
        // Bool vs number comparison returns None
        let state = State::new(vec![("flag", Value::Bool(true))]);
        let domain = Domain::Atom {
            attribute: "flag".to_string(),
            op: RelOp::Lt,
            value: Value::Integer(10), // Bool vs Integer → None → Lt → false
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_value_contains_non_string_non_list_returns_false() {
        // value_contains on a non-string/non-list actual value returns false
        let state = State::new(vec![("n", Value::Integer(42))]);
        let domain = Domain::Atom {
            attribute: "n".to_string(),
            op: RelOp::Contains,
            value: Value::Integer(2), // Integer does not support contains → false
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_value_in_non_list_returns_false() {
        // value_in on a non-list expected returns false
        let state = State::new(vec![("s", Value::string("abc"))]);
        let domain = Domain::Atom {
            attribute: "s".to_string(),
            op: RelOp::In,
            value: Value::Integer(123), // Integer does not support in → false
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_between_with_non_numeric_value_returns_false() {
        // value_between on non-numeric values returns false
        let state = State::new(vec![("name", Value::string("alice"))]);
        let domain = Domain::Atom {
            attribute: "name".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(1), Value::Integer(10)]),
        };
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_domain_instruction_matches_instruction_type() {
        // Instruction domain checks __exec__.instruction.type
        let state = State::new(vec![(
            "__exec__",
            Value::from(im::HashMap::from(vec![(
                "instruction".to_string(),
                Value::from(im::HashMap::from(vec![(
                    "type".to_string(),
                    Value::string("validate"),
                )])),
            )])),
        )]);
        let domain = Domain::Instruction("validate".to_string());
        assert!(domain.contains(&state));

        // Type mismatch → false
        let domain2 = Domain::Instruction("compute".to_string());
        assert!(!domain2.contains(&state));
    }

    #[test]
    fn test_domain_instruction_no_exec_returns_false() {
        // When __exec__ doesn't exist, Instruction domain returns false
        let state = State::new(vec![("x", Value::Integer(5))]);
        let domain = Domain::Instruction("validate".to_string());
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_domain_not_containing_and() {
        // Not(And(...)) correctly negates
        let state = State::new(vec![("x", Value::Integer(5)), ("y", Value::Integer(20))]);
        // And: x > 0 AND y < 10 → true AND false → false
        // Not: !false → true
        let inner = Domain::And(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Gt,
                value: Value::Integer(0),
            },
            Domain::Atom {
                attribute: "y".to_string(),
                op: RelOp::Lt,
                value: Value::Integer(10),
            },
        ]);
        let domain = Domain::Not(Box::new(inner));
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_domain_not_containing_or() {
        // Not(Or(...)) correctly negates
        let state = State::new(vec![("x", Value::Integer(5))]);
        // Or: x > 0 OR x < -10 → true OR false → true
        // Not: !true → false
        let inner = Domain::Or(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Gt,
                value: Value::Integer(0),
            },
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Lt,
                value: Value::Integer(-10),
            },
        ]);
        let domain = Domain::Not(Box::new(inner));
        assert!(!domain.contains(&state));
    }

    #[test]
    fn test_relop_aliases_has_and_lacks() {
        // contains alias: has; not_contains alias: lacks
        assert_eq!(RelOp::parse("has"), Some(RelOp::Contains));
        assert_eq!(RelOp::parse("lacks"), Some(RelOp::NotContains));
    }

    #[test]
    fn test_atom_missing_attr_eq_vs_ne() {
        // Attribute missing: Eq → false, Ne → true
        let state = State::new(vec![("x", Value::Integer(5))]);
        let domain_eq = Domain::Atom {
            attribute: "missing".to_string(),
            op: RelOp::Eq,
            value: Value::Integer(0),
        };
        let domain_ne = Domain::Atom {
            attribute: "missing".to_string(),
            op: RelOp::Ne,
            value: Value::Integer(0),
        };
        assert!(!domain_eq.contains(&state));
        assert!(domain_ne.contains(&state)); // attribute missing → not equal → Ne → true
    }

    #[test]
    fn test_parse_domain_list_invalid_item_in_list() {
        // parse_domain_list with invalid list item → error
        use std::collections::HashMap;
        let mut data = HashMap::new();
        data.insert(
            "domains".to_string(),
            Value::List(vec![Value::Integer(42)].into()), // Integer is not a valid Domain
        );
        let result = Domain::from_dict(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dict_atom_float_value() {
        // atom supports floating-point values
        use std::collections::HashMap;
        let data = HashMap::from([
            ("type".to_string(), Value::string("atom")),
            ("attribute".to_string(), Value::string("temp")),
            ("op".to_string(), Value::string("gt")),
            ("value".to_string(), Value::float(36.5)),
        ]);
        let result = Domain::from_dict(&data);
        assert!(result.is_ok());
        let state = State::new(vec![("temp", Value::float(37.0))]);
        assert!(result.unwrap().contains(&state));
    }

    #[test]
    fn test_or_nested_chain() {
        // Or chain nesting
        let state = State::new(vec![("status", Value::string("c"))]);
        let domain = Domain::Or(vec![
            Domain::Atom {
                attribute: "status".to_string(),
                op: RelOp::Eq,
                value: Value::string("a"),
            },
            Domain::Or(vec![
                Domain::Atom {
                    attribute: "status".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("b"),
                },
                Domain::Atom {
                    attribute: "status".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("c"),
                },
            ]),
        ]);
        assert!(domain.contains(&state));
    }

    #[test]
    fn test_and_with_or_nested() {
        // And containing nested Or
        let state = State::new(vec![
            ("x", Value::Integer(5)),
            ("status", Value::string("active")),
        ]);
        let domain = Domain::And(vec![
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Gt,
                value: Value::Integer(0),
            },
            Domain::Or(vec![
                Domain::Atom {
                    attribute: "status".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("active"),
                },
                Domain::Atom {
                    attribute: "status".to_string(),
                    op: RelOp::Eq,
                    value: Value::string("pending"),
                },
            ]),
        ]);
        assert!(domain.contains(&state));
    }

    // ========== FLOW-01 fix verification: Domain serialization roundtrip ==========

    #[test]
    fn test_domain_not_to_value_from_value_roundtrip() {
        // FLOW-01 fix verification: Domain::Not serialization roundtrip
        // Before fix: to_value() used "domain" field name, from_value() expected "inner"
        // After fix: unified to "inner", roundtrip preserves semantics

        // Test 1: Simple Not(Domain::Empty)
        let original = Domain::Not(Box::new(Domain::Empty));
        let serialized = original.to_value();
        let restored = Domain::from_value(&serialized).unwrap();

        // After roundtrip, semantics should be preserved
        let state = State::new(vec![("x", Value::Integer(5))]);
        assert_eq!(original.contains(&state), restored.contains(&state));

        // Test 2: Not(Atom) roundtrip
        let original = Domain::Not(Box::new(Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Eq,
            value: Value::Integer(5),
        }));
        let serialized = original.to_value();
        let restored = Domain::from_value(&serialized).unwrap();

        let state = State::new(vec![("x", Value::Integer(10))]);
        // x == 5 is false, Not should be true
        assert!(original.contains(&state));
        // Semantics preserved after roundtrip
        assert_eq!(original.contains(&state), restored.contains(&state));

        // Test 3: Nested Not(Not(...))
        let inner = Domain::Not(Box::new(Domain::Atom {
            attribute: "x".to_string(),
            op: RelOp::Gt,
            value: Value::Integer(0),
        }));
        let original = Domain::Not(Box::new(inner));
        let serialized = original.to_value();
        let restored = Domain::from_value(&serialized).unwrap();

        let state = State::new(vec![("x", Value::Integer(5))]);
        // x > 0 is true, Not → false, Not again → true
        assert!(original.contains(&state));
        assert_eq!(original.contains(&state), restored.contains(&state));
    }

    #[test]
    fn test_domain_all_types_to_value_from_value_roundtrip() {
        // Verify all Domain types roundtrip correctly
        let test_cases = vec![
            Domain::Empty,
            Domain::Universal,
            Domain::Atom {
                attribute: "x".to_string(),
                op: RelOp::Eq,
                value: Value::Integer(5),
            },
            Domain::And(vec![Domain::Empty, Domain::Universal]),
            Domain::Or(vec![Domain::Empty, Domain::Universal]),
            Domain::Not(Box::new(Domain::Empty)),
            Domain::Instruction("eval".to_string()),
        ];

        for original in test_cases {
            let serialized = original.to_value();
            let restored = Domain::from_value(&serialized).unwrap();

            // Verify the structure type after roundtrip
            match (&original, &restored) {
                (Domain::Empty, Domain::Empty) => {}
                (Domain::Universal, Domain::Universal) => {}
                (Domain::Atom { .. }, Domain::Atom { .. }) => {}
                (Domain::And(_), Domain::And(_)) => {}
                (Domain::Or(_), Domain::Or(_)) => {}
                (Domain::Not(_), Domain::Not(_)) => {}
                (Domain::Instruction(a), Domain::Instruction(b)) => {
                    assert_eq!(a, b);
                }
                _ => panic!("Type mismatch: {:?} vs {:?}", original, restored),
            }
        }
    }

    // ── Verify that `RelOp::apply` returns false (not panics) on Exists/NotExists ──
    // [P0 FIX] Previously these panicked; now they return false defensively
    // to preserve TCB liveness (ER-600: no runtime panics in production).

    #[test]
    fn test_relop_apply_exists_returns_false() {
        let result = RelOp::Exists.apply(&Value::Null, &Value::Null);
        assert!(
            !result,
            "Exists should return false when called directly (short-circuited in contains)"
        );
    }

    #[test]
    fn test_relop_apply_not_exists_returns_false() {
        let result = RelOp::NotExists.apply(&Value::Null, &Value::Null);
        assert!(
            !result,
            "NotExists should return false when called directly (short-circuited in contains)"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // P2: Property-based tests (proptest)
    // ═══════════════════════════════════════════════════════════════════════
    // Verify algebraic properties of Domain::contains that hold for arbitrary
    // states. These tests catch edge cases that example-based tests miss
    // (e.g., Not-Not elimination, And/Or identity, Universal/Empty reflexivity).

    use proptest::prelude::*;

    /// Generate an arbitrary State with string keys and primitive values.
    fn arb_state() -> BoxedStrategy<State> {
        prop::collection::hash_map(
            any::<String>(),
            prop_oneof![
                Just(Value::Null),
                any::<bool>().prop_map(Value::Bool),
                any::<i64>().prop_map(Value::Integer),
                any::<String>().prop_map(Value::string),
            ],
            0..5,
        )
        .prop_map(|m| {
            // State::new expects Vec<(&str, Value)>; use from_std_map instead
            State::from_std_map(m)
        })
        .boxed()
    }

    /// Simpler domain generator (for nesting inside Not, avoiding infinite recursion).
    fn arb_domain_simple() -> BoxedStrategy<Domain> {
        prop_oneof![
            Just(Domain::Universal),
            Just(Domain::Empty),
            any::<String>().prop_map(|attr| Domain::Atom {
                attribute: attr,
                op: RelOp::Exists,
                value: Value::Null,
            }),
        ]
        .boxed()
    }

    #[test]
    fn prop_domain_universal_always_true() {
        proptest!(|(s in arb_state())| {
            prop_assert!(Domain::Universal.contains(&s));
        });
    }

    #[test]
    fn prop_domain_empty_always_false() {
        proptest!(|(s in arb_state())| {
            prop_assert!(!Domain::Empty.contains(&s));
        });
    }

    #[test]
    fn prop_domain_double_negation_elimination() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let not_not = Domain::Not(Box::new(Domain::Not(Box::new(d.clone()))));
            let expected = d.contains(&s);
            let actual = not_not.contains(&s);
            prop_assert_eq!(actual, expected);
        });
    }

    #[test]
    fn prop_domain_and_single_element_identity() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let and_single = Domain::And(vec![d.clone()]);
            let expected = d.contains(&s);
            let actual = and_single.contains(&s);
            prop_assert_eq!(actual, expected);
        });
    }

    #[test]
    fn prop_domain_or_single_element_identity() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let or_single = Domain::Or(vec![d.clone()]);
            let expected = d.contains(&s);
            let actual = or_single.contains(&s);
            prop_assert_eq!(actual, expected);
        });
    }

    #[test]
    fn prop_domain_and_with_empty_is_false() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let and_with_empty = Domain::And(vec![Domain::Empty, d]);
            prop_assert!(!and_with_empty.contains(&s));
        });
    }

    #[test]
    fn prop_domain_or_with_universal_is_true() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let or_with_universal = Domain::Or(vec![Domain::Universal, d]);
            prop_assert!(or_with_universal.contains(&s));
        });
    }

    #[test]
    fn prop_domain_and_with_universal_preserves() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let and_with_universal = Domain::And(vec![Domain::Universal, d.clone()]);
            let expected = d.contains(&s);
            let actual = and_with_universal.contains(&s);
            prop_assert_eq!(actual, expected);
        });
    }

    #[test]
    fn prop_domain_or_with_empty_preserves() {
        proptest!(|(d in arb_domain_simple(), s in arb_state())| {
            let or_with_empty = Domain::Or(vec![Domain::Empty, d.clone()]);
            let expected = d.contains(&s);
            let actual = or_with_empty.contains(&s);
            prop_assert_eq!(actual, expected);
        });
    }

    #[test]
    fn prop_domain_not_universal_equals_empty() {
        proptest!(|(s in arb_state())| {
            let not_universal = Domain::Not(Box::new(Domain::Universal));
            let actual = not_universal.contains(&s);
            let empty_result = Domain::Empty.contains(&s);
            prop_assert_eq!(actual, empty_result);
            prop_assert!(!actual);
        });
    }

    #[test]
    fn prop_domain_not_empty_equals_universal() {
        proptest!(|(s in arb_state())| {
            let not_empty = Domain::Not(Box::new(Domain::Empty));
            let actual = not_empty.contains(&s);
            let universal_result = Domain::Universal.contains(&s);
            prop_assert_eq!(actual, universal_result);
            prop_assert!(actual);
        });
    }

    #[test]
    fn prop_domain_de_morgan_and_to_or() {
        proptest!(|(d1 in arb_domain_simple(), d2 in arb_domain_simple(), s in arb_state())| {
            let lhs = Domain::Not(Box::new(Domain::And(vec![d1.clone(), d2.clone()])));
            let rhs = Domain::Or(vec![
                Domain::Not(Box::new(d1.clone())),
                Domain::Not(Box::new(d2.clone())),
            ]);
            prop_assert_eq!(lhs.contains(&s), rhs.contains(&s));
        });
    }

    #[test]
    fn prop_domain_de_morgan_or_to_and() {
        proptest!(|(d1 in arb_domain_simple(), d2 in arb_domain_simple(), s in arb_state())| {
            let lhs = Domain::Not(Box::new(Domain::Or(vec![d1.clone(), d2.clone()])));
            let rhs = Domain::And(vec![
                Domain::Not(Box::new(d1.clone())),
                Domain::Not(Box::new(d2.clone())),
            ]);
            prop_assert_eq!(lhs.contains(&s), rhs.contains(&s));
        });
    }
}
