**EvoRule-TCB · LLM Context Injection Document**

> **Version**: 1.0 | **Created**: 2026-07-05
> **Purpose**: Single entry point for AI/LLM when first engaging with EvoRule-TCB.
> **Design Goal**: Give an LLM all necessary information to "start working" within 200 lines.
> **Maintenance Principle**: This document is a **navigation hub + primitive quick reference + end‑to‑end example**. It does **not** replicate the full *Programming Standards Encyclopedia*.

---

## 0. I am an LLM – What Should I Read?

### 0.1 One‑Sentence Understanding of EvoRule‑TCB

> **EvoRule‑TCB is the deterministic execution foundation of EvoRule**: given the same input it always produces the same output. It implements **no business logic** – it provides only **22 atomic primitives** + **1 self‑driven loop (`while_loop`)** + **1 dispatcher (`dispatch`)**. Business logic is orchestrated entirely by JSON rules.

### 0.2 Three‑Layer Mental Model

```
┌──────────────────────────────────────────────────────┐
│  JSON Rule Layer   ← all business logic (human/AI editable) │
│  ────────────────────────────────────────────────    │
│  Governance Layer  ← rule loading / I/O channels / execution framework │
│  ────────────────────────────────────────────────    │
│  TCB Layer (this doc) ← 22 primitives + while_loop + audit chain │
└──────────────────────────────────────────────────────┘
```

**Iron Law**: Upper layers depend on lower layers; the lower layer **never** depends on upper layers. The TCB knows nothing about “business”.

### 0.3 Required Reading (Sorted by LLM Priority)

| Priority | Document | When to Read |
| -------- | -------- | ------------ |
| P0 | **This document** | Before every EvoRule task |
| P0 | [`lib.rs` module docs](../crates/tcb/src/lib.rs) | To understand the layered architecture |
| P1 | [EvoRule Programming Standards Encyclopedia](spec/en-US/EvoRule_programming_Spec.md) | Before writing handlers / editing JSON rules |
| P1 | [TCB_Governance_Contract.md](spec/en-US/TCB_Governance_Contract.md) | When needing `__exec__` field conventions |
| P2 | [EvoRule Determinism Standard](spec/en-US/EvoRule_Determinism_Standard.md) | When reasoning about determinism boundaries |
| P2 | [GATES.md](gate/GATES.md) | For pre‑commit paradigm gate checks |
| P3 | [design-decisions/](developer-guide/design-decisions/) | When needing to understand *why* |

---

## 1. What I Can / Cannot Do

### 1.1 Constitutional Red Lines (ER‑600 Series, Inviolable)

| Red Line | Prohibited Behaviour | Common LLM Pitfalls |
| -------- | -------------------- | ------------------- |
| **ER‑600** | Add non‑deterministic operations to the TCB | Using `SystemTime::now()` / `Uuid::new_v4()` / `rand::random()` / iterating over `HashMap` |
| **ER‑601** | Add LambdaDomain / Callable transforms | Using `{"$func": ...}` / `{"$eval": ...}` in JSON |
| **ER‑602** | Import governance or external crates into TCB | `use crate::governance::...` appearing in the `tcb` crate |
| **ER‑603** | Use unsafe code | Any `unsafe {}` block |
| **ER‑604** | Use an `if_else` control‑flow primitive | **This primitive does not exist** – use `evaluate_domain` + `on_true`/`on_false` |
| **ER‑605** | Modify core TCB source files | See “Frozen TCB File List” below |
| **ER‑606** | Use `$func` in JSON | Use concrete values or `$ref` |

### 1.2 Frozen TCB File List (Absolutely Forbidden to Modify)

```
crates/tcb/src/lib.rs
crates/tcb/src/value.rs
crates/tcb/src/state.rs
crates/tcb/src/domain.rs
crates/tcb/src/rule.rs
crates/tcb/src/exec_context.rs
crates/tcb/src/deterministic.rs
crates/tcb/src/error.rs
crates/tcb/src/audit.rs
crates/tcb/src/primitive/*       ← 22 primitive implementations
crates/tcb/src/control/*         ← while_loop / dispatch
crates/tcb/src/instruction/*     ← instruction registry
```

**Exception**: Bug fixes require a “constitutional change approval” and must be accompanied by regression tests.

### 1.3 Non‑Deterministic API Replacements (LLM Must Memorise)

| Forbidden | Intended Use | Replacement |
| --------- | ------------ | ----------- |
| `SystemTime::now()` | current time | `LogicalClock::current_tick()` / `__temporal__.tick` |
| `Uuid::new_v4()` | unique ID | `content_hash(&content)` |
| `rand::random()` | random number | `DeterministicRNG::from_seed(seed)` |
| `Instant::now()` | measuring duration | Should not be used in business logic |
| `process::id()` | process ID | Must not be used as a logical input |
| `env::var()` directly | environment variable | Snapshot into `State.__env__` at startup |
| iterating over `HashMap` | collection traversal | Use `BTreeMap` or sort before traversal |

---

## 2. Primitive API Quick Reference (Core Value)

> **Complete List**: 21 physical primitives + 1 control‑flow primitive (`while_loop`) + 1 dispatcher (`dispatch`) = **23 primitives**.
> **Generic Signature**: `fn(reg: &InstructionRegistry, state: &State, instr: &GenericInstruction) -> Result<State, EvoRuleError>`
> **Universal Rule**: All primitives are **L1 deterministic** pure functions – same input ⇒ same output, no side effects.

### 2.1 state_ops (State Operations, 3)

#### `state_set` — Pure Assignment

- **Params**: `attr` (String, supports `$ref`), `value` (any, supports `$ref`)
- **Behaviour**: Writes `value` to `attr`; missing `value` defaults to `Null`
- **Silent fallthrough**: if `attr` is not a string, returns the original state unchanged
- **Example**:

```json
{ "type": "state_set", "params": { "attr": "x", "value": 42 } }
{ "type": "state_set", "params": { "attr": "user.name", "value": {"$ref": "input.name"} } }
```

#### `state_compute` — Compute + Assign

- **Params**: `attr` (String), `operation` (String), `value` (any)
- **Operation values**: `add` / `sub` / `mul` / `div` / `append` / `remove` / `length`
- **Note**: **Does not support** `"set"` – use `state_set` for that.
- **Arithmetic semantics**: saturating arithmetic – never panics.
- **List semantics**: `append` appends, `remove` removes first match, `length` returns length of `value` list.
- **Examples**:

```json
{ "type": "state_compute", "params": { "attr": "count", "operation": "add", "value": 1 } }
{ "type": "state_compute", "params": { "attr": "items", "operation": "append", "value": "new" } }
{ "type": "state_compute", "params": { "attr": "n", "operation": "length", "value": {"$ref": "items"} } }
```

#### `set_context` — Compatibility Alias (not recommended for new code)

- **Params**: `transform` (Object, containing `attr` / `operation` / `value`)
- **Behaviour**: if `operation == "set"`, dispatches to `state_set`; otherwise to `state_compute`.
- **LLM Advice**: New code should use `state_set` / `state_compute` directly; avoid `set_context`.

### 2.2 queue_ops (Queue Operations, 4)

#### `advance_instruction` — Advance Instruction Pointer

- **Params**: none (reads context entirely from `__exec__`)
- **Behaviour**: if queue non‑empty, pop the front as the new current instruction; if empty, inject `default_instruction` (usually `noop`).
- **Termination detection**: after injecting `default_instruction`, check `termination_domain`.
- **Example**: `{ "type": "advance_instruction", "params": {} }`

#### `push_instruction` — Push a Single Instruction

- **Params**: `instruction` (Object, must contain a `type` field)
- **Behaviour**: appends to the end of the queue; does not execute immediately.
- **Startup validation**: if `__exec__.dispatch_cases` exists, verify that `type` is registered.
- **Example**:

```json
{
    "type": "push_instruction",
    "params": {
        "instruction": {
            "type": "state_set",
            "params": { "attr": "x", "value": 1 }
        }
    }
}
```

#### `push_instruction_sequence` — Push a Sequence of Instructions

- **Params**: `instructions` (List, must be non‑empty)
- **Atomicity (FLOW‑04)**: all validations complete before enqueue; if any fails, none are enqueued.
- **Example**:

```json
{
    "type": "push_instruction_sequence",
    "params": {
        "instructions": [
            { "type": "state_set", "params": { "attr": "x", "value": 1 } },
            { "type": "state_set", "params": { "attr": "y", "value": 2 } }
        ]
    }
}
```

#### `instruction_sequence` — Execute a Sequence In‑Line (Core Orchestration Primitive)

- **Params**: `instructions` (List)
- **Behaviour**: executes directly (no queue enqueue); the output `State` of one becomes the input of the next.
- **Failure semantics**: fail‑fast – if any instruction fails, stop immediately and return the error.
- **Example**:

```json
{
    "type": "instruction_sequence",
    "params": {
        "instructions": [
            { "type": "state_set", "params": { "attr": "x", "value": 1 } },
            { "type": "state_set", "params": { "attr": "y", "value": 2 } }
        ]
    }
}
```

### 2.3 domain_ops (Domain Operations, 3)

#### `evaluate_domain` — Domain Evaluation + Branch (The Only Correct Way for Conditional Branching)

- **Params**: `domain` (Object, required), `on_true` (Instruction, optional), `on_false` (Instruction, optional)
- **Behaviour**: evaluate `domain` → if true, push `on_true` to the front of the queue; if false, push `on_false`; missing branch means nothing is pushed.
- **Side effects**: writes to `__exec__.__trace_branch` (value `"then"`/`"else"`/`"none"`), and writes `__domain_result__`.
- **Startup validation**: if `__exec__.dispatch_cases` exists, verify that branch instruction types are registered.
- **Example**:

```json
{
    "type": "evaluate_domain",
    "params": {
        "domain": { "type": "atom", "attribute": "x", "op": "eq", "value": 42 },
        "on_true": {
            "type": "state_set",
            "params": { "attr": "msg", "value": "matched" }
        },
        "on_false": {
            "type": "state_set",
            "params": { "attr": "msg", "value": "no" }
        }
    }
}
```

#### `match_domain` — Domain Match (No Branch, Only Records Result)

- **Params**: `domain` (required), `state_ref` (optional `$ref`), `result_attr` (optional, default `"__matched__"`)
- **Behaviour**: evaluates `domain` and writes the boolean result to `result_attr`.
- **Use case**: when you need “does it match?” but do not want to branch immediately.
- **Example**:

```json
{
    "type": "match_domain",
    "params": {
        "domain": { "type": "atom", "attribute": "x", "op": "eq", "value": 42 },
        "result_attr": "x_is_42"
    }
}
```

#### `domain_intersect` — Domain Intersection Check (Conservative)

- **Params**: `domain1` (required), `domain2` (required), `result_attr` (required)
- **Semantics**: `(universal, *)` ⇒ true; `(empty, *)` ⇒ false; `(atom, atom)` ⇒ same attr; `(not, X)` ⇒ true (conservative); default true.
- **Use case**: rule scheduler deciding whether two rules can fire simultaneously.

### 2.4 rule_ops (Rule Operations, 4)

#### `apply_rule` — Apply a Single Rule

- **Params**: `rule` (Object, inline rule) or `rule_id` (String, looked up in `__universe_rules__`), `result_attr` (optional)
- **Behaviour**: extracts the rule’s `transform` and executes it; if the rule is not found, returns the original state.
- **Example**:

```json
{ "type": "apply_rule", "params": { "rule_id": "physics.force.gravity" } }
```

#### `observe_rules` — Observe the Rule Set

- **Params**: `result_attr` (optional, default `"rules_list"`)
- **Behaviour**: reads `__universe_rules__` and writes it to `result_attr`; if absent, writes an empty list.

#### `filter_rules` — Filter Rules by Attributes

- **Params**: `rules_ref` (optional, default `"__universe_rules__"`), `goal_attrs` (List<String>), `result_attr` (optional, default `"filtered_rules"`)
- **Behaviour**: returns rules whose domain references any attribute in `goal_attrs`; a `universal` domain is treated as referencing all attributes.

#### `inject_rule` — Inject / Remove / Replace Rules (For Meta‑Rules)

- **Params**: `rules_key` (optional, default `"__universe_rules__"`), `remove_rule_id` (optional), `add_rule` (optional), `result_attr` (optional, default `"__inject_result__"`)
- **Result structure**: `{ removed_count, replaced_count, added_count, total_rules }`
- **Important**: `removed_count` reflects the **actual** number removed (BUG‑06 fix semantics); providing both `remove_rule_id` and `add_rule` is treated as a replacement, with `replaced_count = 1`.
- **Example**:

```json
{ "type": "inject_rule", "params": {
    "remove_rule_id": "old.rule",
    "add_rule": { "rule_id": "new.rule", "domain": {...}, "transform": {...} }
} }
```

### 2.5 compute_ops (Computation Operations, 3)

#### `content_hash` — SHA‑256 Content Hash (The Correct Replacement for `Uuid::new_v4`)

- **Params**: `keys` (optional, List<String>; empty ⇒ hash entire State), `store_as` (optional, default `"__hash__"`)
- **Behaviour**: non‑String keys are filtered; missing keys use `Value::Null`; outputs a 64‑character hex string.
- **Example**:

```json
{ "type": "content_hash", "params": { "keys": ["x", "y"], "store_as": "hash" } }
```

#### `format_string` — Template String

- **Params**: `template` (String, containing `{key}` placeholders), `store_as` (optional, default `"formatted"`), `source` (optional, overrides state as value source)
- **Strict retention red line**: **Only `Value::String` types are substituted**; other types (Integer/Bool/List/Null/missing) **keep the placeholder verbatim**.
- **Example**:

```json
{
    "type": "format_string",
    "params": {
        "template": "Hello {name}, score={score}",
        "store_as": "message"
    }
}
```

(If `name="Alice"` (String) and `score=42` (Integer), output is `"Hello Alice, score={score}"`.)

#### `get_index` — List Index Access

- **Params**: `ref` (String, `$ref` pointing to a list), `index` (Integer, supports negative), `result_attr` (optional, default `"item"`)
- **Behaviour**: out‑of‑bounds / not a list ⇒ `Value::Null`; `-1` ⇒ last element.
- **Example**:

```json
{
    "type": "get_index",
    "params": {
        "ref": "items",
        "index": 0,
        "result_attr": "first_item"
    }
}
```

### 2.6 audit_ops (Audit Operations, 1)

#### `trace_step` — Audit Trail Step

- **Params**: `label` (optional, default `"step"`), `rule_id` (optional, default taken from `__exec__.instruction.type`)
- **Behaviour**: controlled by `__exec__.audit_on` switch; constructs an `AuditRecord` and HMAC‑SHA256 signs it, appending to `__audit_chain`.
- **Important**: the audit chain signs all of: `id/nonce/rule_id/timestamp/state_before_hash/state_after_hash/previous_hash/execution_result/change_summary/error_message/version` (BUG‑03 fix now includes `version`).
- **Example**:

```json
{ "type": "trace_step", "params": { "label": "computed result" } }
```

### 2.7 error_ops (Error Handling, 2)

#### `execute_try_catch` — Try‑Catch

- **Params**: `try` (Instruction, required), `catch` (Instruction, optional), `error_attr` (optional, default `"__error__"`)
- **Behaviour**: if `try` succeeds, return its result; if it fails, write the error to `error_attr` and execute `catch` (if present); if `try` is missing, return the original state.
- **Example**:

```json
{
    "type": "execute_try_catch",
    "params": {
        "try": { "type": "apply_rule", "params": { "rule_id": "risky.rule" } },
        "catch": {
            "type": "state_set",
            "params": { "attr": "fallback", "value": true }
        }
    }
}
```

#### `execute_parallel` — Parallel Execution of Multiple Branches

- **Params**: `branches` (List<Instruction>), `merge` (optional, `"last_wins"` default / `"force"`), `error_strategy` (optional, `"fail_fast"` default / `"continue"`)
- **Merge strategies**:
    - `last_wins`: returns the state of the last successful branch.
    - `force`: merges all branch modifications, later writers overwrite (**determinism enforced**: collect modified fields lexicographically, determine last writer by branch_idx ascending; BUG‑07 fix ensures ER‑601).
- **Provenance**: in `"force"` mode, writes `__parallel_provenance__` recording the source branch for each field.
- **Example**:

```json
{
    "type": "execute_parallel",
    "params": {
        "branches": [
            { "type": "state_set", "params": { "attr": "a", "value": 1 } },
            { "type": "state_set", "params": { "attr": "b", "value": 2 } }
        ],
        "merge": "force"
    }
}
```

### 2.8 noop_ops (No‑Operation, 1)

#### `noop` — No Operation

- **Params**: none
- **Behaviour**: returns a clone of the original state; modifies no fields and enqueues no instructions.
- **Use cases**: default branch / placeholder / typical `default_instruction` when the queue is empty.
- **Example**: `{ "type": "noop", "params": {} }`

### 2.9 control_ops (Control Flow, 2)

#### `while_loop` — Self‑Driven Loop (Core TCB Execution Model)

- **Params**: `condition` (domain expression or boolean `$ref`, optional, defaults to reading `__exec__.__running`), `max_steps` (Integer, optional, default 10000, upper limit 100000), `body` (Instruction or List<Instruction>)
- **Behaviour**:
    1. Check `condition`; if false, exit.
    2. Execute `body` (each instruction is dispatched + `trace_step`).
    3. Drain the queue: meta‑instructions are executed immediately; business instructions become the new current instruction and break.
    4. Return to step 1, until `condition` false or `max_steps` exhausted.
- **Truncation flag**: when `max_steps` is exhausted, write `__exec__.__terminated_by_max_steps = true`.
- **Meta type source**: `__exec__.meta_instruction_types` (injected by `evaluate()` from `eval_config.json`, **must include `"noop"`**).
- **Example**:

```json
{
    "type": "while_loop",
    "params": {
        "condition": {
            "type": "atom",
            "attribute": "__exec__.__running",
            "op": "eq",
            "value": true
        },
        "body": [
            { "type": "dispatch" },
            { "type": "trace_step", "params": { "label": "tick" } }
        ],
        "max_steps": 1000
    }
}
```

#### `dispatch` — Instruction Dispatcher (Typically Called Inside `while_loop`)

- **Params**: none (reads the current instruction from `__exec__.instruction`)
- **Behaviour**: looks up `InstructionRegistry` → finds executor → executes; if not found, falls back to `__exec__.dispatch_cases`.
- **Side effect**: writes `__exec__.last_dispatch_hashes` (before/after state hash, used by `trace_step`).

---

## 3. End‑to‑End Execution Flow (A Complete Example)

### 3.1 Scenario: A Minimal JSON Rule + Full Execution Path

**Goal**: When `input.x == 10`, compute `result = x * 2`; otherwise `result = 0`.

**JSON Rule** (`rules/example.double_x.json`):

```json
{
    "rule_id": "example.double_x",
    "name": "Double X",
    "domain": {
        "type": "atom",
        "attribute": "__exec__.instruction.type",
        "op": "eq",
        "value": "double_x"
    },
    "transform": {
        "type": "instruction_sequence",
        "params": {
            "instructions": [
                {
                    "type": "evaluate_domain",
                    "params": {
                        "domain": {
                            "type": "atom",
                            "attribute": "input.x",
                            "op": "eq",
                            "value": 10
                        },
                        "on_true": {
                            "type": "state_compute",
                            "params": {
                                "attr": "result",
                                "operation": "mul",
                                "value": 2
                            }
                        },
                        "on_false": {
                            "type": "state_set",
                            "params": { "attr": "result", "value": 0 }
                        }
                    }
                },
                { "type": "advance_instruction" }
            ]
        }
    }
}
```

### 3.2 Execution Path (Governance Layer View)

```
[1] Handler constructs initial State
    state.input.x = 10
    state.__exec__.instruction = { "type": "double_x", "params": {} }
    state.__exec__.queue = []
    state.__exec__.__running = true
    state.__exec__.meta_instruction_types = ["noop", "advance_instruction"]
    state.__exec__.audit_on = true

[2] Handler calls engine.infer(state, &rules)
    ↓
[3] Governance finds the rule whose domain matches: example.double_x
    Pushes rule.transform into the queue
    ↓
[4] while_loop starts (step=0)
    condition = __running == true ✓
    body = [dispatch, trace_step]
    ↓
[5] dispatch: current instruction = double_x → lookup in registry
    → primitive "double_x" not found → consult dispatch_cases
    → hit (cases injected by the rule engine) → execute rule.transform
    ↓
[6] instruction_sequence executes:
    [6.1] evaluate_domain(input.x == 10)
          → true → push on_true (state_compute mul 2) to the front
          → write __domain_result__ = true, __trace_branch = "then"
    [6.2] state_compute(result, mul, 2)
          → result = input.x * 2 = 20
    [6.3] advance_instruction → queue empty → inject noop → termination check passes → __running = false
    ↓
[7] trace_step: records audit record (HMAC signed, including before/after hash)
    ↓
[8] while_loop step=1: condition = __running == false → exit
    ↓
[9] Handler reads from result.final_state:
    state.result = 20
    state.__audit_chain = [AuditRecord{...}]
```

### 3.3 Key Observations (LLM Must Read)

1. **All business logic is in JSON**: the logic “if x==10 multiply by 2, else set to 0” is **not in any Rust code**.
2. **Rust provides only primitives**: `evaluate_domain` / `state_compute` / `state_set` / `advance_instruction` are atomic operations.
3. **`while_loop` is the driver**: it does not care about business – it just does “check condition → execute body → drain queue”.
4. **Auditing happens automatically**: `trace_step` is explicitly called in the `while_loop` body; the hash chain makes execution tamper‑evident.
5. **`__exec__` is the context**: all “system fields” are prefixed with `__`; business fields must not use this prefix.

---

## 4. Decision Tree: Should I Change Rust or JSON?

```
What you want to do…
│
├─ Business logic (e.g. “how to decide dimensional consistency”)
│   │
│   ├─ Existing JSON rule is complete?
│   │   └─ Yes → go through the rule engine: construct State + set trigger flag + call engine.infer
│   │
│   ├─ JSON rule is only a skeleton?
│   │   └─ Complete the JSON rule (use instruction_sequence + evaluate_domain + while_loop)
│   │
│   └─ No matching rule exists?
│       └─ Create a new JSON rule
│
├─ Computational primitive (e.g. “SHA‑256 hash” / “take first element of list”)
│   │
│   ├─ TCB already has it? (check §2 of this doc)
│   │   └─ Yes → reuse it
│   │
│   ├─ Is it deterministic pure computation?
│   │   └─ TCB is frozen – go through change approval → add new x_ops.rs under primitive/
│   │
│   └─ Involves I/O or external systems?
│       └─ Register it in the Governance layer primitive/ and call from JSON rules
│
├─ Web/CLI handler
│   └─ Only: parse input → construct State → call engine.infer → read result → serialise output
│       Forbidden: calling DimensionChecker / BackwardChainer etc. (deprecated types) inside the handler
│
└─ TCB bug fix
    └─ Must go through constitutional change approval + regression tests + update FREEZE.md
```

---

## 5. 5 Common Paradigm Misalignments for LLMs (Self‑Check List)

Before generating EvoRule code, the LLM must ask itself:

| # | Misalignment | Self‑Check Question | Correction if “Yes” |
|---|-------------|----------------------|----------------------|
| 1 | **Shortest‑path bias** | “Am I calling a Rust function directly in the handler instead of constructing a State and invoking the engine?” | Refactor to rule‑engine‑driven |
| 2 | **Full‑feature bias** | “Because the JSON rule is a skeleton, am I filling in the missing parts with Rust?” | Complete the JSON rule first |
| 3 | **Config‑as‑rules misunderstanding** | “Am I treating JSON as a configuration file for Rust functions?” | JSON is the carrier of business logic, not configuration |
| 4 | **Deprecated misunderstanding** | “Do I treat `#[deprecated]` as a normal warning?” | In EvoRule, it is an architecture‑migration directive – do not use in new code |
| 5 | **Determinism compromise** | “Am I using a non‑deterministic API in a scenario that is ‘just logging / test / temporary ID’?” | Determinism is a global promise – no local exceptions |

---

## 6. Common Task Templates

### 6.1 Add a New Business Rule

```rust
// 1. Create crates/governance/rules/{namespace}/{name}.json
//    (domain + transform using instruction_sequence + evaluate_domain + while_loop)

// 2. Handler only triggers:
let mut state = State::new();
state = state.set("__namespace__.action_requested", Value::Bool(true));
let result = engine.infer(state, &rules);
let value = result.final_state.get("__namespace__.result")?;
```

### 6.2 Modify an Existing Rule

```
1. Change only the JSON file – do not change Rust.
2. Keep the rule_id unchanged (if you need to change it, create a new rule).
3. Run cargo test to verify no existing functionality is broken.
```

### 6.3 Fix a TCB Bug (Requires Change Approval)

```
1. Write a reproduction test first (red).
2. Modify the corresponding file under primitive/.
3. Verify the test turns green.
4. Update FREEZE.md to document the change.
5. Run the full cargo test suite.
6. In the PR, explain why violating the “TCB frozen” rule was necessary.
```

### 6.4 Add a New Primitive (Very Rare, Needs Evaluation)

```
1. Confirm: is this deterministic pure computation? Cannot be expressed by composing existing primitives?
2. Implement exec_xxx in primitive/{category}_ops.rs.
3. Register in three places in primitive/mod.rs: register_all, all_exec_fns, all_explainers.
4. Write unit tests (covering normal + edge + silent fallthrough).
5. Update the quick reference table in §2 of this document.
6. Update the Programming Standards Encyclopedia §4.5.
```

> **v1.1 Change** (2026‑07‑05, Constitutional Dispatch Architecture): Adding a new primitive now **no longer requires modifying `core_eval.json`’s cases table**. `auto_from_registry()` automatically scans `InstructionRegistry` and generates the cases. Flow:
> 1. Add `register_*` in `crates/tcb/src/primitive/<op>_ops.rs` (unchanged)
> 2. Call `register` in `crates/tcb/src/primitive/mod.rs` (unchanged)
> 3. Add business aliases in `crates/governance-core/src/engine/aliases.rs` if needed (optional, not mandatory)
> 4. ~~Modify `crates/governance-core/src/engine/core_eval.json` cases table~~ — **deprecated, no longer required**
> 5. If adding a business‑alias namespace, register the new alias in `[EvoRule Constitutional Dispatch Architecture](../EvoRule%20Constitutional%20Dispatch%20Architecture.md)` §4.2 `register_core_aliases`
>
> TCB‑side integration: `control/dispatch.rs` automatically determines whether to use main dispatch (from `__exec__.dispatch_cases`) or sub‑dispatch (from `instruction.params.cases`) via `contains_key("cases")` – LLM writing JSON does not need to worry about it.

---

## 7. Documentation Navigation (Want to Dive Deeper into X? Read Y)

### 7.0 v1.1 Constitutional Dispatch Architecture – New / Updated Documents

> **v1.1 (2026‑07‑05) Important**: The following documents are new or updated for this architecture change; LLM must read them:

| To Understand X | Read Y | Key Points |
|-----------------|--------|------------|
| Complete **design** of the constitutional dispatch architecture | [EvoRule Constitutional Dispatch Architecture](../EvoRule%20Constitutional%20Dispatch%20Architecture.md) | Five principles + five cases categories (A direct‑map / B alias / C placeholder / D sub‑dispatch / E default) + 10‑step implementation sequence + 12 ADRs |
| TCB‑Governance interaction **contract v1.1** | `docs/spec/en-US/TCB_Governance_Contract.md` §1 + §9 | `dispatch_cases` source changed to governance‑core build; two new fields `dispatch_default` + `dispatch_table_version` |
| Determinism **classification** of dispatch tables | `docs/spec/en-US/EvoRule_Determinism_Standard.md` §5.5 | `dispatch_cases` / `dispatch_default` / `dispatch_table_version` all ✅ L1~L3; `[tbl:<hash>]` ✅ L1~L3 |
| Code locations for TCB‑side integration | `docs/developer-guide/design-decisions/02-TheEquation, core_eval.json, and while_loop.md` §7 | ER‑605 exception #1 (`control/dispatch.rs`) + exception #2 (`audit_ops.rs::trace_step`) code locations |


| I want to know about … | Go read … |
| ----------------------- | --------- |
| TCB layered architecture + module dependencies | Header comments in [lib.rs](../crates/tcb/src/lib.rs) |
| Full programming standards + anti‑pattern catalogue | [EvoRule Programming Standards Encyclopedia](spec/en-US/EvoRule_programming_Spec.md) |
| `__exec__` field conventions + TCB/Governance contract | [TCB_Governance_Contract.md](spec/en-US/TCB_Governance_Contract.md) |
| Determinism standard L1‑L4 levels | [EvoRule Determinism Standard](spec/en-US/EvoRule_Determinism_Standard.md) |
| Paradigm gate checklist | [GATES.md](gate/GATES.md) |
| `while_loop` self‑driven model design *why* | [01-while-loop-self-driving.md](developer-guide/design-decisions/01-while-loop-self-driving.md) |
| TheEquation / core_eval.json design *why* | [02-TheEquation, core_eval.json, and while_loop.md](developer-guide/design-decisions/02-TheEquation,%20core_eval.json,%20and%20while_loop.md) |
| Precise implementation of a single primitive | `///` docs in `crates/tcb/src/primitive/{category}_ops.rs` |
| Audit chain HMAC implementation | [audit.rs](../crates/tcb/src/audit.rs) |
| Known bugs + fix history | `FREEZE.md` (project root) |

---

## 8. Quick Reference: Domain Condition Types + Operators

### 8.1 Domain Types

| type | Description | Key Fields |
| ---- | ----------- | ---------- |
| `atom` | Atomic condition | `attribute` / `op` / `value` |
| `and` | AND | `domains` (List) |
| `or` | OR | `domains` (List) |
| `not` | NOT | `domain` (single) |
| `universal` | Always true | none |
| `empty` | Always false | none |

### 8.2 Operators

| op | Aliases | Description |
| -- | ------- | ----------- |
| `eq` | `==` / `equals` | Equal |
| `ne` | `!=` / `not_equals` | Not equal |
| `gt` | `>` / `greater_than` | Greater than |
| `ge` | `>=` / `greater_equal` | Greater or equal |
| `lt` | `<` / `less_than` | Less than |
| `le` | `<=` / `less_equal` | Less or equal |
| `contains` | `has` | Contains |
| `matches` | `regex` | Regex match |
| `in` | – | In a set |
| `notin` | `not_in` | Not in a set |

---

## 9. Quick Reference: Transform Types

| type | Description | Key Parameters |
| ---- | ----------- | -------------- |
| `instruction_sequence` | Sequential execution | `instructions` |
| `state_set` | Pure assignment | `attr` / `value` |
| `state_compute` | Compute + assign | `attr` / `operation` / `value` |
| `evaluate_domain` | Domain eval + branch | `domain` / `on_true` / `on_false` |
| `while_loop` | Loop | `condition` / `body` / `max_steps` |

**Prohibited**: `if_else` (does not exist) / `for_each` / `iterate_list` / `lambda` / `call` / `$func` / `$eval`

---

## 10. Quick Reference: Priorities / Layers

| Layer | Priority Range | Purpose |
| ----- | -------------- | ------- |
| Layer 0 (Constitution) | 9000‑10000 | Immutable |
| Layer 0.5 (Amendments) | 8000‑8999 | Amendment rules |
| Layer 1 (Meta‑rules) | 7000‑7999 | Meta‑rules |
| Layer 2 (Business) | 0‑6999 | Ordinary business rules |

---

**End of Document**

> This document is the “first injection context” for LLMs. For deeper content, follow the navigation in §7 to the corresponding documents.
> Every time you find that an LLM still needs to look elsewhere to complete a task, reflect on whether this document can be supplemented with one more item, and add it.
