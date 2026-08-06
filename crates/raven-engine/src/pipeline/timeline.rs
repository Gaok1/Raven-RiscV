//! What a host reads back about cycles that have already happened.
//!
//! Two records, both written by an engine and read through
//! [`PipelineInspect`](crate::capability::PipelineInspect): the Gantt history —
//! one row per instruction, one cell per cycle it was visible — and the
//! per-cycle [`Trace`]s explaining why something moved or did not. The recorder
//! keeps the rows bounded so a long run cannot grow without limit.

use crate::capability::{
    PipelineInstructionClass, PipelineStageRole, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceKind,
};

/// One thing that happened between two stages this cycle — a bypass that
/// carried an operand, a hazard that held something back.
#[derive(Clone)]
pub(super) struct Trace {
    pub kind: PipelineTraceKind,
    pub from: usize,
    pub to: usize,
    pub detail: String,
}

#[derive(Clone)]
pub(super) struct Cell {
    pub label: &'static str,
    pub state: PipelineTimelineState,
    pub role: PipelineStageRole,
}

#[derive(Clone)]
pub(super) struct Row {
    pub id: u64,
    /// The instruction this row follows, so a host can light it up from
    /// somewhere else on screen.
    pub address: u64,
    pub disassembly: String,
    pub class: PipelineInstructionClass,
    pub first_cycle: u64,
    pub atomic: bool,
    pub cells: Vec<Cell>,
}

/// Cells kept per row before the oldest cycle scrolls off.
///
/// A row normally collects a handful — one per stage it passes through — but an
/// instruction that sits waiting, on input or on a long unit, collects one every
/// cycle for as long as it waits. Without a cap a paused machine would grow the
/// history forever.
const MAX_CELLS: usize = 200;

#[derive(Clone)]
pub(super) struct Timeline {
    rows: Vec<Row>,
    capacity: usize,
}

impl Timeline {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            rows: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub(super) fn clear(&mut self) {
        self.rows.clear();
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn open(
        &mut self,
        id: u64,
        address: u64,
        disassembly: String,
        class: PipelineInstructionClass,
        atomic: bool,
        first_cycle: u64,
        first: Cell,
    ) {
        self.rows.push(Row {
            id,
            address,
            disassembly,
            class,
            first_cycle,
            atomic,
            cells: vec![first],
        });
        if self.rows.len() > self.capacity {
            self.rows.remove(0);
        }
    }

    pub(super) fn push(&mut self, id: u64, cell: Cell) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.cells.push(cell);
            if row.cells.len() > MAX_CELLS {
                row.cells.remove(0);
                row.first_cycle = row.first_cycle.saturating_add(1);
            }
        }
    }

    pub(super) fn row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        let row = self.rows.get(index)?;
        Some(PipelineTimelineRow {
            address: row.address,
            disassembly: &row.disassembly,
            class: row.class,
            first_cycle: row.first_cycle,
            cells: row.cells.len(),
            atomic: row.atomic,
        })
    }

    pub(super) fn cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        let cell = self.rows.get(row)?.cells.get(cell)?;
        Some(PipelineTimelineCell {
            label: cell.label,
            state: cell.state,
            role: cell.role,
        })
    }
}
