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

//! EvoRule TCB (Trusted Computing Base) — Deterministic physical execution layer.
//!
//! Target audience: AI/LLM systems (primary) and human developers (secondary).
//!
//! This is the low-level execution pipeline for EvoRule. All code is deterministic —
//! the same input always produces the same output. The TCB does NOT participate in
//! rule evaluation, constitutional judgment, or meta-rule triggering.
//! Modules are organized bottom-up by dependency (lower layers never import upper layers).
//!
//! Dependencies: None (base library)
//!
//! ┌──────────────────┐
//! │    Layer 0       │  value, error
//! │    Layer 1       │  exec_context, exec_ctl_ctx
//! │    Layer 2       │  state, domain, deterministic
//! │    Layer 3       │  rule
//! │    Layer 4       │  audit
//! │    Layer 5       │  instruction (registry, dispatch, meta, ops, primitives)
//! └──────────────────┘
//!
//! Constitutional Redlines (ER-600 series):
//!   - Do NOT add non-deterministic operations to the TCB
//!   - Do NOT add LambdaDomain / Callable transform
//!   - Do NOT import governance or any external crates
//!   - Do NOT use unsafe code

// Phase 3 lints configuration (2026-06-28):
// Allow unwrap/expect in test code (testing idiom, panic == failure)
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// ER-603 constitutional red line: TCB forbids unsafe code unconditionally.
// Enforced by CI G-10 (paradigm-gate.yml + ci.yml).
#![forbid(unsafe_code)]
// Pedantic/nursery lints: suppress pure-noise categories that don't affect
// correctness or maintainability. Categories that DO add value (redundant_clone,
// use_self, let_else, etc.) are left enabled and fixed individually.
#![allow(
    // Doc backticks around CamelCase identifiers: stylistic, not semantic
    clippy::doc_markdown,
    // format!("x={}", x) vs format!("x={x}"): stylistic, pre-2021 edition idiom
    clippy::uninlined_format_args,
    // Integer cast warnings: TCB uses deliberate casts (e.g., tick as i64)
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    // Missing const fn: not all functions can/should be const
    clippy::missing_const_for_fn,
    // Redundant else after return: stylistic
    clippy::redundant_else,
    // Function too long: sometimes long is fine (e.g., test setup)
    clippy::too_many_lines,
    // More than 3 bools in struct: design choice
    clippy::struct_excessive_bools,
    // Option<Option<T>>: design choice for optional nullable fields
    clippy::option_option,
    // Return Self without must_use: design choice
    clippy::return_self_not_must_use,
    // Unnecessary Result wrapper: some APIs return Result for future-proofing
    clippy::unnecessary_wraps,
    // Unnecessary structure name repetition (use Self): would be valuable but
    // too many false positives in enum Display impls
    clippy::use_self,
    // Single char pattern: marginal perf, hurts readability
    clippy::single_char_pattern,
    // Map unwrap_or: often clearer than map_or_else for simple cases
    clippy::map_unwrap_or,
    // Items after statements: sometimes needed for readability
    clippy::items_after_statements,
    // Ref option: &Option<T> vs Option<&T> — API design choice
    clippy::ref_option_ref,
    // Intra-doc link with quotes: false positive in code blocks
    clippy::doc_link_with_quotes,
    // Unwrap_or with function call: common test pattern, negligible cost
    clippy::unwrap_or_default,
    // or_fun_call: unwrap_or(function_call()) — common in tests, negligible cost
    clippy::or_fun_call,
    // Float comparison: TCB uses deliberate exact float comparisons in tests
    clippy::float_cmp,
    // Manual let else: refactoring to let...else hurts readability in some cases
    clippy::manual_let_else,
    // Match same arms: sometimes arms are intentionally separate for clarity
    clippy::match_same_arms,
    // Redundant closure for method calls: false positives with generics
    clippy::redundant_closure_for_method_calls,
    // Unreadable literal: long numeric constants (e.g., LCG multipliers) are
    // sometimes more readable without underscores in math contexts
    clippy::unreadable_literal,
    // Cast lossless: i32→i64 as is fine, From is verbose for no safety gain
    clippy::cast_lossless,
    // Redundant clone: nursery lint, false positives when clone is for ownership
    clippy::redundant_clone,
    // Used underscore binding: false positive in destructuring patterns
    clippy::used_underscore_binding,
    // If not else: sometimes !x is clearer than if !x { } else { }
    clippy::if_not_else,
    // Option if let else: map_or/map_or_else is not always clearer than if let
    clippy::option_if_let_else,
    // Needless collect: sometimes collect is needed for ownership
    clippy::needless_collect,
    // Explicit iter loop: iter() is sometimes clearer than &container
    clippy::explicit_iter_loop,
    // Nonminimal bool: sometimes !a is clearer than a == false
    clippy::nonminimal_bool,
    // Derive PartialEq without Eq: Eq is not always safe (e.g., floats)
    clippy::derive_partial_eq_without_eq,
    // Single match else: if let is not always clearer
    clippy::single_match_else,
)]

// Layer 0 — Leaf nodes, no dependencies

// Crate-level allow for unknown lint names: clippy 1.75.0 (pinned toolchain) does
// not recognize every lint introduced in later clippy versions. Source files
// retain their per-module `#![allow(clippy::<newer-name>)]` lines for forward
// compatibility with newer toolchains; this blanket allows those names to be
// accepted as unknown by 1.75.0 instead of failing compilation.
#![allow(unknown_lints)]

pub mod error;
pub mod value;

// Layer 1 — Depends on value
pub mod exec_context;
pub mod exec_ctl_ctx;

// Layer 2 — Depends on exec_context + value
pub mod state;

// Layer 2 — Depends on state + value
pub mod domain;

// Layer 2 — Depends on value
pub mod deterministic;

// Layer 3 — Depends on domain + error + value
pub mod rule;

// Layer 4 — Depends on deterministic + value
pub mod audit;

// Layer 5 — Instruction execution system
pub mod instruction;

// Layer 0 (Physical primitives) — Depends on instruction::registry + state + value
pub mod primitive;

// Layer 1 (Control flow) — Depends on primitive + instruction::registry
pub mod control;

// Re-exports for convenient access
pub use domain::Domain;
pub use error::EvoRuleError;
pub use exec_ctl_ctx::ExecCtlCtx;
pub use state::State;
pub use value::Value;
