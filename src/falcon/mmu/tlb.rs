// falcon/mmu/tlb.rs — unified TLB shared by I- and D-side translations.
//
// N-way set-associative. Index by `(vpn % num_sets)`; linear search the `A`
// entries in the set. Megapages match only the high 10 bits of the VPN; the
// `global` bit lets entries match across ASIDs.

#![allow(dead_code)]

use crate::falcon::cache::ReplacementPolicy;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PtePerms {
    pub r: bool,
    pub w: bool,
    pub x: bool,
    pub u: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TlbEntry {
    pub valid: bool,
    pub vpn: u32,
    pub ppn: u32,
    pub asid: u16,
    pub perms: PtePerms,
    pub global: bool,
    pub accessed: bool,
    pub dirty: bool,
    /// L1 leaf — masks vpn[0] (10 LSB) during lookup so a single entry covers
    /// the whole 4 MiB superpage.
    pub megapage: bool,
    /// LRU/FIFO age counter — bumped on access (LRU) or install (FIFO).
    pub age: u32,
    /// Reference bit for the Clock replacement policy. Set on every probe
    /// hit; cleared by the clock hand as it scans for a victim.
    pub ref_bit: bool,
}

#[derive(Clone, Debug)]
pub struct TlbConfig {
    /// Total entries (power of two, ≥ associativity).
    pub entry_count: u16,
    pub associativity: u8,
    pub replacement: ReplacementPolicy,
    pub hit_latency: u8,
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

#[derive(Default)]
pub struct TlbStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub page_faults: u64,
    pub total_cycles: u64,
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

pub struct Tlb {
    pub config: TlbConfig,
    pub entries: Vec<TlbEntry>,
    pub stats: TlbStats,
    age_counter: u32,
    /// Per-set position of the Clock policy's "hand". Indexed by set index.
    /// Each entry is `0..associativity` (offset within the set). Resized on
    /// `reconfigure` so it always matches `num_sets()`.
    clock_hands: Vec<usize>,
}

impl Tlb {
    pub fn new(config: TlbConfig) -> Self {
        let assoc = config.associativity.max(1) as usize;
        let raw = (config.entry_count.max(1) as usize).next_power_of_two();
        // Pad up so total entries is a multiple of associativity (≥ assoc).
        let n = raw.max(assoc);
        let n = ((n + assoc - 1) / assoc) * assoc;
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

    /// Reconfigure the TLB. Resets entries; also resets stats.
    pub fn reconfigure(&mut self, cfg: TlbConfig) {
        *self = Self::new(cfg);
    }

    pub fn num_sets(&self) -> usize {
        let assoc = self.config.associativity.max(1) as usize;
        (self.entries.len() / assoc).max(1)
    }

    fn set_range(&self, set_idx: usize) -> std::ops::Range<usize> {
        let assoc = self.config.associativity.max(1) as usize;
        let start = set_idx * assoc;
        let end = (start + assoc).min(self.entries.len());
        start..end
    }

    fn vpn_set(&self, vpn: u32) -> usize {
        // Standard 4 KiB indexing: hash the full VPN so 1024 consecutive pages
        // distribute evenly across sets. Megapages are probed/installed via
        // [`megapage_vpn_set`] so a single megapage shares a set with the 1 K
        // 4 KiB pages it covers in the *megapage* direction (vpn>>10).
        let n = self.num_sets();
        (vpn as usize) % n
    }

    fn megapage_vpn_set(&self, vpn: u32) -> usize {
        let n = self.num_sets();
        ((vpn >> 10) as usize) % n
    }

    fn matches(entry: &TlbEntry, vpn: u32, asid: u16) -> bool {
        if !entry.valid {
            return false;
        }
        if !entry.global && entry.asid != asid {
            return false;
        }
        if entry.megapage {
            (entry.vpn >> 10) == (vpn >> 10)
        } else {
            entry.vpn == vpn
        }
    }

    /// Look up an entry for `vpn`/`asid`. Bumps age on LRU/MRU; returns a copy
    /// on hit so callers don't fight the borrow checker against `self.stats`.
    ///
    /// Two sets are probed: the 4 KiB-indexed set (`vpn_set`) and the megapage
    /// set (`megapage_vpn_set`). Real hardware splits these into two arrays;
    /// we share storage but probe both indices so a 4 KiB hit isn't aliased
    /// against megapages and vice-versa.
    pub fn probe(&mut self, vpn: u32, asid: u16) -> Option<TlbEntry> {
        let s4k = self.vpn_set(vpn);
        if let Some(hit) = self.probe_in_set(s4k, vpn, asid) {
            return Some(hit);
        }
        let smega = self.megapage_vpn_set(vpn);
        if smega != s4k {
            if let Some(hit) = self.probe_in_set(smega, vpn, asid) {
                return Some(hit);
            }
        }
        None
    }

    fn probe_in_set(&mut self, set_idx: usize, vpn: u32, asid: u16) -> Option<TlbEntry> {
        let range = self.set_range(set_idx);
        for i in range {
            if Self::matches(&self.entries[i], vpn, asid) {
                self.age_counter = self.age_counter.wrapping_add(1);
                match self.config.replacement {
                    ReplacementPolicy::Lru
                    | ReplacementPolicy::Mru
                    | ReplacementPolicy::Lfu => {
                        self.entries[i].age = self.age_counter;
                    }
                    ReplacementPolicy::Clock => {
                        // Clock: a hit just sets the reference bit. Eviction
                        // scan will clear it later.
                        self.entries[i].ref_bit = true;
                    }
                    _ => {}
                }
                return Some(self.entries[i]);
            }
        }
        None
    }

    /// Install `entry`. If an existing entry already maps this VPN+ASID, that
    /// slot is reused (so a re-walk that updates A/D doesn't leave a stale
    /// duplicate behind). Otherwise picks a victim per replacement policy and
    /// counts the eviction.
    pub fn install(&mut self, mut entry: TlbEntry) {
        let set_idx = if entry.megapage {
            self.megapage_vpn_set(entry.vpn)
        } else {
            self.vpn_set(entry.vpn)
        };
        let range = self.set_range(set_idx);
        let existing = range
            .clone()
            .find(|&i| Self::matches(&self.entries[i], entry.vpn, entry.asid));
        let target = match existing {
            Some(i) => i,
            None => {
                let v = self.find_victim(set_idx, range);
                if self.entries[v].valid {
                    self.stats.evictions += 1;
                }
                v
            }
        };
        self.age_counter = self.age_counter.wrapping_add(1);
        entry.valid = true;
        entry.age = self.age_counter;
        // Newly installed entries start with the ref bit set (one free pass
        // before Clock can evict them).
        if matches!(self.config.replacement, ReplacementPolicy::Clock) {
            entry.ref_bit = true;
        }
        self.entries[target] = entry;
    }

    fn find_victim(&mut self, set_idx: usize, range: std::ops::Range<usize>) -> usize {
        // Prefer invalid slots.
        for i in range.clone() {
            if !self.entries[i].valid {
                return i;
            }
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
                // Second-chance algorithm: walk from the clock hand, skipping
                // entries with ref_bit=true (clearing it as we go). The first
                // entry with ref_bit=false is the victim; the hand resumes
                // just past it next time.
                let n = (range.end - range.start).max(1);
                if set_idx >= self.clock_hands.len() {
                    self.clock_hands.resize(set_idx + 1, 0);
                }
                let mut hand = self.clock_hands[set_idx] % n;
                // At most 2*n steps: one to clear all ref_bits, one to find
                // a victim. Guaranteed to terminate.
                for _ in 0..(2 * n) {
                    let idx = range.start + hand;
                    if !self.entries[idx].ref_bit {
                        // Advance hand past the victim for the next call.
                        self.clock_hands[set_idx] = (hand + 1) % n;
                        return idx;
                    }
                    self.entries[idx].ref_bit = false;
                    hand = (hand + 1) % n;
                }
                // Fallback (should be unreachable): take the slot under the
                // hand and bump.
                let idx = range.start + hand;
                self.clock_hands[set_idx] = (hand + 1) % n;
                idx
            }
        }
    }

    /// Invalidate every entry whose VPN matches `vaddr` (for `sfence.vma`
    /// with a non-zero rs1). Checks both the 4 KiB set and the megapage set,
    /// since a hardware sfence.vma must invalidate either representation.
    pub fn flush_vaddr(&mut self, vaddr: u32) {
        let vpn = vaddr >> 12;
        let s4k = self.vpn_set(vpn);
        for i in self.set_range(s4k) {
            if Self::matches(&self.entries[i], vpn, self.entries[i].asid) {
                self.entries[i].valid = false;
            }
        }
        let smega = self.megapage_vpn_set(vpn);
        if smega != s4k {
            for i in self.set_range(smega) {
                if Self::matches(&self.entries[i], vpn, self.entries[i].asid) {
                    self.entries[i].valid = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(entries: u16, assoc: u8, policy: ReplacementPolicy) -> TlbConfig {
        TlbConfig {
            entry_count: entries,
            associativity: assoc,
            replacement: policy,
            hit_latency: 1,
            miss_penalty: 20,
        }
    }

    fn mk_entry(vpn: u32, ppn: u32, asid: u16) -> TlbEntry {
        TlbEntry {
            valid: true,
            vpn,
            ppn,
            asid,
            perms: PtePerms {
                r: true,
                w: true,
                x: true,
                u: true,
            },
            global: false,
            accessed: true,
            dirty: false,
            megapage: false,
            age: 0,
            ref_bit: false,
        }
    }

    #[test]
    fn install_then_probe_hits() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        tlb.install(mk_entry(0x10, 0x100, 1));
        let e = tlb.probe(0x10, 1).expect("hit");
        assert_eq!(e.ppn, 0x100);
    }

    #[test]
    fn probe_miss_on_wrong_asid() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        tlb.install(mk_entry(0x10, 0x100, 1));
        assert!(tlb.probe(0x10, 2).is_none());
    }

    #[test]
    fn global_entry_matches_any_asid() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        let mut e = mk_entry(0x10, 0x100, 1);
        e.global = true;
        tlb.install(e);
        assert!(tlb.probe(0x10, 2).is_some());
        assert!(tlb.probe(0x10, 99).is_some());
    }

    #[test]
    fn megapage_matches_any_vpn0() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        let mut e = mk_entry(0x4000, 0x4000, 1); // vpn1=16, vpn0=0
        e.megapage = true;
        tlb.install(e);
        // Different vpn0 within same vpn1 must hit.
        assert!(tlb.probe(0x4000 | 0x123, 1).is_some());
        // Different vpn1 must miss.
        assert!(tlb.probe(0x8000, 1).is_none());
    }

    #[test]
    fn flush_invalidates_all() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        tlb.install(mk_entry(0x10, 0x100, 1));
        tlb.install(mk_entry(0x11, 0x101, 1));
        tlb.flush();
        assert!(tlb.probe(0x10, 1).is_none());
        assert!(tlb.probe(0x11, 1).is_none());
    }

    #[test]
    fn flush_vaddr_targets_one_entry() {
        let mut tlb = Tlb::new(cfg(8, 2, ReplacementPolicy::Lru));
        tlb.install(mk_entry(0x10, 0x100, 1));
        tlb.install(mk_entry(0x11, 0x101, 1));
        tlb.flush_vaddr(0x10 << 12);
        assert!(tlb.probe(0x10, 1).is_none());
        assert!(tlb.probe(0x11, 1).is_some());
    }

    #[test]
    fn lru_evicts_least_recently_used() {
        // 2 entries, fully-associative → 1 set, 2 ways.
        let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Lru));
        // All three VPNs hash to set 0 (only 1 set).
        tlb.install(mk_entry(0x10, 0xA, 1));
        tlb.install(mk_entry(0x11, 0xB, 1));
        // Touch 0x10 so 0x11 becomes the LRU.
        tlb.probe(0x10, 1).unwrap();
        tlb.install(mk_entry(0x12, 0xC, 1));
        // 0x11 should have been evicted.
        assert!(tlb.probe(0x11, 1).is_none());
        assert!(tlb.probe(0x10, 1).is_some());
        assert!(tlb.probe(0x12, 1).is_some());
        assert_eq!(tlb.stats.evictions, 1);
    }

    #[test]
    fn fifo_evicts_oldest_install_regardless_of_touch() {
        let mut tlb = Tlb::new(cfg(2, 2, ReplacementPolicy::Fifo));
        tlb.install(mk_entry(0x10, 0xA, 1));
        tlb.install(mk_entry(0x11, 0xB, 1));
        // Touching 0x10 in FIFO must NOT save it from eviction.
        tlb.probe(0x10, 1).unwrap();
        tlb.install(mk_entry(0x12, 0xC, 1));
        assert!(tlb.probe(0x10, 1).is_none(), "0x10 should be evicted");
        assert!(tlb.probe(0x11, 1).is_some());
        assert!(tlb.probe(0x12, 1).is_some());
    }
}
