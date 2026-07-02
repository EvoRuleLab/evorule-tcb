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

//! EvoRule TCB error types.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! All TCB operations return this error type. TCB errors are considered fatal —
//! recovery is not possible within the TCB layer.
//!
//! Constitutional Redlines (ER-600 series):
//!   - This module contains ONLY pure computation errors.
//!   - NO I/O errors (e.g., file read/write, network) are permitted here.
//!   - NO policy or business logic errors are permitted here.
//!   - Rule loading errors belong to Governance (RuleLoader), NOT TCB.

use std::fmt;

/// EvoRule TCB error.
///
/// This is the system's low-level error type. All exec_ functions, state operations,
/// and domain matching return this error on failure. Errors are unrecoverable —
/// the system should halt immediately upon encountering any TCB error.
#[derive(Debug, Clone, PartialEq)]
pub enum EvoRuleError {
    /// Type mismatch.
    /// Expected one type but received another.
    TypeError { expected: String, found: String },

    /// Key not found.
    /// Attempted to access a nonexistent key in State or Value.
    KeyNotFound { key: String },

    /// Index out of bounds.
    IndexOutOfBounds { index: usize, length: usize },

    /// Empty rule ID.
    /// Attempted to execute a rule with an empty rule_id.
    EmptyRuleId,

    /// Invalid domain type.
    /// Domain::from_dict encountered an unknown `type` field value.
    InvalidDomainType { domain_type: String },

    /// Unknown instruction type.
    /// InstructionRegistry has no registered executor for this instruction type.
    UnknownInstruction { instruction_type: String },

    /// Execution depth limit exceeded.
    /// Instruction nesting depth exceeded max_depth (possible infinite loop).
    DepthLimitExceeded {
        current_depth: usize,
        max_depth: usize,
    },

    /// Rule not found in the in-memory rule set.
    /// The rule ID does not exist in the currently loaded Universe.
    RuleNotFound { rule_id: String },

    /// Missing required parameter.
    /// An instruction or rule is missing a required parameter.
    MissingParam { context: String, param: String },

    /// Invalid parameter.
    /// A parameter exists but its type or value is invalid.
    InvalidParam {
        context: String,
        param: String,
        detail: String,
    },

    /// Execution failed.
    /// General-purpose error during instruction execution (fallback).
    ExecutionFailed {
        instruction_type: String,
        detail: String,
    },

    /// Domain mismatch.
    DomainMismatch { domain_type: String, detail: String },

    /// JSON parse error.
    /// Occurs during deterministic deserialization of in-memory data.
    JsonParseError { detail: String },

    /// Serialization error.
    /// Occurs during deterministic serialization of in-memory data.
    SerializationError { detail: String },

    /// Hash error.
    HashError { detail: String },

    /// Audit error.
    /// Audit chain integrity check failed.
    AuditError { detail: String },

    /// Immutability violation.
    /// Attempted to modify an immutable State.
    ImmutabilityViolation { field: String },

    /// Invalid in-memory computation configuration.
    /// Covers internal inconsistencies (e.g., max_depth vs. registry capacity).
    /// Does NOT cover external file/config parsing errors — those belong to Governance.
    InvalidConfig { detail: String },

    /// Generic error.
    Generic(String),
}

impl fmt::Display for EvoRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvoRuleError::TypeError { expected, found } => {
                write!(f, "Type error: expected {}, found {}", expected, found)
            }
            EvoRuleError::KeyNotFound { key } => {
                write!(f, "Key not found: {}", key)
            }
            EvoRuleError::IndexOutOfBounds { index, length } => {
                write!(f, "Index out of bounds: index {}, length {}", index, length)
            }
            EvoRuleError::EmptyRuleId => {
                write!(f, "Rule ID is empty")
            }
            EvoRuleError::InvalidDomainType { domain_type } => {
                write!(f, "Invalid domain type: {}", domain_type)
            }
            EvoRuleError::UnknownInstruction { instruction_type } => {
                write!(f, "Unknown instruction type: {}", instruction_type)
            }
            EvoRuleError::DepthLimitExceeded {
                current_depth,
                max_depth,
            } => {
                write!(
                    f,
                    "Execution depth limit exceeded: current {}, max {} (possible infinite loop)",
                    current_depth, max_depth
                )
            }
            EvoRuleError::RuleNotFound { rule_id } => {
                write!(f, "Rule not found: {}", rule_id)
            }
            EvoRuleError::MissingParam { context, param } => {
                write!(f, "Missing required parameter [{}]: {}", context, param)
            }
            EvoRuleError::InvalidParam {
                context,
                param,
                detail,
            } => {
                write!(f, "Invalid parameter [{}]: {} — {}", context, param, detail)
            }
            EvoRuleError::ExecutionFailed {
                instruction_type,
                detail,
            } => {
                write!(f, "Execution failed [{}]: {}", instruction_type, detail)
            }
            EvoRuleError::DomainMismatch {
                domain_type,
                detail,
            } => {
                write!(f, "Domain mismatch [{}]: {}", domain_type, detail)
            }
            EvoRuleError::JsonParseError { detail } => {
                write!(f, "JSON parse error: {}", detail)
            }
            EvoRuleError::SerializationError { detail } => {
                write!(f, "Serialization error: {}", detail)
            }
            EvoRuleError::HashError { detail } => {
                write!(f, "Hash error: {}", detail)
            }
            EvoRuleError::AuditError { detail } => {
                write!(f, "Audit error: {}", detail)
            }
            EvoRuleError::ImmutabilityViolation { field } => {
                write!(
                    f,
                    "Immutability violation: attempted to modify field '{}'",
                    field
                )
            }
            EvoRuleError::InvalidConfig { detail } => {
                write!(f, "Invalid configuration: {}", detail)
            }
            EvoRuleError::Generic(msg) => {
                write!(f, "Error: {}", msg)
            }
        }
    }
}

impl std::error::Error for EvoRuleError {}

/// Convenience function for creating a TypeError.
pub fn type_error(expected: &str, found: &str) -> EvoRuleError {
    EvoRuleError::TypeError {
        expected: expected.to_string(),
        found: found.to_string(),
    }
}

/// Convenience function for creating a KeyNotFound error.
pub fn key_not_found(key: &str) -> EvoRuleError {
    EvoRuleError::KeyNotFound {
        key: key.to_string(),
    }
}

/// Convenience function for creating an ExecutionFailed error (fallback).
pub fn exec_error(instruction_type: &str, detail: impl Into<String>) -> EvoRuleError {
    EvoRuleError::ExecutionFailed {
        instruction_type: instruction_type.to_string(),
        detail: detail.into(),
    }
}

/// Convenience function for creating an EmptyRuleId error.
pub fn empty_rule_id() -> EvoRuleError {
    EvoRuleError::EmptyRuleId
}

/// Convenience function for creating an InvalidDomainType error.
pub fn invalid_domain_type(domain_type: &str) -> EvoRuleError {
    EvoRuleError::InvalidDomainType {
        domain_type: domain_type.to_string(),
    }
}

/// Convenience function for creating an UnknownInstruction error.
pub fn unknown_instruction(instruction_type: &str) -> EvoRuleError {
    EvoRuleError::UnknownInstruction {
        instruction_type: instruction_type.to_string(),
    }
}

/// Convenience function for creating a DepthLimitExceeded error.
pub fn depth_limit_exceeded(current_depth: usize, max_depth: usize) -> EvoRuleError {
    EvoRuleError::DepthLimitExceeded {
        current_depth,
        max_depth,
    }
}

/// Convenience function for creating a RuleNotFound error.
pub fn rule_not_found(rule_id: &str) -> EvoRuleError {
    EvoRuleError::RuleNotFound {
        rule_id: rule_id.to_string(),
    }
}

/// Convenience function for creating a MissingParam error.
pub fn missing_param(context: &str, param: &str) -> EvoRuleError {
    EvoRuleError::MissingParam {
        context: context.to_string(),
        param: param.to_string(),
    }
}

/// Convenience function for creating an InvalidParam error.
pub fn invalid_param(context: &str, param: &str, detail: &str) -> EvoRuleError {
    EvoRuleError::InvalidParam {
        context: context.to_string(),
        param: param.to_string(),
        detail: detail.to_string(),
    }
}

/// Convenience function for creating an InvalidConfig error.
pub fn invalid_config(detail: impl Into<String>) -> EvoRuleError {
    EvoRuleError::InvalidConfig {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = type_error("integer", "string");
        let msg = format!("{}", err);
        assert!(msg.contains("integer"));
        assert!(msg.contains("string"));

        let err = key_not_found("x");
        assert_eq!(format!("{}", err), "Key not found: x");
    }

    #[test]
    fn test_error_clone() {
        let err = exec_error("test", "something failed");
        let cloned = err.clone();
        assert_eq!(format!("{}", err), format!("{}", cloned));
    }

    #[test]
    fn test_convenience_functions() {
        let err = key_not_found("missing");
        assert!(matches!(err, EvoRuleError::KeyNotFound { .. }));

        let err = exec_error("dispatch", "unknown type");
        assert!(matches!(err, EvoRuleError::ExecutionFailed { .. }));

        let err = empty_rule_id();
        assert!(matches!(err, EvoRuleError::EmptyRuleId));

        let err = invalid_domain_type("unknown_type");
        assert!(matches!(err, EvoRuleError::InvalidDomainType { .. }));

        let err = unknown_instruction("foo");
        assert!(matches!(err, EvoRuleError::UnknownInstruction { .. }));

        let err = depth_limit_exceeded(65, 64);
        assert!(matches!(err, EvoRuleError::DepthLimitExceeded { .. }));

        let err = rule_not_found("missing_rule");
        assert!(matches!(err, EvoRuleError::RuleNotFound { .. }));

        let err = missing_param("increment", "attr");
        assert!(matches!(err, EvoRuleError::MissingParam { .. }));

        let err = invalid_param("increment", "delta", "must be numeric");
        assert!(matches!(err, EvoRuleError::InvalidParam { .. }));
    }

    #[test]
    fn test_semantic_error_display() {
        assert!(format!("{}", empty_rule_id()).contains("Rule ID is empty"));
        assert!(format!("{}", unknown_instruction("foo")).contains("foo"));
        assert!(format!("{}", depth_limit_exceeded(100, 64)).contains("100"));
        assert!(format!("{}", rule_not_found("bar")).contains("bar"));
        assert!(format!("{}", missing_param("ctx", "p")).contains("p"));
        assert!(format!("{}", invalid_param("ctx", "p", "bad")).contains("bad"));
    }
}
