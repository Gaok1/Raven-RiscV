//! Pipeline simulator tick logic — per-stage execution.

use super::{
    GanttCell, GanttRow, HazardTrace, HazardType, InstrClass, MAX_GANTT_COLS, MAX_GANTT_ROWS,
    PipeSlot, PipelineSimState, Stage, TraceKind, fu_latency_for_class,
};
use crate::falcon::Cpu;
use crate::falcon::cache::CacheController;
use crate::falcon::instruction::Instruction;
use crate::ui::app::CpiConfig;
use crate::ui::console::Console;
use std::collections::VecDeque;

// ── Commit info returned to caller ──────────────────────────────────────────

pub struct CommitInfo {
    pub pc: u32,
    pub class: InstrClass,
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

    state.hazard_msgs.clear();
    state.hazard_traces.clear();
    state.cycle_count += 1;

    // ── 1. COMMIT: WB stage ───────────────────────────────────────────────
    let commit = commit_wb(state, cpu, mem, console);

    // ── 2. Detect stalls ─────────────────────────────────────────────────
    let stall = detect_stall(state);

    // ── 3. Advance or stall ───────────────────────────────────────────────
    if let Some((stall_stage, hazard)) = stall {
        insert_stall(state, stall_stage, hazard, cpu, mem, console);
        state.stall_count += 1;
    } else {
        advance_stages(state, cpu, mem, cpi, console);
    }

    // ── 4. Report forwarding hazards (informational) ─────────────────────
    if state.forwarding {
        report_raw_hazards(state);
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
        Instruction::FmaddS { .. } => {
            let a = f32::from_bits(slot.rs1_val);
            let b = f32::from_bits(slot.rs2_val);
            // rs3 is not in our slot — we only have rs1/rs2. For fused ops, we stored rs3's
            // value... actually we don't have an rs3_val field. We'll need the instruction's rs3.
            // For now, use the decoded instruction to get rs3 index and read from the slot's
            // existing values. But we need a workaround here.
            // Actually: fmadd uses rs1, rs2, rs3 — but we only read rs1, rs2 at ID.
            // For rs3, we need to handle it specially at ID stage.
            // WORKAROUND: store rs3 value in mem_addr field (reusing unused field for FP arith)
            // This is set in stage_id via special handling.
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
    let instr = slot.instr?;
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
    let instr = slot.instr?;
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
    let instr = match slot.instr {
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
    matches!(
        slot.instr,
        Some(
            Instruction::Lb { .. }
                | Instruction::Lh { .. }
                | Instruction::Lw { .. }
                | Instruction::Lbu { .. }
                | Instruction::Lhu { .. }
                | Instruction::Flw { .. }
                | Instruction::LrW { .. }
        )
    )
}

fn slot_has_wb_only_syscall_result(slot: &PipeSlot) -> bool {
    !slot.is_bubble && matches!(slot.instr, Some(Instruction::Ecall))
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

fn predicted_control_target(slot: &PipeSlot, state: &PipelineSimState) -> Option<(bool, u32)> {
    let instr = slot.instr?;
    match instr {
        Instruction::Beq { imm, .. }
        | Instruction::Bne { imm, .. }
        | Instruction::Blt { imm, .. }
        | Instruction::Bge { imm, .. }
        | Instruction::Bltu { imm, .. }
        | Instruction::Bgeu { imm, .. } => {
            let target = slot.pc.wrapping_add(imm as u32);
            match state.predict {
                super::BranchPredict::Taken => Some((true, target)),
                super::BranchPredict::NotTaken => Some((false, slot.pc.wrapping_add(4))),
            }
        }
        Instruction::Jal { imm, .. } => Some((true, slot.pc.wrapping_add(imm as u32))),
        Instruction::Jalr { imm, .. } => Some((true, (slot.rs1_val.wrapping_add(imm as u32)) & !1)),
        _ => None,
    }
}

fn apply_branch_prediction(state: &mut PipelineSimState) {
    let id_idx = Stage::ID as usize;
    let should_predict = match state.stages[id_idx].as_ref() {
        Some(slot)
            if !slot.is_bubble
                && matches!(slot.class, InstrClass::Branch | InstrClass::Jump)
                && slot.predicted_target.is_none() =>
        {
            true
        }
        _ => false,
    };
    if !should_predict {
        return;
    }

    let (predicted_taken, predicted_pc) = {
        let slot = state.stages[id_idx].as_ref().unwrap();
        match predicted_control_target(slot, state) {
            Some(v) => v,
            None => return,
        }
    };

    if let Some(ref mut slot) = state.stages[id_idx] {
        slot.predicted_taken = predicted_taken;
        slot.predicted_target = Some(predicted_pc);
    }

    if predicted_taken {
        state.fetch_pc = predicted_pc;
        state.stages[Stage::IF as usize] = None;
    }
}

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
            let (result, access_latency) = mem.dcache_read32_timed(addr);
            latency += access_latency;
            match result {
                Ok(v) => {
                    slot.mem_result = Some(v);
                    cpu.lr_reservation = Some(addr);
                }
                Err(e) => {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                    faulted = true;
                }
            }
        }
        Instruction::ScW { .. } => {
            if cpu.lr_reservation == Some(addr) {
                let (result, access_latency) = mem.store32_timed(addr, slot.rs2_val);
                latency += access_latency;
                if let Err(e) = result {
                    console.push_error(format!("MEM fault at 0x{addr:08X}: {e}"));
                }
                slot.alu_result = 0; // success
            } else {
                slot.alu_result = 1; // failure
            }
            cpu.lr_reservation = None;
        }
        Instruction::AmoswapW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, slot.rs2_val);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmoaddW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) =
                    mem.store32_timed(addr, old.wrapping_add(slot.rs2_val));
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmoxorW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, old ^ slot.rs2_val);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmoandW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, old & slot.rs2_val);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmoorW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, old | slot.rs2_val);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmomaxW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let new = (old as i32).max(slot.rs2_val as i32) as u32;
                let (write_result, write_latency) = mem.store32_timed(addr, new);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmominW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let new = (old as i32).min(slot.rs2_val as i32) as u32;
                let (write_result, write_latency) = mem.store32_timed(addr, new);
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmomaxuW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, old.max(slot.rs2_val));
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }
        Instruction::AmominuW { .. } => {
            let (read_result, read_latency) = mem.dcache_read32_timed(addr);
            latency += read_latency;
            if let Ok(old) = read_result {
                let (write_result, write_latency) = mem.store32_timed(addr, old.min(slot.rs2_val));
                latency += write_latency;
                let _ = write_result;
                slot.alu_result = old;
            }
        }

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
            cpu.instr_count += 1;
            let code = cpu.read(17); // a7
            match crate::falcon::syscall::handle_syscall(code, cpu, mem, console) {
                Ok(cont) => {
                    if !cont && console.reading {
                        cpu.pc = slot.pc; // rewind for blocking stdin
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

        Instruction::Fence => {} // nop
    }

    cpu.instr_count += 1;
    true
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

    // Unknown/undecodable instructions: treat as NOP, don't execute
    if slot.class == InstrClass::Unknown {
        return None;
    }

    // Set cpu.pc to this instruction's PC (needed for ecall handlers)
    cpu.pc = slot.pc;

    let alive = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stage_wb(&slot, cpu, mem, console)
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

    // Update cpu.pc: branch target or next sequential.
    // Terminal local hart exits (ecall-based) stay parked on their own ecall so
    // the Run view highlights the instruction that stopped the hart.
    // halt is a pseudo-instruction: advance PC like sequential mode so cpu.pc
    // is consistent with exec::step (which pre-increments before executing).
    if !alive && cpu.local_exit && !matches!(slot.instr, Some(Instruction::Halt)) {
        cpu.pc = slot.pc;
    } else if slot.branch_taken {
        cpu.pc = slot.branch_target.unwrap_or(slot.pc.wrapping_add(4));
    } else {
        cpu.pc = slot.pc.wrapping_add(4);
    }

    state.instr_committed += 1;
    state.class_counts[slot.class.as_usize()] += 1;

    Some(CommitInfo {
        pc: slot.pc,
        class: slot.class,
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Branch resolution ────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Flush stages behind the given resolution point and redirect fetch_pc.
fn resolve_branch(state: &mut PipelineSimState, resolve_stage: usize) {
    let (actual_taken, actual_target, predicted_taken, predicted_target, detail) =
        match state.stages[resolve_stage].as_ref() {
            Some(s) if !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump) => {
                let actual_taken = s.branch_taken;
                let actual_target = if actual_taken {
                    s.branch_target.unwrap_or(s.pc.wrapping_add(4))
                } else {
                    s.pc.wrapping_add(4)
                };
                let predicted_taken = s.predicted_taken;
                let predicted_target = s.predicted_target.unwrap_or(s.pc.wrapping_add(4));
                let detail = format!(
                    "{} flush",
                    s.disasm.split_whitespace().next().unwrap_or("?")
                );
                (
                    actual_taken,
                    actual_target,
                    predicted_taken,
                    predicted_target,
                    detail,
                )
            }
            _ => return,
        };

    let mispredicted =
        actual_taken != predicted_taken || (actual_taken && actual_target != predicted_target);
    if !mispredicted {
        return;
    }

    // Flush all stages before the resolution point (younger instructions)
    for i in 0..resolve_stage {
        let should_flush = state.stages[i]
            .as_ref()
            .map(|s| !s.is_bubble)
            .unwrap_or(false);
        if should_flush {
            push_trace(
                state,
                TraceKind::Hazard(HazardType::BranchFlush),
                resolve_stage,
                i,
                detail.clone(),
            );
        }
        if let Some(ref mut s) = state.stages[i] {
            if !s.is_bubble {
                s.is_bubble = true;
                s.hazard = Some(HazardType::BranchFlush);
                state.flush_count += 1;
            }
        }
    }
    state.fetch_pc = actual_target;
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Hazard detection ─────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn detect_stall(state: &mut PipelineSimState) -> Option<(usize, HazardType)> {
    // ── Syscall barrier hazards around ABI arg regs a0..a7 ───────────────
    let syscall_abi_hazard: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
        let id = state.stages[Stage::ID as usize].as_ref();
        let ex = state.stages[Stage::EX as usize].as_ref();
        let mem_s = state.stages[Stage::MEM as usize].as_ref();
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
                    ] {
                        let Some(p) = producer else {
                            continue;
                        };
                        if slot_has_wb_only_syscall_result(p) {
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
                            if let Some((prod_file, prod_rd)) = slot_destination(p) {
                                if prod_file == RegFile::Int && prod_rd == arg_reg {
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
        let mut found = None;

        if let Some(id_s) = id {
            if !id_s.is_bubble {
                for arg_reg in 10..=17u8 {
                    if !slot_reads_register(id_s, RegFile::Int, arg_reg) {
                        continue;
                    }
                    for (stage_idx, stage_name, producer) in [
                        (Stage::EX as usize, "EX", ex),
                        (Stage::MEM as usize, "MEM", mem_s),
                    ] {
                        if let Some(p) = producer {
                            if slot_has_wb_only_syscall_result(p) {
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
        if let (Some(id_s), Some(ex_s)) = (id, ex) {
            if !id_s.is_bubble && slot_has_late_mem_result(ex_s) {
                if let Some((prod_file, ex_rd)) = slot_destination(ex_s) {
                    if slot_reads_register(id_s, prod_file, ex_rd) {
                        Some((
                            HazardType::LoadUse,
                            format!("load-use: ID uses {} written by lw in EX", reg_name(ex_rd)),
                            Some((
                                Stage::EX as usize,
                                Stage::ID as usize,
                                format!(
                                    "{} -> {}",
                                    ex_s.disasm.split_whitespace().next().unwrap_or("?"),
                                    id_s.disasm.split_whitespace().next().unwrap_or("?")
                                ),
                            )),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
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

    // ── RAW without forwarding ────────────────────────────────────────────
    if !state.forwarding {
        let raw_result: Option<(HazardType, String, Option<(usize, usize, String)>)> = {
            let id = state.stages[Stage::ID as usize].as_ref();
            let ex = state.stages[Stage::EX as usize].as_ref();
            let mem_s = state.stages[Stage::MEM as usize].as_ref();
            let mut found = None;
            if let Some(id_s) = id {
                if !id_s.is_bubble {
                    'outer: for (stage_name, producer) in [("EX", ex), ("MEM", mem_s)] {
                        if let Some(p) = producer {
                            if !p.is_bubble {
                                if let Some((prod_file, p_rd)) = slot_destination(p) {
                                    if p_rd != 0 {
                                        if slot_reads_register(id_s, prod_file, p_rd) {
                                            let from_idx = if stage_name == "EX" {
                                                Stage::EX as usize
                                            } else {
                                                Stage::MEM as usize
                                            };
                                            found = Some((
                                                HazardType::Raw,
                                                format!(
                                                    "RAW: ID reads {} — written by {} in {}",
                                                    reg_name(p_rd),
                                                    p.disasm
                                                        .split_whitespace()
                                                        .next()
                                                        .unwrap_or("?"),
                                                    stage_name
                                                ),
                                                Some((
                                                    from_idx,
                                                    Stage::ID as usize,
                                                    format!(
                                                        "{} -> {}",
                                                        p.disasm
                                                            .split_whitespace()
                                                            .next()
                                                            .unwrap_or("?"),
                                                        id_s.disasm
                                                            .split_whitespace()
                                                            .next()
                                                            .unwrap_or("?")
                                                    ),
                                                )),
                                            ));
                                            break 'outer;
                                        }
                                    }
                                }
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
                    "CTL: {} at 0x{:04X} — resolves at {} (+{} stall{})",
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
    // Collect writers: (stage_index, reg_file, rd, mnemonic)
    let writers: Vec<(usize, RegFile, u8, String)> = state
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
            Some((i, rf, rd, mnem))
        })
        .collect();

    // Collect readers: (stage_index, mnemonic, slot clone)
    let readers: Vec<(usize, String, PipeSlot)> = state
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
            Some((i, mnem, s.clone()))
        })
        .collect();

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
                    Stage::all()[writers[j].0].label(),
                    writers[i].3,
                    Stage::all()[writers[i].0].label(),
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
    for &(w_idx, w_rf, w_rd, ref w_name) in &writers {
        for &(r_idx, ref r_name, ref reader_slot) in &readers {
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
                    Stage::all()[w_idx].label(),
                    dest_name,
                    r_name,
                    Stage::all()[r_idx].label(),
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
    cpu: &mut Cpu,
    mem: &mut CacheController,
    cpi: &CpiConfig,
    console: &mut Console,
) {
    use super::PipelineMode;

    let ex_producer_before = state.stages[Stage::EX as usize].clone();
    let mem_producer_before = state.stages[Stage::MEM as usize].clone();
    let wb_producer_before = state.stages[Stage::WB as usize].clone();
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
        state.stages[4] = None; // WB fica vazio; MEM não avança
        return;
    }

    // ── FU mode: decrement fu_cycles_left on EX slot ─────────────────────
    if state.mode == PipelineMode::FunctionalUnits {
        let ex_stall = state.stages[Stage::EX as usize]
            .as_ref()
            .map_or(false, |s| !s.is_bubble && s.fu_cycles_left > 1);

        if ex_stall {
            if if_cache_stall {
                if let Some(ref mut s) = state.stages[Stage::IF as usize] {
                    s.hazard = Some(HazardType::MemLatency);
                }
            }
            if let Some(ref mut s) = state.stages[Stage::EX as usize] {
                s.fu_cycles_left -= 1;
                s.hazard = Some(HazardType::FuBusy);
            }
            state.stages[4] = state.stages[3].take();
            state.stages[3] = Some(PipeSlot::bubble());
            for i in [1usize, 0usize] {
                if let Some(ref mut s) = state.stages[i] {
                    s.hazard = None;
                }
            }
            if if_cache_stall {
                if let Some(ref mut s) = state.stages[Stage::IF as usize] {
                    s.hazard = Some(HazardType::MemLatency);
                }
            }
            return;
        }
    }

    if if_cache_stall {
        if let Some(ref mut s) = state.stages[Stage::IF as usize] {
            s.if_stall_cycles -= 1;
            s.hazard = Some(HazardType::MemLatency);
        }
    }

    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        stage_id(s, cpu);
        if state.forwarding {
            apply_forwarding_to_id(
                s,
                &ex_producer_before,
                &mem_producer_before,
                &wb_producer_before,
            );
        }
    }

    // ── BranchResolve::Id — resolve before the branch leaves ID ──────────
    // Must happen here, while the branch is still in stages[ID].  After the
    // advance below it moves to EX and stages[ID] holds a different instruction.
    if matches!(state.branch_resolve, BranchResolve::Id) {
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            resolve_control_in_id(s);
        }
        resolve_branch(state, Stage::ID as usize);
    }

    // ── Normal advance / IF cache hold ───────────────────────────────────
    state.stages[4] = state.stages[3].take(); // MEM → WB
    state.stages[3] = state.stages[2].take(); // EX  → MEM
    state.stages[2] = state.stages[1].take(); // ID  → EX
    // Apply FU latency to newly entered EX slot
    if state.mode == PipelineMode::FunctionalUnits {
        if let Some(ref mut s) = state.stages[2] {
            if !s.is_bubble {
                s.fu_cycles_left = fu_latency_for_class(s.class, cpi);
            }
        }
    }
    if if_cache_stall {
        state.stages[1] = None;
    } else {
        state.stages[1] = state.stages[0].take(); // IF → ID
    }
    for s in state.stages.iter_mut().flatten() {
        s.hazard = None;
    }
    if if_cache_stall {
        if let Some(ref mut s) = state.stages[Stage::IF as usize] {
            s.hazard = Some(HazardType::MemLatency);
        }
        state.stall_count += 1;
    }

    let ex_producer = state.stages[Stage::EX as usize].clone();
    let mem_producer = state.stages[Stage::MEM as usize].clone();
    let wb_producer = state.stages[Stage::WB as usize].clone();

    // ── Execute per-stage work on newly placed slots ─────────────────────
    if let Some(ref mut s) = state.stages[Stage::ID as usize] {
        stage_id(s, cpu);
        if state.forwarding {
            apply_forwarding_to_id(s, &ex_producer, &mem_producer, &wb_producer);
        }
    }
    apply_branch_prediction(state);

    if !state.halted && state.stages[Stage::IF as usize].is_none() {
        let branch_in_flight = state.stages[Stage::ID as usize]
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

        let (fetched, fetch_fault) = fetch_slot(state.fetch_pc, mem);
        if fetch_fault {
            state.faulted = true;
        }
        state.stages[0] = fetched;
        if let Some(ref mut slot) = state.stages[0] {
            slot.is_speculative = branch_in_flight;
            if slot.if_stall_cycles > 0 {
                slot.hazard = Some(HazardType::MemLatency);
            }
            state.fetch_pc = state.fetch_pc.wrapping_add(4);
        }
    }
    if let Some(ref mut s) = state.stages[Stage::EX as usize] {
        if state.forwarding {
            apply_forwarding_to_ex(s, &mem_producer, &wb_producer);
        }
        stage_ex(s);
        // Handle fused multiply-add rs3 reading (was done at ID, stored in mem_addr)
        // This is already handled in stage_id via special code below
    }
    let mut mem_faulted = false;
    if let Some(ref mut s) = state.stages[Stage::MEM as usize] {
        if state.forwarding {
            apply_forwarding_to_mem(s, &wb_producer);
        }
        let (latency, fault) = stage_mem(s, cpu, mem, console);
        s.mem_stall_cycles = latency.saturating_sub(1).min(255) as u8;
        mem_faulted = fault;
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
            resolve_branch(state, Stage::EX as usize);
        }
        BranchResolve::Mem => {
            resolve_branch(state, Stage::MEM as usize);
        }
    }
}

fn insert_stall(
    state: &mut PipelineSimState,
    stall_point: usize,
    hazard: HazardType,
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
    if stall_point == Stage::ID as usize {
        let ex_producer = state.stages[Stage::EX as usize].clone();
        let mem_producer = state.stages[Stage::MEM as usize].clone();
        let wb_producer = state.stages[Stage::WB as usize].clone();
        if let Some(ref mut s) = state.stages[Stage::ID as usize] {
            stage_id(s, cpu);
            if state.forwarding {
                apply_forwarding_to_id(s, &ex_producer, &mem_producer, &wb_producer);
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Fetch ────────────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Returns `(slot, faulted)`.  A fetch bus error yields `(None, true)`;
/// a normal empty cycle yields `(None, false)`.
fn fetch_slot(pc: u32, mem: &mut CacheController) -> (Option<PipeSlot>, bool) {
    let (result, latency) = mem.fetch32_timed_no_count(pc);
    match result {
        Ok(word) => {
            let mut slot = PipeSlot::from_word(pc, word);
            slot.if_stall_cycles = latency.saturating_sub(1).min(255) as u8;
            (Some(slot), false)
        }
        Err(_) => (None, true),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Gantt update ─────────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

fn update_gantt(state: &mut PipelineSimState) {
    let cycle = state.cycle_count;

    for (stage_idx, maybe_slot) in state.stages.iter().enumerate() {
        let stage = Stage::all()[stage_idx];
        let cell = match maybe_slot {
            None => continue,
            Some(s) if s.is_bubble && s.hazard == Some(HazardType::BranchFlush) => GanttCell::Flush,
            Some(s) if s.is_bubble => GanttCell::Bubble,
            Some(_) => GanttCell::InStage(stage),
        };
        if let Some(slot) = maybe_slot {
            if slot.is_bubble && slot.hazard != Some(HazardType::BranchFlush) {
                continue;
            }
            let pc = slot.pc;
            let cell = if let GanttCell::InStage(s) = cell {
                GanttCell::InStage(s)
            } else {
                cell
            };
            let row = state.gantt.iter_mut().find(|r| r.pc == pc && !r.done);
            if let Some(row) = row {
                let emit_cell = if let GanttCell::InStage(s) = cell {
                    if row.last_stage == Some(s) {
                        GanttCell::Stall
                    } else {
                        row.last_stage = Some(s);
                        GanttCell::InStage(s)
                    }
                } else {
                    cell
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
                }
            } else {
                let initial_stage = if let GanttCell::InStage(s) = cell {
                    Some(s)
                } else {
                    None
                };
                let mut new_row = GanttRow {
                    pc,
                    disasm: slot.disasm.clone(),
                    class: slot.class,
                    cells: VecDeque::new(),
                    first_cycle: cycle,
                    done: false,
                    last_stage: initial_stage,
                };
                new_row.cells.push_back(cell);
                state.gantt.push_back(new_row);
                while state.gantt.len() > MAX_GANTT_ROWS + 4 {
                    state.gantt.pop_front();
                    state.gantt_scroll = state.gantt_scroll.saturating_sub(1);
                }
            }
        }
    }

    let active_pcs: Vec<u32> = state
        .stages
        .iter()
        .flatten()
        .filter(|s| !s.is_bubble)
        .map(|s| s.pc)
        .collect();

    for row in state.gantt.iter_mut() {
        if !row.done && !active_pcs.contains(&row.pc) {
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

fn push_trace(
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
    if !state.forwarding {
        return;
    }
    let id = match state.stages[Stage::ID as usize].as_ref() {
        Some(s) if !s.is_bubble => s.clone(),
        _ => return,
    };
    for &prod_idx in &[Stage::EX as usize, Stage::MEM as usize, Stage::WB as usize] {
        let trace_info = state.stages[prod_idx].as_ref().and_then(|p| {
            if p.is_bubble {
                return None;
            }
            let p_rd = p.rd?;
            if p_rd == 0 {
                return None;
            }
            let (prod_file, _, _) = slot_result(p)?;
            if !slot_reads_register(&id, prod_file, p_rd) {
                return None;
            }
            let prod_name = p
                .disasm
                .split_whitespace()
                .next()
                .unwrap_or("?")
                .to_string();
            Some((p_rd, prod_name))
        });
        if let Some((p_rd, prod_name)) = trace_info {
            let stage_name = Stage::all()[prod_idx].label();
            let consumer_name = id.disasm.split_whitespace().next().unwrap_or("?");
            let detail = format!(
                "{}:{} -> {}:{} ({})",
                stage_name,
                prod_name,
                Stage::ID.label(),
                consumer_name,
                reg_name(p_rd),
            );
            push_trace(
                state,
                TraceKind::Forward,
                prod_idx,
                Stage::ID as usize,
                detail,
            );
            let msg = format!(
                "FWD: {} bypassed from {}:{} into ID:{} [RAW covered]",
                reg_name(p_rd),
                stage_name,
                prod_name,
                consumer_name,
            );
            state.hazard_msgs.push((HazardType::Raw, msg));
        }
    }
}
