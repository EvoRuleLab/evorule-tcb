# Mechanism vs. Policy: Defining the Boundary for Deterministic Execution Engines(v1.0-en)

## 1. One-Sentence Summary

**Mechanism = how the engine executes (efficiency, data structures, algorithms); Policy = what the engine does (business rules, thresholds, correctness criteria). Mechanisms belong in Rust; Policies belong in JSON.**

This distinction creates a "mechanism exemption" for G-13/GG-31: optimizations that improve execution efficiency without changing external behavior or encoding business logic are allowed in Rust. All internal mechanisms requiring automatic invocation are registered in [mechanism-exemption.json](mechanism-exemption.json).

---

## 2. Why This Distinction Is Needed

### 2.1 The Current Problem

G-13 and GG-31 are interpreted as absolute prohibitions:

- **G-13**: "Do NOT modify TCB core files" — interpreted as "never touch state.rs, value.rs, etc."
- **GG-31**: "Business logic must be in JSON" — interpreted as "no configurable logic in Rust"

This strict interpretation has caused real harm:

1. **Performance penalty**: The version counter optimization (O(1) state change detection) was rejected because it modified `state.rs`, forcing the system to retain O(n) full-state comparison.

2. **Flexibility loss**: Adaptive iteration bounds based on state size were rejected because they "encoded policy," even though the actual threshold values came from JSON.

3. **Maintenance burden**: Developers must work around artificial constraints, creating convoluted solutions that achieve the same result but with worse performance.

### 2.2 The Core Insight

The purpose of G-13 and GG-31 is to protect **determinism and auditability**, not **code immutability**. A modification that:

- Does not change external behavior (same input → same output)
- Does not encode business logic (no thresholds, no decisions about "what" to do)
- Does not affect auditability (the audit chain still records all state changes)

...is safe and should be allowed.

---

## 3. Definitions

### 3.1 Mechanism

**Definition**: Implementation details of how the execution engine performs its work. Mechanisms are concerned with _efficiency_, _correctness_, and _determinism_.

**Key characteristics**:

- Same input always produces the same output (deterministic)
- No business meaning encoded
- No configurable thresholds or decision criteria
- Changes do not require JSON modification
- Changes are transparent to the audit chain

**Examples**:

- Version counter in `State` for O(1) change detection
- `im::HashMap` for O(1) clone operations
- `BTreeMap` for deterministic iteration order
- Internal caching of frequently accessed state paths
- Adaptive scheduling algorithms (how to schedule, not what threshold to use)
- Optimized dispatch table lookup

### 3.2 Policy

**Definition**: Human-readable rules and configurations that define what the system should do. Policies are concerned with _business logic_, _thresholds_, and _correctness criteria_.

**Key characteristics**:

- Encodes business meaning or decision criteria
- Contains configurable values that affect behavior
- Changes require JSON modification
- Changes are explicitly recorded in the audit chain

**Examples**:

- `max_iterations` threshold in `eval_config.json`
- Rule domains and transforms in JSON rule files
- `termination_domain` configuration
- Business aliases in `aliases.json`
- Priority values for rule sorting
- Any "if X then Y" logic that encodes domain knowledge

---

## 4. The Boundary Framework

### 4.1 Decision Matrix

| Question                                 | Mechanism (Rust) | Policy (JSON) |
| ---------------------------------------- | ---------------- | ------------- |
| Does it encode business meaning?         | ❌               | ✅            |
| Does it contain configurable thresholds? | ❌               | ✅            |
| Does it affect external behavior?        | ❌               | ✅            |
| Does it change the audit record format?  | ❌               | ✅            |
| Is it an optimization for efficiency?    | ✅               | ❌            |
| Is it a data structure improvement?      | ✅               | ❌            |
| Is it an algorithmic improvement?        | ✅               | ❌            |

### 4.2 Test Cases

Use these test cases to determine if a change is mechanism or policy:

**Test 1: The "Same Input" Test**

> If I give the system the same input, will it produce the same output?
>
> - Yes → Mechanism (efficiency optimization, no behavior change)
> - No → Policy (behavior change, must be in JSON)

**Test 2: The "Business Meaning" Test**

> Does this change encode any decision about "what" the system should do?
>
> - No → Mechanism (only "how" to do it)
> - Yes → Policy (contains business logic)

**Test 3: The "Configurable Value" Test**

> Does this change contain numeric thresholds or configurable values?
>
> - No → Mechanism (pure algorithm/data structure)
> - Yes → Policy (threshold values must be in JSON; the mechanism to use them can be in Rust)

**Test 4: The "Audit Chain" Test**

> Does this change affect what gets recorded in the audit chain?
>
> - No → Mechanism (transparent to audit)
> - Yes → Policy (audit-visible behavior change)

---

## 5. Primitive-First Principle

### 5.1 The Core Question

> **"Can this mechanism be implemented as a physical primitive instead of modifying TCB core files?"**

The answer depends on whether the mechanism requires **explicit invocation** (by JSON rules) or **automatic invocation** (on every state modification).

### 5.2 Decision Flowchart

```
Is this a mechanism optimization?
    │
    ├─ Can it be EXPLICITLY invoked by JSON rules?
    │     ├─ Yes → Implement as a PHYSICAL PRIMITIVE (preferred)
    │     │        Examples: content_hash, set_intersection, format_string
    │     │
    │     └─ No → Does it require AUTOMATIC/TRANSPARENT invocation?
    │           ├─ Yes → Implement as INTERNAL MECHANISM (mechanism exemption)
    │           │        Examples: version counter, im::HashMap structural sharing
    │           │
    │           └─ No → It's not a mechanism; re-evaluate
    │
    └─ Is it a policy decision? → Implement in JSON
```

### 5.3 Key Distinction: Explicit vs. Automatic

#### Explicit Invocation (Primitives)

**Definition**: The mechanism is called when a JSON rule explicitly references it.

**Examples**:

- `content_hash`: Called when a rule needs to compute a hash
- `set_intersection`: Called when a rule needs to compute a set intersection
- `format_string`: Called when a rule needs to format text
- `state_compute`: Called when a rule needs to compute a value

**Advantages**:

- Follows G-13 (no modification to core files)
- Follows GG-31 (invocation is controlled by JSON rules)
- Explicit in audit chain (each invocation is traced)
- Reusable across different rule patterns

**Disadvantages**:

- Requires rule authors to know about the primitive
- Adds overhead of rule invocation (dispatch, trace, etc.)
- Not suitable for mechanisms that must be transparent

#### Automatic Invocation (Internal Mechanisms)

**Definition**: The mechanism is automatically triggered on every relevant operation, without explicit rule invocation.

**Examples**:

- Version counter: Must increment on every state modification (`set`, `remove`, `set_path`)
- `im::HashMap` structural sharing: Automatically provides O(1) clone
- `BTreeMap` sorting: Automatically maintains deterministic iteration order

**Why these cannot be primitives**:

1. **Version counter**: If implemented as a `bump_version` primitive, every JSON rule that modifies state would need to call it — this is error-prone and places burden on rule authors. The version counter must be incremented _automatically_ on every `set`/`remove`/`set_path` call.

2. **Structural sharing**: This is a data structure property that works automatically — you don't need to call a primitive to get O(1) clone.

3. **Deterministic iteration**: `BTreeMap` maintains sorted order automatically — you don't need to call a primitive to sort keys.

**Advantages**:

- Transparent to rule authors
- No overhead of rule invocation
- Guaranteed to be applied consistently

**Disadvantages**:

- Requires modification to TCB core files (needs mechanism exemption)
- Not visible in audit chain (internal implementation detail)

### 5.4 Hybrid Approaches

Some mechanisms can be implemented as a hybrid:

**Example: State Fingerprint**

Instead of a version counter in `state.rs`, implement a `state_fingerprint` primitive that computes a lightweight hash of the state:

```json
{
    "type": "state_fingerprint",
    "params": {
        "result_attr": "__state_version__"
    }
}
```

**Pros**:

- No modification to `state.rs`
- Explicit in audit chain
- Can be called at any point in rule execution

**Cons**:

- O(n) computation time (must traverse all state keys)
- Must be called explicitly in JSON rules
- Still requires rule authors to remember to call it

**When to use**: If the performance cost of O(n) fingerprint computation is acceptable, and you need explicit auditability.

**When not to use**: If O(1) change detection is critical for performance, or if automatic invocation is required.

### 5.5 Implementation Guidance

**Step 1: Always try the primitive approach first**

Ask: Can this mechanism be expressed as a primitive that JSON rules can explicitly invoke?

- If yes → Implement as a primitive in `primitive/`
- If no → Proceed to Step 2

**Step 2: Check if automatic invocation is required**

Ask: Does this mechanism need to be triggered automatically on every relevant operation?

- If yes → Implement as internal mechanism with mechanism exemption
- If no → Re-evaluate whether this is truly a mechanism

**Step 3: Document the decision**

If implementing as an internal mechanism:

- Add comments explaining why it couldn't be a primitive
- Add tests verifying the mechanism doesn't change behavior
- Reference this ADR in the code

---

## 6. G-13/GG-31 Reinterpretation

### 6.1 G-13: TCB Core File Modification

**Current**: "Do NOT modify TCB core files."

**Reinterpreted**: "Do NOT modify TCB core files in ways that change external behavior or encode business logic. Mechanism optimizations that preserve deterministic behavior are allowed."

**Allowed modifications**:

- Adding version counters or other internal tracking fields
- Changing data structures (e.g., `std::HashMap` → `im::HashMap`)
- Optimizing algorithmic complexity (O(n) → O(1))
- Improving memory efficiency without changing semantics
- Adding helper methods that don't affect state semantics

**Prohibited modifications**:

- Adding new primitives (must go in `primitive/`)
- Changing the behavior of existing primitives
- Adding non-deterministic operations
- Adding business logic

### 6.2 GG-31: Business Logic in Rust

**Current**: "Business logic must be encoded in JSON rules."

**Reinterpreted**: "Business policy (what to do, thresholds, rules) must be encoded in JSON. Business mechanisms (how to execute, optimization algorithms) may be implemented in Rust, provided they: (1) do not encode policy values themselves, and (2) policy values are read from JSON."

**Allowed in Rust**:

- Adaptive scheduling algorithms (how to adjust based on state)
- Efficient change detection mechanisms
- Optimization heuristics (as long as the actual thresholds come from JSON)
- Execution framework improvements

**Prohibited in Rust**:

- Hard-coded threshold values (must come from JSON)
- Business rules or decision logic
- Logic that would change if JSON configuration changed

---

## 7. Case Studies

### 7.1 Case Study: State Version Counter

**Original Decision**: Rejected because modifying `state.rs` violates G-13.

**Re-evaluation**:

| Test                    | Result                             |
| ----------------------- | ---------------------------------- |
| Same Input Test         | ✅ Same input produces same output |
| Business Meaning Test   | ✅ No business meaning encoded     |
| Configurable Value Test | ✅ No configurable values          |
| Audit Chain Test        | ✅ No effect on audit records      |

**Conclusion**: **Mechanism**. Should be allowed.

**Implementation Guidance**:

- Add `version: u64` field to `State` struct
- Increment on all state modifications (`set`, `set_all`, `remove`, `set_path`)
- Add `version()` getter for external access
- Use in `execute_until_stable` to detect state changes in O(1) time

### 7.2 Case Study: Adaptive Iteration Bounds

**Original Decision**: Rejected because "state size → iteration count" mapping is policy.

**Re-evaluation**:

| Test                    | Result                                                                  |
| ----------------------- | ----------------------------------------------------------------------- |
| Same Input Test         | ✅ Same input produces same output                                      |
| Business Meaning Test   | ✅ The mapping algorithm is mechanism; the actual thresholds are policy |
| Configurable Value Test | ⚠️ Threshold values must come from JSON                                 |
| Audit Chain Test        | ✅ No effect on audit records                                           |

**Conclusion**: **Hybrid**. The mapping algorithm (mechanism) can be in Rust; the threshold values (policy) must be in JSON.

**Implementation Guidance**:

- Keep `fixpoint_max_iterations` and `fixpoint_min_iterations` in `eval_config.json`
- Allow `RuleExecutor` to read these values from `EvalConfig`
- Allow adaptive scheduling logic in Rust that reads threshold ranges from JSON
- Do NOT hard-code threshold values in Rust

### 7.3 Case Study: while_loop max_steps

**Current Status**: `max_steps: 10000` is a default value in Rust, with JSON override.

**Evaluation**:

| Test                    | Result                                                               |
| ----------------------- | -------------------------------------------------------------------- |
| Same Input Test         | ❌ Different max_steps produces different output (early termination) |
| Business Meaning Test   | ✅ Encodes "how many steps are acceptable" — a policy decision       |
| Configurable Value Test | ✅ Threshold value                                                   |
| Audit Chain Test        | ✅ Affects audit chain length                                        |

**Conclusion**: **Policy**. The default value is acceptable as a fallback, but the actual value should come from JSON.

**Current State**: Correctly implemented — `eval_config.json` defines `max_steps`, Rust provides a fallback.

---

## 8. Implementation Guidelines

### 8.1 When Adding a New Mechanism

1. **Verify determinism**: Ensure the change doesn't introduce non-determinism
2. **Verify no policy encoding**: Ensure no business rules or thresholds are encoded
3. **Add tests**: Verify the optimization doesn't change behavior
4. **Update documentation**: Add comments explaining the mechanism vs. policy distinction

### 8.2 When Adding New Policy Configuration

1. **Define in JSON**: Place all configurable values in JSON files
2. **Read in Rust**: Read values from JSON; provide sensible defaults
3. **Document the mapping**: Clearly document which JSON fields map to which Rust fields
4. **Add validation**: Validate that policy values are within acceptable ranges

### 8.3 When in Doubt

If you're unsure whether a change is mechanism or policy:

1. Apply the four test cases above
2. Ask: "Would a human need to review this change as part of business logic?"
3. If yes → Policy (must be in JSON)
4. If no → Mechanism (can be in Rust)

---

## 9. Design Principles

### 9.1 Determinism First

All mechanism optimizations must preserve determinism. A mechanism that introduces non-determinism is not allowed, even if it improves performance.

### 9.2 Transparency

Mechanism optimizations should be transparent to users and auditors. The audit chain should record the same information regardless of whether optimizations are enabled.

### 9.3 Configurability

All policy values must be configurable via JSON. Rust code should never contain hard-coded policy decisions.

### 9.4 Separation of Concerns

- **TCB**: Provides deterministic execution primitives and core mechanisms
- **Governance**: Provides policy configuration and execution framework
- **JSON Rules**: Define business logic and orchestration

---

## 9. Code Location Index

| Component             | File                                                 | Mechanism/Policy                 |
| --------------------- | ---------------------------------------------------- | -------------------------------- |
| State struct          | `crates/tcb/src/state.rs`                            | Mechanism (data structure)       |
| State version counter | `crates/tcb/src/state.rs`                            | Mechanism (optimization)         |
| EvalConfig            | `crates/governance-core/src/engine/equation.rs`      | Mechanism (configuration loader) |
| eval_config.json      | `crates/governance-core/src/engine/eval_config.json` | Policy (config values)           |
| RuleExecutor          | `crates/governance-core/src/rule_executor.rs`        | Mechanism (execution framework)  |
| execute_until_stable  | `crates/governance-core/src/rule_executor.rs`        | Mechanism (iteration logic)      |
| JSON rules            | `rules/` directory                                   | Policy (business logic)          |

---

## 10. Significance

### 10.1 For Performance

This distinction allows the execution engine to be optimized for performance without sacrificing determinism or auditability. Version counters, efficient data structures, and adaptive scheduling can all be implemented in Rust while keeping business logic in JSON.

### 10.2 For Maintainability

Clear boundaries make it easier to:

- Understand what can be changed in Rust vs. JSON
- Review code changes for policy violations
- Optimize the engine without modifying business logic
- Test performance improvements independently of business rules

### 10.3 For Trust

By separating mechanism from policy, we create a clearer trust boundary:

- **Trusted**: The execution engine (mechanisms)
- **Auditable**: Business policy (JSON rules)
- **Verifiable**: The mapping between them

This means users can trust that the engine executes rules correctly and efficiently, while still being able to audit and verify the rules themselves.

---

## 11. Revision History

| Version | Date       | Changes                                                                         |
| ------- | ---------- | ------------------------------------------------------------------------------- |
| v1.0    | 2026-07-09 | Initial version. Defines mechanism/policy boundary and reinterprets G-13/GG-31. |
