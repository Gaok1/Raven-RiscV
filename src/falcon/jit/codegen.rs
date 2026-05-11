//! dynasm-rs codegen — Phase B.
//!
//! Gated behind the `jit` cargo feature so Phase A binaries do not pull in
//! the dynasm dependency.

// TODO(phase-b): emit x86_64 / aarch64 from BasicBlock with faithful callouts
// to mem.fetch32 / mem.dcache_read{8,16,32} / mem.store{8,16,32} /
// CacheController::add_instruction_cycles.
