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

//! Audit primitives — Audit tracing and recording.
//!
//! # Core Functions
//!
//! - `trace_step`: Audit trace step — records the current execution snapshot.
//!
//! # Design Principles
//!
//! ## Deterministic Audit Chain
//!
//! `trace_step` is **fully deterministic**:
//! - Uses `LogicalClock` tick instead of wall-clock timestamps.
//! - Uses HMAC-derived nonce instead of UUID.
//! - Same input → same audit chain (idempotent).
//!
//! ## State Hash Tracking (P0-A)
//!
//! `trace_step` reads from `__exec__.last_dispatch_hashes` to obtain before/after
//! state hashes recorded by `dispatch`. This enables audit records to distinguish
//! "what dispatch changed" — the exact state transformation caused by the
//! dispatched instruction.
//!
//! If `last_dispatch_hashes` is absent (non-dispatch path), `trace_step` falls
//! back to computing the current state hash (legacy behavior, before == after).
//!
//! ## Chain Integrity
//!
//! Each record is HMAC-SHA256 protected, covering all fields (including
//! `execution_result`, `change_summary`, and `error_message` — fixed in
//! the audit vulnerability fix). The chain is tamper-proof and verifiable.
//!
//! # Determinism Guarantee
//!
//! `trace_step` is **L1 deterministic**:
//! - Same input state + same instruction → same audit record.
//! - No randomness, wall-clock time, or side effects.
//! - ID and nonce are deterministically derived from chain position.
//! - State hashes are computed deterministically.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `trace_step` record generation | ✅ L1 deterministic | Deterministic derivation |
//! | Record ID | ✅ L1 deterministic | `content_hash(prev_hash + tick)` |
//! | Record nonce | ✅ L1 deterministic | HMAC(key, `prev_hash` + tick) |
//! | Logical tick | ✅ L1 deterministic | `u64` increment |
//! | State hash (dispatch path) | ✅ L1 deterministic | From `last_dispatch_hashes` |
//! | State hash (fallback) | ✅ L1 deterministic | `content_hash` of business state |
//! | HMAC-SHA256 | ✅ L1 + L2 deterministic | `hmac` + `sha2` versions locked |
//! | Audit chain chain links | ✅ L1 deterministic | Hash chain |
//!
//! # Cross-Language Note (L4)
//!
//! To replicate audit record verification in other languages, see the encoding
//! specification in `audit.rs`. The HMAC key (`DEFAULT_HMAC_KEY`) is fixed for
//! deterministic reproducibility.

use crate::audit::{AuditChainState, AuditRecord, ExecutionResult, DEFAULT_HMAC_KEY};
use crate::deterministic::content_hash;
use crate::error::EvoRuleError;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;
use crate::value::Value;

/// Register audit primitives.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("trace_step", exec_trace_step);
}

/// Audit trace step — records the current execution snapshot.
///
/// # Parameters
/// - `label` (optional): Label to identify the step (default: `"step"`).
/// - `rule_id` (optional): Rule ID that triggered the step. If not provided,
///   defaults to the current instruction type from `__exec__.instruction.type`.
///
/// # Behavior
/// 1. Checks the `audit_on` switch in `__exec__`. If disabled, returns the
///    original state unchanged.
/// 2. Constructs a `change_summary` containing the label and instruction type.
/// 3. Reads `last_dispatch_hashes` from `__exec__` to obtain before/after state hashes.
/// 4. Uses the logical clock tick from the audit chain state.
/// 5. Deterministically derives ID and nonce from the chain position.
/// 6. Creates an `AuditRecord` with HMAC-SHA256 integrity protection.
/// 7. Appends the record to the audit chain and stores it in `__audit_chain`.
///
/// # Audit Trail (`__audit_chain`)
/// - The audit chain is stored in the `__audit_chain` field of the state.
/// - Each record is HMAC-SHA256 chained to the previous record.
/// - The chain is tamper-proof and verifiable via `AuditRecord::verify()`.
///
/// # Determinism
/// - ID: `content_hash(previous_hash + tick)`
/// - Nonce: `HMAC(key, previous_hash + tick)`
/// - Timestamp: logical tick (not wall-clock time)
pub(crate) fn exec_trace_step(
    _reg: &crate::instruction::registry::InstructionRegistry,
    state: &State,
    instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    // Check the audit_on switch (injected into __exec__ by evaluate())
    let audit_on = state
        .get("__exec__")
        .and_then(|v| v.get("audit_on"))
        .and_then(super::super::value::Value::as_bool)
        .unwrap_or(true);

    if !audit_on {
        // Audit is disabled; skip and return the original state
        return Ok(state.clone());
    }

    let label = instruction
        .params
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("step");

    let rule_id = instruction
        .params
        .get("rule_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            // Default to the current instruction type as the rule_id
            state
                .get("__exec__")
                .and_then(|v| v.get("instruction"))
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        });

    // Get the instruction type and write it into change_summary to distinguish the trigger primitive
    let instruction_type = state
        .get("__exec__")
        .and_then(|v| v.get("instruction"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Construct change_summary: contains label and instruction_type
    let change_summary = match instruction_type {
        Some(ref itype) if itype != label => Some(format!("[{itype}] {label}")),
        _ => Some(label.to_string()),
    };

    // Get the current chain state
    let mut chain_state: AuditChainState = state
        .get("__audit_chain")
        .and_then(AuditChainState::from_value)
        .unwrap_or_else(AuditChainState::empty);

    // ── P0-A: Read before/after hash from __exec__.last_dispatch_hashes ──
    // Dispatch records the state hash before and after executing the case.
    // trace_step reads from this field, allowing audit records to distinguish
    // "what dispatch changed".
    // If last_dispatch_hashes doesn't exist (e.g., trace_step triggered from
    // a non-dispatch path), fall back to computing the current state hash
    // (legacy behavior).
    let dispatch_hashes = state
        .get("__exec__")
        .and_then(|v| v.get("last_dispatch_hashes"));

    let (state_before_hash, state_after_hash) = if let Some(hashes) = dispatch_hashes {
        let before = hashes
            .get("before_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let after = hashes
            .get("after_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        (before, after)
    } else {
        // Fallback: compute the current state hash (legacy behavior, before == after)
        let mut snapshot = im::HashMap::new();
        for (k, v) in state.data().iter() {
            if k != "__exec__" && k != "__audit_chain" && k != "__parallel_provenance__" {
                snapshot.insert(k.clone(), v.clone());
            }
        }
        let snapshot_value = Value::Object(snapshot);
        // content_hash takes &[Value] (owned), so clone is required despite clippy suggestion.
        #[allow(clippy::cloned_ref_to_slice_refs)]
        let state_hash = content_hash(&[snapshot_value]);
        (state_hash.clone(), state_hash)
    };

    // Deterministic derivation: use LogicalClock tick instead of wall-clock timestamp
    let tick = chain_state.next_tick();
    let previous_hash = chain_state.latest_hash.clone();

    // Deterministically derive ID and nonce
    let id = AuditRecord::derive_id(&previous_hash, tick);
    let nonce = AuditRecord::derive_nonce(&previous_hash, tick, DEFAULT_HMAC_KEY);

    let record = AuditRecord::new(
        &id,
        &rule_id,
        tick as i64, // Logical tick, not wall-clock time
        &state_before_hash,
        &state_after_hash,
        change_summary,
        ExecutionResult::Success,
        None,
        &previous_hash,
        &nonce,
        DEFAULT_HMAC_KEY,
    );

    // Update the chain state
    if chain_state.records.is_empty() {
        chain_state.root_hash = record.hash.clone();
        chain_state.created_at = tick as i64;
    }
    chain_state.latest_hash = record.hash.clone();
    chain_state.updated_at = tick as i64;
    chain_state.records.push(record);

    // Save to the state
    Ok(state.set("__audit_chain", chain_state.to_value()))
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_trace_step_simple() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("test_step"));
        params.insert("rule_id".to_string(), Value::string("test.rule"));
        let instr = GenericInstruction::new("trace_step", params);

        let result = exec_trace_step(&reg, &state, &instr).unwrap();
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain_state.records.len(), 1);
        let record = &chain_state.records[0];
        assert_eq!(record.rule_id, "test.rule");
        assert_eq!(record.change_summary, Some("test_step".to_string()));
        assert!(record.verify(DEFAULT_HMAC_KEY));
        // Determinism verification: timestamp is a logical tick, not wall-clock
        assert_eq!(record.timestamp, 0);
    }

    #[test]
    fn test_trace_step_multiple() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let mut state = State::new(vec![("x", Value::Integer(1))]);

        for i in 0..3 {
            let mut params = HashMap::new();
            params.insert("label".to_string(), Value::string(format!("step_{}", i)));
            params.insert("rule_id".to_string(), Value::string("test.rule"));
            let instr = GenericInstruction::new("trace_step", params);
            state = exec_trace_step(&reg, &state, &instr).unwrap();
        }

        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain_state.records.len(), 3);
        // Verify logical ticks are increasing
        assert_eq!(chain_state.records[0].timestamp, 0);
        assert_eq!(chain_state.records[1].timestamp, 1);
        assert_eq!(chain_state.records[2].timestamp, 2);
    }

    #[test]
    fn test_trace_step_chain_validation() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let mut state = State::new(vec![("counter", Value::Integer(0))]);

        // Add multiple records
        for i in 0..5 {
            let mut params = HashMap::new();
            params.insert(
                "label".to_string(),
                Value::string(format!("increment_{}", i)),
            );
            params.insert("rule_id".to_string(), Value::string("test.increment"));
            let instr = GenericInstruction::new("trace_step", params);
            state = exec_trace_step(&reg, &state, &instr).unwrap();
        }

        // Verify the chain structure
        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain_state.records.len(), 5);

        // Verify chain links
        for i in 1..chain_state.records.len() {
            assert_eq!(
                chain_state.records[i].previous_hash,
                chain_state.records[i - 1].hash
            );
        }

        // Verify each record's hash
        for record in &chain_state.records {
            assert!(record.verify(DEFAULT_HMAC_KEY));
        }
    }

    #[test]
    fn test_trace_step_deterministic() {
        // Core test: same input produces the same audit chain
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let state1 = State::new(vec![("x", Value::Integer(42))]);
        let state2 = State::new(vec![("x", Value::Integer(42))]);

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("test_step"));
        params.insert("rule_id".to_string(), Value::string("test.rule"));
        let instr = GenericInstruction::new("trace_step", params);

        let result1 = exec_trace_step(&reg, &state1, &instr).unwrap();
        let result2 = exec_trace_step(&reg, &state2, &instr).unwrap();

        let chain1 = result1.get("__audit_chain").cloned().unwrap();
        let chain2 = result2.get("__audit_chain").cloned().unwrap();

        // Same input → same audit chain (fully deterministic)
        assert_eq!(chain1, chain2);
    }

    // ══════════════════════════════════════════
    // P0-A Specific tests: dispatch before/after hash distinction
    // ══════════════════════════════════════════

    /// P0-A-01: When __exec__.last_dispatch_hashes exists,
    /// trace_step should use the dispatch-recorded before/after hashes
    /// rather than computing its own hash of the same state.
    #[test]
    fn test_p0a_trace_step_uses_dispatch_hashes() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);

        // Simulate last_dispatch_hashes written by dispatch
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("set_context"),
                "params".to_string() => Value::empty_object(),
            }),
            "audit_on".to_string() => Value::Bool(true),
            "last_dispatch_hashes".to_string() => Value::Object(im::hashmap! {
                "before_hash".to_string() => Value::string("hash_before_dispatch"),
                "after_hash".to_string() => Value::string("hash_after_dispatch"),
                "case_key".to_string() => Value::string("increment"),
            }),
        });
        let state = state.set("__exec__", exec_ctx);

        // Initialize the audit chain
        let state = state.set("__audit_chain", AuditChainState::empty().to_value());

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("dispatch_step"));
        params.insert("rule_id".to_string(), Value::string("increment"));
        let instr = GenericInstruction::new("trace_step", params);

        let result = exec_trace_step(&reg, &state, &instr).unwrap();
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain_state.records.len(), 1);
        let record = &chain_state.records[0];

        // Core assertion: before_hash and after_hash should be different
        assert_eq!(record.state_before_hash, "hash_before_dispatch");
        assert_eq!(record.state_after_hash, "hash_after_dispatch");
        assert_ne!(record.state_before_hash, record.state_after_hash);
    }

    /// P0-A-02: When __exec__.last_dispatch_hashes doesn't exist,
    /// trace_step should fall back to the legacy behavior (before == after).
    #[test]
    fn test_p0a_trace_step_fallback_without_dispatch_hashes() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(42))]);

        // Don't set last_dispatch_hashes
        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("noop"),
                "params".to_string() => Value::empty_object(),
            }),
            "audit_on".to_string() => Value::Bool(true),
        });
        let state = state.set("__exec__", exec_ctx);
        let state = state.set("__audit_chain", AuditChainState::empty().to_value());

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("non_dispatch_step"));
        params.insert("rule_id".to_string(), Value::string("noop"));
        let instr = GenericInstruction::new("trace_step", params);

        let result = exec_trace_step(&reg, &state, &instr).unwrap();
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain_state = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain_state.records.len(), 1);
        let record = &chain_state.records[0];

        // Fallback behavior: before_hash == after_hash
        assert_eq!(record.state_before_hash, record.state_after_hash);
    }

    /// P0-A-03: The before/after hashes recorded by dispatch are deterministic
    /// in the audit chain.
    #[test]
    fn test_p0a_dispatch_hashes_deterministic() {
        let reg = crate::instruction::registry::InstructionRegistry::new();
        let state1 = State::new(vec![("x", Value::Integer(42))]);
        let state2 = State::new(vec![("x", Value::Integer(42))]);

        let exec_ctx = Value::Object(im::hashmap! {
            "instruction".to_string() => Value::Object(im::hashmap!{
                "type".to_string() => Value::string("test"),
                "params".to_string() => Value::empty_object(),
            }),
            "audit_on".to_string() => Value::Bool(true),
            "last_dispatch_hashes".to_string() => Value::Object(im::hashmap! {
                "before_hash".to_string() => Value::string("abc123"),
                "after_hash".to_string() => Value::string("def456"),
                "case_key".to_string() => Value::string("test"),
            }),
        });
        let state1 = state1.set("__exec__", exec_ctx.clone());
        let state1 = state1.set("__audit_chain", AuditChainState::empty().to_value());
        let state2 = state2.set("__exec__", exec_ctx);
        let state2 = state2.set("__audit_chain", AuditChainState::empty().to_value());

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("step"));
        params.insert("rule_id".to_string(), Value::string("test"));
        let instr = GenericInstruction::new("trace_step", params);

        let result1 = exec_trace_step(&reg, &state1, &instr).unwrap();
        let result2 = exec_trace_step(&reg, &state2, &instr).unwrap();

        let chain1 = result1.get("__audit_chain").cloned().unwrap();
        let chain2 = result2.get("__audit_chain").cloned().unwrap();

        // Same input → same audit chain (determinism preserved)
        assert_eq!(chain1, chain2);
    }
}
