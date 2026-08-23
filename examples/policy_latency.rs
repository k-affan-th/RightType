//! Dependency-free release-mode latency gate for the production boundary policy.

use std::hint::black_box;
use std::time::{Duration, Instant};

use righttype::dict;
use righttype::policy::{self, InputLayout};

const BATCHES: usize = 50;
const OPS_PER_BATCH: usize = 1_000;
const BUDGET: Duration = Duration::from_millis(1);

fn main() {
    let en = dict::english();
    let th = dict::thai();
    // Warm lazy dictionary initialization before measurement.
    black_box(policy::detect_at_boundary(
        "แนพพำแะ",
        InputLayout::ThaiKedmanee,
        en,
        th,
    ));

    let mut total = Duration::ZERO;
    let mut worst_average = Duration::ZERO;
    for batch in 0..BATCHES {
        let started = Instant::now();
        for index in 0..OPS_PER_BATCH {
            let (token, layout) = if (batch + index) % 2 == 0 {
                ("แนพพำแะ", InputLayout::ThaiKedmanee)
            } else {
                ("l;ylfu", InputLayout::UsQwerty)
            };
            black_box(policy::detect_at_boundary(token, layout, en, th));
        }
        let elapsed = started.elapsed();
        total += elapsed;
        worst_average = worst_average.max(elapsed / OPS_PER_BATCH as u32);
    }

    let operations = (BATCHES * OPS_PER_BATCH) as u32;
    let average = total / operations;
    println!(
        "production boundary policy: {operations} ops, average {average:?}, worst batch average {worst_average:?}"
    );
    assert!(
        worst_average < BUDGET,
        "boundary policy exceeded the {:?} per-token release budget",
        BUDGET
    );
}
