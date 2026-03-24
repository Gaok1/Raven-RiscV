use crate::raven_api::syscall::getrandom;

// ── Raw random helpers ────────────────────────────────────────────────────────

/// Return a uniformly random `u32` via `getrandom`.
pub fn rand_u32() -> u32 {
    let mut v = 0u32;
    unsafe { getrandom(&mut v as *mut u32 as *mut u8, 4, 0) };
    v
}

/// Return a uniformly random byte (0–255).
pub fn rand_u8() -> u8 {
    let mut v = 0u8;
    unsafe { getrandom(&mut v as *mut u8, 1, 0) };
    v
}

/// Return a random `i32` (full signed range).
pub fn rand_i32() -> i32 {
    rand_u32() as i32
}

/// Return a random `u32` in `[lo, hi)`. Returns `lo` if `hi <= lo`.
///
/// Uses modulo reduction — good for teaching, not for cryptographic use.
pub fn rand_range(lo: u32, hi: u32) -> u32 {
    if hi <= lo {
        return lo;
    }
    lo + rand_u32() % (hi - lo)
}

/// Return `true` or `false` with equal probability.
pub fn rand_bool() -> bool {
    rand_u8() & 1 != 0
}

// ── Macros ────────────────────────────────────────────────────────────────────

/// Random `u32` from getrandom.
#[macro_export]
macro_rules! rand_u32 {
    () => {
        $crate::raven_api::random::rand_u32()
    };
}

/// Random byte (0–255).
#[macro_export]
macro_rules! rand_u8 {
    () => {
        $crate::raven_api::random::rand_u8()
    };
}

/// Random `i32` (full signed range).
#[macro_export]
macro_rules! rand_i32 {
    () => {
        $crate::raven_api::random::rand_i32()
    };
}

/// Random `u32` in `[lo, hi)`.
#[macro_export]
macro_rules! rand_range {
    ($lo:expr, $hi:expr) => {
        $crate::raven_api::random::rand_range($lo, $hi)
    };
}

/// Random `bool`.
#[macro_export]
macro_rules! rand_bool {
    () => {
        $crate::raven_api::random::rand_bool()
    };
}
