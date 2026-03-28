#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod raven_api;

use core::ptr::{addr_of, addr_of_mut};
use portable_atomic::{AtomicBool, Ordering};
use raven_api::hart::spawn_hart_fn;
use raven_api::syscall::{exit, get_instr_count, hart_exit, pause_sim};

// ── Config ────────────────────────────────────────────────────────────────────

const N: usize = 32;
const HALF: usize = N / 2;

// ── Shared state ──────────────────────────────────────────────────────────────
//
// DATA[0..HALF]  — sorted by the worker hart in the parallel phase.
// DATA[HALF..N]  — sorted by main hart in the parallel phase.
// No overlap → no race on the data arrays.
// WORKER_DONE uses AtomicBool (portable-atomic) for the release/acquire fence.

static mut DATA: [i32; N] = [0; N];
static WORKER_DONE: AtomicBool = AtomicBool::new(false);

// 16-byte aligned so that WORKER_STACK.end is also 16-byte aligned (4096 % 16 == 0),
// satisfying Raven's stack-pointer alignment requirement.
#[repr(C, align(16))]
struct AlignedStack([u8; 4096]);
static mut WORKER_STACK: AlignedStack = AlignedStack([0; 4096]);

// ── Raw-pointer helpers (Rust 2024 forbids &[mut] STATIC_MUT) ─────────────────

#[inline(always)]
unsafe fn data_slice(offset: usize, len: usize) -> &'static [i32] {
    unsafe { core::slice::from_raw_parts(addr_of!(DATA[offset]), len) }
}

#[inline(always)]
unsafe fn data_slice_mut(offset: usize, len: usize) -> &'static mut [i32] {
    unsafe { core::slice::from_raw_parts_mut(addr_of_mut!(DATA[offset]), len) }
}

// ── Insertion sort ────────────────────────────────────────────────────────────

fn insertion_sort(a: &mut [i32]) {
    for i in 1..a.len() {
        let key = a[i];
        let mut j = i;
        while j > 0 && a[j - 1] > key {
            a[j] = a[j - 1];
            j -= 1;
        }
        a[j] = key;
    }
}

// ── Merge sorted DATA[0..HALF] ++ DATA[HALF..N] ───────────────────────────────

fn merge(left: &[i32], right: &[i32]) -> alloc::vec::Vec<i32> {
    let mut out = alloc::vec::Vec::with_capacity(N);
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] { out.push(left[i]); i += 1; }
        else                    { out.push(right[j]); j += 1; }
    }
    out.extend_from_slice(&left[i..]);
    out.extend_from_slice(&right[j..]);
    out
}

fn worker_sort_first_half(_: u32) -> ! {
    insertion_sort(unsafe { data_slice_mut(0, HALF) });
    WORKER_DONE.store(true, Ordering::Release);
    hart_exit()
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Fill DATA with random values in [-99, 100]
    for x in unsafe { data_slice_mut(0, N) } {
        *x = rand_range!(0u32, 200u32) as i32 - 99;
    }

    println!("=== Parallel Merge Sort — {N} elements, 2 harts ===\n");
    print!("Input:  ");
    for x in unsafe { data_slice(0, N) } { print!("{x:5}"); }
    println!();

    let t0 = get_instr_count();

    // ── Fork ─────────────────────────────────────────────────────────────────
    WORKER_DONE.store(false, Ordering::Relaxed);

    let worker_stack = unsafe {
        core::slice::from_raw_parts_mut(addr_of_mut!(WORKER_STACK.0[0]), 4096)
    };
    spawn_hart_fn(worker_sort_first_half, worker_stack, 0);

    // Main hart sorts DATA[HALF..N] in parallel.
    insertion_sort(unsafe { data_slice_mut(HALF, HALF) });

    // ── Join ─────────────────────────────────────────────────────────────────
    // Acquire: guarantees we see the worker's stores to DATA[0..HALF].
    while !WORKER_DONE.load(Ordering::Acquire) { /* spin */ }

    let sorted = merge(
        unsafe { data_slice(0, HALF) },
        unsafe { data_slice(HALF, HALF) },
    );

    let elapsed = get_instr_count() - t0;

    print!("Sorted: ");
    for x in &sorted { print!("{x:5}"); }
    println!();

    if sorted.windows(2).all(|w| w[0] <= w[1]) {
        println!("\n✓ Correct!  Main-hart instructions (fork → join → merge): {elapsed}");
    } else {
        println!("\n✗ Sort failed!");
    }

    println!("\nTip: open the Pipeline tab and step through to watch both harts\n\
              sort their halves independently, then observe the merge.");

    pause_sim();
    exit(0)
}
