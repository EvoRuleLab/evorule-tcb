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

//! Instruction execution system — Registry.
//!
//! # Module Structure
//!
//! Physical primitives have been migrated to `primitive/`, control flow has been
//! migrated to `control/`. This module only retains infrastructure:
//! - `registry`: Instruction registry (with context operations, serialization)
//!
//! # Type Aliases
//!
//! - `ExecutorFn`: The common signature for all TCB primitives.
//! - `ContextOpFn`: The common signature for context operations (add/sub/mul/div/etc.).
//!
//! # Determinism Guarantee
//!
//! All type aliases in this module are **L1 deterministic** — they are compile-time
//! type definitions with no runtime behavior.

pub mod registry;

/// Executor function type signature.
///
/// All TCB primitives (physical primitives and control flow primitives) must
/// conform to this signature. This is the fundamental contract between the
/// registry and the primitive implementations.
///
/// # Parameters
/// - `&registry::InstructionRegistry`: The registry instance (for recursive execution)
/// - `&crate::state::State`: The immutable input state
/// - `&crate::rule::GenericInstruction`: The instruction to execute
/// - `&mut crate::exec_ctl_ctx::ExecCtlCtx`: Execution control context (depth, tick, budget)
///
/// # Returns
/// - `Ok(crate::state::State)`: The new state after execution
/// - `Err(crate::error::EvoRuleError)`: An execution error
///
/// # Migration Notes
///
/// This signature was updated as part of Phase 1 (pure function transformation)
/// to include `ExecCtlCtx`. The previous signature (without `ExecCtlCtx`) used
/// `Cell<usize>` for depth tracking, which broke purity. The new signature makes
/// all execution functions true pure functions.
pub type ExecutorFn = fn(
    &registry::InstructionRegistry,
    &crate::state::State,
    &crate::rule::GenericInstruction,
    &mut crate::exec_ctl_ctx::ExecCtlCtx,
) -> Result<crate::state::State, crate::error::EvoRuleError>;

/// Executor result type.
///
/// Convenience alias for `Result<T, EvoRuleError>` used throughout the execution
/// system.
pub type ExecutorResult<T> = Result<T, crate::error::EvoRuleError>;

/// Context operation function type.
///
/// Context operations (add, sub, mul, div, set, append, remove) are deterministic
/// operations that transform values. They are used by `set_context` to perform
/// arithmetic and list manipulations.
///
/// # Parameters
/// - `&crate::value::Value`: The left-hand operand
/// - `&crate::value::Value`: The right-hand operand
///
/// # Returns
/// - `crate::value::Value`: The result of the operation
pub type ContextOpFn = fn(&crate::value::Value, &crate::value::Value) -> crate::value::Value;
