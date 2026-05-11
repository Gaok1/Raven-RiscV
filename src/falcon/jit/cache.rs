//! Compiled-block cache stub for the future JIT.
//!
//! Phase A: a no-op shell so driver-side invalidation calls
//! (`backend.invalidate(start, end)`) compile today without later having to
//! change call sites when Phase B adds real storage.

pub struct CompiledBlockCache {
    _private: (),
}

impl CompiledBlockCache {
    pub fn new() -> Self {
        Self { _private: () }
    }

    // TODO(jit-smc): Phase B must invalidate all compiled blocks whose
    // [start_pc, end_pc) intersects [start, end).
    pub fn invalidate_range(&mut self, _start: u32, _end: u32) {}
}

impl Default for CompiledBlockCache {
    fn default() -> Self {
        Self::new()
    }
}
