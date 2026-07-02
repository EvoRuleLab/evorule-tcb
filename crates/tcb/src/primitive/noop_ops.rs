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

//! No-op primitive — does nothing and returns directly.
//!
//! # Core Functions
//!
//! - `noop`: No operation — returns the input state unchanged.
//!
//! # Design Principles
//!
//! `noop` is the simplest TCB primitive. It serves as:
//! - A placeholder for conditional branches that require a do-nothing branch.
//! - The default instruction when the queue is empty (`advance_instruction` injects `noop`).
//! - A safe fallback for testing and debugging.
//!
//! # Determinism Guarantee
//!
//! `noop` is **L1 deterministic**:
//! - Same input state → same output state (a clone of the input).
//! - No randomness, wall-clock time, or side effects.
//! - Pure identity function.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `noop` execution | ✅ L1 deterministic | Identity function |
//! | State cloning | ✅ L1 deterministic | Deterministic clone |
//!
//! # Cross-Language Note (L4)
//!
//! `noop` is a Rust-only construct; there is no cross-language equivalent.
//! The concept of a no-op instruction is universal across languages.

use crate::error::EvoRuleError;
use crate::instruction::registry::InstructionRegistry;
use crate::rule::GenericInstruction;
use crate::state::State;

/// Register the no-op primitive.
pub fn register(reg: &mut InstructionRegistry) {
    reg.register("noop", exec_noop);
}

/// No-op — does nothing and returns directly.
///
/// # Behavior
/// - Returns a clone of the input state.
/// - Does not modify any state fields.
/// - Does not enqueue any instructions.
/// - Does not affect the execution queue or running flag.
///
/// # Use Cases
/// - Default branch for conditional instructions.
/// - Placeholder for `advance_instruction` when the queue is empty.
/// - No-op fallback in rule transforms.
pub(crate) fn exec_noop(
    _reg: &InstructionRegistry,
    state: &State,
    _instruction: &GenericInstruction,
) -> Result<State, EvoRuleError> {
    Ok(state.clone())
}

// ══════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Test noop: returns the original state unchanged
    #[test]
    fn test_noop_returns_state_unchanged() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![
            ("x", crate::value::Value::Integer(42)),
            ("y", crate::value::Value::String("hello".to_string())),
        ]);
        let instr = GenericInstruction::simple("noop");

        let result = exec_noop(&reg, &state, &instr).unwrap();

        assert_eq!(result.get("x"), Some(&crate::value::Value::Integer(42)));
        assert_eq!(
            result.get("y"),
            Some(&crate::value::Value::String("hello".to_string()))
        );
    }

    /// Test noop: works correctly with an empty state
    #[test]
    fn test_noop_empty_state() {
        let reg = InstructionRegistry::new();
        let state = State::empty();
        let instr = GenericInstruction::simple("noop");

        let result = exec_noop(&reg, &state, &instr).unwrap();

        // Empty state remains empty after noop
        assert!(result.data().is_empty());
    }

    /// Test noop: returns a clone (different instance from the original)
    #[test]
    fn test_noop_returns_clone() {
        let reg = InstructionRegistry::new();
        let state = State::new(vec![("x", crate::value::Value::Integer(1))]);
        let instr = GenericInstruction::simple("noop");

        let result = exec_noop(&reg, &state, &instr).unwrap();

        // Verify contents are the same
        assert_eq!(result.get("x"), state.get("x"));
        // Verify result and state are not the same object
        let result_modified = result.set("x", crate::value::Value::Integer(999));
        assert_eq!(state.get("x"), Some(&crate::value::Value::Integer(1))); // state unchanged
        assert_eq!(
            result_modified.get("x"),
            Some(&crate::value::Value::Integer(999))
        );
    }
}
