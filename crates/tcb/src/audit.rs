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

//! Audit chain — `EvoRule`'s trusted record system.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Determinism Guarantee
//!
//! All components in this module are **L1 deterministic**:
//! - `AuditRecord`: ID and nonce deterministically derived from chain position.
//! - `AuditChainState`: Logical clock (tick) replaces wall-clock time.
//! - HMAC integrity: Fixed key + deterministic encoding → deterministic hashes.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `AuditRecord` ID | ✅ L1 deterministic | `content_hash(prev_hash` + tick) |
//! | `AuditRecord` nonce | ✅ L1 deterministic | HMAC(key, `prev_hash` + tick) |
//! | Logical tick | ✅ L1 deterministic | u64 increment |
//! | HMAC-SHA256 | ✅ L1 + L2 deterministic | `hmac` + `sha2` versions locked |
//! | `to_le_bytes()` / `to_be_bytes()` | ✅ L1 + L3 deterministic | Fixed endianness |
//!
//! # Cross-Language Note (L4)
//!
//! To replicate audit record verification in other languages:
//! 1. Implement HMAC-SHA256 with the same key
//! 2. Encode fields with length-prefixed strings (big-endian u64 length)
//! 3. Use little-endian for integer fields (timestamp, tick)
//! 4. The HMAC input order must exactly match `AuditRecord::compute_hash`
//!
//! # Security Note
//!
//! The default HMAC key (`DEFAULT_HMAC_KEY`) is fixed for deterministic reproducibility.
//! For production deployments requiring cryptographic non-repudiation, replace with
//! a deployment-specific key via the `verify(key)` parameter.
//!
//! The TCB layer only defines data structures and security primitives.
//! `AuditChain` implementation is in the Governance layer.

use crate::deterministic::content_hash;
use crate::value::Value;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// HMAC key type.
type HmacSha256 = Hmac<Sha256>;

/// Default audit HMAC key.
///
/// Fixed key ensures deterministic nonce generation. For deployments requiring
/// cryptographic non-repudiation, replace with a deployment-specific key via
/// the `verify(key)` parameter. This key is **not** a secret in the
/// deterministic TCB context — it's a fixed parameter that guarantees
/// reproducibility.
pub const DEFAULT_HMAC_KEY: &[u8] = b"evorule-audit-chain-v2";

// ══════════════════════════════════════════════
// ExecutionResult
// ══════════════════════════════════════════════

/// Execution result enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionResult {
    Success,
    Failure,
    Skipped,
}

// ══════════════════════════════════════════════
// AuditRecord
// ══════════════════════════════════════════════

/// A single audit record.
///
/// Fully deterministic: id and nonce are deterministically derived from the
/// `LogicalClock` tick. timestamp is a logical tick (not wall-clock time),
/// guaranteeing that the same input produces the same record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditRecord {
    /// Record unique ID (deterministically derived: `content_hash(previous_hash` + tick)).
    pub id: String,
    /// The rule ID that triggered this record.
    pub rule_id: String,
    /// Logical clock tick (deterministic, not wall-clock time).
    pub timestamp: i64,
    /// State hash before execution.
    pub state_before_hash: String,
    /// State hash after execution.
    pub state_after_hash: String,
    /// Summary of state changes (optional).
    pub change_summary: Option<String>,
    /// Execution result.
    pub execution_result: ExecutionResult,
    /// Error message (on failure).
    pub error_message: Option<String>,
    /// Current record hash (HMAC-SHA256).
    pub hash: String,
    /// Hash of the previous record (chain structure).
    pub previous_hash: String,
    /// Deterministic nonce (HMAC-derived: HMAC(key, `previous_hash` + tick), anti-replay).
    pub nonce: String,
    /// Record format version.
    pub version: u32,
}

impl AuditRecord {
    /// Deterministically derive a record ID from a logical clock tick.
    ///
    /// ID = `content_hash(previous_hash` + tick), guaranteeing:
    /// - Same chain position → same ID (deterministic)
    /// - Different chain positions → different IDs (uniqueness)
    pub fn derive_id(previous_hash: &str, tick: u64) -> String {
        content_hash(&[Value::string(previous_hash), Value::Integer(tick as i64)])
    }

    /// Deterministically derive a nonce from a logical clock tick.
    ///
    /// nonce = HMAC(key, `previous_hash` || tick), guaranteeing:
    /// - Same chain position → same nonce (deterministic)
    /// - Unforgeable (requires HMAC key), preserving anti-replay capability
    ///
    /// # Panics
    ///
    /// This function uses `HmacSha256::new_from_slice(key).unwrap()`. HMAC
    /// accepts any key length, so this never panics in practice. The `unwrap`
    /// is intentional and safe due to the HMAC API guarantee.
    pub fn derive_nonce(previous_hash: &str, tick: u64, key: &[u8]) -> String {
        // HMAC accepts any key length per RFC 2104; new_from_slice never errors.
        // Local allow is required because clippy::expect_used cannot prove this.
        #[allow(clippy::expect_used)]
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(previous_hash.as_bytes());
        mac.update(&tick.to_le_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Create a new audit record.
    #[allow(clippy::too_many_arguments)] // 11 args needed for AuditRecord::new (HMAC fields)
    pub fn new(
        id: &str,
        rule_id: &str,
        timestamp: i64,
        state_before_hash: &str,
        state_after_hash: &str,
        change_summary: Option<String>,
        execution_result: ExecutionResult,
        error_message: Option<String>,
        previous_hash: &str,
        nonce: &str,
        key: &[u8],
    ) -> Self {
        // BUG-03 fix: version field included in HMAC signature to prevent version tampering without breaking hash chain.
        // Use constant instead of hardcoded literal to ensure new() and compute_hash() use the same version.
        const CURRENT_VERSION: u32 = 3;
        let hash = Self::compute_hash(
            id,
            nonce,
            rule_id,
            timestamp,
            state_before_hash,
            state_after_hash,
            previous_hash,
            execution_result,
            change_summary.as_deref(),
            error_message.as_deref(),
            CURRENT_VERSION,
            key,
        );

        Self {
            id: id.to_string(),
            rule_id: rule_id.to_string(),
            timestamp,
            state_before_hash: state_before_hash.to_string(),
            state_after_hash: state_after_hash.to_string(),
            change_summary,
            execution_result,
            error_message,
            hash,
            previous_hash: previous_hash.to_string(),
            nonce: nonce.to_string(),
            version: CURRENT_VERSION,
        }
    }

    /// Compute the HMAC-SHA256 hash of the record.
    ///
    /// Integrity coverage fields (fixing audit vulnerability where `execution_result`,
    /// `change_summary`, and `error_message` were not included in the signature,
    /// making them tamperable without breaking the hash chain).
    ///
    /// `Option<String>` fields are encoded uniformly with `0x00` (None) /
    /// `0x01 + utf8 bytes` (Some), preventing hash collisions between
    /// empty strings and None.
    ///
    /// # Panics
    ///
    /// This function uses `HmacSha256::new_from_slice(key).unwrap()`. HMAC
    /// accepts any key length, so this never panics in practice. The `unwrap`
    /// is intentional and safe due to the HMAC API guarantee.
    #[allow(clippy::too_many_arguments)] // 12 args needed for HMAC compute_hash inputs (including version)
    pub fn compute_hash(
        id: &str,
        nonce: &str,
        rule_id: &str,
        timestamp: i64,
        state_before_hash: &str,
        state_after_hash: &str,
        previous_hash: &str,
        execution_result: ExecutionResult,
        change_summary: Option<&str>,
        error_message: Option<&str>,
        version: u32,
        key: &[u8],
    ) -> String {
        // HMAC accepts any key length per RFC 2104; new_from_slice never errors.
        #[allow(clippy::expect_used)]
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");

        // Helper: write a length-prefixed string to prevent field-boundary
        // collisions (e.g., id="ab"+nonce="cd" vs id="abcd"+nonce="").
        fn write_str(mac: &mut HmacSha256, s: &str) {
            mac.update(&(s.len() as u64).to_be_bytes());
            mac.update(s.as_bytes());
        }

        write_str(&mut mac, id);
        write_str(&mut mac, nonce);
        write_str(&mut mac, rule_id);
        mac.update(&timestamp.to_le_bytes());
        write_str(&mut mac, state_before_hash);
        write_str(&mut mac, state_after_hash);
        write_str(&mut mac, previous_hash);

        // Execution result: single-byte encoding of enum variants to avoid string ambiguity
        mac.update(&[match execution_result {
            ExecutionResult::Success => 0u8,
            ExecutionResult::Failure => 1u8,
            ExecutionResult::Skipped => 2u8,
        }]);
        // change_summary: None=0x00, Some=0x01 + length-prefixed utf8
        match change_summary {
            Some(s) => {
                mac.update(&[1u8]);
                write_str(&mut mac, s);
            }
            None => mac.update(&[0u8]),
        }
        // error_message: None=0x00, Some=0x01 + length-prefixed utf8
        match error_message {
            Some(s) => {
                mac.update(&[1u8]);
                write_str(&mut mac, s);
            }
            None => mac.update(&[0u8]),
        }
        // BUG-03 fix: version field included in signature to prevent version tampering from misleading verifiers
        mac.update(&version.to_le_bytes());

        hex::encode(mac.finalize().into_bytes())
    }

    /// Verify the integrity of the record.
    pub fn verify(&self, key: &[u8]) -> bool {
        let expected_hash = Self::compute_hash(
            &self.id,
            &self.nonce,
            &self.rule_id,
            self.timestamp,
            &self.state_before_hash,
            &self.state_after_hash,
            &self.previous_hash,
            self.execution_result,
            self.change_summary.as_deref(),
            self.error_message.as_deref(),
            self.version,
            key,
        );

        self.hash == expected_hash
    }

    /// Convert to a Value.
    pub fn to_value(&self) -> Value {
        let mut map = im::HashMap::new();
        map.insert("id".to_string(), Value::string(&self.id));
        map.insert("rule_id".to_string(), Value::string(&self.rule_id));
        map.insert("timestamp".to_string(), Value::Integer(self.timestamp));
        map.insert(
            "state_before_hash".to_string(),
            Value::string(&self.state_before_hash),
        );
        map.insert(
            "state_after_hash".to_string(),
            Value::string(&self.state_after_hash),
        );

        if let Some(summary) = &self.change_summary {
            map.insert("change_summary".to_string(), Value::string(summary));
        }

        map.insert(
            "execution_result".to_string(),
            Value::string(match self.execution_result {
                ExecutionResult::Success => "success",
                ExecutionResult::Failure => "failure",
                ExecutionResult::Skipped => "skipped",
            }),
        );

        if let Some(error) = &self.error_message {
            map.insert("error_message".to_string(), Value::string(error));
        }

        map.insert("hash".to_string(), Value::string(&self.hash));
        map.insert(
            "previous_hash".to_string(),
            Value::string(&self.previous_hash),
        );
        map.insert("nonce".to_string(), Value::string(&self.nonce));
        map.insert(
            "version".to_string(),
            Value::Integer(i64::from(self.version)),
        );

        Value::Object(map)
    }

    /// Restore from a Value.
    pub fn from_value(val: &Value) -> Option<Self> {
        let id = val.get("id").and_then(|v| v.as_str())?.to_string();
        let rule_id = val.get("rule_id").and_then(|v| v.as_str())?.to_string();
        let timestamp = val
            .get("timestamp")
            .and_then(super::value::Value::as_integer)?;
        let state_before_hash = val
            .get("state_before_hash")
            .and_then(|v| v.as_str())?
            .to_string();
        let state_after_hash = val
            .get("state_after_hash")
            .and_then(|v| v.as_str())?
            .to_string();
        let change_summary = val
            .get("change_summary")
            .and_then(|v| v.as_str())
            .map(String::from);

        let execution_result = match val.get("execution_result").and_then(|v| v.as_str())? {
            "success" => ExecutionResult::Success,
            "failure" => ExecutionResult::Failure,
            "skipped" => ExecutionResult::Skipped,
            _ => return None,
        };

        let error_message = val
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(String::from);
        let hash = val.get("hash").and_then(|v| v.as_str())?.to_string();
        let previous_hash = val
            .get("previous_hash")
            .and_then(|v| v.as_str())?
            .to_string();
        let nonce = val.get("nonce").and_then(|v| v.as_str())?.to_string();
        let version = val
            .get("version")
            .and_then(super::value::Value::as_integer)? as u32;

        Some(Self {
            id,
            rule_id,
            timestamp,
            state_before_hash,
            state_after_hash,
            change_summary,
            execution_result,
            error_message,
            hash,
            previous_hash,
            nonce,
            version,
        })
    }
}

// ══════════════════════════════════════════════
// AuditChainState
// ══════════════════════════════════════════════

/// Audit chain state (for persistence).
///
/// Fully deterministic: uses `LogicalClock` instead of wall-clock time.
/// `created_at/updated_at` are logical ticks, not Unix milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditChainState {
    /// List of records in the chain.
    pub records: Vec<AuditRecord>,
    /// Root hash of the chain (hash of the first record).
    pub root_hash: String,
    /// Latest hash of the chain (hash of the last record).
    pub latest_hash: String,
    /// Chain creation time (logical tick).
    pub created_at: i64,
    /// Chain last update time (logical tick).
    pub updated_at: i64,
    /// Current logical clock tick.
    pub logical_tick: u64,
    /// Chain format version.
    pub version: u32,
}

impl AuditChainState {
    /// Create an empty chain state.
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
            root_hash: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            latest_hash: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            created_at: 0,
            updated_at: 0,
            logical_tick: 0,
            version: 3,
        }
    }

    /// Get the next logical tick and increment.
    pub fn next_tick(&mut self) -> u64 {
        let tick = self.logical_tick;
        self.logical_tick += 1;
        tick
    }

    /// Convert to a Value.
    pub fn to_value(&self) -> Value {
        let items: Vec<Value> = self.records.iter().map(AuditRecord::to_value).collect();
        let mut map = im::HashMap::new();
        map.insert("records".to_string(), Value::list(items));
        map.insert("root_hash".to_string(), Value::string(&self.root_hash));
        map.insert("latest_hash".to_string(), Value::string(&self.latest_hash));
        map.insert("created_at".to_string(), Value::Integer(self.created_at));
        map.insert("updated_at".to_string(), Value::Integer(self.updated_at));
        map.insert(
            "logical_tick".to_string(),
            Value::Integer(self.logical_tick as i64),
        );
        map.insert(
            "version".to_string(),
            Value::Integer(i64::from(self.version)),
        );
        Value::Object(map)
    }

    /// Restore from a Value.
    pub fn from_value(val: &Value) -> Option<Self> {
        let records_val = val.get("records")?;
        let list = records_val.as_list()?;
        let mut records = Vec::new();
        for item in list {
            records.push(AuditRecord::from_value(item)?);
        }

        let root_hash = val.get("root_hash").and_then(|v| v.as_str())?.to_string();
        let latest_hash = val.get("latest_hash").and_then(|v| v.as_str())?.to_string();
        let created_at = val
            .get("created_at")
            .and_then(super::value::Value::as_integer)?;
        let updated_at = val
            .get("updated_at")
            .and_then(super::value::Value::as_integer)?;
        let logical_tick = val
            .get("logical_tick")
            .and_then(super::value::Value::as_integer)
            .unwrap_or(0) as u64;
        let version = val
            .get("version")
            .and_then(super::value::Value::as_integer)? as u32;

        Some(Self {
            records,
            root_hash,
            latest_hash,
            created_at,
            updated_at,
            logical_tick,
            version,
        })
    }
}

// ══════════════════════════════════════════════
// Hash Utility
// ══════════════════════════════════════════════

/// Compute the SHA256 hash of a State.
pub fn compute_state_hash(state: &Value) -> String {
    content_hash(&[state.clone()])
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecCtlCtx;

    #[test]
    fn test_derive_id_deterministic() {
        let id1 = AuditRecord::derive_id("prev_hash", 1);
        let id2 = AuditRecord::derive_id("prev_hash", 1);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_derive_id_unique_per_tick() {
        let id1 = AuditRecord::derive_id("prev_hash", 1);
        let id2 = AuditRecord::derive_id("prev_hash", 2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_derive_nonce_deterministic() {
        let n1 = AuditRecord::derive_nonce("prev_hash", 1, DEFAULT_HMAC_KEY);
        let n2 = AuditRecord::derive_nonce("prev_hash", 1, DEFAULT_HMAC_KEY);
        assert_eq!(n1, n2);
    }

    #[test]
    fn test_derive_nonce_unique_per_tick() {
        let n1 = AuditRecord::derive_nonce("prev_hash", 1, DEFAULT_HMAC_KEY);
        let n2 = AuditRecord::derive_nonce("prev_hash", 2, DEFAULT_HMAC_KEY);
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_derive_nonce_key_dependent() {
        let n1 = AuditRecord::derive_nonce("prev_hash", 1, b"key-a");
        let n2 = AuditRecord::derive_nonce("prev_hash", 1, b"key-b");
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_audit_record_creation() {
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;
        let id = AuditRecord::derive_id(prev_hash, tick);
        let nonce = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);

        let rec = AuditRecord::new(
            &id,
            "test.rule",
            tick as i64,
            "state_before_hash",
            "state_after_hash",
            Some("counter: 0 → 1".to_string()),
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce,
            DEFAULT_HMAC_KEY,
        );

        assert_eq!(rec.id, id);
        assert_eq!(rec.rule_id, "test.rule");
        assert_eq!(rec.timestamp, 0);
        assert!(!rec.hash.is_empty());
        assert!(rec.verify(DEFAULT_HMAC_KEY));
    }

    #[test]
    fn test_audit_record_verify_wrong_key() {
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;
        let id = AuditRecord::derive_id(prev_hash, tick);
        let nonce = AuditRecord::derive_nonce(prev_hash, tick, b"correct-key");

        let rec = AuditRecord::new(
            &id,
            "test.rule",
            tick as i64,
            "state_before_hash",
            "state_after_hash",
            None,
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce,
            b"correct-key",
        );

        assert!(!rec.verify(b"wrong-key"));
    }

    #[test]
    fn test_audit_record_roundtrip() {
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 5;
        let id = AuditRecord::derive_id(prev_hash, tick);
        let nonce = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);

        let rec = AuditRecord::new(
            &id,
            "test.rule",
            tick as i64,
            "state_before_hash",
            "state_after_hash",
            Some("test summary".to_string()),
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce,
            DEFAULT_HMAC_KEY,
        );

        let val = rec.to_value();
        let restored = AuditRecord::from_value(&val).unwrap();

        assert_eq!(rec.id, restored.id);
        assert_eq!(rec.rule_id, restored.rule_id);
        assert_eq!(rec.hash, restored.hash);
        assert_eq!(rec.previous_hash, restored.previous_hash);
    }

    #[test]
    fn test_audit_chain_state_empty() {
        let state = AuditChainState::empty();
        assert!(state.records.is_empty());
        assert_eq!(state.version, 3);
        assert_eq!(state.logical_tick, 0);
    }

    #[test]
    fn test_audit_chain_state_next_tick() {
        let mut state = AuditChainState::empty();
        assert_eq!(state.next_tick(), 0);
        assert_eq!(state.next_tick(), 1);
        assert_eq!(state.next_tick(), 2);
        assert_eq!(state.logical_tick, 3);
    }

    #[test]
    fn test_audit_chain_state_roundtrip() {
        let mut state = AuditChainState::empty();
        state.next_tick();
        state.next_tick();
        let val = state.to_value();
        let restored = AuditChainState::from_value(&val).unwrap();
        assert_eq!(restored.logical_tick, 2);
    }

    #[test]
    fn test_audit_record_deterministic() {
        // Core assertion: same input produces the same audit record
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;

        let id1 = AuditRecord::derive_id(prev_hash, tick);
        let nonce1 = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);
        let rec1 = AuditRecord::new(
            &id1,
            "test.rule",
            tick as i64,
            "state_before",
            "state_after",
            None,
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce1,
            DEFAULT_HMAC_KEY,
        );

        let id2 = AuditRecord::derive_id(prev_hash, tick);
        let nonce2 = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);
        let rec2 = AuditRecord::new(
            &id2,
            "test.rule",
            tick as i64,
            "state_before",
            "state_after",
            None,
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce2,
            DEFAULT_HMAC_KEY,
        );

        assert_eq!(rec1, rec2);
    }

    #[test]
    fn test_compute_state_hash() {
        let state = Value::string("test state");
        let hash = compute_state_hash(&state);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }

    // ══════════════════════════════════════════
    // C0 Determinism tests
    // ══════════════════════════════════════════

    /// C0-01: Idempotent execution — same input produces the same output
    /// (the audit chain is fully identical).
    #[test]
    fn test_c0_idempotent_execution() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        // Execute the exact same operation twice; the audit chain must be identical
        let run_once = || -> AuditChainState {
            let reg = InstructionRegistry::new();
            let mut state = State::new(vec![("x", Value::Integer(1))]);
            let mut ctx = ExecCtlCtx::new();

            let mut params = HashMap::new();
            params.insert("label".to_string(), Value::string("step1"));
            params.insert("rule_id".to_string(), Value::string("rule.test"));
            let instr = GenericInstruction::new("trace_step", params);
            state = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();

            let mut params2 = HashMap::new();
            params2.insert("label".to_string(), Value::string("step2"));
            params2.insert("rule_id".to_string(), Value::string("rule.test2"));
            let instr2 = GenericInstruction::new("trace_step", params2);
            state = audit_ops::exec_trace_step(&reg, &state, &instr2, &mut ctx).unwrap();

            let chain_val = state.get("__audit_chain").cloned().unwrap();
            AuditChainState::from_value(&chain_val).unwrap()
        };

        let chain1 = run_once();
        let chain2 = run_once();

        // Two independent runs must produce identical audit chains
        assert_eq!(chain1.records.len(), chain2.records.len());
        for (r1, r2) in chain1.records.iter().zip(chain2.records.iter()) {
            assert_eq!(r1.id, r2.id, "record ID mismatch");
            assert_eq!(r1.hash, r2.hash, "record hash mismatch");
            assert_eq!(r1.nonce, r2.nonce, "nonce mismatch");
            assert_eq!(r1.rule_id, r2.rule_id, "rule_id mismatch");
            assert_eq!(r1.timestamp, r2.timestamp, "logical tick mismatch");
        }
        assert_eq!(
            chain1.latest_hash, chain2.latest_hash,
            "chain hash mismatch"
        );
    }

    /// C0-02: Sequential determinism — instruction execution order is consistent
    /// across runs.
    #[test]
    fn test_c0_sequential_determinism() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        // Execute N instructions and verify that the execution order is consistent
        // across runs.
        let run_once = || -> Vec<String> {
            let reg = InstructionRegistry::new();
            let mut state = State::new(vec![("x", Value::Integer(0))]);
            let mut ctx = ExecCtlCtx::new();

            let labels = vec!["alpha", "beta", "gamma", "delta"];
            for label in &labels {
                let mut params = HashMap::new();
                params.insert("label".to_string(), Value::string(*label));
                params.insert(
                    "rule_id".to_string(),
                    Value::string(format!("rule.{}", label)),
                );
                let instr = GenericInstruction::new("trace_step", params);
                state = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();
            }

            let chain_val = state.get("__audit_chain").cloned().unwrap();
            let chain = AuditChainState::from_value(&chain_val).unwrap();
            chain
                .records
                .iter()
                .map(|r| r.change_summary.clone().unwrap_or_default())
                .collect()
        };

        // Multiple runs, order must be consistent
        let order1 = run_once();
        let order2 = run_once();
        let order3 = run_once();

        assert_eq!(order1, order2, "Run 1 and run 2 order mismatch");
        assert_eq!(order2, order3, "Run 2 and run 3 order mismatch");
        assert_eq!(
            order1,
            vec!["alpha", "beta", "gamma", "delta"],
            "Execution order does not match enqueue order"
        );
    }

    /// C0-03: No implicit state — two independently constructed audit chain states
    /// must be identical.
    #[test]
    fn test_no_implicit_state() {
        let state1 = AuditChainState::empty();
        let state2 = AuditChainState::empty();
        assert_eq!(
            state1, state2,
            "Two independent empty chain states must be identical"
        );

        // Independently construct the same record; results must be identical
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;

        let id1 = AuditRecord::derive_id(prev_hash, tick);
        let nonce1 = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);
        let rec1 = AuditRecord::new(
            &id1,
            "rule.a",
            tick as i64,
            "before",
            "after",
            None,
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce1,
            DEFAULT_HMAC_KEY,
        );

        let id2 = AuditRecord::derive_id(prev_hash, tick);
        let nonce2 = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);
        let rec2 = AuditRecord::new(
            &id2,
            "rule.a",
            tick as i64,
            "before",
            "after",
            None,
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce2,
            DEFAULT_HMAC_KEY,
        );

        assert_eq!(
            rec1, rec2,
            "Two independently constructed records must be identical, with no implicit state"
        );
    }

    /// C2-04: Tamper detection — modifying any record field must cause verify()
    /// to fail.
    #[test]
    fn test_tamper_detection() {
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;
        let id = AuditRecord::derive_id(prev_hash, tick);
        let nonce = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);

        let rec = AuditRecord::new(
            &id,
            "test.rule",
            tick as i64,
            "state_before",
            "state_after",
            Some("original summary".to_string()),
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce,
            DEFAULT_HMAC_KEY,
        );

        assert!(
            rec.verify(DEFAULT_HMAC_KEY),
            "Original record must verify successfully"
        );

        // Tamper with rule_id
        let mut tampered = rec.clone();
        tampered.rule_id = "tampered.rule".to_string();
        assert!(
            !tampered.verify(DEFAULT_HMAC_KEY),
            "Tampering with rule_id must cause verification to fail"
        );

        // Tamper with state_after_hash
        let mut tampered = rec.clone();
        tampered.state_after_hash = "tampered_hash".to_string();
        assert!(
            !tampered.verify(DEFAULT_HMAC_KEY),
            "Tampering with state_after_hash must cause verification to fail"
        );

        // Tamper with previous_hash
        let mut tampered = rec.clone();
        tampered.previous_hash = "tampered_prev".to_string();
        assert!(
            !tampered.verify(DEFAULT_HMAC_KEY),
            "Tampering with previous_hash must cause verification to fail"
        );

        // Tamper with nonce
        let mut tampered = rec.clone();
        tampered.nonce = "tampered_nonce".to_string();
        assert!(
            !tampered.verify(DEFAULT_HMAC_KEY),
            "Tampering with nonce must cause verification to fail"
        );

        // Tamper with timestamp
        let mut tampered = rec.clone();
        tampered.timestamp = 999;
        assert!(
            !tampered.verify(DEFAULT_HMAC_KEY),
            "Tampering with timestamp must cause verification to fail"
        );
    }

    // ══════════════════════════════════════════
    // C1 Transparency tests
    // ══════════════════════════════════════════

    /// C1-02: Instruction type traceability — the audit record's rule_id must
    /// reflect the instruction type.
    #[test]
    fn test_c1_instruction_type_traceable() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(1))]);

        // Execute trace_step with a specified rule_id
        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("increment"));
        params.insert("rule_id".to_string(), Value::string("rule.increment_x"));
        let instr = GenericInstruction::new("trace_step", params);
        let mut ctx = ExecCtlCtx::new();

        let result = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain.records.len(), 1);
        // rule_id precisely records the rule source
        assert_eq!(chain.records[0].rule_id, "rule.increment_x");
        // change_summary contains the label information
        assert_eq!(
            chain.records[0].change_summary,
            Some("increment".to_string())
        );
    }

    /// C1-03: Rule source distinguishability — different rule_ids must produce
    /// distinguishable audit records.
    #[test]
    fn test_c1_rule_source_identifiable() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let mut state = State::new(vec![("x", Value::Integer(0))]);
        let mut ctx = ExecCtlCtx::new();

        // Execute two trace_step records with different rule_ids
        let mut params1 = HashMap::new();
        params1.insert("label".to_string(), Value::string("init"));
        params1.insert("rule_id".to_string(), Value::string("rule.init_x"));
        let instr1 = GenericInstruction::new("trace_step", params1);
        state = audit_ops::exec_trace_step(&reg, &state, &instr1, &mut ctx).unwrap();

        let mut params2 = HashMap::new();
        params2.insert("label".to_string(), Value::string("increment"));
        params2.insert("rule_id".to_string(), Value::string("rule.increment_x"));
        let instr2 = GenericInstruction::new("trace_step", params2);
        state = audit_ops::exec_trace_step(&reg, &state, &instr2, &mut ctx).unwrap();

        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain.records.len(), 2);
        // The two records have different rule_ids, making the rule source distinguishable
        assert_eq!(chain.records[0].rule_id, "rule.init_x");
        assert_eq!(chain.records[1].rule_id, "rule.increment_x");
        assert_ne!(chain.records[0].rule_id, chain.records[1].rule_id);
    }

    /// C1-04: Dispatch path visibility — change_summary distinguishes case hit
    /// vs default fallback.
    #[test]
    fn test_c1_dispatch_path_visible() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let mut state = State::new(vec![("x", Value::Integer(0))]);
        let mut ctx = ExecCtlCtx::new();

        // Simulate a case-hit trace_step
        let mut params1 = HashMap::new();
        params1.insert(
            "label".to_string(),
            Value::string("[dispatch] case hit: increment"),
        );
        params1.insert(
            "rule_id".to_string(),
            Value::string("dispatch.case.increment"),
        );
        let instr1 = GenericInstruction::new("trace_step", params1);
        state = audit_ops::exec_trace_step(&reg, &state, &instr1, &mut ctx).unwrap();

        // Simulate a default fallback trace_step
        let mut params2 = HashMap::new();
        params2.insert(
            "label".to_string(),
            Value::string("[dispatch] default fallback"),
        );
        params2.insert("rule_id".to_string(), Value::string("dispatch.default"));
        let instr2 = GenericInstruction::new("trace_step", params2);
        state = audit_ops::exec_trace_step(&reg, &state, &instr2, &mut ctx).unwrap();

        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain.records.len(), 2);
        // Case hit vs default fallback can be distinguished via label/rule_id
        assert!(chain.records[0]
            .change_summary
            .as_ref()
            .unwrap()
            .contains("case hit"));
        assert!(chain.records[1]
            .change_summary
            .as_ref()
            .unwrap()
            .contains("default"));
        assert_ne!(chain.records[0].rule_id, chain.records[1].rule_id);
    }

    // ══════════════════════════════════════════
    // C3 Traceability tests
    // ══════════════════════════════════════════

    /// C3-01: Result backtracking — the audit record must allow backtracking
    /// to the before/after state.
    #[test]
    fn test_c3_result_backtracking() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", Value::Integer(1))]);
        let mut ctx = ExecCtlCtx::new();

        let mut params = HashMap::new();
        params.insert("label".to_string(), Value::string("step"));
        params.insert("rule_id".to_string(), Value::string("test.rule"));
        let instr = GenericInstruction::new("trace_step", params);

        let result = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();
        let chain_val = result.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        assert_eq!(chain.records.len(), 1);
        let rec = &chain.records[0];
        // The audit record contains state_before_hash and state_after_hash
        assert!(!rec.state_before_hash.is_empty());
        assert!(!rec.state_after_hash.is_empty());
        // trace_step does not modify the state, so the before and after hashes are the same
        assert_eq!(rec.state_before_hash, rec.state_after_hash);
        // The record integrity can be verified via the hash
        assert!(rec.verify(DEFAULT_HMAC_KEY));
    }

    /// C3-02: Path reconstruction — the audit chain can be used to reconstruct
    /// the complete execution path.
    #[test]
    fn test_c3_path_reconstruction() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let mut state = State::new(vec![("x", Value::Integer(0))]);
        let mut ctx = ExecCtlCtx::new();

        // Execute 3 steps, simulating: increment → set_value → trace
        let steps = vec![
            ("rule.increment", "increment"),
            ("rule.set_value", "set_value"),
            ("rule.trace", "trace"),
        ];

        for (rule_id, label) in &steps {
            let mut params = HashMap::new();
            params.insert("label".to_string(), Value::string(*label));
            params.insert("rule_id".to_string(), Value::string(*rule_id));
            let instr = GenericInstruction::new("trace_step", params);
            state = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();
        }

        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        // Path reconstruction: traverse records in order to reconstruct the execution path
        assert_eq!(chain.records.len(), 3);
        let reconstructed: Vec<&str> = chain
            .records
            .iter()
            .map(|r| r.change_summary.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(reconstructed, vec!["increment", "set_value", "trace"]);

        // Verify the chain structure: each record's previous_hash points to the previous record
        for i in 1..chain.records.len() {
            assert_eq!(
                chain.records[i].previous_hash,
                chain.records[i - 1].hash,
                "Chain link broken at position {}",
                i
            );
        }

        // Verify logical ticks are strictly increasing
        for i in 1..chain.records.len() {
            assert!(
                chain.records[i].timestamp > chain.records[i - 1].timestamp,
                "Logical ticks must be strictly increasing"
            );
        }
    }

    /// C3-03: Nested tracking — the audit chain must support tracking nested
    /// while_loop execution.
    #[test]
    fn test_c3_nested_tracking() {
        use crate::instruction::registry::InstructionRegistry;
        use crate::primitive::audit_ops;
        use crate::rule::GenericInstruction;
        use crate::state::State;
        use std::collections::HashMap;

        let reg = InstructionRegistry::new();
        let mut state = State::new(vec![("x", Value::Integer(0))]);
        let mut ctx = ExecCtlCtx::new();

        // Simulate multi-round while_loop execution tracking
        for round in 0..3 {
            let mut params = HashMap::new();
            params.insert(
                "label".to_string(),
                Value::string(format!("while_loop_round_{}", round)),
            );
            params.insert(
                "rule_id".to_string(),
                Value::string(format!("while_loop.round_{}", round)),
            );
            let instr = GenericInstruction::new("trace_step", params);
            state = audit_ops::exec_trace_step(&reg, &state, &instr, &mut ctx).unwrap();

            // Each round also has dispatch tracking inside it
            let mut inner_params = HashMap::new();
            inner_params.insert(
                "label".to_string(),
                Value::string(format!("dispatch_case_round_{}", round)),
            );
            inner_params.insert(
                "rule_id".to_string(),
                Value::string(format!("dispatch.case.round_{}", round)),
            );
            let inner_instr = GenericInstruction::new("trace_step", inner_params);
            state = audit_ops::exec_trace_step(&reg, &state, &inner_instr, &mut ctx).unwrap();
        }

        let chain_val = state.get("__audit_chain").cloned().unwrap();
        let chain = AuditChainState::from_value(&chain_val).unwrap();

        // 6 records: 3 rounds × (while_loop + dispatch)
        assert_eq!(chain.records.len(), 6);

        // Verify the nested structure: each while_loop record is followed by a dispatch record
        for round in 0..3 {
            let while_idx = round * 2;
            let dispatch_idx = round * 2 + 1;
            assert!(
                chain.records[while_idx]
                    .change_summary
                    .as_ref()
                    .unwrap()
                    .contains(&format!("while_loop_round_{}", round)),
                "while_loop record missing for round {}",
                round
            );
            assert!(
                chain.records[dispatch_idx]
                    .change_summary
                    .as_ref()
                    .unwrap()
                    .contains(&format!("dispatch_case_round_{}", round)),
                "dispatch record missing for round {}",
                round
            );
        }

        // Full chain verification
        for record in &chain.records {
            assert!(
                record.verify(DEFAULT_HMAC_KEY),
                "Record verification failed: {}",
                record.id
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // PoC test: Verify HMAC integrity fix (execution_result / change_summary /
    //          error_message are now included in the signature)
    //
    // After the fix, tampering with these fields should cause verify() to return false.
    // ═══════════════════════════════════════════════════════════════════

    /// Create a baseline record for tampering tests.
    fn make_baseline_record() -> AuditRecord {
        let prev_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let tick: u64 = 0;
        let id = AuditRecord::derive_id(prev_hash, tick);
        let nonce = AuditRecord::derive_nonce(prev_hash, tick, DEFAULT_HMAC_KEY);

        AuditRecord::new(
            &id,
            "test.rule",
            tick as i64,
            "state_before_hash",
            "state_after_hash",
            Some("original summary".to_string()),
            ExecutionResult::Success,
            None,
            prev_hash,
            &nonce,
            DEFAULT_HMAC_KEY,
        )
    }

    #[test]
    fn poc_tamper_execution_result_still_verifies() {
        // Verification fix: tampering with execution_result should cause verify() to return false
        let mut rec = make_baseline_record();
        assert!(
            rec.verify(DEFAULT_HMAC_KEY),
            "Baseline record must verify successfully"
        );

        // Attack: change Success to Failure, leaving the hash field unchanged
        rec.execution_result = ExecutionResult::Failure;

        assert!(
            !rec.verify(DEFAULT_HMAC_KEY),
            "After fix: tampering with execution_result must cause verify() to return false"
        );
    }

    #[test]
    fn poc_tamper_change_summary_still_verifies() {
        // Verification fix: tampering with change_summary should cause verify() to return false
        let mut rec = make_baseline_record();
        assert!(rec.verify(DEFAULT_HMAC_KEY));

        // Attack: change the summary content
        rec.change_summary = Some("user never logged in".to_string());

        assert!(
            !rec.verify(DEFAULT_HMAC_KEY),
            "After fix: tampering with change_summary must cause verify() to return false"
        );
    }

    #[test]
    fn poc_tamper_error_message_still_verifies() {
        // Verification fix: tampering with error_message should cause verify() to return false
        let mut rec = make_baseline_record();
        assert!(rec.verify(DEFAULT_HMAC_KEY));

        // Attack: inject a fabricated error message
        rec.error_message = Some("fabricated runtime error".to_string());

        assert!(
            !rec.verify(DEFAULT_HMAC_KEY),
            "After fix: tampering with error_message must cause verify() to return false"
        );
    }

    #[test]
    fn poc_tamper_signatured_field_breaks_verify() {
        // Control group: tampering with a field already included in the signature
        // (rule_id) should cause verify() to fail — proving the signature mechanism
        // itself works, only the field set was incomplete.
        let mut rec = make_baseline_record();
        assert!(rec.verify(DEFAULT_HMAC_KEY));

        rec.rule_id = "tampered.rule".to_string();

        assert!(
            !rec.verify(DEFAULT_HMAC_KEY),
            "Control: tampering with rule_id must cause verify() to fail"
        );
    }

    // ══════════════════════════════════════════════
    // BUG-03 regression test: version field is now included in HMAC signature
    // ══════════════════════════════════════════════
    // Original issue: compute_hash() signed 11 fields, but version was not included.
    // Consequence: attacker could tamper with record.version without breaking the hash chain.
    // Fix: compute_hash() adds version parameter, new() and verify() updated accordingly.
    // After fix: tampering with version should cause verify() to return false.
    #[test]
    fn poc_tamper_version_still_verifies() {
        let mut rec = make_baseline_record();
        assert!(rec.verify(DEFAULT_HMAC_KEY));

        // Tamper version: change from 3 to 99
        assert_eq!(rec.version, 3, "Baseline record should be version 3");
        rec.version = 99;

        // Expectation: verify() should return false after version tampering (version is signed)
        assert!(
            !rec.verify(DEFAULT_HMAC_KEY),
            "BUG-03 regression: tampering with version must cause verify() to return false, \
             but it returned true — version field was not included in HMAC signature"
        );
    }
}
