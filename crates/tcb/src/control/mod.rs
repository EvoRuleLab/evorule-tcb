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

//! Control flow layer (Layer 1) — module entry point.
//!
//! # Module Structure
//!
//! This module aggregates the two control flow primitives:
//! - `dispatch`: Instruction dispatch and path reference resolution.
//! - `while_loop`: Self-driven execution loop.
//!
//! Both primitives are registered via `register_all()` and exposed via
//! `all_exec_fns()` and `all_explainers()` for introspection.
//!
//! # Determinism Guarantee
//!
//! This module itself is **L1 deterministic** — it is a pure aggregation
//! of deterministic primitives. The registration and mapping functions have
//! no runtime behavior; they are compile-time data structures.
//!
//! # Determinism Boundary (L2-L4)
//!
//! | Aspect | Status | Condition |
//! |--------|--------|-----------|
//! | `register_all()` | ✅ L1 deterministic | Pure registration |
//! | `all_exec_fns()` | ✅ L1 deterministic | Static mapping table |
//! | `exec_fn_map()` | ✅ L1 deterministic | `HashMap` construction |
//! | `all_explainers()` | ✅ L1 deterministic | Static explainer table |
//! | `explainer_map()` | ✅ L1 deterministic | `HashMap` construction |
//! | `dispatch` primitive | ✅ L1 deterministic | See `dispatch.rs` |
//! | `while_loop` primitive | ✅ L1 deterministic | See `while_loop.rs` |
//!
//! # Compositional Determinism
//!
//! The determinism of `dispatch` and `while_loop` primitives depends on the
//! determinism of the instructions they invoke. Both primitives are L1
//! deterministic themselves, but their **compositions** are conditionally
//! deterministic based on the registered instruction set.
//!
//! See `dispatch.rs` and `while_loop.rs` for detailed compositional
//! determinism notes.
//!
//! # Cross-Language Note (L4)
//!
//! This module is a Rust-only construct; there is no cross-language equivalent.
//! The primitives it aggregates are defined entirely within the TCB.

pub mod dispatch;
pub mod while_loop;

use crate::instruction::registry::InstructionRegistry;
use crate::instruction::ExecutorFn;
use std::collections::HashMap;

/// Register all control flow primitives.
pub fn register_all(reg: &mut InstructionRegistry) {
    dispatch::register(reg);
    while_loop::register(reg);
}

/// Static mapping table: control flow primitive name → executor function.
pub fn all_exec_fns() -> Vec<(&'static str, ExecutorFn)> {
    vec![
        ("dispatch", dispatch::exec_dispatch),
        ("while_loop", while_loop::exec_while_loop),
    ]
}

/// Build a `HashMap` from the executor function mapping table.
pub fn exec_fn_map() -> HashMap<&'static str, ExecutorFn> {
    all_exec_fns().into_iter().collect()
}

/// Static mapping table: control flow primitive name → explainer function.
///
/// Each explainer returns a human-readable description of the instruction's behavior,
/// including key parameters. This fulfills the C4 constitutional self-explanation
/// commitment: every TCB primitive must be able to describe its own purpose.
pub type ExplainerFn = fn(&crate::rule::GenericInstruction) -> String;

pub fn all_explainers() -> Vec<(&'static str, ExplainerFn)> {
    vec![
        ("dispatch", |instr| {
            let cases = instr
                .params
                .get("cases")
                .and_then(|v| v.as_list())
                .map_or(0, im::Vector::len);
            format!("Dispatch instruction: match {cases} cases + default")
        }),
        ("while_loop", |instr| {
            // Provide a meaningful description of the loop condition
            let cond_desc = instr
                .params
                .get("condition")
                .map(|v| {
                    // If it's a $ref, show the referenced path
                    if let Some(m) = v.as_object() {
                        if let Some(path) = m.get("$ref").and_then(|v| v.as_str()) {
                            return format!("$ref:{path}");
                        }
                        if let Some(domain_type) = m.get("type").and_then(|v| v.as_str()) {
                            return format!("domain:{domain_type}");
                        }
                    }
                    // If it's a simple string, show it directly
                    if let Some(s) = v.as_str() {
                        return s.to_string();
                    }
                    // Fallback: display the value in a safe, deterministic way
                    format!("{v:?}")
                })
                .unwrap_or_else(|| "default:__running".to_string());
            format!("Self-driven loop: execute while condition '{cond_desc}' is satisfied")
        }),
    ]
}

/// Build a `HashMap` from the explainer mapping table.
pub fn explainer_map() -> HashMap<&'static str, fn(&crate::rule::GenericInstruction) -> String> {
    all_explainers().into_iter().collect()
}
