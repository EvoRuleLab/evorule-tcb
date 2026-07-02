# Changelog

All notable changes to `evorule-tcb` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release files (complementing the [0.1.0] TCB implementation):
  - `.gitattributes` — `* text=auto eol=lf` enforcement
  - `.editorconfig` — UTF-8, LF, 4-space indent
  - `rustfmt.toml` — `edition = "2021"`, `max_width = 100`, stable options only
  - `CONTRIBUTING.md` — development workflow + paradigm-gate reference
  - `CODE_OF_CONDUCT.md` — Contributor Covenant v2.1
  - `SECURITY.md` — vulnerability disclosure policy (AGPL-3.0 reporting)
- Filename normalization: `docs/spec/en-US/EvoRule_programming_Spec.txt` and `docs/spec/en-US/TCB_Governance_Contract.txt` renamed to `.md` extension to match the rest of the spec directory.

### Changed

- **Hygiene**: Convert 18 files from CRLF to LF line endings. Affected: 3 .rs sources (`while_loop.rs`, `error.rs`, `lib.rs`), LICENSE, README.md, CHANGELOG.md, RELEASE.md, all `.md` docs under `docs/`, both `.github/workflows/*.yml`, `Cargo.toml`, `crates/tcb/Cargo.toml`, and `tools/install-hooks.sh`. Enforced going forward by `.gitattributes * text=auto eol=lf`.
- **Documentation layout**: Removed internal `docs/audit/` directory (auditor working memory has been relocated to an external location outside the repo).
- **Dependencies**: Removed unused `schemars = "=0.8.22"` from `[workspace.dependencies]`. Confirmed via `cargo tree` that no source file references it; the crate was dead-dependency.

### Deprecated

- None.

### Removed

- None.

### Fixed

- N/A (initial release baseline).

### Security

- All 23 Rust source files carry `SPDX-License-Identifier: AGPL-3.0-or-later` headers.
- Production-code safety counters (post-v0.1.0 audit): 0 `.unwrap()` / 0 `panic!()` / 0 `.expect()`.
- TCB redline gates (R-07 immutable state, R-08 no `exec_` calls inside TCB) enforced via `tools/paradigm-gate.sh` pre-commit + CI workflow.

## [0.1.0] - 2026-07-01

### Added

- Initial TCB (Trusted Computation Base) library implementation.
- 23 Rust source files (~19,300 LOC) covering:
  - **Core types**: `lib.rs`, `error.rs`, `value.rs` (Value enum), `exec_context.rs`
  - **State and Domain**: `state.rs` (im::HashMap state management), `domain.rs` (domain types)
  - **Rule and Audit**: `rule.rs`, `audit.rs` (SHA-256 + HMAC audit chain)
  - **Determinism**: `deterministic.rs` (determinism guarantees)
  - **Control flow**: `control/while_loop.rs` (self-driving evaluation loop), `control/dispatch.rs`, `control/mod.rs`
  - **Instruction registry**: `instruction/mod.rs`, `instruction/registry.rs`
  - **Primitives**: `primitive/mod.rs` + 8 primitive ops files (audit_ops, compute_ops, domain_ops, error_ops, noop_ops, queue_ops, rule_ops, state_ops)
- Project configuration: `Cargo.toml`, `Cargo.lock`, `.gitignore`.
- `README.md` (295 lines) — full project overview.
- CI: `.github/workflows/ci.yml` (273 lines, 3-OS matrix) + `.github/workflows/paradigm-gate.yml` (112 lines, 4-job).
- Canonical documentation under `docs/`:
  - `docs/spec/en-US/EvoRule_programming_Spec.md` (1449 lines)
  - `docs/spec/en-US/EvoRule_Determinism_Standard.md` (258 lines)
  - `docs/spec/en-US/TCB_Governance_Contract.md` (180 lines)
  - `docs/gate/GATES.md` (110 lines, canonical paradigm-gate spec)
  - `docs/developer-guide/design-decisions/01-while-loop-self-driving.md` (592 lines)
  - `docs/developer-guide/design-decisions/02-TheEquation, core_eval.json, and while_loop.md` (172 lines)
- Tooling: `tools/paradigm-gate.sh` (575 lines, 9 PASS / 0 FAIL / 5 SKIP gates), `tools/install-hooks.sh`.
- **public API surface** (initial scope, evaluated as STABLE for v0.1.x): `State`, `Domain`, `Rule`, `Universe`, `EvoRule`, `while_loop`, `TheEquation`, plus the audit-chain entry point.

### Fixed

- N/A (initial release).

### Security

- 100% `SPDX-License-Identifier` headers on all 23 .rs source files.
- TCB redline gates: R-07 state immutable, R-08 no `exec_` calls inside TCB — enforced via `paradigm-gate.sh`.
- All production code carries 0 `.unwrap()` / 0 `panic!()` (single `.expect()` planned for removal in the next minor release).

[Unreleased]: https://github.com/EvoRuleLab/evorule-tcb/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EvoRuleLab/evorule-tcb/releases/tag/v0.1.0
