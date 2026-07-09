# EvoRule Forbidden Items Overview

> **Version**: 1.0 | **Created**: 2026-07-08
> **Purpose**: Single source of truth for ALL forbidden patterns, constraints, and redlines across TCB + Governance layers.
> **Scope**: EvoRule TCB (`evorule-tcb`) and Governance (`evorule-governance`) workspaces.
> **Source**: Aggregated from `docs/gate/GATES.md` (TCB), `docs/gate/GATES.md` (Governance), `docs/spec/en-US/EvoRule_programming_Spec.md`, `.github/workflows/ci.yml`, and code comments.

---

## 0. Quick Reference Guide

| Category                                      | Severity                          | Scope      |
| --------------------------------------------- | --------------------------------- | ---------- |
| **Constitutional Redlines** (ER-600 ~ ER-606) | FATAL — cannot be bypassed        | TCB        |
| **TCB Gates** (G-01 ~ G-25)                   | BLOCK — commit blocked            | TCB        |
| **Governance Gates** (GG-01 ~ GG-31)          | BLOCK — commit blocked            | Governance |
| **Code Comment Constraints**                  | WARNING — violates best practices | Both       |

---

## 1. Constitutional Redlines (ER-600 ~ ER-606)

These are **absolute prohibitions** that cannot be bypassed under any circumstances. Violation = non-deterministic behavior, audit corruption, or architectural breach.

| ID         | Prohibited Behavior                                 | Reason                                                   | Location |
| ---------- | --------------------------------------------------- | -------------------------------------------------------- | -------- |
| **ER-600** | Adding non-deterministic operations to TCB          | Same input must produce same output across all platforms | TCB      |
| **ER-601** | Adding LambdaDomain / Callable transform            | Breaks transparency and auditability                     | TCB      |
| **ER-602** | Importing governance or any external crate into TCB | TCB must have zero external dependencies                 | TCB      |
| **ER-603** | Using unsafe code                                   | Ensure memory safety                                     | TCB      |
| **ER-604** | Using the `if_else` control-flow primitive          | This primitive does NOT exist                            | TCB      |
| **ER-605** | Modifying TCB core source files                     | TCB is the minimized trust base                          | TCB      |
| **ER-606** | Using `$func` / `$eval` in JSON rules               | Function references are not supported                    | JSON     |

**Source**: `docs/spec/en-US/EvoRule_programming_Spec.md` §2.5

---

## 2. TCB Gates (G-01 ~ G-25)

### 2.1 Mechanical Gates — Pre-Commit Hook

| ID       | Gate                        | Forbidden Pattern                                                        | Scope |
| -------- | --------------------------- | ------------------------------------------------------------------------ | ----- |
| **G-01** | No unsafe in TCB            | `unsafe` in `crates/tcb/src/**/*.rs`                                     | TCB   |
| **G-02** | No wall-clock time          | `SystemTime::now()` / `Instant::now()`                                   | TCB   |
| **G-03** | No UUID v4                  | `Uuid::new_v4()`                                                         | TCB   |
| **G-04** | No RNG                      | `rand::random()` / `thread_rng()`                                        | TCB   |
| **G-05** | No direct env::var          | `env::var(...)` outside snapshot pattern                                 | TCB   |
| **G-06** | No deprecated type usage    | 17 deprecated types listed in §6.2                                       | TCB   |
| **G-07** | No $func/$eval in JSON      | `$func` / `$eval` keys in `.json`                                        | JSON  |
| **G-08** | No forbidden transform      | `if_else` / `for_each` / `iterate_list` / `lambda` / `call` in transform | JSON  |
| **G-09** | No Chinese in tracked files | CJK characters (U+4E00-U+9FFF)                                           | All   |

### 2.2 Compile-Time Gates — CI

| ID       | Gate                                | Mechanism                                                                                                                              | Scope           |
| -------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| **G-10** | TCB forbids unsafe                  | `#![forbid(unsafe_code)]` in `crates/tcb/src/lib.rs`                                                                                   | TCB             |
| **G-11** | TCB forbids deprecated              | `RUSTFLAGS="-D deprecated"` in CI build                                                                                                | TCB             |
| **G-12** | TCB has no external deps            | Only `serde`/`im`/`ordered-float`/`sha2`/`hex`/`regex`/`ryu`/`hmac`/`log` allowed                                                      | TCB             |
| **G-13** | No edits to TCB core files          | Git diff blocks changes to `lib.rs`/`value.rs`/`state.rs`/`domain.rs`/`rule.rs`/`exec_context.rs`/`deterministic.rs`/`error.rs`        | TCB             |
| **G-14** | New .rs files only in allowed paths | New files only in `crates/tcb/src/primitive/` or `crates/governance/src/`                                                              | TCB             |
| **G-25** | No platform-dependent integer casts | Direct `i64 as usize` or `u64 as usize` cast without bounds checking; use `(n as u64).min(usize::MAX as u64) as usize` pattern instead | TCB, Governance |

### 2.3 Schema Gates — Pre-Commit / CI

| ID       | Gate                  | Check                                                       | Scope |
| -------- | --------------------- | ----------------------------------------------------------- | ----- |
| **G-15** | rule_id format        | Regex `^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$` | JSON  |
| **G-16** | No skeleton transform | `transform.instructions` has >= 2 entries                   | JSON  |
| **G-17** | JSON valid            | `jq` parse succeeds                                         | JSON  |

### 2.4 Process Gates — CI Required

| ID       | Gate                | Command                                     | Scope |
| -------- | ------------------- | ------------------------------------------- | ----- |
| **G-18** | Release build clean | `cargo build --release` (0 warnings)        | TCB   |
| **G-19** | Docs clean          | `cargo doc --no-deps` (0 warnings)          | TCB   |
| **G-20** | Clippy clean        | `cargo clippy --all-targets -- -D warnings` | TCB   |
| **G-21** | Tests pass          | `cargo test --all-targets` (0 failed)       | TCB   |
| **G-22** | Doc-tests pass      | `cargo test --doc` (0 failed)               | TCB   |
| **G-23** | Publish dry-run     | `cargo publish --dry-run` (0 errors)        | TCB   |
| **G-24** | Version locking     | Critical deps use exact versions (`=x.y.z`) | TCB   |

**Source**: `docs/gate/GATES.md` + `.github/workflows/ci.yml`

---

## 3. Governance Gates (GG-01 ~ GG-31)

Governance inherits all TCB gates (G-01 ~ G-25) and adds governance-specific gates (GG-01 ~ GG-31).

### 3.1 Mechanical Gates — Pre-Commit Hook

| ID         | Gate                                                | Forbidden Pattern                                                                                                                                                                                                                | Scope      |
| ---------- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| **GG-01**  | No v2-compat code in Tier 1                         | `V2ToV4Converter` / `load_from_v2_file` / `load_from_v2_directory` / `load_v2_rules`                                                                                                                                             | Governance |
| **GG-02**  | No `ConfigDrivenStrategy` revival                   | `ConfigDrivenStrategy`                                                                                                                                                                                                           | Governance |
| **GG-03**  | No forbidden I/O channels in Tier 1                 | `FileChannel` / `HttpChannel` / `DatabaseChannel` / `WebsocketChannel` / `StdioChannel` / `ProcessChannel`                                                                                                                       | Governance |
| **GG-04**  | No forbidden deps in Tier 1 source                  | `use tokio` / `use reqwest` / `use rusqlite` / `use rand` / `use tokio_tungstenite`                                                                                                                                              | Governance |
| **GG-05**  | No business rules in `src/`                         | `*.json` with top-level `rule_id` key (excludes mechanism configs)                                                                                                                                                               | Governance |
| **GG-06**  | No non-determinism APIs in Tier 1                   | `SystemTime::now` / `Instant::now` / `Uuid::new_v4` / `rand::random` / `thread_rng`                                                                                                                                              | Governance |
| **GG-07**  | No `unsafe` in Tier 1                               | `unsafe` keyword                                                                                                                                                                                                                 | Governance |
| **GG-08a** | No forbidden transform keywords                     | `if_else` / `for_each` / `iterate_list` / `lambda` / `call`                                                                                                                                                                      | Governance |
| **GG-08b** | No constitutional bans                              | `LambdaDomain` / `$func` / `$eval`                                                                                                                                                                                               | Governance |
| **GG-09**  | No `env::var` at runtime in Tier 1                  | `env::var(`                                                                                                                                                                                                                      | Governance |
| **GG-09b** | No HashMap/HashSet iteration affecting output order | `for ... in &<HashMap>` / `<HashMap>.values().*collect` / `<HashMap>.keys().*collect` / `<HashMap>.iter().*collect` / `<HashSet>.iter()` where collected `Vec` feeds into State, audit, validation results, or public API return | Governance |
| **GG-31**  | No business logic hardcoded in Rust                 | Business logic (algorithms / instruction dispatch / control-flow orchestration / filtering / sorting / comparison / reporting) implemented in Rust function bodies under `src/**`                                                | Governance |

### 3.2 Compile-Time Gates — CI

| ID        | Gate                         | Mechanism                                                                      | Scope      |
| --------- | ---------------------------- | ------------------------------------------------------------------------------ | ---------- |
| **GG-10** | Tier 1 forbids `unsafe`      | `#![deny(unsafe_code)]` in workspace lints                                     | Governance |
| **GG-11** | Tier 1 clippy clean          | `cargo clippy --workspace --all-targets -- -D warnings`                        | Governance |
| **GG-12** | Tier 1 has no forbidden deps | Parse `Cargo.toml` for `tokio`/`reqwest`/`rusqlite`/`rand`/`tokio-tungstenite` | Governance |
| **GG-13** | Release build clean          | `cargo build --release` (0 warnings, 0 errors)                                 | Governance |
| **GG-14** | Docs clean                   | `cargo doc --no-deps` (0 warnings)                                             | Governance |

### 3.3 Schema & Config Gates — Pre-Commit / CI

| ID        | Gate                                                         | Check                                                                     | Scope      |
| --------- | ------------------------------------------------------------ | ------------------------------------------------------------------------- | ---------- |
| **GG-15** | `eval_config.json` valid                                     | `jq` parse succeeds; all declared primitives have corresponding `exec_fn` | Governance |
| **GG-16** | `core_eval.json` valid                                       | `jq` parse succeeds; `dispatch_cases` table well-formed                   | Governance |
| **GG-17** | No `inference_ops`/`solver_ops` in Tier 1 `eval_config.json` | Grep for `inference_ops`/`solver_ops` keys                                | Governance |
| **GG-18** | `meta_instruction_types` includes `noop`                     | Grep `core_eval.json` for `"noop"`                                        | Governance |

### 3.4 Process Gates — CI Required

| ID        | Gate            | Command                              | Scope      |
| --------- | --------------- | ------------------------------------ | ---------- |
| **GG-19** | All tests pass  | `cargo test --lib` (0 failed)        | Governance |
| **GG-20** | Doc-tests pass  | `cargo test --doc` (0 failed)        | Governance |
| **GG-21** | Publish dry-run | `cargo publish --dry-run` (0 errors) | Governance |

### 3.5 Boundary Documentation Gates — CI / PR Review

| ID        | Gate                                    | Check                                                                                             | Scope      |
| --------- | --------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------- |
| **GG-22** | New public API has master-table entry   | PR diff adds `pub fn`/`pub struct`/`pub trait` → diff must modify `layer_Boundary_Contract.md` §6 | Governance |
| **GG-23** | Removed public API updates master table | PR diff removes `pub fn`/`pub struct` → diff must modify §6                                       | Governance |
| **GG-24** | Non-obvious boundary decision has ADR   | PR introduces `DEFERRED` or `REMOVED` classification not in §6 → PR must add ADR                  | Governance |
| **GG-25** | No governance-local docs directory tracked | `git ls-files '文档/'` must return empty (governance-side rule) | Governance |

**Source**: Governance `docs/gate/GATES.md`

---

## 4. Code Comment Constraints

These are constraints documented in code comments but not yet formalized as gates.

### 4.1 Determinism Constraints

| Constraint            | Description                                                     | Location                          |
| --------------------- | --------------------------------------------------------------- | --------------------------------- |
| **Deterministic RNG** | Use `DeterministicRNG` instead of platform-dependent randomness | `crates/tcb/src/deterministic.rs` |

### 4.2 Data Structure Constraints

| Constraint                                | Description                                                                                  | Location   |
| ----------------------------------------- | -------------------------------------------------------------------------------------------- | ---------- |
| **Use im::HashMap for ordered iteration** | Use `im::HashMap` instead of `std::collections::HashMap` when iteration order affects output | Governance |
| **Sort before iteration**                 | If using `std::collections::HashMap`, always sort keys before iteration when order matters   | Governance |

### 4.3 JSON Rule Constraints

| Constraint                                  | Description                                                                                                      | Location                                           |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| **No executable code in JSON**              | JSON rules MUST NOT contain executable code (`$func` / `$eval` are constitutionally banned)                      | `docs/spec/en-US/EvoRule_programming_Spec.md` §2.4 |
| **No wall-clock time / randomness in JSON** | JSON rules MUST NOT depend on wall-clock time / randomness; use `__temporal__.tick` / `DeterministicRNG` instead | `docs/spec/en-US/EvoRule_programming_Spec.md` §2.4 |

---

## 5. Deprecated Types (Forbidden to Use)

The following Rust types are deprecated and MUST NOT be used in new code. Use the corresponding JSON rule instead.

| Deprecated Rust Type           | Replacement JSON Rule                                           |
| ------------------------------ | --------------------------------------------------------------- |
| `ForwardChain`                 | `rules/inference/forward_chain.json`                            |
| `BackwardChainer`              | `rules/inference/backward_chain.json`                           |
| `DimensionChecker`             | `rules/inference/dimension_check.json`                          |
| `ConvergenceChecker`           | `rules/inference/convergence_check.json`                        |
| `InformationGainCalculator`    | `rules/inference/entropy.json` + `calculate_gain.json`          |
| `Planner`                      | `rules/inference/planning_dispatch.json`                        |
| `EffectPredictor`              | JSON rules + `evaluate_expression`                              |
| `CycleDetector`                | TCB primitive `detect_cycles` + JSON rules                      |
| `ConflictDetector`             | TCB primitive `detect_conflicts` + JSON rules                   |
| `SolverValidator`              | `rules/solver/validate_solution.json`                           |
| `SelfCheckConfig`              | `rules/gate/self_check.json`                                    |
| `RedlineChecker`               | `rules/constitution/builtin_*.json`                             |
| `ConstitutionalGate`           | `rules/constitution/` full suite                                |
| `Pipeline` / `PipelineBuilder` | `rules/pipeline/step_definitions.json`                          |
| `ConfigDrivenStrategy`         | `rules/universe/select_rules.json`                              |
| `MetaExecutor`                 | `rules/meta/execute_meta.json`                                  |
| `AdaptiveMeta`                 | `rules/meta/adaptive_cycle.json`                                |
| `InjectPruneExecutor`          | `rules/meta/action_inject_rule.json` + `action_prune_rule.json` |

**Source**: `docs/spec/en-US/EvoRule_programming_Spec.md` §3.2

---

## 6. Inheritance Model

Governance layer **inherits** all TCB gates (G-01 through G-25). The inheritance is non-selective: every TCB gate that applies to Rust code applies to `governance-core` as well.

| Gate Prefix | Origin     | Scope                   |
| ----------- | ---------- | ----------------------- |
| `G-xx`      | TCB        | Inherited by Governance |
| `GG-xx`     | Governance | Governance-specific     |

**Path scope is disjoint**: TCB gates scan `crates/tcb/src/**`; governance gates scan `crates/governance-core/src/**`.

---

## 7. Bypass Policy

**There is no bypass.** Every gate is a hard block. If a gate fails:

1. Read the violation message (gate ID + file:line + pattern)
2. Fix the source (do not bypass)
3. Re-run the gate to verify
4. Commit

If you believe a gate is wrong (false positive), open an issue in the docs repo and reference the gate ID + spec section. **Do NOT commit with `--no-verify`**.

---

### 7.1 Exception: Intentional Deferral Documentation

If a gate flags code that is _intentionally_ deferred (e.g., a Tier 2 prototype temporarily in a branch), the correct response is **not** to bypass the gate but to ensure the code does not land in `crates/governance-core/src/`. Tier 2 code belongs in a separate crate (`crates/governance-io/`, `crates/governance-v2compat/`, etc.) from the start, not in Tier 1 with a "will move later" note.

---

### 7.2 Exception: GG-31 Allowlist Escapes

GG-31 (No business logic hardcoded in Rust) provides two annotation-based escapes:

| Annotation                      | Purpose                                                                                                                                                                                                                                                            | Example                                               |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| `// GG-31: physical primitive`  | Declares a function performs a single atomic physical operation (e.g., `domain.contains(state)` lookup) and is exempt from business-logic classification                                                                                                           | `// GG-31: physical primitive` on function signature  |
| `// GG-31: execution framework` | Declares a function is an execution-framework method in a special-permission file (per Programming Spec §2.3, e.g., `rule_executor.rs`) and is exempt — the method is the framework, not business logic; actual instruction dispatch is delegated to `TheEquation` | `// GG-31: execution framework` on function signature |

**Note**: These escapes are specific to GG-31 and do NOT apply to other gates.

---

### 7.3 Exception: GG-31 Migration Placeholders

Methods marked with `// GG-31: TODO migrate` are temporarily allowed as placeholders to preserve the functional blueprint during migration. They MUST be migrated to JSON rules + primitives before the annotation is removed. Each annotation MUST name the target primitive + JSON rule.

---

### 7.4 Exception: Pre-Commit vs CI Gates

| Gate Type                   | Enforcement                 | Bypass Consideration                         |
| --------------------------- | --------------------------- | -------------------------------------------- |
| **Pre-commit gates** (A, C) | Run locally on every commit | Cannot be bypassed; commit is blocked        |
| **CI gates** (B, D)         | Run in CI pipeline          | Cannot be bypassed; PR is blocked            |
| **Documentation gates** (E) | PR review                   | Cannot be bypassed; requires review approval |

---

### 7.5 Exception: Test Code

Test code (`tests/` directory, `#[test]` functions) is subject to most gates, but with the following distinctions:

| Category                   | Test Code Rule                                                           | Rationale                                                |
| -------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------- |
| **Deprecated types**       | Allowed to reference deprecated types for backward-compatibility testing | Tests must verify deprecated behavior still works        |
| **Non-deterministic APIs** | NOT allowed — tests must be deterministic                                | Tests should produce consistent results                  |
| **unsafe**                 | NOT allowed — tests must be memory-safe                                  | Safety violations in tests are still bugs                |
| **Business logic**         | NOT allowed to implement new business logic                              | Tests should verify existing behavior, not add new logic |

**Annotation for test-specific needs**: Use `// TEST:` annotation on the same line or 2 lines above to document why a test-specific pattern is needed. This annotation does NOT bypass gates but provides audit trail for reviewers.

---

### 7.6 Exception: GG-09b Allowlist Annotation

GG-09b (No HashMap/HashSet iteration affecting output order) provides an annotation-based escape for cases where HashMap iteration is unavoidable:

| Annotation   | Purpose                                                                                    | Scope                                                               |
| ------------ | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| `// ER-601:` | Allowlist comment that permits HashMap/HashSet iteration on the same line or 2 lines above | Must be on the same line or within 2 lines above the iteration code |

**Allowed patterns with annotation**:

- Lookup-only use (`map.get(k)`, `map.insert(k, v)`) — always allowed, no annotation needed
- Iteration guarded by an immediate `sort_by` / `sort_keys` / `BTreeMap`/`BTreeSet` re-collect — always allowed, no annotation needed
- Iteration that feeds into State, audit, validation results, or public API return — requires `// ER-601:` annotation

---

### 7.7 Exception: TCB Primitive Files

ER-605 ('no edits to TCB core source files') and G-13 ('no edits to TCB core files') do NOT apply to `crates/tcb/src/primitive/*.rs` files. This exception is intentional:

| File Type                                                                                                                    | Protection Status                           | Rationale                                                |
| ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- | -------------------------------------------------------- |
| TCB core files (`lib.rs`, `value.rs`, `state.rs`, `domain.rs`, `rule.rs`, `exec_context.rs`, `deterministic.rs`, `error.rs`) | Protected — cannot be modified              | These form the minimal trust base                        |
| TCB primitive files (`primitive/*.rs`)                                                                                       | NOT protected — new primitives can be added | Extensibility requires the ability to add new primitives |

**Process for adding new primitives**:

1. Create new file in `crates/tcb/src/primitive/`
2. Register the primitive in `primitive/mod.rs`
3. Add corresponding test in `primitive/` directory
4. Ensure primitive is deterministic and safe

---

### 7.8 Exception: Emergency Hotfix

In case of a critical security vulnerability in production, the following emergency procedure may be used:

1. **Assess severity**: Confirm the issue is a critical security vulnerability requiring immediate attention
2. **Document exception**: Add `// EMERGENCY HOTFIX: <CVE-ID> — <description>` annotation to the code
3. **Minimal change**: Make only the minimal change necessary to fix the vulnerability
4. **Review**: Get approval from at least one other team member
5. **Follow-up**: Create a follow-up issue to properly migrate/remediate the code once the emergency is resolved

This exception applies ONLY to security vulnerabilities and NOT to feature development or non-critical bugs.

---

## 8. How to Run

### 8.1 TCB Gates

```bash
# Mechanical gates (G-01 through G-09)
cd evorule-tcb
./tools/paradigm-gate.sh
```

### 8.2 Governance Gates

```bash
# Forbidden patterns scan
cd evorule-governance
rg -t rust 'V2ToV4Converter|load_from_v2|ConfigDrivenStrategy' crates/governance-core/src/
rg -t rust 'use tokio|use reqwest|use rusqlite|use rand|use tokio_tungstenite' crates/governance-core/src/
rg -t rust 'SystemTime::now|Instant::now|Uuid::new_v4|rand::random' crates/governance-core/src/
rg 'unsafe' crates/governance-core/src/
```

### 8.3 CI

Both workspaces have CI pipelines that run all gates.

---

## 9. Gate Traceability

Every gate ID maps to a source document. To audit any gate:

1. Find the gate ID in this document
2. Look up the Source column
3. Read the cited section

| Abbreviation | Full Document                                                   |
| ------------ | --------------------------------------------------------------- |
| ER-xxx       | `docs/spec/en-US/EvoRule_programming_Spec.md`                   |
| G-xx         | `evorule-tcb/docs/gate/GATES.md`                                |
| GG-xx        | `evorule-governance/docs/gate/GATES.md`                         |
| CI           | `evorule-tcb/.github/workflows/ci.yml`                          |
| LBC          | `evorule-governance/docs/spec/en-US/layer_Boundary_Contract.md` |
| RSD          | `evorule-governance/docs/architecture/rule-system-design.md`    |

---

**End of Forbidden Items Overview**
