use std::collections::HashMap;

/// Counts control-flow targets — the PC after a taken branch or jump.
/// Future Hot-JIT policies query this to pick blocks to compile.
pub struct HotProfile {
    counts: HashMap<u32, u32>,
}

impl HotProfile {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    #[inline]
    pub fn record_target(&mut self, pc: u32) {
        let slot = self.counts.entry(pc).or_insert(0);
        *slot = slot.saturating_add(1);
    }

    pub fn get(&self, pc: u32) -> u32 {
        self.counts.get(&pc).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &u32)> {
        self.counts.iter()
    }

    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn clear(&mut self) {
        self.counts.clear();
    }
}

impl Default for HotProfile {
    fn default() -> Self {
        Self::new()
    }
}
