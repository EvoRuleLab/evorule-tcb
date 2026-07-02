# Contributing to EvoRule TCB

Thank you for your interest in contributing to `evorule-tcb` - the Trusted Computing Base for the EvoRule rule-driven execution engine.

`evorule-tcb` is licensed under **AGPL-3.0-or-later**. By contributing, you agree that your contributions will be licensed under the same terms.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Development Environment](#development-environment)
- [Build, Test, Lint](#build-test-lint)
- [Paradigm Gates (Critical)](#paradigm-gates-critical)
- [Pull Request Process](#pull-request-process)
- [Reporting Issues](#reporting-issues)

---

## Code of Conduct

This project adheres to the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to the maintainers.

## Development Environment

| Tool | Version |
|---|---|
| Rust toolchain | **stable** (`rustup default stable`) |
| Minimum supported Rust | **1.75** (per `rust-version` in `Cargo.toml`) |
| OS | Linux, macOS, Windows (CI runs on all three) |
| Git | >= 2.30 (for `.gitattributes` support) |

Recommended: install the pre-commit hook to run paradigm gates locally:

```bash
./tools/install-hooks.sh   # installs .git/hooks/pre-commit
```

## Build, Test, Lint

All commands below use `--locked` to enforce the exact dependency versions in `Cargo.lock` (deterministic builds).

```bash
# Build (release profile - 0 warnings required)
cargo build --release --locked

# Run all tests (unit + doc + integration)
cargo test --locked --all-targets

# Lint (0 warnings required - matches CI gate)
cargo clippy --all-targets --locked --all-features -- -D warnings

# Documentation (0 warnings required - matches CI gate)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked

# Format check (must pass before commit)
cargo fmt --all -- --check

# Auto-format
cargo fmt --all
```

## Paradigm Gates (Critical)

In addition to standard Rust tooling, every commit must pass **`tools/paradigm-gate.sh`** - a bash script that enforces EvoRule-specific redlines beyond what `cargo clippy` can check:

```bash
bash ./tools/paradigm-gate.sh
```

**Current gates** (9 PASS / 0 FAIL / 5 SKIP):

- **G-01**: No `unsafe` in TCB code
- **G-02**: No wall-clock time (`SystemTime::now`, `Instant::now`)
- **G-03**: No UUID v4 (must be deterministic)
- **G-04**: No `rand` crate (RNG forbidden)
- **G-05**: No direct `env::var` (config must flow through `ExecutionContext`)
- **G-06**: No deprecated types
- **G-09**: No Chinese characters in source code
- **R-07**: `State` type must be immutable
- **R-08**: No `exec_*` calls from TCB primitives

Skipped gates activate when relevant features (JSON rules, handler modules) exist.

See `docs/gate/GATES.md` for the canonical gate definitions.

## Pull Request Process

1. **Fork and branch** off `main`
2. **Make changes** following the style in `.editorconfig` and `rustfmt.toml`
3. **Verify locally** - all 5 commands in [Build, Test, Lint](#build-test-lint) must exit 0
4. **Run paradigm gates** - `bash ./tools/paradigm-gate.sh` must report `ALL GATES PASS`
5. **Write a clear PR description**:
   - What problem does this solve?
   - How was it tested?
   - Any backward-compatibility considerations?
6. **Wait for CI** - GitHub Actions runs the same gates on 3 OSes (ubuntu, windows, macOS). All must pass.
7. **Reviewer** - at least one maintainer approval required

### PR Title Convention

`<scope>: <imperative summary>` (50 chars or less)

Examples:

- `clippy: fix 4 approx_constant warnings`
- `primitive: add queue_ops::len()`
- `docs: clarify CHANGELOG entry for v0.1.0`

### Commit Message Convention

Conventional Commits style is encouraged but not enforced.

```
<type>(<scope>): <summary>

<body - 72 char wrap>

<footer>
```

## Reporting Issues

- **Bugs**: open a GitHub issue with reproduction steps, expected vs actual, environment
- **Security vulnerabilities**: see [SECURITY.md](SECURITY.md) - **do not** file a public issue
- **Feature requests**: open an issue describing the use case (not just the solution)

---

## Questions?

Open an issue with the `question` label, or reach out via the contact in [SECURITY.md](SECURITY.md).
