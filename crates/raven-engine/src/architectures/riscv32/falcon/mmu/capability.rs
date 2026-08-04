//! Adapter from RV32's Sv32 MMU to the engine-level translation capability.
//!
//! Everything ISA-specific stops here: the host sees page-table levels, page
//! sizes and permissions described in bits, not an `Sv32` type.

use super::{Mmu, PagingScheme};
use crate::capability::{
    AddressTranslation, TlbEntryView, TlbStatistics, TranslationLevel, TranslationOutcome,
    TranslationPermissions, TranslationResult, TranslationScheme,
};

/// Level names run outermost-first and are numbered the way RISC-V writes them
/// — `VPN[1]` then `VPN[0]` for Sv32 — so the page tree reads like the spec.
fn levels(scheme: &PagingScheme) -> Vec<TranslationLevel> {
    let count = scheme.level_bits.len();
    let mut coverage_shift = u32::from(scheme.offset_bits);
    // Innermost level covers one page; each step out multiplies by that
    // level's fan-out.
    let mut coverages = Vec::with_capacity(count);
    for bits in scheme.level_bits.iter().rev() {
        coverages.push(1u64 << coverage_shift);
        coverage_shift += u32::from(*bits);
    }
    coverages.reverse();

    scheme
        .level_bits
        .iter()
        .enumerate()
        .map(|(index, bits)| TranslationLevel {
            name: format!("VPN[{}]", count - 1 - index),
            index_bits: *bits,
            coverage: coverages[index],
        })
        .collect()
}

impl Mmu {
    /// Whether a load or store would actually be translated right now.
    ///
    /// Mirrors the gate in `Mmu::translate`: paging on, a paging mode selected,
    /// and either not in M-mode or forced. Hosts used to re-derive this from
    /// `satp`, `priv_mode` and `force_translate` themselves, which meant the
    /// panel could disagree with the machine.
    fn translation_applies(&self) -> bool {
        use super::{PrivMode, SatpMode};
        self.enabled
            && self.satp.mode() == SatpMode::Sv32
            && (self.priv_mode != PrivMode::M || self.force_translate)
    }
}

impl AddressTranslation for Mmu {
    fn scheme(&self) -> TranslationScheme {
        if !self.translation_applies() {
            return TranslationScheme {
                name: "bare".into(),
                virtual_bits: 32,
                physical_bits: 32,
                offset_bits: self.scheme.offset_bits,
                levels: Vec::new(),
            };
        }
        // Sv32 is the standard 10+10+12 shape; a scheme the user has retuned in
        // Custom mode reports its own widths rather than claiming to be Sv32.
        let name = if self.scheme == PagingScheme::sv32() {
            "Sv32".to_string()
        } else {
            format!(
                "custom {}+{}",
                self.scheme
                    .level_bits
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join("+"),
                self.scheme.offset_bits
            )
        };
        TranslationScheme {
            name,
            virtual_bits: 32,
            physical_bits: 32,
            offset_bits: self.scheme.offset_bits,
            levels: levels(&self.scheme),
        }
    }

    fn enabled(&self) -> bool {
        self.translation_applies()
    }

    fn root_table(&self) -> Option<u64> {
        self.translation_applies()
            .then(|| u64::from(self.satp.ppn()) << self.scheme.offset_bits)
    }

    fn hit_rate_history(&self) -> Vec<(f64, f64)> {
        self.tlb.stats.history.iter().copied().collect()
    }

    fn tlb_stats(&self) -> TlbStatistics {
        let stats = &self.tlb.stats;
        TlbStatistics {
            hits: stats.hits,
            misses: stats.misses,
            evictions: stats.evictions,
            faults: stats.page_faults,
            total_cycles: stats.total_cycles,
        }
    }

    fn tlb_len(&self) -> usize {
        self.tlb.entries.len()
    }

    fn tlb_sets(&self) -> usize {
        self.tlb.num_sets()
    }

    fn tlb_entry(&self, index: usize) -> Option<TlbEntryView> {
        let entry = self.tlb.entries.get(index)?;
        let associativity = usize::from(self.tlb.config.associativity).max(1);
        Some(TlbEntryView {
            valid: entry.valid,
            virtual_page: u64::from(entry.vpn),
            physical_page: u64::from(entry.ppn),
            address_space: Some(u32::from(entry.asid)),
            permissions: TranslationPermissions {
                read: entry.perms.r,
                write: entry.perms.w,
                execute: entry.perms.x,
                user: entry.perms.u,
            },
            global: entry.global,
            accessed: entry.accessed,
            dirty: entry.dirty,
            superpage_bits: entry.mask_bits,
            set: index / associativity,
        })
    }

    /// Probes the resident entries only. A real walk would need the bus and
    /// could fault or fill, and this is called while drawing a frame — so a
    /// miss is reported as such rather than resolved.
    fn probe(&self, address: u64) -> TranslationResult {
        if !self.translation_applies() {
            return TranslationResult {
                outcome: TranslationOutcome::Identity,
                physical: Some(address),
                permissions: TranslationPermissions {
                    read: true,
                    write: true,
                    execute: true,
                    user: false,
                },
                levels_walked: 0,
            };
        }
        let offset_bits = u32::from(self.scheme.offset_bits);
        let vpn = address >> offset_bits;
        let offset = address & ((1u64 << offset_bits) - 1);

        for index in 0..self.tlb.entries.len() {
            let Some(entry) = self.tlb_entry(index) else {
                continue;
            };
            if !entry.valid {
                continue;
            }
            let shift = u32::from(entry.superpage_bits);
            if entry.virtual_page >> shift != vpn >> shift {
                continue;
            }
            // A superpage keeps the low VPN bits from the virtual address.
            let low_mask = (1u64 << shift) - 1;
            let physical_page = (entry.physical_page & !low_mask) | (vpn & low_mask);
            return TranslationResult {
                outcome: TranslationOutcome::TlbHit,
                physical: Some((physical_page << offset_bits) | offset),
                permissions: entry.permissions,
                levels_walked: 0,
            };
        }

        TranslationResult {
            outcome: TranslationOutcome::Fault,
            physical: None,
            permissions: TranslationPermissions::default(),
            levels_walked: self.scheme.level_bits.len(),
        }
    }
}
