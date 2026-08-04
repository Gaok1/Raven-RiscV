use crate::capability::{
    PipelineControl, PipelineEdge, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineSlotView, PipelineStageRole, PipelineStageView,
    PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceKind, PipelineTraceView, PipelineUnitView,
};

#[derive(Clone, Copy)]
pub(crate) struct StageSpec {
    pub name: &'static str,
    pub role: PipelineStageRole,
}

#[derive(Clone)]
pub(crate) struct TeachingInstruction {
    pub address: u64,
    pub encoding: u32,
    pub disassembly: String,
    pub class: PipelineInstructionClass,
    pub destination: Option<String>,
    pub sources: [Option<String>; 2],
    pub writes: Vec<String>,
    pub branch: bool,
    pub fault: Option<String>,
    row: u64,
    hazard: Option<PipelineHazardKind>,
}

impl TeachingInstruction {
    pub(crate) fn new(
        address: u64,
        encoding: u32,
        disassembly: String,
        class: PipelineInstructionClass,
    ) -> Self {
        Self {
            address,
            encoding,
            disassembly,
            class,
            destination: None,
            sources: [None, None],
            writes: Vec::new(),
            branch: false,
            fault: None,
            row: 0,
            hazard: None,
        }
    }

    fn view(&self) -> PipelineSlotView<'_> {
        PipelineSlotView {
            address: self.address,
            disassembly: &self.disassembly,
            class: self.class,
            destination: self.destination.as_deref(),
            sources: [self.sources[0].as_deref(), self.sources[1].as_deref()],
            bubble: false,
            speculative: false,
            predicted_taken: false,
            hazard: self.hazard,
            atomic: false,
            cycles_remaining: 0,
        }
    }
}

struct Trace {
    kind: PipelineTraceKind,
    from: usize,
    to: usize,
    detail: String,
}

struct Cell {
    label: &'static str,
    state: PipelineTimelineState,
    role: PipelineStageRole,
}

struct Row {
    id: u64,
    disassembly: String,
    class: PipelineInstructionClass,
    first_cycle: u64,
    cells: Vec<Cell>,
}

/// A small scalar in-order pipeline shared by the teaching architectures.
pub(crate) struct TeachingPipeline {
    specs: &'static [StageSpec],
    stages: Vec<Option<TeachingInstruction>>,
    enabled: bool,
    halted: bool,
    faulted: bool,
    fetch_pc: u64,
    instruction_bytes: u64,
    stats: PipelineStats,
    traces: Vec<Trace>,
    rows: Vec<Row>,
    next_row: u64,
    status: Option<String>,
}

impl TeachingPipeline {
    pub(crate) fn new(specs: &'static [StageSpec], instruction_bytes: u64, entry: u64) -> Self {
        Self {
            specs,
            stages: vec![None; specs.len()],
            enabled: false,
            halted: false,
            faulted: false,
            fetch_pc: entry,
            instruction_bytes,
            stats: PipelineStats::default(),
            traces: Vec::new(),
            rows: Vec::new(),
            next_row: 0,
            status: None,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn start_cycle(&mut self) -> Option<TeachingInstruction> {
        self.stats.cycles = self.stats.cycles.saturating_add(1);
        self.traces.clear();
        self.stages.last_mut().and_then(Option::take)
    }

    pub(crate) fn retire(&mut self, instruction: &TeachingInstruction) {
        self.stats.committed = self.stats.committed.saturating_add(1);
        if instruction.branch {
            self.stats.branches = self.stats.branches.saturating_add(1);
        }
    }

    pub(crate) fn halt(&mut self) {
        self.halted = true;
        self.stages.fill(None);
        self.status = Some("halted".into());
    }

    pub(crate) fn fault(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.faulted = true;
        self.stages.fill(None);
        self.status = Some(message);
    }

    /// Shift the datapath after retirement and return the next fetch address.
    pub(crate) fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        if self.halted || self.faulted {
            return None;
        }
        if let Some(target) = redirect {
            let flushed_rows: Vec<u64> = self
                .stages
                .iter()
                .flatten()
                .map(|instruction| instruction.row)
                .collect();
            let flushed = flushed_rows.len();
            for row in flushed_rows {
                self.mark_flushed(row);
            }
            self.stages.fill(None);
            self.fetch_pc = target;
            self.stats.flushes = self.stats.flushes.saturating_add(1);
            self.stats.stalls = self.stats.stalls.saturating_add(flushed as u64);
            self.stats.branch_stalls = self.stats.branch_stalls.saturating_add(flushed as u64);
            self.traces.push(Trace {
                kind: PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush),
                from: self.specs.len().saturating_sub(1),
                to: 0,
                detail: format!("redirect to 0x{target:X}"),
            });
        }

        for instruction in self.stages.iter_mut().flatten() {
            instruction.hazard = None;
        }

        let decode = 1usize;
        // Drain stages after execute even when decode is stalled.
        for destination in (decode + 2..self.stages.len()).rev() {
            if self.stages[destination].is_none() {
                self.stages[destination] = self.stages[destination - 1].take();
            }
        }

        let (forwards, load_use) = self.resolve_operands(decode);
        if let Some((producer, source)) = load_use {
            self.stats.stalls = self.stats.stalls.saturating_add(1);
            self.stats.load_use_stalls = self.stats.load_use_stalls.saturating_add(1);
            if let Some(decoded) = self.stages[decode].as_mut() {
                decoded.hazard = Some(PipelineHazardKind::LoadUse);
            }
            self.traces.push(Trace {
                kind: PipelineTraceKind::Hazard(PipelineHazardKind::LoadUse),
                from: producer,
                to: decode,
                detail: format!("{source} is still being loaded"),
            });
        } else {
            for (producer, source) in forwards {
                self.traces.push(Trace {
                    kind: PipelineTraceKind::Forward,
                    from: producer,
                    to: self.execute_stage(),
                    detail: format!("{source} from {}", self.specs[producer].name),
                });
            }
            if self.stages[decode + 1].is_none() {
                self.stages[decode + 1] = self.stages[decode].take();
            }
            if self.stages[decode].is_none() {
                self.stages[decode] = self.stages[0].take();
            }
        }

        let empty = self.stages.iter().all(Option::is_none);
        let may_fetch = self.stages[0].is_none() && (self.enabled || empty);
        self.record_timeline();
        may_fetch.then_some(self.fetch_pc)
    }

    /// Which of the decoded instruction's operands a bypass covers, and the one
    /// that no bypass can cover.
    ///
    /// By the time this runs the older instructions have already been shifted,
    /// so a producer sitting at stage `p` has *finished* every stage before it.
    /// An operand is therefore forwardable when the producer is already past
    /// the stage that computes its result; when it is not, the consumer has to
    /// wait, which is the load-use case and the only stall forwarding leaves.
    fn resolve_operands(&self, decode: usize) -> (Vec<(usize, String)>, Option<(usize, String)>) {
        let Some(decoded) = self.stages[decode].as_ref() else {
            return (Vec::new(), None);
        };
        let mut forwards = Vec::new();
        for source in decoded.sources.iter().flatten() {
            // Stages closer to decode hold younger instructions, so the first
            // writer found is the one whose value this operand must see.
            let producer = (decode + 1..self.stages.len()).find(|&stage| {
                self.stages[stage]
                    .as_ref()
                    .is_some_and(|older| older.writes.iter().any(|written| written == source))
            });
            let Some(producer) = producer else { continue };
            let class = self.stages[producer]
                .as_ref()
                .map_or(PipelineInstructionClass::Unknown, |older| older.class);
            if producer > self.result_stage(class) {
                forwards.push((producer, source.clone()));
            } else {
                return (Vec::new(), Some((producer, source.clone())));
            }
        }
        (forwards, None)
    }

    /// The stage that produces `class`'s result.
    ///
    /// An ALU result appears at the end of execute; a load's only at the end of
    /// the memory stage. A datapath with no memory stage — SAP's three-stage
    /// one — resolves a load in execute, so one rule covers both shapes.
    fn result_stage(&self, class: PipelineInstructionClass) -> usize {
        let execute = self.execute_stage();
        if class == PipelineInstructionClass::Load {
            self.role_stage(PipelineStageRole::Memory)
                .unwrap_or(execute)
        } else {
            execute
        }
    }

    fn execute_stage(&self) -> usize {
        self.role_stage(PipelineStageRole::Execute)
            .unwrap_or(self.specs.len().saturating_sub(1))
    }

    fn role_stage(&self, role: PipelineStageRole) -> Option<usize> {
        self.specs.iter().position(|spec| spec.role == role)
    }

    pub(crate) fn fetched(&mut self, mut instruction: TeachingInstruction) {
        instruction.row = self.next_row;
        self.next_row = self.next_row.saturating_add(1);
        self.fetch_pc = instruction.address.saturating_add(self.instruction_bytes);
        self.rows.push(Row {
            id: instruction.row,
            disassembly: instruction.disassembly.clone(),
            class: instruction.class,
            first_cycle: self.stats.cycles,
            cells: vec![Cell {
                label: self.specs[0].name,
                state: PipelineTimelineState::Active,
                role: self.specs[0].role,
            }],
        });
        if self.rows.len() > 256 {
            self.rows.remove(0);
        }
        self.stages[0] = Some(instruction);
    }

    fn mark_flushed(&mut self, id: u64) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.cells.push(Cell {
                label: "FL",
                state: PipelineTimelineState::Flushed,
                role: PipelineStageRole::Other,
            });
        }
    }

    fn record_timeline(&mut self) {
        for (index, instruction) in self.stages.iter().enumerate() {
            let Some(instruction) = instruction else {
                continue;
            };
            let Some(row) = self.rows.iter_mut().find(|row| row.id == instruction.row) else {
                continue;
            };
            row.cells.push(Cell {
                label: self.specs[index].name,
                state: if instruction.hazard.is_some() {
                    PipelineTimelineState::Stalled
                } else {
                    PipelineTimelineState::Active
                },
                role: self.specs[index].role,
            });
        }
    }
}

impl PipelineControl for TeachingPipeline {
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self, address: u64) {
        self.stages.fill(None);
        self.halted = false;
        self.faulted = false;
        self.fetch_pc = address;
        self.stats = PipelineStats::default();
        self.traces.clear();
        self.rows.clear();
        self.status = None;
    }

    fn redirect(&mut self, address: u64) {
        self.stages.fill(None);
        self.fetch_pc = address;
        self.halted = false;
        self.faulted = false;
        self.status = None;
    }
}

impl PipelineInspect for TeachingPipeline {
    fn status(&self) -> PipelineStatus {
        PipelineStatus {
            enabled: self.enabled,
            sequential: !self.enabled,
            halted: self.halted,
            faulted: self.faulted,
        }
    }

    fn stats(&self) -> PipelineStats {
        self.stats
    }

    fn stage_count(&self) -> usize {
        self.specs.len()
    }

    /// The straight chain plus one bypass per stage that can feed a value back
    /// into execute. A datapath whose execute stage is last — SAP's — declares
    /// none, because there is nothing downstream to bypass from.
    fn edges(&self) -> Vec<PipelineEdge> {
        let execute = self.execute_stage();
        let mut edges: Vec<PipelineEdge> = (1..self.specs.len())
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        edges.extend((execute + 1..self.specs.len()).map(|from| PipelineEdge {
            from,
            to: execute,
            kind: PipelineEdgeKind::Forward,
        }));
        edges
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let spec = self.specs.get(index)?;
        Some(PipelineStageView {
            name: spec.name,
            role: spec.role,
            slot: self.stages[index].as_ref().map(TeachingInstruction::view),
        })
    }

    fn unit_count(&self) -> usize {
        0
    }

    fn unit(&self, _index: usize) -> Option<PipelineUnitView<'_>> {
        None
    }

    fn trace_count(&self) -> usize {
        self.traces.len()
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        let trace = self.traces.get(index)?;
        Some(PipelineTraceView {
            kind: trace.kind,
            from_stage: trace.from,
            to_stage: trace.to,
            detail: &trace.detail,
        })
    }

    fn status_message(&self) -> Option<&str> {
        self.status.as_deref()
    }

    fn timeline_len(&self) -> usize {
        self.rows.len()
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        let row = self.rows.get(index)?;
        Some(PipelineTimelineRow {
            disassembly: &row.disassembly,
            class: row.class,
            first_cycle: row.first_cycle,
            cells: row.cells.len(),
            atomic: false,
        })
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        let cell = self.rows.get(row)?.cells.get(cell)?;
        Some(PipelineTimelineCell {
            label: cell.label,
            state: cell.state,
            role: cell.role,
        })
    }
}
