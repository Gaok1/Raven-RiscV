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

use crate::capability::{PipelineInspect, PipelineInstructionClass};
use crate::falcon::Cpu;
use crate::falcon::cache::CacheController;
use crate::pipeline::{PipelineEngine, PipelineMode, PipelineOp, PipelineShape, PipelineTiming};
use crate::ui::Console;

use super::sim::{self, CommitInfo};
use super::{
    FuKind, InstrClass, OooEngine, PipeSlot, PipelineSimState, STAGES, Stage, UNITS, classify,
    operands,
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
        if let Some(message) = op.fault.clone() {
            // A fetch that could not happen faults where the program reached
            // it, not where the front end ran ahead to.
            console.push_error(message);
            engine.halt();
            state.faulted = true;
        } else {
            commit = execute(&op, state, cpu, mem, console);
            engine.retire(&op);
            redirect = engine.resolve(&op, u64::from(cpu.pc));
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

/// Run one instruction all the way through, in one piece.
///
/// ID reads the registers, EX computes and resolves the branch, MEM touches
/// memory, and `commit_wb` writes back and moves the PC. Nothing younger has
/// touched the machine — the engine only ever hands back its oldest entry — so
/// a trap raised here lands on exactly the state program order says it should.
fn execute(
    op: &PipelineOp<u32>,
    state: &mut PipelineSimState,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
) -> Option<CommitInfo> {
    let pc = op.address as u32;
    let mut slot = PipeSlot::from_word(pc, op.payload);
    slot.instr = crate::falcon::decoder::decode(op.payload).ok();

    sim::stage_id(&mut slot, cpu);
    sim::stage_ex(&mut slot);
    let (_latency, faulted, trap) = sim::stage_mem(&mut slot, cpu, mem, console);
    if faulted {
        state.faulted = true;
        return None;
    }
    if let Some(trap) = trap {
        sim::pipeline_enter_trap(state, cpu, mem, console, pc, trap);
        return None;
    }

    // `commit_wb` takes its instruction from the writeback slot, which is the
    // one place both models agree on.
    state.stages[Stage::WB as usize] = Some(slot);
    sim::commit_wb(state, cpu, mem, console)
}

/// Decode at `address` into an op the engine can schedule.
///
/// Everything the engine needs to know about an RV32 instruction is here, and
/// none of it is about RV32's datapath: what class of work it is, which
/// registers it reads and writes, and whether it can change the next fetch
/// address.
fn fetch(address: u32, mem: &mut CacheController) -> PipelineOp<u32> {
    let (slot, error) = sim::fetch_slot(address, mem);
    let Some(slot) = slot else {
        let message = match error {
            Some(Err(message)) => message,
            _ => format!("fetch fault at 0x{address:08X}"),
        };
        return PipelineOp::faulted(u64::from(address), message, 0);
    };

    let word = slot.word;
    let class = classify(word);
    let (rd, rs1, rs2) = operands(word);
    let mut op = PipelineOp::new(u64::from(address), slot.disasm.clone(), class, word);
    if let Some(rd) = rd.filter(|rd| *rd != 0) {
        op = op.writing(register(rd, class));
    }
    // `x0` is never owed a value, so naming it would invent dependencies the
    // hardware does not have. The float file has no such register.
    op.sources.extend(
        [rs1, rs2]
            .into_iter()
            .flatten()
            .filter(|reg| *reg != 0 || class == InstrClass::FloatingPoint)
            .map(|reg| register(reg, class)),
    );
    if matches!(class, InstrClass::Branch | InstrClass::Jump) {
        op.branch = true;
        op.branch_target = branch_target(&slot);
    }
    op
}

/// A register's architectural name, which is all the engine's hazard tracking
/// wants. Float and integer files are separate namespaces, and naming them
/// apart is what keeps `f3` from colliding with `x3`.
fn register(index: u8, class: PipelineInstructionClass) -> String {
    if class == PipelineInstructionClass::FloatingPoint {
        format!("f{index}")
    } else {
        format!("x{index}")
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
