//! The package's own behaviour, exercised through a two-instruction fake ISA.
//!
//! Nothing here mentions a real architecture: if one of these needs an ISA to
//! make sense, the shape declaration is not carrying its weight.

use crate::capability::{
    PipelineControl, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineSettingValue, PipelineStageRole, PipelineTimelineState,
    PipelineTraceKind, PipelineTuning,
};

use super::{
    BranchPredict, PipelineBypassConfig, PipelineEngine, PipelineMode, PipelineOp, PipelineShape,
    PipelineTiming, RISC_FIVE_STAGE, ScalarPipeline, StageSpec, UnitSpec,
};

static THREE_STAGE: [StageSpec; 3] = [
    StageSpec::new("IF", PipelineStageRole::Fetch),
    StageSpec::new("ID", PipelineStageRole::Decode),
    StageSpec::new("EX", PipelineStageRole::Execute),
];

static UNITS: [UnitSpec; 3] = [
    UnitSpec::new(
        "ALU",
        &[PipelineInstructionClass::Alu],
        PipelineInstructionClass::Alu,
    ),
    UnitSpec::new(
        "Load/Store",
        &[
            PipelineInstructionClass::Load,
            PipelineInstructionClass::Store,
        ],
        PipelineInstructionClass::Load,
    ),
    UnitSpec::new(
        "DIV",
        &[PipelineInstructionClass::Divide],
        PipelineInstructionClass::Divide,
    ),
];

fn shape() -> PipelineShape {
    PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
}

fn op(address: u64, class: PipelineInstructionClass) -> PipelineOp<()> {
    PipelineOp::new(address, format!("op@{address:x}"), class, ())
}

fn alu(address: u64, writes: &str, reads: &[&str]) -> PipelineOp<()> {
    op(address, PipelineInstructionClass::Alu)
        .writing(writes)
        .reading(reads.iter().map(|name| name.to_string()))
}

/// Drive the pipeline the way a backend does, retiring whatever comes out and
/// refilling the front end from `program`.
fn run(pipeline: &mut ScalarPipeline<()>, program: &[PipelineOp<()>], cycles: usize) {
    PipelineControl::set_enabled(pipeline, true);
    for _ in 0..cycles {
        if let Some(retired) = pipeline.start_cycle() {
            pipeline.retire(&retired);
        }
        if let Some(address) = pipeline.advance(None) {
            if let Some(next) = program.iter().find(|op| op.address == address) {
                pipeline.fetched(next.clone());
            }
        }
    }
}

#[test]
fn a_declared_shape_is_immediately_inspectable() {
    let pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    let inspect: &dyn PipelineInspect = &pipeline;

    assert_eq!(inspect.stage_count(), 5);
    assert_eq!(
        (0..inspect.stage_count())
            .map(|index| inspect.stage(index).unwrap().name)
            .collect::<Vec<_>>(),
        ["IF", "ID", "EX", "MEM", "WB"]
    );
    assert_eq!(inspect.stage_order(), [0, 1, 2, 3, 4]);
    assert_eq!(inspect.stats().cycles, 0);
    assert!(inspect.stage(usize::MAX).is_none());
}

#[test]
fn forwarding_leaves_only_the_load_use_stall() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    let program = [
        op(0, PipelineInstructionClass::Load).writing("a"),
        alu(4, "b", &["a"]),
        alu(8, "c", &["b"]),
    ];
    run(&mut pipeline, &program, 12);

    let stats = pipeline.stats();
    assert_eq!(stats.load_use_stalls, 1, "the load feeding an ALU stalls");
    assert_eq!(stats.raw_stalls, 0, "every other operand is bypassed");
    assert_eq!(stats.committed, 3);
}

#[test]
fn a_datapath_without_bypasses_stalls_on_every_dependency() {
    let mut pipeline: ScalarPipeline<()> =
        ScalarPipeline::new(shape().with_bypass(PipelineBypassConfig::disabled()), 0);
    let program = [alu(0, "a", &[]), alu(4, "b", &["a"])];
    run(&mut pipeline, &program, 14);

    let stats = pipeline.stats();
    assert!(stats.raw_stalls >= 2, "waits for the producer to write back");
    assert_eq!(stats.load_use_stalls, 0, "there is no bypass to fall short");
    assert_eq!(stats.committed, 2);
}

#[test]
fn a_short_datapath_resolves_its_own_load_stage() {
    // Three stages, so execute is last and a load has nowhere later to land.
    let mut pipeline: ScalarPipeline<()> =
        ScalarPipeline::new(PipelineShape::scalar(&THREE_STAGE, Some(1)), 0);
    let program = [
        op(0, PipelineInstructionClass::Load).writing("a"),
        alu(1, "b", &["a"]),
    ];
    run(&mut pipeline, &program, 10);

    assert_eq!(pipeline.stats().committed, 2);
    let forward_edges = PipelineInspect::edges(&pipeline)
        .iter()
        .filter(|edge| edge.kind == PipelineEdgeKind::Forward)
        .count();
    assert_eq!(forward_edges, 0, "nothing downstream of execute to bypass");
}

#[test]
fn a_multi_cycle_class_holds_execute_and_stalls_behind_it() {
    let timing = PipelineTiming {
        mul: 2,
        ..PipelineTiming::single_cycle()
    };
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape().with_timing(timing), 0);
    let program = [
        op(0, PipelineInstructionClass::Multiply).writing("a"),
        alu(4, "b", &[]),
    ];
    run(&mut pipeline, &program, 12);

    let stats = pipeline.stats();
    assert_eq!(stats.functional_unit_stalls, 2, "1 + 2 extra cycles");
    assert_eq!(stats.committed, 2);
}

#[test]
fn a_predicted_branch_redirects_the_front_end_before_it_resolves() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        shape().with_predict(BranchPredict::Taken),
        0,
    );
    PipelineControl::set_enabled(&mut pipeline, true);
    pipeline.advance(None);
    pipeline.fetched(op(0, PipelineInstructionClass::Branch).branching_to(Some(0x40)));

    assert_eq!(pipeline.fetch_pc(), 0x40, "speculated down the taken path");
    let taken = pipeline.stage_op(0).unwrap();
    assert!(taken.was_predicted_taken());
    assert_eq!(taken.predicted_next(), 0x40);
}

#[test]
fn resolving_a_branch_only_redirects_when_the_guess_was_wrong() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        shape().with_predict(BranchPredict::NotTaken),
        0,
    );
    PipelineControl::set_enabled(&mut pipeline, true);
    pipeline.advance(None);
    pipeline.fetched(op(0, PipelineInstructionClass::Branch).branching_to(Some(0x40)));
    let branch = pipeline.stage_op(0).cloned().unwrap();

    assert_eq!(pipeline.resolve(&branch, 4), None, "not-taken, as predicted");
    assert_eq!(pipeline.resolve(&branch, 0x40), Some(0x40), "mispredicted");
}

#[test]
fn a_flush_marks_the_discarded_rows_and_counts_the_bubbles() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    let program = [alu(0, "a", &[]), alu(4, "b", &[]), alu(8, "c", &[])];
    run(&mut pipeline, &program, 3);
    let in_flight = (0..pipeline.stage_count())
        .filter(|index| PipelineInspect::stage(&pipeline, *index).unwrap().slot.is_some())
        .count();

    pipeline.advance(Some(0x100));

    let stats = pipeline.stats();
    assert_eq!(stats.flushes, 1);
    assert_eq!(stats.branch_stalls, in_flight as u64);
    assert_eq!(pipeline.fetch_pc(), 0x100);
    let flushed: usize = (0..pipeline.timeline_len())
        .map(|row| {
            let cells = pipeline.timeline_row(row).unwrap().cells;
            (0..cells)
                .filter_map(|cell| pipeline.timeline_cell(row, cell))
                .filter(|cell| cell.state == PipelineTimelineState::Flushed)
                .count()
        })
        .sum();
    assert_eq!(flushed, in_flight);
}

#[test]
fn instructions_behind_an_unresolved_branch_are_speculative() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    let program = [
        op(0, PipelineInstructionClass::Branch).branching(),
        alu(4, "b", &[]),
    ];
    run(&mut pipeline, &program, 2);

    let speculative = (0..pipeline.stage_count())
        .filter_map(|index| PipelineInspect::stage(&pipeline, index).unwrap().slot)
        .filter(|slot| slot.speculative)
        .count();
    assert_eq!(speculative, 1, "only the instruction behind the branch");
}

#[test]
fn declared_units_report_what_execute_is_working_on() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape().with_units(&UNITS), 0);
    let program = [op(0, PipelineInstructionClass::Load).writing("a")];
    run(&mut pipeline, &program, 3);

    assert_eq!(pipeline.unit_count(), 3);
    let alu_unit = pipeline.unit(0).unwrap();
    let lsu = pipeline.unit(1).unwrap();
    assert_eq!((alu_unit.name, alu_unit.active), ("ALU", 0));
    assert_eq!((lsu.name, lsu.active), ("Load/Store", 1));
}

#[test]
fn a_name_dependency_is_reported_without_stalling() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    // Four cycles puts the younger writer in decode with the older still ahead.
    let program = [alu(0, "a", &[]), alu(4, "a", &[])];
    run(&mut pipeline, &program, 4);

    let name_hazards = (0..pipeline.trace_count())
        .filter_map(|index| pipeline.trace(index))
        .filter(|trace| {
            matches!(
                trace.kind,
                PipelineTraceKind::Hazard(
                    PipelineHazardKind::WriteAfterWrite | PipelineHazardKind::WriteAfterRead
                )
            )
        })
        .count();
    assert_eq!(name_hazards, 1);
    assert_eq!(pipeline.stats().stalls, 0, "in order, so nothing waits");
}

/// Turning the unit bank on must never cost anything.
///
/// Dispatch is free, so a datapath whose work all takes one cycle runs
/// identically either way — which is what makes the bank safe to switch on for
/// a backend that has no long-running instructions yet.
#[test]
fn dispatching_into_units_does_not_slow_single_cycle_work() {
    let program = [
        alu(0, "a", &[]),
        op(4, PipelineInstructionClass::Load).writing("b"),
        alu(8, "c", &["b"]),
        op(12, PipelineInstructionClass::Store).reading(["c".to_string()]),
    ];
    let mut serial: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    let mut parallel: ScalarPipeline<()> =
        ScalarPipeline::new(shape().with_parallel_units(&UNITS), 0);
    run(&mut serial, &program, 20);
    run(&mut parallel, &program, 20);

    assert_eq!(
        (parallel.stats().cycles, parallel.stats().committed),
        (serial.stats().cycles, serial.stats().committed)
    );
    assert_eq!(parallel.stats().load_use_stalls, serial.stats().load_use_stalls);
}

/// And the utilization strip still reports the work, even when it is fast
/// enough never to wait in the bank.
#[test]
fn a_single_cycle_unit_still_reports_the_work_it_is_doing() {
    let mut pipeline: ScalarPipeline<()> =
        ScalarPipeline::new(shape().with_parallel_units(&UNITS), 0);
    let program = [op(0, PipelineInstructionClass::Load).writing("a")];
    run(&mut pipeline, &program, 3);

    assert_eq!(pipeline.unit(1).unwrap().active, 1, "the LSU has the load");
    assert_eq!(pipeline.unit(0).unwrap().active, 0, "the ALU has nothing");
}

#[test]
fn parallel_units_let_a_short_op_execute_beside_a_long_one() {
    let timing = PipelineTiming {
        div: 4,
        ..PipelineTiming::single_cycle()
    };
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
            .with_parallel_units(&UNITS)
            .with_timing(timing),
        0,
    );
    let program = [
        op(0, PipelineInstructionClass::Divide).writing("a"),
        alu(4, "b", &[]),
    ];
    // Five cycles puts the divide in its unit with the ALU op dispatched behind
    // it — in a serialized datapath the add would still be waiting in decode.
    run(&mut pipeline, &program, 5);

    let divider = pipeline.unit(2).unwrap();
    let alu_unit = pipeline.unit(0).unwrap();
    assert_eq!(divider.name, "DIV");
    assert_eq!(
        (divider.active, alu_unit.active),
        (1, 1),
        "both units hold work at the same time"
    );
}

#[test]
fn a_finished_unit_result_waits_for_the_older_one() {
    let timing = PipelineTiming {
        div: 5,
        ..PipelineTiming::single_cycle()
    };
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
            .with_parallel_units(&UNITS)
            .with_timing(timing),
        0,
    );
    let program = [
        op(0, PipelineInstructionClass::Divide).writing("a"),
        alu(4, "b", &[]),
    ];
    run(&mut pipeline, &program, 8);

    assert_eq!(pipeline.unit(0).unwrap().active, 1, "the add is done…");
    assert_eq!(
        pipeline.stats().committed,
        0,
        "…but it may not retire ahead of the divide it followed"
    );
    run(&mut pipeline, &program, 10);
    assert_eq!(pipeline.stats().committed, 2, "both retire, in order");
}

#[test]
fn a_full_unit_stalls_the_instruction_behind_it() {
    let timing = PipelineTiming {
        div: 6,
        ..PipelineTiming::single_cycle()
    };
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
            .with_parallel_units(&UNITS)
            .with_timing(timing),
        0,
    );
    let program = [
        op(0, PipelineInstructionClass::Divide).writing("a"),
        op(4, PipelineInstructionClass::Divide).writing("b"),
    ];
    run(&mut pipeline, &program, 6);

    assert!(
        pipeline.stats().functional_unit_stalls > 0,
        "the second divide finds the one divider busy"
    );
    assert_eq!(pipeline.unit(2).unwrap().active, 1, "capacity is one");
}

#[test]
fn a_row_that_never_finishes_does_not_grow_the_history_forever() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    PipelineControl::set_enabled(&mut pipeline, true);
    pipeline.advance(None);
    pipeline.fetched(alu(0, "a", &[]));
    // A machine parked on input re-records the same instruction every cycle.
    for _ in 0..500 {
        if let Some(op) = pipeline.start_cycle() {
            pipeline.retry(op, "awaiting input");
        }
        pipeline.advance(None);
    }

    let cells = pipeline.timeline_row(0).unwrap().cells;
    assert!(cells <= 200, "history is bounded, got {cells} cells");
    assert!(cells > 0, "and still holds the recent cycles");
}

/// The settings screen is part of the package too: a shape is not just
/// something a host can look at, it is something a user can change.
#[test]
fn a_declared_shape_is_adjustable_through_the_capability() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape().with_units(&UNITS), 0);
    let tuning: &dyn PipelineTuning = &pipeline;
    assert_eq!(
        tuning.setting_count(),
        7 + UNITS.len() + PipelineTiming::field_names().len()
    );
    assert!(
        (0..tuning.setting_count()).all(|index| tuning.setting(index).is_some()),
        "every counted row can be described"
    );
    assert!(tuning.setting(usize::MAX).is_none());

    // The first row is the model, for every engine, so a host's cursor is on
    // the same question whichever one is running.
    assert_eq!(pipeline.setting(0).unwrap().name, "Execution");

    // Turning a bypass off is the experiment the screen exists for, and it has
    // to reach the model, not just the display.
    let ex_to_ex = 3;
    assert_eq!(
        pipeline.setting(ex_to_ex).unwrap().value,
        PipelineSettingValue::Toggle(true)
    );
    assert!(pipeline.adjust(ex_to_ex, true));
    assert_eq!(
        pipeline.setting(ex_to_ex).unwrap().value,
        PipelineSettingValue::Toggle(false)
    );
    assert!(!pipeline.shape().bypass.ex_to_ex);
}

#[test]
fn adjusting_a_unit_changes_how_much_work_it_takes() {
    let timing = PipelineTiming {
        div: 6,
        ..PipelineTiming::single_cycle()
    };
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(
        PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
            .with_parallel_units(&UNITS)
            .with_timing(timing),
        0,
    );
    // The divider is the third declared unit, so its row follows the seven
    // fixed ones.
    let divider = 7 + 2;
    assert_eq!(
        pipeline.setting(divider).unwrap().value,
        PipelineSettingValue::Number(1)
    );
    assert!(pipeline.set_number(divider, 2));
    assert_eq!(pipeline.unit(2).unwrap().capacity, 2);

    let program = [
        op(0, PipelineInstructionClass::Divide).writing("a"),
        op(4, PipelineInstructionClass::Divide).writing("b"),
    ];
    run(&mut pipeline, &program, 6);
    assert_eq!(
        pipeline.stats().functional_unit_stalls,
        0,
        "with two dividers the second divide no longer waits"
    );
}

// ── Choosing the engine ──────────────────────────────────────────────────────
//
// The models are three engines, and a user picks between them from one row of
// one screen. What that costs — and what it must not cost — is what these
// check.

/// A machine with something worth watching in it: a divider that takes long
/// enough for the models to disagree about what happens meanwhile.
fn engine(mode: PipelineMode) -> PipelineEngine<()> {
    let timing = PipelineTiming {
        div: 5,
        ..PipelineTiming::single_cycle()
    };
    let mut shape = shape().with_units(&UNITS).with_timing(timing);
    shape.mode = mode;
    PipelineEngine::new(shape, 0)
}

/// Six independent instructions, so nothing in these tests turns on a hazard.
fn independent() -> Vec<PipelineOp<()>> {
    ["p", "q", "r", "s", "t", "u"]
        .iter()
        .enumerate()
        .map(|(index, name)| alu(index as u64 * 4, name, &[]))
        .collect()
}

/// Drive whichever engine it is exactly as a backend does, reporting what
/// reached the machine.
fn drive(engine: &mut PipelineEngine<()>, program: &[PipelineOp<()>], cycles: usize) -> Vec<u64> {
    let mut retired = Vec::new();
    PipelineControl::set_enabled(engine, true);
    for _ in 0..cycles {
        let mut redirect = None;
        if let Some(op) = engine.start_cycle() {
            engine.retire(&op);
            retired.push(op.address);
            redirect = engine.resolve(&op, op.address + 4);
        }
        if let Some(address) = engine.advance(redirect) {
            if let Some(next) = program.iter().find(|op| op.address == address) {
                engine.fetched(next.clone());
            }
        }
    }
    retired
}

fn row_names(engine: &PipelineEngine<()>) -> Vec<&str> {
    (0..engine.setting_count())
        .filter_map(|index| engine.setting(index))
        .map(|setting| setting.name)
        .collect()
}

#[test]
fn the_declared_mode_picks_the_engine() {
    for (mode, phases) in [
        (PipelineMode::SingleCycle, ["IF", "ID", "EX", "MEM", "WB"]),
        (
            PipelineMode::FunctionalUnits,
            ["IF", "ID", "EX", "MEM", "WB"],
        ),
        (PipelineMode::Scoreboard, ["IF", "IS", "RO", "EX", "WR"]),
        (PipelineMode::TomasuloRob, ["IF", "IS", "EX", "WB", "CM"]),
    ] {
        let engine = engine(mode);
        assert_eq!(engine.mode(), mode);
        assert_eq!(
            (0..engine.stage_count())
                .filter_map(|index| engine.stage(index))
                .map(|stage| stage.name)
                .collect::<Vec<_>>(),
            phases,
            "{mode:?} reports its own phases, not the declared datapath's"
        );
    }
}

/// The claim that makes swapping engines mid-run legitimate: an instruction
/// that had not executed is refetched, and one that had is not run twice.
#[test]
fn changing_the_model_mid_run_loses_no_instruction_and_repeats_none() {
    let program = independent();
    let mut engine = engine(PipelineMode::SingleCycle);
    let mut retired = drive(&mut engine, &program, 8);
    assert!(
        !retired.is_empty() && retired.len() < program.len(),
        "the switch has to land mid-program to prove anything: {retired:?}"
    );

    assert!(engine.set_mode(PipelineMode::TomasuloRob));
    retired.extend(drive(&mut engine, &program, 40));

    assert_eq!(
        retired,
        program.iter().map(|op| op.address).collect::<Vec<_>>(),
        "every instruction once, in order, across two engines"
    );
}

/// Reconfiguring a machine keeps its numbers; replacing it cannot.
#[test]
fn only_a_new_engine_starts_the_counters_over() {
    let mut engine = engine(PipelineMode::SingleCycle);
    drive(&mut engine, &independent(), 8);
    let cycles = engine.stats().cycles;
    assert!(cycles > 0);

    assert!(engine.set_mode(PipelineMode::FunctionalUnits));
    assert_eq!(
        engine.stats().cycles,
        cycles,
        "the same engine, adjusted — as much as turning a bypass off is"
    );
    assert!(engine.timeline_len() > 0);

    assert!(engine.set_mode(PipelineMode::Scoreboard));
    assert_eq!(
        engine.stats().cycles,
        0,
        "another model's cycles would describe neither machine"
    );
    assert_eq!(engine.timeline_len(), 0);
}

#[test]
fn the_machine_a_user_built_survives_a_model_change() {
    let mut engine = engine(PipelineMode::FunctionalUnits);
    assert!(engine.set_capacity(2, 3), "three dividers");
    assert!(engine.set_number(7 + UNITS.len() + 2, 9), "a slower divide");

    assert!(engine.set_mode(PipelineMode::Scoreboard));
    assert_eq!(engine.unit(2).unwrap().capacity, 3);
    assert_eq!(engine.shape().timing.div, 9);
}

#[test]
fn the_model_row_is_the_first_row_of_every_engine() {
    let mut engine = engine(PipelineMode::SingleCycle);
    for expected in [
        PipelineMode::FunctionalUnits,
        PipelineMode::Scoreboard,
        PipelineMode::TomasuloRob,
        PipelineMode::SingleCycle,
    ] {
        assert!(engine.adjust(0, true));
        assert_eq!(engine.mode(), expected);
        let row = engine.setting(0).unwrap();
        assert_eq!(row.name, "Execution", "the cursor has not moved off it");
        assert_eq!(
            row.value,
            PipelineSettingValue::Choice(expected.label()),
            "and it says which model is running"
        );
    }
}

/// A row a model cannot honour is a row it does not offer — a control that does
/// nothing would be worse than no control at all.
#[test]
fn each_model_offers_only_the_rows_it_can_honour() {
    let scoreboard = engine(PipelineMode::Scoreboard);
    assert!(
        (0..scoreboard.setting_count())
            .filter_map(|index| scoreboard.setting(index))
            .all(|row| row.group != "FORWARDING"),
        "a scoreboard has no bypass network to wire"
    );
    assert!(
        !row_names(&scoreboard).contains(&"Branch predict"),
        "and nothing to predict with: it does not speculate"
    );

    let tomasulo = engine(PipelineMode::TomasuloRob);
    let rows = row_names(&tomasulo);
    assert!(rows.contains(&"Branch predict"), "this one does speculate");
    assert!(rows.contains(&"Entries"), "and has a buffer to size");
    assert!(
        !rows.contains(&"Branch resolve"),
        "but nowhere to move: a mispredict is found at commit"
    );
}

#[test]
fn every_row_of_every_model_reads_and_writes() {
    for mode in PipelineMode::all() {
        let mut engine = engine(mode);
        assert!(
            (0..engine.setting_count()).all(|index| engine.setting(index).is_some()),
            "{mode:?}: every counted row can be described"
        );
        assert!(engine.setting(usize::MAX).is_none());

        for index in 1..engine.setting_count() {
            engine.adjust(index, true);
        }
        assert_eq!(
            engine.mode(),
            mode,
            "{mode:?}: no row but the first changes which engine is running"
        );
    }
}

#[test]
fn a_stopped_machine_does_not_restart_when_the_model_changes() {
    let mut engine = engine(PipelineMode::SingleCycle);
    drive(&mut engine, &independent(), 8);
    engine.halt();

    assert!(engine.set_mode(PipelineMode::TomasuloRob));
    assert!(
        engine.status().halted,
        "a new engine is not a reason for a stopped program to run again"
    );
    assert!(drive(&mut engine, &independent(), 8).is_empty());
}

#[test]
fn reset_clears_history_and_redirect_keeps_it() {
    let mut pipeline: ScalarPipeline<()> = ScalarPipeline::new(shape(), 0);
    run(&mut pipeline, &[alu(0, "a", &[])], 4);
    assert!(pipeline.timeline_len() > 0);

    PipelineControl::redirect(&mut pipeline, 0x20);
    assert_eq!(pipeline.fetch_pc(), 0x20);
    assert!(pipeline.timeline_len() > 0, "a jump is not a new run");

    PipelineControl::reset(&mut pipeline, 0);
    assert_eq!(pipeline.timeline_len(), 0);
    assert_eq!(pipeline.stats().cycles, 0);
}
