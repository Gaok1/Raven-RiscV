// ui/app/vm.rs — the Virtual Memory tab's actions on the machine.
//
// Every setting applies the moment it changes: a new mode writes or drops the
// simulator's page map, a new TLB shape rebuilds the TLB, new map permissions
// rewrite the map. There is no "apply" step to forget.

use raven_engine::capability::{
    AddressTranslation, TableEntryKind, TableEntryView, TranslationRecordView,
};

use super::{App, VmField};
use crate::falcon::mmu::{PageMapSpec, TlbConfig, VmMode};

/// Bounds for the typed config fields.
const MAX_ASID: u16 = 511;

impl App {
    /// Drop every cached translation, in every core's TLB.
    pub(crate) fn flush_tlb(&mut self) {
        if let Some(runtime) = self.rv32_mut() {
            runtime.mem_mut_unjournaled().flush_all_tlbs();
        }
        self.tlb.config_status = Some(if self.max_cores > 1 {
            "every core's TLB flushed".into()
        } else {
            "TLB flushed".into()
        });
        self.tlb.config_error = None;
    }

    /// The translations the machine still remembers, oldest first.
    pub(crate) fn vm_records(&self) -> Vec<TranslationRecordView> {
        self.translation()
            .map(|translation| translation.recent())
            .unwrap_or_default()
    }

    /// The access every VM view is about: the pinned record, or the newest one
    /// of the followed kind — the newest of any kind if there is none yet.
    pub(crate) fn vm_current(&self) -> Option<TranslationRecordView> {
        if let Some(pinned) = &self.tlb.pinned {
            return Some(pinned.clone());
        }
        let records = self.vm_records();
        let follow = self.tlb.follow;
        records
            .iter()
            .rev()
            .find(|r| follow.admits(r.access))
            .or_else(|| records.last())
            .cloned()
    }

    /// The TLB slot under the cursor: the reader's, or the one the current
    /// access touched, or the first way of the set it probed.
    pub(crate) fn vm_tlb_cursor(&self) -> Option<usize> {
        let translation = self.translation()?;
        let len = translation.tlb_len();
        if len == 0 {
            return None;
        }
        if let Some(slot) = self.tlb.tlb_cursor {
            return Some(slot.min(len - 1));
        }
        let ways = (len / translation.tlb_sets().max(1)).max(1);
        let rec = self.vm_current()?;
        rec.installed_entry
            .or(rec.hit_entry)
            .or_else(|| rec.probed_sets.first().map(|&(set, _)| set * ways))
            .map(|slot| slot.min(len - 1))
    }

    /// Where the simulator's page tables end: just below the lowest hart's
    /// stack, so no push can reach a PTE.
    pub(crate) fn vm_tables_top(&self) -> u32 {
        self.stack_slot_bounds(self.max_cores.saturating_sub(1)).0
    }

    /// The span of the loaded program — text, data and bss — which the map
    /// refines into 4 KiB pages.
    pub(crate) fn vm_window(&self) -> (u32, u32) {
        let start = self.session.base_pc.min(self.session.data_base);
        let end = self.session.heap_start;
        if end > start { (start, end) } else { (0, 0) }
    }

    /// Write the simulator's page map and point the hart at it. Errors land in
    /// the footer.
    pub(crate) fn install_auto_map(&mut self) {
        let (top, spec, window) = (self.vm_tables_top(), self.tlb.page_map, self.vm_window());
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        match runtime
            .mem_mut_unjournaled()
            .install_auto_map(top, spec, window)
        {
            Ok(satp) => {
                runtime.cpu_mut_unjournaled().satp = satp;
                self.tlb.config_error = None;
            }
            Err(error) => {
                self.tlb.config_error = Some(format!("page map not installed: {error}"));
            }
        }
        self.tlb.follow_current();
    }

    /// Forget the simulator's map: satp goes back to Bare if it still pointed
    /// there, so a program that drops to S/U without writing satp does not
    /// walk tables it never built.
    fn drop_auto_map(&mut self) {
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        let mem = runtime.mem_mut_unjournaled();
        let Some(layout) = mem.mmu().auto_map else {
            return;
        };
        let pointed_there = mem.mmu().satp.ppn() << 12 == layout.root;
        let mmu = mem.mmu_mut();
        mmu.auto_map = None;
        if pointed_there {
            mmu.satp = crate::falcon::mmu::Satp::new(0);
            runtime.cpu_mut_unjournaled().satp = 0;
        }
    }

    /// Switch mode now: the auto map is written the moment auto is chosen, not
    /// at the next program load.
    pub(crate) fn apply_vm_mode(&mut self, mode: VmMode) {
        if mode != VmMode::Auto {
            self.drop_auto_map();
        }
        if let Some(runtime) = self.rv32_mut() {
            let mem = runtime.mem_mut_unjournaled();
            mem.flush_all_tlbs();
            mem.mmu_mut().trace.clear();
        }
        if mode == VmMode::Auto {
            self.install_auto_map();
        }
        self.tlb.follow_current();
    }

    /// Rebuild every core's TLB in a new shape now, and for every later load.
    /// Their entries and counters start over.
    pub(crate) fn set_tlb_geometry(&mut self, mut cfg: TlbConfig) {
        cfg.entry_count = cfg.entry_count.clamp(1, 256);
        cfg.associativity = cfg
            .associativity
            .clamp(1, u8::try_from(cfg.entry_count).unwrap_or(u8::MAX));
        self.tlb.pending = cfg.clone();
        if let Some(runtime) = self.rv32_mut() {
            runtime.mem_mut_unjournaled().reconfigure_tlbs(cfg);
        }
        self.tlb.tlb_cursor = None;
        self.tlb.tlb_scroll = 0;
        self.tlb.config_status = Some("TLB rebuilt — its entries and counters start over".into());
        self.tlb.config_error = None;
    }

    /// Change what the simulator's map grants, rewriting it if it is in use.
    pub(crate) fn set_page_map(&mut self, spec: PageMapSpec) {
        self.tlb.page_map = spec;
        if self.vm_mode() == VmMode::Auto {
            self.install_auto_map();
            if self.tlb.config_error.is_none() {
                self.tlb.config_status = Some("page map rewritten — TLB flushed".into());
            }
        }
    }

    /// The value a config field shows, and starts its edit from.
    pub(crate) fn vm_field_value(&self, field: VmField) -> String {
        match field {
            VmField::Asid => self.tlb.page_map.asid.to_string(),
            VmField::HitCycles => self.tlb.pending.hit_latency.to_string(),
            VmField::WalkCycles => self.tlb.pending.miss_penalty.to_string(),
        }
    }

    pub(crate) fn start_vm_edit(&mut self, field: VmField) {
        self.commit_vm_edit();
        self.tlb.edit = Some((field, self.vm_field_value(field)));
    }

    /// Apply the field being typed into, if any, and close it.
    pub(crate) fn commit_vm_edit(&mut self) {
        let Some((field, text)) = self.tlb.edit.take() else {
            return;
        };
        let Ok(value) = text.trim().parse::<u32>() else {
            if !text.trim().is_empty() {
                self.tlb.config_error = Some(format!("'{text}' is not a number"));
            }
            return;
        };
        match field {
            VmField::Asid => {
                let mut spec = self.tlb.page_map;
                spec.asid = value.min(u32::from(MAX_ASID)) as u16;
                self.set_page_map(spec);
            }
            VmField::HitCycles | VmField::WalkCycles => {
                let cycles = value.min(u32::from(u8::MAX)) as u8;
                let mut cfg = self.tlb.pending.clone();
                if field == VmField::HitCycles {
                    cfg.hit_latency = cycles;
                } else {
                    cfg.miss_penalty = cycles;
                }
                // Latencies do not change what the TLB holds, so they apply
                // in place instead of rebuilding it.
                let (hit, walk) = (cfg.hit_latency, cfg.miss_penalty);
                self.tlb.pending = cfg;
                if let Some(runtime) = self.rv32_mut() {
                    runtime.mem_mut_unjournaled().set_tlb_latencies(hit, walk);
                }
                self.tlb.config_status = Some("TLB latency updated".into());
            }
        }
    }
}

// ── The map's model ──────────────────────────────────────────────────────────
//
// A page table has 1024 entries, most of them alike: runs of invalid entries,
// runs of leaves mapping consecutive frames with the same permissions. The map
// shows such a run as one row, so a reader sees the table's shape instead of
// scrolling past a thousand lines; a run opens on Enter. The entry on the
// current access's path, and the one under the cursor, always stand alone.

/// Shortest run of alike entries the map folds into one row.
const MIN_RUN: usize = 3;

/// One row of a map column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MapRow {
    One(TableEntryView),
    Run {
        first: TableEntryView,
        last: TableEntryView,
        count: usize,
    },
}

impl MapRow {
    pub(crate) fn first(&self) -> &TableEntryView {
        match self {
            MapRow::One(entry) | MapRow::Run { first: entry, .. } => entry,
        }
    }

    pub(crate) fn contains(&self, index: u64) -> bool {
        match self {
            MapRow::One(entry) => entry.index == index,
            MapRow::Run { first, last, .. } => (first.index..=last.index).contains(&index),
        }
    }
}

/// One level of the table, as the map shows it.
pub(crate) struct MapColumn {
    pub(crate) level: usize,
    /// Physical address of this table.
    pub(crate) table: u64,
    /// The virtual address the first entry of this table maps.
    pub(crate) va_base: u64,
    pub(crate) rows: Vec<MapRow>,
    /// The row holding the selected entry.
    pub(crate) selected: usize,
    /// The selected entry's index.
    pub(crate) selected_index: u64,
    /// The index the traced address takes through this table, when this table
    /// is on its path.
    pub(crate) path: Option<u64>,
}

impl MapColumn {
    pub(crate) fn selected_entry(&self) -> Option<TableEntryView> {
        match self.rows.get(self.selected)? {
            MapRow::One(entry) => Some(*entry),
            MapRow::Run { first, .. } => Some(*first),
        }
    }
}

/// Two entries continue one run: both empty, or both leaves granting the same
/// thing to consecutive frames.
fn alike(prev: &TableEntryView, next: &TableEntryView, coverage: u64) -> bool {
    match (prev.kind, next.kind) {
        (TableEntryKind::Invalid, TableEntryKind::Invalid) => true,
        (
            TableEntryKind::Leaf {
                frame: a,
                permissions: pa,
                global: ga,
                accessed: aa,
                dirty: da,
            },
            TableEntryKind::Leaf {
                frame: b,
                permissions: pb,
                global: gb,
                accessed: ab,
                dirty: db,
            },
        ) => b == a.wrapping_add(coverage) && pa == pb && ga == gb && aa == ab && da == db,
        _ => false,
    }
}

/// Fold `entries` into rows, leaving the indices in `alone` and every run the
/// reader opened as single rows.
fn fold(
    entries: &[TableEntryView],
    coverage: u64,
    alone: &[u64],
    open: impl Fn(u64) -> bool,
) -> Vec<MapRow> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let mut j = i + 1;
        if !alone.contains(&entries[i].index) {
            while j < entries.len()
                && !alone.contains(&entries[j].index)
                && alike(&entries[j - 1], &entries[j], coverage)
            {
                j += 1;
            }
        }
        if j - i >= MIN_RUN && !open(entries[i].index) {
            rows.push(MapRow::Run {
                first: entries[i],
                last: entries[j - 1],
                count: j - i,
            });
        } else {
            rows.extend(entries[i..j].iter().copied().map(MapRow::One));
        }
        i = j;
    }
    rows
}

impl App {
    /// The address the map traces: one the reader typed, or the current access.
    pub(crate) fn vm_map_target(&self) -> Option<u64> {
        self.tlb
            .map_target
            .or_else(|| self.vm_current().map(|r| r.virtual_address))
    }

    /// The map's columns, root first. The second appears when the entry
    /// selected in the root points to a table.
    pub(crate) fn vm_map_columns(&self) -> Vec<MapColumn> {
        let Some(translation) = self.translation() else {
            return Vec::new();
        };
        let Some(root) = translation.root_table() else {
            return Vec::new();
        };
        let scheme = translation.scheme();
        let target = self.vm_map_target();
        let mut columns: Vec<MapColumn> = Vec::new();
        let mut table = root;
        let mut va_base = 0u64;
        let mut on_path = true;

        for level in 0..scheme.levels.len().min(super::MAP_COLUMNS) {
            let entries = translation.read_table(table, level);
            if entries.is_empty() {
                break;
            }
            let path = if on_path {
                target.map(|va| scheme.index_at(va, level))
            } else {
                None
            };
            let selected_index = self.tlb.map_cursor[level]
                .or(path)
                .unwrap_or(0)
                .min(entries.len() as u64 - 1);
            let coverage = scheme.levels[level].coverage;
            let alone: Vec<u64> = [Some(selected_index), path].into_iter().flatten().collect();
            let rows = fold(&entries, coverage, &alone, |first| {
                self.tlb.map_open.contains(&(table, first))
            });
            let selected = rows
                .iter()
                .position(|row| row.contains(selected_index))
                .unwrap_or(0);
            let next = match entries[selected_index as usize].kind {
                TableEntryKind::Pointer { table } => Some(table),
                _ => None,
            };
            columns.push(MapColumn {
                level,
                table,
                va_base,
                rows,
                selected,
                selected_index,
                path,
            });
            let Some(next) = next else {
                break;
            };
            on_path = path == Some(selected_index);
            va_base += selected_index * coverage;
            table = next;
        }
        columns
    }
}
