//! What x86-64 has to say about its own pipeline.
//!
//! Everything structural — stage occupancy, hazards, flushes, the Gantt history
//! — comes from [`crate::pipeline`]. All that is left here is the part only this
//! ISA can answer: what a decoded instruction reads and writes, what class of
//! work it is, and where a taken branch goes. The implicit operands are the
//! interesting half: `div` reads `rdx:rax` without naming either, a `cl` shift
//! reads `rcx`, and almost everything writes flags. A pipeline view that missed
//! those would show an x86 program as far more independent than it is.

use super::{BinaryOp, Decoded, DecodedOp, Operand, REG64, UnaryOp, WideningOp, reg_name};
use crate::capability::{PipelineInstructionClass, PipelineStageRole};
use crate::pipeline::{
    PipelineBypassConfig, PipelineOp, PipelineShape, PipelineTiming, ScalarPipeline, StageSpec,
    UnitSpec,
};

/// An in-flight x86 instruction: the shared op, carrying this ISA's decode.
pub(super) type X86Op = PipelineOp<Decoded>;
pub(super) type X86Pipeline = ScalarPipeline<Decoded>;

static STAGES: [StageSpec; 6] = [
    StageSpec::new("IF", PipelineStageRole::Fetch),
    StageSpec::new("ID", PipelineStageRole::Decode),
    StageSpec::new("IS", PipelineStageRole::Issue),
    StageSpec::new("EX", PipelineStageRole::Execute),
    StageSpec::new("MEM", PipelineStageRole::Memory),
    StageSpec::new("RT", PipelineStageRole::Commit),
];

/// The units execute dispatches into.
///
/// A class is matched against these in order, so the integer ALU names what it
/// takes rather than claiming everything: `imul` and `div` belong to units of
/// their own precisely because they are the instructions that take long enough
/// for the overlap to be worth watching.
static UNITS: [UnitSpec; 5] = [
    UnitSpec::new(
        "Integer ALU",
        &[
            PipelineInstructionClass::Alu,
            PipelineInstructionClass::Branch,
            PipelineInstructionClass::Jump,
        ],
        PipelineInstructionClass::Alu,
    ),
    UnitSpec::new(
        "Multiply",
        &[PipelineInstructionClass::Multiply],
        PipelineInstructionClass::Multiply,
    ),
    UnitSpec::new(
        "Divide",
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

/// What x86 instructions cost beyond the one cycle every instruction takes.
///
/// Only the two that genuinely run long on real silicon: an `imul` is a few
/// cycles and a 64-bit `div` is tens of them. Everything else is left at zero
/// rather than invented, so a cycle count still means something.
fn timing() -> PipelineTiming {
    PipelineTiming {
        mul: 2,
        div: 19,
        ..PipelineTiming::single_cycle()
    }
}

/// x86-64's datapath as this backend models it.
///
/// Six stages and variable-length instructions, with no operand bypasses — a
/// dependent instruction waits for its producer to retire, which is what makes
/// the hazards visible in the first place. Execute dispatches into the unit
/// bank, so a long `div` no longer blocks the adds behind it; they overlap it
/// and still retire in program order.
pub(super) fn shape() -> PipelineShape {
    PipelineShape::scalar(&STAGES, None)
        .with_bypass(PipelineBypassConfig::disabled())
        .with_parallel_units(&UNITS)
        .with_timing(timing())
}

pub(super) fn op(address: u64, decoded: Decoded) -> X86Op {
    let (class, destination, sources, writes, target) = metadata(&decoded, address);
    let mut op = X86Op::new(address, decoded.text.clone(), class, decoded);
    op.length = op.payload.len as u64;
    op.destination = destination;
    op.sources = sources;
    op.writes = writes;
    if let Some(target) = target {
        op.branch = true;
        op.branch_target = target;
    }
    op
}

pub(super) fn fault_op(address: u64, message: impl Into<String>) -> X86Op {
    X86Op::faulted(
        address,
        message,
        Decoded {
            len: 1,
            text: "fetch fault".into(),
            class: "Fault",
            op: DecodedOp::Nop,
        },
    )
}

/// The class, operands and control target of one decoded instruction.
///
/// The outer `Option` on the last field says whether this is a control transfer
/// at all; the inner one says whether its target is known from the encoding, as
/// it is for a relative jump but not for `ret`.
type Metadata = (
    PipelineInstructionClass,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Option<Option<u64>>,
);

fn metadata(decoded: &Decoded, address: u64) -> Metadata {
    let mut sources = Vec::new();
    let mut writes = Vec::new();
    let mut destination = None;
    let mut control = None;
    // Relative displacements count from the end of the instruction.
    let relative = |displacement: i32| {
        Some(Some(
            address
                .wrapping_add(decoded.len as u64)
                .wrapping_add_signed(i64::from(displacement)),
        ))
    };
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
        DecodedOp::Jump(displacement) => {
            control = relative(displacement);
            PipelineInstructionClass::Jump
        }
        DecodedOp::JumpIf {
            displacement,
            condition: _,
        } => {
            control = relative(displacement);
            sources.push("rflags".into());
            PipelineInstructionClass::Branch
        }
        DecodedOp::Call(displacement) => {
            control = relative(displacement);
            sources.push("rsp".into());
            writes.push("rsp".into());
            PipelineInstructionClass::Jump
        }
        DecodedOp::Ret => {
            // The return address is in memory, so the front end cannot know
            // where this goes until it has run.
            control = Some(None);
            sources.push("rsp".into());
            writes.push("rsp".into());
            PipelineInstructionClass::Jump
        }
    };
    (class, destination, sources, writes, control)
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
