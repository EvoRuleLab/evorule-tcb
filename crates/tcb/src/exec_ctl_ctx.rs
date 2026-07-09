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

//! Execution control context — carries runtime control state that was
//! previously hidden in `Cell<usize>` inside `InstructionRegistry`.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! # Design Principles
//!
//! - **Pure data**: All fields are plain data, no interior mutability.
//! - **Explicit passing**: Passed as `&mut` parameter to all execution functions.
//! - **Deterministic**: Same (State, Instruction, ExecCtlCtx) → same output.
//! - **Bounded**: All fields have explicit upper bounds.
//!
//! # Determinism Guarantee
//!
//! `ExecCtlCtx` is **L1 deterministic**:
//! - `depth` and `tick` are simple integer counters
//! - No randomness, no wall-clock, no side effects
//! - Same initial `ExecCtlCtx` + same inputs → same final `ExecCtlCtx`
//!
//! # Relationship with ExecContext
//!
//! `ExecContext` (existing, in `exec_context.rs`) wraps the `__exec__` business
//! state (instruction, queue, dispatch_cases, etc.) — it is part of `State`.
//!
//! `ExecCtlCtx` (this module) wraps execution control state (depth, tick) — it
//! is NOT part of `State`, passed as a separate parameter.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `ExecCtlCtx` data model | ✅ L1 deterministic | Pure data |
//! | `enter()` / `exit()` | ✅ L1 deterministic | Integer increment/decrement |
//! | `next_tick()` | ✅ L1 deterministic | u64 increment |
//! | `is_budget_exhausted()` | ✅ L1 deterministic | Integer comparison |
//!
//! # Formal Verification Notes
//!
//! This structure was introduced as part of the TCB formal verification refactoring
//! (Phase 1: pure function transformation). It replaces the `Cell<usize>` interior
//! mutability pattern, making all execution functions true pure functions:
//!
//! Before (non-verifiable):
//! ```ignore
//! registry.execute(&self, state, instr)  // self.depth is Cell<usize>
//! ```
//!
//! After (verifiable):
//! ```ignore
//! registry.execute(&self, state, instr, &mut ctx)  // ctx is explicit parameter
//! ```

use crate::error::EvoRuleError;

/// Default maximum execution depth.
///
/// Increased from the original 64 to 256 to support deeper rule chains
/// (e.g., forward chaining with 500+ rules requires more headroom).
pub const DEFAULT_MAX_DEPTH: usize = 256;

/// Default maximum total instructions (safety valve against runaway execution).
///
/// 1,000,000 is sufficient for any reasonable workload while preventing
/// infinite loops that slip past `max_depth` (e.g., wide rather than deep).
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Execution control context — carries runtime control state.
///
/// This structure replaces the `Cell<usize>` interior mutability pattern
/// in `InstructionRegistry`. All execution functions receive it as an
/// explicit `&mut` parameter, making them true pure functions.
///
/// # Fields
///
/// - `depth`: Current recursion depth (incremented on each `execute()` call)
/// - `max_depth`: Maximum allowed depth (returns error when exceeded)
/// - `tick`: Logical clock for audit chain (monotonically increasing)
/// - `instruction_count`: Total instructions executed (for budget enforcement)
/// - `max_instructions`: Maximum total instructions allowed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecCtlCtx {
    /// Current execution depth (for recursion protection).
    ///
    /// Incremented on each `execute()` call, decremented on return.
    /// Must never exceed `max_depth`.
    depth: usize,

    /// Maximum execution depth (default 256, configurable).
    ///
    /// When `depth >= max_depth`, `enter()` returns `DepthLimitExceeded`.
    max_depth: usize,

    /// Logical clock tick (for audit chain).
    ///
    /// Monotonically increasing. Used by `trace_step` to generate
    /// deterministic audit record IDs.
    tick: u64,

    /// Total instructions executed so far (for statistics and bounding).
    ///
    /// Incremented on each `execute()` call. Used to enforce
    /// a global execution budget.
    instruction_count: u64,

    /// Maximum total instructions allowed (safety valve).
    ///
    /// When `instruction_count >= max_instructions`, `enter()` returns error.
    max_instructions: u64,
}

impl ExecCtlCtx {
    /// Create a new execution control context with default limits.
    ///
    /// Defaults:
    /// - `max_depth`: 256
    /// - `max_instructions`: 1,000,000
    /// - `tick`: 0
    /// - `depth`: 0
    /// - `instruction_count`: 0
    #[must_use]
    pub fn new() -> Self {
        Self {
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
            tick: 0,
            instruction_count: 0,
            max_instructions: DEFAULT_MAX_INSTRUCTIONS,
        }
    }

    /// Create a context with a custom max_depth.
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Create a context with a custom max_instructions.
    #[must_use]
    pub fn with_max_instructions(mut self, max_instructions: u64) -> Self {
        self.max_instructions = max_instructions;
        self
    }

    /// Create a context with an initial tick value (for resuming execution).
    #[must_use]
    pub fn with_initial_tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }

    // ══════════════════════════════════════════
    // Depth management
    // ══════════════════════════════════════════

    /// Enter a new execution level (increment depth).
    ///
    /// Returns `Err(DepthLimitExceeded)` if `depth >= max_depth`.
    /// Returns `Err(InstructionLimitExceeded)` if budget is exhausted.
    /// This is called at the start of `registry.execute()`.
    ///
    /// # Errors
    /// - `DepthLimitExceeded`: depth would exceed max_depth
    /// - `InstructionLimitExceeded`: instruction_count would exceed max_instructions
    #[inline]
    pub fn enter(&mut self) -> Result<(), EvoRuleError> {
        if self.depth >= self.max_depth {
            return Err(EvoRuleError::DepthLimitExceeded {
                current_depth: self.depth,
                max_depth: self.max_depth,
            });
        }
        // Check instruction budget BEFORE incrementing (off-by-one safety)
        if self.instruction_count >= self.max_instructions {
            return Err(EvoRuleError::InstructionLimitExceeded {
                count: self.instruction_count,
                max: self.max_instructions,
            });
        }
        self.depth += 1;
        self.instruction_count += 1;
        Ok(())
    }

    /// Exit the current execution level (decrement depth).
    ///
    /// Called after `registry.execute()` completes (success or error).
    /// Uses `saturating_sub` to prevent underflow (defensive programming).
    #[inline]
    pub fn exit(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Get the current depth.
    #[inline]
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Get the max depth.
    #[inline]
    #[must_use]
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Set the max depth (for runtime configuration).
    ///
    /// Used by `InstructionRegistry::set_max_depth()` compatibility wrapper.
    pub fn set_max_depth(&mut self, max_depth: usize) {
        self.max_depth = max_depth;
    }

    // ══════════════════════════════════════════
    // Tick management
    // ══════════════════════════════════════════

    /// Get the next logical tick and increment the counter.
    ///
    /// Used by `trace_step` to generate deterministic audit record IDs.
    /// Returns the current tick value, then increments.
    #[inline]
    pub fn next_tick(&mut self) -> u64 {
        let current = self.tick;
        self.tick += 1;
        current
    }

    /// Get the current tick (without incrementing).
    #[inline]
    #[must_use]
    pub const fn current_tick(&self) -> u64 {
        self.tick
    }

    /// Peek at what the next tick would be (without incrementing).
    #[inline]
    #[must_use]
    pub const fn peek_next_tick(&self) -> u64 {
        self.tick
    }

    // ══════════════════════════════════════════
    // Statistics
    // ══════════════════════════════════════════

    /// Get the total instructions executed.
    #[inline]
    #[must_use]
    pub const fn instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Get the max instructions budget.
    #[inline]
    #[must_use]
    pub const fn max_instructions(&self) -> u64 {
        self.max_instructions
    }

    /// Check if the instruction budget is exhausted.
    #[inline]
    #[must_use]
    pub const fn is_budget_exhausted(&self) -> bool {
        self.instruction_count >= self.max_instructions
    }

    // ══════════════════════════════════════════
    // Reset
    // ══════════════════════════════════════════

    /// Reset the context to initial state (for reuse).
    ///
    /// Resets depth, tick, and instruction_count to 0.
    /// Keeps max_depth and max_instructions settings.
    pub fn reset(&mut self) {
        self.depth = 0;
        self.tick = 0;
        self.instruction_count = 0;
    }

    /// Reset only the depth (for testing and compatibility).
    ///
    /// Replaces `InstructionRegistry::reset_depth()`.
    pub fn reset_depth(&mut self) {
        self.depth = 0;
    }

    /// Reset only the tick (for testing).
    pub fn reset_tick(&mut self) {
        self.tick = 0;
    }
}

impl Default for ExecCtlCtx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_values() {
        let ctx = ExecCtlCtx::new();
        assert_eq!(ctx.depth(), 0);
        assert_eq!(ctx.max_depth(), DEFAULT_MAX_DEPTH);
        assert_eq!(ctx.current_tick(), 0);
        assert_eq!(ctx.instruction_count(), 0);
        assert_eq!(ctx.max_instructions(), DEFAULT_MAX_INSTRUCTIONS);
        assert!(!ctx.is_budget_exhausted());
    }

    #[test]
    fn test_enter_exit() {
        let mut ctx = ExecCtlCtx::new();
        assert_eq!(ctx.depth(), 0);

        assert!(ctx.enter().is_ok());
        assert_eq!(ctx.depth(), 1);
        assert_eq!(ctx.instruction_count(), 1);

        assert!(ctx.enter().is_ok());
        assert_eq!(ctx.depth(), 2);
        assert_eq!(ctx.instruction_count(), 2);

        ctx.exit();
        assert_eq!(ctx.depth(), 1);

        ctx.exit();
        assert_eq!(ctx.depth(), 0);

        // Exit below 0 is clamped (saturating_sub)
        ctx.exit();
        assert_eq!(ctx.depth(), 0);
    }

    #[test]
    fn test_depth_limit_exceeded() {
        let mut ctx = ExecCtlCtx::new().with_max_depth(3);

        assert!(ctx.enter().is_ok()); // depth 1
        assert!(ctx.enter().is_ok()); // depth 2
        assert!(ctx.enter().is_ok()); // depth 3

        // depth >= max_depth (3 >= 3), should fail
        let result = ctx.enter();
        assert!(result.is_err());
        match result.unwrap_err() {
            EvoRuleError::DepthLimitExceeded {
                current_depth,
                max_depth,
            } => {
                assert_eq!(current_depth, 3);
                assert_eq!(max_depth, 3);
            }
            _ => panic!("Expected DepthLimitExceeded"),
        }
    }

    #[test]
    fn test_instruction_limit_exceeded() {
        let mut ctx = ExecCtlCtx::new().with_max_instructions(3);

        assert!(ctx.enter().is_ok()); // count 1
        assert!(ctx.enter().is_ok()); // count 2
        assert!(ctx.enter().is_ok()); // count 3

        // count >= max (3 >= 3), should fail
        let result = ctx.enter();
        assert!(result.is_err());
        match result.unwrap_err() {
            EvoRuleError::InstructionLimitExceeded { count, max } => {
                assert_eq!(count, 3);
                assert_eq!(max, 3);
            }
            _ => panic!("Expected InstructionLimitExceeded"),
        }
    }

    #[test]
    fn test_tick_management() {
        let mut ctx = ExecCtlCtx::new();

        assert_eq!(ctx.current_tick(), 0);
        assert_eq!(ctx.next_tick(), 0);
        assert_eq!(ctx.current_tick(), 1);
        assert_eq!(ctx.next_tick(), 1);
        assert_eq!(ctx.current_tick(), 2);
        assert_eq!(ctx.peek_next_tick(), 2);
        assert_eq!(ctx.current_tick(), 2);
    }

    #[test]
    fn test_with_initial_tick() {
        let ctx = ExecCtlCtx::new().with_initial_tick(100);
        assert_eq!(ctx.current_tick(), 100);
        assert_eq!(ctx.peek_next_tick(), 100);
    }

    #[test]
    fn test_reset() {
        let mut ctx = ExecCtlCtx::new();
        ctx.enter().unwrap();
        ctx.enter().unwrap();
        ctx.next_tick();
        ctx.next_tick();

        assert_eq!(ctx.depth(), 2);
        assert_eq!(ctx.current_tick(), 2);
        assert_eq!(ctx.instruction_count(), 2);

        ctx.reset();

        assert_eq!(ctx.depth(), 0);
        assert_eq!(ctx.current_tick(), 0);
        assert_eq!(ctx.instruction_count(), 0);
        // max_depth and max_instructions are preserved
        assert_eq!(ctx.max_depth(), DEFAULT_MAX_DEPTH);
        assert_eq!(ctx.max_instructions(), DEFAULT_MAX_INSTRUCTIONS);
    }

    #[test]
    fn test_reset_depth() {
        let mut ctx = ExecCtlCtx::new();
        ctx.enter().unwrap();
        ctx.enter().unwrap();
        ctx.next_tick();

        ctx.reset_depth();

        assert_eq!(ctx.depth(), 0);
        // tick is not reset by reset_depth
        assert_eq!(ctx.current_tick(), 1);
    }

    #[test]
    fn test_set_max_depth() {
        let mut ctx = ExecCtlCtx::new();
        assert_eq!(ctx.max_depth(), DEFAULT_MAX_DEPTH);

        ctx.set_max_depth(512);
        assert_eq!(ctx.max_depth(), 512);

        // Now can go deeper
        for _ in 0..300 {
            assert!(ctx.enter().is_ok());
        }
        assert_eq!(ctx.depth(), 300);
    }

    #[test]
    fn test_budget_exhausted() {
        let mut ctx = ExecCtlCtx::new().with_max_instructions(5);
        assert!(!ctx.is_budget_exhausted());

        for _ in 0..5 {
            assert!(ctx.enter().is_ok());
        }
        assert!(ctx.is_budget_exhausted());

        // Next enter should fail
        assert!(ctx.enter().is_err());
    }

    #[test]
    fn test_clone_preserves_state() {
        let mut ctx = ExecCtlCtx::new().with_max_depth(100);
        ctx.enter().unwrap();
        ctx.next_tick();

        let cloned = ctx.clone();
        assert_eq!(cloned.depth(), 1);
        assert_eq!(cloned.max_depth(), 100);
        assert_eq!(cloned.current_tick(), 1);
        assert_eq!(cloned.instruction_count(), 1);
    }

    #[test]
    fn test_equality() {
        let ctx1 = ExecCtlCtx::new().with_max_depth(64);
        let ctx2 = ExecCtlCtx::new().with_max_depth(64);
        assert_eq!(ctx1, ctx2);

        let ctx3 = ExecCtlCtx::new().with_max_depth(128);
        assert_ne!(ctx1, ctx3);
    }

    #[test]
    fn test_default_impl() {
        let ctx = ExecCtlCtx::default();
        let ctx2 = ExecCtlCtx::new();
        assert_eq!(ctx, ctx2);
    }
}
