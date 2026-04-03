//! Atomic primitives for `rust-to-raven`.
//!
//! This module is the stable entry point for shared-state synchronization
//! between Raven harts. The implementation is backed directly by RV32A
//! instructions (`lr.w`, `sc.w`, and AMOs), so it tracks the simulator's
//! multi-hart atomic semantics instead of relying on a host-side atomic shim.
//!
//! Available wrappers:
//! - [`AtomicBool`]
//! - [`AtomicU32`]
//! - [`AtomicI32`]
//! - [`AtomicUsize`]
//! - [`Ordering`]
//!
//! Typical usage:
//! ```ignore
//! use crate::raven_api::atomic::{AtomicBool, AtomicU32, Ordering};
//!
//! static READY: AtomicBool = AtomicBool::new(false);
//! static COUNT: AtomicU32 = AtomicU32::new(0);
//!
//! COUNT.fetch_add(1, Ordering::AcqRel);
//! READY.store(true, Ordering::Release);
//! if READY.load(Ordering::Acquire) {
//!     let total = COUNT.load(Ordering::Acquire);
//! }
//! ```
//!
//! Notes:
//! - The wrappers are word-sized and aligned to Raven's RV32A support.
//! - Ordering is expressed through [`Ordering`] and lowered to fences plus
//!   atomic instructions in generated code.
//! - This layer is intended for code running inside Raven, not for host tools.
mod ordering;
mod types;

pub use ordering::Ordering;
pub use types::{Arc, AtomicBool, AtomicI32, AtomicU32, AtomicUsize};
