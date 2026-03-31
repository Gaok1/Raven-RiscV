use super::*;

pub(super) fn classify_cpi_cycles(word: u32, cpu: &crate::falcon::Cpu, cpi: &CpiConfig) -> u64 {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(
            Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Sll { .. }
            | Srl { .. }
            | Sra { .. }
            | Slt { .. }
            | Sltu { .. }
            | Addi { .. }
            | Andi { .. }
            | Ori { .. }
            | Xori { .. }
            | Slti { .. }
            | Sltiu { .. }
            | Slli { .. }
            | Srli { .. }
            | Srai { .. }
            | Lui { .. }
            | Auipc { .. },
        ) => 1 + cpi.alu,
        Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => 1 + cpi.mul,
        Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => 1 + cpi.div,
        Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. }) => 1 + cpi.load,
        Ok(Sb { .. } | Sh { .. } | Sw { .. }) => 1 + cpi.store,
        Ok(Jal { .. } | Jalr { .. }) => 1 + cpi.jump,
        Ok(Ecall | Ebreak | Halt) => 1 + cpi.system,
        Ok(Beq { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] == cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bne { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] != cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Blt { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) < (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bge { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bltu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] < cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bgeu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] >= cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(
            Flw { .. }
            | Fsw { .. }
            | FaddS { .. }
            | FsubS { .. }
            | FmulS { .. }
            | FdivS { .. }
            | FsqrtS { .. }
            | FminS { .. }
            | FmaxS { .. }
            | FsgnjS { .. }
            | FsgnjnS { .. }
            | FsgnjxS { .. }
            | FeqS { .. }
            | FltS { .. }
            | FleS { .. }
            | FcvtWS { .. }
            | FcvtWuS { .. }
            | FcvtSW { .. }
            | FcvtSWu { .. }
            | FmvXW { .. }
            | FmvWX { .. }
            | FclassS { .. }
            | FmaddS { .. }
            | FmsubS { .. }
            | FnmsubS { .. }
            | FnmaddS { .. },
        ) => 1 + cpi.fp,
        _ => 1,
    }
}

pub(crate) fn classify_cpi_for_display(
    word: u32,
    _addr: u32,
    cpu: &crate::falcon::Cpu,
    cpi: &CpiConfig,
) -> u64 {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(
            Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Sll { .. }
            | Srl { .. }
            | Sra { .. }
            | Slt { .. }
            | Sltu { .. }
            | Addi { .. }
            | Andi { .. }
            | Ori { .. }
            | Xori { .. }
            | Slti { .. }
            | Sltiu { .. }
            | Slli { .. }
            | Srli { .. }
            | Srai { .. }
            | Lui { .. }
            | Auipc { .. },
        ) => 1 + cpi.alu,
        Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => 1 + cpi.mul,
        Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => 1 + cpi.div,
        Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. }) => 1 + cpi.load,
        Ok(Sb { .. } | Sh { .. } | Sw { .. }) => 1 + cpi.store,
        Ok(Jal { .. } | Jalr { .. }) => 1 + cpi.jump,
        Ok(Ecall | Ebreak | Halt) => 1 + cpi.system,
        Ok(Beq { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] == cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bne { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] != cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Blt { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) < (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bge { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bltu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] < cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bgeu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] >= cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(
            Flw { .. }
            | Fsw { .. }
            | FaddS { .. }
            | FsubS { .. }
            | FmulS { .. }
            | FdivS { .. }
            | FsqrtS { .. }
            | FminS { .. }
            | FmaxS { .. }
            | FsgnjS { .. }
            | FsgnjnS { .. }
            | FsgnjxS { .. }
            | FeqS { .. }
            | FltS { .. }
            | FleS { .. }
            | FcvtWS { .. }
            | FcvtWuS { .. }
            | FcvtSW { .. }
            | FcvtSWu { .. }
            | FmvXW { .. }
            | FmvWX { .. }
            | FclassS { .. }
            | FmaddS { .. }
            | FmsubS { .. }
            | FnmsubS { .. }
            | FnmaddS { .. },
        ) => 1 + cpi.fp,
        _ => 1,
    }
}

pub(crate) fn cpi_class_label(word: u32) -> &'static str {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(
            Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Sll { .. }
            | Srl { .. }
            | Sra { .. }
            | Slt { .. }
            | Sltu { .. }
            | Addi { .. }
            | Andi { .. }
            | Ori { .. }
            | Xori { .. }
            | Slti { .. }
            | Sltiu { .. }
            | Slli { .. }
            | Srli { .. }
            | Srai { .. }
            | Lui { .. }
            | Auipc { .. },
        ) => "ALU",
        Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => "MUL",
        Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => "DIV",
        Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. }) => "Load",
        Ok(Sb { .. } | Sh { .. } | Sw { .. }) => "Store",
        Ok(Jal { .. } | Jalr { .. }) => "Jump",
        Ok(Ecall | Ebreak | Halt) => "System",
        Ok(Beq { .. } | Bne { .. } | Blt { .. } | Bge { .. } | Bltu { .. } | Bgeu { .. }) => {
            "Branch"
        }
        Ok(
            Flw { .. }
            | Fsw { .. }
            | FaddS { .. }
            | FsubS { .. }
            | FmulS { .. }
            | FdivS { .. }
            | FsqrtS { .. }
            | FminS { .. }
            | FmaxS { .. }
            | FsgnjS { .. }
            | FsgnjnS { .. }
            | FsgnjxS { .. }
            | FeqS { .. }
            | FltS { .. }
            | FleS { .. }
            | FcvtWS { .. }
            | FcvtWuS { .. }
            | FcvtSW { .. }
            | FcvtSWu { .. }
            | FmvXW { .. }
            | FmvWX { .. }
            | FclassS { .. }
            | FmaddS { .. }
            | FmsubS { .. }
            | FnmsubS { .. }
            | FnmaddS { .. },
        ) => "FP",
        _ => "?",
    }
}
