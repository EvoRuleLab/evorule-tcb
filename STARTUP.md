## Session Startup Checklist

> LLM must read these documents at the start of every new session before writing any code.

### P0 — Must read before ANY code change

**Primary Entry Point**: Read [`evorule-tcb/docs/LLM_CONTEXT.md`](evorule-tcb/docs/LLM_CONTEXT.md) in full. It is the single source of truth for architecture, primitives, ER-600 constitutional red lines, and the "Rust vs JSON" decision tree.

**For deeper references** (Programming Spec, TCB-Governance Contract, Layer Boundary Contract, Rule Format, Manifest, Gates, Determinism Standard, etc.), refer to **§0.3 (Required Reading)** within `LLM_CONTEXT.md`. The list is maintained there to avoid duplication and drift.

---

### Quick Reference — Most Violated Rules

- **GG-31**: Policy decisions (thresholds, iteration limits, business rules) must be in JSON. Mechanisms (how to execute, optimization algorithms) can be in Rust as long as policy values come from JSON. See ADR-03 for distinction.
- **G-13**: TCB core files are frozen for behavior changes. Mechanism optimizations (version counters, data structures, algorithms) are allowed if they preserve determinism. See ADR-03.
- **ER-600**: No non-deterministic APIs (SystemTime, Uuid, rand, HashMap iteration)
- **ER-601**: No LambdaDomain or Callable transform — use evaluate_domain + on_true/on_false
- **GG-09/G-09**: No Chinese comments in code (use English only)

---

## Hard Constraints

- Rule engine must support hot-reloading of JSON rules without compilation
- All rule modifications must pass constitutional checks to prevent bypassing
- Rule persistence is required; API-created rules must survive system restarts
- TCB module must maintain deterministic execution (same input → same output)
- TCB audit records must include all critical fields (execution_result, change_summary, error_message, version) in HMAC signature
- TCB must maintain minimal scope, excluding non-essential components like I/O operations and cross-module tests
- Business logic instructions must be migrated from TCB to governance layer
- All business logic must be implemented via JSON rules; hardcoding in Rust is prohibited unless JSON rules + existing primitives are insufficient
- Core_eval.json must remain immutable (constitutionally immutable) and serve as the system's constitutional rule
- TCB dispatch must use dual-source case reading: instruction.params.cases (sub-dispatch) and **exec**.dispatch_cases (main dispatch)
- Dispatch case for 'dispatch' instruction type must be explicitly registered in register_core_aliases with three explicit $ref fields (key, cases, default) to prevent infinite recursion
- Engine facade must only expose 'register_primitive' and 'register_io_channel' as compliant extension points
- TCB+core must maintain deterministic principles; callers (humans/LLM/own applications) are fully open
- TCB and governance-core must remain as separate repositories with distinct change review standards (TCB: strict, Core: normal)
- TCB must be published as an independent crate to crates.io to enable external usage as a deterministic primitive library
- governance-core must be published as an independent crate to crates.io after TCB, with version dependency on TCB
- InstructionRegistry uses ArcSwap<InstructionRegistry> for atomic version swapping to enable runtime primitive switching without process restart
- Runtime primitive switching requires creating new frozen registry versions to maintain execution determinism
- In-flight executions continue using old registry versions while new requests use updated versions
- TCB and governance-core must use Cargo.lock files and build with --locked flag in CI to ensure dependency version consistency
- Rust toolchain must be locked to version 1.75.0 via rust-toolchain.toml
- proptest version must be unified to 1.5.0 across TCB and governance-core
- tempfile dependency must be pinned to version 3.9.0 to avoid getrandom v0.4.3 incompatibility with Rust 1.75.0
- Registry must be frozen during evaluate() execution, with verification tests ensuring no runtime modifications
- Critical paths must use im::HashMap or explicit sorting for std::collections::HashMap to maintain deterministic iteration order
- No platform-dependent integer casts — direct i64/u64 to usize conversion is prohibited; must use (n as u64).min(usize::MAX as u64) as usize pattern
- All recursive functions must have explicit depth limits to ensure termination (e.g., MAX_PATH_DEPTH=64, MAX_DISPLAY_DEPTH=128, MAX_DOMAIN_DEPTH=128, MAX_HASH_DEPTH=128, while_loop default=10000)
- Missing critical parameters in state operations (state_set, state_compute, set_context) must return explicit MissingParam errors instead of silent failure
- TCB must only handle JSON structured data; text processing capabilities must be implemented in governance layer

## Engineering Conventions

- Rule storage uses file-based persistence in data/rules_runtime/ directory
- Science rules must be organized in discipline-specific directories (mathematics/physics/chemistry)
- All order-dependent operations must either be sorted or include ER-601 determinism comment
- TCB control flow uses evaluate_domain + on_true/on_false for conditional branches
- TCB list traversal uses while_loop + get_index instead of deprecated iterate_list
- TheEquation::build_dispatch_table must be public to enable runtime alias loading verification
- governance-core Cargo.toml must use dual-mode dependency configuration for TCB (path for local dev, version for crates.io)
- registry.rs includes unfreeze() method to enable runtime modification of cloned registries
- equation.rs provides swap_primitive() method for runtime primitive replacement with version tracking
- HashMap iteration in TCB uses iter_std_hashmap_sorted() helper function for deterministic order
- Recursive functions implement depth-limited inner helpers (e.g., value_fmt_inner, contains_inner, value_to_bytes_inner)
- State struct includes version field that increments on modification operations (set, set_all, remove, set_path)
- RuleExecutor supports max_iterations and min_iterations configuration via eval_config.json

## Lessons Learned

- param_schema_for design cannot express alias mappings, hardcoded literals, or field renaming in dispatch cases
- Attempting to completely remove cases from core_eval.json led to loss of critical alias and fallback functionality
- Governance layer cannot directly write to audit chain; must use TCB primitives for audit records
- Dynamic dispatch table generation requires handling both static alias cases and dynamically added primitive cases
- Using $pass for dispatch case causes infinite recursion; explicit $ref fields required to maintain params key structure
- JSON alias files must not use 'literal:' prefix for primitive references (e.g., 'add' instead of 'literal:add')
- State::set() only supports top-level keys; use State::set_path() for nested paths (e.g., **inference**.has_cycle)
- State::get() only supports top-level keys; use State::get_path() for nested paths (e.g., **planning**.rules_ref)
- koa-connect wrapper caused ctx leaks; native Koa middleware implementation required instead
- getrandom v0.4.3 is incompatible with Rust 1.75.0 due to edition2024 requirement; tempfile must be downgraded to 3.9.0 to avoid this dependency
- std::collections::HashMap iteration order is non-deterministic; critical paths must use sorted iteration or im::HashMap
- Full state comparison using != is O(n) and inefficient; use version field comparison for O(1) state change detection