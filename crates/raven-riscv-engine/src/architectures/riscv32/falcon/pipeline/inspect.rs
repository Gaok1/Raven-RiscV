//! Adapter from the RV32 pipeline simulator to the engine-level observer API.

use crate::capability::{
    PipelineHazardKind, PipelineInspect, PipelineInstructionClass, PipelineSlotView,
    PipelineStageView, PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceKind, PipelineTraceView, PipelineUnitView,
};

use super::{
    FuKind, GanttCell, HazardType, InstrClass, PipeSlot, PipelineSimState, Stage, TraceKind,
};

fn instruction_class(class: InstrClass) -> PipelineInstructionClass {
    match class {
        InstrClass::Alu => PipelineInstructionClass::Alu,
        InstrClass::Mul => PipelineInstructionClass::Multiply,
        InstrClass::Div => PipelineInstructionClass::Divide,
        InstrClass::Load => PipelineInstructionClass::Load,
        InstrClass::Store => PipelineInstructionClass::Store,
        InstrClass::Branch => PipelineInstructionClass::Branch,
        InstrClass::Jump => PipelineInstructionClass::Jump,
        InstrClass::System => PipelineInstructionClass::System,
        InstrClass::Fp => PipelineInstructionClass::FloatingPoint,
        InstrClass::Unknown => PipelineInstructionClass::Unknown,
    }
}

fn hazard_kind(hazard: HazardType) -> PipelineHazardKind {
    match hazard {
        HazardType::Raw => PipelineHazardKind::ReadAfterWrite,
        HazardType::LoadUse => PipelineHazardKind::LoadUse,
        HazardType::BranchFlush => PipelineHazardKind::BranchFlush,
        HazardType::FuBusy => PipelineHazardKind::FunctionalUnitBusy,
        HazardType::MemLatency => PipelineHazardKind::MemoryLatency,
        HazardType::Waw => PipelineHazardKind::WriteAfterWrite,
        HazardType::War => PipelineHazardKind::WriteAfterRead,
    }
}

fn is_atomic(slot: &PipeSlot) -> bool {
    use crate::falcon::instruction::Instruction;

    matches!(
        slot.instr,
        Some(
            Instruction::LrW { .. }
                | Instruction::ScW { .. }
                | Instruction::AmoswapW { .. }
                | Instruction::AmoaddW { .. }
                | Instruction::AmoxorW { .. }
                | Instruction::AmoandW { .. }
                | Instruction::AmoorW { .. }
                | Instruction::AmomaxW { .. }
                | Instruction::AmominW { .. }
                | Instruction::AmomaxuW { .. }
                | Instruction::AmominuW { .. }
        )
    )
}

fn slot_view(slot: &PipeSlot) -> PipelineSlotView<'_> {
    PipelineSlotView {
        address: u64::from(slot.pc),
        disassembly: &slot.disasm,
        class: instruction_class(slot.class),
        destination: slot.rd.map(super::sim::reg_name),
        sources: [slot.rs1.map(super::sim::reg_name), slot.rs2.map(super::sim::reg_name)],
        bubble: slot.is_bubble,
        speculative: slot.is_speculative,
        predicted_taken: slot.predicted_taken,
        hazard: slot.hazard.map(hazard_kind),
        atomic: is_atomic(slot),
        cycles_remaining: slot.fu_cycles_left,
    }
}

fn slot_belongs_to_unit(slot: &PipeSlot, unit: FuKind) -> bool {
    match unit {
        FuKind::Alu => matches!(slot.class, InstrClass::Alu | InstrClass::Branch | InstrClass::Jump),
        FuKind::Mul => slot.class == InstrClass::Mul,
        FuKind::Div => slot.class == InstrClass::Div,
        FuKind::Fpu => slot.class == InstrClass::Fp,
        FuKind::Lsu => matches!(slot.class, InstrClass::Load | InstrClass::Store),
        FuKind::Sys => slot.class == InstrClass::System,
    }
}

fn row_is_atomic(disassembly: &str) -> bool {
    [
        "lr.w", "sc.w", "amoswap.w", "amoadd.w", "amoxor.w", "amoand.w", "amoor.w",
        "amomax.w", "amomin.w", "amomaxu.w", "amominu.w",
    ]
    .iter()
    .any(|prefix| disassembly.starts_with(prefix))
}

impl PipelineInspect for PipelineSimState {
    fn status(&self) -> PipelineStatus {
        PipelineStatus {
            enabled: self.enabled,
            sequential: self.sequential_mode,
            halted: self.halted,
            faulted: self.faulted,
        }
    }

    fn stats(&self) -> PipelineStats {
        let [raw, load_use, branch, unit, memory] = self.stall_by_type;
        PipelineStats {
            cycles: self.cycle_count,
            committed: self.instr_committed,
            stalls: self.stall_count,
            raw_stalls: raw,
            load_use_stalls: load_use,
            branch_stalls: branch,
            functional_unit_stalls: unit,
            memory_stalls: memory,
            flushes: self.flush_count,
            branches: self.branches_executed,
        }
    }

    fn stage_count(&self) -> usize {
        self.stages.len()
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let name = Stage::all().get(index)?.label();
        Some(PipelineStageView {
            name,
            slot: self.stages[index].as_ref().map(slot_view),
        })
    }

    fn unit_count(&self) -> usize {
        FuKind::COUNT
    }

    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>> {
        let kind = *FuKind::all().get(index)?;
        let ex = self.stages[Stage::EX as usize].as_ref();
        let mut active = 0usize;
        let mut first = None;
        let mut mirrored = None;
        for state in &self.fu_bank[kind.index()] {
            let Some(slot) = state.slot.as_ref().filter(|slot| !slot.is_bubble) else {
                continue;
            };
            let is_mirrored = ex.is_some_and(|ex| {
                ex.seq != 0
                    && ex.seq == slot.seq
                    && ex.pc == slot.pc
                    && slot_belongs_to_unit(ex, kind)
            });
            if is_mirrored {
                mirrored = Some(slot);
            } else {
                active += 1;
                first.get_or_insert_with(|| slot_view(slot));
            }
        }
        if let Some(slot) = mirrored {
            active += 1;
            first.get_or_insert_with(|| slot_view(slot));
        }
        let latency_class = match kind {
            FuKind::Alu => first.map_or(PipelineInstructionClass::Alu, |slot| slot.class),
            FuKind::Mul => PipelineInstructionClass::Multiply,
            FuKind::Div => PipelineInstructionClass::Divide,
            FuKind::Fpu => PipelineInstructionClass::FloatingPoint,
            FuKind::Lsu => first.map_or(PipelineInstructionClass::Load, |slot| {
                if slot.class == PipelineInstructionClass::Store {
                    PipelineInstructionClass::Store
                } else {
                    PipelineInstructionClass::Load
                }
            }),
            FuKind::Sys => PipelineInstructionClass::System,
        };
        Some(PipelineUnitView {
            name: kind.label(),
            capacity: usize::from(self.fu_capacity[kind.index()].max(1)),
            active,
            first,
            latency_class,
        })
    }

    fn trace_count(&self) -> usize {
        self.hazard_traces.len()
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        let trace = self.hazard_traces.get(index)?;
        let kind = match trace.kind {
            TraceKind::Hazard(hazard) => PipelineTraceKind::Hazard(hazard_kind(hazard)),
            TraceKind::Forward => PipelineTraceKind::Forward,
        };
        let detail = if trace.detail.is_empty() {
            match trace.kind {
                TraceKind::Hazard(hazard) => self
                    .hazard_msgs
                    .iter()
                    .find(|(candidate, _)| *candidate == hazard)
                    .map_or("", |(_, message)| message.as_str()),
                TraceKind::Forward => "",
            }
        } else {
            trace.detail.as_str()
        };
        Some(PipelineTraceView {
            kind,
            from_stage: trace.from_stage,
            to_stage: trace.to_stage,
            detail,
        })
    }

    fn status_message(&self) -> Option<&str> {
        self.hazard_msgs.first().map(|(_, message)| message.as_str())
    }

    fn timeline_len(&self) -> usize {
        self.gantt.len()
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        let row = self.gantt.get(index)?;
        Some(PipelineTimelineRow {
            disassembly: &row.disasm,
            class: instruction_class(row.class),
            first_cycle: row.first_cycle,
            cells: row.cells.len(),
            atomic: row_is_atomic(&row.disasm),
        })
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        let cell = *self.gantt.get(row)?.cells.get(cell)?;
        let (label, state) = match cell {
            GanttCell::Empty => ("·", PipelineTimelineState::Empty),
            GanttCell::InStage(stage) => (stage.label(), PipelineTimelineState::Active),
            GanttCell::InFu(_) => ("EX", PipelineTimelineState::Active),
            GanttCell::Speculative(stage) => {
                (stage.label(), PipelineTimelineState::Speculative)
            }
            GanttCell::SpeculativeFu(_) => ("EX", PipelineTimelineState::Speculative),
            GanttCell::Stall => ("──", PipelineTimelineState::Stalled),
            GanttCell::Bubble => ("NOP", PipelineTimelineState::Bubble),
            GanttCell::Flush => ("◀FL", PipelineTimelineState::Flushed),
        };
        Some(PipelineTimelineCell { label, state })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::capability::{
        PipelineHazardKind, PipelineInspect, PipelineInstructionClass, PipelineTimelineState,
        PipelineTraceKind,
    };
    use crate::falcon::machine::{JournaledPipeline, NoPipeline};

    use super::super::{
        FuKind, FuState, GanttCell, GanttRow, HazardTrace, HazardType, InstrClass, PipeSlot,
        PipelineSimState, Stage, TraceKind,
    };

    #[test]
    fn fresh_pipeline_observer_has_a_consistent_shape() {
        let pipeline = PipelineSimState::new();
        let inspect: &dyn PipelineInspect = &pipeline;

        assert_eq!(inspect.stage_count(), 5);
        assert_eq!(
            (0..inspect.stage_count())
                .map(|index| inspect.stage(index).unwrap().name)
                .collect::<Vec<_>>(),
            ["IF", "ID", "EX", "MEM", "WB"]
        );
        assert_eq!(inspect.unit_count(), 6);
        assert!(inspect.status().enabled);
        assert_eq!(inspect.stats().cycles, 0);
        assert_eq!(inspect.trace_count(), 0);
        assert_eq!(inspect.timeline_len(), 0);
        assert!(inspect.stage(usize::MAX).is_none());
        assert!(inspect.unit(usize::MAX).is_none());
        assert!(inspect.trace(usize::MAX).is_none());
        assert!(inspect.timeline_row(usize::MAX).is_none());
    }

    #[test]
    fn runtime_observer_is_available_only_for_an_inspectable_pipeline() {
        let pipeline = PipelineSimState::new();

        assert!(JournaledPipeline::inspect(&pipeline).is_some());
        assert!(JournaledPipeline::inspect(&NoPipeline).is_none());
    }

    #[test]
    fn stage_views_widen_addresses_and_name_registers() {
        let mut pipeline = PipelineSimState::new();
        let mut slot = PipeSlot::from_word(0x1234, 0x0015_8513); // addi a0, a1, 1
        slot.hazard = Some(HazardType::Raw);
        pipeline.stages[Stage::ID as usize] = Some(slot);

        let stage = pipeline.stage(Stage::ID as usize).unwrap();
        let slot = stage.slot.unwrap();
        assert_eq!(slot.address, 0x1234u64);
        assert_eq!(slot.class, PipelineInstructionClass::Alu);
        assert_eq!(slot.destination, Some("a0"));
        assert_eq!(slot.sources[0], Some("a1"));
        assert_eq!(slot.hazard, Some(PipelineHazardKind::ReadAfterWrite));
    }

    #[test]
    fn observer_reads_do_not_change_pipeline_state() {
        let mut pipeline = PipelineSimState::new();
        pipeline.cycle_count = 7;
        pipeline.instr_committed = 3;
        pipeline.stages[0] = Some(PipeSlot::from_word(0, 0x0000_0013));
        let before = (
            pipeline.cycle_count,
            pipeline.instr_committed,
            pipeline.stages[0].as_ref().unwrap().seq,
            pipeline.gantt.len(),
        );

        let _ = pipeline.status();
        let _ = pipeline.stats();
        let _ = pipeline.stage(0);
        let _ = pipeline.unit(0);
        let _ = pipeline.trace(0);
        let _ = pipeline.timeline_row(0);

        assert_eq!(
            before,
            (
                pipeline.cycle_count,
                pipeline.instr_committed,
                pipeline.stages[0].as_ref().unwrap().seq,
                pipeline.gantt.len(),
            )
        );
    }

    #[test]
    fn statistics_keep_the_existing_counter_values() {
        let mut pipeline = PipelineSimState::new();
        pipeline.cycle_count = 20;
        pipeline.instr_committed = 5;
        pipeline.stall_count = 9;
        pipeline.stall_by_type = [1, 2, 3, 1, 2];
        pipeline.flush_count = 4;
        pipeline.branches_executed = 8;

        let stats = pipeline.stats();
        assert_eq!(stats.cpi(), 4.0);
        assert_eq!(stats.stalls, 9);
        assert_eq!(
            [
                stats.raw_stalls,
                stats.load_use_stalls,
                stats.branch_stalls,
                stats.functional_unit_stalls,
                stats.memory_stalls,
            ],
            [1, 2, 3, 1, 2]
        );
        assert_eq!((stats.flushes, stats.branches), (4, 8));
    }

    #[test]
    fn traces_resolve_fallback_messages_without_cloning_them() {
        let mut pipeline = PipelineSimState::new();
        pipeline
            .hazard_msgs
            .push((HazardType::LoadUse, "waiting for load".to_string()));
        pipeline.hazard_traces.push(HazardTrace {
            kind: TraceKind::Hazard(HazardType::LoadUse),
            from_stage: Stage::EX as usize,
            to_stage: Stage::ID as usize,
            detail: String::new(),
        });

        let trace = pipeline.trace(0).unwrap();
        assert_eq!(
            trace.kind,
            PipelineTraceKind::Hazard(PipelineHazardKind::LoadUse)
        );
        assert_eq!(trace.detail, "waiting for load");
        assert_eq!(pipeline.status_message(), Some("waiting for load"));
    }

    #[test]
    fn timeline_and_functional_units_preserve_visible_activity() {
        let mut pipeline = PipelineSimState::new();
        let mut slot = PipeSlot::from_word(0, 0x0200_0033); // mul x0, x0, x0
        slot.seq = 1;
        slot.fu_cycles_left = 2;
        pipeline.fu_bank[FuKind::Mul.index()].push(FuState {
            kind: Some(FuKind::Mul),
            slot: Some(slot),
            busy_cycles_left: 2,
        });
        pipeline.gantt.push_back(GanttRow {
            gantt_id: 1,
            pc: 0,
            disasm: "lr.w a0, (a1)".to_string(),
            class: InstrClass::Load,
            cells: VecDeque::from([
                GanttCell::InStage(Stage::IF),
                GanttCell::Speculative(Stage::ID),
                GanttCell::Stall,
                GanttCell::Flush,
            ]),
            first_cycle: 4,
            done: false,
            last_stage: None,
        });

        let unit = pipeline.unit(FuKind::Mul.index()).unwrap();
        assert_eq!((unit.name, unit.active, unit.first.unwrap().cycles_remaining), ("MUL", 1, 2));
        let row = pipeline.timeline_row(0).unwrap();
        assert!(row.atomic);
        assert_eq!((row.first_cycle, row.cells), (4, 4));
        assert_eq!(pipeline.timeline_cell(0, 0).unwrap().label, "IF");
        assert_eq!(
            pipeline.timeline_cell(0, 1).unwrap().state,
            PipelineTimelineState::Speculative
        );
        assert_eq!(
            pipeline.timeline_cell(0, 2).unwrap().state,
            PipelineTimelineState::Stalled
        );
        assert_eq!(
            pipeline.timeline_cell(0, 3).unwrap().state,
            PipelineTimelineState::Flushed
        );
    }
}
