// falcon/cache/config.rs — CacheConfig struct and address decomposition helpers

use super::{InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy};

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub size: usize,          // total bytes
    pub line_size: usize,     // bytes per line
    pub associativity: usize, // ways per set
    pub replacement: ReplacementPolicy,
    pub write_policy: WritePolicy,
    pub write_alloc: WriteAllocPolicy,
    /// Inclusion/exclusion policy with respect to the next cache level.
    /// Ignored for the last (lowest) cache level.
    pub inclusion: InclusionPolicy,
    /// Cycles consumed on a cache hit
    pub hit_latency: u64,
    /// Extra cycles added on a cache miss (stall waiting for RAM)
    pub miss_penalty: u64,
    /// Extra cycles per additional way beyond the first during tag search
    pub assoc_penalty: u64,
    /// Bus width in bytes; transfer cost = ceil(line_size / transfer_width)
    pub transfer_width: u32,
}

impl CacheConfig {
    pub fn num_sets(&self) -> usize {
        self.size / (self.line_size * self.associativity)
    }
    pub fn offset_bits(&self) -> u32 {
        self.line_size.trailing_zeros()
    }
    pub fn index_bits(&self) -> u32 {
        self.num_sets().trailing_zeros()
    }
    pub fn addr_tag(&self, addr: u32) -> u32 {
        addr >> (self.offset_bits() + self.index_bits())
    }
    pub fn addr_index(&self, addr: u32) -> usize {
        let mask = (self.num_sets() as u32).saturating_sub(1);
        ((addr >> self.offset_bits()) & mask) as usize
    }
    pub fn addr_offset(&self, addr: u32) -> usize {
        (addr & (self.line_size as u32 - 1)) as usize
    }
    pub fn line_base(&self, addr: u32) -> u32 {
        addr & !(self.line_size as u32 - 1)
    }
    /// Returns Ok(()) if the config is usable, or an Err with a human-readable reason.
    pub fn validate(&self) -> Result<(), String> {
        if self.size == 0 {
            return Err("Size must be > 0".to_string());
        }
        if self.line_size < 4 {
            return Err(format!("Line size must be ≥ 4 B (got {})", self.line_size));
        }
        if self.associativity == 0 {
            return Err("Associativity must be ≥ 1".to_string());
        }
        if !self.line_size.is_power_of_two() {
            return Err(format!(
                "Line size {} is not a power of 2 (try {})",
                self.line_size,
                self.line_size.next_power_of_two()
            ));
        }
        let bytes_per_set = match self.line_size.checked_mul(self.associativity) {
            Some(v) => v,
            None => return Err("assoc × line_size overflows usize".to_string()),
        };
        if bytes_per_set > self.size {
            return Err(format!(
                "assoc({}) × line({}) = {} B > size({} B): need ≥ 1 set",
                self.associativity, self.line_size, bytes_per_set, self.size
            ));
        }
        if self.size % bytes_per_set != 0 {
            return Err(format!(
                "size({}) not divisible by assoc({}) × line({})",
                self.size, self.associativity, self.line_size
            ));
        }
        let sets = self.size / bytes_per_set;
        if !sets.is_power_of_two() {
            let next = sets.next_power_of_two();
            return Err(format!(
                "{sets} sets is not a power of 2 (try size={})",
                bytes_per_set * next
            ));
        }
        if !matches!(self.inclusion, InclusionPolicy::NonInclusive) {
            return Err(
                "Inclusive and Exclusive policies are not yet implemented; use NonInclusive"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn is_valid_config(&self) -> bool {
        self.validate().is_ok()
    }

    /// Cycles consumed for a tag lookup: hit_latency + (ways-1) * assoc_penalty
    pub fn tag_search_cycles(&self) -> u64 {
        self.hit_latency + (self.associativity as u64).saturating_sub(1) * self.assoc_penalty
    }

    /// Cycles to transfer one cache line over the bus: ceil(line_size / transfer_width)
    pub fn line_transfer_cycles(&self) -> u64 {
        let w = self.transfer_width.max(1) as u64;
        ((self.line_size as u64) + w - 1) / w
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            size: 1024,
            line_size: 16,
            associativity: 2,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 1,
            miss_penalty: 50,
            assoc_penalty: 1,
            transfer_width: 8,
        }
    }
}
