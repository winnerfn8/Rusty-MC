//! What a mixin hook point costs: the call-site pattern of `tick_server` (a start that may
//! cancel, an end with the return value) around a small body, with nothing attached, then with
//! a Rust handler that does nothing attached to both (the site, what the method is handed, is made
//! only then). `cargo run --release -p horizon-java
//! --example hook_cost` (docs/java-mod-runtime.md has the numbers).
use horizon_java::hooks::{self, Call, Hook, NoHost, Obj, Site};
use std::hint::black_box;
use std::time::Instant;

const N: u64 = 200_000_000;

/// A stand-in for a ported method's own work: a few dependent operations.
#[inline(never)]
fn body(x: u64) -> f32 {
    (black_box(x).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32
}

#[inline(never)]
fn plain(x: u64) -> f32 {
    body(x)
}

#[inline(never)]
fn hooked(x: u64) -> f32 {
    let mut host = NoHost;
    let site = |_: &NoHost| {
        Site::new(
            Obj::Block(1),
            vec![Obj::BlockState(1), Obj::BlockPos([0, 64, 0])],
        )
    };
    let mut head = Call::head();
    hooks::run(Hook::GetDestroyProgressHead, &mut head, &mut host, site);
    if head.is_canceled() {
        return head.into_value().unwrap_or_default();
    }
    let v = body(x);
    let mut end = Call::returning(v);
    hooks::run(Hook::GetDestroyProgressReturn, &mut end, &mut host, site);
    end.into_value().unwrap_or(v)
}

fn time(name: &str, f: fn(u64) -> f32) -> f64 {
    let mut sum = 0.0f32;
    for i in 0..N / 20 {
        sum += f(i);
    }
    let start = Instant::now();
    for i in 0..N {
        sum += f(i);
    }
    let ns = start.elapsed().as_secs_f64() * 1e9 / N as f64;
    black_box(sum);
    println!("{name:<44} {ns:>7.3} ns per call");
    ns
}

fn main() {
    let base = time("method alone", plain);
    let idle = time("method with idle hook points (start + end)", hooked);
    println!("idle hook points: {:+.3} ns per call", idle - base);
    hooks::install(Box::new(|_, _, _, _, _| {}));
    hooks::attach(Hook::GetDestroyProgressHead);
    hooks::attach(Hook::GetDestroyProgressReturn);
    let busy = time("with a no-op Rust handler on both", hooked);
    println!("attached dispatch: {:+.3} ns per call", busy - base);
}
