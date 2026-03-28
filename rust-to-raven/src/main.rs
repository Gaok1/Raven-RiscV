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

const N: usize = 24;

// ── Shared state ──────────────────────────────────────────────────────────────
//
// After the top-level partition:
//   DATA[0 .. pivot_idx]        — owned exclusively by the worker hart
//   DATA[pivot_idx]             — pivot, already in final position (untouched)
//   DATA[pivot_idx+1 .. N]      — owned exclusively by the main hart
//
// No overlap → no data race on the array cells.
// WORKER_DONE is the only cross-hart synchronisation point.

static mut DATA: [i32; N] = [0; N];
static WORKER_DONE: AtomicBool = AtomicBool::new(false);

// 16-byte aligned so the computed stack-top is also 16-byte aligned.
#[repr(C, align(16))]
struct AlignedStack([u8; 8192]);
static mut WORKER_STACK: AlignedStack = AlignedStack([0; 8192]);

// ── Quicksort (recursive) ─────────────────────────────────────────────────────

fn partition(arr: &mut [i32]) -> usize {
    let last = arr.len() - 1;
    let pivot = arr[last];
    let mut store = 0;
    for j in 0..last {
        if arr[j] <= pivot {
            arr.swap(store, j);
            store += 1;
        }
    }
    arr.swap(store, last);
    store
}

fn quicksort(arr: &mut [i32]) {
    if arr.len() <= 1 {
        return;
    }
    let p = partition(arr);
    let (left, rest) = arr.split_at_mut(p);
    quicksort(left);
    quicksort(&mut rest[1..]); // rest[0] is the pivot — already in place
}

// ── Worker hart ───────────────────────────────────────────────────────────────
//
// Receives the left-partition length as the u32 arg placed in a0 by hart_start.
// Runs quicksort recursively on DATA[0..left_len], then signals completion.

fn worker_sort_left(left_len: u32) -> ! {
    // Safety: after the top-level partition main wrote DATA[0..left_len] before
    // spawning, then only touches DATA[pivot+1..N]. No overlap.
    let slice = unsafe {
        core::slice::from_raw_parts_mut(addr_of_mut!(DATA[0]), left_len as usize)
    };
    quicksort(slice);
    WORKER_DONE.store(true, Ordering::Release);
    hart_exit()
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let data = unsafe { core::slice::from_raw_parts_mut(addr_of_mut!(DATA[0]), N) };

    // Seed with random values in [-99, 100]
    for x in data.iter_mut() {
        *x = rand_range!(0u32, 200u32) as i32 - 99;
    }

    println!("=== Parallel Quicksort — {N} elements, 2 harts ===\n");
    print!("Input:  ");
    for x in data.iter() {
        print!("{x:5}");
    }
    println!();

    let t0 = get_instr_count();

    // ── Top-level partition ────────────────────────────────────────────────────
    //
    //   Before:  [ unsorted array ]
    //   After:   [ ≤ pivot | pivot | ≥ pivot ]
    //                 ↑                ↑
    //            worker hart        main hart  (both recurse independently)

    let pivot_idx = partition(data);
    let pivot_val = data[pivot_idx];

    println!(
        "Pivot = {pivot_val} at index {pivot_idx}  \
         (left {pivot_idx} elems → hart 1,  right {} elems → hart 0)",
        N - pivot_idx - 1,
    );

    // ── Fork ──────────────────────────────────────────────────────────────────
    WORKER_DONE.store(false, Ordering::Relaxed);

    let worker_stack = unsafe {
        core::slice::from_raw_parts_mut(addr_of_mut!(WORKER_STACK.0[0]), 8192)
    };
    spawn_hart_fn(worker_sort_left, worker_stack, pivot_idx as u32);

    // Main hart recursively sorts the right partition while the worker handles left.
    if pivot_idx + 1 < N {
        let right = unsafe {
            core::slice::from_raw_parts_mut(
                addr_of_mut!(DATA[pivot_idx + 1]),
                N - pivot_idx - 1,
            )
        };
        quicksort(right);
    }

    // ── Join ──────────────────────────────────────────────────────────────────
    while !WORKER_DONE.load(Ordering::Acquire) {
        // spin — wait for worker hart to finish its recursive sort
    }

    let elapsed = get_instr_count() - t0;

    // ── Verify ────────────────────────────────────────────────────────────────
    let data = unsafe { core::slice::from_raw_parts(addr_of!(DATA[0]), N) };

    print!("Sorted: ");
    for x in data {
        print!("{x:5}");
    }
    println!();

    let ok = data.windows(2).all(|w| w[0] <= w[1]);
    if ok {
        println!("\n✓ Correct!  fork→join (main-hart instructions): {elapsed}");
    } else {
        println!("\n✗ Sort failed!");
    }

    println!(
        "\nTip: step through the Run tab to watch both harts recurse\n\
         through their partitions simultaneously."
    );

    pause_sim();
    exit(0)
}
