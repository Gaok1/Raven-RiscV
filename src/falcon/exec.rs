// falcon/exec.rs
use crate::falcon::{registers::Cpu, memory::Bus, instruction::Instruction};

pub fn step<B: Bus>(cpu: &mut Cpu, mem: &mut B) -> bool {
    let pc = cpu.pc;
    let word = mem.load32(pc);
    let instr = match crate::falcon::decoder::decode(word) {
        Ok(i) => i,
        Err(_) => return false, // sinaliza erro/halt
    };
    cpu.pc = pc.wrapping_add(4);

    match instr {
        // R
        Instruction::Add{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1).wrapping_add(cpu.read(rs2))),
        Instruction::Sub{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1).wrapping_sub(cpu.read(rs2))),
        Instruction::And{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1) & cpu.read(rs2)),
        Instruction::Or {rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1) | cpu.read(rs2)),
        Instruction::Xor{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1) ^ cpu.read(rs2)),
        Instruction::Sll{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1) << (cpu.read(rs2) & 0x1F)),
        Instruction::Srl{rd,rs1,rs2} => cpu.write(rd, cpu.read(rs1) >> (cpu.read(rs2) & 0x1F)),
        Instruction::Sra{rd,rs1,rs2} => {
            let s = (cpu.read(rs2) & 0x1F) as u32;
            cpu.write(rd, ((cpu.read(rs1) as i32) >> s) as u32);
        }
        Instruction::Slt{rd,rs1,rs2} => {
            let v = (cpu.read(rs1) as i32) < (cpu.read(rs2) as i32);
            cpu.write(rd, v as u32);
        }
        Instruction::Sltu{rd,rs1,rs2} => cpu.write(rd, (cpu.read(rs1) < cpu.read(rs2)) as u32),
        Instruction::Mul{rd,rs1,rs2} => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as i32 as i64);
            cpu.write(rd, res as u32);
        }
        Instruction::Mulh{rd,rs1,rs2} => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as i32 as i64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Mulhsu{rd,rs1,rs2} => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as u64 as i64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Mulhu{rd,rs1,rs2} => {
            let res = (cpu.read(rs1) as u64).wrapping_mul(cpu.read(rs2) as u64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Div{rd,rs1,rs2} => {
            let num = cpu.read(rs1) as i32;
            let den = cpu.read(rs2) as i32;
            let val = if den == 0 { -1 }
                      else if num == i32::MIN && den == -1 { i32::MIN }
                      else { num.wrapping_div(den) };
            cpu.write(rd, val as u32);
        }
        Instruction::Divu{rd,rs1,rs2} => {
            let den = cpu.read(rs2);
            let val = if den == 0 { u32::MAX } else { cpu.read(rs1).wrapping_div(den) };
            cpu.write(rd, val);
        }
        Instruction::Rem{rd,rs1,rs2} => {
            let num = cpu.read(rs1) as i32;
            let den = cpu.read(rs2) as i32;
            let val = if den == 0 { num }
                      else if num == i32::MIN && den == -1 { 0 }
                      else { num.wrapping_rem(den) };
            cpu.write(rd, val as u32);
        }
        Instruction::Remu{rd,rs1,rs2} => {
            let den = cpu.read(rs2);
            let val = if den == 0 { cpu.read(rs1) }
                      else { cpu.read(rs1).wrapping_rem(den) };
            cpu.write(rd, val);
        }

        // I
        Instruction::Addi{rd,rs1,imm} => cpu.write(rd, cpu.read(rs1).wrapping_add(imm as u32)),
        Instruction::Andi{rd,rs1,imm} => cpu.write(rd, cpu.read(rs1) & (imm as u32)),
        Instruction::Ori {rd,rs1,imm} => cpu.write(rd, cpu.read(rs1) | (imm as u32)),
        Instruction::Xori{rd,rs1,imm} => cpu.write(rd, cpu.read(rs1) ^ (imm as u32)),
        Instruction::Slti{rd,rs1,imm} => {
            let v = (cpu.read(rs1) as i32) < imm;
            cpu.write(rd, v as u32);
        }
        Instruction::Sltiu{rd,rs1,imm} => {
            cpu.write(rd, (cpu.read(rs1) < imm as u32) as u32);
        }
        Instruction::Slli{rd,rs1,shamt} => cpu.write(rd, cpu.read(rs1) << (shamt & 0x1F)),
        Instruction::Srli{rd,rs1,shamt} => cpu.write(rd, cpu.read(rs1) >> (shamt & 0x1F)),
        Instruction::Srai{rd,rs1,shamt} => cpu.write(rd, ((cpu.read(rs1) as i32) >> (shamt & 0x1F)) as u32),

        Instruction::Lb{rd,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, (mem.load8(a) as i8 as i32) as u32);
        }
        Instruction::Lh{rd,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, (mem.load16(a) as i16 as i32) as u32);
        }
        Instruction::Lw{rd,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.load32(a));
        }
        Instruction::Lbu{rd,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.load8(a) as u32);
        }
        Instruction::Lhu{rd,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.load16(a) as u32);
        }

        Instruction::Sb{rs2,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store8(a, cpu.read(rs2) as u8);
        }
        Instruction::Sh{rs2,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store16(a, cpu.read(rs2) as u16);
        }
        Instruction::Sw{rs2,rs1,imm} => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store32(a, cpu.read(rs2));
        }

        // Branches (offset relativo ao PC da instrução fetchada)
        Instruction::Beq{rs1,rs2,imm} if cpu.read(rs1)==cpu.read(rs2) => cpu.pc = pc.wrapping_add(imm as u32),
        Instruction::Bne{rs1,rs2,imm} if cpu.read(rs1)!=cpu.read(rs2) => cpu.pc = pc.wrapping_add(imm as u32),
        Instruction::Blt{rs1,rs2,imm} if (cpu.read(rs1) as i32) <  (cpu.read(rs2) as i32) => cpu.pc = pc.wrapping_add(imm as u32),
        Instruction::Bge{rs1,rs2,imm} if (cpu.read(rs1) as i32) >= (cpu.read(rs2) as i32) => cpu.pc = pc.wrapping_add(imm as u32),
        Instruction::Bltu{rs1,rs2,imm} if cpu.read(rs1) <  cpu.read(rs2) => cpu.pc = pc.wrapping_add(imm as u32),
        Instruction::Bgeu{rs1,rs2,imm} if cpu.read(rs1) >= cpu.read(rs2) => cpu.pc = pc.wrapping_add(imm as u32),

        Instruction::Jal{rd,imm} => { cpu.write(rd, pc.wrapping_add(4)); cpu.pc = pc.wrapping_add(imm as u32); }
        Instruction::Jalr{rd,rs1,imm} => {
            let target = (cpu.read(rs1).wrapping_add(imm as u32)) & !1;
            cpu.write(rd, pc.wrapping_add(4));
            cpu.pc = target;
        }
        Instruction::Lui{rd,imm}    => cpu.write(rd, imm as u32),
        Instruction::Auipc{rd,imm}  => cpu.write(rd, pc.wrapping_add(imm as u32)),

        Instruction::Ecall | Instruction::Ebreak => return false, // HALT
        _ => {}
    }
    true
}



// em src/falcon/exec.rs (logo abaixo de `step`)
pub fn run<B: crate::falcon::memory::Bus>(cpu: &mut crate::falcon::registers::Cpu,
                                          mem: &mut B,
                                          max_steps: usize) -> usize {
    let mut steps = 0;
    while steps < max_steps && step(cpu, mem) {
        steps += 1;
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::falcon::{Ram, instruction::Instruction};
    use crate::falcon::encoder;

    #[test]
    fn ecall_halts() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let inst = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, inst);
        assert!(!step(&mut cpu, &mut mem));
    }

    #[test]
    fn ebreak_halts() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let inst = encoder::encode(Instruction::Ebreak).unwrap();
        mem.store32(0, inst);
        assert!(!step(&mut cpu, &mut mem));
    }

    #[test]
    fn sw_stores_word() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        cpu.write(1, 0xDEADBEEF); // valor a ser armazenado
        cpu.write(2, 0x20);       // endereço base
        let sw = encoder::encode(Instruction::Sw { rs2: 1, rs1: 2, imm: 0 }).unwrap();
        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, sw);
        mem.store32(4, ecall);
        assert!(step(&mut cpu, &mut mem));
        assert_eq!(mem.load32(0x20), 0xDEADBEEF);
        assert!(!step(&mut cpu, &mut mem));
    }
}
