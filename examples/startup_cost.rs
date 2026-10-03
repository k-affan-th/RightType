//! What RightType's bundled data costs when it is first used: time and
//! heap, per table (2.3 P3). `cargo run --release --example startup_cost`.
//! The numbers go in docs/TYPING_BENCHMARK.md.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new as isize - l.size() as isize, Ordering::Relaxed);
        System.realloc(p, l, new)
    }
}

#[global_allocator]
static A: Counting = Counting;

fn measure(name: &str, f: impl FnOnce()) -> (f64, f64) {
    let before = LIVE.load(Ordering::Relaxed);
    let t = Instant::now();
    f();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let mb = (LIVE.load(Ordering::Relaxed) - before) as f64 / 1_048_576.0;
    println!("{name:<28} {ms:>8.1} ms {mb:>8.2} MB");
    (ms, mb)
}

fn main() {
    use righttype::*;
    println!("{:<28} {:>11} {:>11}", "first use of", "time", "heap");
    let mut total = (0.0, 0.0);
    let mut add = |r: (f64, f64)| {
        total.0 += r.0;
        total.1 += r.1;
    };
    add(measure("English dictionary", || {
        dict::english();
    }));
    add(measure("Thai dictionary", || {
        dict::thai();
    }));
    add(measure("English prefix index", || {
        dict::english().has_extension("midd");
    }));
    add(measure("first word decided", || {
        let _ = policy::detect_token(
            "l;ylfu8iy[",
            policy::InputLayout::UsQwerty,
            dict::english(),
            dict::thai(),
        );
    }));
    println!("{:<28} {:>8.1} ms {:>8.2} MB", "total", total.0, total.1);
}
