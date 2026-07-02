# EvoRule Determinism Classification Standard

> **Version**: 1.2 | **Created**: 2026-06-30
> **Purpose**: Unified conceptual framework for the EvoRule team — resolving all ambiguity around the term "determinism".
> **Source**: Derived from `EvoRule_correctness_definition_discussion.md`, formalized as the team standard.

---

## 0. Why This Document Exists

Within the EvoRule team, "determinism" is the most frequently used and most easily misunderstood concept. Common scenarios:

- Someone says "this function is non-deterministic," actually referring to "the external library it depends on may behave differently across versions."
- Someone says "the TCB is non-deterministic," actually referring to "this functionality doesn't belong in the TCB."
- Someone says "regex is non-deterministic," actually referring to "different regex engines may interpret the same pattern differently."

All of these discussions have value, but they address **different problems**, all compressed into the single word "determinism." The result:

- Discussions fail to converge
- Fixes target the wrong issues
- LLMs are misled

**The purpose of this document: to provide a layered definition of "determinism," enabling precise communication rather than vague debates.**

---

## 1. Four Layers of Determinism

| Layer | Name | Definition | Criterion |
|-------|------|------------|-----------|
| **L1** | **Computational Determinism** | Same input → same output, guaranteed by algorithm, mathematical definition, or language specification | No randomness, no wall-clock time, no UB |
| **L2** | **Version Determinism** | Under a fixed dependency version, same input → same output; version upgrades may change behavior | Dependencies locked, upgrades require review |
| **L3** | **Platform Determinism** | Under the same Rust target platform, same input → same output | Only commits to current platform behavior |
| **L4** | **Ecological Determinism** | Rust implementation behaves identically to other language implementations (JS/Python/Go) | Cross-language consistency tests pass |

### Core Principles

- **The TCB commits only to L1 (Computational Determinism).** This is its core responsibility and reason for existence.
- **L2 (Version Determinism) and L3 (Platform Determinism)** are engineering management issues — managed via "locked versions + CI testing" — and do not constitute "non-determinism."
- **L4 (Ecological Determinism) is an optional enhancement target, NOT a default TCB commitment.** L4 validation is only required when a function is explicitly marked as "requiring cross-language consistency." Unmarked functions do not consider cross-language differences as defects. **L4 must not be used to deny TCB determinism.**

### Prerequisite for L1: Input Contract

L1 defines "same input → same output," but the premise is that "input is normalized." For functions that accept external input (e.g., `serde_json_to_value`), the following must be explicitly declared:

- **Accepted input subset**: e.g., reject duplicate keys, reject integers outside `i64` range
- **Normalization rules**: e.g., all integers must be within `i64` range, floats must be finite
- **Handling of illegal input**: Reject (return error) vs. Normalize (NaN → 0.0)

**For functions without a declared input contract, L1 determinism only holds for "normalized inputs."** JSON itself has ambiguities (duplicate keys, large integer precision loss, floating-point representation) — these ambiguities must be eliminated before entering the TCB.

### Compositional Determinism (Cross-Layer)

L1 determinism of an individual primitive **does not automatically compose** to composite structures. The determinism of control-flow primitives (`while_loop`, `try_catch`, `execute_parallel`) depends on "the determinism of the invoked instructions."

**Judgment rule**: The L1 status of a control-flow primitive = `min(own L1, L1 of all invocable instructions)`.

Example: `while_loop`'s own algorithm is L1 ✅, but if the registry contains an instruction that reads system time, calling it degrades the whole to L1 ❌. **The TCB currently has no "instruction determinism whitelist" mechanism**, so the L1 status of control-flow primitives is **conditionally deterministic** — dependent on the discipline of external callers.

---

## 2. Judgment Workflow

When someone says "this functionality is non-deterministic," follow this flow:


Step 0】Is the input contract declared?
│
├─ No → Declare the input contract first; otherwise L1 assessment cannot proceed
│
└─ Yes → Continue
│
【Step 1】Is this an L1 (Computational Determinism) issue?
│
├─ Yes → True determinism defect → Fix immediately (bug)
│
└─ No → Continue
│
【Step 1.5】Is this a compositional determinism issue? (control-flow primitive calling non-deterministic instructions)
│
├─ Yes → Harden registry whitelist, or restrict the invocable instruction set
│
└─ No → Continue
│
【Step 2】Is this an L2 (Version Determinism) issue?
│
├─ Yes → Lock dependency versions → Document declaration
│
└─ No → Continue
│
【Step 3】Is this an L3 (Platform Determinism) issue?
│
├─ Yes → Confirm target platform → Document declaration
│
└─ No → Continue
│
【Step 4】Is this an L4 (Ecological Determinism) issue?
│
├─ Yes → Only required if the function is marked "requires cross-language consistency" → Do NOT deny TCB determinism
│
└─ No → Re-examine the issue; it may not be a "determinism" problem at all
│
【Fallback】If none of the above:
│
└─ Likely a responsibility boundary issue (doesn't belong here), not a determinism issue


---

## 3. Case Evaluations

### `content_hash` (SHA-256)

| Layer | Status | Note |
|-------|--------|------|
| L1 | ✅ Deterministic | SHA-256 is defined by FIPS 180-4 |
| L2 | ✅ Deterministic | `sha2` crate version locked |
| L3 | ✅ Deterministic | SHA-256 consistent across all platforms |
| L4 | ⚠️ Needs declaration | Rust/JS SHA-256 consistent, but requires testing verification |

**Conclusion: Deterministic.**

---

### `LogicalClock::tick`

| Layer | Status | Note |
|-------|--------|------|
| L1 | ✅ Deterministic | `u64` increment, no randomness, no wall-clock, no UB |
| L2 | ✅ Deterministic | No external dependencies |
| L3 | ✅ Deterministic | Platform-independent |
| L4 | ✅ Deterministic | Behavior is intuitive |

**Conclusion: Deterministic.**

---

### `Value::Float` (`OrderedFloat`)

| Layer | Status | Note |
|-------|--------|------|
| L1 | ✅ Deterministic | `OrderedFloat` normalizes NaN to 0.0, comparison semantics are deterministic |
| L2 | ✅ Deterministic | Lock `ordered-float` version |
| L3 | ✅ Deterministic | IEEE 754 guarantees basic arithmetic (`+ - * /`) cross-platform consistency; Rust uses SSE2 (not x87), no extended precision issues. TCB does not use transcendental functions (`sin`/`cos`/`pow`/`sqrt`), no last-bit differences. |
| L4 | ⚠️ Needs declaration | Cross-language float comparison requires documented declaration (not a default TCB commitment) |

**Conclusion: L1~L3 deterministic. L4 only required if marked "requires cross-language consistency."**

> **Correction Note**: Earlier versions incorrectly marked L3 as ⚠️ "f64 operations may have different rounding on different CPUs." This was incorrect — IEEE 754 guarantees basic arithmetic cross-platform consistency, and Rust does not use x87 80-bit extended precision. Attaching "possible different rounding" to `Value::Float` creates unnecessary anxiety.

---

### `Domain::Matches` (Regex Matching)

| Layer | Status | Note |
|-------|--------|------|
| L1 | ⚠️ Implementation-dependent | Regular expressions have **no unified specification** (POSIX BRE, PCRE, RE2, ECMAScript Regex all differ). The `regex` crate uses RE2 semantics — this is an **implementation contract**, not a **specification guarantee**. Matching results are deterministic under a fixed engine, but this is "implementation-determined" rather than "specification-determined." |
| L2 | ⚠️ Must lock version | Different `regex` crate versions may change Unicode character class semantics (e.g., `\w`) |
| L3 | ✅ Deterministic | Cross-platform consistent |
| L4 | ❌ Inconsistent | Rust `regex` (RE2 semantics) and JS/Python (PCRE-like) regex engine semantics are fundamentally different |

**Conclusion: Conditionally deterministic (lock version + document RE2 semantics). This is an L1 boundary case — "implementation-determined" rather than "specification-determined." If the team requires "specification-determined," restrict to ASCII subset or remove the feature.**

> **Correction Note**: Earlier versions marked L1 as ✅ "given the regex engine, matching results are determined by the algorithm." By this document's own L1 definition ("guaranteed by specification"), this is invalid — regex has no unified specification, RE2 is an implementation choice.

---

### `DeterministicRNG` (Deterministic PRNG)

| Layer | Status | Note |
|-------|--------|------|
| L1 | ✅ Deterministic | LCG (Linear Congruential Generator) algorithm, same seed → same sequence |
| L2 | ✅ Deterministic | No external dependencies |
| L3 | ✅ **Fixed (v1.1)** | Original defect: `choice` and `shuffle` used `next_u64() as usize`. `usize` is 4 bytes on 32-bit platforms, 8 bytes on 64-bit. 32-bit platforms would truncate the high 32 bits, causing `choice`/`shuffle` sequences to **differ** from 64-bit platforms. **Fix**: Changed to modulo within `u64` domain: `next_u64() % (len as u64)`, finally `as usize` for indexing. Cross-platform sequences are now consistent. |
| L4 | ⚠️ Needs declaration | LCG parameters need cross-language documentation if used. |

**Conclusion: L1~L3 deterministic. L4 only required if marked "requires cross-language consistency" (document LCG parameters).**

> **Fix Record**: v1.1 fixed the `as usize` truncation defect. Code location: `crates/tcb/src/deterministic.rs:208` (`choice`), `crates/tcb/src/deterministic.rs:216` (`shuffle`). Fix method: `(self.next_u64() % (items.len() as u64)) as usize`. 517 tests all pass, no regression.

---

### `detect_conflicts` (Conflict Detection Algorithm)

| Layer | Status | Note |
|-------|--------|------|
| L1 | ✅ Deterministic | O(n²) domain intersection comparison. **Output order verified deterministic**: rule list traversed by `im::Vector` index (not HashMap), conflicts pushed in `i<j` double-loop order, `rule_id` order in error messages is deterministic. |
| L2 | ✅ Deterministic | No external dependencies |
| L3 | ✅ Deterministic | Pure algorithm, platform-independent |
| L4 | ✅ Deterministic | Algorithm is explicit |

**Conclusion: Algorithm L1 deterministic, but responsibility boundary overstepped (should not be in TCB). This is a responsibility boundary issue, NOT a determinism issue.**

> **Verification Note**: Earlier versions marked L1 ✅ but did not verify output order. Confirmed `detect_conflicts` (`inference_ops.rs:34-90`) uses `im::Vector` index traversal + sequential push, no `HashMap` iteration order dependence. The message filling order is determined by the `i<j` loop — output order is canonical, deterministic.

---

## 4. Team Collaboration Standards

### 4.1 Daily Communication

When discussing "determinism" issues, the layer must be explicitly specified:

- ✅ "Is `regex` matching deterministic under version `1.10`?" (L2 issue)
- ✅ "Will `content_hash` produce the same result in Rust and Python?" (L4 issue)
- ✅ "Does `detect_conflicts` violate responsibility boundaries by being in TCB?" (Not a determinism issue)
- ✅ "Is `while_loop` still deterministic after invoking registered instructions?" (Compositional determinism, Step 1.5)
- ✅ "Is `DeterministicRNG::choice`'s sequence consistent across 32-bit and 64-bit platforms?" (L3 issue)
- ✅ "What does `serde_json_to_value` do with duplicate keys in JSON?" (Input contract, Step 0)

- ❌ "This function is non-deterministic." (Too vague to discuss)
- ❌ "The TCB is non-deterministic." (Confuses L1~L4 with responsibility boundaries)

### 4.2 Code Review

PRs involving "determinism" assertions MUST include layer annotations:

```rust
/// This function is L1 (Computational Determinism) deterministic:
/// - Uses SHA-256 (FIPS 180-4)
/// - No randomness, no wall-clock, no UB
/// - `sha2` crate version locked at 0.10.x
pub fn content_hash(...) -> String { ... }

Control-flow primitives (while_loop, try_catch, execute_parallel) PRs MUST additionally declare:

/// L1 conditionally deterministic: own algorithm is deterministic,
/// but overall determinism depends on the L1 status of all registered
/// instructions in the registry. Callers must ensure no non-deterministic
/// instructions are registered.
pub fn exec_while_loop(...) -> Result<State, EvoRuleError> { ... }


4.3 Documentation Declarations
L1 Input Contracts: Functions accepting external input MUST declare accepted input subsets and normalization rules.

L2 Dependencies: Lock versions in Cargo.toml and document accordingly.

L3 Platform Dependencies: Document supported platforms; code using usize/isize requires special attention.

L4 Cross-Language Behavior: Only required for functions marked "requires cross-language consistency" — provide test verification.

5. Relationship to EvoRule Programming Specification
This standard refines Chapter 7 ("Determinism Constitutional Engineering") of the EvoRule Programming Specification.

Specification Section	Role of This Standard
§7.1 Determinism is not "try your best", it's "must achieve"	Provides criteria for "how much" must be achieved
§7.2 Alternatives to non-deterministic APIs	Provides basis for judging "why the alternative is deterministic"
§7.3 Determinism boundary	Clearly defines where the "boundary" lies
6. Version History
Version	Date	Notes
1.0	2026-06-30	Initial release. Based on team self-calibration results.
1.1	2026-06-30	Post-code-review corrections: ① Corrected Value::Float L3 assessment (IEEE 754 basic arithmetic cross-platform consistent, not "possible different rounding"); ② Added DeterministicRNG case identifying real L3 as usize 32-bit truncation defect; ③ Corrected Domain::Matches L1 assessment (regex has no unified specification, "implementation-determined" not "specification-determined"); ④ Added "Compositional Determinism" section and judgment Step 1.5; ⑤ Added "Input Contract" section and judgment Step 0; ⑥ Clarified L4 as optional enhancement target, not TCB default commitment; ⑦ Verified detect_conflicts output order determinism (im::Vector index traversal + sequential push).
1.2	2026-06-30	Code fix sync: DeterministicRNG::choice/shuffle as usize truncation defect fixed in code (changed to u64 domain modulo). L3 status updated from ⚠️ "needs fix" to ✅ "fixed". 517 tests all pass, no regression.
End of Document

This standard aims to end ambiguous debates about "determinism." It is not intended to add process burden, but to ensure team discussions precisely target specific issues, avoiding time wasted on vague concepts.

