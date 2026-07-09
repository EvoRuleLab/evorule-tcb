# EvoRule

**Rule-Driven, Deterministic Execution Engine**

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/badge/CI-passing-brightgreen.svg)](https://github.com/EvoRuleLab/evorule-tcb/actions)

EvoRule is a **rule-driven execution platform** where business logic lives entirely in JSON rules, not in code. Rust provides the **deterministic computational base** — the TCB (Trusted Computing Base) — while JSON rules define what to execute and when.

> **One sentence**: Same rules + same input = same output, forever.

---

## Architecture (v1.1, 2026-07-05)

EvoRule adopts the **Constitutional Dispatch Architecture**: the dispatch table is **dynamically built at startup** by governance-core's `DispatchTableBuilder` from the `InstructionRegistry`, eliminating JSON/Rust duplication while preserving L1 determinism. The constitutional `core_eval.json` shrinks from ~660 → ~30 lines; every physical primitive registration in Rust auto-projects to a cases entry — no JSON edit required. Each `trace_step` audit record carries a `[tbl:<8-char-hash>]` dispatch-table version, enabling audit-chain traceability across dispatch-table evolution. See [CHANGELOG](CHANGELOG.md) and [`docs/spec/en-US/TCB_Governance_Contract.md` §1 + §9](docs/spec/en-US/TCB_Governance_Contract.md) for the full governance contract.

---

## Why EvoRule?

Traditional rule engines embed execution logic in code, with JSON as mere configuration. EvoRule inverts this:

| Traditional Framework                    | EvoRule                                 |
| ---------------------------------------- | --------------------------------------- |
| Business logic in Rust/Java/Go           | Business logic in JSON rules            |
| JSON configures "when"                   | JSON defines "what" and "how"           |
| Changing behavior requires recompilation | Change JSON, reload, done               |
| Execution is opaque                      | Every step is auditable and traceable   |
| Non-determinism is tolerated             | Determinism is a constitutional promise |

EvoRule is built for domains where **determinism, auditability, and transparency** are non-negotiable — scientific computing, formal verification, compliance-critical workflows, and AI/LLM-driven rule systems.

---

## Core Principles

| Principle                | Meaning                                                                    |
| ------------------------ | -------------------------------------------------------------------------- |
| **Rule-Driven**          | Business logic lives in JSON rules, not in code.                           |
| **Deterministic L1**     | Same input → same output. No wall-clock time, no UUIDs, no random numbers. |
| **TCB Minimization**     | The Trusted Computing Base is small, auditable, and formally verifiable.   |
| **Auditable by Default** | Every execution step is recorded in an HMAC-chained audit trail.           |
| **Transparent**          | Execution flow is defined in JSON, visible to humans and AI alike.         |
| **JSON as Program**      | `core_eval.json` is not configuration — it's executable logic.             |

---

## Architecture

EvoRule is split into two cleanly separated layers:

> **Note**: This repository (`evorule-tcb`) contains **only the TCB layer**. The Governance layer lives in a separate repository: [`EvoRuleLab/evorule-governance`](https://github.com/EvoRuleLab/evorule-governance).

```
┌─────────────────────────────────────────────────────────────┐
│                    Governance Layer                        │
│  - Rule loading and management (RuleLoader)               │
│  - I/O channels (file, env, http, database)               │
│  - High-level execution engines (ForwardChain, etc.)      │
│  - Session management, audit export                       │
│  - Python bindings (evorule-py)                          │
├─────────────────────────────────────────────────────────────┤
│                    TCB Layer (Trusted Computing Base)      │
│  - 25 atomic primitives + 2 control flow tools             │
│  - State/Value/Domain data models                         │
│  - InstructionRegistry (dispatch center)                  │
│  - while_loop self-driving execution engine               │
│  - HMAC-chained audit chain primitives                    │
│  - SHA-256 content hash, LogicalClock, DeterministicRNG   │
│  - Zero unsafe, zero I/O, zero non-determinism            │
└─────────────────────────────────────────────────────────────┘
```

**Dependency direction**: Python bindings → Governance → TCB. The TCB has **zero dependencies** on Governance. It depends on 10 version-locked external crates (all within the ER-602 exception list): `serde`, `serde_json`, `im`, `ordered-float`, `sha2`, `hmac`, `hex`, `ryu`, `regex`, `log`.

---

## Repository Structure

```
evorule-tcb/                       # This repo (TCB only)
├── crates/
│   └── tcb/                           # Trusted Computing Base
│       ├── src/
│       │   ├── lib.rs                 # TCB entry point
│       │   ├── value.rs               # Unified Value type (deterministic)
│       │   ├── state.rs               # Immutable State container
│       │   ├── domain.rs              # Domain matching (conditional logic)
│       │   ├── rule.rs                # Rule + GenericInstruction
│       │   ├── exec_context.rs        # __exec__ typed accessor
│       │   ├── exec_ctl_ctx.rs        # Execution control context
│       │   ├── deterministic.rs       # content_hash, LogicalClock, RNG
│       │   ├── audit.rs               # HMAC-chained audit records
│       │   ├── error.rs               # TCB error types (pure computation)
│       │   ├── control/               # Control flow primitives
│       │   │   ├── mod.rs
│       │   │   ├── dispatch.rs        # O(1) instruction dispatch
│       │   │   └── while_loop.rs      # Self-driving execution loop
│       │   ├── instruction/           # Instruction registry
│       │   │   ├── mod.rs
│       │   │   └── registry.rs        # InstructionRegistry (dispatch center)
│       │   └── primitive/             # 25 atomic primitives + 2 control flow
│       │       ├── mod.rs
│       │       ├── state_ops.rs       # set_context, state_set, state_compute
│       │       ├── queue_ops.rs       # advance_instruction, push_instruction, push_instruction_sequence, instruction_sequence
│       │       ├── domain_ops.rs      # evaluate_domain, match_domain, domain_intersect
│       │       ├── rule_ops.rs        # apply_rule, observe_rules, filter_rules, inject_rule
│       │       ├── compute_ops.rs     # content_hash, format_string, get_index, object_keys, set_intersection, set_diff, set_union
│       │       ├── audit_ops.rs       # trace_step
│       │       ├── error_ops.rs       # execute_try_catch, execute_parallel
│       │       └── noop_ops.rs        # noop
│       ├── tests/                     # Integration verification tests
│       │   ├── v1_pure_function.rs
│       │   ├── v4_determinism.rs
│       │   ├── v6_iter_order.rs
│       │   ├── v7_termination.rs
│       │   ├── v8_recursion_bound.rs
│       │   └── v9_audit_chain.rs
│       └── Cargo.toml                 # Strictly limited dependencies
│
├── tools/
│   ├── paradigm-gate.sh               # Pre-commit hook runner (9 gates)
│   └── install-hooks.sh               # Install Git pre-commit hook
│
├── docs/
│   ├── spec/                          # Specification (authoritative)
│   │   └── en-US/
│   │       ├── EvoRule_programming_Spec.md
│   │       ├── EvoRule_Determinism_Standard.md
│   │       └── TCB_Governance_Contract.md
│   ├── gate/
│   │   ├── GATES.md                   # Admission gates (pre-commit + CI)
│   │   └── FORBIDDEN_OVERVIEW.md      # Unified forbidden rules overview
│   ├── developer-guide/
│   │   └── design-decisions/          # Architecture decision records
│   │       ├── 01-while-loop-self-driving.md
│   │       └── 02-TheEquation, core_eval.json, and while_loop.md
│   └── LLM_CONTEXT.md                 # LLM context injection document
│
├── .github/workflows/
│   ├── ci.yml                         # CI pipeline (3-OS matrix)
│   └── paradigm-gate.yml              # Paradigm gate workflow
└── README.md                          # This file
```

### Governance Layer (separate repo: EvoRuleLab/evorule-governance)

```
evorule-governance/                # Governance layer repo
├── crates/
│   ├── governance-core/               # Core governance engine
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── engine/               # TheEquation, dispatch table
│   │   │   │   ├── mod.rs
│   │   │   │   ├── aliases.rs / aliases.json
│   │   │   │   ├── core_eval.json    # Entry point — while_loop + dispatch
│   │   │   │   ├── eval_config.json  # primitives, termination, meta_types
│   │   │   │   ├── equation.rs       # TheEquation facade
│   │   │   │   └── dispatch_table.rs # Dispatch table builder
│   │   │   ├── io/                   # I/O channels (env, memory)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── env.rs
│   │   │   │   ├── memory.rs
│   │   │   │   └── primitives.rs
│   │   │   ├── rule_loader.rs        # JSON rule loading & validation
│   │   │   ├── rule_executor.rs      # Rule execution engine
│   │   │   ├── planning.rs           # Planning module
│   │   │   ├── universe.rs           # Universe (rule management)
│   │   │   ├── config_runtime.rs     # Runtime configuration
│   │   │   └── audit_export.rs       # Audit export utilities
│   │   ├── tests/                    # Integration tests
│   │   │   ├── aliases_json_loading.rs
│   │   │   ├── evaluate_end_to_end.rs
│   │   │   ├── l0_constitution_exec.rs
│   │   │   ├── l0_constitution_load.rs
│   │   │   ├── l1_core_load_and_exec.rs
│   │   │   ├── l2_domain_load_and_exec.rs
│   │   │   └── registry_freeze_invariant.rs
│   │   └── Cargo.toml
│   │
│   └── governance-inference/          # Inference module
│       ├── src/
│       │   ├── lib.rs
│       │   ├── cycle.rs              # Cycle detection
│       │   └── forward.rs            # Forward chaining
│       ├── tests/
│       │   └── forward_chain_integration.rs
│       └── Cargo.toml
│
├── rules/                            # All JSON rules
│   ├── manifest.json                 # Rule manifest (versioned)
│   ├── L0_constitution/             # Constitutional rules (immutable)
│   │   ├── builtins.json
│   │   ├── clauses.json
│   │   ├── constitution_config.json
│   │   └── orchestration.json
│   ├── L1_core/                      # Core evaluation rules
│   │   └── kernel.json
│   ├── L2_domain/                    # Domain-specific rules
│   │   ├── amendment.json
│   │   ├── inference.json
│   │   ├── meta_actions.json
│   │   ├── planning.json
│   │   └── universe.json
│   ├── L2_inference/                 # Inference rules
│   │   └── forward_chain.json
│   └── loader/                       # Loader utility rules
│       ├── filter_rules_by_flag.json
│       └── version_diff.json
│
├── tools/
│   ├── paradigm-gate.sh              # Governance paradigm gates
│   ├── validate-manifest.py          # Manifest validation
│   └── validate-primitives.py       # Primitive validation
│
└── docs/                             # Governance documentation
    ├── spec/
    │   └── en-US/
    │       ├── Manifest_Specification.md
    │       ├── Rule_Format_Specification.md
    │       └── layer_Boundary_Contract.md
    ├── architecture/
    │   ├── ai-agent-design.md
    │   └── rule-system-design.md
    ├── gate/
    │   ├── GATES.md
    │   └── FORBIDDEN_OVERVIEW.md
    ├── developer-guide/
    │   ├── rule-author-guide.md
    │   └── design-decisions/
    │       ├── 02-io-channel-layering.md
    │       ├── 03-v2-compat-deferred.md
    │       ├── 04-v4-rule-composite-migration.md
    │       └── 05-domain-intersect-primitive.md
    ├── schema/
    │   └── manifest-schema.json
    └── primitive-and-rule-creation-guide.md
```

---

## Key Concepts

### The Self-Driving `while_loop`

EvoRule's execution model is unique. The engine (`TheEquation`) only initializes `__exec__` and triggers one `while_loop`. After that, **JSON rules take over**:

```rust
evaluate(instruction, state):
    init __exec__ context            // engine only initializes
    reg.execute(core_eval)           // engine only triggers once
    // thereafter, JSON rules drive everything
```

`core_eval.json` defines `while_loop` with `body = [dispatch, trace_step]` — the loop condition (`__running`), dispatching logic, advancement, and termination are all defined in JSON.

> **Engine is "dumb"; rules are "smart".**

### Determinism at L1

EvoRule is **L1 deterministic** (Computational Determinism): same input → same output, guaranteed by algorithm, mathematical definition, or language specification.

| Non-Deterministic API  | EvoRule Alternative             |
| ---------------------- | ------------------------------- |
| `SystemTime::now()`    | `LogicalClock::current_tick()`  |
| `Uuid::new_v4()`       | `content_hash()`                |
| `rand::random()`       | `DeterministicRNG::from_seed()` |
| File system timestamps | `content_hash()` of content     |

### The Audit Chain

Every execution step is recorded in an **HMAC-SHA256-chained audit trail**:

- **Deterministic IDs**: `content_hash(prev_hash + logical_tick)`
- **Deterministic nonces**: `HMAC(key, prev_hash + logical_tick)`
- **Tamper-proof**: Modifying any record breaks the chain
- **Verifiable**: `record.verify(key)` validates integrity

---

## Getting Started

### Prerequisites

- Rust 1.75+
- Git

### Build

```bash
git clone https://github.com/EvoRuleLab/evorule-tcb.git
cd evorule-tcb
cargo build --release
```

### Run Tests

```bash
cargo test --all-targets
cargo test --doc
```

### Install Pre-commit Hook

```bash
./tools/install-hooks.sh
```

This installs `paradigm-gate.sh` as the Git pre-commit hook, enforcing all paradigm rules before every commit.

### Run Paradigm Gates Manually

```bash
./tools/paradigm-gate.sh
```

This checks:

- No `unsafe` in TCB
- No wall-clock time / UUID / RNG
- No deprecated types (`DimensionChecker`, etc.)
- No `$func`/`$eval` in JSON
- No forbidden transform types
- No Chinese characters in code (release blocker)

---

## Examples

A minimal example is included to demonstrate the core TCB API:

- `state_pipeline.rs` — runs a small `state_set` + `state_compute` pipeline and verifies that replaying it produces a byte-identical state JSON and a stable content hash.

```bash
cargo run --example state_pipeline
```

See [`crates/tcb/examples/`](crates/tcb/examples/) for the full example source. The example doubles as a smoke test: it is wired into the same test pipeline (`cargo test --all-targets`) and is enforced by clippy.

## Documentation

| Document                                                                 | Language         | Purpose                                                |
| ------------------------------------------------------------------------ | ---------------- | ------------------------------------------------------ |
| [Programming Specification](docs/spec/en-US/EvoRule_Programming_Spec.md) | 🇬🇧 English (ref) | Full programming spec — paradigm, rules, anti-patterns |
| [Determinism Standard](docs/spec/en-US/EvoRule_Determinism_Standard.md)  | 🇬🇧 English (ref) | L1-L4 deterministic classification                     |
| [TCB-Governance Contract](docs/spec/en-US/TCB_Governance_Contract.md)    | 🇬🇧 English (ref) | Runtime interface boundary                             |
| [GATES.md](docs/gate/GATES.md)                                           | 🇬🇧 English       | Admission gates catalog                                |

> 🇨🇳 Chinese source documents are authoritative. English versions are reference translations.

---

## License

EvoRule is released under the **GNU Affero General Public License v3.0**.

```
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 DAMU ZHENG
```

See the [LICENSE](LICENSE) file for full terms.

---

## Contributing

EvoRule is built around **strict paradigm enforcement**. Every commit is checked by:

- **Pre-commit hooks** (mechanical gates)
- **CI pipeline** (compile-time + schema + process gates)
- **Code review** against the specification

Before contributing:

1. Read the [Programming Specification](docs/spec/en-US/EvoRule_Programming_Spec.md)
2. Run `./tools/install-hooks.sh` to install pre-commit checks
3. Ensure `./tools/paradigm-gate.sh` passes locally
4. Sign off on the AGPL-3.0 terms

**There is no `--no-verify` bypass.** Gate failures must be fixed, not bypassed.

---

## Project Status

| Component       | Status      | Notes                                                       |
| --------------- | ----------- | ----------------------------------------------------------- |
| TCB             | ✅ Complete | 25 primitives + 2 control flow, all deterministic           |
| Governance      | ✅ Complete | Constitution dispatch architecture integrated               |
| JSON Rules      | ✅ Complete | `core_eval.json`, inference, constitution                   |
| Paradigm Gates  | ✅ Complete | Pre-commit + CI enforcement                                 |
| Documentation   | ✅ Complete | Spec + determinism + contract                               |
| Python Bindings | 🚧 Porting  | Porting from evorule-v4 (selective integration in progress) |

---

## Version History

| Version | Date       | Notes                                                                                                                                                                                                            |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.1.1   | 2026-07-08 | Added `domain_intersect`, `set_intersection`, `set_diff`, `set_union` primitives and `state_compute` length operation. Fixed ER-601 determinism bug. G-09 compliance: all Chinese comments converted to English. |
| 0.1.0   | 2026-07-01 | Initial TCB release. Governance layer lives in [EvoRuleLab/evorule-governance](https://github.com/EvoRuleLab/evorule-governance).                                                                                |

---

**EvoRule: Rules drive everything. Determinism is the law.**
