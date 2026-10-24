//! RV32 under a dynamically scheduled model.
//!
//! This is not a second simulator. The scheduling is the shared engine's — a
//! [`ScoreboardPipeline`](crate::pipeline::ScoreboardPipeline) or a
//! [`TomasuloPipeline`](crate::pipeline::TomasuloPipeline), the same ones every
//! other backend runs — and RV32 supplies only what an ISA has to: the
//! instruction word, the registers it reads and writes, and what its semantics
//! do when the engine says its turn has come.
//!
//! # Why the stage model does not do this
//!
//! [`sim`](super::sim) executes an instruction *across* the datapath: operands
//! are read in ID, the result computed in EX, memory touched in MEM. That is
//! what lets RV32 forward real values and take an MMU trap from the middle of
//! the pipeline, and it is exactly what an out-of-order model may not do —
//! there, an instruction must change the machine in one piece, at commit, in
//! program order, or the precise exceptions the reorder buffer exists for are
//! not precise.
//!
//! So a commit here runs ID, EX, MEM and WB back to back for one instruction,
//! reusing [`sim`]'s own stage functions. The semantics are RV32's, down to the
//! trap handling; only *when* they happen is the engine's.

use crate::capability::PipelineInspect;
use crate::falcon::Cpu;
use crate::falcon::cache::CacheController;
use crate::pipeline::{PipelineEngine, PipelineMode, PipelineOp, PipelineShape, PipelineTiming};
use crate::ui::Console;

use super::forwarding::{RegFile, is_hardwired_zero, operand_reg_file, slot_destination};
use super::sim::{self, CommitInfo, Writeback};
use super::{
    FetchFault, FuKind, InstrClass, OooEngine, PipeSlot, PipelineSimState, STAGES, Stage, UNITS,
};

/// RV32's datapath as the shared engines see it.
///
/// The same five stages and six units the in-order model declares, so the
/// unit bank a user configured means the same thing under either — switching
/// model changes the scheduling, not the machine.
fn shape(mode: PipelineMode, timing: &PipelineTiming) -> PipelineShape {
    let mut shape = PipelineShape::scalar(&STAGES, Some(4))
        .with_units(&UNITS)
        .with_timing(timing.clone());
    shape.mode = mode;
    shape
}

/// Build the engine a dynamically scheduled mode asks for, or `None` for the
/// in-order modes, which [`sim`] still runs.
pub fn engine_for(
    mode: PipelineMode,
    timing: &PipelineTiming,
    capacity: &[u8; FuKind::COUNT],
    entry: u32,
) -> Option<OooEngine> {
    if !mode.is_out_of_order() {
        return None;
    }
    let mut engine = PipelineEngine::new(shape(mode, timing), u64::from(entry));
    for (unit, instances) in capacity.iter().enumerate() {
        engine.set_capacity(unit, (*instances).max(1) as usize);
    }
    Some(engine)
}

/// One out-of-order clock.
///
/// Same shape as the in-order tick: take what the engine hands back, execute
/// it, tell the engine where the branch really went, then let it advance and
/// refill the front end.
pub fn ooo_tick(
    state: &mut PipelineSimState,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    timing: &PipelineTiming,
    console: &mut Console,
) -> Option<CommitInfo> {
    // Taken out for the cycle: executing an instruction needs the rest of
    // `state`, and the engine is part of it.
    let mut engine = state.ooo.take()?;
    // Latency edits on the settings screen reach the shape a user is running.
    engine.set_timing(timing.clone());

    let committing = engine.start_cycle();
    let mut commit = None;
    let mut redirect = None;

    if let Some(op) = committing {
        if op.fault.is_some() {
            // A fetch that could not happen faults where the program reached
            // it, not where the front end ran ahead to. Fetching again now,
            // with every older instruction retired, says what really happens
            // there: a page fault enters the handler, anything else is fatal.
            let pc = op.address as u32;
            match sim::fetch_slot(pc, mem).fetch_fault {
                Some(FetchFault::Fatal(message)) => {
                    console.push_error(message);
                    engine.halt();
                    state.faulted = true;
                }
                Some(FetchFault::Trap(trap)) => {
                    sim::pipeline_enter_trap(state, cpu, mem, console, pc, trap);
                    engine.retire(&op);
                    redirect = Some(u64::from(cpu.pc));
                }
                // The older instructions made it fetchable after all.
                None => {
                    engine.retire(&op);
                    redirect = Some(u64::from(pc));
                }
            }
        } else {
            match execute(&op, state, cpu, mem, console) {
                // Still the oldest entry: it runs again once the host
                // resumes, exactly as the in-order model refetches it.
                Writeback::Parked => engine.retry(op, "awaiting input"),
                outcome => {
                    if let Writeback::Retired(info) = outcome {
                        commit = Some(info);
                    }
                    engine.retire(&op);
                    redirect = engine
                        .resolve(&op, u64::from(cpu.pc))
                        .or_else(|| refetch_after(&op, cpu.pc));
                }
            }
        }
    }

    if state.halted {
        engine.halt();
    }
    if let Some(address) = engine.advance(redirect) {
        engine.fetched(fetch(address as u32, mem));
    }

    state.fetch_pc = engine.fetch_pc() as u32;
    state.cycle_count = engine.stats().cycles;
    state.ooo = Some(engine);
    commit
}

/// Where the front end has to start over after an instruction that is not a
/// branch, if anywhere.
///
/// The engine fetches down the sequential path behind every such
/// instruction. That path is wrong when the instruction moved the PC anyway —
/// a trap, `mret`, `sret` — and stale when it changed what fetching means: the
/// code (`fence.i`), the translations (`sfence.vma`, a write to `satp`) or the
/// privilege level. Every system instruction is one of those or cheap enough
/// to treat as one, so the front end refetches after all of them, the way an
/// in-order core drains behind a serialising instruction.
fn refetch_after(op: &PipelineOp<u32>, pc: u32) -> Option<u64> {
    let sequential = op.address.wrapping_add(4);
    (op.class == InstrClass::System || u64::from(pc) != sequential).then_some(u64::from(pc))
}

/// Run one instruction all the way through, in one piece.
///
/// ID reads the registers, EX computes and resolves the branch, MEM touches
/// memory, and `writeback` writes back and moves the PC. Nothing younger has
/// touched the machine — the engine only ever hands back its oldest entry — so
/// a trap raised here lands on exactly the state program order says it should.
fn execute(
    op: &PipelineOp<u32>,
    state: &mut PipelineSimState,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
) -> Writeback {
    let pc = op.address as u32;
    let mut slot = PipeSlot::from_word(pc, op.payload);
    slot.instr = crate::falcon::decoder::decode(op.payload).ok();

    sim::stage_id(&mut slot, cpu);
    sim::stage_ex(&mut slot);
    let (_latency, faulted, trap) = sim::stage_mem(&mut slot, cpu, mem, console);
    if faulted {
        state.faulted = true;
        return Writeback::Empty;
    }
    if let Some(trap) = trap {
        sim::pipeline_enter_trap(state, cpu, mem, console, pc, trap);
        return Writeback::Empty;
    }

    // `writeback` takes its instruction from the writeback slot, which is the
    // one place both models agree on.
    state.stages[Stage::WB as usize] = Some(slot);
    sim::writeback(state, cpu, mem, console)
}

/// Decode at `address` into an op the engine can schedule.
///
/// Everything the engine needs to know about an RV32 instruction is here, and
/// none of it is about RV32's datapath: what class of work it is, which
/// registers it reads and writes, and whether it can change the next fetch
/// address.
///
/// A register left out here costs more than cycles. The scoreboard executes an
/// instruction when its unit writes the result, so a reader that was never
/// told to wait for its producer runs on a stale value.
fn fetch(address: u32, mem: &mut CacheController) -> PipelineOp<u32> {
    let mut slot = sim::fetch_slot(address, mem);
    if let Some(fault) = &slot.fetch_fault {
        let message = match fault {
            FetchFault::Fatal(message) => message.clone(),
            FetchFault::Trap(trap) => {
                format!("fetch page fault at 0x{address:08X} (cause {})", trap.cause)
            }
        };
        return PipelineOp::faulted(u64::from(address), message, 0);
    }
    // `fetch_slot` leaves decoding to ID, a stage this model does not have,
    // and which file an operand names or where a branch goes are questions
    // about the instruction, not the word. A word that does not decode has
    // already become a fetch fault above.
    slot.instr = crate::falcon::decoder::decode(slot.word).ok();

    let mut op = PipelineOp::new(
        u64::from(address),
        slot.disasm.clone(),
        slot.class,
        slot.word,
    );
    // `x0` is never owed a value, so naming it would invent dependencies the
    // hardware does not have. `f0` is an ordinary register, named like any
    // other.
    let named = |(file, index): &(RegFile, u8)| !is_hardwired_zero(*file, *index);
    let mut writes = destinations(&slot).into_iter().filter(named);
    if let Some((file, index)) = writes.next() {
        op = op.writing(register(file, index));
    }
    for (file, index) in writes {
        op = op.also_writes(register(file, index));
    }
    op.sources.extend(
        sources(&slot)
            .into_iter()
            .filter(named)
            .map(|(file, index)| register(file, index)),
    );
    if matches!(slot.class, InstrClass::Branch | InstrClass::Jump) {
        op.branch = true;
        op.branch_target = branch_target(&slot);
    }
    op
}

/// The registers an instruction writes. The first is the one a host shows.
///
/// [`slot_destination`] answers this for the in-order model, which never asks
/// it of the two kinds of instruction that write their register at WB with no
/// bypass, Zicsr and `ecall`: its front end stops behind them until they
/// commit. Here they are named too, so a host can show the register and the
/// instruction waits for an older one still writing it.
fn destinations(slot: &PipeSlot) -> Vec<(RegFile, u8)> {
    use crate::falcon::instruction::Instruction::*;
    match slot.instr {
        Some(
            Csrrw { rd, .. }
            | Csrrs { rd, .. }
            | Csrrc { rd, .. }
            | Csrrwi { rd, .. }
            | Csrrsi { rd, .. }
            | Csrrci { rd, .. },
        ) => vec![(RegFile::Int, rd)],
        // Every call answers in a0, and the two that return a 64-bit count
        // put its high half in a1. Which call this is depends on a7, which
        // is not read until commit.
        Some(Ecall) => vec![(RegFile::Int, 10), (RegFile::Int, 11)],
        _ => slot_destination(slot).into_iter().collect(),
    }
}

/// The registers an instruction reads, each in the file its operand names.
///
/// That is a property of the operand, not of the instruction: `flw` and `fsw`
/// address memory through an integer base, `fmv.w.x` and `fcvt.s.w` convert
/// from an integer, and the fused multiply-adds read a third float source
/// the (rd, rs1, rs2) triple has no room for.
fn sources(slot: &PipeSlot) -> Vec<(RegFile, u8)> {
    use crate::falcon::instruction::Instruction::*;
    let Some(instr) = slot.instr else {
        return Vec::new();
    };
    if matches!(instr, Ecall) {
        // RAVEN's syscall ABI: a7 picks the call and a0..a6 carry its
        // arguments. Which of them a call reads is not known until a7 is, so
        // all eight are named, as the in-order model's hazard check does.
        return (10..=17).map(|index| (RegFile::Int, index)).collect();
    }
    let rs3 = match instr {
        FmaddS { rs3, .. } | FmsubS { rs3, .. } | FnmsubS { rs3, .. } | FnmaddS { rs3, .. } => {
            Some(rs3)
        }
        _ => None,
    };
    [(1, slot.rs1), (2, slot.rs2), (3, rs3)]
        .into_iter()
        .filter_map(|(operand, index)| Some((operand_reg_file(instr, operand)?, index?)))
        .collect()
}

/// A register's architectural name, which is all the engine's hazard tracking
/// wants. Float and integer files are separate namespaces, and naming them
/// apart is what keeps `f3` from colliding with `x3`.
fn register(file: RegFile, index: u8) -> String {
    match file {
        RegFile::Int => format!("x{index}"),
        RegFile::Float => format!("f{index}"),
    }
}

/// Where a taken branch goes, when the encoding says so — which is what lets
/// the front end guess instead of always assuming the fall-through path.
fn branch_target(slot: &PipeSlot) -> Option<u64> {
    use crate::falcon::instruction::Instruction::*;
    match slot.instr? {
        Jal { imm, .. }
        | Beq { imm, .. }
        | Bne { imm, .. }
        | Blt { imm, .. }
        | Bge { imm, .. }
        | Bltu { imm, .. }
        | Bgeu { imm, .. } => Some(u64::from(slot.pc.wrapping_add(imm as u32))),
        // `jalr` computes its target from a register the front end has not read
        // yet, so there is nothing to guess with.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::falcon::asm::assemble;
    use crate::falcon::cache::{CacheConfig, CacheController};
    use crate::falcon::program::load_words;
    use crate::pipeline::{
        BranchPredict, PipelineEngine, PipelineMode, PipelineOp, PipelineTiming,
    };

    use super::{fetch, shape};

    /// Memory holding `asm` from address 0, as the front end would fetch it.
    fn memory_with(asm: &str) -> CacheController {
        let prog = assemble(asm, 0).expect("assemble");
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        mem.bypass = true;
        load_words(&mut mem.ram, 0, &prog.text).expect("load text");
        mem
    }

    /// The op the front end builds for the one instruction in `line`.
    fn op_for(line: &str) -> PipelineOp<u32> {
        fetch(0, &mut memory_with(&format!(".text\n    {line}\n")))
    }

    /// What an op tells the engine about registers: the one it shows, every
    /// one it writes, and every one it reads.
    fn registers(op: &PipelineOp<u32>) -> (Option<&str>, Vec<&str>, Vec<&str>) {
        (
            op.destination.as_deref(),
            op.writes.iter().map(String::as_str).collect(),
            op.sources.iter().map(String::as_str).collect(),
        )
    }

    #[test]
    fn f0_is_a_destination_like_any_float_register_and_x0_is_not() {
        // f0 used to be dropped like x0, so nothing waited for its producer.
        assert_eq!(
            registers(&op_for("fsgnj.s f0, f1, f2")),
            (Some("f0"), vec!["f0"], vec!["f1", "f2"])
        );
        assert_eq!(
            registers(&op_for("fsub.s f0, f0, f0")),
            (Some("f0"), vec!["f0"], vec!["f0", "f0"])
        );
        assert_eq!(registers(&op_for("addi x0, x0, 1")), (None, vec![], vec![]));
        assert_eq!(
            registers(&op_for("fmv.w.x f0, x0")),
            (Some("f0"), vec!["f0"], vec![])
        );
    }

    #[test]
    fn each_operand_is_named_in_its_own_register_file() {
        // Every operand of a float instruction used to be named in the float
        // file, so an integer base, source or result never met its producer
        // or its reader.
        let cases: [(&str, Option<&str>, &[&str]); 12] = [
            ("fmv.x.w a0, f0", Some("x10"), &["f0"]),
            ("fmv.w.x f0, a1", Some("f0"), &["x11"]),
            ("flw f0, 4(a1)", Some("f0"), &["x11"]),
            ("fsw f0, 8(a1)", None, &["x11", "f0"]),
            ("feq.s a0, f0, f1", Some("x10"), &["f0", "f1"]),
            ("flt.s a0, f1, f0", Some("x10"), &["f1", "f0"]),
            ("fle.s a0, f0, f1", Some("x10"), &["f0", "f1"]),
            ("fclass.s a0, f0", Some("x10"), &["f0"]),
            ("fcvt.w.s a0, f0", Some("x10"), &["f0"]),
            ("fcvt.wu.s a0, f0", Some("x10"), &["f0"]),
            ("fcvt.s.w f0, a1", Some("f0"), &["x11"]),
            ("fcvt.s.wu f0, a1", Some("f0"), &["x11"]),
        ];
        for (line, destination, sources) in cases {
            let writes: Vec<&str> = destination.into_iter().collect();
            let expected = (destination, writes, sources.to_vec());
            assert_eq!(registers(&op_for(line)), expected, "{line}");
        }
    }

    #[test]
    fn a_fused_multiply_add_reads_its_third_source() {
        assert_eq!(
            registers(&op_for("fmadd.s f0, f1, f2, f3")),
            (Some("f0"), vec!["f0"], vec!["f1", "f2", "f3"])
        );
        for line in [
            "fmsub.s f4, f5, f6, f0",
            "fnmsub.s f4, f5, f6, f0",
            "fnmadd.s f4, f5, f6, f0",
        ] {
            assert_eq!(
                registers(&op_for(line)),
                (Some("f4"), vec!["f4"], vec!["f5", "f6", "f0"]),
                "{line}"
            );
        }
    }

    #[test]
    fn ecall_reads_the_syscall_arguments_and_writes_the_result() {
        // a7 picks the call and a0..a6 carry its arguments; the answer comes
        // back in a0, and in a1 too for the calls that return a 64-bit count.
        assert_eq!(
            registers(&op_for("ecall")),
            (
                Some("x10"),
                vec!["x10", "x11"],
                vec!["x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17"]
            )
        );
    }

    #[test]
    fn floating_point_status_is_not_a_register_dependency() {
        // Both models run a CSR instruction with the machine drained, and the
        // flags an F instruction raises are OR-ed in, which no order spoils,
        // so fflags and fcsr are nothing for the scheduler to wait on.
        assert_eq!(
            registers(&op_for("fdiv.s f0, f1, f2")),
            (Some("f0"), vec!["f0"], vec!["f1", "f2"])
        );
        assert_eq!(
            registers(&op_for("csrrw a0, fcsr, a1")),
            (Some("x10"), vec!["x10"], vec!["x11"])
        );
        assert_eq!(
            registers(&op_for("csrr a0, fflags")),
            (Some("x10"), vec!["x10"], vec![])
        );
    }

    #[test]
    fn a_branch_carries_the_target_it_encodes() {
        // The slot `fetch` starts from is not decoded yet, so the target used
        // to be `None` for every branch.
        let mut mem = memory_with(
            r#"
.text
top:
    beq  a0, a1, ahead
    bne  a0, zero, top
    jal  ra, top
    jalr ra, a0, 0
ahead:
    halt
"#,
        );
        let mut target = |pc: u32| {
            let op = fetch(pc, &mut mem);
            assert!(op.branch, "0x{pc:x} is a branch");
            op.branch_target
        };
        assert_eq!(target(0x0), Some(0x10));
        assert_eq!(target(0x4), Some(0x0));
        assert_eq!(target(0x8), Some(0x0));
        // `jalr` goes where a register says, and nothing has read it yet.
        assert_eq!(target(0xC), None);
    }

    #[test]
    fn the_front_end_follows_a_branch_it_predicts_taken() {
        // With no target the engine can only guess not-taken, whatever
        // predictor a user picked.
        let mut mem = memory_with(
            r#"
.text
top:
    addi a0, a0, -1
    bne  a0, zero, top
    halt
"#,
        );
        let shape = shape(PipelineMode::TomasuloRob, &PipelineTiming::default())
            .with_predict(BranchPredict::Taken);
        let mut engine = PipelineEngine::new(shape, 0);
        engine.fetched(fetch(0x4, &mut mem));
        assert_eq!(engine.fetch_pc(), 0x0, "fetch went down the fall-through");
    }
}
