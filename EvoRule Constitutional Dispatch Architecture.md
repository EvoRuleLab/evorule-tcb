**EvoRule Constitutional Dispatch Architecture**

> **Document Level**: Architecture Guidance  
> **Status**: Supersedes all prior design proposals and serves as the sole architectural source of truth for the system going forward  
> **Effective Date**: 2026-07-05  
> **Scope**: Full stack of evorule-tcb + evorule-governance  
> **Change Threshold**: The architecture described herein involves TCB ER-605 exceptions (§9); any changes must undergo architectural review and may not be bypassed

---

## 1. Design Philosophy

### 1.1 Fundamental Motivation for the Refactoring

**Unsustainability of the current architecture**: `core_eval.json` and `eval_config.json` will grow **indefinitely** as the system and business rules evolve, with no mechanism to externalise them:

- Every new primitive → modify the `primitives` section of `eval_config.json`
- Every new business alias (e.g., `increment`, `conditional`) → modify `dispatch.params.cases` in `core_eval.json`
- All cases knowledge is **centralised in a single file** – no modular extension mechanism
- Both TCB improvements and business evolution touch these two files, violating the goal of "constitutional immutability"

**The fundamental goal of this refactoring**: Make `core_eval.json` and `eval_config.json` truly immutable "constitutions" – distribute cases knowledge to **each business module for self‑registration**, establishing an external extension mechanism.

### 1.2 Five Guiding Principles

| Principle | Description | Implementation Location |
| --------- | ----------- | ----------------------- |
| **Constitution Immutable** | `core_eval.json` reduced to ~30 lines of execution skeleton, `eval_config.json` reduced to ~50 lines of environment config – never to be modified again | G‑2, G‑3 streamlined JSON |
| **Business Pluggable** | All business primitives (Formula/Algebra/Inference/IO) are self‑registered at runtime by their respective modules | `register_*_primitives` in governance modules |
| **Dispatch Auto‑Generated** | `dispatch_cases` are built dynamically at startup by governance scanning the registry and injected into `__exec__` | G‑1 `DispatchTableBuilder` |
| **Cases Knowledge Decentralised** | Alias/placeholder and other business semantics are self‑registered by the corresponding business modules – no centralised growth point | `engine/aliases.rs` + Layer‑2 modules |
| **Zero‑Config Extension** | Adding a new business primitive only requires registration in Rust – no JSON changes whatsoever | `register_all` call chain |

### 1.3 Key Differences from the Old Architecture

| Dimension | Old Architecture | New Architecture (this document) |
| --------- | ----------------- | -------------------------------- |
| `core_eval.json` | Contained `dispatch.params.cases` (~600 lines of business mappings) | Only execution skeleton (~30 lines); cases completely removed |
| `eval_config.json` | Contained `primitives` section (~200 lines of primitive declarations) | Only environment config (~50 lines); primitives completely removed |
| Dispatch main path | Reads static cases from `instruction.params.cases` | **Dual‑source read**: main dispatch reads `__exec__.dispatch_cases`, sub‑dispatch reads `instruction.params.cases` |
| Cases knowledge location | Centralised in `core_eval.json` | Distributed across `engine/aliases.rs` + Layer‑2 modules |
| Primitive registration | JSON‑driven (`create_full_registry_from_config`) | Direct Rust calls (`create_full_registry()`) |
| Audit traceability | Only logs instruction execution | Each audit record carries dispatch table version `[tbl:<hash>]` |
| ER‑605 touches | 0 | 2 (registered exceptions, see §9) |

### 1.4 Design Boundaries

This architecture **does not change system business logic** – it only refactors implementation:

- All primitive behaviours remain unchanged
- All instruction execution results remain unchanged
- All audit record HMAC computation remains unchanged
- All external APIs remain unchanged
- **Sub‑dispatch functionality is fully preserved**: the ability for users to construct a `dispatch` instruction with custom cases remains unchanged

What changes is "where the dispatch table comes from" and "how the dispatch table version is audited".

---

## 2. System Layering

```
┌─────────────────────────────────────────────────────────────┐
│                    Layer 2: Business Modules                │
│  (governance-inference, governance-solver, governance-gate) │
│                                                             │
│  Responsibilities:                                          │
│  1. Self‑register business primitives into the registry    │
│  2. Self‑register business aliases/placeholder cases       │
│     into DispatchTableBuilder                               │
│  Interfaces: register_*_primitives(&mut registry)          │
│              register_*_aliases(&mut builder)               │
└───────────────────────────┬─────────────────────────────────┘
                            │ registered at startup
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                  Layer 1: Governance Core                   │
│                  (governance-core crate)                    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  TheEquation                                         │   │
│  │  ├── new() / from_builtin()                          │   │
│  │  │   └── create_full_registry()  ← TCB hard‑coded    │   │
│  │  │   └── register_*_primitives()  ← Layer‑1 business │   │
│  │  ├── build_dispatch_table()      ← builds dispatch   │   │
│  │  │   ├── DispatchTableBuilder                        │   │
│  │  │   │   ├── auto_from_registry()  ← Category A      │   │
│  │  │   │   ├── register_alias()      ← Category B      │   │
│  │  │   │   ├── register_placeholder() ← Category C     │   │
│  │  │   │   └── set_default()        ← Category E      │   │
│  │  │   └── compute version via content_hash            │   │
│  │  ├── evaluate()                                      │   │
│  │  │   ├── injects __exec__.dispatch_cases             │   │
│  │  │   ├── injects __exec__.dispatch_default           │   │
│  │  │   ├── injects __exec__.dispatch_table_version     │   │
│  │  │   └── reg.execute(core.eval)                      │   │
│  │  └── execute_instruction()                           │   │
│  │      └── (injects same three fields)                 │   │
│  └─────────────────────────────────────────────────────┘   │
└───────────────────────────┬─────────────────────────────────┘
                            │ depends on
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                       Layer 0: TCB                          │
│                     (evorule-tcb crate)                     │
│                                                             │
│  ┌──────────────────────────┐  ┌─────────────────────────┐ │
│  │  ExecContext              │  │  InstructionRegistry    │ │
│  │  ├── dispatch_cases       │  │  ├── create_full_registry│ │
│  │  ├── dispatch_default     │  │  ├── register_all       │ │
│  │  │   (new)                │  │  └── fallback path      │ │
│  │  ├── dispatch_table_      │  │      (reads             │ │
│  │  │   version (new)        │  │       dispatch_cases)   │ │
│  │  └── ...                  │  │                         │ │
│  └──────────────────────────┘  └─────────────────────────┘ │
│                                                             │
│  ┌──────────────────────────┐  ┌─────────────────────────┐ │
│  │  dispatch primitive       │  │  trace_step primitive   │ │
│  │  (dual‑source read:       │  │  (appends [tbl:<hash>] │ │
│  │   main reads __exec__,    │  │   to change_summary,    │ │
│  │   sub reads instruction.  │  │   not for sub‑dispatch) │ │
│  │   params.cases)           │  │                         │ │
│  └──────────────────────────┘  └─────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 Layer Responsibilities

**Layer 0 (TCB)**:

- Provides physical primitives (`state_set`, `dispatch`, `while_loop`, `trace_step`, etc.)
- Provides the `ExecContext` data structure (including `dispatch_cases`, `dispatch_default`, `dispatch_table_version`)
- Provides `create_full_registry()` to hard‑code register all TCB primitives
- The `dispatch` primitive uses **dual‑source reading** (ER‑605 exception #1):
  - Main dispatch: reads from `__exec__.dispatch_cases` + `__exec__.dispatch_default`
  - Sub‑dispatch: reads from `instruction.params.cases` + `instruction.params.default`
- The `trace_step` primitive appends the dispatch table version to `change_summary` (ER‑605 exception #2; only for main dispatch)

**Layer 1 (Governance Core)**:

- Calls `create_full_registry()` at startup to register TCB primitives
- Calls `register_planning_primitives`, `register_io_primitives`, `register_formula_primitives`, `register_algebra_primitives` to register Layer‑1 business primitives
- Uses `DispatchTableBuilder` to build the dispatch table:
  - `auto_from_registry()` generates Category A (direct‑mapped cases)
  - `register_core_aliases()` registers Category B (business aliases) and Category C (placeholders)
  - `set_default()` sets Category E (default)
- Computes the dispatch table version hash via `content_hash`
- Injects `dispatch_cases`, `dispatch_default`, `dispatch_table_version` into `__exec__`
- Executes the `core.eval` rule to drive the entire execution loop

**Layer 2 (Business Modules)**:

- Self‑register business primitives into the registry at startup
- Self‑register business aliases/placeholder cases into `DispatchTableBuilder` (extension point)
- Do not touch `core_eval.json`, `eval_config.json`, or `__exec__`
- New primitives automatically appear in the dispatch table (Category A); new aliases extend business semantics via registration (Categories B/C)

---

## 3. Data Structures

### 3.1 `__exec__` Context (including new fields)

```json
{
  "__exec__": {
    "instruction": { "type": "...", "params": { ... } },
    "queue": [],
    "__running": true,
    "metadata": {},
    "default_instruction": { "type": "noop", "params": {} },
    "termination_domain": { ... },
    "meta_instruction_types": ["state_set", "dispatch", "while_loop", ...],
    "audit_on": true,
    "drain_meta_trace": false,
    "dispatch_cases": { ... },
    "dispatch_default": { "type": "advance_instruction", "params": {} },
    "dispatch_table_version": "a1b2c3d4..."
  }
}
```

**New fields**:

| Field | Type | Description |
| ----- | ---- | ----------- |
| `dispatch_cases` | Object | System dispatch table (Category A/B/C cases), read by main dispatch |
| `dispatch_default` | Object | System default case body (Category E), executed when main dispatch misses |
| `dispatch_table_version` | String | Content hash (SHA‑256) of the dispatch table, for audit traceability |

**Note**: `dispatch_default` is a separate field – **not mixed into `dispatch_cases`** – to avoid naming conflicts with business cases.

### 3.2 Five Categories of Cases

Existing `core_eval.json` cases are divided into five semantic categories; the new architecture handles each differently:

| Category | Semantics | Examples | Count | Handling |
| -------- | --------- | -------- | ----- | -------- |
| A | Direct mapping (case key == type, 1:1 forwarding) | `state_set`, `state_compute`, `noop`, `trace_step`, `while_loop` | ~30 | `auto_from_registry()` auto‑generated |
| B | Aliases + field renaming + literals | `increment`(→state_compute), `set`(→state_set), `conditional`(→evaluate_domain) | 6 | Registered via `register_core_aliases()` |
| C | Placeholders (fallback when Layer‑2 not implemented) | `evaluate_expression`, `state_diff`, `state_merge` | 3 | Registered via `register_placeholder()` |
| D | `dispatch` itself (sub‑dispatch entry) | `dispatch` instruction | 1 | **Special case**: explicit `$ref` three fields, see §4.2 |
| E | Default branch | `default: advance_instruction` | 1 | `set_default()`, stored in `dispatch_default` |

**Key design decisions**:

- **Category D must be in the dispatch table, but with a special case**: `dispatch` is a registered primitive. If the dispatch table lacks a case for `dispatch`, then when a user instruction `instruction.type == "dispatch"` arrives, the main dispatch will fall through to default (`advance_instruction`), **completely breaking sub‑dispatch**. Therefore a special case for `dispatch` must be registered.
- **Category D cannot use `$pass` passthrough**: `$pass` performs wholesale replacement ([dispatch.rs:305](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L305)). If the user does not pass `cases`, `contains_key("cases")` is false → main dispatch → hits the `dispatch` case → passes through again → **infinite recursion**. We must use explicit `$ref` three fields, ensuring the case body always contains the `"cases"` key (even if its value is Null), so that the sub‑dispatch path is taken and safely exits.
- **Category A skips `dispatch`**: `auto_from_registry()` explicitly skips `"dispatch"`, letting the Category‑D special case take over, avoiding the direct‑mapped case overriding the special one.
- **Category E is stored independently**: the default is a dead‑lock protection semantic of `core.eval` itself, not a business case.

### 3.3 Passthrough Semantics (Category A case bodies)

Category‑A case bodies use the existing `$pass` primitive to forward all `instruction.params`:

```json
{
  "state_set": {
    "type": "state_set",
    "params": { "$pass": "" }
  }
}
```

**Difference between `$pass` and `$ref`** (based on [dispatch.rs:282-283, 384-407](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L282)):

- `{"$ref": "__exec__.instruction.params"}`: resolves to the params object, **with recursive chain resolution** (if the result itself is a `$ref`, it continues resolving)
- `{"$pass": ""}`: forwards the entire `__exec__.instruction.params` **without recursive resolution**
- `{"$pass": "attr"}`: forwards `__exec__.instruction.params.attr`, equivalent to `{"$ref": "__exec__.instruction.params.attr"}`

For Category A, `$pass` is correct – direct‑mapped primitives need raw params and should not be disturbed by chained resolution.

---

## 4. Dispatch Table Generation

### 4.1 DispatchTableBuilder Interface

```rust
// governance-core/src/engine/dispatch_table.rs (new)

use std::collections::BTreeMap;
use evorule_tcb::value::Value;
use evorule_tcb::instruction::registry::InstructionRegistry;
use evorule_tcb::deterministic::content_hash;

/// Dispatch table builder.
///
/// Builds the dispatch table according to the five categories in §3.2:
/// - Category A: auto_from_registry() auto‑generates direct‑mapped cases (skips dispatch)
/// - Category B: register_alias() registers business aliases
/// - Category C: register_placeholder() registers placeholder cases
/// - Category D: register_alias("dispatch", ...) registers the special case (explicit $ref three fields, see §4.2)
/// - Category E: set_default() sets the default
pub struct DispatchTableBuilder {
    cases: BTreeMap<String, Value>,
    default: Option<Value>,
}

impl DispatchTableBuilder {
    pub fn new() -> Self {
        Self { cases: BTreeMap::new(), default: None }
    }

    /// Category A: scan the registry and auto‑generate direct‑mapped cases
    ///
    /// For each registered primitive type_name, generate:
    ///   { "type": type_name, "params": { "$pass": "" } }
    ///
    /// **Critical**: skips "dispatch", which is taken over by the Category‑D special case (see §4.2).
    ///
    /// **Why we cannot use `$pass` to generate a direct‑mapped case for dispatch**:
    /// `$pass` does wholesale replacement ([dispatch.rs:305](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L305)).
    /// If the user does not pass cases, `contains_key("cases")` is false → main dispatch → hits the dispatch case
    /// → passes through again → main dispatch → **infinite recursion**. Therefore dispatch must be registered
    /// as a special case by `register_core_aliases` using explicit `$ref` three fields, ensuring the case body
    /// always contains the "cases" key (even if the value is Null), thus triggering the sub‑dispatch path
    /// to safely exit.
    pub fn auto_from_registry(&mut self, registry: &InstructionRegistry) -> &mut Self {
        for type_name in registry.all_type_names() {
            if type_name == "dispatch" {
                continue;  // Category D: skip; special case registered by register_core_aliases
            }
            let case = Value::Object(im::hashmap! {
                "type".to_string() => Value::string(type_name.clone()),
                "params".to_string() => Value::Object(im::hashmap! {
                    "$pass".to_string() => Value::string(""),
                }),
            });
            self.cases.insert(type_name, case);
        }
        self
    }

    /// Category B: register a business alias case
    ///
    /// `alias` is the user instruction name; `case_body` is the full case definition (type + params).
    /// `$ref` inside `case_body` is resolved by the `dispatch` primitive's resolve_refs.
    pub fn register_alias(&mut self, alias: &str, case_body: Value) -> &mut Self {
        self.cases.insert(alias.to_string(), case_body);
        self
    }

    /// Category C: register a placeholder case (fallback when Layer‑2 not implemented)
    ///
    /// A placeholder case body is `advance_instruction`, ensuring that an unimplemented instruction
    /// does not dead‑lock the while_loop.
    pub fn register_placeholder(&mut self, name: &str) -> &mut Self {
        let case = Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
            "params".to_string() => Value::empty_object(),
        });
        self.cases.insert(name.to_string(), case);
        self
    }

    /// Category E: set the default (core.eval's own semantics)
    ///
    /// The default is executed when main dispatch misses any case, preventing the while_loop from dead‑locking.
    pub fn set_default(&mut self, default: Value) -> &mut Self {
        self.default = Some(default);
        self
    }

    /// Build the final dispatch table + default + version hash
    ///
    /// Determinism guarantee: BTreeMap ensures key ordering; content_hash internally sorts Object keys
    /// ([deterministic.rs:138-140]), so the same input produces the same version hash.
    pub fn build(self) -> (Value, Value, String) {
        let cases_val = Value::Object(
            self.cases.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        );
        let default_val = self.default.unwrap_or_else(|| Value::Object(im::hashmap! {
            "type".to_string() => Value::string("advance_instruction"),
            "params".to_string() => Value::empty_object(),
        }));
        // Compute version using content_hash (already deterministically sorted)
        let version = content_hash(&[cases_val.clone(), default_val.clone()]);
        (cases_val, default_val, version)
    }
}
```

### 4.2 dispatch Special Case + Business Alias Registration (Categories B/C/D)

```rust
// governance-core/src/engine/aliases.rs (new)

use crate::engine::dispatch_table::DispatchTableBuilder;
use evorule_tcb::value::Value;

/// Register governance-core's dispatch special case (Category D), business alias cases (Category B),
/// and placeholder cases (Category C).
///
/// Category D: special case for dispatch itself
///   - Must use explicit $ref three fields; cannot use $pass (see §3.2 key design decisions)
///   - The case body always contains the "cases" key, ensuring contains_key("cases") is true → sub‑dispatch path
///   - When the user does not pass cases, the cases field resolves to Null → not an Object → safe exit, no infinite recursion
///
/// Category B: aliases + field renaming + literals
///   - increment/decrement: map to state_compute, inject operation literals
///   - set: map to state_set
///   - sequence: map to push_instruction_sequence, subtransforms→instructions
///   - conditional: map to evaluate_domain, then→on_true, else→on_false
///   - execute_rule: map to apply_rule
///
/// Category C: placeholders (fallback when Layer‑2 not implemented)
///   - evaluate_expression / state_diff / state_merge
pub fn register_core_aliases(builder: &mut DispatchTableBuilder) {
    // ── Category D: special case for dispatch itself ──
    //
    // **Critical**: must use explicit $ref three fields; cannot use $pass!
    //
    // Scenario walk‑through (abnormal input where user does not pass cases):
    //   1. while_loop pops {type:"dispatch", params:{key:"dispatch"}} (no cases)
    //   2. core.eval dispatch → main dispatch → hits this case
    //   3. resolve_refs resolves explicit $ref:
    //      - cases → __exec__.instruction.params.cases → Null (path does not exist)
    //      - default → __exec__.instruction.params.default → Null
    //   4. case_instr = {type:"dispatch", params:{key, cases:Null, default:Null}}
    //   5. reg.execute → exec_dispatch → contains_key("cases")=true → sub‑dispatch
    //   6. user_cases=Null → not an Object → no case lookup
    //   7. default=Null → instruction.params.get("default")=Some(Null) → resolve_refs(Null)=Null
    //   8. GenericInstruction::from_value(Null) errors or returns a default → safe exit, no infinite recursion
    //
    // If we used $pass passthrough, the catastrophic consequence:
    //   case body = {type:"dispatch", params:{"$pass":""}}
    //   → resolve_pass replaces the entire params with __exec__.instruction.params
    //   → case_instr.params = {key:"dispatch"} (no "cases" key)
    //   → contains_key("cases")=false → main dispatch → hits this case → passes through again → infinite recursion
    builder.register_alias("dispatch", Value::Object(im::hashmap! {
        "type".to_string() => Value::string("dispatch"),
        "params".to_string() => Value::Object(im::hashmap! {
            "key".to_string() => Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("__exec__.instruction.params.key"),
            }),
            "cases".to_string() => Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("__exec__.instruction.params.cases"),
            }),
            "default".to_string() => Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string("__exec__.instruction.params.default"),
            }),
        }),
    }));

    // ── Category B: business aliases ──
    builder.register_alias("increment", json_case("state_compute", &[
        ("attr", "$ref:__exec__.instruction.params.attr"),
        ("operation", "literal:add"),
        ("value", "$ref:__exec__.instruction.params.delta"),
    ]));
    builder.register_alias("decrement", json_case("state_compute", &[
        ("attr", "$ref:__exec__.instruction.params.attr"),
        ("operation", "literal:sub"),
        ("value", "$ref:__exec__.instruction.params.delta"),
    ]));
    builder.register_alias("set", json_case("state_set", &[
        ("attr", "$ref:__exec__.instruction.params.attr"),
        ("value", "$ref:__exec__.instruction.params.value"),
    ]));
    builder.register_alias("sequence", json_case("push_instruction_sequence", &[
        ("instructions", "$ref:__exec__.instruction.params.subtransforms"),
    ]));
    builder.register_alias("conditional", json_case("evaluate_domain", &[
        ("domain", "$ref:__exec__.instruction.params.domain"),
        ("on_true", "$ref:__exec__.instruction.params.then"),
        ("on_false", "$ref:__exec__.instruction.params.else"),
    ]));
    builder.register_alias("execute_rule", json_case("apply_rule", &[
        ("rule", "$ref:__exec__.instruction.params.rule"),
        ("state", "$ref:__exec__.instruction.params.state"),
        ("result_attr", "$ref:__exec__.instruction.params.result_attr"),
    ]));

    // ── Category C: placeholders (map to advance_instruction until Layer‑2 implements them) ──
    builder.register_placeholder("evaluate_expression");
    builder.register_placeholder("state_diff");
    builder.register_placeholder("state_merge");
}

/// Helper: construct a case body
fn json_case(type_name: &str, fields: &[(&str, &str)]) -> Value {
    let mut params: im::HashMap<String, Value> = im::HashMap::new();
    for (case_field, source) in fields {
        if let Some(literal) = source.strip_prefix("literal:") {
            params.insert(case_field.to_string(), Value::string(literal.to_string()));
        } else if let Some(ref_path) = source.strip_prefix("$ref:") {
            params.insert(case_field.to_string(), Value::Object(im::hashmap! {
                "$ref".to_string() => Value::string(ref_path.to_string()),
            }));
        }
    }
    Value::Object(im::hashmap! {
        "type".to_string() => Value::string(type_name.to_string()),
        "params".to_string() => Value::Object(params),
    })
}
```

### 4.3 Dispatch Table Construction at Startup

```rust
// equation.rs build_dispatch_table() (replaces extract_cases_table)

fn build_dispatch_table(&self) -> (Value, Value, String) {
    let mut builder = DispatchTableBuilder::new();
    builder.auto_from_registry(&self.registry);
    crate::engine::aliases::register_core_aliases(&mut builder);
    // Category E: default is core.eval's own semantics; fixed as advance_instruction
    builder.set_default(Value::Object(im::hashmap! {
        "type".to_string() => Value::string("advance_instruction"),
        "params".to_string() => Value::empty_object(),
    }));
    builder.build()
}
```

**Layer‑2 extension point**: Layer‑2 modules that need to register business aliases can do so via an `Equation`‑exposed `register_aliases` hook (future extension, see §11).

### 4.4 Determinism Guarantee

**No separate `sort_value_keys` implementation is needed**. We reuse the existing `content_hash`:

- `DispatchTableBuilder` internally uses `BTreeMap<String, Value>` for cases – keys are already sorted
- `content_hash` internally sorts Object keys in `value_to_bytes` ([deterministic.rs:138-140](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/deterministic.rs#L138))
- The same registry state + the same alias registration order → the same `dispatch_table_version`

**Determinism of alias registration order**: the registration order in `register_core_aliases()` is fixed (source‑code order). Registration order for Layer‑2 modules is determined by the call order inside `TheEquation::new()`, which must be fixed as specified in §7.

---

## 5. Audit Chain Integrity

### 5.1 Design Constraint

**Governance cannot directly write to the audit chain**: the audit chain is managed by TCB's `AuditRecord` + `AuditChainState`; the governance layer can only operate via primitives.

**Solution**: Carry dispatch table version information through the `trace_step` primitive's `change_summary` field.

### 5.2 trace_step change_summary Construction

**Current format** ([audit_ops.rs:162-165](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/primitive/audit_ops.rs#L162)):

```rust
let change_summary = match instruction_type {
    Some(ref itype) if itype != label => Some(format!("[{itype}] {label}")),
    _ => Some(label.to_string()),
};
```

**New format** (version appended only for main dispatch):

```rust
let is_sub_dispatch = state
    .get("__exec__")
    .and_then(|v| v.get("instruction"))
    .and_then(|v| v.get("params"))
    .map(|p| p.as_object().map(|m| m.contains_key("cases")).unwrap_or(false))
    .unwrap_or(false);

let table_version = if is_sub_dispatch {
    ""  // sub‑dispatch: do not append version (executes user cases, not system dispatch table)
} else {
    state
        .get("__exec__")
        .and_then(|v| v.get("dispatch_table_version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
};

let change_summary = match (instruction_type.as_ref(), table_version.is_empty()) {
    (Some(itype), false) if itype != label =>
        Some(format!("[{itype}][tbl:{table_version}] {label}")),
    (Some(itype), true) if itype != label =>
        Some(format!("[{itype}] {label}")),
    (None, false) =>
        Some(format!("[tbl:{table_version}] {label}")),
    _ => Some(label.to_string()),
};
```

### 5.3 Audit Record Examples

**Main dispatch** (system dispatch table):

```
[increment][tbl:a1b2c3d4] apply_rule:meta_actions
```

**Sub‑dispatch** (user cases, no version appended):

```
[state_set] apply_rule:meta_actions
```

### 5.4 Version Traceability

The `dispatch_table_version` enables:

- Audit replay: reconstruct the dispatch table at that time from the version hash (requires saved snapshots – future extension)
- Anomaly diagnosis: compare versions between two executions to locate differences caused by dispatch table changes
- Integrity checks: compute the version at startup; if the dispatch table is tampered with at runtime, version mismatch can be detected

---

## 6. TCB Change Specifications (ER‑605 Exceptions)

### 6.1 dispatch.rs: Dual‑Source Reading (ER‑605 Exception #1)

**Change location**: `dispatch_by_cases` function at [dispatch.rs:115-119](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L115)

**Current implementation**:

```rust
let cases = instruction
    .params
    .get("cases")
    .cloned()
    .unwrap_or(Value::empty_object());
```

**New implementation** (dual‑source read):

```rust
// Dual‑source cases reading:
// - Sub‑dispatch: instruction.params.cases exists → use user's cases + user's default
// - Main dispatch: instruction.params.cases does not exist → use system dispatch table + system default
//
// Use contains_key rather than get().is_some() to distinguish "explicit empty cases" from "cases not provided"
let (cases, default) = if instruction.params.contains_key("cases") {
    // Sub‑dispatch: user‑provided cases
    let user_cases = instruction.params.get("cases").cloned()
        .unwrap_or(Value::empty_object());
    let user_default = instruction.params.get("default").cloned();
    (user_cases, user_default)
} else {
    // Main dispatch: system dispatch table
    let sys_cases = state
        .get("__exec__")
        .and_then(|v| v.get("dispatch_cases"))
        .cloned()
        .unwrap_or(Value::empty_object());
    let sys_default = state
        .get("__exec__")
        .and_then(|v| v.get("dispatch_default"))
        .cloned();
    (sys_cases, sys_default)
};
```

**Accompanying change for the default branch**: the original code at line 167 reads `instruction.params.get("default")`; change to use the `default` variable destructured above.

### 6.2 audit_ops.rs: Appending Version to change_summary (ER‑605 Exception #2)

**Change location**: [audit_ops.rs:162-165](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/primitive/audit_ops.rs#L162)

**New implementation**: see code in §5.2.

### 6.3 Test Fixture Rewrites

**Affected tests**: 8 dispatch tests in [dispatch.rs:1090-1600](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L1090).

**Rewrite pattern**:

```rust
// Old pattern: cases constructed inside instruction.params.cases
let dispatch_instr = GenericInstruction::from_value(&Value::Object(im::hashmap! {
    "type".to_string() => Value::string("dispatch"),
    "params".to_string() => Value::Object(im::hashmap! {
        "key".to_string() => Value::string("increment"),
        "cases".to_string() => Value::Object(im::hashmap! { ... }),
        "default".to_string() => Value::Object(im::hashmap! { ... }),
    }),
}));

// New pattern (main dispatch test): inject __exec__.dispatch_cases + dispatch_default
let dispatch_cases = Value::Object(im::hashmap! {
    "increment".to_string() => Value::Object(im::hashmap! { ... }),
});
let dispatch_default = Value::Object(im::hashmap! {
    "type".to_string() => Value::string("advance_instruction"),
    "params".to_string() => Value::empty_object(),
});
let state = state.set("__exec__", Value::Object(im::hashmap! {
    "dispatch_cases".to_string() => dispatch_cases,
    "dispatch_default".to_string() => dispatch_default,
    "instruction".to_string() => /* current instruction */,
    // ... other fields ...
}));

let dispatch_instr = GenericInstruction::from_value(&Value::Object(im::hashmap! {
    "type".to_string() => Value::string("dispatch"),
    "params".to_string() => Value::Object(im::hashmap! {
        "key".to_string() => Value::string("increment"),
        // no cases → main dispatch
    }),
}));

// New pattern (sub‑dispatch test): keep the old style – cases in instruction.params.cases
let dispatch_instr = GenericInstruction::from_value(&Value::Object(im::hashmap! {
    "type".to_string() => Value::string("dispatch"),
    "params".to_string() => Value::Object(im::hashmap! {
        "key".to_string() => Value::string("true"),
        "cases".to_string() => Value::Object(im::hashmap! { ... }),  // sub‑dispatch
        "default".to_string() => Value::Object(im::hashmap! { ... }),
    }),
}));
```

---

## 7. Governance Change Specifications

### 7.1 New File: engine/dispatch_table.rs

Implements `DispatchTableBuilder` – see §4.1.

**Lines**: ~100 lines.

### 7.2 New File: engine/aliases.rs

Implements `register_core_aliases` – see §4.2.

**Lines**: ~80 lines.

### 7.3 Modify engine/equation.rs

**Change 1**: Delete `extract_cases_table()`, add `build_dispatch_table()`:

```rust
// Old method (delete):
fn extract_cases_table(&self) -> Option<Value> { ... }

// New method:
fn build_dispatch_table(&self) -> (Value, Value, String) {
    let mut builder = DispatchTableBuilder::new();
    builder.auto_from_registry(&self.registry);
    crate::engine::aliases::register_core_aliases(&mut builder);
    builder.set_default(Value::Object(im::hashmap! {
        "type".to_string() => Value::string("advance_instruction"),
        "params".to_string() => Value::empty_object(),
    }));
    builder.build()
}
```

**Change 1b**: Rewrite `validate_cases_integrity()`, delete `validate_cases_completeness()`:

```rust
// Old methods (delete):
fn extract_cases_table(&self) -> Option<Value> { ... }
fn validate_cases_completeness(&self) -> Result<(), EvoRuleError> { ... }

// New method: validate dynamically generated dispatch table
fn validate_cases_integrity(&self) -> Result<(), EvoRuleError> {
    let (cases, default, _) = self.build_dispatch_table();

    if let Value::Object(cases_map) = &cases {
        for (key, case_val) in cases_map.iter() {
            if let Some(target_type) = case_val.get("type").and_then(|v| v.as_str()) {
                if !self.registry.has(target_type) {
                    return Err(invalid_config(format!(
                        "case '{}' references unregistered instruction type '{}'",
                        key, target_type
                    )));
                }
            }
        }
    }

    // Validate that default's type is also registered
    if let Some(default_type) = default.get("type").and_then(|v| v.as_str()) {
        if !self.registry.has(default_type) {
            return Err(invalid_config(format!(
                "dispatch_default references unregistered instruction type '{}'",
                default_type
            )));
        }
    }

    Ok(())
}
```

**Design notes**:

- Keep `validate_cases_integrity`: it checks that hard‑coded primitive names in `register_core_aliases` are spelled correctly (e.g., `"state_compute"` vs `"state_computeX"`). Under the new architecture, Category‑A types are guaranteed registered (from registry), but Category‑B types come from hard‑coded strings and still need checking.
- Delete `validate_cases_completeness`: since `auto_from_registry()` automatically generates cases for all primitives except `dispatch`, and `dispatch` is covered by the Category‑D special case, completeness is automatically satisfied – this validation is no longer meaningful.
- Simplify `validate_strict()` to only call `validate_cases_integrity()`.

**Change 2**: In `evaluate()`, inject the new fields:

```rust
let (dispatch_cases, dispatch_default, dispatch_table_version) = self.build_dispatch_table();

let exec_ctx = Value::Object(im::hashmap! {
    // ... existing fields ...
    "dispatch_cases".to_string() => dispatch_cases,
    "dispatch_default".to_string() => dispatch_default,
    "dispatch_table_version".to_string() => Value::string(dispatch_table_version),
});
```

**Change 3**: In `execute_instruction()`, inject the same three fields (for consistency).

**Change 4**: In `new()`, fix the order of Layer‑1 business primitive registration (determinism guarantee):

```rust
// Existing code – order is fixed:
crate::planning::register_planning_primitives(&mut registry);
crate::io::register_io_primitives(&mut registry);
crate::formula::register_formula_primitives(&mut registry);
crate::formula::algebra::register_algebra_primitives(&mut registry);
```

**Lines**: ~30 lines of modifications.

### 7.4 Modify engine/core_eval.json

**Streamline to** (only execution skeleton – remove `cases` and `default`):

```json
{
  "rule_id": "core.eval",
  "name": "Core Eval Rule",
  "domain": { "type": "universal" },
  "transform": {
    "type": "while_loop",
    "params": {
      "condition": { "$ref": "__exec__.__running" },
      "max_steps": 10000,
      "body": [
        {
          "type": "state_set",
          "params": {
            "attr": "__matched_case__",
            "value": { "$ref": "__exec__.instruction.type" }
          }
        },
        {
          "type": "dispatch",
          "params": {
            "key": { "$ref": "__exec__.instruction.type" }
          }
        },
        {
          "type": "trace_step",
          "params": { "record_hash": true }
        }
      ]
    }
  },
  "priority": 0,
  "order": 0,
  "metadata": {},
  "constitutional": true,
  "enabled": true,
  "is_meta": true
}
```

**Lines**: reduced from ~660 lines to ~30 lines.

### 7.5 Modify engine/eval_config.json

**Remove the entire `primitives` section**, keep only environment configuration:

```json
{
  "_license": "CC0 1.0 Universal (Public Domain)",
  "_description": "EvoRule Governance Layer 1 configuration.",
  "termination": { ... },
  "result_path": "",
  "result_exclusions": ["__exec__", "__universe_rules__"],
  "default_instruction": { "type": "noop", "params": {} },
  "max_steps": 10000,
  "running_flag": "__running",
  "audit": { "record_hash": true, "trace_path": "__audit_trace" },
  "drain_strategy": { "meta_trace": false },
  "context_operations": { ... },
  "io_channels": { ... }
}
```

**Lines**: reduced from ~288 lines to ~50 lines.

### 7.6 Modify registry construction in new()

**Current**: `create_full_registry_from_config(config.primitives.as_ref())`  
**New**: `create_full_registry()` (direct hard‑coded registration, no JSON dependency)

```rust
// equation.rs:231
let mut registry = create_full_registry();  // change to hard‑coded registration
```

---

## 8. Implementation Order and Verification Points

### 8.1 Implementation Order (strictly sequential)

| Step | Change | Verification Point |
| ---- | ------ | ------------------- |
| 1 | Create `engine/dispatch_table.rs` | Unit tests: `DispatchTableBuilder` methods |
| 2 | Create `engine/aliases.rs` | Unit tests: `register_core_aliases` registers 6 aliases + 3 placeholders |
| 3 | Modify `engine/equation.rs`: `build_dispatch_table()` + injection | Unit tests: `__exec__` contains the three new fields |
| 4 | Modify TCB `dispatch.rs`: dual‑source read | TCB unit tests: main dispatch + sub‑dispatch |
| 5 | Modify TCB `audit_ops.rs`: change_summary construction | TCB unit tests: audit record format correct |
| 6 | Rewrite TCB `dispatch.rs` test fixtures | All TCB tests pass |
| 7 | Streamline `core_eval.json` (remove cases/default) | Governance tests: core.eval executes normally |
| 8 | Streamline `eval_config.json` (remove primitives) | Governance tests: registry construction correct |
| 9 | Modify `new()`: use `create_full_registry()` | Full test suite passes |
| 10 | Full regression test | All 179 tests + 8 gates pass |

### 8.2 Key Verification Scenarios

**Main dispatch** (system dispatch table):

- User instruction `increment` → hits alias increment case → executes state_compute
- User instruction `state_set` → hits direct‑mapped case → executes state_set
- User instruction `unknown` → misses → executes default (advance_instruction)

**Sub‑dispatch** (user‑provided cases):

- User constructs a `dispatch` instruction with cases → sub‑dispatch path → executes user cases
- Nested dispatch (default is dispatch) → each layer uses sub‑dispatch → no infinite recursion

**Abnormal input** (dispatch without cases):

- User constructs a `dispatch` instruction but does not pass cases → main dispatch hits the dispatch special case → resolve_refs resolves cases to Null → sub‑dispatch path → Null is not an Object → safe exit
- **No infinite recursion** (see walk‑through in §12.4)

**Registry fallback**:

- Unregistered primitive → looks up `__exec__.dispatch_cases` → hits → resolve_refs → executes
- FLOW‑03 check: dispatch is a registered primitive, so it uses the registered path – FLOW‑03 is not triggered (see §13.5)

**Audit chain**:

- Main dispatch: `change_summary` contains `[tbl:<hash>]`
- Sub‑dispatch: `change_summary` does **not** contain `[tbl:<hash>]`

---

## 9. ER‑605 Exception Registration

### 9.1 Exception #1: dispatch.rs Dual‑Source Read

**File**: `evorule-tcb/crates/tcb/src/control/dispatch.rs`  
**Function**: `dispatch_by_cases`  
**Change**: Cases source changed from `instruction.params.cases` to dual‑source (main reads `__exec__.dispatch_cases`, sub reads `instruction.params.cases`)  
**Rationale**: Enables the constitutional dispatch architecture, making `core_eval.json` never‑modified  
**Risk**: Low – dual‑source detection uses `contains_key("cases")`, semantics are clear  
**Rollback**: Revert to reading `instruction.params.get("cases")`

### 9.2 Exception #2: audit_ops.rs Append Version

**File**: `evorule-tcb/crates/tcb/src/primitive/audit_ops.rs`  
**Function**: `trace_step` (specifically the `change_summary` construction)  
**Change**: `change_summary` appends `[tbl:<hash>]` version information  
**Rationale**: Audit chain must carry dispatch table version to support traceability  
**Risk**: Low – `change_summary` is a string field, backwards‑compatible  
**Rollback**: Revert to the original `change_summary` construction logic

---

## 10. Key Risks and Mitigations

### 10.1 Risk: Ambiguity in Dual‑Source Detection

**Scenario**: User constructs a `dispatch` instruction but does not pass cases (misuse).

**Current behaviour**: `instruction.params.cases` absent → cases is empty_object → miss → fallback to default  
**New behaviour**: `instruction.params.cases` absent → **main dispatch** → look up key in `__exec__.dispatch_cases`

**Mitigation**: Use `contains_key("cases")` rather than `get("cases").is_some()` to distinguish "explicit empty cases" from "cases not provided":

- `params.cases` present (even as empty object) → sub‑dispatch
- `params.cases` completely absent → main dispatch

**Test coverage**: see §8.2 main‑dispatch and sub‑dispatch scenarios.

### 10.2 Risk: User passes a dispatch instruction without cases (abnormal usage)

**Scenario**: User `instruction.type = "dispatch"` but no `cases` provided (misuse).

**Behaviour** (safe under the new architecture):

1. Main dispatch looks up key="dispatch" → hits the dispatch special case (explicit $ref three fields)
2. resolve_refs resolves: cases → Null (path does not exist), default → Null
3. case_instr = `{type:"dispatch", params:{key, cases:Null, default:Null}}`
4. reg.execute → exec_dispatch → `contains_key("cases")`=true → sub‑dispatch
5. user_cases=Null → not an Object → no case lookup
6. default=Null → resolve_refs(Null)=Null → `GenericInstruction::from_value(Null)` errors or returns a default
7. **Safe exit – no infinite recursion**

**Mitigation**: This is user misuse; the system degrades gracefully (returns an error or advances the instruction rather than dead‑locking). Document that the `dispatch` instruction must pass `cases` for sub‑dispatch. See walk‑through in §12.4.

### 10.3 Risk: Alias Registration Order Affects Version Hash

**Scenario**: Layer‑2 modules register aliases in different orders, causing `dispatch_table_version` to differ.

**Mitigation**:

- Layer‑1 alias order is fixed (source‑code order in `register_core_aliases`)
- Layer‑2 module registration order is fixed by the call order inside `TheEquation::new()`
- Once started, the dispatch table does not change; version hash is stable within a single run

### 10.4 Risk: Audit Tests Asserting change_summary Format

**Scenario**: Existing tests assert `change_summary` is `"[increment] apply_rule:..."`; new format becomes `"[increment][tbl:abc] apply_rule:..."`.

**Mitigation**: Update test assertions when rewriting test fixtures in §8.1 step 6.

### 10.5 Risk: Audit Version Semantics for Sub‑Dispatch

**Scenario**: Sub‑dispatch does **not** append `[tbl:<hash>]` to `change_summary`, yet sub‑dispatch may still execute in the context of the system dispatch table.

**Mitigation**: This is intentional – sub‑dispatch executes user cases, which are independent of the system dispatch table; not appending the version avoids misleading traces. If system‑context version is needed for sub‑dispatch, it can be obtained from adjacent main‑dispatch records in the same audit chain.

---

## 11. Not Yet Implemented (Future Extensions)

### 11.1 Layer‑2 Alias Registration Extension Point

Currently `register_core_aliases` is hard‑coded. In the future, we can expose an `Equation::register_aliases()` hook to allow Layer‑2 modules to register business aliases:

```rust
// Future API (not yet implemented)
equation.register_aliases(|builder| {
    builder.register_alias("my_business_op", json_case("my_primitive", &[...]));
});
```

### 11.2 Dispatch Table Snapshots and Audit Replay

Currently `dispatch_table_version` is only a hash – it cannot reconstruct the full dispatch table. In the future, we can:

- Write a snapshot of the dispatch table into the first audit record at startup
- During audit replay, look up the corresponding snapshot by version hash

### 11.3 Dispatch Table Hot‑Update

Currently the dispatch table is built at startup and immutable at runtime. If hot‑updates (e.g., dynamic loading of Layer‑2 modules) are needed in the future:

- Rebuild the dispatch table + compute a new version
- Inject the new dispatch table + new version into `__exec__`
- Record the dispatch table change event in the audit chain

---

## 12. Architecture Decision Records

### 12.1 Why Choose Approach E (DispatchTableBuilder + Module Self‑Registration)?

**Other approaches considered**:

- **Approach A** (extend `param_schema_for` into a full case schema): moves cases knowledge to Rust constants – still centralised and grows
- **Approach B** (modify TCB to implement a purely constitutional architecture): the original 02.md proposal, but confused the two dispatch paths
- **Approach C** (extend registry metadata): moves cases knowledge to the registry, requiring 3 ER‑605 touches
- **Approach D** (dynamic injection + static retention): `core_eval.json` still needs changes – doesn't truly solve growth
- **Approach E** (DispatchTableBuilder + module self‑registration): cases knowledge distributed across modules – no centralised growth point

**Advantages of Approach E**:

1. `core_eval.json` and `eval_config.json` become truly immutable (streamlined to 30 + 50 lines)
2. Cases knowledge is distributed across business modules – no centralised growth point
3. Adding a new business alias only requires modifying the corresponding module – no centralised file changes
4. Only 2 ER‑605 touches (same as the original proposal)
5. Reuses the existing `register_*_primitives` pattern – good architectural consistency

### 12.2 Why Use `contains_key("cases")` to Distinguish Main vs Sub‑Dispatch?

**Alternatives considered**:

- Use `get("cases").is_some()`: cannot distinguish "explicit empty cases" from "cases not provided"
- Use `instruction_type == "dispatch"` to determine: but all main‑dispatch `dispatch` instructions have type "dispatch"
- Use `contains_key("cases")`: clearly distinguishes "explicitly passed cases" (sub‑dispatch) from "cases not passed" (main dispatch)

**Decision**: `contains_key("cases")` has the clearest semantics.

### 12.3 Why Store default Separately Rather Than Mixing It into dispatch_cases?

**Alternatives considered**:

- Put default as a special entry like `dispatch_cases["__default__"]`: could conflict with other case names
- Store default as an independent field `__exec__.dispatch_default`: clear semantics, no naming conflict

**Decision**: independent field.

### 12.4 Why Must Category D (dispatch case) Use Explicit $ref Rather Than $pass?

**Reason**:

- `dispatch` is a registered primitive – if the dispatch table lacks a case for `dispatch`, then when `instruction.type == "dispatch"`, main dispatch will fall through to default (`advance_instruction`), **completely breaking sub‑dispatch**
- Therefore a special case for `dispatch` must be in the dispatch table
- But it cannot use `$pass` passthrough: `$pass` performs wholesale replacement ([dispatch.rs:305](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L305)). If the user does not pass `cases`, `contains_key("cases")` is false → main dispatch → hits the dispatch case → passes through again → **infinite recursion**
- Must use explicit `$ref` three fields: `resolve_refs` recursively resolves each field of a multi‑field Object ([dispatch.rs:307-314](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/control/dispatch.rs#L307)). **Keys are always preserved** – even when a `$ref` path does not exist, it returns Null. Therefore the case body always contains the `"cases"` key → `contains_key("cases")` is true → sub‑dispatch path → Null is not an Object → safe exit

**Abnormal scenario walk‑through** (user does not pass cases):

1. while_loop pops `{type:"dispatch", params:{key:"dispatch"}}` (no cases)
2. core.eval dispatch → main dispatch → hits the dispatch special case
3. resolve_refs resolves explicit $ref: cases → Null, default → Null
4. case_instr = `{type:"dispatch", params:{key, cases:Null, default:Null}}`
5. reg.execute → exec_dispatch → `contains_key("cases")`=true → sub‑dispatch
6. user_cases=Null → not an Object → no case lookup
7. default=Null → `instruction.params.get("default")`=Some(Null) → resolve_refs(Null)=Null
8. `GenericInstruction::from_value(Null)` errors or returns a default → **safe exit, no infinite recursion**

**Verification**: see implementation of the dispatch special case in §4.2 and the verification scenarios in §8.2.

---

## 13. Relationship with Existing Standards

### 13.1 ER‑605 (TCB Freeze)

This architecture touches TCB twice (§9), both registered as exceptions. Beyond these, the TCB remains frozen.

### 13.2 ER‑601 (Determinism)

`DispatchTableBuilder` uses `BTreeMap` to store cases; `content_hash` internally sorts Object keys – determinism is guaranteed.

### 13.3 ER‑602 (No Panic)

This architecture does not introduce any new panic points – all `unwrap_or` usages have reasonable defaults.

### 13.4 GG‑31 (Business Logic Must Not Be Hard‑Coded in Rust)

The `register_core_aliases` in this architecture is **business alias registration** – not business logic implementation. Aliases merely declare that "user instruction X maps to primitive Y with field Z"; the actual business logic remains inside the primitive implementations. This complies with the spirit of GG‑31.

### 13.5 FLOW‑03 (Nested dispatch Prohibition)

This architecture does not change the FLOW‑03 check ([registry.rs:319-327](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/instruction/registry.rs#L319)). FLOW‑03 check is located in the registry's **fallback path**, and only applies to **unregistered** primitives. `dispatch` is a registered primitive and uses the **registered path** ([registry.rs:289-293](file:///d:/evorule-project/evorule-tcb/crates/tcb/src/instruction/registry.rs#L289)). Therefore the dispatch special case in the dispatch table will not be intercepted by FLOW‑03.

---

## 14. Acceptance Criteria

### 14.1 Functional Acceptance

- [ ] `core_eval.json` streamlined to ~30 lines – no `dispatch.params.cases` or `dispatch.params.default`
- [ ] `eval_config.json` streamlined to ~50 lines – no `primitives` section
- [ ] `__exec__` contains three new fields: `dispatch_cases`, `dispatch_default`, `dispatch_table_version`
- [ ] Main dispatch: user instructions `increment`/`state_set`/`unknown` behave correctly
- [ ] Sub‑dispatch: user‑constructed `dispatch` instruction with cases behaves correctly
- [ ] Nested dispatch (default is dispatch) behaves correctly
- [ ] Abnormal input: user‑constructed `dispatch` without cases → safe exit, no infinite recursion
- [ ] Registry fallback path behaviour unchanged
- [ ] Audit records: main dispatch includes `[tbl:<hash>]`; sub‑dispatch does not

### 14.2 Test Acceptance

- [ ] All TCB tests pass
- [ ] All governance tests pass
- [ ] All 179 tests + 8 gates pass

### 14.3 Architectural Acceptance

- [ ] `core_eval.json` will no longer be modified when adding new primitives or aliases
- [ ] `eval_config.json` will no longer be modified when adding new primitives
- [ ] Adding a new primitive only requires `register_*_primitives` – it automatically appears in the dispatch table (Category A)
- [ ] Adding a new business alias only requires `register_alias` in the corresponding module – no centralised file changes