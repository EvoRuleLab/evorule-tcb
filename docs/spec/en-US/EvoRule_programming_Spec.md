# EvoRule Programming Specification

> **Version**: 1.0 | **Created**: 2026-06-28
> **Purpose**: The sole authoritative programming specification for EvoRule V4 programmers + AI/LLM (Triple-merge of original 31 + LLM_GUIDE + DO_AND_DONT).
> **Source Docs**: Merged from `31_EvoRule_Programming_Paradigm_and_Rule_Driven_Architecture_Guide.md` + `LLM_GUIDE.md` + `DO_AND_DONT.md` + Chapter 3 of `32_Paradigm_Violation_Detection_and_Fix_Guide.md`.
> **Maintenance Principle**: Single source of truth. Every rework should reflect on "could this have been avoided by a rule in this document?" — if yes, immediately add to the relevant section.

---

## 0. Document Metadata + Reading Path

### 0.1 Audience

- **Human Programmers**: MUST read before writing EvoRule Rust code + JSON rules.
- **AI/LLM**: MUST read before generating EvoRule code (supersedes the old LLM_GUIDE.md).
- **Architects / Reviewers**: Reference section numbers from this document during code review.

### 0.2 Reading Path (by Role)

| Your Role                         | Step 1                       | Step 2                | Step 3                  | Optional            |
| --------------------------------- | ---------------------------- | --------------------- | ----------------------- | ------------------- |
| **First contact with EvoRule**    | §1 Paradigm Awareness        | §4 JSON Rule Format   | §5 Decision Framework   | §13 Three Maxims    |
| **Ready to write code**           | §2 Responsibility Boundaries | §5 Decision Framework | §6 Handler Writing      | §8 Anti-Patterns    |
| **Facing `#[deprecated]`**        | §3 Deprecated Semantics      | §5 Decision Framework | §10 Migration Process   | Appendix A          |
| **Writing Web handlers**          | §6 Handler Writing           | §8 Anti-Patterns      | §12 Automated Detection | §11 Troubleshooting |
| **AI/LLM before code generation** | §9 LLM Guidelines            | §7 Determinism        | §8 Anti-Patterns        | §12 Detection       |
| **Doing migration/refactoring**   | §10 Migration Process        | §3 Deprecated         | §12 Detection           | §11 Troubleshooting |
| **Code review**                   | §8 Anti-Patterns             | §3 Deprecated Table   | §12 Detection Scripts   | All                 |

### 0.3 Document Structure

- §1 Paradigm Awareness — What EvoRule is NOT
- §2 Responsibility Boundaries — The Rust/JSON Contract
- §3 Deprecated Semantics & Replacement Mapping
- §4 JSON Rule Format (Complete Specification)
- §5 Programming Decision Framework
- §6 Handler Correctness
- §7 Determinism Constitutional Engineering
- §8 Anti-Patterns & Positive Patterns
- §9 Special Guidelines for AI/LLM
- §10 Migration & Refactoring Process
- §11 Troubleshooting
- §12 Automated Detection & Quality Assurance
- §13 Three Maxims
- Appendix A Quick Decision Flowchart
- Appendix B Glossary
- Appendix C Detection Scripts (ps1 / bash / rust / IDE)
- Appendix D Source Document Cross-Reference

---

## 1. Paradigm Awareness: What EvoRule is NOT

> **Source**: §1 = Chapter 1 of Doc 31 + LLM_GUIDE §1.1 Core Philosophy + DO_AND_DONT implicit principles.

EvoRule is a **novel programming paradigm**, not "a Rust framework with JSON configuration." Its core commitments — **Determinism / Auditable / Rule-Driven** — fundamentally conflict with traditional imperative programming habits.

Historical lessons repeatedly show: **The vast majority of rework stems not from insufficient technical ability, but from paradigm misalignment.** Even after reading `LLM_GUIDE.md`, programmers (including AI/LLM) are pulled by strong mainstream habits and unconsciously:

- Write business logic in Rust functions, then "conveniently" wrap them with JSON rules.
- See `#[deprecated]` annotations but continue using the old Rust implementation because "the JSON rule isn't complete yet."
- Directly call Rust modules in Web handlers, downgrading the rule engine to a "configuration loader."
- Use "seemingly harmless" non-deterministic APIs like `SystemTime::now()` / `Uuid::new_v4()`.

Each such choice systematically erodes EvoRule's constitutional commitments, ultimately triggering unpredictable behavior and massive rework.

### 1.1 EvoRule is NOT "a Rust framework with JSON configuration"

**Incorrect Perception**:

> "EvoRule is a Rust framework. Business logic is written in Rust. JSON is just configuration controlling when rules fire."

**Correct Perception**:

> "EvoRule is a rule-driven computation platform. **All business logic MUST reside in JSON rules;** Rust code provides only deterministic primitives / the rule engine / I/O channels — the **computational base**."

| Dimension                | "JSON-configured Rust framework" (WRONG) | "Rule-driven platform" (RIGHT)                                |
| ------------------------ | ---------------------------------------- | ------------------------------------------------------------- |
| Business logic location  | Rust function bodies                     | JSON rule `transform`                                         |
| Rust's role              | Implements business logic                | Provides primitives / registers instructions / executes rules |
| JSON's role              | Configures trigger conditions            | Expresses complete business logic                             |
| Adding new features      | Write Rust functions                     | Write JSON rules                                              |
| Modifying business logic | Change Rust code                         | Change JSON rules                                             |

### 1.2 EvoRule is NOT "Rust first, JSON as fallback"

**Incorrect tendency**:

> "The JSON rule isn't ready yet, so I'll implement it in Rust first, and migrate once the JSON rule is complete."

**Correct Perception**:

> "If a feature already has a corresponding JSON rule declaration (even just a skeleton), the rule engine path **MUST** be used. The Rust implementation is either a TCB primitive or deprecated legacy — **there is no 'temporary Rust implementation' intermediate state**."

Why no intermediate state? Because "temporary" becomes "permanent." Every "Rust first" choice effectively degrades EvoRule to a traditional framework, making the rule engine a decoration. EvoRule's evolution history has repeatedly proven this.

### 1.3 EvoRule is NOT "code vs. rules binary choice"

**False dichotomy**:

> "Since it's rule-driven, the less Rust code the better — put everything into JSON."

**Correct Perception**:

> "Rust and JSON each have responsibility boundaries. **TCB primitives MUST be implemented in Rust** (determinism / performance / formal verification); **business logic MUST be expressed in JSON** (transparent / auditable / evolvable). They occupy **distinct boundaries** (division of labor), not substitution."

### 1.4 Core Philosophy (One Sentence)

| Key Concept          | Description                                                                |
| -------------------- | -------------------------------------------------------------------------- |
| **Rule-Driven**      | Business logic resides in JSON rules, not in code                          |
| **JSON Rules**       | Rules described in JSON format — readable / verifiable / evolvable         |
| **TCB Minimization** | Core computational base minimized, formally verifiable                     |
| **Determinism**      | Same rules + same input = same output, pure functions with no side effects |
| **Transparency**     | Rules are self-explanatory; JSON is human-readable                         |

---

## 2. Responsibility Boundaries: The Rust/JSON Contract

> **Source**: Chapter 2 of Doc 31 + DO_AND_DONT §5 Architecture Constraints + LLM_GUIDE §4.3 Forbidden Files + LLM_GUIDE §1.2 Architecture Overview.

### 2.1 Dependency Direction (Iron Rule)

Python bindings (evorule-py)
↓
Governance Layer (~6300 lines)
↓
TCB Layer (~3500 lines)

**Rule**: Upper layers depend on lower layers; lower layers **MUST NEVER** depend on upper layers.

### 2.2 TCB Layer (Rust) Responsibilities

The TCB is the **minimized trust base**, doing only four things:

1. **Provide deterministic primitives**: `state_set`, `evaluate_domain`, `while_loop`, `content_hash`, `trace_step`, etc.
2. **Execute the rule engine**: `InstructionRegistry` / `while_loop` self-driven cycle.
3. **Guarantee determinism**: `LogicalClock` / `content_hash` / `DeterministicRNG`.
4. **Provide data models**: `State` / `Value` / `Rule` / `Domain`.

**What the TCB ABSOLUTELY DOES NOT do**:

- ❌ Does NOT implement any business logic (e.g., "dimension checking", "forward chaining reasoning", "session management").
- ❌ Does NOT implement business algorithms that belong in JSON rules (e.g., `detect_conflicts` — see Determinism Standard §3 case evaluation).
- ❌ Does NOT call `SystemTime::now()` / `Uuid::new_v4()` / `rand::random()`.
- ❌ Does NOT read/write filesystem / network / environment variables (these belong to Governance I/O channels).
- ❌ Does NOT depend on external crates (except `serde` / `im` and other basic serialization/persistent data structure libraries).
- ❌ Does NOT use `unsafe` code.
- ❌ Does NOT use `if_else` control-flow primitive (this primitive does not exist).

**TCB Core Files (ABSOLUTELY FORBIDDEN TO MODIFY)**:

- `crates/tcb/src/lib.rs`
- `crates/tcb/src/value.rs`
- `crates/tcb/src/state.rs`
- `crates/tcb/src/domain.rs`
- `crates/tcb/src/rule.rs`
- `crates/tcb/src/exec_context.rs`
- `crates/tcb/src/deterministic.rs`
- `crates/tcb/src/error.rs`
- `crates/tcb/src/primitive/*`

### 2.3 Governance Layer (Rust) Responsibilities

Governance is the **computational base extension layer**, doing four things:

1. **Load and manage rules**: `RuleLoader` / `Universe` / `RuleExecutor`.
2. **Provide I/O channels**: `io::file` / `io::env` / `io::http` / `io::database` (called by JSON rules via `io_read` / `io_write`).
3. **Provide high-level execution engines**: `ForwardChain` / `BackwardChainer` / `SessionManager` / `AuditChain`.
4. **Register Governance primitives**: Extend the instruction set via `registry.register()`.

**Governance Rust code boundary**:

- ✅ MAY implement "execution frameworks" (e.g., `ForwardChain::infer` loop logic).
- ❌ MUST NOT implement "business logic" (e.g., "how to determine dimension consistency" — this belongs in JSON rules).
- ⚠️ When a Rust type is annotated `#[deprecated(note = "use JSON rule xxx.json instead")]`, its **business logic has migrated to JSON rules**, and the Rust implementation exists only for historical compatibility.

**Governance files requiring special permission**:

- `crates/governance/src/lib.rs`
- `crates/governance/src/rule_executor.rs`
- `crates/governance/src/rule_loader.rs`

### 2.4 JSON Rule Layer Responsibilities

JSON rules carry **all business logic**:

- Scientific computation rules (e.g., `physics_rules.json` / `math_rules.json`)
- Reasoning workflows (e.g., `forward_chain.json` / `dimension_check.json`)
- Governance workflows (e.g., `amendment/*.json` / `constitution/*.json`)
- Meta-rules (e.g., `meta/*.json`)
- Application workflows (e.g., `session/state_transition.json` / `pipeline/step_definitions.json`)

**What JSON rules DO NOT do**:

- ❌ Do NOT directly call Rust functions (invoked indirectly via primitives).
- ❌ Do NOT contain executable code (`$func` / `$eval` are constitutionally banned).
- ❌ Do NOT depend on wall-clock time / randomness (replaced by `__temporal__.tick` / `DeterministicRNG`).

**JSON rules can be safely modified at**: `crates/governance/rules/*.json` + `docs/*.md`.

### 2.5 Absolute Prohibitions (Constitutional Redlines ER-600 to ER-606)

| Redline ID | Prohibited Behavior                                 | Reason                                   |
| ---------- | --------------------------------------------------- | ---------------------------------------- |
| **ER-600** | Adding non-deterministic operations to TCB          | Same input must produce same output      |
| **ER-601** | Adding LambdaDomain / Callable transform            | Breaks transparency and auditability     |
| **ER-602** | Importing governance or any external crate into TCB | TCB must have zero external dependencies |
| **ER-603** | Using unsafe code                                   | Ensure memory safety                     |
| **ER-604** | Using the `if_else` control-flow primitive          | **THIS PRIMITIVE DOES NOT EXIST!**       |
| **ER-605** | Modifying TCB core source files                     | TCB is the minimized trust base          |
| **ER-606** | Using `$func` in JSON rules                         | Function references are not supported    |

---

## 3. Deprecated Semantics & Replacement Mapping

> **Source**: Chapter 3 of Doc 31 + Chapter 2 of Doc 32 (Violation Type Classification).

### 3.1 This is NOT an ordinary deprecation warning

In traditional Rust projects, `#[deprecated]` generally means "there is a better API, migration recommended." But in EvoRule, its semantics are stronger:

> **`#[deprecated(note = "use JSON rule xxx.json instead")]` is an architecture-level migration directive.**

Its true meaning is:

1. **The Rust type's business logic has migrated to JSON rules**; the Rust implementation exists only for historical compatibility.
2. **New code is FORBIDDEN from using this type** — use would violate the rule-driven architecture.
3. **The correct path is**: drive the corresponding JSON rule through the rule engine, not directly call the Rust type.

### 3.2 Current Deprecated List & Replacement Mapping

> Full list available via `grep -rn "#\[deprecated" crates/governance/src/`. Key mappings below:

| Deprecated Rust Type           | Replacement JSON Rule                                           | Trigger Condition                                   |
| ------------------------------ | --------------------------------------------------------------- | --------------------------------------------------- |
| `ForwardChain`                 | `rules/inference/forward_chain.json`                            | `__inference__.mode == "forward"`                   |
| `BackwardChainer`              | `rules/inference/backward_chain.json`                           | `__inference__.mode == "backward"`                  |
| `DimensionChecker`             | `rules/inference/dimension_check.json`                          | `__inference__.dimension_check_requested == true`   |
| `ConvergenceChecker`           | `rules/inference/convergence_check.json`                        | `__inference__.convergence_check_requested == true` |
| `InformationGainCalculator`    | `rules/inference/entropy.json` + `calculate_gain.json`          | `__inference__.entropy_requested == true`           |
| `Planner`                      | `rules/inference/planning_dispatch.json`                        | `__inference__.planning_requested == true`          |
| `EffectPredictor`              | JSON rules + `evaluate_expression`                              | `__inference__.effect_predict_requested == true`    |
| `CycleDetector`                | TCB primitive `detect_cycles` + JSON rules                      | `__inference__.cycle_detect_requested == true`      |
| `ConflictDetector`             | TCB primitive `detect_conflicts` + JSON rules                   | `__inference__.conflict_detect_requested == true`   |
| `SolverValidator`              | `rules/solver/validate_solution.json`                           | `__solver__.validate_requested == true`             |
| `SelfCheckConfig`              | `rules/gate/self_check.json`                                    | `__gate__.self_check_requested == true`             |
| `RedlineChecker`               | `rules/constitution/builtin_*.json`                             | Constitutional rules auto-trigger                   |
| `ConstitutionalGate`           | `rules/constitution/` full suite                                | Constitutional rules auto-trigger                   |
| `Pipeline` / `PipelineBuilder` | `rules/pipeline/step_definitions.json`                          | `__pipeline__.run_requested == true`                |
| `ConfigDrivenStrategy`         | `rules/universe/select_rules.json`                              | `__universe__.select_requested == true`             |
| `MetaExecutor`                 | `rules/meta/execute_meta.json`                                  | `__meta__.execute_requested == true`                |
| `AdaptiveMeta`                 | `rules/meta/adaptive_cycle.json`                                | `__meta__.adaptive_cycle_requested == true`         |
| `InjectPruneExecutor`          | `rules/meta/action_inject_rule.json` + `action_prune_rule.json` | Corresponding `__meta__.*_requested`                |

### 3.3 Correct Invocation Pattern

**Incorrect pattern (directly calling deprecated Rust type)**:

```rust
// ❌ Violates rule-driven architecture
let checker = DimensionChecker::new(5);
let result = checker.check(&rules);
let passed = result.passed;
```

Correct pattern (driving JSON rules via rule engine):

```rust
// ✅ Construct State, set trigger flags
let mut state = State::new();
state = state.set("__inference__.dimension_check_requested", Value::Bool(true));
state = state.set("__inference__.target_formula", target_formula);

// ✅ Execute via rule engine
let engine = ForwardChain::new(max_steps);  // ForwardChain as execution framework may remain
let result = engine.infer(state, &rules);   // Business logic driven by JSON rules

// ✅ Read from result State
let passed = result.final_state
    .get("__inference__.dimension_result")
    .map(|v| v == "consistent")
    .unwrap_or(false);
```

Key difference: In the incorrect pattern, checker.check() hardcodes "how to check dimensions" inside Rust. In the correct pattern, Rust provides only the loop framework engine.infer(), and "how to check dimensions" is determined by dimension_check.json's transform.

### 3.4 What if the JSON Rule is Only a Skeleton?

The most common cognitive trap:

"I see dimension_check.json is just a skeleton returning "consistent" / "no_target" strings, with no real checking logic. So I must use the Rust DimensionChecker."

This is the wrong priority judgment. Correct approach:

Complete the JSON rule's business logic first (use instruction_sequence + evaluate_domain + while_loop to implement real dimension checking).

Then have the handler use the rule engine path.

NEVER keep using deprecated Rust because "the JSON rule is incomplete" — this will keep the JSON rule a skeleton forever.

Judgment criteria:

If deprecated Rust implements business logic X, and the JSON rule does not → Complete the JSON rule, do NOT keep the Rust.

If the JSON rule's capability is genuinely insufficient to express X (requires new primitives) → Register new primitives in Governance, then call them from JSON rules.

If X is essentially a deterministic computational primitive (e.g., SHA-256) → It should be provided as a TCB primitive, called by JSON rules.

## 4. JSON Rule Format (Complete Specification)

Source: Chapter 2 of LLM_GUIDE + DO_AND_DONT §2/§3/§4 + Chapter 3 of LLM_GUIDE (Error Patterns).

### 4.1 Rule Structure

```json
{
    "rule_id": "namespace.category.name",
    "name": "Rule Name",
    "domain": {
        // Matching conditions
    },
    "transform": {
        // Execution actions
    }
}
```

### 4.2 Domain Condition Types

| Type      | Description      | Example                                                      |
| --------- | ---------------- | ------------------------------------------------------------ |
| atom      | Atomic condition | `{"type": "atom", "attribute": "x", "op": "eq", "value": 1}` |
| and       | AND condition    | `{"type": "and", "domains": [...]}`                          |
| or        | OR condition     | `{"type": "or", "domains": [...]}`                           |
| not       | NOT condition    | `{"type": "not", "domain": {...}}`                           |
| universal | Always true      | `{"type": "universal"}`                                      |
| empty     | Always false     | `{"type": "empty"}`                                          |

### 4.3 Operator List

| Operator | Aliases            | Description                               |
| -------- | ------------------ | ----------------------------------------- |
| eq       | == / equals        | Equal                                     |
| ne       | != / not_equals    | Not equal                                 |
| gt       | > / greater_than   | Greater than                              |
| ge       | >= / greater_equal | Greater or equal                          |
| lt       | < / less_than      | Less than                                 |
| le       | <= / less_equal    | Less or equal                             |
| contains | has                | Contains                                  |
| matches  | regex              | Regex match (RE2 semantics, lock version) |
| in       | -                  | In set                                    |
| notin    | not_in             | Not in set                                |

### 4.4 Transform Types

| Type                 | Description                      | Key Parameters               |
| -------------------- | -------------------------------- | ---------------------------- |
| instruction_sequence | Sequential instruction execution | instructions                 |
| state_set            | Pure assignment                  | attr / value                 |
| evaluate_domain      | Domain evaluation + branching    | domain / on_true / on_false  |
| while_loop           | Loop                             | condition / body / max_steps |

Prohibited: if_else / for_each / iterate_list / lambda / call / $func / $eval.

### 4.5 Physical Primitive List (Complete)

state_ops:

state_set — pure assignment: {"type": "state_set", "params": {"attr": "x", "value": 1}}

queue_ops:

advance_instruction — advance instruction pointer

push_instruction — push single instruction

push_instruction_sequence — push instruction sequence

instruction_sequence — sequential execution

domain_ops:

evaluate_domain — domain evaluation + branch push

compute_ops:

content_hash — SHA-256 hash

audit_ops:

trace_step — audit trail step

4.6 Rule ID Format
{namespace}.{category}.{name} (three-level namespace).

Examples: core.instruction.increment / physics.force.gravity / math.equation.solve.

4.7 Priority / Layers
Layer Priority Range Purpose
Layer 0 (Constitution) 9000-10000 Constitutional rules (immutable)
Layer 0.5 (Amendments) 8000-8999 Amendment rules
Layer 1 (Meta-rules) 7000-7999 Meta-rules
Layer 2 (Business) 0-6999 Ordinary business rules

4.8 Minimal Runnable Rule Example

{
"rule_id": "example.hello_world",
"name": "Hello World",
"domain": {
"type": "atom",
"attribute": "**exec**.instruction.type",
"op": "eq",
"value": "hello"
},
"transform": {
"type": "instruction_sequence",
"params": {
"instructions": [
{"type": "state_set", "params": {"attr": "message", "value": "Hello, World!"}},
{"type": "advance_instruction"}
]
}
}
}

5. Programming Decision Framework
   Source: Chapter 4 of Doc 31 + §6.1 of Doc 32 (New Feature Decision Tree).

5.1 Decision Tree (When Facing a Choice)

【Step 1】Is this "business logic" or "computational primitive"?
│
├─ Business logic (e.g., "how to determine dimension consistency")
│ │
│ 【Step 2】Does a corresponding JSON rule already exist?
│ │
│ ├─ Yes and complete → Use rule engine path (construct State + set trigger flags + call ForwardChain)
│ │
│ ├─ Yes but incomplete → Complete the JSON rule (use instruction_sequence + evaluate_domain + while_loop)
│ │
│ └─ No → Create new JSON rule (rules/namespace/x.json + domain + transform + result_key + tests)
│
└─ Computational primitive (e.g., "SHA-256 hash")
│
【Step 3】Does the TCB already have this primitive?
│
├─ Yes → Reuse primitive
│
└─ No → Register new primitive
├─ Deterministic computational primitive → Register in TCB layer (requires change review; TCB is frozen)
│ ├─ crates/tcb/src/primitive/x_ops.rs
│ ├─ Implement exec_x() function
│ ├─ Register in mod.rs
│ └─ Write unit tests + update §3 mapping table
│
└─ I/O or high-level operation → Register in Governance layer
├─ crates/governance/src/primitive/x.rs
├─ Implement exec_x() function
├─ Register in mod.rs
└─ Write unit tests + update §3 mapping table

5.2 Five Hard Judgment Rules
Rule A: Zero-Rust-for-Business-Logic Principle

Any business logic that can be expressed in JSON rules is FORBIDDEN from being implemented in Rust.

Rule B: Deprecated = Forbidden Principle

Upon seeing #[deprecated(note = "use JSON rule xxx.json instead")], FORBID calling that type in new code. Existing calls MUST be migrated.

Rule C: Handler = Adapter Layer Principle

Web handler / CLI handler / Python binding responsibilities are strictly:

Parse input (HTTP body / CLI args / Python kwargs)

Construct State, set trigger flags

Call the rule engine (ForwardChain::infer, etc.)

Read response data from the result State

Serialize output

FORBIDDEN to directly call business logic modules (DimensionChecker / AuditChain business methods, etc.) in handlers.

Rule D: Absolute Determinism Principle

Any code path (Rust or JSON rules) is FORBIDDEN from using:

SystemTime::now() / Instant::now()

Uuid::new_v4()

rand::random() / thread_rng()

Process ID / Thread ID as logical input

Filesystem timestamps as logical input

Alternatives: LogicalClock::current_tick() / content_hash() / DeterministicRNG::from_seed().

Rule E: JSON-Rule-First Completion Principle

When a JSON rule is only a skeleton, complete the rule first, then use the rule path. NOT ALLOWED to keep using deprecated Rust because "the rule is incomplete."

6. Handler Correctness
   Source: Chapter 5 of Doc 31 + DO_AND_DONT §2.1/§2.2/§2.4.

6.1 Five-Segment Structure
Every handler MUST strictly follow the five-segment structure:

async fn some_handler(
AxumState(state): AxumState<Arc<AppState>>,
Path(id): Path<String>, // 1. Parse input
Json(req): Json<RequestBody>, // 1. Parse input
) -> Result<Json<JsonValue>, (StatusCode, Json<ErrorResponse>)> {
// 2. Construct State, set trigger flags
let mut state = State::new();
state = state.set("**namespace**.action_requested", Value::Bool(true));
state = state.set("**namespace**.input", req.into_value());

    // 3. Call rule engine
    let engine = ForwardChain::new(state.config.max_steps);
    let rules = state.rule_loader.lock().unwrap().get_rules_clone();
    let result = engine.infer(state, &rules);

    // 4. Read response from result State
    let response_data = result.final_state
        .get("__namespace__.result")
        .ok_or_else(|| internal_error("rule produced no result"))?;

    // 5. Serialize output
    Ok(Json(json!({ "result": response_data })))

}

6.2 Handler Prohibited List
Handlers are FORBIDDEN to contain:

❌ DimensionChecker::new() / DimensionChecker::check()

❌ BackwardChainer::new() / BackwardChainer::chain()

❌ ConvergenceChecker::new() / ConvergenceChecker::check()

❌ InformationGainCalculator::new() / .calculate()

❌ Planner::new() / .plan()

❌ EffectPredictor::new() / .predict()

❌ CycleDetector::new() / .detect()

❌ ConflictDetector::new() / .detect()

❌ SolverValidator::new() / .validate()

❌ SelfCheckConfig::new() / SelfCheck::run()

❌ RedlineChecker::new() / .check()

❌ ConstitutionalGate::new() / .evaluate()

❌ Pipeline::new() / PipelineBuilder::new()

❌ ConfigDrivenStrategy::new()

❌ MetaExecutor::new() / .execute()

❌ AdaptiveMeta::new() / .cycle()

❌ InjectPruneExecutor::new() / .execute()

Handlers MAY contain:

✅ Read-only access to RuleLoader (getting rule lists).

✅ Read-only access to Universe (statistics).

✅ ForwardChain::new() / BackwardChainer::new() as execution frameworks (note: these are also deprecated themselves; long-term plan to migrate to unified RuleEngine::infer).

✅ State::new() / State::set() / State::get().

✅ Read-only access to AuditChain (export / statistics) — audit chain writes are auto-handled by the rule engine.

✅ SessionManager session lifecycle management (create / query) — session management is Governance infrastructure, not business logic.

✅ Serialization / error handling / HTTP adaptation.

6.3 When Business Logic Genuinely Needs a New Primitive
If JSON rules cannot express some logic with existing primitives (e.g., "batch filter rules"), the correct flow is:

Register new primitive in Governance (e.g., planning.rs with filter_applicable_rules).

Call that primitive from JSON rules.

Handler still only triggers execution.

The incorrect flow is:

❌ Write Rust implementation directly in the handler.

❌ Add new methods to deprecated Rust types.

7. Determinism Constitutional Engineering
   Source: Chapter 6 of Doc 31 + DO_AND_DONT §1 + LLM_GUIDE §1.3.

7.1 Determinism is NOT "try your best", it's "MUST achieve"
EvoRule's core commitment:

Same rules + same input = same output (forever).

This commitment is what distinguishes EvoRule from other rule engines. It enables:

Computation results are reproducible

Audit chains are verifiable

Reasoning processes are replayable

Errors are localizable

Any code that breaks determinism effectively degrades EvoRule to an ordinary rule engine.

7.2 Non-Deterministic API Alternatives
Forbidden API Use Case Alternative
SystemTime::now() Get current time LogicalClock::current_tick() / **temporal**.tick
Instant::now() Measure duration Should not measure duration in business logic (performance monitoring belongs to Governance infrastructure)
Uuid::new_v4() Generate unique ID content_hash(&content)
rand::random() Generate random numbers DeterministicRNG::from_seed(seed)
thread_rng() Thread RNG DeterministicRNG::from_seed(seed)
process::id() Get process ID Should not be used as logical input
env::var() direct Read environment variables Snapshot at startup into State.**env**; business logic reads via state_set + $ref
File mtime File modification time Use content_hash to detect content changes
7.3 Determinism Boundary
The determinism boundary refers to operations that "appear non-deterministic but are actually controllable":

I/O operations: File reads / network requests — content may change, but under the same snapshot results are consistent.

Environment variables: May differ across processes, but within the same process are consistent.

Handling method:

I/O results stored in State.**io**.snapshot; business logic reads only the snapshot.

Environment variables snapshotted at startup into State.**env**; business logic reads only the snapshot.

Audit chain records I/O call input parameters and output hashes, ensuring replayability.

7.4 Input Contract & Normalization (L1 Prerequisite)
Functions accepting external input (e.g., serde_json_to_value) MUST declare:

Accepted input subset: e.g., reject duplicate keys, reject integers outside i64 range.

Normalization rules: e.g., all integers within i64 range, floats finite.

Handling of illegal input: Reject (return error) vs. Normalize (NaN → 0.0).

For functions without a declared input contract, L1 determinism only holds for "normalized inputs." JSON ambiguities (duplicate keys, large integer precision loss, floating-point representation) MUST be eliminated before entering the TCB.

7.5 Compositional Determinism (Cross-Layer)
L1 determinism of an individual primitive does not automatically compose. Control-flow primitives (while_loop, try_catch, execute_parallel) have L1 status = min(own L1, L1 of all invocable instructions).

Judgment rule: If a control-flow primitive invokes a non-deterministic instruction from the registry, the whole composition degrades to non-deterministic.

TCB currently lacks an "instruction determinism whitelist" mechanism, so control-flow primitives are conditionally deterministic — dependent on caller discipline.

## 8. Anti-Patterns & Positive Patterns

> **Source**: §4.3 of Doc 31 + DO_AND_DONT §4 + Chapter 3 of LLM_GUIDE.

### 8.1 Anti-Pattern 1: Handler directly calls Rust business modules

```rust
// ❌ Anti-pattern
async fn dimension_check_handler(...) -> Json<Value> {
    let checker = DimensionChecker::new(5);          // deprecated!
    let result = checker.check(&rules);              // Business logic in Rust
    Json(json!({ "passed": result.passed }))
}
```

```rust
// ✅ Positive pattern
async fn dimension_check_handler(...) -> Json<Value> {
    let mut state = State::new();
    state = state.set("__inference__.dimension_check_requested", Value::Bool(true));
    state = state.set("__inference__.target_rules", rules_value);

    let engine = ForwardChain::new(1000);            // Execution framework
    let result = engine.infer(state, &rules);        // Business logic driven by JSON rules

    let passed = result.final_state
        .get("__inference__.dimension_result")
        .map(|v| v == "consistent")
        .unwrap_or(false);

    Json(json!({ "passed": passed }))
}
```

### 8.2 Anti-Pattern 2: Using wall-clock time for ID generation

```rust
// ❌ Anti-pattern
let session_id = format!("sess-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
```

```rust
// ✅ Positive pattern
let session_id = content_hash(&format!("session:{}:{}", name, logical_clock.current_tick()));
```

### 8.3 Anti-Pattern 3: Implementing "how to verify audit chain integrity" in Rust

```rust
// ❌ Anti-pattern
fn verify_audit_chain(chain: &AuditChain) -> bool {
    // 100 lines of Rust implementing integrity check logic
}
```

```rust
// ✅ Positive pattern
// 1. Write JSON rule audit/verify.json with transform using instruction_sequence
//    calling audit_ops primitives (trace_step / content_hash / etc.)
// 2. Handler only triggers execution
let mut state = State::new();
state = state.set("__audit__.verify_requested", Value::Bool(true));
let result = engine.infer(state, &rules);
let is_valid = result.final_state.get("__audit__.is_valid").unwrap();
```

### 8.4 Anti-Pattern 4: Keeping Rust implementation because "Rust is faster"

```rust
// ❌ Anti-pattern
// "Traversing 1000 rules with while_loop is too slow, I'll just use a for loop in Rust"
fn fast_check_all_rules(rules: &HashMap<String, Rule>) -> Vec<Issue> {
    rules.iter().filter_map(|(_, r)| check_rule(r)).collect()
}
```

```rust
// ✅ Positive pattern
// Performance issues are solved by optimizing the rule engine, not by moving logic back to Rust.
// If while_loop is genuinely underperforming, register a batch primitive (e.g., filter_rules),
// but the logic of "determining rule compliance" remains in JSON rules.
```

### 8.5 Anti-Pattern 5: Using non-existent primitives (Quick Reference)

| Scenario              | Incorrect ❌       | Correct ✅                                   |
| --------------------- | ------------------ | -------------------------------------------- |
| Conditional branch    | Use `if_else`      | Use `evaluate_domain` + `on_true`/`on_false` |
| Loop                  | Use `for_each`     | Use `while_loop` + `max_steps`               |
| List iteration        | Use `iterate_list` | Use `while_loop` + `get_index`               |
| Function return value | Use `$func`        | Use concrete value or `$ref`                 |
| Time retrieval        | Use `now()`        | Use `__temporal__.state.timestamp`           |
| Random numbers        | Use `rand()`       | Not supported (determinism required)         |
| File reading          | Use `std::fs`      | Via `io_read` primitive                      |
| External API          | Direct HTTP call   | Via Governance layer I/O                     |

### 8.6 Anti-Pattern 6: Using Lambda / Callable / Dynamic Code in JSON

```json
// ❌ Incorrect! Function references not supported
{"type": "atom", "attribute": "x", "op": "eq", "value": {"$func": "calculate"}}

// ❌ Incorrect! Dynamic code not supported
{"type": "state_set", "params": {"attr": "code", "value": {"$eval": "x + 1"}}}
```

```json
// ✅ Correct: Use concrete values
{"type": "atom", "attribute": "x", "op": "eq", "value": 42}

// ✅ Correct: Use $ref to reference state values
{"type": "state_set", "params": {"attr": "result", "value": {"$ref": "__exec__.instruction.params.delta"}}}
```

---

## 9. Special Guidelines for AI/LLM

> **Source**: Chapter 7 of Doc 31 + LLM_GUIDE §1.3 + Chapter 3 of LLM_GUIDE.

### 9.1 Paradigm Misalignments LLMs are Prone To

AI/LLM generating EvoRule code, influenced by mainstream training data, are particularly prone to:

1. **"Shortest runnable path" preference**: Direct Rust function call is "more efficient" than constructing State + invoking engine + reading result keys. But this violates rule-driven architecture.
2. **"Complete functionality" preference**: Seeing a JSON rule skeleton, they tend to implement functionality in Rust rather than completing the JSON rule.
3. **"Configuration-as-rule" misconception**: Treating JSON rules as configuration files for Rust functions, rather than carriers of business logic.
4. **"deprecated = recommended migration" misconception**: Treating `#[deprecated]` as an ordinary Rust deprecation warning rather than an architecture-level migration directive.
5. **"Determinism can be compromised" tendency**: Using non-deterministic APIs in "just test code" / "just logging" / "just temporary ID" scenarios, believing it "doesn't affect main logic." But determinism is a global commitment — there are no "local exceptions."

### 9.2 Pre-Generation Self-Checklist for LLMs

Before generating any EvoRule code, LLMs MUST ask themselves:

- [ ] Is the logic I'm writing business logic or a computational primitive?
- [ ] If business logic, does a JSON rule already exist? Am I calling a deprecated Rust type?
- [ ] Am I directly calling business modules in a handler?
- [ ] Am I using `SystemTime` / `Uuid::new_v4` / `rand::random`?
- [ ] Am I choosing Rust implementation because "the JSON rule is incomplete"?
- [ ] Am I saying "use Rust first, migrate later"?

**If any answer is "yes", it indicates paradigm misalignment — redesign is required.**

### 9.3 What LLMs SHOULD Actively Do

- Upon seeing `#[deprecated]` annotation, **actively** locate the corresponding JSON rule file.
- Upon seeing a JSON rule skeleton, **actively** complete the rule logic (using `instruction_sequence` + `evaluate_domain` + `while_loop`).
- Upon seeing a handler directly calling Rust business modules, **actively** refactor to rule-engine-driven architecture.
- Upon seeing non-deterministic APIs, **actively** replace with deterministic alternatives.

### 9.4 Correct Process for Adding New Rules (LLM MUST READ)

1. **Determine rule type**: Business rule or meta-rule?
2. **Design Domain**: First design the matching conditions.
3. **Design Transform**: Design the execution actions.
4. **Write JSON**: Follow format specification (§4).
5. **Validate format**: Ensure JSON Schema compliance.
6. **Test**: Run tests to verify correctness.

### 9.5 Correct Process for Modifying Existing Rules

1. **Modify only JSON files**: Do NOT modify Rust source code.
2. **Keep rule_id unchanged**: If rule_id must change, create a new rule.
3. **Verify compatibility**: Ensure modifications don't break existing functionality.
4. **Update documentation**: If rule semantics change, update relevant docs.

---

## 10. Migration & Refactoring Process

> **Source**: Chapter 8 of Doc 31 + Chapter 4 of Doc 32.

### 10.1 Standard Process for Migrating from Deprecated Rust to JSON Rules

```
【Phase 1】Assessment
│
├─ Locate the JSON rule file corresponding to the deprecated Rust type
├─ Assess JSON rule completeness (skeleton / partial / complete)
└─ Assess all call sites (grep all uses of the type)
│
【Phase 2】Complete JSON Rules (if incomplete)
│
├─ Express business logic using instruction_sequence + evaluate_domain + while_loop
├─ Define trigger conditions (domain field)
├─ Define output key (result_key)
└─ Write unit tests to verify rule behavior
│
【Phase 3】Migrate Call Sites
│
├─ Change "direct Rust type call" to "construct State + invoke engine + read result key"
├─ Migrate call sites one by one, running tests after each
└─ Ensure behavioral equivalence (equivalence testing)
│
【Phase 4】Remove Deprecated Rust Type
│
├─ After confirming no call sites remain, remove the Rust type definition
├─ Remove related tests
└─ Run full test suite to verify
```

### 10.2 Equivalence Test Template

Behavioral equivalence MUST be verified before and after migration. Template:

```rust
#[test]
fn test_migration_equivalence() {
    let rules = load_test_rules();
    let input_state = build_test_input();

    // Old path (deprecated Rust)
    let old_result = DeprecatedType::new().do_something(&input_state);

    // New path (rule-engine driven)
    let mut state = input_state.into_state();
    state = state.set("__namespace__.action_requested", Value::Bool(true));
    let engine = ForwardChain::new(1000);
    let new_result = engine.infer(state, &rules);
    let new_value = new_result.final_state.get("__namespace__.result").unwrap();

    // Assert equivalence
    assert_eq!(old_result.to_value(), *new_value);
}
```

### 10.3 Handler Refactoring Priority

Refactor handlers in the following priority order:

1. **High priority**: Handlers directly calling deprecated Rust business types.
2. **Medium priority**: Handlers with business logic hardcoded.
3. **Low priority**: Handlers doing only I/O adaptation (mostly compliant).

### 10.4 Fix Execution Workflow (Condensed from Chapter 4 of Doc 32)

```
P1 (Immediate): High-severity violations — deprecated business types called by new code
   ↓
P2 (This sprint): Medium-severity — non-deterministic APIs + handlers calling business logic directly
   ↓
P3 (Ongoing): Low-severity — JSON rule skeletons / documentation inconsistencies
   ↓
Each P step: Assess → Complete rules → Migrate call sites → Remove old code → Test verification
```

---

## 11. Troubleshooting

> **Source**: Chapter 5 of LLM_GUIDE.

### 11.1 Common Error Messages

| Error Message                       | Cause                                 | Solution                                                    |
| ----------------------------------- | ------------------------------------- | ----------------------------------------------------------- |
| `Unknown instruction type: if_else` | Used non-existent primitive           | Use `evaluate_domain` instead                               |
| `Failed to parse domain`            | Domain format error                   | Check JSON structure (§4.2)                                 |
| `Circular dependency detected`      | Rules have circular dependencies      | Check rule dependency graph                                 |
| `Non-deterministic operation`       | Attempted non-deterministic operation | Remove randomness or time dependence, use §7.2 alternatives |

### 11.2 Debugging Techniques

1. **View rule index**: `crates/governance/rules/index.json`
2. **Check rule format**: `crates/governance/rules/RULE_ATLAS.md`
3. **Run tests**: `cargo test`
4. **Verify determinism**: `cargo test --test deterministic`
5. **TCB compliance check**: `cargo test --test tcb_compliance`
6. **Rule format validation**: `cargo run --bin rule_validator`
7. **Scan for paradigm violations**: `tools/scan_violations.ps1` (Appendix C script)

---

## 12. Automated Detection & Quality Assurance

> **Source**: DO_AND_DONT §6 + Chapters 3/5 of Doc 32 (scripts moved to Appendix C, workflow retained here).

### 12.1 Three-Layer Detection System

| Layer                    | Method                   | Use Case                  | Tool/Script             |
| ------------------------ | ------------------------ | ------------------------- | ----------------------- |
| **L1 Static Scan**       | grep / Select-String     | Pre-commit quick check    | Appendix C.1 (ps1/bash) |
| **L2 Automated Tool**    | Cargo tool               | CI integration            | Appendix C.2 (Rust)     |
| **L3 Compiler Warnings** | `#[deprecated]` + clippy | Compile-time interception | Appendix C.3            |

### 12.2 Detection Dimensions (Aligned with §3 Mapping Table)

- **V-01**: Deprecated type usage detection (see §3 mapping table)
- **V-02**: Handler directly calling business modules detection
- **V-03**: Non-deterministic API detection (see §7.2)
- **V-04**: JSON rule skeleton detection (transform empty / instructions too short)

### 12.3 Pre-Commit Compliance Checklist

```markdown
## Mandatory Items (ALL must pass before commit)

- [ ] No calls to any `#[deprecated]` Rust business types (V-01)
- [ ] Handlers do NOT directly call business modules, only trigger rule engine (V-02)
- [ ] No `SystemTime::now()` / `Uuid::new_v4()` / `rand::random()` used (V-03)
- [ ] No new `unsafe` code added (ER-603)
- [ ] JSON rule transform is NOT an empty skeleton (V-04)
- [ ] No new external dependencies introduced to TCB (ER-602)
- [ ] No modifications to TCB core files (§2.2 list)

## Recommended Items (suggested to check)

- [ ] Unit tests cover all domain branches of new rules
- [ ] Equivalence tests verify deprecated→JSON migration
- [ ] Priority is at correct layer (Layer 0/0.5/1/2)
- [ ] rule_id follows `{namespace}.{category}.{name}` format
- [ ] Corresponding JSON rule documentation updated (RULE_ATLAS.md)
```

### 12.4 Periodic Audit Process

```bash
# Run full scan weekly (Appendix C.1 / C.2)
# Compare with last week's report, track changes

# Monthly migration progress report
## Deprecated type migration progress
- Remaining: N locations
- Migrated this month: M locations
- Fix progress: X deprecated Rust types removed

## New violations
- New this month: N locations
- Fixed: M locations

## Next month plan
- P1: Zero high-severity violations
- P2: Reduce medium-severity by half
```

### 12.5 Violation Tracking System (Template)

```markdown
# EvoRule Paradigm Violation Tracking

## V-01 Tracking (deprecated types)

### DimensionChecker::new()

- Status: Pending migration
- Call sites:
    - crates/governance/src/handlers/dimension.rs:42
    - crates/science_platform/src/handlers/dimension.rs:18
- Plan: Migrate to rules/inference/dimension_check.json
- Owner: @xxx
- Deadline: 2026-xx-xx

### BackwardChainer::new()

- Status: ...

## V-03 Tracking (non-deterministic APIs)

### SystemTime::now()

- Status: ...
```

---

## 13. Three Maxims

> **Source**: Chapter 10 of Doc 31.

### Maxim 1: Paradigm Before Code

> Before writing any code, ask yourself: "Does this conform to the rule-driven architecture?" not "Does this work?"

Code that works is not necessarily code that conforms to the paradigm. Every rework in EvoRule's history originated from the "get it working first" compromise.

### Maxim 2: JSON Rules First

> All business logic MUST reside in JSON rules. Rust is the base, not the business carrier.

When you find yourself writing business logic in Rust, stop and ask: "Can this be expressed in JSON rules?" The answer is almost always "yes."

### Maxim 3: Determinism is Non-Negotiable

> Any "just this once" non-determinism is a betrayal of the constitutional commitment.

Determinism is EvoRule's raison d'être. Without determinism, EvoRule is just another rule engine. Between "just use `SystemTime::now()` once" and "use `LogicalClock::current_tick()`" — always choose the latter.

---

## Appendix A: Quick Decision Flowchart

> **Source**: Appendix A of Doc 31.

```
You need to write some code...

│
├─ Is it a TCB primitive? (state_set / evaluate_domain / while_loop / content_hash...)
│   └─ Yes → Implement in TCB layer (requires change review; TCB is frozen)
│
├─ Is it an I/O channel? (file / env / http / database)
│   └─ Yes → Register channel in Governance io/; JSON rules call via io_read
│
├─ Is it business logic?
│   │
│   ├─ Does a JSON rule already exist?
│   │   └─ Yes → Use rule engine path (construct State + call ForwardChain + read result key)
│   │
│   ├─ Is the JSON rule only a skeleton?
│   │   └─ Complete the JSON rule, then use rule engine path
│   │
│   └─ No corresponding JSON rule?
│       └─ Write JSON rule, then use rule engine path
│
├─ Is it a Web/CLI/Python handler?
│   └─ Yes → Only do input parsing + trigger rule engine + output serialization — NO business logic
│
└─ Is it a deterministic primitive extension?
    └─ Yes → Register in Governance via registry.register(); JSON rules call it
```

---

## Appendix B: Glossary

> **Source**: Appendix B of Doc 31 + DO_AND_DONT implicit terminology.

| Term                       | Definition                                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------- |
| **TCB**                    | Trusted Computing Base — EvoRule's trust foundation                                               |
| **Governance**             | Governance layer — provides rule management / I/O channels / high-level execution engines         |
| **Rule-Driven**            | Business logic expressed in JSON rules, executed by the rule engine                               |
| **Determinism**            | Same input forever produces same output                                                           |
| **LogicalClock**           | Logical clock — deterministic timestamp replacing wall-clock time                                 |
| **content_hash**           | Content hash — generates deterministic IDs based on content                                       |
| **deprecated**             | In EvoRule, specifically means "business logic has migrated to JSON rules"                        |
| **Primitive**              | Minimum computational unit provided by TCB (state_set / evaluate_domain / etc.)                   |
| **Trigger Flag**           | Boolean field in State used to trigger JSON rules (e.g., `__inference__.forward_chain_requested`) |
| **Result Key**             | Field in State storing rule execution results (e.g., `__inference__.result`)                      |
| **Constitutional Redline** | Inviolable architecture constraints (ER-600 to ER-606)                                            |
| **Paradigm Misalignment**  | Writing EvoRule code with traditional programming habits, violating rule-driven architecture      |
| **Skeleton**               | JSON rule transform empty or returns only constant strings — business logic not implemented       |

---

## Appendix C: Detection Scripts (ps1 / bash / rust / IDE)

> **Source**: Chapter 3 of Doc 32 (scripts moved here, Doc 32 retains explanatory text for that chapter; scripts centralized here for easy reference).
> **Note**: These scripts can be copied directly to the `tools/` directory.

### C.1 Static Scan Scripts

#### C.1.1 Deprecated Type Usage Detection

**Windows PowerShell**:

```powershell
# Detect deprecated type instantiation calls
$deprecatedTypes = @(
    "DimensionChecker",
    "BackwardChainer",
    "ConvergenceChecker",
    "InformationGainCalculator",
    "Planner",
    "EffectPredictor",
    "CycleDetector",
    "ConflictDetector",
    "SolverValidator",
    "SelfCheckConfig",
    "RedlineChecker",
    "ConstitutionalGate",
    "PipelineBuilder",
    "ConfigDrivenStrategy",
    "MetaExecutor",
    "AdaptiveMeta",
    "InjectPruneExecutor"
)

foreach ($type in $deprecatedTypes) {
    Write-Host "Detecting $type::new() calls..." -ForegroundColor Yellow
    Select-String -Path "crates\**\*.rs" -Pattern "$type::new\(\)" -CaseSensitive
}
```

**Linux/macOS Bash**:

```bash
#!/bin/bash
deprecated_types=(
    "DimensionChecker"
    "BackwardChainer"
    "ConvergenceChecker"
    "InformationGainCalculator"
    "Planner"
    "EffectPredictor"
    "CycleDetector"
    "ConflictDetector"
    "SolverValidator"
    "SelfCheckConfig"
    "RedlineChecker"
    "ConstitutionalGate"
    "PipelineBuilder"
    "ConfigDrivenStrategy"
    "MetaExecutor"
    "AdaptiveMeta"
    "InjectPruneExecutor"
)

for type in "${deprecated_types[@]}"; do
    echo "Detecting $type::new() calls..."
    grep -rn "$type::new()" crates/ --include="*.rs"
done
```

#### C.1.2 Non-Deterministic API Detection

```powershell
$nonDeterministicAPIs = @(
    "SystemTime::now\(\)",
    "Instant::now\(\)",
    "Uuid::new_v4\(\)",
    "rand::random\(\)",
    "thread_rng\(\)",
    "process::id\(\)",
    "env::var\([^)]+\)"
)

Write-Host "Detecting non-deterministic APIs..." -ForegroundColor Red
foreach ($api in $nonDeterministicAPIs) {
    Write-Host "  Detecting $api ..." -ForegroundColor Yellow
    Select-String -Path "crates\**\*.rs" -Pattern $api
}
```

#### C.1.3 Handler Violation Detection

```powershell
$handlerPath = "crates\governance\src\handlers"
$businessModules = @(
    "DimensionChecker",
    "BackwardChainer",
    "ForwardChain",
    "Planner",
    "EffectPredictor"
)

Write-Host "Detecting handler violations..." -ForegroundColor Red
foreach ($module in $businessModules) {
    Write-Host "  Detecting $module ..." -ForegroundColor Yellow
    Select-String -Path "$handlerPath\*.rs" -Pattern $module
}
```

#### C.1.4 Comprehensive Scan Script (tools/scan_violations.ps1)

```powershell
# tools/scan_violations.ps1
# EvoRule paradigm violation scanner
# Usage: .\tools\scan_violations.ps1 [-Output report.json]

param(
    [string]$Output = "violation_report.md"
)

$projectRoot = "D:\evorule-project\evorule-v4"
$cratesPath = "$projectRoot\crates"

$violations = @{
    "V-01_deprecated_types" = 0
    "V-02_handler_direct_call" = 0
    "V-03_non_deterministic_api" = 0
    "V-04_json_rule_skeleton" = 0
}

Write-Host "=== EvoRule Paradigm Violation Scan ===" -ForegroundColor Cyan
Write-Host "Project root: $projectRoot" -ForegroundColor Gray
Write-Host ""

# 1. Deprecated type detection
Write-Host "[V-01] Detecting deprecated type usage..." -ForegroundColor Yellow
$deprecatedTypes = @("DimensionChecker", "BackwardChainer", "Planner", "ConvergenceChecker")
foreach ($type in $deprecatedTypes) {
    $matches = Select-String -Path "$cratesPath\**\*.rs" -Pattern "$type::new\(\)" -Quiet
    if ($matches) {
        $violations["V-01_deprecated_types"]++
        Write-Host "  Found $type::new() call" -ForegroundColor Red
    }
}

# 2. Non-deterministic API detection
Write-Host "[V-03] Detecting non-deterministic APIs..." -ForegroundColor Yellow
$apis = @("SystemTime::now", "Uuid::new_v4", "rand::random")
foreach ($api in $apis) {
    $matches = Select-String -Path "$cratesPath\**\*.rs" -Pattern $api -Quiet
    if ($matches) {
        $violations["V-03_non_deterministic_api"]++
        Write-Host "  Found $api call" -ForegroundColor Red
    }
}

# 3. Handler violation detection
Write-Host "[V-02] Detecting handler violations..." -ForegroundColor Yellow
$handlerPath = "$cratesPath\governance\src\handlers"
if (Test-Path $handlerPath) {
    $businessModules = @("DimensionChecker", "BackwardChainer::chain", "Planner::plan")
    foreach ($module in $businessModules) {
        $matches = Select-String -Path "$handlerPath\*.rs" -Pattern $module -Quiet
        if ($matches) {
            $violations["V-02_handler_direct_call"]++
            Write-Host "  Found handler calling $module" -ForegroundColor Red
        }
    }
}

Write-Host ""
Write-Host "=== Scan Results Summary ===" -ForegroundColor Cyan
foreach ($key in $violations.Keys) {
    $count = $violations[$key]
    $color = if ($count -gt 0) { "Red" } else { "Green" }
    Write-Host "  $key : $count violation(s)" -ForegroundColor $color
}

$reportContent = @"
# EvoRule Paradigm Violation Scan Report

**Scan time**: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
**Project path**: $projectRoot

## Violation Statistics

| Type | Count |
|-----|------|
| deprecated type usage | $($violations["V-01_deprecated_types"]) |
| handler direct business call | $($violations["V-02_handler_direct_call"]) |
| non-deterministic API usage | $($violations["V-03_non_deterministic_api"]) |

## Next Actions

Recommend fixing in priority order P1 → P2 → P3.
"@

$reportContent | Out-File -FilePath $Output -Encoding UTF8
Write-Host "Report saved to: $Output" -ForegroundColor Green
```

### C.2 Automated Detection Tool (Rust)

> Full Cargo tool source (deprecated.rs / nondeterministic.rs / skeleton.rs) see §3.2 of Doc 32, ~200 lines, omitted here. Tool directory structure:

```
tools/violation_checker/
├── Cargo.toml
├── src/
│   ├── main.rs          # Entry point
│   ├── detectors/
│   │   ├── deprecated.rs    # Deprecated type detection
│   │   ├── nondeterministic.rs  # Non-deterministic API detection
│   │   ├── handler.rs       # Handler violation detection
│   │   └── skeleton.rs      # JSON rule skeleton detection
│   ├── models.rs        # Violation data models
│   └── report.rs        # Report generation
└── tests/
    └── integration_tests.rs
```

**Build & Run**:

```bash
cd tools/violation_checker
cargo build --release

# Run scan
./target/release/violation_checker --project-root D:/evorule-project/evorule-v4 --output violation_report.md

# Integrate with CI
cargo run -- --project-root . --format json --output violations.json
```

### C.3 Compiler Warning Enhancement

```rust
// Example: DimensionChecker
#[deprecated(
    since = "0.5.0",
    note = "⚠️ Paradigm violation warning: This type has migrated to JSON rule rules/inference/dimension_check.json.

Calling this type violates rule-driven architecture (Spec Rule B).

Correct approach:
1. Construct State, set __inference__.dimension_check_requested = true
2. Drive rule engine via ForwardChain::infer()
3. Read __inference__.dimension_result from result State

DO NOT use this type in new code."
)]
pub struct DimensionChecker { ... }
```

### C.4 IDE Integration (VS Code)

`.vscode/tasks.json`:

```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "Scan Paradigm Violations",
            "type": "shell",
            "command": "powershell",
            "args": [
                "-File",
                "${workspaceFolder}/tools/scan_violations.ps1",
                "-Output",
                "${workspaceFolder}/violation_report.md"
            ],
            "problemMatcher": [],
            "presentation": {
                "echo": true,
                "reveal": "always",
                "focus": false,
                "panel": "shared"
            },
            "group": {
                "kind": "build",
                "isDefault": false
            }
        },
        {
            "label": "Detect Deprecated Types",
            "type": "shell",
            "command": "Select-String",
            "args": [
                "-Path",
                "${workspaceFolder}/crates/**/*.rs",
                "-Pattern",
                "DimensionChecker::new|BackwardChainer::new|Planner::new"
            ],
            "problemMatcher": []
        }
    ]
}
```

`.vscode/keybindings.json`:

```json
[
    {
        "key": "ctrl+shift+v",
        "command": "workbench.action.tasks.runTask",
        "args": "Scan Paradigm Violations"
    }
]
```

---

## Appendix D: Source Document Cross-Reference

| This Document Section        | Source Document                                                        |
| ---------------------------- | ---------------------------------------------------------------------- |
| §1 Paradigm Awareness        | Chapter 1 of Doc 31 + LLM_GUIDE §1.1 + DO_AND_DONT implicit principles |
| §2 Responsibility Boundaries | Chapter 2 of Doc 31 + DO_AND_DONT §5 + LLM_GUIDE §4.3 + LLM_GUIDE §1.2 |
| §3 Deprecated                | Chapter 3 of Doc 31 + Chapter 2 of Doc 32 (Violation Classification)   |
| §4 JSON Rule Format          | Chapter 2 of LLM_GUIDE + DO_AND_DONT §2/§3/§4                          |
| §5 Decision Framework        | Chapter 4 of Doc 31 + §6.1 of Doc 32                                   |
| §6 Handler                   | Chapter 5 of Doc 31 + DO_AND_DONT §2.1/§2.2                            |
| §7 Determinism               | Chapter 6 of Doc 31 + DO_AND_DONT §1 + LLM_GUIDE §1.3                  |
| §8 Anti-Patterns             | §4.3 of Doc 31 + DO_AND_DONT §4 + Chapter 3 of LLM_GUIDE               |
| §9 LLM Guidelines            | Chapter 7 of Doc 31 + LLM_GUIDE §1.3/Chapter 3                         |
| §10 Migration                | Chapter 8 of Doc 31 + Chapter 4 of Doc 32                              |
| §11 Troubleshooting          | Chapter 5 of LLM_GUIDE                                                 |
| §12 Automated Detection      | DO_AND_DONT §6 + Chapters 3/5 of Doc 32                                |
| §13 Three Maxims             | Chapter 10 of Doc 31                                                   |
| Appendix A Flowchart         | Appendix A of Doc 31                                                   |
| Appendix B Glossary          | Appendix B of Doc 31 + DO_AND_DONT                                     |
| Appendix C Scripts           | Chapter 3 of Doc 32                                                    |
| Appendix D Cross-Reference   | This merge's traceability                                              |

---

**End of Document — Part 2**

> This specification will be continuously updated as EvoRule evolves and real-world lessons accumulate. Every rework should prompt reflection: "Could this rework have been avoided by a rule in this document?" If yes, please add it to the corresponding section.

```

```
