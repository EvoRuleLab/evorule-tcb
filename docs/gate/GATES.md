# EvoRule Paradigm Gates

> **Source**: canonical programming spec (Chinese reference, not in release) sections §2.2 + §2.4 + §2.5 + §4.4 + §6.2 + §7.2 + §8.6
> **Version**: 1.0 | Created: 2026-06-30 (project genesis)
> **Enforcement**: pre-commit hook + CI workflow

This document is the **single source of truth** for admission gates. Every gate traces back to a specific spec section.

---

## Gate Categories

### A. Mechanical (regex) Gates — pre-commit hook

These run on every commit. Must complete in < 5 seconds. Failure = commit blocked.

| ID | Gate | Forbidden Pattern | Spec Reference |
|----|------|-------------------|----------------|
| **G-01** | No unsafe in TCB | `unsafe` in `crates/tcb/src/**/*.rs` | ER-603, §2.2 |
| **G-02** | No wall-clock time | `SystemTime::now()` / `Instant::now()` | §2.2, §7.2 |
| **G-03** | No UUID v4 | `Uuid::new_v4()` | §2.2, §7.2 |
| **G-04** | No RNG | `rand::random()` / `thread_rng()` | §2.2, §7.2 |
| **G-05** | No direct env::var | `env::var(...)` outside snapshot pattern | §7.2 |
| **G-06** | No deprecated type usage | 17 deprecated types listed in §6.2 | V-01, §3.2 |
| **G-07** | No $func/$eval in JSON | `$func` / `$eval` keys in `.json` | ER-606, §8.6 |
| **G-08** | No forbidden transform | `if_else` / `for_each` / `iterate_list` / `lambda` / `call` in transform | §4.4 |
| **G-09** | No Chinese in tracked files | CJK characters (U+4E00-U+9FFF) | release blocker |

### B. Compile-time Gates — CI

Enforced by Cargo and clippy in CI.

| ID | Gate | Mechanism | Spec Reference |
|----|------|-----------|----------------|
| **G-10** | TCB forbids unsafe | `#![forbid(unsafe_code)]` in `crates/tcb/src/lib.rs` | ER-603 |
| **G-11** | TCB forbids deprecated | `RUSTFLAGS="-D deprecated"` in CI build | V-01 |
| **G-12** | TCB has no external deps | parse `crates/tcb/Cargo.toml` deps, fail if non-`serde`/`im` | ER-602 |
| **G-13** | No edits to TCB core files | git diff vs base ref, fail on `lib.rs`/`value.rs`/... | §2.2 |
| **G-14** | New .rs files only in allowed paths | git diff, new files in `crates/tcb/src/primitive/` or `crates/governance/src/` | §2.1 |

### C. Schema Gates — pre-commit or CI

JSON rule structure validation.

| ID | Gate | Check | Spec Reference |
|----|------|-------|----------------|
| **G-15** | rule_id format | regex `^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$` | §4.6 |
| **G-16** | No skeleton transform | `transform.instructions` has >= 2 entries | V-04 |
| **G-17** | JSON valid | jq parse succeeds | §4.1 |

### D. Process Gates — CI required

Cargo commands must all succeed before release.

| ID | Gate | Command | Spec Reference |
|----|------|---------|----------------|
| **G-18** | Release build clean | `cargo build --release` (0 warnings) | release baseline |
| **G-19** | Docs clean | `cargo doc --no-deps` (0 warnings) | release baseline |
| **G-20** | Clippy clean | `cargo clippy --all-targets -- -D warnings` | release baseline |
| **G-21** | Tests pass | `cargo test --all-targets` (0 failed) | release baseline |
| **G-22** | Doc-tests pass | `cargo test --doc` (0 failed) | release baseline |
| **G-23** | Publish dry-run | `cargo publish --dry-run` (0 errors) | release baseline |

---

## How to Run

### Locally (fast, pre-commit)

```bash
./tools/paradigm-gate.sh          # all mechanical gates
./tools/paradigm-gate.sh --fix    # (reserved, not implemented)
```

### Install as pre-commit hook

```bash
./tools/install-hooks.sh
```

After installation, every `git commit` invokes `paradigm-gate.sh` automatically.

### CI

`.github/workflows/paradigm-gate.yml` runs the same gates plus cargo commands.

---

## Bypass Policy

**There is no bypass.** Every gate is a hard block. If a gate fails:

1. Read the violation message (gate ID + file:line + pattern)
2. Fix the source (do not bypass)
3. Re-run `paradigm-gate.sh` to verify
4. Commit

If you believe a gate is wrong (false positive), open an issue in the docs/ repo, do not commit with `--no-verify`.

---

## Gate Traceability

Every gate ID maps to a spec section. To audit any gate:

1. Find the gate ID in this doc
2. Look up the spec reference column
3. Read the spec section to understand why the gate exists

Example: Gate G-06 (no deprecated type usage) maps to §3.2 (deprecated list) + §6.2 (handler forbid list) + V-01 (detection dimension). The forbidden patterns come from §3.2 table, the enforcement location comes from §6.2, and the categorization comes from §12.2.
