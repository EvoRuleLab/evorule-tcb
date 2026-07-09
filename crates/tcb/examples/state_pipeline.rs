// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project

//! Minimal state-pipeline example.
//!
//! Demonstrates the core EvoRule TCB workflow:
//!   1. Build an initial immutable `State`.
//!   2. Apply a sequence of `state_set` / `state_compute` instructions
//!      through the full primitive registry.
//!   3. Verify the pipeline is deterministic -- replaying it from the
//!      same initial state must produce an identical state JSON and
//!      an identical content hash.
//!
//! Run with:  `cargo run --example state_pipeline`

use std::collections::HashMap;

use evorule_tcb::{
    deterministic::content_hash,
    exec_ctl_ctx::ExecCtlCtx,
    instruction::registry::create_full_registry,
    rule::GenericInstruction,
    state::State,
    value::Value,
};

/// One step in the pipeline. `primitive` is the registered primitive name;
/// `attr` is the state attribute to touch; `value` is the integer payload.
struct Step {
    primitive: &'static str,
    attr: &'static str,
    value: i64,
}

impl Step {
    const fn set(attr: &'static str, value: i64) -> Self {
        Self { primitive: "state_set", attr, value }
    }
    const fn add(attr: &'static str, value: i64) -> Self {
        Self { primitive: "state_compute", attr, value }
    }
}

fn run(reg: &evorule_tcb::instruction::registry::InstructionRegistry, pipeline: &[Step]) -> State {
    let mut state = State::empty()
        .set("counter", Value::Integer(0))
        .set("step",    Value::Integer(0));
    let mut ctx = ExecCtlCtx::new();

    for s in pipeline {
        let mut params: HashMap<String, Value> = HashMap::new();
        params.insert("attr".to_string(), Value::string(s.attr));
        if s.primitive == "state_compute" {
            params.insert("operation".to_string(), Value::string("add"));
        }
        params.insert("value".to_string(), Value::Integer(s.value));
        let instr = GenericInstruction::new(s.primitive, params);

        state = reg.execute(&state, &instr, &mut ctx)
            .unwrap_or_else(|e| panic!("{} failed: {e}", s.primitive));
    }
    state
}

fn main() {
    let reg = create_full_registry();

    let pipeline: &[Step] = &[
        Step::add("counter", 10), // counter += 10
        Step::set("step", 1),     // step    = 1
        Step::add("counter", 5),  // counter += 5
    ];

    // Run twice from the same empty initial state.
    let state        = run(&reg, pipeline);
    let state_replay = run(&reg, pipeline);

    // Determinism contract: same inputs must yield byte-identical state.
    assert_eq!(
        state.to_json(),
        state_replay.to_json(),
        "determinism violated: replay diverged from first run"
    );

    // Content hash is the canonical fingerprint of the final state.
    let hash = content_hash(&[
        Value::string("state_pipeline"),
        state.business_state_snapshot(),
    ]);

    println!("final state    : {}", state.to_json());
    println!("content_hash   : {hash}");
    println!("OK determinism : replay matches, pipeline is content-stable");
}
