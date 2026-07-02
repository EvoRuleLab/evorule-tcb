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

//! Rule structures — Core concepts of `EvoRule`.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Core Concepts
//!
//! - `Rule`: Domain + Transform. When the state satisfies the Domain condition,
//!   the Transform is executed.
//! - `GenericInstruction`: The basic instruction type used by the execution engine.
//! - `RuleCollection`: Ordered container for rules (sorted by priority).
//!
//! All structures are **L1 deterministic** — pure data transformations,
//! no randomness, no wall-clock time, no side effects.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `Rule` data model | ✅ L1 deterministic | Pure data |
//! | `GenericInstruction` | ✅ L1 deterministic | Pure data |
//! | `RuleCollection` sorting | ✅ L1 deterministic | Stable priority + order sort |
//! | `Rule::from_dict` parsing | ✅ L1 deterministic | Defensive parsing, no panic |
//!
//! # Cross-Language Note (L4)
//!
//! These data structures are designed to be serialized to/from JSON.
//! The JSON schema is documented in the specification (`docs/spec/`).

use crate::domain::Domain;
use crate::error::{missing_param, EvoRuleError};
use crate::value::Value;
use std::collections::HashMap;

/// Maximum rule priority.
pub const MAX_PRIORITY: i64 = 10000;

/// Default priority.
pub const DEFAULT_PRIORITY: i64 = 0;

// ══════════════════════════════════════════════
// GenericInstruction
// ══════════════════════════════════════════════

/// Generic instruction — the basic instruction structure for the execution engine.
///
/// There is no need to create a Rule before execution. `GenericInstruction` can be
/// used directly to create a temporary instruction and execute it via the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericInstruction {
    /// Instruction type (e.g., "`set_context`", "`advance_instruction`", etc.).
    pub instruction_type: String,
    /// Instruction parameters.
    pub params: HashMap<String, Value>,
    /// Metadata (optional).
    pub metadata: Value,
}

impl GenericInstruction {
    /// Create an instruction.
    pub fn new(instruction_type: impl Into<String>, params: HashMap<String, Value>) -> Self {
        Self {
            instruction_type: instruction_type.into(),
            params,
            metadata: Value::empty_object(),
        }
    }

    /// Create an instruction with only a type (no parameters).
    pub fn simple(instruction_type: impl Into<String>) -> Self {
        Self::new(instruction_type, HashMap::new())
    }

    /// Create an instruction from a Value.
    pub fn from_value(val: &Value) -> Result<Self, EvoRuleError> {
        let instr_type = val
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| missing_param("GenericInstruction::from_value", "type"))?
            .to_string();

        let params = match val.get("params") {
            Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        };

        let metadata = val
            .get("metadata")
            .cloned()
            .unwrap_or(Value::empty_object());

        Ok(Self {
            instruction_type: instr_type,
            params,
            metadata,
        })
    }

    /// Convert to a Value.
    pub fn to_value(&self) -> Value {
        let mut map = im::HashMap::new();
        map.insert("type".to_string(), Value::string(&self.instruction_type));

        if !self.params.is_empty() {
            map.insert(
                "params".to_string(),
                Value::Object(im::HashMap::from_iter(self.params.clone())),
            );
        }

        if !self.metadata.is_null() {
            map.insert("metadata".to_string(), self.metadata.clone());
        }

        Value::Object(map)
    }
}

// ══════════════════════════════════════════════
// Rule
// ══════════════════════════════════════════════

/// Rule — Domain + Transform.
///
/// When the state satisfies the domain condition, the transform is executed.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Rule name.
    pub name: String,
    /// Rule ID (unique identifier).
    pub rule_id: String,
    /// Rule condition domain.
    pub domain: Domain,
    /// Rule transform (can be a Value or a sequence of instructions).
    pub transform: Value,
    /// Priority (0-10000), higher priority rules execute first.
    pub priority: i64,
    /// Order number (for stable sorting).
    pub order: i64,
    /// Metadata.
    pub metadata: Value,
    /// Whether this is a constitutional rule (non-deletable).
    pub constitutional: bool,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Whether this is a meta-rule.
    pub is_meta: bool,
}

impl Rule {
    /// Create a rule.
    pub fn new(
        rule_id: impl Into<String>,
        name: impl Into<String>,
        domain: Domain,
        transform: Value,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            name: name.into(),
            domain,
            transform,
            priority: DEFAULT_PRIORITY,
            order: 0,
            metadata: Value::empty_object(),
            constitutional: false,
            enabled: true,
            is_meta: false,
        }
    }

    /// Create a Rule from a dictionary.
    pub fn from_dict(data: &HashMap<String, Value>) -> Result<Self, EvoRuleError> {
        let rule_id = data
            .get("rule_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| missing_param("Rule::from_dict", "rule_id"))?
            .to_string();

        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&rule_id)
            .to_string();
        let domain = data
            .get("domain")
            .map(Domain::from_value)
            .transpose()?
            .unwrap_or(Domain::Universal);
        let transform = data
            .get("transform")
            .cloned()
            .ok_or_else(|| missing_param("Rule::from_dict", "transform"))?;

        let priority_raw = data
            .get("priority")
            .and_then(super::value::Value::as_integer)
            .unwrap_or(0);
        let priority = if (0..=MAX_PRIORITY).contains(&priority_raw) {
            priority_raw
        } else {
            DEFAULT_PRIORITY
        };

        let order = data
            .get("order")
            .and_then(super::value::Value::as_integer)
            .unwrap_or(0);
        let metadata = data
            .get("metadata")
            .cloned()
            .unwrap_or(Value::empty_object());
        let constitutional = data
            .get("constitutional")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(false);
        let enabled = data
            .get("enabled")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(true);
        // Constitution rules are always meta-rules. This ensures constitutional
        // rules retain their meta status even if the JSON does not explicitly set it.
        let is_meta = data
            .get("is_meta")
            .and_then(super::value::Value::as_bool)
            .unwrap_or(false)
            || rule_id.starts_with("constitution.");

        Ok(Self {
            name,
            rule_id,
            domain,
            transform,
            priority,
            order,
            metadata,
            constitutional,
            enabled,
            is_meta,
        })
    }

    /// Convert to a dictionary.
    pub fn to_dict(&self) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        map.insert("rule_id".to_string(), Value::string(&self.rule_id));
        map.insert("name".to_string(), Value::string(&self.name));

        let domain_obj = self.domain.to_dict();
        map.insert(
            "domain".to_string(),
            Value::Object(im::HashMap::from(domain_obj)),
        );

        map.insert("transform".to_string(), self.transform.clone());
        map.insert("priority".to_string(), Value::Integer(self.priority));
        map.insert("order".to_string(), Value::Integer(self.order));

        if !self.metadata.is_null() {
            map.insert("metadata".to_string(), self.metadata.clone());
        }

        if self.constitutional {
            map.insert("constitutional".to_string(), Value::Bool(true));
        }
        if !self.enabled {
            map.insert("enabled".to_string(), Value::Bool(false));
        }
        if self.is_meta {
            map.insert("is_meta".to_string(), Value::Bool(true));
        }

        map
    }

    /// Determine whether the rule applies to the given state.
    pub fn applies_to(&self, state: &crate::state::State) -> bool {
        self.domain.contains(state)
    }

    /// Get the rule's effective weight (priority + order combination).
    pub const fn weight(&self) -> i64 {
        self.priority * 10000 + self.order
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rule[{}] {} -> transform", self.rule_id, self.domain)
    }
}

// ══════════════════════════════════════════════
// RuleCollection
// ══════════════════════════════════════════════

/// Rule collection — an ordered container for rules.
///
/// Rules are ordered by priority descending (higher priority first).
#[derive(Debug, Clone)]
pub struct RuleCollection {
    rules: Vec<Rule>,
}

impl RuleCollection {
    /// Create an empty collection.
    pub const fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create a collection from a Vec (automatically sorted).
    pub fn from_vec(mut rules: Vec<Rule>) -> Self {
        rules.sort_by_key(|b| std::cmp::Reverse(b.weight()));
        Self { rules }
    }

    /// Add a rule (inserted in priority-sorted order).
    pub fn add(&mut self, rule: Rule) {
        let weight = rule.weight();
        let pos = self
            .rules
            .iter()
            .position(|r| r.weight() < weight)
            .unwrap_or(self.rules.len());
        self.rules.insert(pos, rule);
    }

    /// Get all rules.
    pub fn all(&self) -> &[Rule] {
        &self.rules
    }

    /// Get all rules that match the state.
    pub fn applicable(&self, state: &crate::state::State) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.applies_to(state)).collect()
    }

    /// Get the number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Get a rule by its `rule_id`.
    pub fn get(&self, rule_id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.rule_id == rule_id)
    }

    /// Get a mutable reference to a rule by its `rule_id`.
    ///
    /// # Note
    ///
    /// If you modify the `priority` or `order` field via this mutable reference,
    /// the collection will become unsorted. After modifications, call `sort()`
    /// to restore the correct order.
    ///
    /// For safe updates, prefer `remove()` + `add()`.
    pub fn get_mut(&mut self, rule_id: &str) -> Option<&mut Rule> {
        self.rules.iter_mut().find(|r| r.rule_id == rule_id)
    }

    /// Re-sort the collection by priority (descending) and order.
    ///
    /// This is useful after modifying `priority` or `order` via `get_mut()`.
    pub fn sort(&mut self) {
        self.rules.sort_by_key(|r| std::cmp::Reverse(r.weight()));
    }

    /// Remove a rule by its `rule_id`.
    pub fn remove(&mut self, rule_id: &str) -> Option<Rule> {
        let pos = self.rules.iter().position(|r| r.rule_id == rule_id);
        pos.map(|p| self.rules.remove(p))
    }

    /// Clear all rules.
    pub fn clear(&mut self) {
        self.rules.clear();
    }

    /// Convert to a Value (list of rules).
    pub fn to_value(&self) -> Value {
        let items: Vec<Value> = self
            .rules
            .iter()
            .map(|r| {
                let dict = r.to_dict();
                Value::Object(im::HashMap::from(dict))
            })
            .collect();
        Value::list(items)
    }

    /// Iterator over rules.
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }
}

impl Default for RuleCollection {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_creation() {
        let rule = Rule::new(
            "test_rule",
            "Test Rule",
            Domain::Universal,
            Value::string("noop"),
        );
        assert_eq!(rule.rule_id, "test_rule");
        assert!(rule.enabled);
        assert!(!rule.constitutional);
        assert_eq!(rule.priority, 0);
    }

    #[test]
    fn test_priority_validation_valid() {
        let mut data = HashMap::new();
        data.insert("rule_id".to_string(), Value::string("test"));
        data.insert("name".to_string(), Value::string("test"));
        data.insert(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("universal"),
            )])),
        );
        data.insert("transform".to_string(), Value::string("noop"));
        data.insert("priority".to_string(), Value::Integer(500));
        let rule = Rule::from_dict(&data).unwrap();
        assert_eq!(rule.priority, 500);
    }

    #[test]
    fn test_priority_clamped() {
        let mut data = HashMap::new();
        data.insert("rule_id".to_string(), Value::string("test"));
        data.insert("name".to_string(), Value::string("test"));
        data.insert(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("universal"),
            )])),
        );
        data.insert("transform".to_string(), Value::string("noop"));
        data.insert("priority".to_string(), Value::Integer(99999));
        let rule = Rule::from_dict(&data).unwrap();
        assert_eq!(rule.priority, 0); // Out of range → default
    }

    #[test]
    fn test_rule_collection_sorted() {
        let mut col = RuleCollection::new();
        let mk_rule = |id: &str, prio: i64| -> Rule {
            let mut r = Rule::new(id, id, Domain::Universal, Value::string("noop"));
            r.priority = prio;
            r
        };
        col.add(mk_rule("low", 10));
        col.add(mk_rule("high", 100));
        col.add(mk_rule("mid", 50));

        let all = col.all();
        assert_eq!(all[0].rule_id, "high"); // Highest priority first
        assert_eq!(all[1].rule_id, "mid");
        assert_eq!(all[2].rule_id, "low");
    }

    #[test]
    fn test_generic_instruction_roundtrip() {
        let mut params = HashMap::new();
        params.insert("attr".to_string(), Value::string("x"));
        params.insert("delta".to_string(), Value::Integer(5));
        let instr = GenericInstruction::new("increment", params);
        let val = instr.to_value();
        let restored = GenericInstruction::from_value(&val).unwrap();
        assert_eq!(instr.instruction_type, restored.instruction_type);
        assert_eq!(instr.params.len(), restored.params.len());
    }

    #[test]
    fn test_rule_applies_to() {
        let rule = Rule::new(
            "test",
            "test",
            Domain::Atom {
                attribute: "x".to_string(),
                op: crate::domain::RelOp::Eq,
                value: Value::Integer(42),
            },
            Value::string("noop"),
        );
        let state = crate::state::State::new(vec![("x", Value::Integer(42))]);
        assert!(rule.applies_to(&state));

        let state2 = crate::state::State::new(vec![("x", Value::Integer(0))]);
        assert!(!rule.applies_to(&state2));
    }

    /// Test: `get_mut` does NOT auto-sort (documented behavior).
    /// After modifying priority via get_mut, call sort() to restore order.
    #[test]
    fn test_rule_collection_get_mut_sort_required() {
        let mut col = RuleCollection::new();
        let mk_rule = |id: &str, prio: i64| -> Rule {
            let mut r = Rule::new(id, id, Domain::Universal, Value::string("noop"));
            r.priority = prio;
            r
        };
        col.add(mk_rule("low", 10));
        col.add(mk_rule("high", 100));

        // Before modification: sorted (high first)
        assert_eq!(col.all()[0].rule_id, "high");

        // Modify priority via get_mut
        if let Some(r) = col.get_mut("high") {
            r.priority = 5; // Now high becomes lower than low
        }

        // Collection is now unsorted (high still first but has lower priority)
        assert_eq!(col.all()[0].rule_id, "high");

        // Call sort to restore order
        col.sort();
        assert_eq!(col.all()[0].rule_id, "low"); // low has higher priority now
    }

    /// Test: `from_vec` correctly sorts rules on creation.
    #[test]
    fn test_rule_collection_from_vec_sorts() {
        let mk_rule = |id: &str, prio: i64| -> Rule {
            let mut r = Rule::new(id, id, Domain::Universal, Value::string("noop"));
            r.priority = prio;
            r
        };
        let rules = vec![mk_rule("low", 10), mk_rule("high", 100)];
        let col = RuleCollection::from_vec(rules);
        assert_eq!(col.all()[0].rule_id, "high");
        assert_eq!(col.all()[1].rule_id, "low");
    }

    /// Test: Constitution rules are automatically meta-rules.
    #[test]
    fn test_constitution_rule_is_meta() {
        let mut data = HashMap::new();
        data.insert("rule_id".to_string(), Value::string("constitution.one"));
        data.insert("name".to_string(), Value::string("Test Constitution"));
        data.insert(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("universal"),
            )])),
        );
        data.insert("transform".to_string(), Value::string("noop"));
        let rule = Rule::from_dict(&data).unwrap();
        assert!(rule.is_meta, "Constitution rule should be meta-rule");
    }
}
