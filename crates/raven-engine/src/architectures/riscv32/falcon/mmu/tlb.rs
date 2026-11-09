// falcon/mmu/tlb.rs — unified TLB shared by I- and D-side translations.
//
// N-way set-associative. A 4 KiB page sits in set `vpn % sets`; a 4 MiB
// megapage covers 1024 such VPNs, so it is filed under `(vpn >> 10) % sets`
// instead — every address it maps then hashes to the same set. A lookup probes
// both sets, the way hardware probes its page-size arrays in parallel. The
// `global` bit lets an entry match across ASIDs.

use super::MEGAPAGE_BITS;
use crate::falcon::cache::ReplacementPolicy;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PtePerms {
    pub r: bool,
    pub w: bool,
    pub x: bool,
    pub u: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlbEntry {
    pub valid: bool,
    /// Virtual page number. A megapage keeps only its top VPN bits; the low
    /// [`MEGAPAGE_BITS`] are zero.
    pub vpn: u32,
    /// Physical page number, as the leaf PTE stores it.
    pub ppn: u32,
    pub asid: u16,
    pub perms: PtePerms,
    pub global: bool,
    pub dirty: bool,
    /// A 4 MiB megapage rather than a 4 KiB page.
    pub megapage: bool,
    /// LRU/FIFO age counter — bumped on access (LRU) or install (FIFO).
    pub age: u32,
    /// Reference bit for the Clock replacement policy. Set on every probe
    /// hit; cleared by the clock hand as it scans for a victim.
    pub ref_bit: bool,
}

impl TlbEntry {
    /// Low VPN bits this entry ignores on lookup.
    pub fn ignored_bits(&self) -> u32 {
        if self.megapage {
            u32::from(MEGAPAGE_BITS)
        } else {
            0
        }
    }

    fn covers(&self, vpn: u32, asid: u16) -> bool {
        self.valid
            && (self.global || self.asid == asid)
            && (self.vpn >> self.ignored_bits()) == (vpn >> self.ignored_bits())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlbConfig {
    /// Total entries (power of two, ≥ associativity).
    pub entry_count: u16,
    pub associativity: u8,
    pub replacement: ReplacementPolicy,
    pub hit_latency: u8,
    /// Cycles a page-table walk costs.
    pub miss_penalty: u8,
}

impl Default for TlbConfig {
    fn default() -> Self {
        Self {
            entry_count: 32,
            associativity: 4,
            replacement: ReplacementPolicy::Lru,
            hit_latency: 1,
            miss_penalty: 20,
        }
    }
}

#[derive(Default, Clone)]
pub struct TlbStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub page_faults: u64,
    /// Cycles every translation cost: hits plus walks.
    pub total_cycles: u64,
    /// The part of `total_cycles` spent walking page tables.
    pub walk_cycles: u64,
    /// Walks that found a leaf with A still clear — the first time the program
    /// touched that page since its mapping was written.
    pub first_touches: u64,
    /// Bytes of the pages counted in `first_touches`.
    pub first_touch_bytes: u64,
    /// Walks run only to set D: a store through a clean TLB entry.
    pub dirty_rewalks: u64,
    /// Walks that ended in a page fault.
    pub walk_faults: u64,
    /// (step, hit_rate_pct) — 300-cycle rolling window for the Stats UI.
    pub history: VecDeque<(f64, f64)>,
}

impl TlbStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64 * 100.0
        }
    }
}

/// Where [`Tlb::install`] put an entry, and what it pushed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Installed {
    pub slot: usize,
    pub evicted: Option<TlbEntry>,
}

#[derive(Clone)]
pub struct Tlb {
    pub config: TlbConfig,
    pub entries: Vec<TlbEntry>,
    pub stats: TlbStats,
    age_counter: u32,
    /// Per-set position of the Clock policy's "hand", as an offset within the
    /// set. Sized to `num_sets()`.
    clock_hands: Vec<usize>,
}

impl Tlb {
    pub fn new(config: TlbConfig) -> Self {
        let assoc = config.associativity.max(1) as usize;
        let raw = (config.entry_count.max(1) as usize).next_power_of_two();
        // Pad up so total entries is a multiple of associativity (≥ assoc).
        let n = raw.max(assoc);
        let n = n.div_ceil(assoc) * assoc;
        let num_sets = (n / assoc).max(1);
        Self {
            entries: vec![TlbEntry::default(); n],
            stats: TlbStats::default(),
            age_counter: 0,
            config,
            clock_hands: vec![0; num_sets],
        }
    }

    pub fn flush(&mut self) {
        for e in self.entries.iter_mut() {
            e.valid = false;
        }
    }

    /// Reconfigure the TLB. Resets entries and stats.
    pub fn reconfigure(&mut self, cfg: TlbConfig) {
        *self = Self::new(cfg);
    }

    pub fn associativity(&self) -> usize {
        self.config.associativity.max(1) as usize
    }

    pub fn num_sets(&self) -> usize {
        (self.entries.len() / self.associativity()).max(1)
    }

    fn set_range(&self, set_idx: usize) -> std::ops::Range<usize> {
        let start = set_idx * self.associativity();
        let end = (start + self.associativity()).min(self.entries.len());
        start..end
    }

    /// The set a page of this size is filed under.
    pub fn set_for(&self, vpn: u32, megapage: bool) -> usize {
        let shift = if megapage { MEGAPAGE_BITS } else { 0 };
        ((vpn >> shift) as usize) % self.num_sets()
    }

    /// The two sets a lookup of `vpn` probes: the 4 KiB set, then the 4 MiB one.
    pub fn probe_sets(&self, vpn: u32) -> [usize; 2] {
        [self.set_for(vpn, false), self.set_for(vpn, true)]
    }

    /// The slot mapping `vpn`/`asid`, if any — a lookup with no side effects.
    pub fn find(&self, vpn: u32, asid: u16) -> Option<usize> {
        let [small, mega] = self.probe_sets(vpn);
        self.set_range(small)
            .chain(if mega == small {
                0..0
            } else {
                self.set_range(mega)
            })
            .find(|&i| self.entries[i].covers(vpn, asid))
    }

    /// Look up `vpn`/`asid`, returning the slot and a copy of the entry. A hit
    /// bumps the entry's age (LRU/MRU/LFU) or sets its reference bit (Clock).
    pub fn probe(&mut self, vpn: u32, asid: u16) -> Option<(usize, TlbEntry)> {
        let slot = self.find(vpn, asid)?;
        self.age_counter = self.age_counter.wrapping_add(1);
        match self.config.replacement {
            ReplacementPolicy::Lru | ReplacementPolicy::Mru | ReplacementPolicy::Lfu => {
                self.entries[slot].age = self.age_counter;
            }
            ReplacementPolicy::Clock => self.entries[slot].ref_bit = true,
            _ => {}
        }
        Some((slot, self.entries[slot]))
    }

    /// Install `entry`. An entry already mapping the same page is overwritten in
    /// place, so a re-walk that sets D does not leave a stale twin behind;
    /// otherwise the replacement policy picks a victim.
    pub fn install(&mut self, mut entry: TlbEntry) -> Installed {
        let set_idx = self.set_for(entry.vpn, entry.megapage);
        let range = self.set_range(set_idx);
        let existing = range.clone().find(|&i| {
            let e = &self.entries[i];
            e.covers(entry.vpn, entry.asid) && e.megapage == entry.megapage
        });
        let (slot, evicted) = match existing {
            Some(i) => (i, None),
            None => {
                let v = self.find_victim(set_idx, range);
                let old = self.entries[v];
                if old.valid {
                    self.stats.evictions += 1;
                }
                (v, old.valid.then_some(old))
            }
        };
        self.age_counter = self.age_counter.wrapping_add(1);
        entry.valid = true;
        entry.age = self.age_counter;
        // A new entry gets one free pass before Clock can evict it.
        entry.ref_bit = matches!(self.config.replacement, ReplacementPolicy::Clock);
        self.entries[slot] = entry;
        Installed { slot, evicted }
    }

    fn find_victim(&mut self, set_idx: usize, range: std::ops::Range<usize>) -> usize {
        if let Some(i) = range.clone().find(|&i| !self.entries[i].valid) {
            return i;
        }
        match self.config.replacement {
            ReplacementPolicy::Lru | ReplacementPolicy::Fifo | ReplacementPolicy::Lfu => range
                .clone()
                .min_by_key(|&i| self.entries[i].age)
                .unwrap_or(range.start),
            ReplacementPolicy::Mru => range
                .clone()
                .max_by_key(|&i| self.entries[i].age)
                .unwrap_or(range.start),
            ReplacementPolicy::Random => {
                let n = range.end - range.start;
                range.start + (self.age_counter as usize % n.max(1))
            }
            ReplacementPolicy::Clock => {
                // Second chance: walk from the hand, clearing reference bits,
                // and take the first entry whose bit is already clear. Two laps
                // always find one.
                let n = (range.end - range.start).max(1);
                if set_idx >= self.clock_hands.len() {
                    self.clock_hands.resize(set_idx + 1, 0);
                }
                let mut hand = self.clock_hands[set_idx] % n;
                for _ in 0..(2 * n) {
                    let idx = range.start + hand;
                    if !self.entries[idx].ref_bit {
                        self.clock_hands[set_idx] = (hand + 1) % n;
                        return idx;
                    }
                    self.entries[idx].ref_bit = false;
                    hand = (hand + 1) % n;
                }
                let idx = range.start + hand;
                self.clock_hands[set_idx] = (hand + 1) % n;
                idx
            }
        }
    }

    /// Invalidate every entry mapping `vaddr`, whatever its page size (for
    /// `sfence.vma` with a non-zero rs1).
    pub fn flush_vaddr(&mut self, vaddr: u32) {
        let vpn = vaddr >> super::PAGE_BITS;
        for set in self.probe_sets(vpn) {
            for i in self.set_range(set) {
                let e = &mut self.entries[i];
                if e.valid && (e.vpn >> e.ignored_bits()) == (vpn >> e.ignored_bits()) {
                    e.valid = false;
                }
            }
        }
    }

    /// 1 for the most recently used valid entry of its set, 2 for the next, …
    /// — by install order under FIFO, by last use under the other policies.
    pub fn recency(&self, slot: usize) -> Option<usize> {
        let entry = self.entries.get(slot).filter(|e| e.valid)?;
        let set = slot / self.associativity();
        let newer = self
            .set_range(set)
            .filter(|&i| i != slot && self.entries[i].valid && self.entries[i].age > entry.age)
            .count();
        Some(newer + 1)
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/falcon_mmu_tlb.rs"]
mod tests;
