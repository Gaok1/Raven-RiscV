use crate::falcon;

pub fn disasm_word(w: u32) -> String {
    match falcon::decoder::decode(w) {
        Ok(ins) => pretty_instr(&ins),
        Err(e) => format!("<decode error: {e}>"),
    }
}

fn pretty_instr(i: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;
    match *i {
        Add { rd, rs1, rs2 } => format!(
            "add  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sub { rd, rs1, rs2 } => format!(
            "sub  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        And { rd, rs1, rs2 } => format!(
            "and  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Or { rd, rs1, rs2 } => format!(
            "or   {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Xor { rd, rs1, rs2 } => format!(
            "xor  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sll { rd, rs1, rs2 } => format!(
            "sll  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Srl { rd, rs1, rs2 } => format!(
            "srl  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sra { rd, rs1, rs2 } => format!(
            "sra  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Slt { rd, rs1, rs2 } => format!(
            "slt  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sltu { rd, rs1, rs2 } => format!(
            "sltu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mul { rd, rs1, rs2 } => format!(
            "mul  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulh { rd, rs1, rs2 } => format!(
            "mulh {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulhsu { rd, rs1, rs2 } => format!(
            "mulhsu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulhu { rd, rs1, rs2 } => format!(
            "mulhu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Div { rd, rs1, rs2 } => format!(
            "div  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Divu { rd, rs1, rs2 } => format!(
            "divu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Rem { rd, rs1, rs2 } => format!(
            "rem  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Remu { rd, rs1, rs2 } => format!(
            "remu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Addi { rd, rs1, imm } => format!("addi {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Andi { rd, rs1, imm } => format!("andi {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Ori { rd, rs1, imm } => format!("ori  {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Xori { rd, rs1, imm } => format!("xori {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Slti { rd, rs1, imm } => format!("slti {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Sltiu { rd, rs1, imm } => format!("sltiu {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Slli { rd, rs1, shamt } => format!("slli {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Srli { rd, rs1, shamt } => format!("srli {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Srai { rd, rs1, shamt } => format!("srai {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Lb { rd, rs1, imm } => format!("lb   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lh { rd, rs1, imm } => format!("lh   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lw { rd, rs1, imm } => format!("lw   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lbu { rd, rs1, imm } => format!("lbu  {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lhu { rd, rs1, imm } => format!("lhu  {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Sb { rs2, rs1, imm } => format!("sb   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Sh { rs2, rs1, imm } => format!("sh   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Sw { rs2, rs1, imm } => format!("sw   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Beq { rs1, rs2, imm } => format!("beq  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bne { rs1, rs2, imm } => format!("bne  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Blt { rs1, rs2, imm } => format!("blt  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bge { rs1, rs2, imm } => format!("bge  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bltu { rs1, rs2, imm } => format!("bltu {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bgeu { rs1, rs2, imm } => format!("bgeu {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Lui { rd, imm } => format!("lui  {}, {imm}", reg_name(rd)),
        Auipc { rd, imm } => format!("auipc {}, {imm}", reg_name(rd)),
        Jal { rd, imm } => format!("jal  {}, {imm}", reg_name(rd)),
        Jalr { rd, rs1, imm } => format!("jalr {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Ecall => "ecall".to_string(),
        Halt => "halt".to_string(),
    }
}

fn reg_name(i: u8) -> &'static str {
    match i {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "",
    }
}

