# Release Process Audit Checklist

This document is maintained by the release auditor.
Each phase must pass before proceeding to the next.

## Scope

This process applies only to this repository:

- **Repository root**: `D:\evorule-project\evorule-tcb\`
- **Release type**: GitHub Release (English only)
- **crates.io**: not yet decided (pending governance readiness)

Other EvoRule projects (canonical v4, v4-EN) follow different processes.

## Phase 0 -- Baseline (run after first code commit)

- [ ] `cargo build --release` exits 0
- [ ] `cargo build --release --all-targets` exits 0
- [ ] `cargo doc --no-deps` exits 0 (no doc warnings)
- [ ] `cargo clippy --all-targets -- -D warnings` exits 0
- [ ] `cargo test --all-targets` all tests pass
- [ ] `cargo test --doc` all doc tests pass (or only intentional #[ignore])
- [ ] `cargo audit` reports 0 vulnerabilities (or skipped with note)
- [ ] `cargo publish --dry-run` exits 0

## Phase 1 -- Chinese Character Scan (CRITICAL)

Scope: only **tracked files** (via `git ls-files`). Files in `.gitignore` or
`.git/info/exclude` are NOT scanned because they will never reach the release.

- [ ] No Chinese characters in `.rs` files (comments, docstrings, string literals)
- [ ] No Chinese characters in `.md` files (README, CHANGELOG, docs/)
- [ ] No Chinese characters in `.yml` / `.toml` / `.json` files
- [ ] No Chinese characters in `.gitignore` (must also be tracked-clean)
- [ ] No Chinese characters in commit messages (audited via `git log`)

Any CJK Unified Ideograph (U+4E00..U+9FFF) in a tracked file is a release blocker.

### Personal Chinese-Named Reference Folders

If the user creates a personal reference folder with a Chinese name (e.g., a folder named with Chinese characters),
it MUST be ignored via `.git/info/exclude`, NOT `.gitignore`. Reason:

- `.gitignore` is itself a tracked file; any Chinese pattern in it becomes a blocker.
- `.git/info/exclude` is local-only, never tracked, never reaches release.

The auditor verifies the folder is ignored (via `git check-ignore`) but does NOT
block the release based on its name or contents.

## Phase 2 -- Metadata and File Audit

### Cargo.toml

- [ ] `description` present
- [ ] `license` field set (e.g., `MIT`, `Apache-2.0`, `MIT OR Apache-2.0`)
- [ ] `repository` URL set
- [ ] `homepage` URL set (optional but recommended)
- [ ] `readme = "README.md"` set
- [ ] `keywords` array (3-5 keywords)
- [ ] `categories` array (e.g., `["engine", "parser"]`)

### LICENSE

- [ ] Exists at repo root
- [ ] Filename matches license type (`LICENSE` / `LICENSE-MIT` / `LICENSE-APACHE`)
- [ ] Year and copyright holder present
- [ ] License name matches Cargo.toml `license` field

### CHANGELOG.md

- [ ] Follows [Keep a Changelog](https://keepachangelog.com/) format
- [ ] Sections: `Added` / `Changed` / `Fixed` / `Removed` / `Deprecated` / `Security`
- [ ] Version header `## [X.Y.Z] - YYYY-MM-DD`
- [ ] `## [Unreleased]` section present
- [ ] Changes from last version are listed

### README.md

- [ ] Project description (1-2 sentences)
- [ ] Installation section
- [ ] Quick start / minimal example
- [ ] Examples section linking to `examples/`
- [ ] License section
- [ ] Contribution section linking to `CONTRIBUTING.md` (if exists)
- [ ] No Chinese characters

### MIGRATION_GUIDE.md

- [ ] Exists at repo root
- [ ] Documents migration paths for every released version (forward and backward)
- [ ] Version compatibility matrix is up-to-date with the current release
- [ ] No Chinese characters (English only -- same rule as README.md)
- [ ] Cross-references LICENSE / DUAL_LICENSE.md for licensing notes
- [ ] For the initial release (v0.1.0), the matrix contains exactly one row: `(none) -> v0.1.0`

### .gitignore

- [ ] Covers `/target/`
- [ ] Covers IDE files (`.idea/`, `.vscode/`, `*.swp`)
- [ ] Covers OS files (`.DS_Store`, `Thumbs.db`)
- [ ] Cargo.lock gitignored (library convention)
- [ ] No Chinese characters (use `.git/info/exclude` for personal Chinese folders)

### CI (`.github/workflows/ci.yml`)

- [ ] Triggers on push to `main` and on PR
- [ ] Tests on at least one OS (preferably matrix: ubuntu/windows/macos)
- [ ] Runs `cargo build`
- [ ] Runs `cargo test --all-targets`
- [ ] Runs `cargo clippy -- -D warnings`
- [ ] Runs `cargo doc --no-deps`

## Phase 3 -- Release Execution (user runs, auditor verifies)

User runs:

```
git tag vX.Y.Z
git push origin vX.Y.Z
gh release create vX.Y.Z --notes-file CHANGELOG.md
```

(Or creates the GitHub Release via web UI.)

Auditor verifies:

- [ ] git tag matches Cargo.toml `version`
- [ ] CHANGELOG reflects all changes since last version
- [ ] Release notes accurate
- [ ] Source tarball attached (default: GitHub auto-generates)

## Phase 4 -- Post-release

- [ ] Verify GitHub Release renders correctly
- [ ] Verify README renders on crates.io (only if `cargo publish` is used)
- [ ] Verify all links work
- [ ] Tag is signed (optional but recommended)
- [ ] Run `cargo publish` (only if decided)

## Audit Report Format

Each phase produces a short audit report:

- PASS: list of gates that passed
- BLOCKED: list of gates that failed, with file:line evidence
- NOTE: informational items (e.g., 1 ignored doc-test, intentional #[allow])

## Hard Rules

1. NO Chinese in release artifacts (release blocker).
2. NO `cargo` warnings (build / test / clippy / doc).
3. ALL tests pass.
4. Metadata complete before tagging.
5. Tag must match version.

---

## Phase 5 — Paradigm Gates (Admission Gates)

**Status**: ✅ Active (since 2026-06-30)

**Concept**: Every code commit must pass the [Paradigm Gate](./docs/gate/GATES.md). The gate encodes the [EvoRule programming spec](./docs/spec/en-US/EvoRule_programming_Spec.md) red lines as mechanical, AST, and CI-level checks. There is no `--no-verify` bypass.

### Gate Coverage

| Category                     | Gates      | Enforcement                               |
| ---------------------------- | ---------- | ----------------------------------------- |
| **A. Mechanical (regex)**    | G-01..G-09 | pre-commit hook (`.git/hooks/pre-commit`) |
| **B. TCB-specific redlines** | R-07, R-08 | pre-commit + CI                           |
| **C. Schema (JSON)**         | G-15, G-16 | pre-commit or CI                          |
| **D. Process (cargo)**       | G-18..G-23 | CI workflow                               |

Total: **14 gates**, each traces to a specific spec section.

### Running Locally

```bash
# Run all mechanical + TCB gates (~1 second)
./tools/paradigm-gate.sh

# Install as pre-commit hook (auto-runs on every commit)
./tools/install-hooks.sh

# CI runs the full pipeline including cargo gates
# See .github/workflows/paradigm-gate.yml
```

### Current Status

```
=== Mechanical Gates (A) ===
[PASS] G-01 no unsafe in TCB
[PASS] G-02 no wall-clock time
[PASS] G-03 no UUID v4
[PASS] G-04 no RNG
[PASS] G-05 no direct env::var
[PASS] G-06 no deprecated types
[PASS] G-09 no Chinese in code

=== TCB Redlines (B) ===
[PASS] R-07 State immutable
[PASS] R-08 no exec_ calls in TCB

Result: 9 PASS / 0 FAIL / 5 SKIP (skipped gates have no targets yet)
```

### Gate Source

- **Canonical gate spec**: `docs/gate/GATES.md`
- **Gate runner**: `tools/paradigm-gate.sh` (575 lines)
- **Hook installer**: `tools/install-hooks.sh` (90 lines)
- **Spec reference**: `docs/spec/en-US/EvoRule_programming_Spec.md`

---

## Phase 6 — Cargo Workspace (Build Configuration)

**Status**: 🟡 Scaffolded + version-locked, awaiting missing TCB modules

### Version Locking (per version-lock reference doc)

All 12 direct dependencies are pinned with `=` to exact versions matching the v4-EN Cargo.lock. CI runs all cargo commands with `--locked` to prevent silent upgrades.

**Critical for determinism** (L1 boundary conditions):

- `regex 1.12.4` — `Domain::Matches` Unicode semantics
- `im 15.1.0` — `im::HashMap` SipHash seed (from `rand_core 0.6.4`)
- `sha2 0.10.9` — FIPS 180-4 cross-platform consistency

**All 12 direct deps (locked):**

| Crate            | Version | Purpose                                     | Status      |
| ---------------- | ------- | ------------------------------------------- | ----------- |
| `im`             | 15.1.0  | Persistent data structures                  | ✅ active   |
| `log`            | 0.4.33  | Observability tracing                       | ✅ active   |
| `sha2`           | 0.10.9  | Audit chain hash                            | 🟡 reserved |
| `hmac`           | 0.12.1  | HMAC signatures                             | 🟡 reserved |
| `serde`          | 1.0.228 | Serialization core                          | 🟡 reserved |
| `serde_json`     | 1.0.150 | JSON parsing                                | 🟡 reserved |
| `ordered-float`  | 4.6.0   | OrderedFloat wrapper                        | 🟡 reserved |
| `ryu`            | 1.0.23  | Float formatting                            | 🟡 reserved |
| `hex`            | 0.4.3   | Hex encoding                                | 🟡 reserved |
| `regex`          | 1.12.4  | Domain::Matches                             | 🟡 reserved |
| `schemars`       | 0.8.22  | JSON schema                                 | 🟡 reserved |
| `proptest` (dev) | 1.5.0   | Property-based tests                        | ✅ active   |
| `getrandom`      | 0.2.15  | Random number generation (used by proptest) | ✅ active   |

`🟡 reserved` = declared in `[workspace.dependencies]` at exact version, but commented out in `crates/tcb/Cargo.toml` until the code that uses them is written. Uncomment as you implement each module.

### Files

- `Cargo.toml` (workspace root) — workspace + 12 locked deps + release profile
- `crates/tcb/Cargo.toml` — active deps (im, log) + reserved (commented) + proptest dev-dep
- `Cargo.lock` — 16,908 bytes / 667 lines / 74 packages (mirrors v4-EN)

### CI Enforcement

All cargo commands in `.github/workflows/paradigm-gate.yml` use `--locked`:

```yaml
cargo build --release --all-targets --locked
cargo doc --no-deps --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo test --doc --locked
cargo publish --dry-run --locked
```

If Cargo.lock is missing or modified, `cargo --locked` fails the build. This guarantees no silent version drift.

### Module Status

All 11 modules declared in `lib.rs` are now complete:

| Module                  | Status      | Notes                                        |
| ----------------------- | ----------- | -------------------------------------------- |
| `error`                 | ✅ Complete | TCB error types (pure computation)           |
| `value`                 | ✅ Complete | Unified Value type (deterministic)           |
| `state`                 | ✅ Complete | Immutable State container (im::HashMap)      |
| `domain`                | ✅ Complete | Domain matching (conditional logic)          |
| `rule`                  | ✅ Complete | Rule + GenericInstruction                    |
| `exec_context`          | ✅ Complete | **exec** typed accessor                      |
| `deterministic`         | ✅ Complete | content_hash, LogicalClock, DeterministicRNG |
| `audit`                 | ✅ Complete | HMAC-chained audit records                   |
| `control::while_loop`   | ✅ Complete | Self-driving evaluation loop                 |
| `control::dispatch`     | ✅ Complete | O(1) instruction dispatch                    |
| `instruction::registry` | ✅ Complete | Dispatch center (23 primitives)              |
| `primitive::*`          | ✅ Complete | 23 atomic primitives + 2 control flow tools  |

**Total**: 23 .rs source files (~19,700 LOC), 613 tests, 0 failed.

---
