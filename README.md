# EvoRule

**Rule-Driven, Deterministic Execution Engine**

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/badge/CI-passing-brightgreen.svg)](https://github.com/EvoRuleLab/evorule-tcb/actions)

EvoRule is a **rule-driven execution platform** where business logic lives entirely in JSON rules, not in code. Rust provides the **deterministic computational base** — the TCB (Trusted Computing Base) — while JSON rules define what to execute and when.

> **One sentence**: Same rules + same input = same output, forever.

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
│  - 22 atomic primitives + 2 control flow tools             │
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
│   ├── tcb/                           # Trusted Computing Base
│   │   ├── src/
│   │   │   ├── lib.rs                 # TCB entry point
│   │   │   ├── value.rs               # Unified Value type (deterministic)
│   │   │   ├── state.rs               # Immutable State container
│   │   │   ├── domain.rs              # Domain matching (conditional logic)
│   │   │   ├── rule.rs                # Rule + GenericInstruction
│   │   │   ├── exec_context.rs        # __exec__ typed accessor
│   │   │   ├── deterministic.rs       # content_hash, LogicalClock, RNG
│   │   │   ├── audit.rs               # HMAC-chained audit records
│   │   │   ├── error.rs               # TCB error types (pure computation)
│   │   │   ├── control/               # Control flow primitives
│   │   │   │   ├── dispatch.rs        # O(1) instruction dispatch
│   │   │   │   └── while_loop.rs      # Self-driving execution loop
│   │   │   └── primitive/             # 22 atomic primitives + 2 control flow
│   │   │       ├── state_ops.rs       # state_set, state_compute
│   │   │       ├── queue_ops.rs       # advance_instruction, push_instruction
│   │   │       ├── domain_ops.rs      # evaluate_domain, match_domain
│   │   │       ├── rule_ops.rs        # apply_rule, filter_rules, inject_rule
│   │   │       ├── compute_ops.rs     # content_hash, format_string, get_index
│   │   │       ├── audit_ops.rs       # trace_step
│   │   │       ├── error_ops.rs       # execute_try_catch, execute_parallel
│   │   │       └── noop_ops.rs        # noop
│   │   └── Cargo.toml                 # Strictly limited dependencies
│   │
│   └── governance/                    # Governance Layer (Rust) — in EvoRuleLab/evorule-governance
│       ├── src/
│       │   ├── engine/                # TheEquation, core_eval.json
│       │   ├── rule_loader.rs         # JSON rule loading
│       │   ├── universe.rs            # Universe (rule management)
│       │   └── session_manager.rs     # Session lifecycle
│       └── rules/                     # All JSON rules
│           ├── core_eval.json         # Entry point — while_loop + dispatch
│           ├── eval_config.json       # primitives, termination, meta_types
│           ├── inference/             # Forward/backward chaining
│           ├── constitution/          # Constitutional rules (immutable)
│           └── ...
│
├── tools/
│   ├── paradigm-gate.sh               # Pre-commit hook runner
│   ├── install-hooks.sh               # Install Git pre-commit hook
│   └── scan_violations.ps1            # Paradigm violation scanner
│
├── docs/
│   ├── spec/                          # Specification (authoritative)
│   │   ├── zh-CN/                     # Chinese source (authoritative)
│   │   │   ├── EvoRule_Programming_Spec.md
│   │   │   ├── EvoRule_Determinism_Standard.md
│   │   │   └── TCB_Governance_Contract.md
│   │   └── en-US/                     # English reference translation
│   │       ├── EvoRule_Programming_Spec.md
│   │       ├── EvoRule_Determinism_Standard.md
│   │       └── TCB_Governance_Contract.md
│   ├── gates/
│   │   └── GATES.md                   # Admission gates (pre-commit + CI)
│   └── developer-guide/
│       └── design-decisions/          # Architecture decision records
│           ├── while-loop-self-driving.md
│           └── theequation-core-eval-analysis.md
│
├── .github/workflows/
│   └── ci.yml                         # CI pipeline (all paradigm gates)
└── README.md                          # This file
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

## Documentation

| Document                                                                 | Language         | Purpose                                                |
| ------------------------------------------------------------------------ | ---------------- | ------------------------------------------------------ |
| [Programming Specification](docs/spec/en-US/EvoRule_Programming_Spec.md) | 🇬🇧 English (ref) | Full programming spec — paradigm, rules, anti-patterns |
| [Determinism Standard](docs/spec/en-US/EvoRule_Determinism_Standard.md)  | 🇬🇧 English (ref) | L1-L4 deterministic classification                     |
| [TCB-Governance Contract](docs/spec/en-US/TCB_Governance_Contract.md)    | 🇬🇧 English (ref) | Runtime interface boundary                             |
| [GATES.md](docs/gates/GATES.md)                                          | 🇬🇧 English       | Admission gates catalog                                |

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
| TCB             | ✅ Complete | 22 primitives + 2 control flow, all deterministic           |
| Governance      | 🚧 Porting  | Porting from evorule-v4 (selective integration in progress) |
| JSON Rules      | ✅ Complete | `core_eval.json`, inference, constitution                   |
| Paradigm Gates  | ✅ Complete | Pre-commit + CI enforcement                                 |
| Documentation   | ✅ Complete | Spec + determinism + contract                               |
| Python Bindings | 🚧 Porting  | Porting from evorule-v4 (selective integration in progress) |

---

## Version History

| Version | Date       | Notes                                                                                                       |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------- |
| 0.1.0   | 2026-07-01 | Initial TCB release. Governance layer lives in [EvoRuleLab/evorule-governance](https://github.com/EvoRuleLab/evorule-governance). |

---

**EvoRule: Rules drive everything. Determinism is the law.**
