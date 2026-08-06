use super::{BinaryOp, Decoded, DecodedOp, Operand, REG64, UnaryOp, WideningOp, reg_name};
use crate::capability::{
    PipelineControl, PipelineEdge, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineSlotView, PipelineStageRole, PipelineStageView,
    PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceKind, PipelineTraceView, PipelineUnitView,
};

const FETCH: usize = 0;
const DECODE: usize = 1;
const ISSUE: usize = 2;
const EXECUTE: usize = 3;
const MEMORY: usize = 4;
const RETIRE: usize = 5;
const STAGES: usize = 6;

const STAGE_NAMES: [&str; STAGES] = ["IF", "ID", "IS", "EX", "MEM", "RT"];
const STAGE_ROLES: [PipelineStageRole; STAGES] = [
    PipelineStageRole::Fetch,
    PipelineStageRole::Decode,
    PipelineStageRole::Issue,
    PipelineStageRole::Execute,
    PipelineStageRole::Memory,
    PipelineStageRole::Commit,
];

#[derive(Clone)]
pub(super) struct PipelineInstruction {
    pub address: u64,
    pub decoded: Decoded,
    pub fault: Option<String>,
    class: PipelineInstructionClass,
    destination: Option<String>,
    sources: Vec<String>,
    writes: Vec<String>,
    branch: bool,
    row: u64,
    hazard: Option<PipelineHazardKind>,
    speculative: bool,
}

impl PipelineInstruction {
    pub(super) fn new(address: u64, decoded: Decoded) -> Self {
        let (class, destination, sources, writes, branch) = metadata(&decoded);
        Self {
            address,
            decoded,
            fault: None,
            class,
            destination,
            sources,
            writes,
            branch,
            row: 0,
            hazard: None,
            speculative: false,
        }
    }

    pub(super) fn fault(address: u64, message: impl Into<String>) -> Self {
        Self {
            address,
            decoded: Decoded {
                len: 1,
                text: "fetch fault".into(),
                class: "Fault",
                op: DecodedOp::Nop,
            },
            fault: Some(message.into()),
            class: PipelineInstructionClass::Unknown,
            destination: None,
            sources: Vec::new(),
            writes: Vec::new(),
            branch: false,
            row: 0,
            hazard: None,
            speculative: false,
        }
    }

    pub(super) fn sequential_address(&self) -> u64 {
        self.address.wrapping_add(self.decoded.len as u64)
    }

    pub(super) fn is_branch(&self) -> bool {
        self.branch
    }

    fn view(&self) -> PipelineSlotView<'_> {
        PipelineSlotView {
            address: self.address,
            disassembly: &self.decoded.text,
            class: self.class,
            destination: self.destination.as_deref(),
            sources: [
                self.sources.first().map(String::as_str),
                self.sources.get(1).map(String::as_str),
            ],
            bubble: false,
            speculative: self.speculative,
            predicted_taken: false,
            hazard: self.hazard,
            atomic: false,
            cycles_remaining: 0,
        }
    }
}

fn metadata(
    decoded: &Decoded,
) -> (
    PipelineInstructionClass,
    Option<String>,
    Vec<String>,
    Vec<String>,
    bool,
) {
    let mut sources = Vec::new();
    let mut writes = Vec::new();
    let mut destination = None;
    let mut branch = false;
    let class = match decoded.op {
        DecodedOp::Nop => PipelineInstructionClass::Alu,
        DecodedOp::Halt | DecodedOp::Syscall => {
            if matches!(decoded.op, DecodedOp::Syscall) {
                sources.extend(["rax", "rdi", "rsi", "rdx"].map(str::to_string));
                writes.push("rax".into());
            }
            PipelineInstructionClass::System
        }
        DecodedOp::Mov { dst, src } => {
            add_source(src, &mut sources);
            add_memory_base(dst, &mut sources);
            add_write(dst, &mut destination, &mut writes);
            if matches!(src, Operand::Memory(_)) {
                PipelineInstructionClass::Load
            } else if matches!(dst, Operand::Memory(_)) {
                PipelineInstructionClass::Store
            } else {
                PipelineInstructionClass::Alu
            }
        }
        DecodedOp::MovImmediate { dst, .. } | DecodedOp::Lea { dst, .. } => {
            if let DecodedOp::Lea { address, .. } = decoded.op {
                add_memory_base(Operand::Memory(address), &mut sources);
            }
            let name = reg_name(dst).to_string();
            destination = Some(name.clone());
            writes.push(name);
            PipelineInstructionClass::Alu
        }
        DecodedOp::Binary { op, dst, src } => {
            add_source(dst, &mut sources);
            add_source(src, &mut sources);
            if !matches!(op, BinaryOp::Cmp | BinaryOp::Test) {
                add_write(dst, &mut destination, &mut writes);
            }
            writes.push("rflags".into());
            if op == BinaryOp::Imul {
                PipelineInstructionClass::Multiply
            } else {
                PipelineInstructionClass::Alu
            }
        }
        DecodedOp::BinaryImmediate { op, dst, .. } => {
            let name = reg_name(dst).to_string();
            sources.push(name.clone());
            if op != BinaryOp::Cmp {
                destination = Some(name.clone());
                writes.push(name);
            }
            writes.push("rflags".into());
            PipelineInstructionClass::Alu
        }
        DecodedOp::Inc { reg, .. } => {
            let name = reg_name(reg).to_string();
            sources.push(name.clone());
            destination = Some(name.clone());
            writes.push(name);
            writes.push("rflags".into());
            PipelineInstructionClass::Alu
        }
        DecodedOp::Shift { dst, amount, .. } => {
            let name = reg_name(dst).to_string();
            sources.push(name.clone());
            // The `cl` form reads rcx, so `mov cl, 4` right before a shift is
            // a genuine hazard the pipeline view should show.
            if amount.is_none() {
                sources.push("rcx".into());
            }
            destination = Some(name.clone());
            writes.push(name);
            writes.push("rflags".into());
            PipelineInstructionClass::Alu
        }
        DecodedOp::Unary { op, dst } => {
            let name = reg_name(dst).to_string();
            sources.push(name.clone());
            destination = Some(name.clone());
            writes.push(name);
            if op != UnaryOp::Not {
                writes.push("rflags".into());
            }
            PipelineInstructionClass::Alu
        }
        DecodedOp::Widening { op, src } => {
            // Both halves are implicit operands, which is exactly what makes
            // these instructions worth showing in a pipeline view.
            sources.extend([reg_name(src).to_string(), "rax".into(), "rdx".into()]);
            destination = Some("rax".into());
            writes.extend(["rax".to_string(), "rdx".into(), "rflags".into()]);
            if matches!(op, WideningOp::Div | WideningOp::Idiv) {
                PipelineInstructionClass::Divide
            } else {
                PipelineInstructionClass::Multiply
            }
        }
        DecodedOp::Push(reg) => {
            sources.extend([reg_name(reg).to_string(), "rsp".into()]);
            writes.push("rsp".into());
            PipelineInstructionClass::Store
        }
        DecodedOp::Pop(reg) => {
            sources.push("rsp".into());
            let name = reg_name(reg).to_string();
            destination = Some(name.clone());
            writes.extend([name, "rsp".into()]);
            PipelineInstructionClass::Load
        }
        DecodedOp::Jump(_) => {
            branch = true;
            PipelineInstructionClass::Jump
        }
        DecodedOp::JumpIf { .. } => {
            branch = true;
            sources.push("rflags".into());
            PipelineInstructionClass::Branch
        }
        DecodedOp::Call(_) => {
            branch = true;
            sources.push("rsp".into());
            writes.push("rsp".into());
            PipelineInstructionClass::Jump
        }
        DecodedOp::Ret => {
            branch = true;
            sources.push("rsp".into());
            writes.push("rsp".into());
            PipelineInstructionClass::Jump
        }
    };
    (class, destination, sources, writes, branch)
}

fn add_source(operand: Operand, sources: &mut Vec<String>) {
    match operand {
        Operand::Register(reg) => sources.push(reg_name(reg).to_string()),
        Operand::Memory(_) => add_memory_base(operand, sources),
    }
}

fn add_memory_base(operand: Operand, sources: &mut Vec<String>) {
    if let Operand::Memory(memory) = operand
        && let Some(base) = memory.base
    {
        sources.push(REG64[base as usize].to_string());
    }
}

fn add_write(operand: Operand, destination: &mut Option<String>, writes: &mut Vec<String>) {
    if let Operand::Register(reg) = operand {
        let name = reg_name(reg).to_string();
        *destination = Some(name.clone());
        writes.push(name);
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

pub(super) struct X86Pipeline {
    stages: [Option<PipelineInstruction>; STAGES],
    enabled: bool,
    halted: bool,
    faulted: bool,
    fetch_pc: u64,
    stats: PipelineStats,
    traces: Vec<Trace>,
    rows: Vec<Row>,
    next_row: u64,
    status: Option<String>,
}

impl X86Pipeline {
    pub(super) fn new(entry: u64) -> Self {
        Self {
            stages: std::array::from_fn(|_| None),
            enabled: false,
            halted: false,
            faulted: false,
            fetch_pc: entry,
            stats: PipelineStats::default(),
            traces: Vec::new(),
            rows: Vec::new(),
            next_row: 0,
            status: None,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn start_cycle(&mut self) -> Option<PipelineInstruction> {
        self.stats.cycles = self.stats.cycles.saturating_add(1);
        self.traces.clear();
        self.stages[RETIRE].take()
    }

    pub(super) fn retry(&mut self, instruction: PipelineInstruction) {
        self.stages[RETIRE] = Some(instruction);
        self.status = Some("awaiting input".into());
    }

    pub(super) fn retire(&mut self, instruction: &PipelineInstruction) {
        self.stats.committed = self.stats.committed.saturating_add(1);
        if instruction.branch {
            self.stats.branches = self.stats.branches.saturating_add(1);
        }
    }

    pub(super) fn halt(&mut self) {
        self.halted = true;
        self.stages.fill(None);
        self.status = Some("halted".into());
    }

    pub(super) fn fault(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.faulted = true;
        self.stages.fill(None);
        self.status = Some(message);
    }

    pub(super) fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        if self.halted || self.faulted {
            return None;
        }
        self.status = None;
        if let Some(target) = redirect {
            let flushed: Vec<u64> = self
                .stages
                .iter()
                .flatten()
                .map(|instruction| instruction.row)
                .collect();
            for row in &flushed {
                self.mark_flushed(*row);
            }
            self.stages.fill(None);
            self.fetch_pc = target;
            self.stats.flushes = self.stats.flushes.saturating_add(1);
            self.stats.stalls = self.stats.stalls.saturating_add(flushed.len() as u64);
            self.stats.branch_stalls = self
                .stats
                .branch_stalls
                .saturating_add(flushed.len() as u64);
            self.traces.push(Trace {
                kind: PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush),
                from: RETIRE,
                to: FETCH,
                detail: format!("redirect to 0x{target:X}"),
            });
        }

        for instruction in self.stages.iter_mut().flatten() {
            instruction.hazard = None;
        }
        for destination in (ISSUE + 1..STAGES).rev() {
            if self.stages[destination].is_none() {
                self.stages[destination] = self.stages[destination - 1].take();
            }
        }

        let dependency = self.stages[DECODE].as_ref().and_then(|consumer| {
            (ISSUE..STAGES).find_map(|stage| {
                let producer = self.stages[stage].as_ref()?;
                consumer
                    .sources
                    .iter()
                    .find(|source| producer.writes.contains(source))
                    .map(|source| (stage, source.clone()))
            })
        });
        if let Some((producer, register)) = dependency {
            self.stats.stalls = self.stats.stalls.saturating_add(1);
            self.stats.raw_stalls = self.stats.raw_stalls.saturating_add(1);
            if let Some(decoded) = self.stages[DECODE].as_mut() {
                decoded.hazard = Some(PipelineHazardKind::ReadAfterWrite);
            }
            self.traces.push(Trace {
                kind: PipelineTraceKind::Hazard(PipelineHazardKind::ReadAfterWrite),
                from: producer,
                to: DECODE,
                detail: format!("waiting for {register}"),
            });
        } else if self.stages[ISSUE].is_none() {
            self.stages[ISSUE] = self.stages[DECODE].take();
        }
        if self.stages[DECODE].is_none() {
            self.stages[DECODE] = self.stages[FETCH].take();
        }

        let empty = self.stages.iter().all(Option::is_none);
        let may_fetch = self.stages[FETCH].is_none() && (self.enabled || empty);
        self.record_timeline();
        may_fetch.then_some(self.fetch_pc)
    }

    pub(super) fn fetched(&mut self, mut instruction: PipelineInstruction) {
        instruction.row = self.next_row;
        instruction.speculative = self.stages.iter().flatten().any(|older| older.branch);
        self.next_row = self.next_row.saturating_add(1);
        self.fetch_pc = instruction.sequential_address();
        self.rows.push(Row {
            id: instruction.row,
            disassembly: instruction.decoded.text.clone(),
            class: instruction.class,
            first_cycle: self.stats.cycles,
            cells: vec![Cell {
                label: STAGE_NAMES[FETCH],
                state: if instruction.speculative {
                    PipelineTimelineState::Speculative
                } else {
                    PipelineTimelineState::Active
                },
                role: STAGE_ROLES[FETCH],
            }],
        });
        if self.rows.len() > 256 {
            self.rows.remove(0);
        }
        self.stages[FETCH] = Some(instruction);
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
                label: STAGE_NAMES[index],
                state: if instruction.hazard.is_some() {
                    PipelineTimelineState::Stalled
                } else if instruction.speculative {
                    PipelineTimelineState::Speculative
                } else {
                    PipelineTimelineState::Active
                },
                role: STAGE_ROLES[index],
            });
        }
    }
}

impl PipelineControl for X86Pipeline {
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
        self.next_row = 0;
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

impl PipelineInspect for X86Pipeline {
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
        STAGES
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        Some(PipelineStageView {
            name: STAGE_NAMES.get(index)?,
            role: *STAGE_ROLES.get(index)?,
            slot: self
                .stages
                .get(index)?
                .as_ref()
                .map(PipelineInstruction::view),
        })
    }

    fn edges(&self) -> Vec<PipelineEdge> {
        let mut edges: Vec<_> = (1..STAGES)
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        edges.push(PipelineEdge {
            from: RETIRE,
            to: FETCH,
            kind: PipelineEdgeKind::Feedback,
        });
        edges
    }

    fn unit_count(&self) -> usize {
        2
    }

    /// Both units retire whatever they hold in a single cycle — this model has
    /// no multi-cycle execute — so each declares a latency of one.
    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>> {
        match index {
            0 => Some(PipelineUnitView {
                name: "Integer ALU",
                capacity: 1,
                active: usize::from(self.stages[EXECUTE].is_some()),
                first: self.stages[EXECUTE].as_ref().map(PipelineInstruction::view),
                latency_class: PipelineInstructionClass::Alu,
                latency: Some(1),
            }),
            1 => Some(PipelineUnitView {
                name: "Load/Store",
                capacity: 1,
                active: usize::from(self.stages[MEMORY].as_ref().is_some_and(|instruction| {
                    matches!(
                        instruction.class,
                        PipelineInstructionClass::Load | PipelineInstructionClass::Store
                    )
                })),
                first: self.stages[MEMORY]
                    .as_ref()
                    .filter(|instruction| {
                        matches!(
                            instruction.class,
                            PipelineInstructionClass::Load | PipelineInstructionClass::Store
                        )
                    })
                    .map(PipelineInstruction::view),
                latency_class: PipelineInstructionClass::Load,
                latency: Some(1),
            }),
            _ => None,
        }
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
