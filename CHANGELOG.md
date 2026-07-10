# Changelog

All notable changes to `evorule-tcb` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-07-08

### Added

- `domain_intersect` primitive (D3 layer, ADR-05): Pure structural check over two domain Value trees with conservative over-approximation. Enables composition-based rewrites of v4's detect_conflicts/detect_cycles/analyze_rule_effects via Recipe G.
- `state_compute` "length" operation: Returns length of list in `value`, writes to `attr`. Non-list -> 0. Enables JSON rules to compute list lengths without an `evaluate_expression` primitive.
- Workflow dispatch trigger added to `paradigm-gate.yml` for manual CI re-runs.
- `set_intersection`, `set_diff`, `set_union` primitives: Set operation primitives returning sorted results for determinism (ER-601 compliant).

### Removed

- `format_string` primitive from TCB compute_ops: Text processing operations violate TCB's JSON-structured-data-only design principle. Migrated to governance-core Layer 1.
- `Matches`, `NotMatches`, `StartsWith`, `EndsWith` variants from `RelOp` enum: String regex and substring matching operations violate TCB design principles. Migrated to governance-core primitives.

### Changed

- **Dependencies**: `proptest` downgraded from 1.11.0 to 1.5.0 to resolve `getrandom 0.4.3` compatibility issue with Rust 1.75.0.
- **Dependencies**: Added explicit `getrandom = "=0.2.15"` to workspace dependencies to prevent accidental upgrade to 0.4.x.
- **Documentation**: All Chinese comments in Rust source code converted to English to comply with G-09 paradigm gate.

### Fixed

- **ER-601 determinism**: Fixed non-deterministic `execute_parallel` force merge strategy in `error_ops.rs`. The original implementation iterated over `im::HashMap` directly, whose iteration order is not guaranteed to be consistent across platforms/versions/hash seeds. Fixed by collecting changed keys into a `HashSet`, then sorting them lexicographically before processing.
- **CI**: Replaced non-existent `actions/swift-actions/cache@v1` with `actions/cache@v4` in `paradigm-gate.yml`.
- **G-09 compliance**: Removed all Chinese characters from code comments (audit.rs, error_ops.rs, rule_ops.rs).

### Security

- TCB redline gates (R-07 immutable state, R-08 no `exec_` calls inside TCB) enforced via `tools/paradigm-gate.sh` pre-commit + CI workflow.

### Added (post-audit)

- `examples/state_pipeline.rs`: minimal pipeline example demonstrating the core TCB API (`State` + `InstructionRegistry` + `content_hash` + determinism check). Runnable via `cargo run --example state_pipeline`.
- 10 new integration test files in `crates/tcb/tests/`: `v1_pure_function`, `v4_determinism`, `v6_iter_order`, `v7_termination`, `v8_recursion_bound`, `v9_audit_chain`, `v10_json_roundtrip`, `v11_state_roundtrip`, `v12_set_ops_commutative`, `v13_hash_collision` — 22 proptests across 5 files.
- `Cargo.toml`: added `homepage` field pointing to the GitHub repository (was missing; required by `cargo publish`).
- `crates/tcb/src/bin/tcb_benchmark.rs`: standalone benchmark binary. **G-02 compliant**: uses `evorule_tcb::deterministic::LogicalClock` (a monotonic u64 counter) instead of wall-clock time, per the TCB redline. The metric is *logical ticks per operation*, not nanoseconds (see `BENCHMARK_REPORT.md` for the trade-off note).
- `rust-toolchain.toml`: pins Rust 1.75.0 + stable rustfmt + clippy (workspace-level toolchain).
- `docs/developer-guide/design-decisions/03-mechanism-vs-policy.md` + `mechanism-exemption.json`: ER-601 mechanism-vs-policy rationale for `State.version` runtime field.
- `docs/gate/FORBIDDEN_OVERVIEW.md`: unified forbidden-rules reference (consolidates `paradigm-gate.sh` checks).
- `BENCHMARK_REPORT.md`: benchmark report (compile-time-stable run, 1.75.0 toolchain, tick budget 100,000). Reports *logical ticks per operation* rather than wall-clock timing, in line with the TCB redline G-02.

### Changed (post-audit)

- Brand attribution consolidated to a single canonical form. Pre-existing native-language attributions (author Chinese name and full Chinese corporate name) replaced with the Latin transliteration `Yuanze` / `Changsha Yuanze Culture Communication Co., Ltd. (Yuanze)` across `NOTICE`, `COMMERCIAL_LICENSE.md`, `COPYRIGHT_ASSIGNMENT_POLICY.md`, `DUAL_LICENSE.md`, `TRADEMARK.md`, plus `README.md` and four `docs/spec/` + `docs/developer-guide/` files. Trademark remains unregistered. Pre-translation names are recorded in commit history (no longer present in tracked files).
- `README.md`: removed cross-repo governance link block (this repository documents the TCB layer only; the governance layer lives in a separate repository).
- `crates/tcb/Cargo.toml`: dropped `v0.1.1` release-binary fragment; no API change.

### Fixed (post-audit)

- `tests/v10_json_roundtrip.rs` + `tests/v11_state_roundtrip.rs`: corrected test assertions for the runtime `State.version` field. The `version: u32` is an in-memory mechanism for O(1) change detection and is `#[serde(skip)]`-marked — it does **not** roundtrip through JSON. Replaced `assert_eq!(state, restored_state)` with `assert_eq!(state.to_value(), restored_state.to_value())` to compare only the JSON-stable part of the state.
- `.gitignore` hygiene (RELEASE.md L40-49): added `*.proptest-regressions`, `cobertura.xml`, `docs/audit/`, `audit/` to prevent accidental commit of test/build artifacts; Chinese-language folder references moved to `.git/info/exclude` (per release procedure, `.gitignore` must be ASCII-only).
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: resolved 12 warnings across 8 files. 9 fixed by code refactor (needless borrow / useless conversion / needless borrows for generic args). 3 fixed by `#![allow(unknown_lints)]` blanket in `crates/tcb/src/lib.rs` (clippy 1.75.0 does not recognise `clippy::cloned_ref_to_slice_refs`, `clippy::assigning_clones`, `clippy::ref_option` — these lints exist in 1.80+) and `#[allow(clippy::assertions_on_constants)]` on the v8 test for sanity-check asserts (no business-logic code was deleted, per user direction).

### Changed (post-audit, G-02 compliance)

- `crates/tcb/src/bin/tcb_benchmark.rs`: replaced `std::time::Instant::now()` + `Duration` with `evorule_tcb::deterministic::LogicalClock` to satisfy paradigm-gate G-02 (no wall-clock time in TCB-scoped sources, including `src/bin/`). The `BenchResult` struct now stores `ticks: u64` / `avg_ticks: f64` instead of `duration: Duration` / `avg_ns: f64`. The benchmark loop runs until `clock.current_tick() < TARGET_TICKS` (100,000) rather than `duration < 200ms`. **Trade-off**: the benchmark no longer reports wall-clock time per operation; every operation reports `1.0000 ticks/op` (LogicalClock advances exactly once per `f()` call). The benchmark is now a deterministic smoke test that proves the redline holds, rather than a comparative performance tool. Verified: `cargo test --workspace --locked` (678 passed / 0 failed / 3 ignored), `cargo clippy --all-targets --all-features --locked -- -D warnings` (exit 0), `bash tools/paradigm-gate.sh` (11 passed / 0 failed / 3 skipped).



### Removed (post-audit, pre-release)

- **Workspace dependency**: removed `criterion = "=0.4.0"` from `[workspace.dependencies]` in `Cargo.toml` (line 33). Criterion was never actually used — no benchmark file imports it; the project's benchmarks use a hand-rolled harness at `crates/tcb/benches/primitives.rs` instead. Removing the dead dependency eliminates the implicit constraint on `getrandom 0.4.x` (which conflicts with rustc 1.75.0) and the stale build dependency.

### Added (post-audit, pre-release)

- `crates/tcb/benches/primitives.rs` (hand-rolled benchmark harness, 24 primitive operations). Replaces criterion; uses `std::time::Instant` for wall-clock timing. **Permitted inside `benches/`** by paradigm-gate G-02 (developer-facing only, never compiled into the release artifact).
- `crates/tcb/Cargo.toml` `[[bench]] name="primitives" harness=false` configuration: required to invoke the custom `main()` instead of cargo's default libtest harness.
- `tools/paradigm-gate.sh` G-02 refinement: `get_rs_files()` now excludes `/benches/` directories. Wall-clock timing is permitted in benchmark harnesses by design; all other `.rs` files (`src/`, `tests/`, `examples/`) remain in scope.
- `STARTUP.md` (project root, 7.6 KB): quick-start developer guide — first file to read when onboarding to evorule-tcb.
- `EvoRule Constitutional Dispatch Architecture.md` (project root, 57 KB): canonical architecture source-of-truth for the dispatch architecture. Supersedes all prior design proposals.
- `docs/LLM_CONTEXT.md` (English, 33 KB): single LLM context injection document for AI assistants working on evorule-tcb. 16 markdown links to canonical `docs/spec/` + `docs/developer-guide/` files (all resolved to tracked sources; English-only on GitHub per release policy).

---

## [0.1.0] - 2026-07-02

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
- Initial release files:
    - `.gitattributes` — `* text=auto eol=lf` enforcement
    - `.editorconfig` — UTF-8, LF, 4-space indent
    - `rustfmt.toml` — `edition = "2021"`, `max_width = 100`, stable options only
    - `CONTRIBUTING.md` — development workflow + paradigm-gate reference
    - `CODE_OF_CONDUCT.md` — Contributor Covenant v2.1
    - `SECURITY.md` — vulnerability disclosure policy (AGPL-3.0 reporting)
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
- `docs/LLM_CONTEXT.md` (31803 B, 713 lines): LLM context injection document — single source of truth for LLM assistants working on `evorule-tcb`. Untracked file.

### Changed

- **Hygiene**: Convert 18 files from CRLF to LF line endings. Affected: 3 .rs sources (`while_loop.rs`, `error.rs`, `lib.rs`), LICENSE, README.md, CHANGELOG.md, RELEASE.md, all `.md` docs under `docs/`, both `.github/workflows/*.yml`, `Cargo.toml`, `crates/tcb/Cargo.toml`, and `tools/install-hooks.sh`. Enforced going forward by `.gitattributes * text=auto eol=lf`.
- **Documentation layout**: Removed internal `docs/audit/` directory (auditor working memory has been relocated to an external location outside the repo).
- **Dependencies**: Removed unused `schemars = "=0.8.22"` from `[workspace.dependencies]`.
- **Constitutional dispatch integration** (governance-core constitutional dispatch architecture integration):
    - `crates/tcb/src/control/dispatch.rs`: **ER-605 Exception #1** — implement dual-source reading. `contains_key("cases")` in `instruction.params` distinguishes main dispatch (read from `__exec__.dispatch_cases`) vs sub-dispatch (read from `instruction.params.cases`).
    - `crates/tcb/src/primitive/audit_ops.rs`: **ER-605 Exception #2** — `trace_step` now appends `[tbl:<hash>]` to `change_summary` so each audit record carries the dispatch-table version.
    - `crates/tcb/src/audit.rs`: audit-chain enhancements supporting `dispatch_table_version` propagation.
    - `crates/tcb/src/primitive/error_ops.rs`: expanded error-handling surface to support dispatch-table validation errors.
    - `crates/tcb/src/primitive/rule_ops.rs`: rule-fixture rewrite aligning with the new dispatch-table test contract.
- **Test coverage**: 557 → 562 tests.
- Filename normalization: `docs/spec/en-US/EvoRule_programming_Spec.txt` and `docs/spec/en-US/TCB_Governance_Contract.txt` renamed to `.md` extension.

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

[Unreleased]: https://github.com/EvoRuleLab/evorule-tcb/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/EvoRuleLab/evorule-tcb/releases/tag/v0.1.1
[0.1.0]: https://github.com/EvoRuleLab/evorule-tcb/releases/tag/v0.1.0
