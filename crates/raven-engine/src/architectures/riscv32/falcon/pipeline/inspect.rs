//! Adapter from the RV32 pipeline simulator to the engine-level observer API.

use crate::capability::{
    PipelineEdge, PipelineEdgeKind, PipelineInspect, PipelineSlotView, PipelineStageRole,
    PipelineStageView, PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceView, PipelineUnitView,
};

use super::{FuKind, GanttCell, PipeSlot, PipelineSimState, Stage, TraceKind};

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
        class: slot.class,
        destination: slot.rd.map(super::sim::reg_name),
        sources: [
            slot.rs1.map(super::sim::reg_name),
            slot.rs2.map(super::sim::reg_name),
        ],
        bubble: slot.is_bubble,
        speculative: slot.is_speculative,
        predicted_taken: slot.predicted_taken,
        hazard: slot.hazard,
        atomic: is_atomic(slot),
        cycles_remaining: slot.fu_cycles_left,
    }
}

fn row_is_atomic(disassembly: &str) -> bool {
    [
        "lr.w",
        "sc.w",
        "amoswap.w",
        "amoadd.w",
        "amoxor.w",
        "amoand.w",
        "amoor.w",
        "amomax.w",
        "amomin.w",
        "amomaxu.w",
        "amominu.w",
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
            // This datapath retires in order, so a name hazard never costs it
            // a cycle. Only a dynamically scheduled model can report these.
            waw_stalls: 0,
            war_stalls: 0,
            flushes: self.flush_count,
            branches: self.branches_executed,
        }
    }

    fn stage_count(&self) -> usize {
        self.stages.len()
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let stage = *Stage::all().get(index)?;
        Some(PipelineStageView {
            name: stage.label(),
            slot: self.stages[index].as_ref().map(slot_view),
            role: stage.role(),
        })
    }

    /// The classic five-stage chain, plus the redirect from EX back to IF that
    /// a resolved branch takes — the default linear chain alone would not show
    /// why a flush happens.
    fn edges(&self) -> Vec<PipelineEdge> {
        let mut edges: Vec<_> = (1..self.stages.len())
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        edges.push(PipelineEdge {
            from: Stage::EX as usize,
            to: Stage::IF as usize,
            kind: PipelineEdgeKind::Feedback,
        });
        edges
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
                    && kind.spec().handles(ex.class)
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
        Some(PipelineUnitView {
            name: kind.label(),
            capacity: usize::from(self.fu_capacity[kind.index()].max(1)),
            active,
            first,
            // Idle, the unit shows the work it exists for; busy, it shows what
            // it is actually running — a store in the LSU rather than a load.
            latency_class: first.map_or(kind.spec().latency_class, |slot| slot.class),
        })
    }

    fn trace_count(&self) -> usize {
        self.hazard_traces.len()
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        let trace = self.hazard_traces.get(index)?;
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
            kind: trace.kind,
            from_stage: trace.from_stage,
            to_stage: trace.to_stage,
            detail,
        })
    }

    fn status_message(&self) -> Option<&str> {
        self.hazard_msgs
            .first()
            .map(|(_, message)| message.as_str())
    }

    fn timeline_len(&self) -> usize {
        self.gantt.len()
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        let row = self.gantt.get(index)?;
        Some(PipelineTimelineRow {
            disassembly: &row.disasm,
            class: row.class,
            first_cycle: row.first_cycle,
            cells: row.cells.len(),
            atomic: row_is_atomic(&row.disasm),
        })
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        let cell = *self.gantt.get(row)?.cells.get(cell)?;
        let (label, state, role) = match cell {
            GanttCell::Empty => ("·", PipelineTimelineState::Empty, PipelineStageRole::Other),
            GanttCell::InStage(stage) => (
                stage.label(),
                PipelineTimelineState::Active,
                stage.role(),
            ),
            GanttCell::InFu(_) => (
                "EX",
                PipelineTimelineState::Active,
                PipelineStageRole::Execute,
            ),
            GanttCell::Speculative(stage) => (
                stage.label(),
                PipelineTimelineState::Speculative,
                stage.role(),
            ),
            GanttCell::SpeculativeFu(_) => (
                "EX",
                PipelineTimelineState::Speculative,
                PipelineStageRole::Execute,
            ),
            GanttCell::Stall => (
                "──",
                PipelineTimelineState::Stalled,
                PipelineStageRole::Other,
            ),
            GanttCell::Bubble => (
                "NOP",
                PipelineTimelineState::Bubble,
                PipelineStageRole::Other,
            ),
            GanttCell::Flush => (
                "◀FL",
                PipelineTimelineState::Flushed,
                PipelineStageRole::Other,
            ),
        };
        Some(PipelineTimelineCell { label, state, role })
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
        slot.hazard = Some(HazardType::ReadAfterWrite);
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
        assert_eq!(
            (unit.name, unit.active, unit.first.unwrap().cycles_remaining),
            ("MUL", 1, 2)
        );
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
