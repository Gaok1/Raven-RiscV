//! The dynamically scheduled models, exercised through a fake ISA.
//!
//! Same rule as the package's own tests: nothing here names a real
//! architecture. A scoreboard that needs an ISA to demonstrate a WAR hazard is
//! a scoreboard that has not been separated from one.

use crate::capability::{
    PipelineControl, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineStageRole, PipelineTimelineState, PipelineTraceKind,
};

use super::super::{
    PipelineMode, PipelineOp, PipelineShape, PipelineTiming, RISC_FIVE_STAGE, UnitSpec,
};
use super::ScoreboardPipeline;

static UNITS: [UnitSpec; 4] = [
    UnitSpec::new(
        "ALU",
        &[
            PipelineInstructionClass::Alu,
            PipelineInstructionClass::Branch,
        ],
        PipelineInstructionClass::Alu,
    ),
    UnitSpec::new(
        "DIV",
        &[PipelineInstructionClass::Divide],
        PipelineInstructionClass::Divide,
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
        "System",
        &[PipelineInstructionClass::System],
        PipelineInstructionClass::System,
    ),
];

/// Long enough that a divide is unmistakably still running while shorter work
/// beside it finishes.
fn timing() -> PipelineTiming {
    PipelineTiming {
        div: 5,
        ..PipelineTiming::single_cycle()
    }
}

fn scoreboard() -> ScoreboardPipeline<()> {
    ScoreboardPipeline::new(
        PipelineShape::scalar(&RISC_FIVE_STAGE, Some(4))
            .with_units(&UNITS)
            .with_timing(timing()),
        0,
    )
}

fn op(address: u64, class: PipelineInstructionClass) -> PipelineOp<()> {
    PipelineOp::new(address, format!("i{address} r"), class, ())
}

fn alu(address: u64, writes: &str, reads: &[&str]) -> PipelineOp<()> {
    op(address, PipelineInstructionClass::Alu)
        .writing(writes)
        .reading(reads.iter().map(|name| name.to_string()))
}

fn divide(address: u64, writes: &str, reads: &[&str]) -> PipelineOp<()> {
    op(address, PipelineInstructionClass::Divide)
        .writing(writes)
        .reading(reads.iter().map(|name| name.to_string()))
}

/// Drive the model the way a backend does, reporting the addresses that reached
/// the register file in the order they got there — which in this model is not
/// the order they were fetched in, and is most of what there is to check.
///
/// Branches are taken when they carry a target, which is all the control flow
/// these tests need.
fn run(board: &mut ScoreboardPipeline<()>, program: &[PipelineOp<()>], cycles: usize) -> Vec<u64> {
    let mut written = Vec::new();
    PipelineControl::set_enabled(board, true);
    for _ in 0..cycles {
        let mut redirect = None;
        if let Some(retired) = board.start_cycle() {
            board.retire(&retired);
            written.push(retired.address);
            let next = retired
                .branch_target
                .filter(|_| retired.branch)
                .unwrap_or(retired.address + 4);
            redirect = board.resolve(&retired, next);
        }
        if let Some(address) = board.advance(redirect) {
            if let Some(next) = program.iter().find(|op| op.address == address) {
                board.fetched(next.clone());
            }
        }
    }
    written
}

fn traces(board: &ScoreboardPipeline<()>, kind: PipelineHazardKind) -> usize {
    (0..board.trace_count())
        .filter_map(|index| board.trace(index))
        .filter(|trace| trace.kind == PipelineTraceKind::Hazard(kind))
        .count()
}

/// The whole reason the model exists: an instruction that is ready runs while
/// an older one that is not still waits, and reaches the register file first.
#[test]
fn independent_work_overtakes_a_long_operation() {
    let mut board = scoreboard();
    let program = [
        divide(0, "q", &["a", "b"]),
        alu(4, "s", &["c", "d"]),
        alu(8, "t", &["s"]),
    ];
    let order = run(&mut board, &program, 20);

    assert_eq!(
        order,
        [4, 0, 8],
        "the add depends on nothing and does not wait its turn"
    );
    assert_eq!(board.stats().committed, 3);
}

#[test]
fn a_second_writer_of_a_live_register_waits_at_issue() {
    let mut board = scoreboard();
    // Nothing renames, so `q` has room for one writer in flight.
    let program = [divide(0, "q", &["a"]), alu(4, "q", &["b"])];
    run(&mut board, &program, 8);

    assert!(board.stats().waw_stalls > 0, "the add cannot claim q");
    assert_eq!(board.stats().committed, 0, "and so neither has written");
    assert_eq!(traces(&board, PipelineHazardKind::WriteAfterWrite), 1);

    run(&mut board, &program, 10);
    assert_eq!(board.stats().committed, 2, "in the order they were written");
}

#[test]
fn a_result_waits_for_an_older_instruction_to_read_the_register() {
    let mut board = scoreboard();
    // Two adders, so the third instruction is held back by the hazard and not
    // by the unit the second one is sitting in.
    assert!(board.set_capacity(0, 2));
    // The second add cannot read `d` until the divide has produced `q`, so the
    // third add's write of `d` has to wait however ready it is.
    let program = [
        divide(0, "q", &["a"]),
        alu(4, "s", &["q", "d"]),
        alu(8, "d", &["b"]),
    ];
    let order = run(&mut board, &program, 10);

    assert!(order.is_empty(), "nothing has written yet");
    assert!(board.stats().war_stalls > 0);
    let war = (0..board.trace_count())
        .filter_map(|index| board.trace(index))
        .find(|trace| trace.kind == PipelineTraceKind::Hazard(PipelineHazardKind::WriteAfterRead))
        .expect("the finished add says why it is waiting");
    assert!(
        war.detail.contains("overwrite d"),
        "unexpected detail: {}",
        war.detail
    );

    run(&mut board, &program, 12);
    assert_eq!(board.stats().committed, 3, "and everything gets through");
}

#[test]
fn an_operand_waits_for_its_producer_to_reach_the_register_file() {
    let mut board = scoreboard();
    let program = [divide(0, "q", &["a"]), alu(4, "s", &["q"])];
    run(&mut board, &program, 6);

    assert!(
        board.stats().raw_stalls > 0,
        "no bypass, so the add sits in its unit until q is written"
    );
    assert_eq!(board.stats().committed, 0);

    run(&mut board, &program, 10);
    assert_eq!(board.stats().committed, 2);
}

#[test]
fn a_second_divide_finds_the_only_divider_busy() {
    let mut board = scoreboard();
    let program = [divide(0, "q", &["a"]), divide(4, "r", &["b"])];
    run(&mut board, &program, 6);
    assert!(board.stats().functional_unit_stalls > 0);

    // A second divider is a second thing that can be in flight.
    let mut wider = scoreboard();
    assert!(wider.set_capacity(1, 2));
    run(&mut wider, &program, 6);
    assert_eq!(
        wider.stats().functional_unit_stalls,
        0,
        "with two dividers neither waits for the other"
    );
    assert_eq!(wider.unit(1).unwrap().active, 2);
}

#[test]
fn nothing_issues_behind_an_unresolved_branch() {
    let mut board = scoreboard();
    let program = [
        op(0, PipelineInstructionClass::Branch).reading(["a".to_string()]),
        alu(4, "s", &["b"]),
    ];
    run(&mut board, &program, 5);

    assert!(traces(&board, PipelineHazardKind::BranchFlush) > 0);
    let issued = (0..board.unit_count())
        .filter_map(|unit| board.unit(unit))
        .map(|unit| unit.active)
        .sum::<usize>();
    assert_eq!(issued, 1, "only the branch itself");
}

/// The bargain the model makes: nothing speculative ever issues, so a wrong
/// guess costs only what the front end fetched.
#[test]
fn a_taken_branch_squashes_only_the_fetch_queue() {
    let mut board = scoreboard();
    let program = [
        op(0, PipelineInstructionClass::Branch).branching_to(Some(0x40)),
        alu(4, "s", &["b"]),
        alu(0x40, "t", &["c"]),
    ];
    run(&mut board, &program, 20);

    assert_eq!(board.stats().flushes, 1);
    assert_eq!(board.fetch_pc(), 0x44, "fetching on past the target");
    assert_eq!(
        board.stats().committed,
        2,
        "the branch and the instruction at the target, never the one at 4"
    );
    let flushed: usize = (0..board.timeline_len())
        .map(|row| {
            let cells = board.timeline_row(row).unwrap().cells;
            (0..cells)
                .filter_map(|cell| board.timeline_cell(row, cell))
                .filter(|cell| cell.state == PipelineTimelineState::Flushed)
                .count()
        })
        .sum();
    assert_eq!(flushed, 1, "one fetched instruction thrown away");
}

#[test]
fn memory_reaches_the_register_file_in_program_order() {
    let mut board = scoreboard();
    let program = [
        op(0, PipelineInstructionClass::Load)
            .writing("v")
            .reading(["p".to_string()]),
        op(4, PipelineInstructionClass::Store).reading(["w".to_string(), "p".to_string()]),
    ];
    // Two load/store instances, so nothing but the ordering rule holds them.
    assert!(board.set_capacity(2, 2));
    assert_eq!(run(&mut board, &program, 14), [0, 4]);
}

#[test]
fn a_system_instruction_writes_with_the_machine_drained() {
    let mut board = scoreboard();
    let program = [
        divide(0, "q", &["a"]),
        op(4, PipelineInstructionClass::System).writing("s"),
        alu(8, "t", &["b"]),
    ];
    run(&mut board, &program, 6);
    assert_eq!(
        board.stats().committed,
        0,
        "the system instruction may not pass the divide it followed"
    );

    run(&mut board, &program, 14);
    assert_eq!(board.stats().committed, 3);
}

#[test]
fn a_scoreboard_declares_its_own_phases_and_no_bypasses() {
    let board = scoreboard();
    let inspect: &dyn PipelineInspect = &board;

    assert_eq!(
        board.shape().mode,
        PipelineMode::Scoreboard,
        "the shape reports the model that is actually running it"
    );
    assert_eq!(
        (0..inspect.stage_count())
            .map(|index| inspect.stage(index).unwrap().name)
            .collect::<Vec<_>>(),
        ["IF", "IS", "RO", "EX", "WR"],
    );
    assert_eq!(
        inspect.stage(1).unwrap().role,
        PipelineStageRole::Issue,
        "the phase the declared datapath has no stage for"
    );
    assert_eq!(inspect.stage_order(), [0, 1, 2, 3, 4]);
    assert_eq!(
        inspect
            .edges()
            .iter()
            .filter(|edge| edge.kind == PipelineEdgeKind::Forward)
            .count(),
        0,
        "operands come from the register file or not at all"
    );
    assert_eq!(inspect.unit_count(), UNITS.len());
    assert!(inspect.stage(usize::MAX).is_none());
}

#[test]
fn the_history_names_the_unit_that_did_the_work() {
    let mut board = scoreboard();
    run(&mut board, &[divide(0, "q", &["a"])], 6);

    let labels: Vec<&str> = (0..board.timeline_row(0).unwrap().cells)
        .filter_map(|cell| board.timeline_cell(0, cell))
        .map(|cell| cell.label)
        .collect();
    assert_eq!(&labels[..3], ["IF", "IS", "RO"]);
    assert!(
        labels.iter().filter(|label| **label == "DIV").count() >= 2,
        "a five-cycle divide occupies its unit for more than one cycle: {labels:?}"
    );
}

#[test]
fn reset_frees_every_unit_and_every_register() {
    let mut board = scoreboard();
    run(&mut board, &[divide(0, "q", &["a"])], 4);
    assert_eq!(board.unit(1).unwrap().active, 1);

    PipelineControl::reset(&mut board, 0x80);
    assert_eq!(board.fetch_pc(), 0x80);
    assert_eq!(board.unit(1).unwrap().active, 0);
    assert_eq!(board.stats().cycles, 0);
    assert_eq!(board.timeline_len(), 0);

    // The claim the divide held is gone, so a fresh writer of q issues at once.
    run(&mut board, &[alu(0x80, "q", &[])], 6);
    assert_eq!(board.stats().waw_stalls, 0);
    assert_eq!(board.stats().committed, 1);
}
