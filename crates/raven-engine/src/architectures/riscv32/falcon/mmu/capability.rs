//! Adapter from RV32's Sv32 MMU to the engine-level translation capability.
//!
//! Everything ISA-specific stops here: the host sees page-table levels, page
//! sizes, permissions and entry fields described in bits, not an `Sv32` type.
//! The capability lives on the cache controller rather than the `Mmu`, because
//! reading a page table — or walking one for a probe — needs the memory the
//! controller owns.

use super::trace::{TraceOutcome, TranslationRecord};
use super::{
    AccessType, LEVELS, MEGAPAGE_BITS, PAGE_BITS, PrivMode, Pte, Tlb, TlbEntry, TlbStats, VPN_BITS,
    page_bytes,
};
use crate::capability::{
    AddressTranslation, EntryField, TableEntryKind, TableEntryView, TlbEntryView, TlbStatistics,
    TranslationAccess, TranslationLevel, TranslationOutcome, TranslationPermissions,
    TranslationRecordView, TranslationScheme, WalkStepView,
};
use crate::falcon::cache::{CacheController, ReplacementPolicy};

/// The Sv32 PTE, most significant field first.
const SV32_ENTRY: [EntryField; 10] = [
    EntryField { name: "PPN", high: 31, low: 10 },
    EntryField { name: "RSW", high: 9, low: 8 },
    EntryField { name: "D", high: 7, low: 7 },
    EntryField { name: "A", high: 6, low: 6 },
    EntryField { name: "G", high: 5, low: 5 },
    EntryField { name: "U", high: 4, low: 4 },
    EntryField { name: "X", high: 3, low: 3 },
    EntryField { name: "W", high: 2, low: 2 },
    EntryField { name: "R", high: 1, low: 1 },
    EntryField { name: "V", high: 0, low: 0 },
];

/// Level names run outermost-first and are numbered the way RISC-V writes them
/// — `VPN[1]` then `VPN[0]` — so the page tree reads like the spec.
fn sv32_levels() -> Vec<TranslationLevel> {
    (0..LEVELS)
        .rev()
        .map(|level| TranslationLevel {
            name: format!("VPN[{level}]"),
            index_bits: VPN_BITS as u8,
            coverage: 1u64 << (PAGE_BITS + VPN_BITS * level as u32),
        })
        .collect()
}

/// Sv32 numbers levels from the leaf up; the capability from the root down.
fn view_level(sv32_level: u8) -> usize {
    LEVELS - 1 - usize::from(sv32_level)
}

fn permissions(perms: super::PtePerms) -> TranslationPermissions {
    TranslationPermissions {
        read: perms.r,
        write: perms.w,
        execute: perms.x,
        user: perms.u,
    }
}

fn access_view(access: AccessType) -> TranslationAccess {
    match access {
        AccessType::Fetch => TranslationAccess::Fetch,
        AccessType::Load => TranslationAccess::Load,
        AccessType::Store => TranslationAccess::Store,
    }
}

fn privilege(mode: PrivMode) -> &'static str {
    match mode {
        PrivMode::M => "M",
        PrivMode::S => "S",
        PrivMode::U => "U",
    }
}

/// Decode one PTE.
fn entry_view(index: u64, address: u32, raw: u32) -> TableEntryView {
    let pte = Pte::new(raw);
    let kind = if !pte.valid() {
        TableEntryKind::Invalid
    } else if pte.is_reserved() {
        TableEntryKind::Reserved
    } else if pte.is_leaf() {
        TableEntryKind::Leaf {
            frame: u64::from(pte.target()),
            permissions: permissions(pte.perms()),
            global: pte.global(),
            accessed: pte.accessed(),
            dirty: pte.dirty(),
        }
    } else {
        TableEntryKind::Pointer {
            table: u64::from(pte.target()),
        }
    };
    TableEntryView {
        index,
        address: u64::from(address),
        raw: u64::from(raw),
        kind,
    }
}

fn policy_label(policy: ReplacementPolicy) -> &'static str {
    match policy {
        ReplacementPolicy::Lru => "LRU",
        ReplacementPolicy::Mru => "MRU",
        ReplacementPolicy::Fifo => "FIFO",
        ReplacementPolicy::Random => "Random",
        ReplacementPolicy::Lfu => "LFU",
        ReplacementPolicy::Clock => "Clock",
    }
}

fn stats_view(stats: &TlbStats) -> TlbStatistics {
    TlbStatistics {
        hits: stats.hits,
        misses: stats.misses,
        evictions: stats.evictions,
        faults: stats.page_faults,
        total_cycles: stats.total_cycles,
        walk_cycles: stats.walk_cycles,
        first_touches: stats.first_touches,
        first_touch_bytes: stats.first_touch_bytes,
        dirty_rewalks: stats.dirty_rewalks,
        walk_faults: stats.walk_faults,
    }
}

fn entry_view_at(tlb: &Tlb, slot: usize, entry: &TlbEntry) -> TlbEntryView {
    let associativity = tlb.associativity();
    TlbEntryView {
        valid: entry.valid,
        virtual_page: u64::from(entry.vpn),
        physical_page: u64::from(entry.ppn),
        address_space: Some(u32::from(entry.asid)),
        permissions: permissions(entry.perms),
        global: entry.global,
        // Entries are only installed after a walk has set A.
        accessed: entry.valid,
        dirty: entry.dirty,
        superpage_bits: if entry.megapage { MEGAPAGE_BITS } else { 0 },
        set: slot / associativity,
        way: slot % associativity,
        recency: tlb.recency(slot),
    }
}

impl CacheController {
    fn record_view(&self, rec: &TranslationRecord) -> TranslationRecordView {
        let probed_sets = if rec.tlb_used {
            let [small, mega] = rec.probed_sets;
            vec![
                (usize::from(small), page_bytes(false)),
                (usize::from(mega), page_bytes(true)),
            ]
        } else {
            Vec::new()
        };
        let steps = rec
            .steps()
            .iter()
            .map(|step| WalkStepView {
                level: view_level(step.level),
                entry: entry_view(u64::from(step.index), step.pte_addr, step.pte),
                served_by: step.served_by,
                // Only a translation that ran was charged.
                cycles: (rec.seq != 0).then_some(u32::from(step.cycles)),
            })
            .collect();
        let outcome = match rec.outcome {
            TraceOutcome::Hit => TranslationOutcome::TlbHit,
            TraceOutcome::Walked => TranslationOutcome::Walked,
            TraceOutcome::Fault(_) => TranslationOutcome::Fault,
        };
        let evicted = rec.evicted.as_ref().and_then(|old| {
            rec.installed_slot
                .map(|slot| entry_view_at(&self.mmu.tlb, usize::from(slot), old))
        });
        TranslationRecordView {
            seq: rec.seq,
            hart: rec.hart,
            pc: u64::from(rec.pc),
            access: access_view(rec.access),
            privilege: privilege(rec.priv_mode),
            virtual_address: u64::from(rec.vaddr),
            address_space: Some(u32::from(rec.asid)),
            probed_sets,
            hit_entry: rec.hit_slot.map(usize::from),
            rewalk_for_dirty: rec.dirty_rewalk,
            steps,
            outcome,
            fault: rec.fault().map(|reason| reason.describe(rec.access)),
            fault_code: rec.fault().map(|_| rec.access.page_fault_cause()),
            physical: rec.paddr.map(u64::from),
            page_size: page_bytes(rec.megapage),
            cycles: rec.cycles,
            installed_entry: rec.installed_slot.map(usize::from),
            evicted: evicted.map(|mut view| {
                // The evicted entry no longer has a place in the set's order.
                view.recency = None;
                view
            }),
            set_accessed: rec.set_accessed,
            set_dirty: rec.set_dirty,
        }
    }
}

impl AddressTranslation for CacheController {
    fn scheme(&self) -> TranslationScheme {
        let on = self.mmu.translation_applies();
        TranslationScheme {
            name: if on { "Sv32" } else { "bare" }.to_string(),
            virtual_bits: 32,
            physical_bits: 32,
            offset_bits: PAGE_BITS as u8,
            levels: if on { sv32_levels() } else { Vec::new() },
            entry_bits: 32,
            entry_fields: SV32_ENTRY.to_vec(),
        }
    }

    fn enabled(&self) -> bool {
        self.mmu.translation_applies()
    }

    fn idle_reason(&self) -> Option<String> {
        self.mmu.idle_reason().map(str::to_string)
    }

    fn root_table(&self) -> Option<u64> {
        self.mmu
            .translation_applies()
            .then(|| u64::from(self.mmu.satp.ppn()) << PAGE_BITS)
    }

    fn tlb_stats(&self) -> TlbStatistics {
        stats_view(&self.mmu.tlb.stats)
    }

    fn tlb_len(&self) -> usize {
        self.mmu.tlb.entries.len()
    }

    fn tlb_entry(&self, index: usize) -> Option<TlbEntryView> {
        self.core_tlb_entry(self.active_core(), index)
    }

    fn tlb_sets(&self) -> usize {
        self.mmu.tlb.num_sets()
    }

    fn tlb_policy(&self) -> String {
        policy_label(self.mmu.tlb.config.replacement).to_string()
    }

    fn tlb_enabled(&self) -> bool {
        self.mmu.tlb_enabled
    }

    fn probe(&self, address: u64) -> TranslationRecordView {
        self.core_probe(self.active_core(), address)
    }

    fn core_probe(&self, core: usize, address: u64) -> TranslationRecordView {
        let vaddr = address as u32;
        match self.dry_run_translation_on(core, vaddr, AccessType::Load) {
            Some(rec) => self.record_view(&rec),
            None => TranslationRecordView {
                seq: 0,
                hart: 0,
                pc: 0,
                access: TranslationAccess::Load,
                privilege: privilege(self.mmu.priv_mode),
                virtual_address: address,
                address_space: None,
                probed_sets: Vec::new(),
                hit_entry: None,
                rewalk_for_dirty: false,
                steps: Vec::new(),
                outcome: TranslationOutcome::Identity,
                fault: None,
                fault_code: None,
                physical: Some(address),
                page_size: page_bytes(false),
                cycles: 0,
                installed_entry: None,
                evicted: None,
                set_accessed: false,
                set_dirty: false,
            },
        }
    }

    fn recent(&self) -> Vec<TranslationRecordView> {
        self.mmu
            .trace
            .records()
            .map(|rec| self.record_view(rec))
            .collect()
    }

    fn read_table(&self, address: u64, level: usize) -> Vec<TableEntryView> {
        if level >= LEVELS {
            return Vec::new();
        }
        let Ok(base) = u32::try_from(address) else {
            return Vec::new();
        };
        (0..(1u32 << VPN_BITS))
            .map_while(|index| {
                let at = base.checked_add(index * 4)?;
                let raw = self.read_pte(at)?;
                Some(entry_view(u64::from(index), at, raw))
            })
            .collect()
    }

    fn hit_rate_history(&self) -> Vec<(f64, f64)> {
        self.mmu.tlb.stats.history.iter().copied().collect()
    }

    fn tlb_count(&self) -> usize {
        CacheController::tlb_count(self)
    }

    fn core_tlb_stats(&self, core: usize) -> TlbStatistics {
        stats_view(&self.tlb_of(core).stats)
    }

    fn core_tlb_entry(&self, core: usize, index: usize) -> Option<TlbEntryView> {
        let tlb = self.tlb_of(core);
        let entry = tlb.entries.get(index)?;
        Some(entry_view_at(tlb, index, entry))
    }

    fn core_hit_rate_history(&self, core: usize) -> Vec<(f64, f64)> {
        self.tlb_of(core).stats.history.iter().copied().collect()
    }
}
