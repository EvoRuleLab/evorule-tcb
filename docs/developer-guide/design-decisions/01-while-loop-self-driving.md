# while_loop Self‑Driving Model: Design Principles and Implementation（v1.0-en）

## 1. One‑Sentence Summary

**`evaluate()` does only initialization + one `reg.execute(while_loop)`; thereafter, JSON rules take over the entire execution flow.**

The engine is “dumb”; the rules are “smart”. TCB minimization is not about reducing line count, but about transferring control from engine code to JSON rules.

---

## 2. Why a Self‑Driving Model Is Needed

### 2.1 Traditional Engine Model (v4 old)

```
evaluate(instruction, state):
    for step in 0..max_steps:
        dispatch(instruction)        // engine decides dispatch
        trace(step)                  // engine decides tracing
        advance()                    // engine decides advancement
        if should_stop(): break      // engine decides termination
```

**Problem**: The engine's for‑loop controls everything—dispatch, tracing, advancement, termination. Every step is hard‑coded in Rust. Changing execution flow requires modifying Rust code and recompiling.

### 2.2 Self‑Driving Model (v4 new)

```
evaluate(instruction, state):
    init __exec__ context            // engine only initializes
    reg.execute(core.eval)           // engine only triggers once
    // thereafter, JSON rules take over
```

**Core shift**: The engine changes from “controller” to “trigger”. Every step of execution—what to dispatch, what to trace, when to advance, when to stop—is defined by rules in `core_eval.json`.

### 2.3 Design Benefits

1. **TCB minimization**: The trusted code in the engine shrinks from “the whole for‑loop” to “initialization + one call”. Dispatch/trace/advance/stop logic is moved from TCB to JSON rules.
2. **Rules as programs**: `core_eval.json` is no longer a configuration file; it is an executable “program”. Changing execution flow requires only JSON modifications, not Rust recompilation.
3. **Auditability**: Every step of execution is explicitly defined by JSON rules, not hidden inside a Rust for‑loop. Anyone can understand the complete execution flow by reading `core_eval.json`.
4. **Verifiability**: The engine’s behavior is strictly bounded to “initialization + one call”. Any deviation is a bug. This makes formal verification feasible.

---

## 3. Detailed Execution Flow

### 3.1 Complete Execution Timeline

```
User call: TheEquation::evaluate(instruction, initial_state)
│
├─ 1. Initialize __exec__ context
│     ├─ instruction: the current instruction
│     ├─ queue: [] (empty)
│     ├─ __running: true
│     ├─ default_instruction: noop
│     ├─ termination_domain: InstructionDomain("noop")
│     ├─ meta_instruction_types: [set_context, advance_instruction, ...]
│     ├─ audit_on: true/false
│     └─ drain_meta_trace: true/false
│
├─ 2. One reg.execute(core.eval)
│     │
│     │  core.eval's transform is a while_loop:
│     │
│     └─ while_loop(condition=__running, body=[dispatch, trace_step]):
│           │
│           ├─ [loop start] check condition: __running == true? → continue
│           │
│           ├─ execute body[0]: dispatch
│           │     ├─ read __exec__.instruction.type → "increment"
│           │     ├─ look up "increment" in cases table → found set_context
│           │     ├─ resolve $ref references → get actual parameters
│           │     ├─ execute set_context(x += 1)
│           │     └─ push advance_instruction onto queue
│           │
│           ├─ execute body[1]: trace_step
│           │     └─ write audit record
│           │
│           ├─ drain queue:
│           │     ├─ pop advance_instruction (meta instruction)
│           │     ├─ directly execute advance_instruction:
│           │     │     ├─ queue empty → inject default_instruction (noop)
│           │     │     └─ check termination_domain:
│           │     │           noop == InstructionDomain("noop") → match!
│           │     │           set __running = false
│           │     └─ drain ends
│           │
│           ├─ [loop check] __running == false? → exit loop
│           │
│           └─ while_loop returns final state
│
├─ 3. extract_result(state)
│     └─ remove __exec__, __universe_rules__, keep __audit_chain
│
└─ return result state
```

### 3.2 Key Step Explanations

#### Step 1: Initialization (engine responsibility)

The only “intelligent” operation `evaluate()` performs is initializing the `__exec__` context. This context is the information base for the entire self‑driving model:

| Field                    | Role                             | Source             |
| ------------------------ | -------------------------------- | ------------------ |
| `instruction`            | current instruction to execute   | user input         |
| `queue`                  | instruction queue (FIFO)         | initially empty    |
| `__running`              | loop condition flag              | initially true     |
| `default_instruction`    | default when queue empty         | eval_config.json   |
| `termination_domain`     | termination condition domain     | eval_config.json   |
| `meta_instruction_types` | list of meta instruction types   | eval_config.json   |
| `audit_on`               | audit switch                      | eval_config.json   |

**Design point**: All configuration is passed to the TCB layer via `__exec__`; the TCB does not directly access `EvalConfig`. This decouples the TCB from the Governance layer.

#### Step 2: One reg.execute (engine responsibility)

The engine calls `reg.execute(core.eval)`, triggering the while_loop. **After this, the engine makes no further decisions.**

#### Step 3: Rules take over (JSON responsibility)

The while_loop body is `[dispatch, trace_step]`, entirely defined by `core_eval.json`. To change the execution flow (e.g., add an `observe` step before dispatch), simply modify the JSON—no Rust code changes needed.

---

## 4. Core Mechanisms Explained

### 4.1 while_loop: The Physical Engine of Self‑Driving

```
while condition(__running):
    exec body: [dispatch, trace_step]
    drain queue
    check __running
```

while_loop is the physical carrier of the self‑driving model. It does not “know” what instructions to execute—it only knows “repeat executing body, draining the queue, until the condition is false.”

**Key design decisions**:

- `__running` is **not checked** between body executions—only after drain and at condition checks. This ensures that `trace_step` in the body always executes, even if `dispatch` has already set `__running=false`.
- `max_steps` acts as a safety valve to prevent infinite loops due to malformed rules.

### 4.2 dispatch: O(1) Dispatching + Pushing advance

`dispatch` is the core embodiment of “rules as programs”. Its job is:

1. Read the current instruction type (`__exec__.instruction.type`)
2. Look up the corresponding physical primitive in the cases table
3. Resolve `$ref` references to build actual parameters
4. Execute the physical primitive
5. **Push `advance_instruction` onto the queue**

Step 5 is the key alignment with v2. `dispatch` does not advance—it only dispatches and enqueues. Advancement is performed by `advance_instruction` during drain.

```
dispatch("increment"):
    case "increment" → set_context(x += 1)
    push advance_instruction to queue
    return state
```

**default branch**: When the instruction type is not found in the cases table, the default branch (usually `apply_rule`, attempting to execute a rule by ID) is taken. The default branch also pushes `advance_instruction`.

### 4.3 drain queue: Core v2 Scheduling Logic

`drain` is the most complex part of while_loop, implementing the queue‑draining strategy:

```
drain queue:
    if queue is not empty:
        pop first item
        if it's a meta instruction:
            execute it directly
            continue drain
        else (business instruction):
            set as current instruction
            break (let dispatch handle it next iteration)
    else (queue is empty):
        if current instruction is noop:
            execute advance_instruction
            break
        else (current instruction is business):
            break (let dispatch handle it next iteration)
```

**The distinction between meta and business instructions** is crucial:

- **Meta instructions** (`set_context`, `advance_instruction`, `evaluate_domain`, etc.): executed directly, no dispatch. They are “control‑flow” instructions that change state but not the current instruction.
- **Business instructions** (`increment`, `set`, `conditional`, etc.): set as current instruction, break back to the outer while_loop, to be handled by the next dispatch round. They are “data‑flow” instructions that require dispatch.

**Rationale**:

- Meta instructions are executed directly to avoid recursive dispatch.
- Business instructions are set and break to ensure dispatch always handles business instructions, maintaining dispatch consistency.

### 4.4 advance_instruction: Advancement + Termination Detection

`advance_instruction` is the physical primitive for instruction advancement and termination detection:

```
advance_instruction:
    if queue is not empty:
        pop first → set as current instruction
    else:
        inject default_instruction (noop)
        check termination_domain:
            if current instruction matches → __running = false
            else → continue
```

**Termination detection mechanism**:

- `termination_domain` is a Domain expression, typically `InstructionDomain("noop")`.
- After `advance_instruction` injects `default_instruction` (noop), it immediately checks `termination_domain`.
- If the current instruction type matches the termination condition, it sets `__running=false`.
- while_loop exits at the next condition check.

**Why check termination inside advance_instruction**:

- This is the only moment when “the queue is empty” is known.
- After injecting `default_instruction`, the current instruction type is deterministic (noop), enabling exact matching.
- Placing termination detection in `advance_instruction` keeps while_loop simple.

### 4.5 evaluate_domain: Conditional Branching via Queue Push

`evaluate_domain` implements conditional branching, but does not directly execute branch instructions—it pushes them onto the queue:

```
evaluate_domain(domain, on_true, on_false):
    evaluate domain against state
    if domain is satisfied:
        push on_true instruction to front of queue
    else:
        push on_false instruction to front of queue
```

**Why push instead of execute directly**:

- Aligns with v2: v2’s evaluate_domain also pushes.
- Maintains `dispatch` as the sole dispatching source—all business instructions go through dispatch.
- `drain` handles queued items uniformly, correctly distinguishing meta from business instructions.

---

## 5. A Complete Execution Example

### 5.1 Scenario: Executing a `conditional` instruction

Input instruction:

```json
{
  "type": "conditional",
  "params": {
    "domain": { "type": "atom", "attribute": "x", "op": "gt", "value": 0 },
    "then": { "type": "increment", "params": { "attr": "x", "delta": 1 } },
    "else": { "type": "set", "params": { "attr": "x", "value": 0 } }
  }
}
```

Initial state: `x = 5`

### 5.2 Step‑by‑Step Execution

```
Step 1: evaluate() initializes
  __exec__ = {
    instruction: {type: "conditional", params: {...}},
    queue: [],
    __running: true,
    termination_domain: InstructionDomain("noop"),
    ...
  }

Step 2: evaluate() calls reg.execute(core.eval)
  → while_loop starts

Step 3: while_loop round 1
  3a. Check condition: __running == true ✓
  3b. Execute body[0] = dispatch:
      - read instruction.type = "conditional"
      - cases table lookup → evaluate_domain
      - resolve $ref → get domain, on_true, on_false
      - execute evaluate_domain:
        - domain: x > 0 → true (x=5)
        - push on_true (increment) to front of queue
      - dispatch pushes advance_instruction to queue
      - queue: [increment, advance_instruction]
  3c. Execute body[1] = trace_step:
      - write audit record
  3d. drain queue:
      - pop increment → business instruction → set as current → break
      - __exec__.instruction = {type: "increment", params: {attr: "x", delta: 1}}

Step 4: while_loop round 2
  4a. Check condition: __running == true ✓
  4b. Execute body[0] = dispatch:
      - read instruction.type = "increment"
      - cases table lookup → set_context
      - execute set_context: x = 5 + 1 = 6
      - push advance_instruction to queue
      - queue: [advance_instruction]
  4c. Execute body[1] = trace_step:
      - write audit record
  4d. drain queue:
      - pop advance_instruction → meta instruction → execute directly
        - queue empty → inject default_instruction (noop)
        - check termination_domain: noop == InstructionDomain("noop") → match!
        - set __running = false
      - drain ends

Step 5: while_loop checks __running == false → exit loop

Step 6: evaluate() extracts result
  result = {x: 6, __audit_chain: [...]}
```

### 5.3 Execution Flow Diagram

```
evaluate()
  │
  ├─ init __exec__
  │
  └─ reg.execute(while_loop)
       │
       ├─ [Round 1] dispatch(conditional)
       │              ├─ evaluate_domain(x>0 → true)
       │              ├─ push increment to queue
       │              ├─ push advance_instruction to queue
       │              ├─ trace_step
       │              └─ drain: increment(business) → set current, break
       │
       ├─ [Round 2] dispatch(increment)
       │              ├─ set_context(x += 1)
       │              ├─ push advance_instruction to queue
       │              ├─ trace_step
       │              └─ drain: advance(meta) → execute → noop → terminate!
       │
       └─ __running=false → exit
```

---

## 6. Comparison with Traditional Model

### 6.1 Control‑Flow Comparison

| Aspect             | Traditional (for loop)                              | Self‑Driving (while_loop)                         |
| ------------------ | --------------------------------------------------- | ------------------------------------------------- |
| Loop control       | Rust for loop                                       | JSON while_loop rule                              |
| Dispatch decision  | Hard‑coded in Rust                                  | Driven by JSON cases table                        |
| Tracing decision   | Hard‑coded in Rust                                  | trace_step in JSON body                           |
| Advancement decision| Rust automatically advances                         | dispatch pushes advance_instruction               |
| Termination decision| Rust code checks                                    | advance_instruction checks termination_domain     |
| Modifying flow     | Change Rust code + recompile                        | Change JSON rules + hot‑reload                    |

### 6.2 TCB Comparison

| TCB Component | Traditional Model                                       | Self‑Driving Model                                |
| ------------- | ------------------------------------------------------- | ------------------------------------------------- |
| evaluate()    | init + for loop + dispatch + trace + advance + stop     | init + one reg.execute                            |
| while_loop    | non‑existent                                            | body execution + drain queue                      |
| dispatch      | inside for loop                                         | inside JSON body                                  |
| advance       | automatically in for loop                               | pushed by dispatch, executed in drain             |
| termination   | checked in for loop                                     | checked inside advance_instruction                |

### 6.3 Trustworthiness Comparison

In the traditional model, you must trust:

1. `evaluate()`’s for‑loop logic
2. `dispatch` logic
3. `trace` logic
4. `advance` logic
5. `stop` logic
6. The interactions among all above

In the self‑driving model, you must trust:

1. `evaluate()`’s initialization logic
2. `evaluate()`’s single‑call logic
3. `while_loop`’s body + drain logic
4. That JSON rules are correctly defined

**Key difference**: In the traditional model, the interactions (1‑6) are implicit (Rust control flow); the entire for‑loop implementation must be trusted. In the self‑driving model, interactions are explicit (JSON data flow); only the while_loop physical implementation must be trusted.

---

## 7. Design Principles

### 7.1 Engine Minimization

The engine (TheEquation) is strictly limited to:

1. Initialize execution context
2. Trigger one rule execution
3. Extract results

Anything beyond these three is a design violation.

### 7.2 Rule‑Driven Principle

All execution flow decisions are defined by JSON rules:

- What to dispatch → `core_eval.json` cases table
- What to trace → `trace_step` in body
- When to advance → `dispatch` pushes `advance_instruction`
- When to stop → `advance_instruction` checks `termination_domain`

### 7.3 Unidirectional Data Flow

```
user instruction → __exec__.instruction → dispatch → case body → queue → drain → next instruction
```

The data flow is one‑way, with no cycles. Each instruction enters from the queue, is processed by dispatch, generates new queue items, and is drained. This guarantees traceability.

### 7.4 Meta/Business Instruction Separation

- **Meta instructions**: change control flow (`set_context`, `advance_instruction`, `evaluate_domain`, `push_instruction`, etc.)
- **Business instructions**: change data flow (`increment`, `set`, `conditional`, `sequence`, etc.)

`drain` handles them differently: meta instructions are executed directly; business instructions are set as current and break. This avoids recursive dispatch issues.

---

## 8. Common Misconceptions Clarified

### 8.1 “while_loop is a loop, so the engine still controls”

**Wrong**. while_loop is a physical primitive that does not know what to loop over. The loop content (body) is defined by JSON rules. while_loop only provides the “repeat + drain” mechanism, not the “what to execute” decision.

Analogy: while_loop is the CPU; JSON rules are the program. The CPU does not know which program to execute—it only provides the fetch‑execute mechanism.

### 8.2 “pushing advance_instruction is redundant; while_loop could auto‑advance”

**Wrong**. If while_loop auto‑advanced, advancement timing would be decided by Rust code, not by JSON rules—violating the rule‑driven principle.

In the self‑driving model, advancement timing is decided by `dispatch` (via pushing `advance_instruction`). This allows you to change advancement timing by modifying JSON rules, without touching Rust.

### 8.3 “termination_domain could be checked inside while_loop”

**Possible, but undesirable**. Keeping termination detection inside `advance_instruction` has benefits:

1. while_loop stays simple—it only checks the `__running` flag.
2. Termination detection is tied to advancement—only when advancing do we need to check termination.
3. `advance_instruction` is the only point where “queue is empty” is known—the natural place for termination detection.

### 8.4 “evaluate_domain should execute branch instructions directly”

**Should not**. If evaluate_domain directly executed branch instructions:

1. Branch instructions would bypass `dispatch`, violating the “single dispatch source” principle.
2. Branch instructions might be business instructions that require dispatch.
3. Pushing to the queue allows `drain` to correctly distinguish meta vs business instructions.

---

## 9. Configuration Injection Mechanism

A key design of the self‑driving model: the TCB does not directly access EvalConfig; it obtains configuration indirectly via the `__exec__` context.

### 9.1 Injection Flow

```
EvalConfig (Governance layer)
    │
    ├─ termination_domain → __exec__.termination_domain
    ├─ meta_instruction_types → __exec__.meta_instruction_types
    ├─ enable_tracing → __exec__.audit_on
    ├─ drain_meta_trace → __exec__.drain_meta_trace
    └─ default_instruction → __exec__.default_instruction
```

### 9.2 Consumers

| Config Item            | Consumer                       | Reading Time                                   |
| ---------------------- | ------------------------------ | ---------------------------------------------- |
| termination_domain     | advance_instruction            | after injecting default_instruction when queue empty |
| meta_instruction_types | while_loop (drain)             | when distinguishing meta vs business instructions |
| audit_on               | trace_step, while_loop (drain) | deciding whether to write audit records        |
| drain_meta_trace       | while_loop (drain)             | deciding whether to trace meta instructions during drain |
| default_instruction    | advance_instruction            | when queue is empty                            |

### 9.3 Why Not Pass Direct Parameters

1. **Decoupling**: TCB does not need to know about EvalConfig.
2. **Extensibility**: Adding a new config item only requires adding a field in `__exec__`, no function signature changes in TCB.
3. **Auditability**: All configurations are inside `__exec__`, trackable via the audit chain.

---

## 10. The Role of the Audit Chain in the Self‑Driving Model

### 10.1 Generating Audit Records

Audit records are generated by `trace_step`, which occurs at two points:

1. **`trace_step` in body**: after each while_loop body execution.
2. **`trace_step` in drain**: when drain processes business instructions or noop advancement (controlled by `audit_on` and `drain_meta_trace`).

### 10.2 Audit Chain Integrity

The audit chain is HMAC‑linked; each record contains a hash of the previous record. This guarantees:

- **Tamper‑proof**: modifying any record breaks the hash chain for all subsequent records.
- **Deletion‑proof**: removing any record breaks the chain.
- **Verifiable**: anyone can verify chain integrity from beginning to end.

### 10.3 `audit_on` Control

- `audit_on=true`: `trace_step` writes audit records; `trace_step` in drain also writes.
- `audit_on=false`: `trace_step` is a no‑op (returns state unchanged); drain‑based `trace_step` also skipped.

---

## 11. Code Location Index

| Component          | File                                                    | Key Function/Structure                     |
| ------------------ | ------------------------------------------------------- | ------------------------------------------ |
| Engine entry       | `crates/governance/src/engine/equation.rs`              | `TheEquation::evaluate()`                  |
| Core rule          | `crates/governance/src/engine/core_eval.json`           | while_loop + dispatch + trace_step         |
| Evaluation config  | `crates/governance/src/engine/eval_config.json`         | termination_domain, meta_types, etc.       |
| while_loop         | `crates/tcb/src/instruction/main_loop.rs`               | `exec_while_loop()`, `drain_queue()`       |
| dispatch           | `crates/tcb/src/instruction/dispatch.rs`                | `exec_dispatch()`, `dispatch_by_cases()`   |
| advance            | `crates/tcb/src/instruction/meta.rs`                    | `exec_advance_instruction()`               |
| evaluate_domain    | `crates/tcb/src/instruction/meta.rs`                    | `exec_evaluate_domain()`                   |
| set_context        | `crates/tcb/src/instruction/meta.rs`                    | `exec_set_context()`                       |
| trace_step         | `crates/tcb/src/instruction/trace.rs`                   | `exec_trace_step()`                        |
| Domain             | `crates/tcb/src/domain.rs`                              | `Domain::contains()`, `Domain::from_value()` |
| State              | `crates/tcb/src/state.rs`                               | `State::get_path()`                        |

---

## 12. Design Motivation and Significance

### 12.1 Separation of Engine and Rules

The self‑driving model achieves a clean separation between engine and rules:

- **Engine (TCB)**: provides execution capability for physical primitives; contains no business logic.
- **Rules (JSON)**: define business logic and execution flow; contain no execution capability.

This separation enables:

- Formal verification of the engine (because its behavior is deterministic and minimal).
- Hot‑loading of rules (because rules are data, not code).
- Auditability of rules (because rules are JSON, parsable and analyzable).

### 12.2 The “Rules as Programs” Paradigm

In this model, `core_eval.json` is not configuration—it is a program. It defines:

- Program entry point (while_loop)
- Program dispatching logic (dispatch + cases)
- Program tracing logic (trace_step)
- Program termination logic (advance_instruction + termination_domain)

Modifying `core_eval.json` is equivalent to modifying the program. This is the ultimate form of data‑driven design—data is not just configuration; data **is** the program.

### 12.3 The True Meaning of Trusted Computing Base

TCB is the part of the system that must be trusted. In the self‑driving model:

**Trusted**:
- `evaluate()` initialization logic
- while_loop body + drain logic
- Execution of physical primitives

**Not Trusted**:
- The definitions of JSON rules (auditable, verifiable, hot‑fixable)
- The orchestration of execution flow (defined by JSON rules, not hard‑coded)

This means that even if JSON rules contain errors, the worst case is incorrect output—the engine’s behavior remains predictable. The engine always does “initialization + one call”, which is verifiable.

### 12.4 Significance for EvoRule

EvoRule’s core design philosophy is “rule‑driven everything”. The while_loop self‑driving model is the physical embodiment of this philosophy:

1. **Rules define behavior**: Not code, but rules define what to do.
2. **Engine executes rules**: The engine does not define behavior; it executes what rules define.
3. **Audit verifies behavior**: The audit chain records every step driven by rules, enabling post‑hoc verification.

These three form EvoRule’s “Rule‑Execute‑Audit” trinity, and the while_loop self‑driving model is the central hub connecting them.
```


