// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 EvoRule Project
//
// Hand-rolled benchmark harness for the core TCB primitives.
//
// Why hand-rolled (not criterion):
//   - criterion's transitive deps (rayon-core 1.13.0, half 2.7.1) require
//     rustc 1.80+/1.81+, but `rust-toolchain.toml` pins rustc 1.75.0 for
//     TCB-wide determinism. Pinning those transitive deps is whack-a-mole.
//   - Hand-rolled with `std::time::Instant` is dependency-free, compiles
//     fast, and gives us the wall-clock numbers we want.
//
// Why `benches/` is allowed to use `Instant::now()`:
//   - paradigm-gate G-02 ("no wall-clock time") exempts `benches/` via
//     the `get_rs_files` filter in `tools/paradigm-gate.sh`. The redline
//     applies to TCB production code (src/, src/bin/); benchmarks are
//     dev-only tooling that never enters the release artifact.
//
// Statistical methodology (matches criterion's defaults roughly):
//   - For each operation: warm up with N_WARMUP iterations, then measure
//     N_SAMPLES batches of BATCH_SIZE iterations each.
//   - Report min, mean, median, and stddev in nanoseconds per iteration.
//   - BATCH_SIZE is chosen so a single batch takes ~1-100 ms (avoids
//     timing noise on sub-microsecond ops, avoids drift on long ops).
//
// Migration note:
//   The previous `src/bin/tcb_benchmark.rs` used `LogicalClock` (a
//   deterministic u64 counter) to comply with G-02 in the TCB scope, but
//   that gave up all timing information -- every op reported
//   `1.0000 ticks/op`. This bench restores real ns/op measurements
//   without violating the redline, because it is not TCB scope.
//
// Run with:
//     cargo bench --bench primitives
//     cargo bench --bench primitives -- "state_set"
//     BENCH_QUICK=1 cargo bench --bench primitives   # reduced samples for CI

use evorule_tcb::deterministic::content_hash;
use evorule_tcb::domain::{Domain, RelOp};
use evorule_tcb::state::State;
use evorule_tcb::value::Value;
use std::collections::HashMap as StdHashMap;
use std::hint::black_box;
use std::time::Instant;

/// Default warmup iterations (give the CPU cache a chance to settle).
const WARMUP_DEFAULT: u32 = 100;
/// Default number of timed sample batches.
const SAMPLES_DEFAULT: u32 = 50;
/// Default iterations per sample batch.
const BATCH_SIZE_DEFAULT: u32 = 1000;

/// Aggregated statistics over `n` per-iteration timings.
#[derive(Debug, Clone, Copy)]
struct Stats {
    n: u32,
    min_ns: f64,
    mean_ns: f64,
    median_ns: f64,
    stddev_ns: f64,
}

impl Stats {
    fn from_samples(mut samples: Vec<f64>) -> Self {
        let n = samples.len() as u32;
        let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let sum: f64 = samples.iter().sum();
        let mean = sum / n as f64;
        let variance: f64 =
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        let stddev = variance.sqrt();
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if n.is_multiple_of(2) {
            (samples[n as usize / 2 - 1] + samples[n as usize / 2]) / 2.0
        } else {
            samples[n as usize / 2]
        };
        Stats {
            n,
            min_ns: min,
            mean_ns: mean,
            median_ns: median,
            stddev_ns: stddev,
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "min {:>10.2} ns | mean {:>10.2} ns | median {:>10.2} ns | stddev {:>9.2} ns | n={}",
            self.min_ns, self.mean_ns, self.median_ns, self.stddev_ns, self.n
        )
    }
}

/// Run `op` for `batch_size` iterations and record the per-batch ns/op.
/// Returns Stats over `n_samples` batches.
fn time_op<F: FnMut()>(label: &str, warmup: u32, batch_size: u32, n_samples: u32, mut op: F) -> Stats {
    // Warmup
    for _ in 0..warmup {
        op();
    }
    let mut samples = Vec::with_capacity(n_samples as usize);
    for _ in 0..n_samples {
        let start = Instant::now();
        for _ in 0..batch_size {
            op();
        }
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as f64 / batch_size as f64);
    }
    let stats = Stats::from_samples(samples);
    println!("    {label:<48} {stats}");
    stats
}

fn params() -> (u32, u32, u32) {
    if std::env::var("BENCH_QUICK").is_ok() {
        (10, 5, 100)
    } else {
        (WARMUP_DEFAULT, SAMPLES_DEFAULT, BATCH_SIZE_DEFAULT)
    }
}

// ============================================================
// Bench functions (10 groups, matching the original LogicalClock
// benchmark's coverage)
// ============================================================

fn bench_state_set() {
    println!("\n[state_set] State::set (persistent) vs underlying operations");
    let (warmup, samples, batch) = params();
    for &size in &[10_usize, 100, 1000] {
        let mut initial_data = StdHashMap::new();
        for i in 0..size {
            initial_data.insert(format!("key{}", i), Value::Integer(i as i64));
        }
        let state = State::from_std_map(initial_data);
        let label = format!("state/{size} keys, set new key");
        time_op(&label, warmup, batch, samples, || {
            let _ = black_box(&state).set("new_key", Value::string("value"));
        });
    }
}

fn bench_std_hashmap_set() {
    println!("\n[std_hashmap_set] std::collections::HashMap::insert (reference)");
    let (warmup, samples, batch) = params();
    for &size in &[10_usize, 100, 1000] {
        let map: StdHashMap<String, i64> = (0..size)
            .map(|i| (format!("key{}", i), i as i64))
            .collect();
        let label = format!("std_hashmap/{size} keys, insert new key");
        time_op(&label, warmup, batch, samples, || {
            let mut m = black_box(&map).clone();
            m.insert("new_key".to_string(), 99);
            black_box(m);
        });
    }
}

fn bench_state_set_path() {
    println!("\n[state_set_path] State::set_path (nested path insert)");
    let (warmup, samples, batch) = params();
    let state = State::empty();
    for &depth in &[1_usize, 3, 5, 10] {
        let path = "a.b.c.d.e.f.g.h.i.j"
            .split('.')
            .take(depth)
            .collect::<Vec<_>>()
            .join(".");
        let label = format!("set_path/depth={depth}");
        time_op(&label, warmup, batch, samples, || {
            let _ = black_box(&state).set_path(&path, Value::Integer(42)).unwrap();
        });
    }
}

fn bench_state_get_path() {
    println!("\n[state_get_path] State::get_path (nested path read)");
    let (warmup, samples, batch) = params();
    let mut state = State::empty();
    state = state
        .set_path("a.b.c.d.e.f.g.h.i.j", Value::Integer(42))
        .unwrap();
    for &depth in &[1_usize, 3, 5, 10] {
        let path = "a.b.c.d.e.f.g.h.i.j"
            .split('.')
            .take(depth)
            .collect::<Vec<_>>()
            .join(".");
        let label = format!("get_path/depth={depth}");
        time_op(&label, warmup, batch, samples, || {
            let _ = black_box(&state).get_path(&path);
        });
    }
}

fn bench_domain_evaluate() {
    println!("\n[domain_evaluate] Domain::contains (rule matching)");
    let (warmup, samples, batch) = params();
    let state = State::new(vec![
        ("age", Value::Integer(30)),
        ("score", Value::float(95.5)),
        ("name", Value::string("Alice")),
        ("active", Value::Bool(true)),
        (
            "tags",
            Value::list(vec![Value::string("admin"), Value::string("user")]),
        ),
    ]);
    let simple_domain = Domain::Atom {
        attribute: "age".to_string(),
        op: RelOp::Gt,
        value: Value::Integer(18),
    };
    let compound_domain = Domain::And(vec![
        Domain::Atom {
            attribute: "age".to_string(),
            op: RelOp::Between,
            value: Value::list(vec![Value::Integer(18), Value::Integer(65)]),
        },
        Domain::Atom {
            attribute: "active".to_string(),
            op: RelOp::Eq,
            value: Value::Bool(true),
        },
        Domain::Atom {
            attribute: "tags".to_string(),
            op: RelOp::Contains,
            value: Value::string("admin"),
        },
    ]);
    time_op("simple_age_gt_18", warmup, batch, samples, || {
        let _ = black_box(&simple_domain).contains(black_box(&state));
    });
    time_op("compound_and_3_atoms", warmup, batch, samples, || {
        let _ = black_box(&compound_domain).contains(black_box(&state));
    });
}

fn bench_value_serialization() {
    println!("\n[value_serialization] serde_json::to_string (JSON encode)");
    let (warmup, samples, batch) = params();
    let simple_value = Value::string("hello world");
    let complex_value = Value::object(
        std::iter::once(("name".to_string(), Value::string("Alice"))).collect(),
    );
    time_op("simple_string_to_json", warmup, batch, samples, || {
        let _ = serde_json::to_string(black_box(&simple_value));
    });
    time_op("complex_object_to_json", warmup, batch, samples, || {
        let _ = serde_json::to_string(black_box(&complex_value));
    });
}

fn bench_value_comparison() {
    println!("\n[value_comparison] Value::eq (equality)");
    let (warmup, samples, batch) = params();
    let a = Value::Integer(42);
    let b = Value::Integer(42);
    let c = Value::Integer(43);
    let d = Value::float(42.0);
    time_op("equal_integers", warmup, batch, samples, || {
        let _ = black_box(&a) == black_box(&b);
    });
    time_op("not_equal_integers", warmup, batch, samples, || {
        let _ = black_box(&a) == black_box(&c);
    });
    time_op("integer_vs_float", warmup, batch, samples, || {
        let _ = black_box(&a) == black_box(&d);
    });
}

fn bench_content_hash() {
    println!("\n[content_hash] content_hash (audit_chain hashing)");
    let (warmup, samples, batch) = params();
    let small_value = Value::string("test");
    let medium_value = Value::list((0..100).map(|i| Value::Integer(i as i64)).collect());
    time_op("small_string", warmup, batch, samples, || {
        let _ = content_hash(&[black_box(small_value.clone())]);
    });
    time_op("medium_list_100", warmup, batch, samples, || {
        let _ = content_hash(&[black_box(medium_value.clone())]);
    });
}

fn bench_state_clone() {
    println!("\n[state_clone] State::clone (persistent vs eager)");
    let (warmup, samples, batch) = params();
    for &size in &[10_usize, 100, 1000, 10_000] {
        let mut initial_data = StdHashMap::new();
        for i in 0..size {
            initial_data.insert(format!("key{}", i), Value::Integer(i as i64));
        }
        let state = State::from_std_map(initial_data);
        let label = format!("state/{size} keys, clone");
        time_op(&label, warmup, batch, samples, || {
            let _ = black_box(&state).clone();
        });
    }
}

fn bench_std_hashmap_clone() {
    println!("\n[std_hashmap_clone] std HashMap::clone (reference)");
    let (warmup, samples, batch) = params();
    for &size in &[10_usize, 100, 1000, 10_000] {
        let map: StdHashMap<String, i64> = (0..size)
            .map(|i| (format!("key{}", i), i as i64))
            .collect();
        let label = format!("std_hashmap/{size} keys, clone");
        time_op(&label, warmup, batch, samples, || {
            let _ = black_box(&map).clone();
        });
    }
}

fn main() {
    let started = Instant::now();
    println!("=== evorule-tcb primitive benchmarks (hand-rolled, wall-clock) ===");
    println!(
        "params: warmup={}, samples={}, batch_size={} (set BENCH_QUICK=1 for 10/5/100)",
        WARMUP_DEFAULT, SAMPLES_DEFAULT, BATCH_SIZE_DEFAULT
    );
    bench_state_set();
    bench_std_hashmap_set();
    bench_state_set_path();
    bench_state_get_path();
    bench_domain_evaluate();
    bench_value_serialization();
    bench_value_comparison();
    bench_content_hash();
    bench_state_clone();
    bench_std_hashmap_clone();
    println!(
        "\n=== done (total wall time: {:.2}s) ===",
        started.elapsed().as_secs_f64()
    );
}
