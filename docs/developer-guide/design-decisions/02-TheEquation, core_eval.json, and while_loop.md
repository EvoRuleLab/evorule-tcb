TheEquation, core_eval.json, and while_loop: Synergy and Capability Analysis
1. What Each Component Is
Component	Essence	Analogy
TheEquation	An "=" sign — applies an instruction to a state to produce a deterministic output	CPU fetch‑execute cycle
core_eval.json	A "program" — defines each step of the execution flow	Microcode in ROM
while_loop	A physical mechanism — repeatedly executes a body and drains the queue	CPU clock generator
2. Core Capabilities from Their Synergy
Capability 1: Extensible Instruction-Set Interpreter
A standalone instruction (e.g., increment) is just data. With the three combined, the system becomes an extensible instruction-set interpreter:

text
TheEquation.evaluate(instruction, state)
  → initializes __exec__ context (places the instruction into context)
  → one reg.execute(core.eval)   ← here, "instruction" becomes "program"
  → core.eval's transform is a while_loop
  → while_loop repeatedly executes dispatch
  → dispatch looks up instruction.type in the cases table
  → executes the corresponding physical primitive
Key insight: The core_eval.json cases table is the Instruction Set Architecture (ISA). Every new case entry adds an instruction to this virtual machine. increment, set, conditional, parallel, solver_sat... these case entries define "what instructions this machine can execute."

Capability significance: The system gains an extensible instruction set — adding instructions requires no Rust changes, only new entries in the cases table.

Capability 2: Programmable Control Flow
This is the most central capability from their synergy. Compare with the traditional imperative engine:

Traditional engine (hard‑coded Rust for loop):

text
for step in 0..max_steps {
    dispatch()    // Rust decides when to dispatch
    trace()       // Rust decides when to trace
    advance()     // Rust decides when to advance
    if stop() { break }  // Rust decides when to stop
}
Self‑driving model (JSON‑driven while_loop):

text
while_loop(body=[dispatch, trace_step]) {
    dispatch     // JSON decides dispatching
    trace_step   // JSON decides tracing
    drain queue  // while_loop physical mechanism drains
    check __running  // JSON sets via advance_instruction
}
Capability significance: The execution flow itself becomes modifiable data. Want to add an observe step before dispatch? Modify JSON:

json
"body": [
  {"type": "observe_rules", ...},  // new
  {"type": "dispatch", ...},
  {"type": "trace_step", ...}
]
No Rust changes, no recompilation. This is the greatest capability from their synergy — programmability of the execution flow.

Capability 3: Configuration as Program
eval_config.json appears as configuration, but it is actually part of the program logic:

eval_config field	In a traditional system	In EvoRule it is actually
termination_domain	Configuration item	Termination algorithm (defines when to stop)
default_instruction	Configuration item	Fallback strategy (what to do when queue is empty)
max_steps	Configuration item	Safety‑valve algorithm
drain_strategy.meta_trace	Configuration item	Audit strategy (whether to trace meta instructions)
primitives	Configuration item	Capability declaration (which instructions this machine has)
Capability significance: The system's behavior is not "code + configuration" — it is "code + program". Modifying eval_config.json changes the system's algorithm, not just parameter tuning.

Capability 4: Auditable Minimal Trusted Base
With their synergy, the amount of code that must be trusted shrinks dramatically:

Traditional engine requires trust in:

The for‑loop logic

dispatch logic

trace logic

advance logic

stop logic

Correct interaction among all five

Self‑driving model requires trust in:

evaluate() initialization + single‑call logic

while_loop body + drain logic

Implementation of each physical primitive

Capability significance: The audit scope shrinks from "the entire engine" to "while_loop + physical primitives". Every step of the execution flow is explicitly visible in JSON — you don't need to read Rust code to understand system behavior.

Capability 5: Separation of Meta vs Business Instructions
The drain queue mechanism separates meta instructions from business instructions, decoupling control flow from data flow:

Meta instructions (advance_instruction, set_context, evaluate_domain): executed directly during drain, changing control flow.

Business instructions (increment, set, conditional): set as current instruction, to be handled by the next dispatch round.

Capability significance: evaluate_domain does not directly execute branches — it pushes branch instructions onto the queue. This keeps dispatch as the sole dispatching source and avoids recursive dispatch complexity.

Capability 6: Self‑Referential Rules
The injection of __universe_rules__, together with primitives like observe_rules, filter_rules, and apply_rule, gives the system self‑referential capability:

text
Rules can observe the rule set
Rules can filter the rule set
Rules can apply other rules
Capability significance: The system can reason about itself. A rule can read all rules, analyze them, and decide what to execute. This underpins high‑level reasoning services (conflict detection, cycle detection, effect analysis) — they need no separate Rust implementation; they can be entirely rule‑driven.

Capability 7: Fail‑Fast at Startup
validate_strict() performs bidirectional validation at startup:

Completeness: every type in the cases table must be registered in the registry.

Coverage: every primitive in the registry must have a corresponding entry in the cases table.

Capability significance: Inconsistencies between JSON rules and Rust implementations are caught at startup, not at runtime. This is a prerequisite guarantee for determinism — if the system starts, the cases table and the registry are guaranteed consistent.

3. Emergent Properties
Beyond the 7 explicitly designed capabilities, their synergy also yields the following non‑designed but naturally emergent properties:

Emergent 1: Rules Can Modify Rules (Metaprogramming)
Because __universe_rules__ is part of State, and rules can modify State, rules can modify the rule set itself. This is not explicitly designed; it is a natural consequence of "immutable State + rules' ability to write State."

Emergent 2: Execution Traces Are Naturally Auditable
Because trace_step is part of the while_loop body, and the body is defined in JSON, every step of execution is automatically recorded. This is not "adding an audit module" — it is "the execution flow inherently includes auditing steps."

Emergent 3: Hot‑Loading Becomes Possible
Because the execution flow is defined in JSON, modifying the execution flow does not require restarting the process. Modify core_eval.json and reload — the new flow takes effect immediately. This is a natural result of "configuration as program."

4. Design Motivations
This design addresses the following engineering challenges:

TCB stability: Need a minimal core that "once written correctly, never needs change" → while_loop + physical primitives.

Frequent business logic changes: Need a layer where "changes don't require recompilation" → core_eval.json + JSON rules.

Audit completeness: Execution flow must be explicitly visible to trace every step → trace_step inside the while_loop body.

Clear team collaboration: Need a "look‑once‑and‑understand" criterion → clearly distinguish "control flow (JSON)" from "physical mechanism (Rust)."

The essence of their synergy is: downgrading the "engine" from "controller" to "trigger", and elevating "control" from Rust code to JSON rules.

5. Architecture Status and Outlook
The core_eval.json cases table currently defines 40+ case entries, covering:

Algebraic operations (algebra_sequence)

Solver integration (solver_sat)

Parallel execution (execute_parallel)

Formula evaluation (evaluate_formula)

And many other control‑flow and data‑flow primitives

Current status: High‑level science rules mainly use the noop instruction and do not yet fully utilise the ISA's capabilities.

Architectural outlook: This does not indicate "over‑engineering" — rather, it shows that the underlying instruction set is already complete. As business requirements grow in complexity, upper‑layer rules can progressively use these instructions to orchestrate more sophisticated execution flows — without any changes to the underlying Rust code.

This "capability sinking" architectural pattern allows EvoRule to continuously expand its expressiveness and application scenarios without altering the TCB.

6. Summary
The synergy of the three components amounts to a complete separation of "virtual machine implementation" from "virtual machine program":

Layer	Component	Responsibility
Physical layer	TheEquation + while_loop	Provides fetch‑execute physical mechanism
Microcode layer	core_eval.json	Defines instruction‑set orchestration logic
Program layer	Upper JSON rules	Expresses business logic using the instruction set
Each layer depends only on the layer below, never upward. This enables independent evolution, independent auditing, and independent verification for each layer. This is the physical implementation of EvoRule's "rule‑driven everything" philosophy at the engineering level.

---

## 7. Architecture Update (v1.1, 2026-07-05) — Constitutional Dispatch

> **Scope of this update**: The above analysis (§1–§6) describes the **v1.0 architecture**, where the cases table lives in `core_eval.json` and is read by `dispatch` at runtime. Starting 2026-07-05, the cases table is **no longer stored in JSON**. It is **built dynamically** by `governance-core`'s `DispatchTableBuilder` at startup, then injected into `__exec__`. This update describes what changed in v1.1 and why.

### 7.1 What changed

| Concern | v1.0 | v1.1 |
|---------|------|------|
| Cases-table source | Hardcoded in `core_eval.json` (~660 lines, 40+ case entries) | **Built at startup** by `DispatchTableBuilder` from the `InstructionRegistry` + `register_core_aliases()` + `set_default()` (core_eval.json shrinks to ~30 lines skeleton) |
| Cases-table injection | Direct field in `core_eval.json` → read by `dispatch` | `governance-core::Equation::build_dispatch_table()` produces `(dispatch_cases, dispatch_default, dispatch_table_version)`; injected into `__exec__` by `evaluate()` |
| TCB-side dispatch | `dispatch` reads `__exec__.dispatch_cases` only | `control/dispatch.rs` **dual-source reading** (ER-605 Exception #1): `contains_key("cases")` in `instruction.params` distinguishes main dispatch (read from `__exec__.dispatch_cases`) vs sub-dispatch (read from `instruction.params.cases`) |
| Audit traceability | `trace_step` records state change only | `primitive/audit_ops.rs` **appends `[tbl:<hash>]`** to `change_summary` (ER-605 Exception #2); `dispatch_table_version` = SHA-256 of `(cases + default)` |
| Governance modules | Monolithic `equation.rs` (~1900 LOC) | Split: `equation.rs` (orchestration) + `engine/dispatch_table.rs` (new, ~140 LOC) + `engine/aliases.rs` (new, ~120 LOC) |
| Adding a new physical primitive | Edit `core_eval.json` cases table + register in Rust | **Just register in Rust** — `auto_from_registry()` auto-generates the case (zero JSON change) |
| Adding a new business alias | Edit `core_eval.json` cases table | Edit `aliases.rs::register_core_aliases()` (JSON never touched) |

### 7.2 Why the change

The v1.0 architecture had a **single source of duplication**: every physical primitive registered in Rust needed a matching entry in `core_eval.json` cases table. This caused:

1. **Maintenance burden**: every Rust `register_*()` call required a paired JSON edit. Bugs at the seam (typo in JSON, drift from Rust) were caught only by `validate_strict()` at startup.
2. **Bloating**: `core_eval.json` grew linearly with primitive count; the "constitution" file was no longer constitutional, just a giant lookup table.
3. **No version traceability**: when a case's behavior changed, there was no audit record of "this instruction was dispatched against cases-table version X".

The v1.1 architecture addresses all three:

- **Single source of truth** = `InstructionRegistry` (Rust). Cases table becomes a *projection* of the registry, not a parallel artifact.
- **Constitutional file** = `core_eval.json` shrinks to ~30 lines (just `rule_id` + `transform` skeleton). It stops growing.
- **Audit traceability** = `dispatch_table_version` (content hash) is appended to every `trace_step`. The audit chain now records *which version of the dispatch table* was active when each instruction executed.

### 7.3 What stays the same

The synergy model from §1–§6 still holds:

- **TheEquation** still does initialization + one `reg.execute(core.eval)` call.
- **while_loop** still self-drives via JSON body `[dispatch, trace_step, …]`.
- **core_eval.json** is still the constitutional file (now with a *much smaller* constitution).
- The three-layer separation (Physical / Microcode / Program) is preserved.

### 7.4 Where to look in source

- **TCB-side changes** (in `evorule-tcb`):
  - `crates/tcb/src/control/dispatch.rs` — dual-source reading (ER-605 Exception #1).
  - `crates/tcb/src/primitive/audit_ops.rs` — `[tbl:<hash>]` appending (ER-605 Exception #2).
  - `crates/tcb/src/audit.rs`, `primitive/error_ops.rs`, `primitive/rule_ops.rs` — supporting changes for dispatch-table validation errors and rule-fixture alignment.
- **Governance-side changes** (in `evorule-governance`):
  - `crates/governance-core/src/engine/equation.rs` — `extract_cases_table()` deleted; `build_dispatch_table()` added.
  - `crates/governance-core/src/engine/dispatch_table.rs` (new) — `DispatchTableBuilder` (categories A/E).
  - `crates/governance-core/src/engine/aliases.rs` (new) — `register_core_aliases()` (categories B/C/D).
  - `crates/governance-core/src/engine/core_eval.json` — shrunk from ~660 → ~30 lines.
  - `crates/governance-core/src/engine/eval_config.json` — `primitives` section deleted (now derived from registry).

### 7.5 Cross-references

- **Full architectural specification**: see [`docs/spec/en-US/TCB_Governance_Contract.md`](../spec/en-US/TCB_Governance_Contract.md) for the TCB-side contract (governance boundary, ER-605 exceptions #1/#2, dispatch-table versioning, audit-chain fields).
- **Audit / determinism**: see [`docs/spec/en-US/EvoRule_Determinism_Standard.md`](../spec/en-US/EvoRule_Determinism_Standard.md) §5.5 for the `[tbl:<hash>]` audit-record format and the L1-L4 determinism boundary.
- **Forbidden-rules contract**: see [`docs/gate/FORBIDDEN_OVERVIEW.md`](../gate/FORBIDDEN_OVERVIEW.md) for the full GG-01..GG-25 governance-side forbidden-rules table (mirrored from `evorule-governance/docs/gate/GATES.md`).

---

End of Document