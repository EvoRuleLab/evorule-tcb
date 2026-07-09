# TCB-Governance Interaction Contract

> **Version**: 1.0 | **Created**: 2026-07-01
> **Audience**: Rust core developers, Governance layer maintainers, and advanced contributors.
> **Status**: Formal specification derived from TCB implementation audit.
> **Source**: Synthesized from `while_loop.rs`, `dispatch.rs`, `registry.rs`, `state_ops.rs`, `queue_ops.rs`, `domain_ops.rs`, `rule_ops.rs`, `audit_ops.rs`, and `error_ops.rs`.

---

## 0. Purpose and Scope

This document defines the **precise interface boundary** between the TCB (Trusted Computing Base) and the Governance layer.

It answers:
- **What must Governance provide** for TCB primitives to function correctly.
- **What does TCB write** that Governance can (and must) read.
- **What are the error/silence contracts** when inputs are malformed.
- **What operations are strictly forbidden** in TCB (delegated to Governance).

All rules in this document are **hard architectural constraints**. Violations will result in non-deterministic behavior, audit chain corruption, or runtime panics, and MUST be caught by the paradigm gates (GATES.md).

---

## 1. The `__exec__` Context: Governance Injection Requirements

The TCB relies entirely on Governance to initialize and maintain the `__exec__` context. If Governance fails to inject any of these fields, TCB behavior is **undefined** (it will default to safe fallbacks or return errors, but logical correctness is not guaranteed).

| Field | Type | Source | Used By (TCB) | Governance MUST |
|-------|------|--------|---------------|-----------------|
| `instruction` | `Object` | `evaluate()` | `dispatch`, `trace_step`, `while_loop` | Inject the current instruction to execute. |
| `queue` | `List` | `evaluate()` (empty) | `while_loop::drain_queue`, `advance_instruction` | Initialize as an empty list. |
| `__running` | `Bool` | `evaluate()` (`true`) | `while_loop`, `advance_instruction` (termination) | Initialize as `true`. |
| `default_instruction` | `Object` | `eval_config.json` | `advance_instruction` (empty queue fallback) | Provide via `EvalConfig`. Defaults to `noop`. |
| `termination_domain` | `Domain` | `eval_config.json` | `advance_instruction` (termination check) | Provide via `EvalConfig`. |
| `meta_instruction_types` | `List` | `eval_config.json` | `while_loop::drain_queue` (meta vs business) | Provide via `EvalConfig`. **MUST include `"noop"`**. |
| `audit_on` | `Bool` | `eval_config.json` | `trace_step`, `drain_queue` | Provide via `EvalConfig`. Defaults to `true`. |
| `drain_meta_trace` | `Bool` | `eval_config.json` | `while_loop::drain_queue` | Provide via `EvalConfig`. Defaults to `false`. |
| `dispatch_cases` | `Object` | governance-core (`DispatchTableBuilder`, built from `core_eval.json` skeleton + registry scan) | `control/dispatch.rs` (main dispatch path; fallback expansion) | Provide cases table built by governance-core's `DispatchTableBuilder.auto_from_registry()` + `register_core_aliases()`. **v1.1 change**: previously sourced directly from `core_eval.json`; now built dynamically (see §1.1). |
| `dispatch_default` | `Object` | governance-core (`DispatchTableBuilder.set_default`) | `control/dispatch.rs` (main dispatch path when no case matches) | Provide default-case body for main-dispatch miss (prevents `while_loop` deadlock). Built by `build_dispatch_table()`. **v1.1 addition**. |
| `dispatch_table_version` | `String` (SHA-256) | governance-core (`content_hash` of cases + default) | `primitive/audit_ops.rs` (appended to `change_summary` as `[tbl:<hash>]`) | Provide deterministic content-hash of the active dispatch table so each audit record carries the version that was in effect. Enables audit-chain traceability across dispatch-table evolution. **v1.1 addition — ER-605 Exception #2**. |

> **Critical Rule**: Governance MUST ensure `meta_instruction_types` contains `"noop"`. If `noop` is not declared as a meta-instruction, the `drain_queue` logic will treat it as a business instruction, breaking the termination detection mechanism.

---

## 2. TCB Outputs: What Governance Must Read

TCB writes state modifications and audit metadata that Governance is expected to read, validate, or persist.

| Field | Writer (TCB) | Purpose | Reader (Governance) |
|-------|--------------|---------|---------------------|
| `__exec__.last_dispatch_hashes` | `dispatch.rs` | Before/after state hashes for audit precision | `trace_step` (TCB internal), audit exporters |
| `__exec__.__trace_source` | `dispatch.rs`, `while_loop.rs` | Marker for audit source | `trace_step`, audit exporters |
| `__exec__.__trace_branch` | `domain_ops.rs` | Branch selection ("then"/"else"/"none") | `trace_step`, audit exporters |
| `__exec__.__original_metadata` | `registry.rs` | Original metadata for expanded composite instructions | Audit trail, debuggers |
| `__exec__.__terminated_by_max_steps` | `while_loop.rs` | Truncation flag (true if max_steps hit) | `TheEquation` execution status |
| `__domain_result__` | `domain_ops.rs` | Boolean result of domain evaluation | Calling instruction context |
| `__audit_chain` | `audit_ops.rs`, `registry.rs` | HMAC-chained tamper-proof audit log | Governance (verification, export) |
| `__parallel_provenance__` | `error_ops.rs` | Source branch metadata for parallel execution | Governance (debugging, tracing) |
| `__universe_rules__` | `rule_ops.rs` | Dynamically modified rule set | Governance (scheduler, amendments) |
| `__inject_result__` | `rule_ops.rs` | Injection operation result (added/removed/replaced counts) | Governance (validation) |

> **Critical Rule**: Governance MUST NOT directly write to `__exec__` fields. All modifications to `__exec__` MUST be performed via TCB primitives (e.g., `state_set`, `update_exec_field`).

---

## 3. Registration: JSON ↔ Rust Consistency

TCB expects Governance to provide a `primitives` configuration (`eval_config.json`) that declares which primitives are available.

### 3.1 Bidirectional Consistency Checks

TCB enforces two validation checks at startup (`register_from_config`):

| Check | Description | Failure Action |
|-------|-------------|----------------|
| **Completeness** | Every primitive declared in JSON MUST have a corresponding `exec_fn` in Rust. | `Err(EvoRuleError)` → startup fails. |
| **Completeness (Reverse)** | Every Rust `exec_fn` SHOULD be declared in JSON. | `eprintln!` warning (debug builds only). |

### 3.2 Context Operations (Hardcoded in TCB)

The following context operations are **hardcoded** in TCB (registered via `with_default_context_ops`) and do NOT require JSON declaration:
- `add`, `sub`, `mul`, `div`, `set`, `append`, `remove`

---

## 4. Error Handling Contracts (When to Fail vs. Silence)

TCB distinguishes between **fatal configuration errors** (return `Err`) and **logical edge cases** (silently return unchanged state).

### 4.1 Fatal Errors (`Err`)

| Scenario | Error Type | Trigger |
|----------|------------|---------|
| Unregistered instruction | `UnknownInstruction` | Instruction not in registry AND not in cases table. |
| Recursion depth exceeded | `DepthLimitExceeded` | `current_depth >= max_depth`. |
| Missing required parameter | `MissingParam` | `domain` missing in `evaluate_domain`, etc. |
| Invalid domain type | `InvalidDomainType` | Unknown `type` field in Domain. |
| Invalid operation | `InvalidConfig` | `state_compute` with unsupported operation. |
| Nested dispatch | `InvalidConfig` | `dispatch_cases` branch contains another `dispatch`. |

### 4.2 Silent Fallbacks (No Error)

TCB MUST NOT panic. For malformed input that is recoverable, it returns the original state or a default value.

| Scenario | TCB Behavior | File |
|----------|--------------|------|
| `attr` resolves to non-String | Return original state (no-op). | `state_ops.rs` |
| `transform` is not an Object | Return original state. | `state_ops.rs` |
| Rule has no `transform` | Return original state. | `rule_ops.rs::apply_rule` |
| Instruction missing `type` | Skip instruction. | `queue_ops.rs::instruction_sequence` |
| `ref` is not a List | Return `Value::Null`. | `compute_ops.rs::get_index` |
| Missing `last_dispatch_hashes` | Use current state hash (legacy). | `audit_ops.rs` |

---

## 5. Determinism and Version Locking (L2)

The following dependencies MUST be locked to exact patch versions in the root `Cargo.toml`. Governance is responsible for maintaining these locks.

| Crate | Required Version Format | Reason |
|-------|-------------------------|--------|
| `sha2` | `=0.10.x` | SHA-256 algorithm consistency. |
| `regex` | `=1.10.x` | Regex engine semantics (RE2). |
| `ordered-float` | `=4.2.x` | Float comparison determinism. |
| `hmac` | `=0.12.x` | HMAC implementation for audit nonces. |

---

## 6. Absolute Prohibitions (TCB Layer)

Governance MUST NOT attempt to implement the following functionalities inside TCB; they belong to Governance:

| Operation | Reason | Reference |
|-----------|--------|-----------|
| File system I/O | TCB is pure computation. | ER-602 |
| Network requests | TCB is pure computation. | ER-602 |
| Environment variable reads | TCB is pure computation. | ER-602 |
| Rule loading from disk | Handled by `RuleLoader` (Governance). | §10.1 (migration) |
| Logging (`log::trace!`) | Removed from TCB; I/O side effect. | §2.2 |
| `std::eprintln!` warnings | Removed; only `#[cfg(debug_assertions)]` allowed. | §2.2 |
| Unsafe code | Forbidden in TCB. | ER-603 |

---

## 7. Special Interaction Points (Governance Dependencies on TCB Helpers)

Governance layer code MUST use these TCB-provided helpers for consistency:

| Helper | Location | Purpose |
|--------|----------|---------|
| `make_test_registry()` | `primitive/mod.rs` | Create a registry with all primitives for tests. |
| `validate_instruction_type()` | `primitive/mod.rs` | Validate instruction types against registry/cases. |
| `create_full_registry()` | `registry.rs` | Create a registry with all primitives (hardcoded). |
| `create_full_registry_from_config()` | `registry.rs` | Create a registry from JSON config (preferred). |
| `AuditChainState::from_value/to_value` | `audit.rs` | Serialize/deserialize audit chains. |

---

## 8. Pre-Release Compliance Checklist (Governance Side)

- [ ] `eval_config.json` defines `meta_instruction_types` containing `"noop"`.
- [ ] `eval_config.json` defines `termination_domain` correctly (e.g., `__exec__.instruction.type == "noop"`).
- [ ] `core_eval.json` cases table entries have corresponding Rust `exec_fn` implementations.
- [ ] `Cargo.toml` locks `regex`, `sha2`, `ordered-float`, and `hmac` to exact versions.
- [ ] Governance tests use `make_test_registry()` instead of manually constructing `InstructionRegistry`.
- [ ] `RuleLoadError` has been removed from TCB `error.rs`; Governance uses its own error types for I/O.
- [ ] No `eprintln!` warnings remain in production TCB code (debug-only allowed).

---

## 9. Version History

| Version | Date | Notes |
|---------|------|-------|
| 1.0 | 2026-07-01 | Initial release. Formalizes TCB-Governance interaction boundary. |
| 1.1 | 2026-07-05 | **Constitutional dispatch architecture integration** (governance-core integration — see §1.1 of this document and `CHANGELOG.md` for design rationale):<br>• §1 table: `dispatch_cases` source changed from `core_eval.json` to governance-core `DispatchTableBuilder`.<br>• §1 table: added `dispatch_default` and `dispatch_table_version` fields (governance-injected).<br>• **ER-605 Exception #1**: `control/dispatch.rs` — dual-source reading via `contains_key("cases")` distinguishes main dispatch (from `__exec__.dispatch_cases`) vs sub-dispatch (from `instruction.params.cases`).<br>• **ER-605 Exception #2**: `primitive/audit_ops.rs` — `trace_step` appends `[tbl:<hash>]` to `change_summary`.<br>• Cross-references: governance-core commits `fbd4844` (enforce ER-601 determinism + audit HMAC verification), `5d96587` (close ER-601 determinism gaps + add G-GOV-07 gate), `b4eaeda` (add GG-31 gate forbidding business logic hardcoded in Rust), `64cfb97` (migrate 14 GG-31 violations to JSON rules / primitives), `1f0d8cb` (Revert GG-31 premature deletion, restore future-code methods as TODO-migrate placeholders).<br>• Test count: 557 → 562 (cargo test --locked, 0 failed). |

---

**End of Document**

> This contract is the single source of truth for the TCB-Governance interface. Any PR modifying Governance behavior must verify that it does not violate these rules. The paradigm gates (GATES.md) provide automated enforcement for critical items.
