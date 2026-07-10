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

//! Rule primitives — Rule application, observation, and filtering.
//!
//! # Core Functions
//!
//! - `apply_rule`: Apply a rule by extracting its `transform` and executing it.
//! - `observe_rules`: Observe the current rule set (`__universe_rules__`).
//! - `filter_rules`: Filter rules by target attributes (structured matching).
//! - `inject_rule`: Inject/replace/remove rules in the rule set.
//!
//! # Design Principles
//!
//! ## `result_attr` Default Naming Convention
//!
//! The `result_attr` parameter on each primitive specifies the key name under which
//! the result is stored in the state. Defaults follow this convention:
//! - **User-layer common names** (e.g., `observe_rules` uses `rules_list`,
//!   `filter_rules` uses `filtered_rules`) — directly user-facing, with business-semantic names
//! - **Infrastructure-layer system names** (e.g., `inject_rule` uses `__inject_result__`)
//!   — prefixed with `__` to indicate system use, not exposed to end users
//! - **`apply_rule`** has no default `result_attr` — it modifies state directly,
//!   producing no independent result field
//!
//! This design ensures:
//! 1. User-layer operation outputs use human-readable names
//! 2. System-layer operation outputs use special prefixes to avoid conflicts with user data
//! 3. Debugging is easier by quickly distinguishing output sources by prefix
//!
//! ## Filter Rules: Structured Matching
//!
//! `filter_rules` uses structured matching instead of brute-force string matching.
//! It recursively traverses the rule's `domain` structure to check if any `atom`
//! references the target attributes. This avoids accidental substring matches
//! within values.
//!
//! # Determinism Guarantee
//!
//! All rule primitives are **L1 deterministic**:
//! - Same input state + same instruction → same output state.
//! - No randomness, wall-clock time, or side effects.
//! - Rule filtering is a pure algorithm (structured matching).
//! - Rule injection is deterministic (list filtering + replacement).
//! - `result_attr` naming is deterministic (constant defaults or user-provided).
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `apply_rule` transform execution | ✅ L1 deterministic | Delegates to registry |
//! | `observe_rules` read | ✅ L1 deterministic | Pure read |
//! | `filter_rules` structured matching | ✅ L1 deterministic | Recursive domain traversal |
//! | `inject_rule` list operations | ✅ L1 deterministic | Filter + insert/replace |
//! | `domain_references_attrs` | ✅ L1 deterministic | Pure recursion |
//! | `universal` domain handling | ✅ L1 deterministic | Always returns `true` |
//!
//! # Cross-Language Note (L4)
//!
//! These primitives are Rust-only constructs; there is no cross-language equivalent.
//! However, the rule data format (JSON) is language-agnostic and can be inspected
//! by other languages.

#![allow(clippy::items_after_test_module)] // test mod in middle (section 51 work, high risk to move)
use crate::control::dispatch::resolve_path;
use crate::error::{missing_param, rule_not_found, EvoRuleError};
use crate::exec_ctl_ctx::ExecCtlCtx;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::{ImMapExt, Value};

/// Register rule primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("apply_rule", exec_apply_rule);
    reg.register("observe_rules", exec_observe_rules);
    reg.register("filter_rules", exec_filter_rules);
    reg.register("inject_rule", exec_inject_rule);
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_reg() -> InstructionRegistry {
        InstructionRegistry::new().with_default_context_ops()
    }

    // ─── domain_references_attrs ───────────────────────────────────────

    #[test]
    fn test_domain_references_attrs_atom_matches() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("atom")),
                ("attribute".to_string(), Value::string("speed")),
                ("op".to_string(), Value::string("gt")),
                ("value".to_string(), Value::Integer(0)),
            ])),
        )]));
        let goal = vec!["speed".to_string(), "distance".to_string()];
        assert!(domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_atom_no_match() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("atom")),
                ("attribute".to_string(), Value::string("temperature")),
                ("op".to_string(), Value::string("gt")),
                ("value".to_string(), Value::Integer(0)),
            ])),
        )]));
        let goal = vec!["speed".to_string()];
        assert!(!domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_and_matches_one_child() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("and")),
                (
                    "domains".to_string(),
                    Value::list(vec![
                        Value::from(im::HashMap::from(vec![
                            ("type".to_string(), Value::string("atom")),
                            ("attribute".to_string(), Value::string("x")),
                        ])),
                        Value::from(im::HashMap::from(vec![
                            ("type".to_string(), Value::string("atom")),
                            ("attribute".to_string(), Value::string("y")),
                        ])),
                    ]),
                ),
            ])),
        )]));
        let goal = vec!["y".to_string()];
        assert!(domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_or_matches_one_child() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("or")),
                (
                    "domains".to_string(),
                    Value::list(vec![
                        Value::from(im::HashMap::from(vec![
                            ("type".to_string(), Value::string("atom")),
                            ("attribute".to_string(), Value::string("a")),
                        ])),
                        Value::from(im::HashMap::from(vec![
                            ("type".to_string(), Value::string("atom")),
                            ("attribute".to_string(), Value::string("b")),
                        ])),
                    ]),
                ),
            ])),
        )]));
        let goal = vec!["b".to_string()];
        assert!(domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_not_recursive() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("not")),
                (
                    "inner".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("type".to_string(), Value::string("atom")),
                        ("attribute".to_string(), Value::string("hidden")),
                    ])),
                ),
            ])),
        )]));
        let goal = vec!["hidden".to_string()];
        assert!(domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_universal_always_matches() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("universal"),
            )])),
        )]));
        let goal = vec!["anything".to_string()];
        assert!(domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_empty_never_matches() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("empty"),
            )])),
        )]));
        let goal = vec!["anything".to_string()];
        assert!(!domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_missing_domain() {
        let rule = Value::from(im::HashMap::from(vec![(
            "other".to_string(),
            Value::string("value"),
        )]));
        let goal = vec!["anything".to_string()];
        assert!(!domain_references_attrs(&rule, &goal));
    }

    #[test]
    fn test_domain_references_attrs_instruction_domain_never_matches() {
        let rule = Value::from(im::HashMap::from(vec![(
            "domain".to_string(),
            Value::from(im::HashMap::from(vec![(
                "type".to_string(),
                Value::string("instruction"),
            )])),
        )]));
        let goal = vec!["anything".to_string()];
        assert!(!domain_references_attrs(&rule, &goal));
    }

    // ─── exec_apply_rule ────────────────────────────────────────────────

    #[test]
    fn test_apply_rule_inline_transform() {
        // Inline rule object (with transform) → executes the transform
        // Uses state_set instruction (provided by make_test_registry)
        let state = State::empty().set("x", Value::Integer(5));
        let rule_obj = Value::from(im::HashMap::from(vec![(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("state_set")),
                (
                    "params".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("attr".to_string(), Value::string("x")),
                        ("value".to_string(), Value::Integer(42)),
                    ])),
                ),
            ])),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("rule".to_string(), rule_obj);
            p
        };
        let instr = GenericInstruction::new("apply_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_apply_rule(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(result.get("x"), Some(&Value::Integer(42)));
    }

    #[test]
    fn test_apply_rule_result_attr_stores_filtered_result() {
        // result_attr specified → stores the filtered result in that attribute
        let state = State::empty().set("x", Value::Integer(5));
        let rule_obj = Value::from(im::HashMap::from(vec![(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("state_set")),
                (
                    "params".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("attr".to_string(), Value::string("x")),
                        ("value".to_string(), Value::Integer(99)),
                    ])),
                ),
            ])),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("rule".to_string(), rule_obj);
            p.insert("result_attr".to_string(), Value::string("snapshot"));
            p
        };
        let instr = GenericInstruction::new("apply_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_apply_rule(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        // x was modified by the transform
        assert_eq!(result.get("x"), Some(&Value::Integer(99)));
        // snapshot stores the filtered complete state (excluding system fields), containing x=99
        let snap = result.get("snapshot").unwrap();
        assert_eq!(snap.get("x"), Some(&Value::Integer(99)));
    }

    #[test]
    fn test_apply_rule_no_rule_returns_error() {
        let state = State::empty().set("x", Value::Integer(5));
        let params = HashMap::new();
        let instr = GenericInstruction::new("apply_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_apply_rule(&make_reg(), &state, &instr, &mut ctx);
        assert!(result.is_err(), "missing rule should return error");
    }

    #[test]
    fn test_apply_rule_rule_without_transform_returns_error() {
        let state = State::empty().set("x", Value::Integer(5));
        let rule_obj = Value::from(im::HashMap::from(vec![(
            "rule_id".to_string(),
            Value::string("some_rule"),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("rule".to_string(), rule_obj);
            p
        };
        let instr = GenericInstruction::new("apply_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_apply_rule(&make_reg(), &state, &instr, &mut ctx);
        assert!(
            result.is_err(),
            "rule without transform should return error"
        );
    }

    #[test]
    fn test_apply_rule_systems_fields_excluded_from_result() {
        // System fields (__-prefixed) are not leaked into result_attr
        let state = State::empty().set("x", Value::Integer(1)).set(
            "__exec__",
            Value::from(im::HashMap::from(vec![(
                "audit_on".to_string(),
                Value::Bool(true),
            )])),
        );
        let rule_obj = Value::from(im::HashMap::from(vec![(
            "transform".to_string(),
            Value::from(im::HashMap::from(vec![
                ("type".to_string(), Value::string("state_set")),
                (
                    "params".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("attr".to_string(), Value::string("x")),
                        ("value".to_string(), Value::Integer(2)),
                    ])),
                ),
            ])),
        )]));
        let params = {
            let mut p = HashMap::new();
            p.insert("rule".to_string(), rule_obj);
            p.insert("result_attr".to_string(), Value::string("out"));
            p
        };
        let instr = GenericInstruction::new("apply_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_apply_rule(
            &crate::primitive::make_test_registry(),
            &state,
            &instr,
            &mut ctx,
        )
        .unwrap();
        let out = result.get("out").unwrap();
        // out should not contain __exec__
        if let Some(obj) = out.as_object() {
            assert!(!obj.contains_key("__exec__"));
        }
    }

    // ─── exec_observe_rules ────────────────────────────────────────────

    #[test]
    fn test_observe_rules_reads_universe_rules() {
        // Reads __universe_rules__ and stores it in result_attr
        let rules = Value::list(vec![
            Value::from(im::HashMap::from(vec![(
                "rule_id".to_string(),
                Value::string("r1"),
            )])),
            Value::from(im::HashMap::from(vec![(
                "rule_id".to_string(),
                Value::string("r2"),
            )])),
        ]);
        let state = State::empty().set("__universe_rules__", rules);
        let params = HashMap::new();
        let instr = GenericInstruction::new("observe_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_observe_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let observed = result.get("rules_list").unwrap();
        assert!(observed.as_list().is_some());
        assert_eq!(observed.as_list().unwrap().len(), 2);
    }

    #[test]
    fn test_observe_rules_custom_result_attr() {
        let state = State::empty().set("__universe_rules__", Value::empty_list());
        let params = {
            let mut p = HashMap::new();
            p.insert("result_attr".to_string(), Value::string("my_rules"));
            p
        };
        let instr = GenericInstruction::new("observe_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_observe_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        assert!(result.get("my_rules").is_some());
    }

    #[test]
    fn test_observe_rules_missing_universe_rules_returns_empty() {
        let state = State::empty();
        let params = HashMap::new();
        let instr = GenericInstruction::new("observe_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_observe_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let observed = result.get("rules_list").unwrap();
        assert!(observed.as_list().is_some());
        assert!(observed.as_list().unwrap().is_empty());
    }

    // ─── exec_filter_rules ──────────────────────────────────────────────

    fn make_rules_list() -> Value {
        Value::list(vec![
            Value::from(im::HashMap::from(vec![
                ("rule_id".to_string(), Value::string("rule_speed")),
                (
                    "domain".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("type".to_string(), Value::string("atom")),
                        ("attribute".to_string(), Value::string("speed")),
                        ("op".to_string(), Value::string("gt")),
                        ("value".to_string(), Value::Integer(0)),
                    ])),
                ),
            ])),
            Value::from(im::HashMap::from(vec![
                ("rule_id".to_string(), Value::string("rule_dist")),
                (
                    "domain".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("type".to_string(), Value::string("atom")),
                        ("attribute".to_string(), Value::string("distance")),
                        ("op".to_string(), Value::string("gt")),
                        ("value".to_string(), Value::Integer(0)),
                    ])),
                ),
            ])),
            Value::from(im::HashMap::from(vec![
                ("rule_id".to_string(), Value::string("rule_time")),
                (
                    "domain".to_string(),
                    Value::from(im::HashMap::from(vec![
                        ("type".to_string(), Value::string("atom")),
                        ("attribute".to_string(), Value::string("time")),
                        ("op".to_string(), Value::string("gt")),
                        ("value".to_string(), Value::Integer(0)),
                    ])),
                ),
            ])),
        ])
    }

    #[test]
    fn test_filter_rules_by_goal_attrs() {
        // Filter by goal_attrs → only rules referencing target attributes are kept
        let state = State::empty().set("my_rules", make_rules_list());
        let params = {
            let mut p = HashMap::new();
            p.insert("rules_ref".to_string(), Value::string("my_rules"));
            p.insert(
                "goal_attrs".to_string(),
                Value::list(vec![Value::string("speed")]),
            );
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let filtered = result.get("filtered_rules").unwrap();
        let list = filtered.as_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get("rule_id").unwrap().as_str(), Some("rule_speed"));
    }

    #[test]
    fn test_filter_rules_empty_goal_attrs_returns_all() {
        // goal_attrs empty → returns all rules
        let state = State::empty().set("my_rules", make_rules_list());
        let params = {
            let mut p = HashMap::new();
            p.insert("rules_ref".to_string(), Value::string("my_rules"));
            p.insert("goal_attrs".to_string(), Value::empty_list());
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let filtered = result.get("filtered_rules").unwrap();
        assert_eq!(filtered.as_list().unwrap().len(), 3);
    }

    #[test]
    fn test_filter_rules_missing_rules_ref_uses_default() {
        // No rules_ref provided → defaults to __universe_rules__
        let state = State::empty().set("__universe_rules__", make_rules_list());
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "goal_attrs".to_string(),
                Value::list(vec![Value::string("time")]),
            );
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let filtered = result.get("filtered_rules").unwrap();
        assert_eq!(filtered.as_list().unwrap().len(), 1);
        assert_eq!(
            filtered.as_list().unwrap()[0]
                .get("rule_id")
                .unwrap()
                .as_str(),
            Some("rule_time")
        );
    }

    #[test]
    fn test_filter_rules_custom_result_attr() {
        let state = State::empty().set("my_rules", make_rules_list());
        let params = {
            let mut p = HashMap::new();
            p.insert("rules_ref".to_string(), Value::string("my_rules"));
            p.insert(
                "goal_attrs".to_string(),
                Value::list(vec![Value::string("distance")]),
            );
            p.insert("result_attr".to_string(), Value::string("my_filtered"));
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        assert!(result.get("my_filtered").is_some());
    }

    #[test]
    fn test_filter_rules_non_list_rules_returns_empty() {
        // rules_ref points to a non-list value → returns empty list
        let state = State::empty().set("bad_ref", Value::Integer(42));
        let params = {
            let mut p = HashMap::new();
            p.insert("rules_ref".to_string(), Value::string("bad_ref"));
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let filtered = result.get("filtered_rules").unwrap();
        assert!(filtered.as_list().unwrap().is_empty());
    }

    #[test]
    fn test_filter_rules_multiple_goal_attrs_match_any() {
        // Multiple goal_attrs → matches if any are referenced
        let state = State::empty().set("my_rules", make_rules_list());
        let params = {
            let mut p = HashMap::new();
            p.insert("rules_ref".to_string(), Value::string("my_rules"));
            p.insert(
                "goal_attrs".to_string(),
                Value::list(vec![Value::string("distance"), Value::string("time")]),
            );
            p
        };
        let instr = GenericInstruction::new("filter_rules", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_filter_rules(&make_reg(), &state, &instr, &mut ctx).unwrap();
        let filtered = result.get("filtered_rules").unwrap();
        assert_eq!(filtered.as_list().unwrap().len(), 2);
    }

    // ══════════════════════════════════════════════
    // inject_rule additional tests
    // ══════════════════════════════════════════════

    /// Test inject_rule: add a new rule to an empty list
    #[test]
    fn test_inject_rule_add_new_rule() {
        let reg = make_reg();
        let state = State::empty();

        let new_rule = Value::from(im::hashmap! {
            "rule_id".to_string() => Value::string("rule1"),
            "priority".to_string() => Value::Integer(1),
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("add_rule".to_string(), new_rule);
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let rules = result.get("__universe_rules__").unwrap();
        let rules_list = rules.as_list().unwrap();
        assert_eq!(rules_list.len(), 1);
        assert_eq!(rules_list[0].get("rule_id"), Some(&Value::string("rule1")));
    }

    /// Test inject_rule: result_info is correct
    #[test]
    fn test_inject_rule_result_info() {
        let reg = make_reg();
        let state = State::empty();

        let new_rule = Value::from(im::hashmap! {
            "rule_id".to_string() => Value::string("rule1"),
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("add_rule".to_string(), new_rule);
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let info = result.get("__inject_result__").unwrap();

        assert_eq!(info.get("added_count"), Some(&Value::Integer(1)));
        assert_eq!(info.get("removed_count"), Some(&Value::Integer(0)));
        assert_eq!(info.get("replaced_count"), Some(&Value::Integer(0)));
        assert_eq!(info.get("total_rules"), Some(&Value::Integer(1)));
    }

    /// Test inject_rule: replace an existing rule
    #[test]
    fn test_inject_rule_replace_existing() {
        let reg = make_reg();
        let existing_rules = Value::list(vec![Value::from(im::hashmap! {
            "rule_id".to_string() => Value::string("rule1"),
            "priority".to_string() => Value::Integer(1),
        })]);
        let state = State::empty().set("__universe_rules__", existing_rules);

        let new_rule = Value::from(im::hashmap! {
            "rule_id".to_string() => Value::string("rule1"),
            "priority".to_string() => Value::Integer(100), // different priority
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("add_rule".to_string(), new_rule);
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let rules = result.get("__universe_rules__").unwrap();
        let rules_list = rules.as_list().unwrap();
        assert_eq!(rules_list.len(), 1); // count unchanged
        assert_eq!(rules_list[0].get("priority"), Some(&Value::Integer(100))); // priority replaced
    }

    /// Test inject_rule: remove a specific rule
    #[test]
    fn test_inject_rule_remove_rule() {
        let reg = make_reg();
        let existing_rules = Value::list(vec![
            Value::from(im::hashmap! {
                "rule_id".to_string() => Value::string("rule1"),
            }),
            Value::from(im::hashmap! {
                "rule_id".to_string() => Value::string("rule2"),
            }),
        ]);
        let state = State::empty().set("__universe_rules__", existing_rules);

        let params = {
            let mut p = HashMap::new();
            p.insert("remove_rule_id".to_string(), Value::string("rule1"));
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let rules = result.get("__universe_rules__").unwrap();
        let rules_list = rules.as_list().unwrap();
        assert_eq!(rules_list.len(), 1);
        assert_eq!(rules_list[0].get("rule_id"), Some(&Value::string("rule2")));
    }

    /// Test inject_rule: custom rules_key and result_attr
    #[test]
    fn test_inject_rule_custom_keys() {
        let reg = make_reg();
        let state = State::empty();

        let new_rule = Value::from(im::hashmap! {
            "rule_id".to_string() => Value::string("rule1"),
        });
        let params = {
            let mut p = HashMap::new();
            p.insert("add_rule".to_string(), new_rule);
            p.insert("rules_key".to_string(), Value::string("my_rules"));
            p.insert("result_attr".to_string(), Value::string("my_result"));
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();

        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        // Using custom rules_key
        let rules = result.get("my_rules").unwrap();
        assert_eq!(rules.as_list().unwrap().len(), 1);
        // Using custom result_attr
        let info = result.get("my_result").unwrap();
        assert_eq!(info.get("added_count"), Some(&Value::Integer(1)));
    }

    // ══════════════════════════════════════════════
    // BUG-06 regression test: inject_rule removed_count reflects actual removals
    // ══════════════════════════════════════════════
    // Original issue: removed_count only checked remove_rule_id.is_some(), reporting 1
    // even when the target rule doesn't exist, which is misleading.
    // After fix: removed_count reflects the actual number of rules removed from the list.
    #[test]
    fn test_inject_rule_removed_count_reflects_actual_removal() {
        let reg = make_reg();
        let existing_rules = Value::list(vec![
            Value::from(im::hashmap! {
                "rule_id".to_string() => Value::string("rule1"),
            }),
            Value::from(im::hashmap! {
                "rule_id".to_string() => Value::string("rule2"),
            }),
        ]);
        let state = State::empty().set("__universe_rules__", existing_rules);

        // Scenario 1: Remove existing rule → removed_count = 1
        let params = {
            let mut p = HashMap::new();
            p.insert("remove_rule_id".to_string(), Value::string("rule1"));
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();
        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let info = result.get("__inject_result__").unwrap();
        assert_eq!(
            info.get("removed_count"),
            Some(&Value::Integer(1)),
            "Removing existing rule: removed_count should be 1"
        );
        assert_eq!(info.get("total_rules"), Some(&Value::Integer(1)));

        // Scenario 2: Remove non-existent rule → removed_count = 0 (was 1 before BUG-06 fix)
        let params = {
            let mut p = HashMap::new();
            p.insert(
                "remove_rule_id".to_string(),
                Value::string("nonexistent_rule"),
            );
            p
        };
        let instr = GenericInstruction::new("inject_rule", params);
        let mut ctx = ExecCtlCtx::new();
        let result = exec_inject_rule(&reg, &state, &instr, &mut ctx).unwrap();
        let info = result.get("__inject_result__").unwrap();
        assert_eq!(
            info.get("removed_count"),
            Some(&Value::Integer(0)),
            "BUG-06 regression: removed_count should be 0 when removing non-existent rule, should not falsely report 1"
        );
        assert_eq!(
            info.get("total_rules"),
            Some(&Value::Integer(2)),
            "Rule list should remain unchanged when target doesn't exist"
        );
    }
}

/// Apply a rule — extracts the transform from the rule parameter and executes it.
///
/// If `result_attr` is specified, stores a snapshot of the state after transform execution
/// in that attribute (applied on top of the original state to avoid recursive nesting).
///
/// # Parameters
/// - `rule`: The rule object (inline rule) or `rule_id` to look up from `__universe_rules__`.
/// - `result_attr` (optional): Attribute to store the filtered state snapshot.
///
/// # Behavior
/// - If `rule` is provided as an object, extracts its `transform` field and executes it.
/// - If `rule_id` is provided, looks up the rule from `__universe_rules__`.
/// - If no rule is found, returns the original state unchanged.
/// - If `result_attr` is specified, stores a filtered snapshot (excluding `__`-prefixed fields).
pub(crate) fn exec_apply_rule(
    reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let rule_raw = instruction.params.get("rule").cloned();
    let rule_val = rule_raw.map(|v| resolve_path(state, &v)).or_else(|| {
        instruction
            .params
            .get("rule_id")
            .and_then(|rid| {
                resolve_path(state, rid)
                    .as_str()
                    .map(std::string::ToString::to_string)
            })
            .and_then(|rule_id| {
                state
                    .get("__universe_rules__")
                    .and_then(|v| v.as_list())
                    .and_then(|rules| {
                        rules
                            .iter()
                            .find(|r| {
                                r.get("rule_id")
                                    .and_then(|v| v.as_str())
                                    .is_some_and(|s| s == rule_id)
                            })
                            .cloned()
                    })
            })
    });

    let rule_obj = match rule_val {
        Some(v) => v,
        None => {
            return Err(rule_not_found("unknown"));
        }
    };

    let transform = match rule_obj.get("transform") {
        Some(t) => t.clone(),
        None => return Err(missing_param("apply_rule", "transform")),
    };

    let transform_instr = GenericInstruction::from_value(&transform)?;
    let result = reg.execute(state, &transform_instr, ctx)?;

    match instruction.params.get("result_attr") {
        Some(attr_val) => {
            let attr_str = resolve_path(state, attr_val);
            match attr_str {
                Value::String(s) => {
                    // Store user data after transform execution in result_attr
                    // Exclude system fields (__exec__, etc.) to avoid recursive nesting
                    // and internal state leakage
                    let result_val = result.to_value();
                    let filtered = match &result_val {
                        Value::Object(m) => {
                            let filtered: im::HashMap<String, Value> = m
                                .iter_sorted()
                                .filter(|(k, _)| !k.starts_with("__"))
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            Value::Object(filtered)
                        }
                        other => other.clone(),
                    };
                    Ok(result.set(s, filtered))
                }
                _ => Ok(result),
            }
        }
        None => Ok(result),
    }
}

/// Observe the rule set — reads `__universe_rules__` from the state and stores it in the specified attribute.
///
/// # Parameters
/// - `result_attr` (optional): Attribute to store the rule list (default: `"rules_list"`).
///
/// # Behavior
/// - Reads the rule set from `__universe_rules__`.
/// - If absent, returns an empty list.
pub(crate) fn exec_observe_rules(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "rules_list".to_string());

    let rules = state
        .get("__universe_rules__")
        .cloned()
        .unwrap_or(Value::empty_list());

    state.set_path(&result_attr, rules)
}

/// Filter the rule set — filter rules by condition.
///
/// P1-3 fix: Uses structured matching instead of brute-force string matching.
/// Old implementation: `format!("{:?}", rule).contains(attr)` — could accidentally
/// match substrings within values.
/// New implementation: recursively checks whether the rule's `domain` field
/// references the target attributes.
///
/// Matching logic:
/// - If `goal_attrs` is empty, returns all rules
/// - Otherwise, checks if the rule's `domain` has an Atom whose `attribute`
///   matches any target attribute
/// - Also checks if And/Or/Not combinations in the `domain` reference the target attributes
///
/// # Parameters
/// - `rules_ref` (optional): Path to the rule list (default: `"__universe_rules__"`).
/// - `goal_attrs`: List of attribute names to filter by.
/// - `result_attr` (optional): Attribute to store the filtered list (default: `"filtered_rules"`).
///
/// # Behavior
/// - Returns rules whose `domain` references any of the `goal_attrs`.
/// - `universal` domains are considered to reference all attributes.
/// - Empty `goal_attrs` returns all rules.
pub(crate) fn exec_filter_rules(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let rules_ref = instruction
        .params
        .get("rules_ref")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "__universe_rules__".to_string());

    let goal_attrs = instruction
        .params
        .get("goal_attrs")
        .and_then(|v| {
            resolve_path(state, v).as_list().map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();

    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "filtered_rules".to_string());

    let rules = state.get_path(&rules_ref);
    let filtered = match rules {
        Some(Value::List(ref rules_list)) => {
            let result: im::Vector<Value> = rules_list
                .iter()
                .filter(|rule| {
                    if goal_attrs.is_empty() {
                        return true;
                    }
                    // Structured matching: check if the rule's domain references the target attributes
                    domain_references_attrs(rule, &goal_attrs)
                })
                .cloned()
                .collect();
            Value::List(result)
        }
        _ => Value::empty_list(),
    };

    state.set_path(&result_attr, filtered)
}

/// Check whether a rule's domain references any of the target attributes.
///
/// Recursively traverses the domain structure, checking all Atom nodes' attribute fields.
///
/// # Semantics
///
/// - `universal` domain: Returns `true` (matches any attribute set).
///   This is a design decision: `universal` domains apply to all states,
///   so they are considered relevant for any filtering criteria.
/// - `empty`/`never` domain: Returns `false`.
/// - `instruction` domain: Returns `false` (does not reference state attributes).
fn domain_references_attrs(rule: &Value, goal_attrs: &[String]) -> bool {
    let domain = match rule.get("domain") {
        Some(d) => d,
        None => return false,
    };
    domain_value_references_attrs(domain, goal_attrs)
}

/// Recursively check whether a domain Value references any target attributes.
fn domain_value_references_attrs(domain: &Value, goal_attrs: &[String]) -> bool {
    match domain {
        Value::Object(m) => {
            let dtype = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match dtype {
                "atom" => {
                    // Atom: check if the attribute field matches any target attribute
                    m.get("attribute")
                        .and_then(|v| v.as_str())
                        .is_some_and(|attr| goal_attrs.iter().any(|goal| attr == goal))
                }
                "and" | "or" => {
                    // And/Or: recursively check the child domain list
                    m.get("domains")
                        .and_then(|v| v.as_list())
                        .is_some_and(|list| {
                            list.iter()
                                .any(|d| domain_value_references_attrs(d, goal_attrs))
                        })
                }
                "not" => {
                    // Not: recursively check the inner domain
                    m.get("inner")
                        .is_some_and(|d| domain_value_references_attrs(d, goal_attrs))
                }
                "instruction" => {
                    // Instruction domains do not reference state attributes
                    false
                }
                "universal" | "empty" | "never" => {
                    // Universal matches everything (returns true).
                    // Empty/Never matches nothing (returns false).
                    dtype == "universal"
                }
                _ => false,
            }
        }
        _ => false,
    }
}

// ══════════════════════════════════════════════
// inject_rule
// ══════════════════════════════════════════════

/// Inject/replace a rule — modifies the rule set in `__universe_rules__`.
///
/// TCB design principle assessment:
///   Q1: Directly modifies State? → Yes (modifies __`universe_rules`__ list)
///   Q2: Calls other exec_ functions? → No
///   Q3: Pure computation? → No
///   Conclusion: Needs to be registered as a primitive.
///
/// Five-criteria test:
///   S1 Indivisible: list filtering + replacement is atomic ✓
///   S2 No composable alternative: `state_set` can only replace the whole list,
///      cannot filter by `rule_id` ✓
///   S3 Cross-scenario reuse: scheduler/amendment/meta all need dynamic rule set
///      modification ✓
///   S4 Minimal interface: `remove_rule_id` + `add_rule` two parameters ✓
///   S5 Deterministic: same input → same output ✓
///
/// # Parameters
/// - `rules_key` (optional): Path to the rule list in State (default: `"__universe_rules__"`).
/// - `remove_rule_id` (optional): ID of the rule to remove.
/// - `add_rule` (optional): New rule object to add.
/// - `result_attr` (optional): Path to store the operation result (default: `"__inject_result__"`).
///
/// # Operation Result
/// The `result_attr` contains:
/// - `removed_count`: Number of rules removed (0 or 1).
/// - `replaced_count`: Number of rules replaced (0 or 1).
/// - `added_count`: Number of rules added (0 or 1).
/// - `total_rules`: Total number of rules after the operation.
pub(crate) fn exec_inject_rule(
    _reg: &InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
    _ctx: &mut ExecCtlCtx,
) -> Result<State, EvoRuleError> {
    let rules_key = instruction
        .params
        .get("rules_key")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "__universe_rules__".to_string());

    let remove_rule_id = instruction.params.get("remove_rule_id").and_then(|v| {
        resolve_path(state, v)
            .as_str()
            .map(std::string::ToString::to_string)
    });

    let add_rule = instruction.params.get("add_rule").cloned();

    let result_attr = instruction
        .params
        .get("result_attr")
        .and_then(|v| {
            resolve_path(state, v)
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "__inject_result__".to_string());

    // Read the current rule list
    let current_rules = state.get_path(&rules_key);
    let rules_list = match &current_rules {
        Some(Value::List(v)) => v.clone(),
        _ => im::Vector::new(),
    };

    // Filter out the rule to be removed
    let mut new_rules = rules_list;
    let len_before = new_rules.len();
    if let Some(ref rule_id) = remove_rule_id {
        new_rules = new_rules
            .into_iter()
            .filter(|r| {
                r.get("rule_id")
                    .and_then(|v| v.as_str())
                    .is_none_or(|id| id != rule_id)
            })
            .collect();
    }
    // BUG-06 fix: removed_count originally only checked remove_rule_id.is_some(),
    // reporting 1 even when the target rule doesn't exist, which was misleading.
    // Changed to reflect actual number of removed rules.
    let actually_removed = len_before - new_rules.len();

    // Add the new rule
    let mut replaced = false;
    if let Some(ref rule) = add_rule {
        // If the new rule's rule_id already exists, replace it
        let new_rule_id = rule
            .get("rule_id")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        if let Some(ref rid) = new_rule_id {
            let existing_pos = new_rules.iter().position(|r| {
                r.get("rule_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id == rid)
            });
            if let Some(pos) = existing_pos {
                new_rules.remove(pos);
                new_rules.insert(pos, rule.clone());
                replaced = true;
            }
        }
        if !replaced {
            new_rules.push_back(rule.clone());
        }
    }

    // Construct result information
    // - removed_count: actual number of rules removed (BUG-06 fix: was falsely reported as remove_rule_id.is_some())
    // - replaced_count: counts only add_rule replacements of existing rules with the same ID
    // - added_count: rules added to the end of the list (excluding replacements)
    let result_info = Value::Object(im::hashmap! {
        "removed_count".to_string() => Value::Integer(actually_removed as i64),
        "replaced_count".to_string() => Value::Integer(i64::from(replaced)),
        "added_count".to_string() => Value::Integer(i64::from(add_rule.is_some() && !replaced)),
        "total_rules".to_string() => Value::Integer(new_rules.len() as i64),
    });

    let state = state.set_path(&rules_key, Value::List(new_rules))?;
    state.set_path(&result_attr, result_info)
}
