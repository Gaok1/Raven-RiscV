//! Pipeline simulator tick logic — per-stage execution.

use super::{
    FuKind, GanttCell, GanttRow, GanttTrack, HazardTrace, HazardType, InstrClass, MAX_GANTT_COLS,
    MAX_GANTT_ROWS, PipeSlot, PipelineSimState, Stage, TraceKind, forwarding, fu_latency_for_class,
    predictor,
};
use crate::falcon::Cpu;
use crate::falcon::cache::CacheController;
use crate::falcon::instruction::Instruction;
use crate::falcon::memory::AmoOp;
use crate::ui::app::CpiConfig;
use crate::ui::console::Console;
use std::collections::VecDeque;

// ── Commit info returned to caller ──────────────────────────────────────────

pub struct CommitInfo {
    pub pc: u32,
    pub class: InstrClass,
}

fn fu_type_idx(class: InstrClass) -> Option<usize> {
    match class {
        InstrClass::Alu => Some(0),
        InstrClass::Mul => Some(1),
        InstrClass::Div => Some(2),
        InstrClass::Fp => Some(3),
        InstrClass::Load => Some(4),
        InstrClass::Store => Some(5),
        InstrClass::System => Some(6),
        InstrClass::Branch | InstrClass::Jump | InstrClass::Unknown => None,
    }
}

// ── Main tick ────────────────────────────────────────────────────────────────

/// Advance the pipeline by one clock cycle using shared cpu/mem.
/// Returns `Some(CommitInfo)` if an instruction was committed this cycle.
pub fn pipeline_tick(
    state: &mut PipelineSimState,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    cpi: &CpiConfig,
    console: &mut Console,
) -> Option<CommitInfo> {
    if state.halted || state.faulted {
        return None;
    }

    state.last_cycle_cache_only = false;
    state.hazard_msgs.clear();
    state.hazard_traces.clear();
    state.cycle_count += 1;
    let wb_slot_before_commit = state.stages[Stage::WB as usize].clone();

    // ── 1. COMMIT: WB stage ───────────────────────────────────────────────
    let commit = commit_wb(state, cpu, mem, console);
    let cpu_after_wb = cpu.clone();

    // ── 2. Detect stalls ─────────────────────────────────────────────────
    let stall = detect_stall(state, wb_slot_before_commit.as_ref());

    // ── 3. Advance or stall ───────────────────────────────────────────────
    if let Some((stall_stage, hazard)) = stall {
        insert_stall(
            state,
            stall_stage,
            hazard,
            &cpu_after_wb,
            wb_slot_before_commit.as_ref(),
            cpu,
            mem,
            console,
        );
        state.stall_count += 1;
        if let Some(idx) = hazard.as_stall_index() {
            state.stall_by_type[idx] += 1;
        }
    } else {
        advance_stages(
            state,
            &cpu_after_wb,
            wb_slot_before_commit.as_ref(),
            cpu,
            mem,
            cpi,
            console,
        );
    }

    // ── 4. Report forwarding hazards (informational) ─────────────────────
    if state.bypass.ex_to_ex || state.bypass.mem_to_ex || state.bypass.wb_to_id {
        forwarding::report_forward_hazards(state);
    }

    // ── 5. Detect WAW/WAR (informational) ────────────────────────────────
    detect_name_hazards(state);

    // ── 6. Update Gantt ──────────────────────────────────────────────────
    update_gantt(state);

    commit
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Per-stage execution functions ────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// **ID stage** — Decode instruction word and read register operands.
fn stage_id(slot: &mut PipeSlot, cpu: &Cpu) {
    if slot.is_bubble || slot.class == InstrClass::Unknown {
        return;
    }

    slot.instr = crate::falcon::decoder::decode(slot.word).ok();

    let instr = match slot.instr {
        Some(i) => i,
        None => return,
    };

    // Determine whether this instruction reads from float registers
    let reads_float = matches!(
        instr,
        Instruction::FaddS { .. }
            | Instruction::FsubS { .. }
            | Instruction::FmulS { .. }
            | Instruction::FdivS { .. }
            | Instruction::FsqrtS { .. }
            | Instruction::FminS { .. }
            | Instruction::FmaxS { .. }
            | Instruction::FsgnjS { .. }
            | Instruction::FsgnjnS { .. }
            | Instruction::FsgnjxS { .. }
            | Instruction::FeqS { .. }
            | Instruction::FltS { .. }
            | Instruction::FleS { .. }
            | Instruction::FmaddS { .. }
            | Instruction::FmsubS { .. }
            | Instruction::FnmsubS { .. }
            | Instruction::FnmaddS { .. }
            | Instruction::FmvXW { .. }
            | Instruction::FclassS { .. }
            | Instruction::FcvtWS { .. }
            | Instruction::FcvtWuS { .. }
    );

    // Fsw: rs1 from int (address base), rs2 from float (value to store)
    let is_fsw = matches!(instr, Instruction::Fsw { .. });

    if let Some(rs1) = slot.rs1 {
        slot.rs1_val = if reads_float && !is_fsw {
            cpu.fread_bits(rs1)
        } else {
            cpu.read(rs1)
        };
    }
    if let Some(rs2) = slot.rs2 {
        slot.rs2_val = if reads_float || is_fsw {
            cpu.fread_bits(rs2)
        } else {
            cpu.read(rs2)
        };
    }

    // FcvtSW/FcvtSWu: rs1 from int register
    if matches!(
        instr,
        Instruction::FcvtSW { .. } | Instruction::FcvtSWu { .. }
    ) {
        if let Some(rs1) = slot.rs1 {
            slot.rs1_val = cpu.read(rs1);
        }
    }
    // FmvWX: rs1 from int register
    if matches!(instr, Instruction::FmvWX { .. }) {
        if let Some(rs1) = slot.rs1 {
            slot.rs1_val = cpu.read(rs1);
        }
    }
    // Flw: rs1 from int register (address base)
    if matches!(instr, Instruction::Flw { .. }) {
        if let Some(rs1) = slot.rs1 {
            slot.rs1_val = cpu.read(rs1);
        }
    }

    // Fused multiply-add: read rs3 from float register, store in mem_addr (reused field)
    match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. } => {
            slot.mem_addr = Some(cpu.fread_bits(rs3));
        }
        _ => {}
    }
}

/// **EX stage** — ALU compute, branch resolve, address calculation.
/// Pure computation — no side effects on cpu/mem.
fn stage_ex(slot: &mut PipeSlot) {
    if slot.is_bubble {
        return;
    }
    let instr = match slot.instr {
        Some(i) => i,
        None => return,
    };

    match instr {
        // ── R-type ALU ──────────────────────────────────────────────────
        Instruction::Add { .. } => slot.alu_result = slot.rs1_val.wrapping_add(slot.rs2_val),
        Instruction::Sub { .. } => slot.alu_result = slot.rs1_val.wrapping_sub(slot.rs2_val),
        Instruction::And { .. } => slot.alu_result = slot.rs1_val & slot.rs2_val,
        Instruction::Or { .. } => slot.alu_result = slot.rs1_val | slot.rs2_val,
        Instruction::Xor { .. } => slot.alu_result = slot.rs1_val ^ slot.rs2_val,
        Instruction::Sll { .. } => slot.alu_result = slot.rs1_val << (slot.rs2_val & 0x1F),
        Instruction::Srl { .. } => slot.alu_result = slot.rs1_val >> (slot.rs2_val & 0x1F),
        Instruction::Sra { .. } => {
            slot.alu_result = ((slot.rs1_val as i32) >> (slot.rs2_val & 0x1F)) as u32
        }
        Instruction::Slt { .. } => {
            slot.alu_result = ((slot.rs1_val as i32) < (slot.rs2_val as i32)) as u32
        }
        Instruction::Sltu { .. } => slot.alu_result = (slot.rs1_val < slot.rs2_val) as u32,

        // ── R-type MUL/DIV ──────────────────────────────────────────────
        Instruction::Mul { .. } => {
            slot.alu_result =
                (slot.rs1_val as i32 as i64).wrapping_mul(slot.rs2_val as i32 as i64) as u32;
        }
        Instruction::Mulh { .. } => {
            let r = (slot.rs1_val as i32 as i64).wrapping_mul(slot.rs2_val as i32 as i64);
            slot.alu_result = (r >> 32) as u32;
        }
        Instruction::Mulhsu { .. } => {
            let r = (slot.rs1_val as i32 as i64).wrapping_mul(slot.rs2_val as u64 as i64);
            slot.alu_result = (r >> 32) as u32;
        }
        Instruction::Mulhu { .. } => {
            let r = (slot.rs1_val as u64).wrapping_mul(slot.rs2_val as u64);
            slot.alu_result = (r >> 32) as u32;
        }
        Instruction::Div { .. } => {
            let d = slot.rs2_val as i32;
            slot.alu_result = if d == 0 {
                u32::MAX
            } else {
                (slot.rs1_val as i32).wrapping_div(d) as u32
            };
        }
        Instruction::Divu { .. } => {
            slot.alu_result = if slot.rs2_val == 0 {
                u32::MAX
            } else {
                slot.rs1_val.wrapping_div(slot.rs2_val)
            };
        }
        Instruction::Rem { .. } => {
            let d = slot.rs2_val as i32;
            slot.alu_result = if d == 0 {
                slot.rs1_val
            } else {
                (slot.rs1_val as i32).wrapping_rem(d) as u32
            };
        }
        Instruction::Remu { .. } => {
            slot.alu_result = if slot.rs2_val == 0 {
                slot.rs1_val
            } else {
                slot.rs1_val.wrapping_rem(slot.rs2_val)
            };
        }

        // ── I-type ALU ──────────────────────────────────────────────────
        Instruction::Addi { imm, .. } => slot.alu_result = slot.rs1_val.wrapping_add(imm as u32),
        Instruction::Andi { imm, .. } => slot.alu_result = slot.rs1_val & (imm as u32),
        Instruction::Ori { imm, .. } => slot.alu_result = slot.rs1_val | (imm as u32),
        Instruction::Xori { imm, .. } => slot.alu_result = slot.rs1_val ^ (imm as u32),
        Instruction::Slti { imm, .. } => slot.alu_result = ((slot.rs1_val as i32) < imm) as u32,
        Instruction::Sltiu { imm, .. } => slot.alu_result = (slot.rs1_val < imm as u32) as u32,
        Instruction::Slli { shamt, .. } => slot.alu_result = slot.rs1_val << (shamt & 0x1F),
        Instruction::Srli { shamt, .. } => slot.alu_result = slot.rs1_val >> (shamt & 0x1F),
        Instruction::Srai { shamt, .. } => {
            slot.alu_result = ((slot.rs1_val as i32) >> (shamt & 0x1F)) as u32
        }

        // ── U-type ──────────────────────────────────────────────────────
        Instruction::Lui { imm, .. } => slot.alu_result = imm as u32,
        Instruction::Auipc { imm, .. } => slot.alu_result = slot.pc.wrapping_add(imm as u32),

        // ── Loads: compute address ──────────────────────────────────────
        Instruction::Lb { imm, .. }
        | Instruction::Lh { imm, .. }
        | Instruction::Lw { imm, .. }
        | Instruction::Lbu { imm, .. }
        | Instruction::Lhu { imm, .. }
        | Instruction::Flw { imm, .. } => {
            slot.mem_addr = Some(slot.rs1_val.wrapping_add(imm as u32));
        }

        // ── Stores: compute address ─────────────────────────────────────
        Instruction::Sb { imm, .. }
        | Instruction::Sh { imm, .. }
        | Instruction::Sw { imm, .. }
        | Instruction::Fsw { imm, .. } => {
            slot.mem_addr = Some(slot.rs1_val.wrapping_add(imm as u32));
        }

        // ── Branches: evaluate condition + compute target ───────────────
        Instruction::Beq { imm, .. } => {
            slot.branch_taken = slot.rs1_val == slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bne { imm, .. } => {
            slot.branch_taken = slot.rs1_val != slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Blt { imm, .. } => {
            slot.branch_taken = (slot.rs1_val as i32) < (slot.rs2_val as i32);
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bge { imm, .. } => {
            slot.branch_taken = (slot.rs1_val as i32) >= (slot.rs2_val as i32);
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bltu { imm, .. } => {
            slot.branch_taken = slot.rs1_val < slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bgeu { imm, .. } => {
            slot.branch_taken = slot.rs1_val >= slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }

        // ── Jumps ───────────────────────────────────────────────────────
        Instruction::Jal { imm, .. } => {
            slot.alu_result = slot.pc.wrapping_add(4); // link address
            slot.branch_taken = true;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Jalr { imm, .. } => {
            slot.alu_result = slot.pc.wrapping_add(4); // link address
            slot.branch_taken = true;
            slot.branch_target = Some((slot.rs1_val.wrapping_add(imm as u32)) & !1);
        }

        // ── FP arithmetic ───────────────────────────────────────────────
        Instruction::FaddS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = (a + b).to_bits();
        }
        Instruction::FsubS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = (a - b).to_bits();
        }
        Instruction::FmulS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = (a * b).to_bits();
        }
        Instruction::FdivS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = (a / b).to_bits();
        }
        Instruction::FsqrtS { .. } => {
            slot.alu_result = f32::from_bits(slot.rs1_val).sqrt().to_bits();
        }
        Instruction::FminS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let r = if a.is_nan() {
                b
            } else if b.is_nan() {
                a
            } else if a == 0.0 && b == 0.0 {
                if a.is_sign_negative() { a } else { b }
            } else {
                a.min(b)
            };
            slot.alu_result = r.to_bits();
        }
        Instruction::FmaxS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let r = if a.is_nan() {
                b
            } else if b.is_nan() {
                a
            } else if a == 0.0 && b == 0.0 {
                if a.is_sign_positive() { a } else { b }
            } else {
                a.max(b)
            };
            slot.alu_result = r.to_bits();
        }
        Instruction::FsgnjS { .. } => {
            slot.alu_result = (slot.rs1_val & 0x7FFF_FFFF) | (slot.rs2_val & 0x8000_0000);
        }
        Instruction::FsgnjnS { .. } => {
            slot.alu_result = (slot.rs1_val & 0x7FFF_FFFF) | (!slot.rs2_val & 0x8000_0000);
        }
        Instruction::FsgnjxS { .. } => {
            slot.alu_result = slot.rs1_val ^ (slot.rs2_val & 0x8000_0000);
        }
        Instruction::FeqS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = if a == b { 1 } else { 0 };
        }
        Instruction::FltS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = if a < b { 1 } else { 0 };
        }
        Instruction::FleS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            slot.alu_result = if a <= b { 1 } else { 0 };
        }
        Instruction::FcvtWS { .. } => {
            let v = f32::from_bits(slot.rs1_val);
            slot.alu_result = if v.is_nan() {
                i32::MAX as u32
            } else {
                (v.clamp(i32::MIN as f32, i32::MAX as f32) as i32) as u32
            };
        }
        Instruction::FcvtWuS { .. } => {
            let v = f32::from_bits(slot.rs1_val);
            slot.alu_result = if v.is_nan() || v < 0.0 {
                0
            } else if v >= u32::MAX as f32 {
                u32::MAX
            } else {
                v as u32
            };
        }
        Instruction::FcvtSW { .. } => {
            slot.alu_result = (slot.rs1_val as i32 as f32).to_bits();
        }
        Instruction::FcvtSWu { .. } => {
            slot.alu_result = (slot.rs1_val as f32).to_bits();
        }
        Instruction::FmvXW { .. } => {
            slot.alu_result = slot.rs1_val; // float bits → int reg
        }
        Instruction::FmvWX { .. } => {
            slot.alu_result = slot.rs1_val; // int reg → float bits
        }
        Instruction::FclassS { .. } => {
            let bits = slot.rs1_val;
            let exp = (bits >> 23) & 0xFF;
            let mant = bits & 0x007F_FFFF;
            let sign = bits >> 31;
            slot.alu_result = match (sign, exp, mant) {
                (1, 0xFF, m) if m != 0 => 0x100,
                (0, 0xFF, m) if m != 0 => 0x200,
                (1, 0xFF, 0) => 0x001,
                (0, 0xFF, 0) => 0x080,
                (1, 0, 0) => 0x008,
                (0, 0, 0) => 0x010,
                (1, 0, _) => 0x004,
                (0, 0, _) => 0x020,
                (1, _, _) => 0x002,
                (0, _, _) => 0x040,
                _ => 0x000,
            };
        }
        // Fused multiply-add family: rs3 is read at ID and stored in slot.mem_addr
        // (the FP ALU instructions never use mem_addr for actual addressing).
        // Forwarding for rs3 is handled in apply_forwarding_to_id (same field).
        Instruction::FmaddS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let c = f32::from_bits(slot.mem_addr.unwrap_or(0));
            slot.alu_result = (a * b + c).to_bits();
        }
        Instruction::FmsubS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let c = f32::from_bits(slot.mem_addr.unwrap_or(0));
            slot.alu_result = (a * b - c).to_bits();
        }
        Instruction::FnmsubS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let c = f32::from_bits(slot.mem_addr.unwrap_or(0));
            slot.alu_result = (-(a * b) + c).to_bits();
        }
        Instruction::FnmaddS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            let c = f32::from_bits(slot.mem_addr.unwrap_or(0));
            slot.alu_result = (-(a * b) - c).to_bits();
        }

        // ── Atomics: compute address ────────────────────────────────────
        Instruction::LrW { .. } => {
            slot.mem_addr = Some(slot.rs1_val);
        }
        Instruction::ScW { .. }
        | Instruction::AmoswapW { .. }
        | Instruction::AmoaddW { .. }
        | Instruction::AmoxorW { .. }
        | Instruction::AmoandW { .. }
        | Instruction::AmoorW { .. }
        | Instruction::AmomaxW { .. }
        | Instruction::AmominW { .. }
        | Instruction::AmomaxuW { .. }
        | Instruction::AmominuW { .. } => {
            slot.mem_addr = Some(slot.rs1_val);
        }

        // ── System / Fence / Unknown: no EX work ────────────────────────
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegFile {
    Int,
    Float,
}

fn operand_reg_file(instr: Instruction, operand: u8) -> Option<RegFile> {
    use Instruction::*;
    match instr {
        FaddS { .. }
        | FsubS { .. }
        | FmulS { .. }
        | FdivS { .. }
        | FminS { .. }
        | FmaxS { .. }
        | FsgnjS { .. }
        | FsgnjnS { .. }
        | FsgnjxS { .. }
        | FeqS { .. }
        | FltS { .. }
        | FleS { .. } => match operand {
            1 | 2 => Some(RegFile::Float),
            _ => None,
        },
        FmaddS { .. } | FmsubS { .. } | FnmsubS { .. } | FnmaddS { .. } => match operand {
            1 | 2 | 3 => Some(RegFile::Float),
            _ => None,
        },
        FsqrtS { .. } | FmvXW { .. } | FclassS { .. } | FcvtWS { .. } | FcvtWuS { .. } => {
            match operand {
                1 => Some(RegFile::Float),
                _ => None,
            }
        }
        FmvWX { .. } | FcvtSW { .. } | FcvtSWu { .. } | Flw { .. } => match operand {
            1 => Some(RegFile::Int),
            _ => None,
        },
        Fsw { .. } => match operand {
            1 => Some(RegFile::Int),
            2 => Some(RegFile::Float),
            _ => None,
        },
        _ => Some(RegFile::Int),
    }
}

fn slot_result(slot: &PipeSlot) -> Option<(RegFile, u8, u32)> {
    use Instruction::*;
    let instr = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())?;
    match instr {
        Add { rd, .. }
        | Sub { rd, .. }
        | And { rd, .. }
        | Or { rd, .. }
        | Xor { rd, .. }
        | Sll { rd, .. }
        | Srl { rd, .. }
        | Sra { rd, .. }
        | Slt { rd, .. }
        | Sltu { rd, .. }
        | Mul { rd, .. }
        | Mulh { rd, .. }
        | Mulhsu { rd, .. }
        | Mulhu { rd, .. }
        | Div { rd, .. }
        | Divu { rd, .. }
        | Rem { rd, .. }
        | Remu { rd, .. }
        | Addi { rd, .. }
        | Andi { rd, .. }
        | Ori { rd, .. }
        | Xori { rd, .. }
        | Slti { rd, .. }
        | Sltiu { rd, .. }
        | Slli { rd, .. }
        | Srli { rd, .. }
        | Srai { rd, .. }
        | Lui { rd, .. }
        | Auipc { rd, .. }
        | Jal { rd, .. }
        | Jalr { rd, .. }
        | FeqS { rd, .. }
        | FltS { rd, .. }
        | FleS { rd, .. }
        | FcvtWS { rd, .. }
        | FcvtWuS { rd, .. }
        | FmvXW { rd, .. }
        | FclassS { rd, .. }
        | ScW { rd, .. }
        | AmoswapW { rd, .. }
        | AmoaddW { rd, .. }
        | AmoxorW { rd, .. }
        | AmoandW { rd, .. }
        | AmoorW { rd, .. }
        | AmomaxW { rd, .. }
        | AmominW { rd, .. }
        | AmomaxuW { rd, .. }
        | AmominuW { rd, .. } => Some((RegFile::Int, rd, slot.alu_result)),

        Lb { rd, .. }
        | Lh { rd, .. }
        | Lw { rd, .. }
        | Lbu { rd, .. }
        | Lhu { rd, .. }
        | LrW { rd, .. } => slot.mem_result.map(|v| (RegFile::Int, rd, v)),

        Flw { rd, .. }
        | FaddS { rd, .. }
        | FsubS { rd, .. }
        | FmulS { rd, .. }
        | FdivS { rd, .. }
        | FsqrtS { rd, .. }
        | FminS { rd, .. }
        | FmaxS { rd, .. }
        | FsgnjS { rd, .. }
        | FsgnjnS { rd, .. }
        | FsgnjxS { rd, .. }
        | FmaddS { rd, .. }
        | FmsubS { rd, .. }
        | FnmsubS { rd, .. }
        | FnmaddS { rd, .. }
        | FcvtSW { rd, .. }
        | FcvtSWu { rd, .. }
        | FmvWX { rd, .. } => {
            let value = if matches!(instr, Flw { .. }) {
                slot.mem_result?
            } else {
                slot.alu_result
            };
            Some((RegFile::Float, rd, value))
        }

        _ => None,
    }
}

fn slot_destination(slot: &PipeSlot) -> Option<(RegFile, u8)> {
    use Instruction::*;
    let instr = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())?;
    match instr {
        Add { rd, .. }
        | Sub { rd, .. }
        | And { rd, .. }
        | Or { rd, .. }
        | Xor { rd, .. }
        | Sll { rd, .. }
        | Srl { rd, .. }
        | Sra { rd, .. }
        | Slt { rd, .. }
        | Sltu { rd, .. }
        | Mul { rd, .. }
        | Mulh { rd, .. }
        | Mulhsu { rd, .. }
        | Mulhu { rd, .. }
        | Div { rd, .. }
        | Divu { rd, .. }
        | Rem { rd, .. }
        | Remu { rd, .. }
        | Addi { rd, .. }
        | Andi { rd, .. }
        | Ori { rd, .. }
        | Xori { rd, .. }
        | Slti { rd, .. }
        | Sltiu { rd, .. }
        | Slli { rd, .. }
        | Srli { rd, .. }
        | Srai { rd, .. }
        | Lui { rd, .. }
        | Auipc { rd, .. }
        | Jal { rd, .. }
        | Jalr { rd, .. }
        | Lb { rd, .. }
        | Lh { rd, .. }
        | Lw { rd, .. }
        | Lbu { rd, .. }
        | Lhu { rd, .. }
        | FeqS { rd, .. }
        | FltS { rd, .. }
        | FleS { rd, .. }
        | FcvtWS { rd, .. }
        | FcvtWuS { rd, .. }
        | FmvXW { rd, .. }
        | FclassS { rd, .. }
        | LrW { rd, .. }
        | ScW { rd, .. }
        | AmoswapW { rd, .. }
        | AmoaddW { rd, .. }
        | AmoxorW { rd, .. }
        | AmoandW { rd, .. }
        | AmoorW { rd, .. }
        | AmomaxW { rd, .. }
        | AmominW { rd, .. }
        | AmomaxuW { rd, .. }
        | AmominuW { rd, .. } => Some((RegFile::Int, rd)),

        Flw { rd, .. }
        | FaddS { rd, .. }
        | FsubS { rd, .. }
        | FmulS { rd, .. }
        | FdivS { rd, .. }
        | FsqrtS { rd, .. }
        | FminS { rd, .. }
        | FmaxS { rd, .. }
        | FsgnjS { rd, .. }
        | FsgnjnS { rd, .. }
        | FsgnjxS { rd, .. }
        | FmaddS { rd, .. }
        | FmsubS { rd, .. }
        | FnmsubS { rd, .. }
        | FnmaddS { rd, .. }
        | FcvtSW { rd, .. }
        | FcvtSWu { rd, .. }
        | FmvWX { rd, .. } => Some((RegFile::Float, rd)),

        _ => None,
    }
}

fn forward_value(
    reg: Option<u8>,
    reg_file: Option<RegFile>,
    producers: &[Option<PipeSlot>],
) -> Option<u32> {
    let reg = reg?;
    let reg_file = reg_file?;
    if reg == 0 {
        return None;
    }
    for producer in producers.iter().flatten() {
        if producer.is_bubble {
            continue;
        }
        if let Some((prod_file, prod_rd, value)) = slot_result(producer) {
            if prod_rd == reg && prod_file == reg_file {
                return Some(value);
            }
        }
    }
    None
}

fn slot_reads_register(slot: &PipeSlot, reg_file: RegFile, reg: u8) -> bool {
    if slot.is_bubble || reg == 0 {
        return false;
    }
    let instr = match slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    {
        Some(instr) => instr,
        None => return false,
    };

    if matches!(instr, Instruction::Ecall) && reg_file == RegFile::Int && (10..=17).contains(&reg) {
        return true;
    }

    let rs1_match = slot.rs1 == Some(reg) && operand_reg_file(instr, 1) == Some(reg_file);
    let rs2_match = slot.rs2 == Some(reg) && operand_reg_file(instr, 2) == Some(reg_file);
    let rs3_match = match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. } => reg_file == RegFile::Float && rs3 == reg,
        _ => false,
    };

    rs1_match || rs2_match || rs3_match
}

fn slot_has_late_mem_result(slot: &PipeSlot) -> bool {
    if slot.is_bubble {
        return false;
    }
    let Some(instr) = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    else {
        return false;
    };
    matches!(
        instr,
        Instruction::Lb { .. }
            | Instruction::Lh { .. }
            | Instruction::Lw { .. }
            | Instruction::Lbu { .. }
            | Instruction::Lhu { .. }
            | Instruction::Flw { .. }
            | Instruction::LrW { .. }
    )
}

fn slot_has_wb_only_syscall_result(slot: &PipeSlot) -> bool {
    !slot.is_bubble
        && matches!(
            slot.instr
                .or_else(|| crate::falcon::decoder::decode(slot.word).ok()),
            Some(Instruction::Ecall)
        )
}

fn apply_forwarding_to_ex(
    slot: &mut PipeSlot,
    mem_prod: &Option<PipeSlot>,
    wb_prod: &Option<PipeSlot>,
) {
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    let producers = [mem_prod.clone(), wb_prod.clone()];
    if let Some(v) = forward_value(slot.rs1, operand_reg_file(instr, 1), &producers) {
        slot.rs1_val = v;
    }
    if let Some(v) = forward_value(slot.rs2, operand_reg_file(instr, 2), &producers) {
        slot.rs2_val = v;
    }
    if matches!(
        instr,
        Instruction::FmaddS { .. }
            | Instruction::FmsubS { .. }
            | Instruction::FnmsubS { .. }
            | Instruction::FnmaddS { .. }
    ) {
        let rs3 = match instr {
            Instruction::FmaddS { rs3, .. }
            | Instruction::FmsubS { rs3, .. }
            | Instruction::FnmsubS { rs3, .. }
            | Instruction::FnmaddS { rs3, .. } => rs3,
            _ => 0,
        };
        if let Some(v) = forward_value(Some(rs3), operand_reg_file(instr, 3), &producers) {
            slot.mem_addr = Some(v);
        }
    }
}

fn apply_forwarding_to_mem(slot: &mut PipeSlot, wb_prod: &Option<PipeSlot>) {
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    let producers = [wb_prod.clone()];
    if let Some(v) = forward_value(slot.rs2, operand_reg_file(instr, 2), &producers) {
        slot.rs2_val = v;
    }
}

fn apply_forwarding_to_id(
    slot: &mut PipeSlot,
    ex_prod: &Option<PipeSlot>,
    mem_prod: &Option<PipeSlot>,
    wb_prod: &Option<PipeSlot>,
) {
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    let producers = [ex_prod.clone(), mem_prod.clone(), wb_prod.clone()];
    if let Some(v) = forward_value(slot.rs1, operand_reg_file(instr, 1), &producers) {
        slot.rs1_val = v;
    }
    if let Some(v) = forward_value(slot.rs2, operand_reg_file(instr, 2), &producers) {
        slot.rs2_val = v;
    }
    // Fused multiply-add instructions have a third source register (rs3) whose
    // value is stored in slot.mem_addr at ID time.  Forward it like rs1/rs2.
    match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. } => {
            if let Some(v) = forward_value(Some(rs3), Some(RegFile::Float), &producers) {
                slot.mem_addr = Some(v);
            }
        }
        _ => {}
    }
}

fn resolve_control_in_id(slot: &mut PipeSlot) {
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    match instr {
        Instruction::Beq { imm, .. } => {
            slot.branch_taken = slot.rs1_val == slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bne { imm, .. } => {
            slot.branch_taken = slot.rs1_val != slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Blt { imm, .. } => {
            slot.branch_taken = (slot.rs1_val as i32) < (slot.rs2_val as i32);
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bge { imm, .. } => {
            slot.branch_taken = (slot.rs1_val as i32) >= (slot.rs2_val as i32);
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bltu { imm, .. } => {
            slot.branch_taken = slot.rs1_val < slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Bgeu { imm, .. } => {
            slot.branch_taken = slot.rs1_val >= slot.rs2_val;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Jal { imm, .. } => {
            slot.alu_result = slot.pc.wrapping_add(4);
            slot.branch_taken = true;
            slot.branch_target = Some(slot.pc.wrapping_add(imm as u32));
        }
        Instruction::Jalr { imm, .. } => {
            slot.alu_result = slot.pc.wrapping_add(4);
            slot.branch_taken = true;
            slot.branch_target = Some((slot.rs1_val.wrapping_add(imm as u32)) & !1);
        }
        _ => {}
    }
}

// Branch prediction lives in predictor.rs.

/// **MEM stage** — Memory access (loads/stores/atomics).
/// Returns `(latency, faulted)`.  On a bus fault, the error is logged and
/// `faulted=true` is returned so the caller can stop the pipeline.
fn stage_mem(
    slot: &mut PipeSlot,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
) -> (u64, bool) {
    if slot.is_bubble {
        return (0, false);
    }
    let instr = match slot.instr {
        Some(i) => i,
        None => return (0, false),
    };

    let addr = match slot.mem_addr {
        Some(a) => a,
        None => return (0, false), // no memory op for this instruction
    };

    let mut latency = 0u64;
    let mut faulted = false;

    match instr {
        // ── Integer loads ────────────────────────────────────────────────
        Instruction::Lb { .. } => {
            let (result, access_latency) = mem.dcache_read8_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some((v as i8 as i32) as u32),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::Lh { .. } => {
            let (result, access_latency) = mem.dcache_read16_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some((v as i16 as i32) as u32),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::Lw { .. } => {
            let (result, access_latency) = mem.dcache_read32_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some(v),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::Lbu { .. } => {
            let (result, access_latency) = mem.dcache_read8_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some(v as u32),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::Lhu { .. } => {
            let (result, access_latency) = mem.dcache_read16_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some(v as u32),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        // ── FP load ─────────────────────────────────────────────────────
        Instruction::Flw { .. } => {
            let (result, access_latency) = mem.dcache_read32_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => slot.mem_result = Some(v),
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }

        // ── Integer stores ──────────────────────────────────────────────
        Instruction::Sb { .. } => {
            let (result, access_latency) = mem.store8_timed(addr, slot.rs2_val as u8);
            latency += access_latency;
            if let Err(e) = result {
                console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                faulted = true;
            }
        }
        Instruction::Sh { .. } => {
            let (result, access_latency) = mem.store16_timed(addr, slot.rs2_val as u16);
            latency += access_latency;
            if let Err(e) = result {
                console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                faulted = true;
            }
        }
        Instruction::Sw { .. } => {
            let (result, access_latency) = mem.store32_timed(addr, slot.rs2_val);
            latency += access_latency;
            if let Err(e) = result {
                console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                faulted = true;
            }
        }
        // ── FP store ────────────────────────────────────────────────────
        Instruction::Fsw { .. } => {
            let (result, access_latency) = mem.store32_timed(addr, slot.rs2_val);
            latency += access_latency;
            if let Err(e) = result {
                console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                faulted = true;
            }
        }

        // ── Atomics ─────────────────────────────────────────────────────
        Instruction::LrW { .. } => {
            let (result, access_latency) = mem.lr_w_timed(cpu.hart_id, addr);
            latency += access_latency;
            match result {
                Ok(v) => {
                    slot.mem_result = Some(v);
                    cpu.lr_reservation = Some(addr & !0x3);
                }
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::ScW { .. } => {
            let (result, access_latency) = mem.sc_w_timed(cpu.hart_id, addr, slot.rs2_val);
            latency += access_latency;
            match result {
                Ok(true) => slot.alu_result = 0,
                Ok(false) => slot.alu_result = 1,
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
            cpu.lr_reservation = None;
        }
        Instruction::AmoswapW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Swap,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmoaddW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Add,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmoxorW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Xor,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmoandW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::And,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmoorW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Or,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmomaxW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Max,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmominW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::Min,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmomaxuW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::MaxU,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),
        Instruction::AmominuW { .. } => stage_mem_amo(
            mem,
            cpu.hart_id,
            addr,
            AmoOp::MinU,
            slot.rs2_val,
            &mut latency,
            &mut slot.alu_result,
            console,
            &mut faulted,
        ),

        _ => {} // non-memory instructions
    }

    (latency, faulted)
}

/// **WB stage** — Write result to destination register, handle system instructions.
/// Returns `true` if the CPU should keep running (false = halt/ebreak/exit).
fn stage_wb(
    slot: &PipeSlot,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
    cycle_count: u64,
) -> bool {
    let instr = match slot.instr {
        Some(i) => i,
        None => return true,
    };

    match instr {
        // ── ALU R-type → write alu_result to int rd ─────────────────────
        Instruction::Add { rd, .. }
        | Instruction::Sub { rd, .. }
        | Instruction::And { rd, .. }
        | Instruction::Or { rd, .. }
        | Instruction::Xor { rd, .. }
        | Instruction::Sll { rd, .. }
        | Instruction::Srl { rd, .. }
        | Instruction::Sra { rd, .. }
        | Instruction::Slt { rd, .. }
        | Instruction::Sltu { rd, .. }
        | Instruction::Mul { rd, .. }
        | Instruction::Mulh { rd, .. }
        | Instruction::Mulhsu { rd, .. }
        | Instruction::Mulhu { rd, .. }
        | Instruction::Div { rd, .. }
        | Instruction::Divu { rd, .. }
        | Instruction::Rem { rd, .. }
        | Instruction::Remu { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── ALU I-type → write alu_result to int rd ─────────────────────
        Instruction::Addi { rd, .. }
        | Instruction::Andi { rd, .. }
        | Instruction::Ori { rd, .. }
        | Instruction::Xori { rd, .. }
        | Instruction::Slti { rd, .. }
        | Instruction::Sltiu { rd, .. }
        | Instruction::Slli { rd, .. }
        | Instruction::Srli { rd, .. }
        | Instruction::Srai { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── U-type → write alu_result to int rd ─────────────────────────
        Instruction::Lui { rd, .. } | Instruction::Auipc { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── Int loads → write mem_result to int rd ──────────────────────
        Instruction::Lb { rd, .. }
        | Instruction::Lh { rd, .. }
        | Instruction::Lw { rd, .. }
        | Instruction::Lbu { rd, .. }
        | Instruction::Lhu { rd, .. } => {
            if let Some(val) = slot.mem_result {
                cpu.write(rd, val);
            }
        }

        // ── FP load → write mem_result to float rd ──────────────────────
        Instruction::Flw { rd, .. } => {
            if let Some(val) = slot.mem_result {
                cpu.fwrite_bits(rd, val);
            }
        }

        // ── Stores: nothing to write (done at MEM) ──────────────────────
        Instruction::Sb { .. }
        | Instruction::Sh { .. }
        | Instruction::Sw { .. }
        | Instruction::Fsw { .. } => {}

        // ── Jumps → write link address to int rd ────────────────────────
        Instruction::Jal { rd, .. } | Instruction::Jalr { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── Branches: no register write ─────────────────────────────────
        Instruction::Beq { .. }
        | Instruction::Bne { .. }
        | Instruction::Blt { .. }
        | Instruction::Bge { .. }
        | Instruction::Bltu { .. }
        | Instruction::Bgeu { .. } => {}

        // ── FP arithmetic → write alu_result to float rd ────────────────
        Instruction::FaddS { rd, .. }
        | Instruction::FsubS { rd, .. }
        | Instruction::FmulS { rd, .. }
        | Instruction::FdivS { rd, .. }
        | Instruction::FsqrtS { rd, .. }
        | Instruction::FminS { rd, .. }
        | Instruction::FmaxS { rd, .. }
        | Instruction::FsgnjS { rd, .. }
        | Instruction::FsgnjnS { rd, .. }
        | Instruction::FsgnjxS { rd, .. }
        | Instruction::FmaddS { rd, .. }
        | Instruction::FmsubS { rd, .. }
        | Instruction::FnmsubS { rd, .. }
        | Instruction::FnmaddS { rd, .. } => {
            cpu.fwrite_bits(rd, slot.alu_result);
        }

        // ── FP compare → write to int rd ────────────────────────────────
        Instruction::FeqS { rd, .. }
        | Instruction::FltS { rd, .. }
        | Instruction::FleS { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── FP conversions ──────────────────────────────────────────────
        Instruction::FcvtWS { rd, .. } | Instruction::FcvtWuS { rd, .. } => {
            cpu.write(rd, slot.alu_result); // f32 → int
        }
        Instruction::FcvtSW { rd, .. } | Instruction::FcvtSWu { rd, .. } => {
            cpu.fwrite_bits(rd, slot.alu_result); // int → f32
        }
        Instruction::FmvXW { rd, .. } => {
            cpu.write(rd, slot.alu_result); // float bits → int reg
        }
        Instruction::FmvWX { rd, .. } => {
            cpu.fwrite_bits(rd, slot.alu_result); // int reg → float bits
        }
        Instruction::FclassS { rd, .. } => {
            cpu.write(rd, slot.alu_result);
        }

        // ── Atomics → write old value to int rd ─────────────────────────
        Instruction::LrW { rd, .. } => {
            if let Some(val) = slot.mem_result {
                cpu.write(rd, val);
            }
        }
        Instruction::ScW { rd, .. } => {
            cpu.write(rd, slot.alu_result); // 0=success, 1=fail
        }
        Instruction::AmoswapW { rd, .. }
        | Instruction::AmoaddW { rd, .. }
        | Instruction::AmoxorW { rd, .. }
        | Instruction::AmoandW { rd, .. }
        | Instruction::AmoorW { rd, .. }
        | Instruction::AmomaxW { rd, .. }
        | Instruction::AmominW { rd, .. }
        | Instruction::AmomaxuW { rd, .. }
        | Instruction::AmominuW { rd, .. } => {
            cpu.write(rd, slot.alu_result); // old value
        }

        // ── System instructions (handled fully at WB) ───────────────────
        Instruction::Ecall => {
            let code = cpu.read(17); // a7
            match crate::falcon::syscall::handle_syscall_with_cycle_override(
                code,
                cpu,
                mem,
                console,
                Some(cycle_count),
            ) {
                Ok(cont) => {
                    if !cont && console.reading {
                        cpu.pc = slot.pc; // rewind for blocking stdin
                        return false;
                    }
                    if !cont && cpu.exit_code.is_some() {
                        // Keep terminal program-exit syscalls parked on their own ecall.
                        cpu.pc = slot.pc;
                        return false;
                    }
                    if !cont && cpu.local_exit {
                        // Keep terminal hart-exit syscalls parked on their own ecall.
                        cpu.pc = slot.pc;
                        return false;
                    }
                    if !cont {
                        return false;
                    }
                }
                Err(e) => {
                    console.push_error(format!("Syscall error at 0x{:08X}: {e}", slot.pc));
                    return false;
                }
            }
        }
        Instruction::Ebreak | Instruction::Halt => {
            cpu.instr_count += 1;
            if matches!(instr, Instruction::Halt) {
                cpu.local_exit = true;
                cpu.ebreak_hit = false;
                console.push_colored(
                    format!("Halt at 0x{:08X}", slot.pc),
                    crate::ui::console::ConsoleColor::Info,
                );
            } else {
                cpu.ebreak_hit = true;
                cpu.local_exit = false;
                console.push_colored(
                    format!("ebreak at 0x{:08X}", slot.pc),
                    crate::ui::console::ConsoleColor::Warning,
                );
            }
            return false;
        }

        Instruction::Fence | Instruction::FenceI => {} // ordering handled by shared memory backend
    }

    cpu.instr_count += 1;
    true
}

fn stage_mem_amo(
    mem: &mut CacheController,
    hart_id: u32,
    addr: u32,
    op: AmoOp,
    operand: u32,
    latency: &mut u64,
    alu_result: &mut u32,
    console: &mut Console,
    faulted: &mut bool,
) {
    let (result, access_latency) = mem.amo_w_timed(hart_id, addr, op, operand);
    *latency += access_latency;
    match result {
        Ok(old) => *alu_result = old,
        Err(e) => {
            console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
            *faulted = true;
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Commit (WB) ──────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn commit_wb(
    state: &mut PipelineSimState,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
) -> Option<CommitInfo> {
    let slot = match state.stages[Stage::WB as usize].take() {
        Some(s) => s,
        None => return None,
    };

    if slot.is_bubble {
        return None;
    }

    // Set cpu.pc to this instruction's PC (needed for ecall handlers)
    cpu.pc = slot.pc;
    let cycle_count = state.cycle_count;

    let alive = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stage_wb(&slot, cpu, mem, console, cycle_count)
    }));

    let alive = match alive {
        Ok(v) => v,
        Err(_) => {
            console.push_error(format!("Pipeline fault (panic) at 0x{:08X}", slot.pc));
            state.faulted = true;
            false
        }
    };

    if !alive && !console.reading {
        state.halted = true;
    }

    fn committed_next_pc(slot: &PipeSlot) -> u32 {
        let instr = slot
            .instr
            .or_else(|| crate::falcon::decoder::decode(slot.word).ok());
        match instr {
            Some(Instruction::Jal { imm, .. }) => slot.pc.wrapping_add(imm as u32),
            Some(Instruction::Jalr { imm, .. }) => (slot.rs1_val.wrapping_add(imm as u32)) & !1,
            Some(Instruction::Beq { imm, .. }) => {
                if slot.rs1_val == slot.rs2_val {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            Some(Instruction::Bne { imm, .. }) => {
                if slot.rs1_val != slot.rs2_val {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            Some(Instruction::Blt { imm, .. }) => {
                if (slot.rs1_val as i32) < (slot.rs2_val as i32) {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            Some(Instruction::Bge { imm, .. }) => {
                if (slot.rs1_val as i32) >= (slot.rs2_val as i32) {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            Some(Instruction::Bltu { imm, .. }) => {
                if slot.rs1_val < slot.rs2_val {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            Some(Instruction::Bgeu { imm, .. }) => {
                if slot.rs1_val >= slot.rs2_val {
                    slot.pc.wrapping_add(imm as u32)
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
            _ => {
                if slot.branch_taken {
                    slot.branch_target.unwrap_or(slot.pc.wrapping_add(4))
                } else {
                    slot.pc.wrapping_add(4)
                }
            }
        }
    }

    // Update cpu.pc: branch target or next sequential.
    // Terminal local hart exits (ecall-based) stay parked on their own ecall so
    // the Run view highlights the instruction that stopped the hart.
    // halt is a pseudo-instruction: advance PC like sequential mode so cpu.pc
    // is consistent with exec::step (which pre-increments before executing).
    if !alive
        && (cpu.local_exit || cpu.exit_code.is_some())
        && !matches!(slot.instr, Some(Instruction::Halt))
    {
        cpu.pc = slot.pc;
    } else {
        cpu.pc = committed_next_pc(&slot);
    }

    state.instr_committed += 1;
    state.class_counts[slot.class.as_usize()] += 1;
    if matches!(slot.class, InstrClass::Branch | InstrClass::Jump) {
        state.branches_executed += 1;
    }

    Some(CommitInfo {
        pc: slot.pc,
        class: slot.class,
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Branch resolution ────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Flush stages behind the given resolution point and redirect fetch_pc.
// Branch resolution lives in predictor.rs.

// ══════════════════════════════════════════════════════════════════════════════
// ── Hazard detection ─────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn detect_stall(
    state: &mut PipelineSimState,
    wb_before_commit: Option<&PipeSlot>,
) -> Option<(usize, HazardType)> {
    fn requires_architectural_visibility_in_id(slot: &PipeSlot) -> bool {
        matches!(slot.instr, Some(Instruction::Ecall))
    }

    fn wb_commit_is_visible_this_cycle(producer: &PipeSlot) -> bool {
        !producer.is_bubble && !forwarding::slot_has_wb_only_syscall_result(producer)
    }

    // ── Syscall barrier hazards around ABI arg regs a0..a7 ───────────────
    let syscall_abi_hazard: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
        let id = state.stages[Stage::ID as usize].as_ref();
        let ex = state.stages[Stage::EX as usize].as_ref();
        let mem_s = state.stages[Stage::MEM as usize].as_ref();
        let wb = wb_before_commit;
        let mut found = None;

        if let Some(id_s) = id {
            if !id_s.is_bubble {
                for arg_reg in 10..=17u8 {
                    let id_reads_arg = slot_reads_register(id_s, RegFile::Int, arg_reg);
                    if !id_reads_arg {
                        continue;
                    }
                    for (stage_idx, stage_name, producer) in [
                        (Stage::EX as usize, "EX", ex),
                        (Stage::MEM as usize, "MEM", mem_s),
                        (Stage::WB as usize, "WB", wb),
                    ] {
                        let Some(p) = producer else {
                            continue;
                        };
                        if forwarding::slot_has_wb_only_syscall_result(p) {
                            found = Some((
                                HazardType::Raw,
                                format!(
                                    "RAW: ID reads {} — ecall still owns ABI arg/result regs in {stage_name}",
                                    reg_name(arg_reg)
                                ),
                                Some((
                                    stage_idx,
                                    Stage::ID as usize,
                                    format!(
                                        "{} -> {}",
                                        p.disasm.split_whitespace().next().unwrap_or("?"),
                                        id_s.disasm.split_whitespace().next().unwrap_or("?")
                                    ),
                                )),
                            ));
                            break;
                        }
                        if matches!(id_s.instr, Some(Instruction::Ecall)) {
                            if let Some((prod_file, prod_rd)) = forwarding::slot_destination(p) {
                                if prod_file == forwarding::RegFile::Int && prod_rd == arg_reg {
                                    found = Some((
                                        HazardType::Raw,
                                        format!(
                                            "RAW: ecall reads {} — value still pending in {stage_name}",
                                            reg_name(arg_reg)
                                        ),
                                        Some((
                                            stage_idx,
                                            Stage::ID as usize,
                                            format!(
                                                "{} -> {}",
                                                p.disasm.split_whitespace().next().unwrap_or("?"),
                                                id_s.disasm
                                                    .split_whitespace()
                                                    .next()
                                                    .unwrap_or("?")
                                            ),
                                        )),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                    // Also check fu_bank producers in FunctionalUnits mode
                    if found.is_none()
                        && matches!(state.mode, super::PipelineMode::FunctionalUnits)
                    {
                        'fu_abi: for fu_group in &state.fu_bank {
                            for fu in fu_group {
                                let Some(p) = fu.slot.as_ref() else { continue };
                                if p.is_bubble { continue; }
                                let fu_label = fu.kind.map_or("FU", |k| k.label());
                                if forwarding::slot_has_wb_only_syscall_result(p) {
                                    found = Some((
                                        HazardType::Raw,
                                        format!(
                                            "RAW: ID reads {} — ecall still owns ABI arg/result regs in {fu_label}",
                                            reg_name(arg_reg)
                                        ),
                                        Some((
                                            Stage::EX as usize,
                                            Stage::ID as usize,
                                            format!(
                                                "{} -> {}",
                                                p.disasm.split_whitespace().next().unwrap_or("?"),
                                                id_s.disasm.split_whitespace().next().unwrap_or("?")
                                            ),
                                        )),
                                    ));
                                    break 'fu_abi;
                                }
                                if matches!(id_s.instr, Some(Instruction::Ecall)) {
                                    if let Some((prod_file, prod_rd)) = forwarding::slot_destination(p) {
                                        if prod_file == forwarding::RegFile::Int && prod_rd == arg_reg {
                                            found = Some((
                                                HazardType::Raw,
                                                format!(
                                                    "RAW: ecall reads {} — value still pending in {fu_label}",
                                                    reg_name(arg_reg)
                                                ),
                                                Some((
                                                    Stage::EX as usize,
                                                    Stage::ID as usize,
                                                    format!(
                                                        "{} -> {}",
                                                        p.disasm.split_whitespace().next().unwrap_or("?"),
                                                        id_s.disasm.split_whitespace().next().unwrap_or("?")
                                                    ),
                                                )),
                                            ));
                                            break 'fu_abi;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
        }

        found
    };
    if let Some((hazard, msg, trace)) = syscall_abi_hazard {
        state.hazard_msgs.push((hazard, msg));
        if let Some((from_idx, to_idx, detail)) = trace {
            push_trace(
                state,
                TraceKind::Hazard(HazardType::Raw),
                from_idx,
                to_idx,
                detail,
            );
        }
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            s.hazard = Some(hazard);
        }
        return Some((Stage::ID as usize, hazard));
    }

    // ── Syscall-result hazard (ecall writes ABI regs only at WB commit) ──
    let syscall_result_hazard: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
        let id = state.stages[Stage::ID as usize].as_ref();
        let ex = state.stages[Stage::EX as usize].as_ref();
        let mem_s = state.stages[Stage::MEM as usize].as_ref();
        let wb = wb_before_commit;
        let mut found = None;

        if let Some(id_s) = id {
            if !id_s.is_bubble {
                for arg_reg in 10..=17u8 {
                    if !forwarding::slot_reads_register(id_s, forwarding::RegFile::Int, arg_reg) {
                        continue;
                    }
                    for (stage_idx, stage_name, producer) in [
                        (Stage::EX as usize, "EX", ex),
                        (Stage::MEM as usize, "MEM", mem_s),
                        (Stage::WB as usize, "WB", wb),
                    ] {
                        if let Some(p) = producer {
                            if forwarding::slot_has_wb_only_syscall_result(p) {
                                found = Some((
                                    HazardType::Raw,
                                    format!(
                                        "RAW: ID reads {} — syscall result from ecall still pending in {stage_name}",
                                        reg_name(arg_reg)
                                    ),
                                    Some((
                                        stage_idx,
                                        Stage::ID as usize,
                                        format!(
                                            "{} -> {}",
                                            p.disasm.split_whitespace().next().unwrap_or("?"),
                                            id_s.disasm.split_whitespace().next().unwrap_or("?")
                                        ),
                                    )),
                                ));
                                break;
                            }
                        }
                    }
                    // Also check fu_bank in FunctionalUnits mode
                    if found.is_none()
                        && matches!(state.mode, super::PipelineMode::FunctionalUnits)
                    {
                        'fu_sr: for fu_group in &state.fu_bank {
                            for fu in fu_group {
                                let Some(p) = fu.slot.as_ref() else { continue };
                                if p.is_bubble { continue; }
                                if forwarding::slot_has_wb_only_syscall_result(p) {
                                    let fu_label = fu.kind.map_or("FU", |k| k.label());
                                    found = Some((
                                        HazardType::Raw,
                                        format!(
                                            "RAW: ID reads {} — syscall result from ecall still pending in {fu_label}",
                                            reg_name(arg_reg)
                                        ),
                                        Some((
                                            Stage::EX as usize,
                                            Stage::ID as usize,
                                            format!(
                                                "{} -> {}",
                                                p.disasm.split_whitespace().next().unwrap_or("?"),
                                                id_s.disasm.split_whitespace().next().unwrap_or("?")
                                            ),
                                        )),
                                    ));
                                    break 'fu_sr;
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
        }

        found
    };
    if let Some((hazard, msg, trace)) = syscall_result_hazard {
        state.hazard_msgs.push((hazard, msg));
        if let Some((from_idx, to_idx, detail)) = trace {
            push_trace(
                state,
                TraceKind::Hazard(HazardType::Raw),
                from_idx,
                to_idx,
                detail,
            );
        }
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            s.hazard = Some(hazard);
        }
        return Some((Stage::ID as usize, hazard));
    }

    // ── Load-use hazard ───────────────────────────────────────────────────
    let load_use_result: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
        let id = state.stages[Stage::ID as usize].as_ref();
        let ex = state.stages[Stage::EX as usize].as_ref();
        let mut result = None;
        // Classic EX-stage load-use (serialized mode and any mode where load is in EX)
        if let (Some(id_s), Some(ex_s)) = (id, ex) {
            if state.bypass.mem_to_ex
                && !id_s.is_bubble
                && forwarding::slot_has_late_mem_result(ex_s)
            {
                if let Some((prod_file, ex_rd)) = forwarding::slot_destination(ex_s) {
                    if forwarding::slot_reads_register(id_s, prod_file, ex_rd) {
                        result = Some((
                            HazardType::LoadUse,
                            format!("load-use: ID uses {} written by load in EX", reg_name(ex_rd)),
                            Some((
                                Stage::EX as usize,
                                Stage::ID as usize,
                                format!(
                                    "{} -> {}",
                                    ex_s.disasm.split_whitespace().next().unwrap_or("?"),
                                    id_s.disasm.split_whitespace().next().unwrap_or("?")
                                ),
                            )),
                        ));
                    }
                }
            }
        }
        // In FunctionalUnits mode, loads live in the LSU fu_bank, not in EX.
        // Detect load-use when a load in LSU (fu_cycles_left == 1, not yet at MEM)
        // is the producer of a register read by the ID instruction.
        if result.is_none() && matches!(state.mode, super::PipelineMode::FunctionalUnits) {
            if let Some(id_s) = id {
                if !id_s.is_bubble {
                    'lsu_lu: for fu in &state.fu_bank[FuKind::Lsu.index()] {
                        let Some(lsu_slot) = fu.slot.as_ref() else { continue };
                        if lsu_slot.is_bubble || !forwarding::slot_has_late_mem_result(lsu_slot) {
                            continue;
                        }
                        // Load in LSU with exactly 1 cycle left: value only available after MEM
                        if lsu_slot.fu_cycles_left <= 1 {
                            if let Some((prod_file, lsu_rd)) = forwarding::slot_destination(lsu_slot) {
                                if forwarding::slot_reads_register(id_s, prod_file, lsu_rd) {
                                    result = Some((
                                        HazardType::LoadUse,
                                        format!(
                                            "load-use: ID uses {} written by load in LSU (awaiting MEM)",
                                            reg_name(lsu_rd)
                                        ),
                                        Some((
                                            Stage::EX as usize,
                                            Stage::ID as usize,
                                            format!(
                                                "{} -> {}",
                                                lsu_slot.disasm.split_whitespace().next().unwrap_or("?"),
                                                id_s.disasm.split_whitespace().next().unwrap_or("?")
                                            ),
                                        )),
                                    ));
                                    break 'lsu_lu;
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    };
    if let Some((hazard, msg, trace)) = load_use_result {
        state.hazard_msgs.push((hazard, msg));
        if let Some((from_idx, to_idx, detail)) = trace {
            push_trace(
                state,
                TraceKind::Hazard(HazardType::LoadUse),
                from_idx,
                to_idx,
                detail,
            );
        }
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            s.hazard = Some(hazard);
        }
        return Some((Stage::ID as usize, hazard));
    }

    // ── RAW path-aware stall logic ───────────────────────────────────────
    let raw_result: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
        let id = state.stages[Stage::ID as usize].as_ref();
        let ex = state.stages[Stage::EX as usize].as_ref();
        let mem_s = state.stages[Stage::MEM as usize].as_ref();
        let wb = wb_before_commit;
        let mut found = None;
        if let Some(id_s) = id {
            if !id_s.is_bubble {
                let control_resolves_in_id =
                    matches!(state.branch_resolve, super::BranchResolve::Id)
                        && matches!(id_s.class, InstrClass::Branch | InstrClass::Jump);
                let architectural_id_consumer = requires_architectural_visibility_in_id(id_s);
                let id_dispatches_to_parallel_fu =
                    matches!(state.mode, super::PipelineMode::FunctionalUnits)
                        && parallel_fu_kind_for_slot(id_s).is_some();

                if matches!(state.mode, super::PipelineMode::FunctionalUnits) {
                    for group in &state.fu_bank {
                        for fu in group {
                            let Some(p) = fu.slot.as_ref() else {
                                continue;
                            };
                            if p.is_bubble {
                                continue;
                            }
                            let Some((prod_file, p_rd)) = forwarding::slot_destination(p) else {
                                continue;
                            };
                            if p_rd == 0 || !forwarding::slot_reads_register(id_s, prod_file, p_rd)
                            {
                                continue;
                            }
                            let store_data_consumer =
                                forwarding::slot_reads_store_data_register(id_s, prod_file, p_rd);
                            let producer_ready_for_id = p.fu_cycles_left <= 1
                                && forwarding::slot_result(p).is_some()
                                && state.bypass.wb_to_id
                                && !control_resolves_in_id
                                && !architectural_id_consumer
                                && !store_data_consumer;
                            if producer_ready_for_id {
                                continue;
                            }
                            let consumer_name =
                                id_s.disasm.split_whitespace().next().unwrap_or("?");
                            let producer_name = p.disasm.split_whitespace().next().unwrap_or("?");
                            found = Some((
                                HazardType::Raw,
                                format!(
                                    "RAW: ID reads {} — {producer_name} is still active in {}",
                                    reg_name(p_rd),
                                    fu.kind.unwrap_or(FuKind::Alu).label(),
                                ),
                                Some((
                                    Stage::EX as usize,
                                    Stage::ID as usize,
                                    format!("{producer_name} -> {consumer_name}"),
                                )),
                            ));
                            break;
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
                if found.is_none() {
                    'outer: for (stage_idx, stage_name, producer) in [
                        (Stage::EX as usize, "EX", ex),
                        (Stage::MEM as usize, "MEM", mem_s),
                        (Stage::WB as usize, "WB", wb),
                    ] {
                        let Some(p) = producer else {
                            continue;
                        };
                        if p.is_bubble {
                            continue;
                        }
                        let Some((prod_file, p_rd)) = forwarding::slot_destination(p) else {
                            continue;
                        };
                        if p_rd == 0 || !forwarding::slot_reads_register(id_s, prod_file, p_rd) {
                            continue;
                        }

                        let store_data_consumer =
                            forwarding::slot_reads_store_data_register(id_s, prod_file, p_rd);

                        let blocked = if control_resolves_in_id
                            || architectural_id_consumer
                            || store_data_consumer
                        {
                            if stage_idx == Stage::WB as usize {
                                !wb_commit_is_visible_this_cycle(p)
                            } else {
                                true
                            }
                        } else if id_dispatches_to_parallel_fu {
                            if stage_idx == Stage::WB as usize {
                                !state.bypass.wb_to_id || !wb_commit_is_visible_this_cycle(p)
                            } else {
                                true
                            }
                        } else if stage_idx == Stage::EX as usize {
                            forwarding::slot_has_late_mem_result(p) || !state.bypass.ex_to_ex
                        } else if stage_idx == Stage::MEM as usize {
                            !state.bypass.mem_to_ex
                        } else {
                            !state.bypass.wb_to_id
                        };

                        if blocked {
                            let consumer_name =
                                id_s.disasm.split_whitespace().next().unwrap_or("?");
                            let producer_name = p.disasm.split_whitespace().next().unwrap_or("?");
                            let detail_msg = if matches!(
                                id_s.class,
                                InstrClass::Branch | InstrClass::Jump
                            ) {
                                format!(
                                    "RAW: {consumer_name} in ID needs {} before its control target/decision is trustworthy; {producer_name} is still producing it in {stage_name}",
                                    reg_name(p_rd),
                                )
                            } else {
                                format!(
                                    "RAW: ID reads {} — {producer_name} result from {stage_name} is not reachable with current bypass config",
                                    reg_name(p_rd),
                                )
                            };
                            found = Some((
                                HazardType::Raw,
                                detail_msg,
                                Some((
                                    stage_idx,
                                    Stage::ID as usize,
                                    format!(
                                        "{} -> {}",
                                        p.disasm.split_whitespace().next().unwrap_or("?"),
                                        id_s.disasm.split_whitespace().next().unwrap_or("?")
                                    ),
                                )),
                            ));
                            break 'outer;
                        }
                    }
                }
            }
        }
        found
    };
    if let Some((hazard, msg, trace)) = raw_result {
        state.hazard_msgs.push((hazard, msg));
        if let Some((from_idx, to_idx, detail)) = trace {
            push_trace(
                state,
                TraceKind::Hazard(HazardType::Raw),
                from_idx,
                to_idx,
                detail,
            );
        }
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            s.hazard = Some(hazard);
        }
        return Some((Stage::ID as usize, hazard));
    }

    // ── Branch stall (informational) ──────────────────────────────────────
    let has_branch_in_flight = state.stages[Stage::IF as usize]
        .as_ref()
        .map_or(false, |s| {
            !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
        })
        || state.stages[Stage::ID as usize]
            .as_ref()
            .map_or(false, |s| {
                !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
            })
        || state.stages[Stage::EX as usize]
            .as_ref()
            .map_or(false, |s| {
                !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
            })
        || state.stages[Stage::MEM as usize]
            .as_ref()
            .map_or(false, |s| {
                !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
            });

    if has_branch_in_flight {
        if let Some(id_s) = state.stages[Stage::ID as usize].as_ref() {
            if !id_s.is_bubble && matches!(id_s.class, InstrClass::Branch | InstrClass::Jump) {
                let depth = state.branch_resolve.flush_depth();
                let msg = format!(
                    "CTL: {} at 0x{:04X} resolves at {} (+{} cycle{} of unresolved control speculation before a redirect/flush is known)",
                    id_s.disasm.split_whitespace().next().unwrap_or("?"),
                    id_s.pc,
                    state
                        .branch_resolve
                        .label()
                        .split_whitespace()
                        .next()
                        .unwrap_or("?"),
                    depth,
                    if depth == 1 { "" } else { "s" },
                );
                state.hazard_msgs.push((HazardType::BranchFlush, msg));
            }
        }
    }

    None
}

// ══════════════════════════════════════════════════════════════════════════════
// ── WAW / WAR hazard detection (informational, no stalls) ────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn detect_name_hazards(state: &mut PipelineSimState) {
    // Collect writers: (stage_index, reg_file, rd, mnemonic, display_label)
    // stage_index is used for ordering (higher = closer to WB = older);
    // display_label is shown in messages and may differ (e.g., "MUL" instead of "EX").
    let mut writers: Vec<(usize, RegFile, u8, String, String)> = state
        .stages
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let s = s.as_ref()?;
            if s.is_bubble {
                return None;
            }
            let (rf, rd) = slot_destination(s)?;
            if rd == 0 && rf == RegFile::Int {
                return None;
            }
            let mnem = s
                .disasm
                .split_whitespace()
                .next()
                .unwrap_or("?")
                .to_string();
            let label = Stage::all()[i].label().to_string();
            Some((i, rf, rd, mnem, label))
        })
        .collect();
    writers.extend(
        state
            .fu_bank
            .iter()
            .flat_map(|group| group.iter())
            .filter_map(|fu| {
                let s = fu.slot.as_ref()?;
                if s.is_bubble {
                    return None;
                }
                let (rf, rd) = slot_destination(s)?;
                if rd == 0 && rf == RegFile::Int {
                    return None;
                }
                let mnem = s
                    .disasm
                    .split_whitespace()
                    .next()
                    .unwrap_or("?")
                    .to_string();
                let label = fu.kind.map_or("FU", |k| k.label()).to_string();
                Some((Stage::EX as usize, rf, rd, mnem, label))
            }),
    );

    // Collect readers: (stage_index, mnemonic, slot clone, display_label)
    let mut readers: Vec<(usize, String, PipeSlot, String)> = state
        .stages
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let s = s.as_ref()?;
            if s.is_bubble {
                return None;
            }
            let mnem = s
                .disasm
                .split_whitespace()
                .next()
                .unwrap_or("?")
                .to_string();
            let label = Stage::all()[i].label().to_string();
            Some((i, mnem, s.clone(), label))
        })
        .collect();
    readers.extend(
        state
            .fu_bank
            .iter()
            .flat_map(|group| group.iter())
            .filter_map(|fu| {
                let s = fu.slot.as_ref()?;
                if s.is_bubble {
                    return None;
                }
                let mnem = s
                    .disasm
                    .split_whitespace()
                    .next()
                    .unwrap_or("?")
                    .to_string();
                let label = fu.kind.map_or("FU", |k| k.label()).to_string();
                Some((Stage::EX as usize, mnem, s.clone(), label))
            }),
    );

    // WAW: two in-flight instructions writing to the same (reg_file, rd) pair
    let mut waw_tags: Vec<usize> = Vec::new();
    for i in 0..writers.len() {
        for j in (i + 1)..writers.len() {
            if writers[i].1 == writers[j].1 && writers[i].2 == writers[j].2 {
                let dest_name = if writers[j].1 == RegFile::Float {
                    format!("f{}", writers[j].2)
                } else {
                    reg_name(writers[j].2).to_string()
                };
                let msg = format!(
                    "WAW: {} in {} and {} in {} both write {}",
                    writers[j].3,
                    writers[j].4,
                    writers[i].3,
                    writers[i].4,
                    dest_name,
                );
                state.hazard_msgs.push((HazardType::Waw, msg));
                push_trace(
                    state,
                    TraceKind::Hazard(HazardType::Waw),
                    writers[j].0,
                    writers[i].0,
                    format!("{} = {}", writers[j].3, dest_name),
                );
                waw_tags.push(writers[j].0);
            }
        }
    }
    for idx in waw_tags {
        if let Some(ref mut s) = state.stages[idx] {
            if s.hazard.is_none() {
                s.hazard = Some(HazardType::Waw);
            }
        }
    }

    // WAR: younger instruction (closer to IF) writes rd that an older
    // instruction (closer to WB) still reads as rs1/rs2.
    let mut war_tags: Vec<usize> = Vec::new();
    for &(w_idx, w_rf, w_rd, ref w_name, ref w_label) in &writers {
        for &(r_idx, ref r_name, ref reader_slot, ref r_label) in &readers {
            if r_idx <= w_idx {
                continue;
            } // reader must be older (closer to WB)
            if slot_reads_register(reader_slot, w_rf, w_rd) {
                let dest_name = if w_rf == RegFile::Float {
                    format!("f{}", w_rd)
                } else {
                    reg_name(w_rd).to_string()
                };
                let msg = format!(
                    "WAR: {} in {} writes {} read by {} in {}",
                    w_name,
                    w_label,
                    dest_name,
                    r_name,
                    r_label,
                );
                state.hazard_msgs.push((HazardType::War, msg));
                push_trace(
                    state,
                    TraceKind::Hazard(HazardType::War),
                    w_idx,
                    r_idx,
                    format!("{} -> {}", w_name, r_name),
                );
                war_tags.push(w_idx);
            }
        }
    }
    for idx in war_tags {
        if let Some(ref mut s) = state.stages[idx] {
            if s.hazard.is_none() {
                s.hazard = Some(HazardType::War);
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Stage advance ────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn advance_stages(
    state: &mut PipelineSimState,
    id_cpu: &Cpu,
    wb_before_commit: Option<&PipeSlot>,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    cpi: &CpiConfig,
    console: &mut Console,
) {
    let wb_producer_before = wb_before_commit.cloned();
    let had_non_if_slots_before = state.stages[Stage::ID as usize..=Stage::WB as usize]
        .iter()
        .flatten()
        .any(|slot| !slot.is_bubble);
    let if_cache_stall = state.stages[Stage::IF as usize]
        .as_ref()
        .map_or(false, |s| !s.is_bubble && s.if_stall_cycles > 0);

    // ── MEM stall: cache miss latency ────────────────────────────────────
    let mem_cache_stall = state.stages[Stage::MEM as usize]
        .as_ref()
        .map_or(false, |s| !s.is_bubble && s.mem_stall_cycles > 0);

    if mem_cache_stall {
        if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
            s.mem_stall_cycles -= 1;
            s.hazard = Some(HazardType::MemLatency);
        }
        if if_cache_stall {
            if let Some(ref mut s) = state.stages[Stage::IF as usize] {
                s.hazard = Some(HazardType::MemLatency);
            }
        }
        state.stall_count += 1;
        state.stall_by_type[HazardType::MemLatency.as_stall_index().unwrap()] += 1;
        state.stages[4] = None; // WB stays empty; MEM does not advance
        advance_parallel_fu_banks(state);
        promote_ready_lsu_to_mem(state);
        promote_ready_fu_result_to_wb(state);
        refresh_fu_busy(state);
        state.last_cycle_cache_only = true;
        return;
    }

    if if_cache_stall {
        if let Some(ref mut s) = state.stages[Stage::IF as usize] {
            s.if_stall_cycles -= 1;
            s.hazard = Some(HazardType::MemLatency);
        }
    }

    let ready_fu_producers_before = forwarding::ready_fu_producers(state);
    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        stage_id(s, id_cpu);
        forwarding::apply_forwarding_to_id(
            s,
            state.bypass,
            &wb_producer_before,
            &ready_fu_producers_before,
        );
    }

    // ── BranchResolve::Id — resolve before the branch leaves ID ──────────
    // Must happen here, while the branch is still in stages[ID].  After the
    // advance below it moves to EX and stages[ID] holds a different instruction.
    if matches!(state.branch_resolve, BranchResolve::Id) {
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            resolve_control_in_id(s);
        }
        predictor::resolve_branch(state, Stage::ID as usize);
    }

    let ex_fu_busy = state.stages[Stage::EX as usize]
        .as_ref()
        .is_some_and(|s| !s.is_bubble && s.fu_cycles_left > 1);
    let mem_commit_blocked = !mem_slot_can_advance_to_wb(state);

    if mem_commit_blocked {
        advance_parallel_fu_banks(state);
        let lsu_promoted = promote_ready_lsu_to_mem(state);
        if lsu_promoted {
            if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
                if !s.is_bubble {
                    let (latency, fault) = stage_mem(s, cpu, mem, console);
                    s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
                    if fault {
                        state.faulted = true;
                    }
                }
            }
        }
        promote_ready_fu_result_to_wb(state);
        // This stall is due to commit-ordering (MEM slot is not the oldest in-flight),
        // not a true FU-capacity stall. Count only in the total; no subtype increment.
        state.stall_count += 1;
        refresh_fu_busy(state);
        return;
    }

    if ex_fu_busy {
        advance_while_ex_busy(state);
        advance_parallel_fu_banks(state);
        let lsu_promoted = promote_ready_lsu_to_mem(state);
        if lsu_promoted {
            if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
                if !s.is_bubble {
                    let (latency, fault) = stage_mem(s, cpu, mem, console);
                    s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
                    if fault {
                        state.faulted = true;
                    }
                }
            }
        }
        promote_ready_fu_result_to_wb(state);
        if matches!(state.mode, super::PipelineMode::FunctionalUnits) {
            // Recompute ready producers after fu_bank advanced — do NOT call stage_id
            // again (it would overwrite forwarded operand values with stale cpu regs).
            let ready_fu_producers = forwarding::ready_fu_producers(state);
            let wb_producer = state.stages[Stage::WB as usize].clone();
            if let Some(ref mut s) = state.stages[Stage::ID as usize] {
                forwarding::apply_forwarding_to_id(
                    s,
                    state.bypass,
                    &wb_producer,
                    &ready_fu_producers,
                );
            }
            let dispatched = dispatch_parallel_fu_from_id(state, cpi);
            if dispatched {
                refill_id_from_if(state, if_cache_stall);
            }
        }
        if if_cache_stall {
            tick_if_cache_latency(state);
            state.stall_by_type[HazardType::MemLatency.as_stall_index().unwrap()] += 1;
        }
        state.stall_count += 1;
        state.stall_by_type[HazardType::FuBusy.as_stall_index().unwrap()] += 1;
        refresh_fu_busy(state);
        return;
    }

    // ── Normal advance / IF cache hold ───────────────────────────────────
    state.stages[4] = state.stages[3].take(); // MEM → WB
    state.stages[3] = state.stages[2].take(); // EX  → MEM
    dispatch_id_to_execution(state, cpi);
    if state.stages[Stage::ID as usize].is_none() {
        refill_id_from_if(state, if_cache_stall);
    } else {
        mark_parallel_fu_capacity_hazard(state);
    }
    for s in state.stages.iter_mut().flatten() {
        if !s.is_bubble {
            s.hazard = None;
        }
    }
    if if_cache_stall {
        if let Some(ref mut s) = state.stages[Stage::IF as usize] {
            s.hazard = Some(HazardType::MemLatency);
        }
        state.stall_count += 1;
        state.stall_by_type[HazardType::MemLatency.as_stall_index().unwrap()] += 1;
        state.last_cycle_cache_only = !had_non_if_slots_before;
    }
    advance_parallel_fu_banks(state);
    let _ = promote_ready_lsu_to_mem(state);
    promote_ready_fu_result_to_wb(state);
    refresh_fu_busy(state);

    let mem_producer = state.stages[Stage::MEM as usize].clone();
    let wb_producer = state.stages[Stage::WB as usize].clone();
    let wb_or_just_committed_producer = wb_producer
        .clone()
        .or_else(|| wb_before_commit.cloned().filter(|slot| !slot.is_bubble));

    // ── Execute per-stage work on newly placed slots ─────────────────────
    let ready_fu_producers = forwarding::ready_fu_producers(state);
    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        stage_id(s, id_cpu);
        forwarding::apply_forwarding_to_id(s, state.bypass, &wb_producer, &ready_fu_producers);
    }
    predictor::apply_branch_prediction(state);

    if !state.halted && state.stages[Stage::IF as usize].is_none() {
        // In sequential mode only fetch the next instruction once all
        // in-flight stages are empty (i.e. the previous instruction has
        // fully committed from WB).
        let ok_to_fetch = !state.sequential_mode
            || state.stages[1..].iter().all(|s| s.as_ref().map_or(true, |slot| slot.is_bubble));
        if ok_to_fetch {
            fetch_into_if(state, mem, console);
        }
    }
    if let Some(ref mut s) = state.stages[Stage::EX as usize] {
        if let Some(committed) = wb_before_commit.as_ref().filter(|slot| !slot.is_bubble) {
            apply_just_committed_visibility_to_ex(s, committed);
        }
        forwarding::apply_forwarding_to_id(
            s,
            state.bypass,
            &wb_or_just_committed_producer,
            &ready_fu_producers,
        );
        forwarding::apply_forwarding_to_ex(
            s,
            state.bypass,
            &mem_producer,
            &wb_or_just_committed_producer,
        );
        stage_ex(s);
        // Handle fused multiply-add rs3 reading (was done at ID, stored in mem_addr)
        // This is already handled in stage_id via special code below
    }
    let mut mem_faulted = false;
    let mut store_load_trace: Option<(usize, String, String)> = None;
    if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
        if let Some(committed) = wb_before_commit.as_ref().filter(|slot| !slot.is_bubble) {
            apply_just_committed_visibility_to_mem(s, committed);
        }
        let store_to_load = forwarding::try_store_to_load_forward(s, state.bypass, &wb_producer);
        let (latency, fault) = stage_mem(s, cpu, mem, console);
        if let Some(forwarded) = store_to_load {
            s.mem_result = Some(forwarded);
        }
        s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
        mem_faulted = fault;
        if store_to_load.is_some() {
            if let Some(prod) = wb_producer.as_ref().filter(|prod| !prod.is_bubble) {
                let prod_name = prod.disasm.split_whitespace().next().unwrap_or("?");
                let consumer_name = s.disasm.split_whitespace().next().unwrap_or("?");
                store_load_trace = Some((
                    Stage::WB as usize,
                    format!(
                        "WB:{} -> MEM:{} ({})",
                        prod_name, consumer_name, "Store->Load"
                    ),
                    format!(
                        "BYPASS: memory value via {} into MEM:{} [RAW covered]",
                        forwarding::BypassPath::StoreToLoad.label(),
                        consumer_name,
                    ),
                ));
            }
        }
    }
    if let Some((from_stage, detail, msg)) = store_load_trace {
        push_trace(
            state,
            TraceKind::Forward,
            from_stage,
            Stage::MEM as usize,
            detail,
        );
        state.hazard_msgs.push((HazardType::Raw, msg));
    }
    if mem_faulted {
        state.faulted = true;
    }

    // ── Branch resolution ────────────────────────────────────────────────
    use super::BranchResolve;
    match state.branch_resolve {
        BranchResolve::Id => {
            // Already resolved before the pipeline advanced (see above).
        }
        BranchResolve::Ex => {
            predictor::resolve_branch(state, Stage::EX as usize);
        }
        BranchResolve::Mem => {
            predictor::resolve_branch(state, Stage::MEM as usize);
        }
    }
}

fn advance_while_ex_busy(state: &mut PipelineSimState) {
    if mem_slot_can_advance_to_wb(state) {
        state.stages[4] = state.stages[3].take(); // MEM -> WB still progresses
    } else {
        state.stages[4] = None;
    }
    state.stages[3] = None; // EX remains occupied; nothing new reaches MEM
    if let Some(ref mut s) = state.stages[Stage::EX as usize] {
        s.fu_cycles_left = s.fu_cycles_left.saturating_sub(1);
        s.hazard = None;
    }
    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        if !s.is_bubble {
            s.hazard = Some(HazardType::FuBusy);
        }
    }
    if let Some(ex_slot) = state.stages[Stage::EX as usize]
        .as_ref()
        .filter(|s| !s.is_bubble)
    {
        state.hazard_msgs.push((
            HazardType::FuBusy,
            format!(
                "FU busy: {} remains in EX for {} more cycle{}",
                ex_slot.disasm.split_whitespace().next().unwrap_or("?"),
                ex_slot.fu_cycles_left,
                if ex_slot.fu_cycles_left == 1 { "" } else { "s" },
            ),
        ));
    }
}

fn dispatch_id_to_execution(state: &mut PipelineSimState, cpi: &CpiConfig) {
    match state.mode {
        super::PipelineMode::SingleCycle => dispatch_id_to_execution_serialized(state, cpi),
        super::PipelineMode::FunctionalUnits => {
            let id_parallel_eligible = state.stages[Stage::ID as usize]
                .as_ref()
                .is_some_and(|slot| !slot.is_bubble && parallel_fu_kind_for_slot(slot).is_some());
            if !dispatch_parallel_fu_from_id(state, cpi) && !id_parallel_eligible {
                dispatch_id_to_execution_serialized(state, cpi);
            }
        }
    }
}

fn dispatch_id_to_execution_serialized(state: &mut PipelineSimState, cpi: &CpiConfig) {
    state.stages[2] = state.stages[1].take(); // ID → EX
    if let Some(ref mut s) = state.stages[Stage::EX as usize] {
        if !s.is_bubble {
            let lat = fu_latency_for_class(s.class, cpi);
            s.fu_cycles_left = lat;
            if let Some(idx) = fu_type_idx(s.class) {
                state.fu_busy[idx] = lat.saturating_sub(1);
            }
        }
    }
}

fn apply_just_committed_visibility_to_ex(slot: &mut PipeSlot, committed: &PipeSlot) {
    let Some(instr) = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    else {
        return;
    };
    let Some((prod_file, prod_rd, value)) = forwarding::slot_result(committed) else {
        return;
    };
    if prod_rd == 0 && prod_file == forwarding::RegFile::Int {
        return;
    }
    if slot.rs1 == Some(prod_rd) && forwarding::operand_reg_file(instr, 1) == Some(prod_file) {
        slot.rs1_val = value;
    }
    if slot.rs2 == Some(prod_rd) && forwarding::operand_reg_file(instr, 2) == Some(prod_file) {
        slot.rs2_val = value;
    }
    match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. }
            if prod_file == forwarding::RegFile::Float && rs3 == prod_rd =>
        {
            slot.mem_addr = Some(value);
        }
        _ => {}
    }
}

fn apply_just_committed_visibility_to_mem(slot: &mut PipeSlot, committed: &PipeSlot) {
    let Some(instr) = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    else {
        return;
    };
    let Some((prod_file, prod_rd, value)) = forwarding::slot_result(committed) else {
        return;
    };
    if prod_rd == 0 && prod_file == forwarding::RegFile::Int {
        return;
    }
    if slot.rs2 == Some(prod_rd) && forwarding::operand_reg_file(instr, 2) == Some(prod_file) {
        slot.rs2_val = value;
    }
}

fn parallel_fu_kind_for_slot(slot: &PipeSlot) -> Option<FuKind> {
    match slot.class {
        InstrClass::Alu | InstrClass::Mul | InstrClass::Div | InstrClass::Fp => {
            FuKind::from_class(slot.class)
        }
        InstrClass::Load | InstrClass::Store => Some(FuKind::Lsu),
        InstrClass::System => match slot
            .instr
            .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
        {
            Some(Instruction::Fence | Instruction::FenceI) => Some(FuKind::Sys),
            _ => None,
        },
        _ => None,
    }
}

fn parallel_fu_occupancy(state: &PipelineSimState, kind: FuKind) -> usize {
    state.fu_bank[kind.index()]
        .iter()
        .filter(|fu| {
            fu.slot
                .as_ref()
                .filter(|slot| !slot.is_bubble)
                .and_then(parallel_fu_kind_for_slot)
                == Some(kind)
        })
        .count()
}

fn is_memory_slot(slot: &PipeSlot) -> bool {
    !slot.is_bubble && matches!(slot.class, InstrClass::Load | InstrClass::Store)
}

fn lsu_dispatch_blocked_by_older_memory(state: &PipelineSimState) -> bool {
    state.fu_bank[FuKind::Lsu.index()]
        .iter()
        .any(|fu| fu.slot.as_ref().is_some_and(is_memory_slot))
        || state.stages[Stage::EX as usize]
            .as_ref()
            .is_some_and(is_memory_slot)
        || state.stages[Stage::MEM as usize]
            .as_ref()
            .is_some_and(is_memory_slot)
}

fn dispatch_parallel_fu_from_id(state: &mut PipelineSimState, cpi: &CpiConfig) -> bool {
    let Some(id_slot) = state.stages[Stage::ID as usize].as_ref() else {
        return false;
    };
    if id_slot.is_bubble {
        return false;
    }
    let Some(kind) = parallel_fu_kind_for_slot(id_slot) else {
        return false;
    };
    if kind == FuKind::Lsu && lsu_dispatch_blocked_by_older_memory(state) {
        return false;
    }
    if parallel_fu_occupancy(state, kind) >= state.fu_capacity[kind.index()] as usize {
        return false;
    }

    let mut slot = match state.stages[Stage::ID as usize].take() {
        Some(slot) => slot,
        None => return false,
    };
    let lat = fu_latency_for_class(slot.class, cpi);
    slot.fu_cycles_left = lat;
    stage_ex(&mut slot);
    state.fu_bank[kind.index()].push(super::FuState {
        kind: Some(kind),
        slot: Some(slot),
        busy_cycles_left: lat.saturating_sub(1),
    });
    true
}

fn mark_parallel_fu_capacity_hazard(state: &mut PipelineSimState) {
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        return;
    }
    let Some(id_s) = state.stages[Stage::ID as usize].as_ref() else {
        return;
    };
    if id_s.is_bubble {
        return;
    }
    let Some(kind) = parallel_fu_kind_for_slot(id_s) else {
        return;
    };
    if kind == FuKind::Lsu && lsu_dispatch_blocked_by_older_memory(state) {
        let producer_name = id_s.disasm.split_whitespace().next().unwrap_or("?");
        state.hazard_msgs.push((
            HazardType::FuBusy,
            format!(
                "FU busy: {producer_name} waits in ID because an older LSU operation is still in flight"
            ),
        ));
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            s.hazard = Some(HazardType::FuBusy);
        }
        state.stall_count += 1;
        state.stall_by_type[HazardType::FuBusy.as_stall_index().unwrap()] += 1;
        return;
    }
    let occupancy = parallel_fu_occupancy(state, kind);
    let capacity = state.fu_capacity[kind.index()] as usize;
    if occupancy < capacity {
        return;
    }
    let producer_name = id_s.disasm.split_whitespace().next().unwrap_or("?");
    state.hazard_msgs.push((
        HazardType::FuBusy,
        format!(
            "FU busy: {producer_name} waits in ID because {} is at capacity ({occupancy}/{capacity})",
            kind.label(),
        ),
    ));
    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        s.hazard = Some(HazardType::FuBusy);
    }
    state.stall_count += 1;
    state.stall_by_type[HazardType::FuBusy.as_stall_index().unwrap()] += 1;
}

fn advance_parallel_fu_banks(state: &mut PipelineSimState) {
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        return;
    }
    for fu_group in &mut state.fu_bank {
        for fu in fu_group {
            let Some(slot) = fu.slot.as_mut() else {
                continue;
            };
            if slot.is_bubble {
                continue;
            }
            if slot.fu_cycles_left > 1 {
                slot.fu_cycles_left -= 1;
            }
            fu.busy_cycles_left = slot.fu_cycles_left.saturating_sub(1);
        }
    }
}

fn oldest_in_flight_seq(state: &PipelineSimState) -> Option<u64> {
    let stage_min = state
        .stages
        .iter()
        .flatten()
        .filter(|s| !s.is_bubble && s.seq != 0)
        .map(|s| s.seq)
        .min();
    let fu_min = state
        .fu_bank
        .iter()
        .flat_map(|group| group.iter())
        .filter_map(|fu| fu.slot.as_ref())
        .filter(|s| !s.is_bubble && s.seq != 0)
        .map(|s| s.seq)
        .min();
    match (stage_min, fu_min) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn mem_slot_can_advance_to_wb(state: &PipelineSimState) -> bool {
    let Some(mem_slot) = state.stages[Stage::MEM as usize].as_ref() else {
        return true;
    };
    if mem_slot.is_bubble {
        return true;
    }
    if mem_slot.seq == 0 {
        return true;
    }
    Some(mem_slot.seq) == oldest_in_flight_seq(state)
}

fn stage_slot_available_for_promotion(state: &PipelineSimState, stage: Stage) -> bool {
    state.stages[stage as usize]
        .as_ref()
        .is_none_or(|slot| slot.is_bubble)
}

fn promote_ready_fu_result_to_wb(state: &mut PipelineSimState) {
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        return;
    }
    if !stage_slot_available_for_promotion(state, Stage::WB) {
        return;
    }
    let oldest_seq = oldest_in_flight_seq(state);
    let ready_idx = state
        .fu_bank
        .iter()
        .enumerate()
        .flat_map(|(kind_idx, group)| {
            group.iter().enumerate().filter_map(move |(slot_idx, fu)| {
                let slot = fu.slot.as_ref()?;
                if slot.is_bubble || slot.fu_cycles_left > 1 || fu.kind == Some(FuKind::Lsu) {
                    return None;
                }
                Some((kind_idx, slot_idx, slot.seq))
            })
        })
        .min_by_key(|(_, _, seq)| *seq)
        .filter(|(_, _, seq)| Some(*seq) == oldest_seq)
        .map(|(kind_idx, slot_idx, _)| (kind_idx, slot_idx));

    if let Some((kind_idx, slot_idx)) = ready_idx {
        let slot = state.fu_bank[kind_idx][slot_idx].slot.take();
        state.stages[Stage::WB as usize] = slot;
        state.fu_bank[kind_idx].swap_remove(slot_idx);
    }
}

fn promote_ready_lsu_to_mem(state: &mut PipelineSimState) -> bool {
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        return false;
    }
    if !stage_slot_available_for_promotion(state, Stage::MEM) {
        return false;
    }
    let oldest_seq = oldest_in_flight_seq(state);
    let ready_idx = state.fu_bank[FuKind::Lsu.index()]
        .iter()
        .enumerate()
        .filter_map(|(slot_idx, fu)| {
            let slot = fu.slot.as_ref()?;
            if slot.is_bubble || slot.fu_cycles_left > 1 {
                return None;
            }
            Some((slot_idx, slot.seq))
        })
        .min_by_key(|(_, seq)| *seq)
        .filter(|(_, seq)| Some(*seq) == oldest_seq)
        .map(|(slot_idx, _)| slot_idx);

    if let Some(slot_idx) = ready_idx {
        let lsu_idx = FuKind::Lsu.index();
        let slot = state.fu_bank[lsu_idx][slot_idx].slot.take();
        state.stages[Stage::MEM as usize] = slot;
        state.fu_bank[lsu_idx].swap_remove(slot_idx);
        return true;
    }
    false
}

fn refill_id_from_if(state: &mut PipelineSimState, if_cache_stall: bool) {
    if if_cache_stall {
        state.stages[1] = Some(PipeSlot {
            hazard: Some(HazardType::MemLatency),
            ..PipeSlot::bubble()
        });
        if let Some(if_slot) = state.stages[Stage::IF as usize].as_ref() {
            state.hazard_msgs.push((
                HazardType::MemLatency,
                format!(
                    "IF cache latency: fetch at 0x{:04X} is still pending, so ID receives no new instruction and a front-end bubble is inserted",
                    if_slot.pc
                ),
            ));
        }
    } else {
        state.stages[1] = state.stages[0].take(); // IF → ID
    }
}

fn branch_in_flight(state: &PipelineSimState) -> bool {
    state.stages[Stage::ID as usize]
        .as_ref()
        .map_or(false, |s| {
            !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
        })
        || state.stages[Stage::EX as usize]
            .as_ref()
            .map_or(false, |s| {
                !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
            })
        || state.stages[Stage::MEM as usize]
            .as_ref()
            .map_or(false, |s| {
                !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump)
            })
}

fn frontend_stop_in_flight(state: &PipelineSimState) -> bool {
    state.stages.iter().flatten().any(|slot| {
        !slot.is_bubble
            && matches!(
                slot.instr,
                Some(Instruction::Ecall | Instruction::Ebreak | Instruction::Halt)
            )
    }) || state
        .fu_bank
        .iter()
        .flat_map(|group| group.iter())
        .filter_map(|fu| fu.slot.as_ref())
        .any(|slot| {
            !slot.is_bubble
                && matches!(
                    slot.instr,
                    Some(Instruction::Ecall | Instruction::Ebreak | Instruction::Halt)
                )
        })
}

fn fetch_into_if(state: &mut PipelineSimState, mem: &mut CacheController, console: &mut Console) {
    if frontend_stop_in_flight(state) {
        return;
    }
    let branch_in_flight = branch_in_flight(state);

    if pc_in_program_range(state, state.fetch_pc) {
        let (fetched, fetch_error) = fetch_slot(state.fetch_pc, mem);
        if let Some(msg) = fetch_error {
            console.push_error(msg);
            state.faulted = true;
        }
        state.stages[0] = fetched;
        if let Some(ref mut slot) = state.stages[0] {
            if slot.seq == 0 {
                slot.seq = state.next_seq;
                state.next_seq += 1;
            }
            if slot.gantt_id == 0 {
                slot.gantt_id = state.next_gantt_id;
                state.next_gantt_id += 1;
            }
            slot.is_speculative = branch_in_flight;
            if slot.if_stall_cycles > 0 {
                slot.hazard = Some(HazardType::MemLatency);
            }
            state.fetch_pc = state.fetch_pc.wrapping_add(4);
        }
    } else if !has_in_flight_work(state) {
        console.push_error(format!(
            "Execution reached 0x{:08X}, outside the loaded program. \
             Add `li a7, 93; ecall` to terminate cleanly.",
            state.fetch_pc
        ));
        state.faulted = true;
    }
}

fn refresh_fu_busy(state: &mut PipelineSimState) {
    state.fu_busy = [0; 7];
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        state.fu_bank = std::array::from_fn(|_| Vec::new());
    } else {
        for group in &state.fu_bank {
            for fu in group {
                if let Some(slot) = fu.slot.as_ref().filter(|s| !s.is_bubble) {
                    if let Some(idx) = fu_type_idx(slot.class) {
                        state.fu_busy[idx] =
                            state.fu_busy[idx].max(slot.fu_cycles_left.saturating_sub(1));
                    }
                }
            }
        }
    }
    if let Some(slot) = state.stages[Stage::EX as usize].as_ref() {
        if !slot.is_bubble {
            if let Some(kind) = FuKind::from_class(slot.class) {
                let idx = kind.index();
                // In FunctionalUnits mode, never mirror the EX slot into fu_bank.
                // Serialized instructions (branches, jumps, ecall) use the EX stage
                // directly and commit via the normal EX→MEM→WB pipeline.  Adding a
                // clone of them into fu_bank would cause promote_ready_fu_result_to_wb
                // to commit them a second time.  RAW detection for EX-stage producers
                // is already covered by the stage-based loop in detect_stall.
                let should_mirror = !matches!(state.mode, super::PipelineMode::FunctionalUnits);
                if should_mirror {
                    state.fu_bank[idx].clear();
                    state.fu_bank[idx].push(super::FuState {
                        kind: Some(kind),
                        slot: Some(slot.clone()),
                        busy_cycles_left: slot.fu_cycles_left.saturating_sub(1),
                    });
                }
            }
            if let Some(idx) = fu_type_idx(slot.class) {
                state.fu_busy[idx] = slot.fu_cycles_left.saturating_sub(1);
            }
        }
    }
}

fn tick_if_cache_latency(state: &mut PipelineSimState) -> bool {
    if let Some(ref mut s) = state.stages[Stage::IF as usize] {
        if !s.is_bubble && s.if_stall_cycles > 0 {
            s.if_stall_cycles -= 1;
            s.hazard = Some(HazardType::MemLatency);
            return true;
        }
    }
    false
}

fn insert_stall(
    state: &mut PipelineSimState,
    stall_point: usize,
    hazard: HazardType,
    id_cpu: &Cpu,
    wb_before_commit: Option<&PipeSlot>,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
) {
    if stall_point < 4 {
        state.stages[4] = state.stages[3].take();
    }
    if stall_point < 3 {
        state.stages[3] = state.stages[2].take();
    }
    let bubble_pos = (stall_point + 1).min(4);
    state.stages[bubble_pos] = Some(PipeSlot {
        hazard: Some(hazard),
        ..PipeSlot::bubble()
    });
    if let Some(ref mut s) = state.stages[stall_point] {
        s.hazard = Some(hazard);
    }
    let mut mem_faulted2 = false;
    if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
        if !s.is_bubble {
            let (latency, fault) = stage_mem(s, cpu, mem, console);
            s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
            mem_faulted2 = fault;
        }
    }
    if mem_faulted2 {
        state.faulted = true;
    }
    advance_parallel_fu_banks(state);
    let lsu_promoted = promote_ready_lsu_to_mem(state);
    if lsu_promoted {
        if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
            if !s.is_bubble {
                let (latency, fault) = stage_mem(s, cpu, mem, console);
                s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
                if fault {
                    state.faulted = true;
                }
            }
        }
    }
    promote_ready_fu_result_to_wb(state);
    refresh_fu_busy(state);
    if stall_point == Stage::ID as usize {
        let wb_producer = wb_before_commit.cloned();
        let ready_fu_producers = forwarding::ready_fu_producers(state);
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            stage_id(s, id_cpu);
            forwarding::apply_forwarding_to_id(s, state.bypass, &wb_producer, &ready_fu_producers);
        }
    }
    if tick_if_cache_latency(state) {
        state.stall_by_type[HazardType::MemLatency.as_stall_index().unwrap()] += 1;
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Fetch ────────────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Returns `(slot, faulted)`.  A fetch bus error yields `(None, true)`;
/// a normal empty cycle yields `(None, false)`.
fn fetch_slot(pc: u32, mem: &mut CacheController) -> (Option<PipeSlot>, Option<String>) {
    let (result, latency) = mem.fetch32_timed_no_count(pc);
    match result {
        Ok(word) => {
            if let Err(e) = crate::falcon::decoder::decode(word) {
                return (
                    None,
                    Some(format!(
                        "Invalid instruction 0x{word:08X} at 0x{pc:08X}: {e}"
                    )),
                );
            }
            let mut slot = PipeSlot::from_word(pc, word);
            slot.seq = 0;
            slot.if_stall_cycles = latency.saturating_sub(1).min(255) as u8;
            (Some(slot), None)
        }
        Err(e) => (None, Some(format!("IF fault at 0x{pc:08X}: {e}"))),
    }
}

fn pc_in_program_range(state: &PipelineSimState, pc: u32) -> bool {
    state
        .program_range
        .map_or(true, |(start, end)| pc >= start && pc < end)
}

fn has_in_flight_work(state: &PipelineSimState) -> bool {
    state.stages.iter().flatten().any(|slot| !slot.is_bubble)
        || state
            .fu_bank
            .iter()
            .flat_map(|group| group.iter())
            .any(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble))
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Gantt update ─────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn update_gantt(state: &mut PipelineSimState) {
    let cycle = state.cycle_count;

    fn stage_cell_for_slot(stage: Stage, slot: &PipeSlot) -> GanttCell {
        let fu_cell = match slot.class {
            InstrClass::Alu => Some(FuKind::Alu),
            InstrClass::Mul => Some(FuKind::Mul),
            InstrClass::Div => Some(FuKind::Div),
            InstrClass::Fp => Some(FuKind::Fpu),
            InstrClass::Load | InstrClass::Store => Some(FuKind::Lsu),
            InstrClass::System => Some(FuKind::Sys),
            InstrClass::Branch | InstrClass::Jump | InstrClass::Unknown => None,
        };
        if stage == Stage::EX {
            if let Some(kind) = fu_cell {
                return if slot.is_speculative {
                    GanttCell::SpeculativeFu(kind)
                } else {
                    GanttCell::InFu(kind)
                };
            }
        }
        if slot.is_speculative {
            GanttCell::Speculative(stage)
        } else {
            GanttCell::InStage(stage)
        }
    }

    fn cell_track(cell: GanttCell) -> Option<GanttTrack> {
        match cell {
            GanttCell::InStage(stage) | GanttCell::Speculative(stage) => {
                Some(GanttTrack::Stage(stage))
            }
            GanttCell::InFu(kind) | GanttCell::SpeculativeFu(kind) => Some(GanttTrack::Fu(kind)),
            _ => None,
        }
    }

    for (stage_idx, maybe_slot) in state.stages.iter().enumerate() {
        let stage = Stage::all()[stage_idx];
        let cell = match maybe_slot {
            None => continue,
            Some(s) if s.is_bubble && s.hazard == Some(HazardType::BranchFlush) => GanttCell::Flush,
            Some(s) if s.is_bubble => GanttCell::Bubble,
            Some(s) => stage_cell_for_slot(stage, s),
        };
        if let Some(slot) = maybe_slot {
            let gantt_id = slot.gantt_id;
            let pc = slot.pc;
            let row = state.gantt.iter_mut().find(|r| {
                !r.done
                    && if gantt_id != 0 {
                        r.gantt_id == gantt_id
                    } else {
                        r.pc == pc
                    }
            });
            if let Some(row) = row {
                let emit_cell = match cell {
                    GanttCell::InStage(s) => {
                        if row.last_stage == Some(GanttTrack::Stage(s)) {
                            GanttCell::Stall
                        } else {
                            row.last_stage = Some(GanttTrack::Stage(s));
                            GanttCell::InStage(s)
                        }
                    }
                    GanttCell::Speculative(s) => {
                        row.last_stage = Some(GanttTrack::Stage(s));
                        GanttCell::Speculative(s)
                    }
                    GanttCell::InFu(kind) => {
                        if row.last_stage == Some(GanttTrack::Fu(kind)) {
                            GanttCell::Stall
                        } else {
                            row.last_stage = Some(GanttTrack::Fu(kind));
                            GanttCell::InFu(kind)
                        }
                    }
                    GanttCell::SpeculativeFu(kind) => {
                        row.last_stage = Some(GanttTrack::Fu(kind));
                        GanttCell::SpeculativeFu(kind)
                    }
                    _ => cell,
                };
                let expected_len = (cycle - row.first_cycle) as usize;
                while row.cells.len() < expected_len {
                    row.cells.push_back(GanttCell::Empty);
                }
                if row.cells.len() == expected_len {
                    row.cells.push_back(emit_cell);
                } else if let Some(last) = row.cells.back_mut() {
                    *last = emit_cell;
                }
                while row.cells.len() > MAX_GANTT_COLS {
                    row.cells.pop_front();
                    row.first_cycle += 1;
                }
            } else {
                let initial_stage = cell_track(cell);
                let mut new_row = GanttRow {
                    gantt_id,
                    pc,
                    disasm: slot.disasm.clone(),
                    class: slot.class,
                    cells: VecDeque::new(),
                    first_cycle: cycle,
                    done: false,
                    last_stage: initial_stage,
                };
                new_row.cells.push_back(cell);
                let prev_len = state.gantt.len();
                state.gantt.push_back(new_row);
                state.gantt_scroll = super::maybe_follow_gantt_tail(
                    state.gantt_scroll,
                    state.gantt_visible_rows_cache.get(),
                    prev_len,
                );
                while state.gantt.len() > MAX_GANTT_ROWS + 4 {
                    state.gantt.pop_front();
                    state.gantt_scroll = state.gantt_scroll.saturating_sub(1);
                }
            }
        }
    }

    for group in &state.fu_bank {
        for fu in group {
            let Some(slot) = fu.slot.as_ref() else {
                continue;
            };
            let cell = if slot.is_bubble {
                GanttCell::Bubble
            } else if slot.is_speculative {
                GanttCell::SpeculativeFu(fu.kind.unwrap_or(FuKind::Alu))
            } else {
                GanttCell::InFu(fu.kind.unwrap_or(FuKind::Alu))
            };
            let gantt_id = slot.gantt_id;
            let pc = slot.pc;
            let row = state.gantt.iter_mut().find(|r| {
                !r.done
                    && if gantt_id != 0 {
                        r.gantt_id == gantt_id
                    } else {
                        r.pc == pc
                    }
            });
            if let Some(row) = row {
                let emit_cell = match cell {
                    GanttCell::InFu(kind) => {
                        if row.last_stage == Some(GanttTrack::Fu(kind)) {
                            GanttCell::Stall
                        } else {
                            row.last_stage = Some(GanttTrack::Fu(kind));
                            GanttCell::InFu(kind)
                        }
                    }
                    GanttCell::SpeculativeFu(kind) => {
                        row.last_stage = Some(GanttTrack::Fu(kind));
                        GanttCell::SpeculativeFu(kind)
                    }
                    other => other,
                };
                let expected_len = (cycle - row.first_cycle) as usize;
                while row.cells.len() < expected_len {
                    row.cells.push_back(GanttCell::Empty);
                }
                if row.cells.len() == expected_len {
                    row.cells.push_back(emit_cell);
                } else if let Some(last) = row.cells.back_mut() {
                    *last = emit_cell;
                }
                while row.cells.len() > MAX_GANTT_COLS {
                    row.cells.pop_front();
                    row.first_cycle += 1;
                }
            } else {
                let mut new_row = GanttRow {
                    gantt_id,
                    pc,
                    disasm: slot.disasm.clone(),
                    class: slot.class,
                    cells: VecDeque::new(),
                    first_cycle: cycle,
                    done: false,
                    last_stage: cell_track(cell),
                };
                new_row.cells.push_back(cell);
                let prev_len = state.gantt.len();
                state.gantt.push_back(new_row);
                state.gantt_scroll = super::maybe_follow_gantt_tail(
                    state.gantt_scroll,
                    state.gantt_visible_rows_cache.get(),
                    prev_len,
                );
                while state.gantt.len() > MAX_GANTT_ROWS + 4 {
                    state.gantt.pop_front();
                    state.gantt_scroll = state.gantt_scroll.saturating_sub(1);
                }
            }
        }
    }

    let active_ids: Vec<(u64, u32)> = state
        .stages
        .iter()
        .flatten()
        .filter(|s| !s.is_bubble)
        .map(|s| (s.gantt_id, s.pc))
        .chain(
            state
                .fu_bank
                .iter()
                .flat_map(|group| group.iter())
                .filter_map(|fu| fu.slot.as_ref())
                .filter(|s| !s.is_bubble)
                .map(|s| (s.gantt_id, s.pc)),
        )
        .collect();

    for row in state.gantt.iter_mut() {
        let still_active = if row.gantt_id != 0 {
            active_ids.iter().any(|(id, _pc)| *id == row.gantt_id)
        } else {
            active_ids.iter().any(|(_id, pc)| *pc == row.pc)
        };
        if !row.done && !still_active {
            let expected = (cycle - row.first_cycle + 1) as usize;
            while row.cells.len() < expected.min(MAX_GANTT_COLS) {
                row.cells.push_back(GanttCell::Empty);
            }
            row.done = true;
        }
    }

    while state.gantt.len() > MAX_GANTT_ROWS {
        state.gantt.pop_front();
        state.gantt_scroll = state.gantt_scroll.saturating_sub(1);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Helpers ──────────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

pub fn reg_name(r: u8) -> &'static str {
    const NAMES: [&str; 32] = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
        "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
        "t5", "t6",
    ];
    NAMES.get(r as usize).copied().unwrap_or("?")
}

pub(super) fn push_trace(
    state: &mut PipelineSimState,
    kind: TraceKind,
    from_stage: usize,
    to_stage: usize,
    detail: String,
) {
    if state.hazard_traces.iter().any(|t| {
        t.kind == kind && t.from_stage == from_stage && t.to_stage == to_stage && t.detail == detail
    }) {
        return;
    }
    state.hazard_traces.push(HazardTrace {
        kind,
        from_stage,
        to_stage,
        detail,
    });
}

// ── RAW hazard reporting (informational, no stall when forwarding) ───────────

pub fn report_raw_hazards(state: &mut PipelineSimState) {
    forwarding::report_forward_hazards(state);
}

#[cfg(test)]
#[path = "../../../tests/support/ui_pipeline_sim.rs"]
mod tests;
