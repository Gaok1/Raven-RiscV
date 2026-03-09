// falcon/exec.rs
use crate::falcon::{errors::FalconError, instruction::Instruction, memory::Bus, registers::Cpu};

use crate::falcon::syscall::handle_syscall;
use crate::ui::Console;

pub fn step<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let pc = cpu.pc;
    let word = mem.fetch32(pc)?;
    let instr = match crate::falcon::decoder::decode(word) {
        Ok(i) => i,
        Err(e) => {
            console.push_error(format!(
                "Invalid instruction 0x{word:08X} at 0x{pc:08X}: {e}"
            ));
            return Err(e);
        }
    };
    cpu.pc = pc.wrapping_add(4);

    match instr {
        i @ (
            Instruction::Add { .. }
                | Instruction::Sub { .. }
                | Instruction::And { .. }
                | Instruction::Or { .. }
                | Instruction::Xor { .. }
                | Instruction::Sll { .. }
                | Instruction::Srl { .. }
                | Instruction::Sra { .. }
                | Instruction::Slt { .. }
                | Instruction::Sltu { .. }
                | Instruction::Mul { .. }
                | Instruction::Mulh { .. }
                | Instruction::Mulhsu { .. }
                | Instruction::Mulhu { .. }
                | Instruction::Div { .. }
                | Instruction::Divu { .. }
                | Instruction::Rem { .. }
                | Instruction::Remu { .. }
        ) => {
            return exec_rtype(i, cpu, mem, console);
        }
        i @ (
            Instruction::Addi { .. }
                | Instruction::Andi { .. }
                | Instruction::Ori { .. }
                | Instruction::Xori { .. }
                | Instruction::Slti { .. }
                | Instruction::Sltiu { .. }
                | Instruction::Slli { .. }
                | Instruction::Srli { .. }
                | Instruction::Srai { .. }
        ) => {
            return exec_itype(i, cpu, mem, console);
        }
        i @ (
            Instruction::Lb { .. }
                | Instruction::Lh { .. }
                | Instruction::Lw { .. }
                | Instruction::Lbu { .. }
                | Instruction::Lhu { .. }
        ) => {
            return exec_loads(i, cpu, mem, console);
        }
        i @ (Instruction::Sb { .. } | Instruction::Sh { .. } | Instruction::Sw { .. }) => {
            return exec_stores(i, cpu, mem, console);
        }

        Instruction::Beq { rs1, rs2, imm } if cpu.read(rs1) == cpu.read(rs2) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Bne { rs1, rs2, imm } if cpu.read(rs1) != cpu.read(rs2) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Blt { rs1, rs2, imm } if (cpu.read(rs1) as i32) < (cpu.read(rs2) as i32) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Bge { rs1, rs2, imm } if (cpu.read(rs1) as i32) >= (cpu.read(rs2) as i32) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Bltu { rs1, rs2, imm } if cpu.read(rs1) < cpu.read(rs2) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Bgeu { rs1, rs2, imm } if cpu.read(rs1) >= cpu.read(rs2) => {
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }

        Instruction::Jal { rd, imm } => {
            cpu.write(rd, pc.wrapping_add(4));
            cpu.pc = pc.wrapping_add(imm as u32);
            return Ok(true);
        }
        Instruction::Jalr { rd, rs1, imm } => {
            let target = (cpu.read(rs1).wrapping_add(imm as u32)) & !1;
            cpu.write(rd, pc.wrapping_add(4));
            cpu.pc = target;
            return Ok(true);
        }
        Instruction::Lui { rd, imm } => {
            cpu.write(rd, imm as u32);
            return Ok(true);
        }
        Instruction::Auipc { rd, imm } => {
            cpu.write(rd, pc.wrapping_add(imm as u32));
            return Ok(true);
        }

        Instruction::Ecall => {
            let old_pc = pc;
            let code = cpu.read(17);
            let cont = handle_syscall(code, cpu, mem, console)?;
            if !cont && console.reading {
                cpu.pc = old_pc;
                return Ok(false);
            }
            return Ok(cont);
        }
        Instruction::Ebreak | Instruction::Halt => {
            console.push_error(format!("EBREAK/HALT at 0x{pc:08X}"));
            return Ok(false);
        }
        Instruction::Fence => {} // nop in single-core simulator

        // RV32A
        i @ (
            Instruction::LrW { .. }
                | Instruction::ScW { .. }
                | Instruction::AmoswapW { .. }
                | Instruction::AmoaddW  { .. }
                | Instruction::AmoxorW  { .. }
                | Instruction::AmoandW  { .. }
                | Instruction::AmoorW   { .. }
                | Instruction::AmomaxW  { .. }
                | Instruction::AmominW  { .. }
                | Instruction::AmomaxuW { .. }
                | Instruction::AmominuW { .. }
        ) => {
            return exec_amo(i, cpu, mem, console);
        }

        // RV32F
        i @ (
            Instruction::Flw { .. }
                | Instruction::Fsw { .. }
                | Instruction::FaddS { .. }
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
                | Instruction::FcvtWS { .. }
                | Instruction::FcvtWuS { .. }
                | Instruction::FcvtSW { .. }
                | Instruction::FcvtSWu { .. }
                | Instruction::FmvXW { .. }
                | Instruction::FmvWX { .. }
                | Instruction::FclassS { .. }
                | Instruction::FmaddS { .. }
                | Instruction::FmsubS { .. }
                | Instruction::FnmsubS { .. }
                | Instruction::FnmaddS { .. }
        ) => {
            return exec_fp(i, cpu, mem, console);
        }

        _ => {}
    }

    Ok(true)
}

fn exec_rtype<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    _mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        Instruction::Add { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1).wrapping_add(cpu.read(rs2)));
        }
        Instruction::Sub { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1).wrapping_sub(cpu.read(rs2)));
        }
        Instruction::And { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1) & cpu.read(rs2));
        }
        Instruction::Or { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1) | cpu.read(rs2));
        }
        Instruction::Xor { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1) ^ cpu.read(rs2));
        }
        Instruction::Sll { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1) << (cpu.read(rs2) & 0x1F));
        }
        Instruction::Srl { rd, rs1, rs2 } => {
            cpu.write(rd, cpu.read(rs1) >> (cpu.read(rs2) & 0x1F));
        }
        Instruction::Sra { rd, rs1, rs2 } => {
            let s = (cpu.read(rs2) & 0x1F) as u32;
            cpu.write(rd, ((cpu.read(rs1) as i32) >> s) as u32);
        }
        Instruction::Slt { rd, rs1, rs2 } => {
            let v = (cpu.read(rs1) as i32) < (cpu.read(rs2) as i32);
            cpu.write(rd, v as u32);
        }
        Instruction::Sltu { rd, rs1, rs2 } => {
            cpu.write(rd, (cpu.read(rs1) < cpu.read(rs2)) as u32);
        }
        Instruction::Mul { rd, rs1, rs2 } => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as i32 as i64);
            cpu.write(rd, res as u32);
        }
        Instruction::Mulh { rd, rs1, rs2 } => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as i32 as i64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Mulhsu { rd, rs1, rs2 } => {
            let res = (cpu.read(rs1) as i32 as i64).wrapping_mul(cpu.read(rs2) as u64 as i64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Mulhu { rd, rs1, rs2 } => {
            let res = (cpu.read(rs1) as u64).wrapping_mul(cpu.read(rs2) as u64);
            cpu.write(rd, (res >> 32) as u32);
        }
        Instruction::Div { rd, rs1, rs2 } => {
            let num = cpu.read(rs1) as i32;
            let den = cpu.read(rs2) as i32;
            if den == 0 {
                console.push_error("Division by zero");
                return Ok(false);
            }
            let val = num.wrapping_div(den);
            cpu.write(rd, val as u32);
        }
        Instruction::Divu { rd, rs1, rs2 } => {
            let den = cpu.read(rs2);
            if den == 0 {
                console.push_error("Division by zero");
                return Ok(false);
            }
            let val = cpu.read(rs1).wrapping_div(den);
            cpu.write(rd, val);
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            let num = cpu.read(rs1) as i32;
            let den = cpu.read(rs2) as i32;
            if den == 0 {
                console.push_error("Division by zero");
                return Ok(false);
            }
            let val = num.wrapping_rem(den);
            cpu.write(rd, val as u32);
        }
        Instruction::Remu { rd, rs1, rs2 } => {
            let den = cpu.read(rs2);
            if den == 0 {
                console.push_error("Division by zero");
                return Ok(false);
            }
            let val = cpu.read(rs1).wrapping_rem(den);
            cpu.write(rd, val);
        }
        _ => unreachable!(),
    }
    Ok(true)
}

fn exec_itype<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    _mem: &mut B,
    _console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        Instruction::Addi { rd, rs1, imm } => {
            cpu.write(rd, cpu.read(rs1).wrapping_add(imm as u32));
        }
        Instruction::Andi { rd, rs1, imm } => {
            cpu.write(rd, cpu.read(rs1) & (imm as u32));
        }
        Instruction::Ori { rd, rs1, imm } => {
            cpu.write(rd, cpu.read(rs1) | (imm as u32));
        }
        Instruction::Xori { rd, rs1, imm } => {
            cpu.write(rd, cpu.read(rs1) ^ (imm as u32));
        }
        Instruction::Slti { rd, rs1, imm } => {
            let v = (cpu.read(rs1) as i32) < imm;
            cpu.write(rd, v as u32);
        }
        Instruction::Sltiu { rd, rs1, imm } => {
            cpu.write(rd, (cpu.read(rs1) < imm as u32) as u32);
        }
        Instruction::Slli { rd, rs1, shamt } => {
            cpu.write(rd, cpu.read(rs1) << (shamt & 0x1F));
        }
        Instruction::Srli { rd, rs1, shamt } => {
            cpu.write(rd, cpu.read(rs1) >> (shamt & 0x1F));
        }
        Instruction::Srai { rd, rs1, shamt } => {
            cpu.write(rd, ((cpu.read(rs1) as i32) >> (shamt & 0x1F)) as u32);
        }
        _ => unreachable!(),
    }
    Ok(true)
}

fn exec_loads<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    mem: &mut B,
    _console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        Instruction::Lb { rd, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, (mem.dcache_read8(a)? as i8 as i32) as u32);
        }
        Instruction::Lh { rd, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, (mem.dcache_read16(a)? as i16 as i32) as u32);
        }
        Instruction::Lw { rd, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.dcache_read32(a)?);
        }
        Instruction::Lbu { rd, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.dcache_read8(a)? as u32);
        }
        Instruction::Lhu { rd, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            cpu.write(rd, mem.dcache_read16(a)? as u32);
        }
        _ => unreachable!(),
    }
    Ok(true)
}

fn exec_stores<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    mem: &mut B,
    _console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        Instruction::Sb { rs2, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store8(a, cpu.read(rs2) as u8)?;
        }
        Instruction::Sh { rs2, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store16(a, cpu.read(rs2) as u16)?;
        }
        Instruction::Sw { rs2, rs1, imm } => {
            let a = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store32(a, cpu.read(rs2))?;
        }
        _ => unreachable!(),
    }
    Ok(true)
}

fn exec_fp<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    mem: &mut B,
    _console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        // Load/Store
        Instruction::Flw { rd, rs1, imm } => {
            let addr = cpu.read(rs1).wrapping_add(imm as u32);
            let bits = mem.dcache_read32(addr)?;
            cpu.fwrite_bits(rd, bits);
        }
        Instruction::Fsw { rs2, rs1, imm } => {
            let addr = cpu.read(rs1).wrapping_add(imm as u32);
            mem.store32(addr, cpu.fread_bits(rs2))?;
        }

        // Arithmetic
        Instruction::FaddS { rd, rs1, rs2 } => cpu.fwrite(rd, cpu.fread(rs1) + cpu.fread(rs2)),
        Instruction::FsubS { rd, rs1, rs2 } => cpu.fwrite(rd, cpu.fread(rs1) - cpu.fread(rs2)),
        Instruction::FmulS { rd, rs1, rs2 } => cpu.fwrite(rd, cpu.fread(rs1) * cpu.fread(rs2)),
        Instruction::FdivS { rd, rs1, rs2 } => cpu.fwrite(rd, cpu.fread(rs1) / cpu.fread(rs2)),
        Instruction::FsqrtS { rd, rs1 }     => cpu.fwrite(rd, cpu.fread(rs1).sqrt()),
        Instruction::FminS { rd, rs1, rs2 } => {
            // RISC-V fmin: if either is NaN return the other; -0.0 < +0.0
            let a = cpu.fread(rs1);
            let b = cpu.fread(rs2);
            cpu.fwrite(rd, if a.is_nan() { b } else if b.is_nan() { a } else if a == 0.0 && b == 0.0 { if a.is_sign_negative() { a } else { b } } else { a.min(b) });
        }
        Instruction::FmaxS { rd, rs1, rs2 } => {
            let a = cpu.fread(rs1);
            let b = cpu.fread(rs2);
            cpu.fwrite(rd, if a.is_nan() { b } else if b.is_nan() { a } else if a == 0.0 && b == 0.0 { if a.is_sign_positive() { a } else { b } } else { a.max(b) });
        }

        // Sign injection
        Instruction::FsgnjS  { rd, rs1, rs2 } => {
            let bits = (cpu.fread_bits(rs1) & 0x7FFF_FFFF) | (cpu.fread_bits(rs2) & 0x8000_0000);
            cpu.fwrite_bits(rd, bits);
        }
        Instruction::FsgnjnS { rd, rs1, rs2 } => {
            let bits = (cpu.fread_bits(rs1) & 0x7FFF_FFFF) | (!cpu.fread_bits(rs2) & 0x8000_0000);
            cpu.fwrite_bits(rd, bits);
        }
        Instruction::FsgnjxS { rd, rs1, rs2 } => {
            let bits = cpu.fread_bits(rs1) ^ (cpu.fread_bits(rs2) & 0x8000_0000);
            cpu.fwrite_bits(rd, bits);
        }

        // Comparison (result → integer register)
        Instruction::FeqS { rd, rs1, rs2 } => {
            cpu.write(rd, if cpu.fread(rs1) == cpu.fread(rs2) { 1 } else { 0 });
        }
        Instruction::FltS { rd, rs1, rs2 } => {
            cpu.write(rd, if cpu.fread(rs1) < cpu.fread(rs2) { 1 } else { 0 });
        }
        Instruction::FleS { rd, rs1, rs2 } => {
            cpu.write(rd, if cpu.fread(rs1) <= cpu.fread(rs2) { 1 } else { 0 });
        }

        // Conversion
        Instruction::FcvtWS  { rd, rs1, .. } => {
            let v = cpu.fread(rs1);
            let result = if v.is_nan() { i32::MAX as u32 }
                         else { (v.clamp(i32::MIN as f32, i32::MAX as f32) as i32) as u32 };
            cpu.write(rd, result);
        }
        Instruction::FcvtWuS { rd, rs1, .. } => {
            let v = cpu.fread(rs1);
            let result = if v.is_nan() || v < 0.0 { 0 }
                         else if v >= u32::MAX as f32 { u32::MAX }
                         else { v as u32 };
            cpu.write(rd, result);
        }
        Instruction::FcvtSW  { rd, rs1 } => {
            cpu.fwrite(rd, cpu.read(rs1) as i32 as f32);
        }
        Instruction::FcvtSWu { rd, rs1 } => {
            cpu.fwrite(rd, cpu.read(rs1) as f32);
        }

        // Move (bit-pattern transfers)
        Instruction::FmvXW { rd, rs1 } => { cpu.write(rd, cpu.fread_bits(rs1)); }
        Instruction::FmvWX { rd, rs1 } => { cpu.fwrite_bits(rd, cpu.read(rs1)); }

        // Classify
        Instruction::FclassS { rd, rs1 } => {
            let bits = cpu.fread_bits(rs1);
            let exp  = (bits >> 23) & 0xFF;
            let mant = bits & 0x007F_FFFF;
            let sign = bits >> 31;
            let result: u32 = match (sign, exp, mant) {
                (1, 0xFF, m) if m != 0 => 0x100, // signaling NaN (bit 8)
                (0, 0xFF, m) if m != 0 => 0x200, // quiet NaN (bit 9)
                (1, 0xFF, 0)           => 0x001, // -infinity
                (0, 0xFF, 0)           => 0x080, // +infinity
                (1, 0,    0)           => 0x008, // -zero
                (0, 0,    0)           => 0x010, // +zero
                (1, 0,    _)           => 0x004, // -subnormal
                (0, 0,    _)           => 0x020, // +subnormal
                (1, _,    _)           => 0x002, // -normal
                (0, _,    _)           => 0x040, // +normal
                _                      => 0x000,
            };
            cpu.write(rd, result);
        }

        // Fused multiply-add
        Instruction::FmaddS  { rd, rs1, rs2, rs3 } => {
            cpu.fwrite(rd, cpu.fread(rs1) * cpu.fread(rs2) + cpu.fread(rs3));
        }
        Instruction::FmsubS  { rd, rs1, rs2, rs3 } => {
            cpu.fwrite(rd, cpu.fread(rs1) * cpu.fread(rs2) - cpu.fread(rs3));
        }
        Instruction::FnmsubS { rd, rs1, rs2, rs3 } => {
            cpu.fwrite(rd, -(cpu.fread(rs1) * cpu.fread(rs2)) + cpu.fread(rs3));
        }
        Instruction::FnmaddS { rd, rs1, rs2, rs3 } => {
            cpu.fwrite(rd, -(cpu.fread(rs1) * cpu.fread(rs2)) - cpu.fread(rs3));
        }

        _ => unreachable!(),
    }
    Ok(true)
}

fn exec_amo<B: Bus>(
    instr: Instruction,
    cpu: &mut Cpu,
    mem: &mut B,
    _console: &mut Console,
) -> Result<bool, FalconError> {
    match instr {
        Instruction::LrW { rd, rs1 } => {
            let addr = cpu.read(rs1);
            let val = mem.dcache_read32(addr)?;
            cpu.write(rd, val);
            cpu.lr_reservation = Some(addr);
        }
        Instruction::ScW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            if cpu.lr_reservation == Some(addr) {
                mem.store32(addr, cpu.read(rs2))?;
                cpu.write(rd, 0); // success
            } else {
                cpu.write(rd, 1); // failure
            }
            cpu.lr_reservation = None;
        }
        Instruction::AmoswapW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, cpu.read(rs2))?;
            cpu.write(rd, old);
        }
        Instruction::AmoaddW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old.wrapping_add(cpu.read(rs2)))?;
            cpu.write(rd, old);
        }
        Instruction::AmoxorW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old ^ cpu.read(rs2))?;
            cpu.write(rd, old);
        }
        Instruction::AmoandW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old & cpu.read(rs2))?;
            cpu.write(rd, old);
        }
        Instruction::AmoorW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old | cpu.read(rs2))?;
            cpu.write(rd, old);
        }
        Instruction::AmomaxW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            let result = (old as i32).max(cpu.read(rs2) as i32) as u32;
            mem.store32(addr, result)?;
            cpu.write(rd, old);
        }
        Instruction::AmominW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            let result = (old as i32).min(cpu.read(rs2) as i32) as u32;
            mem.store32(addr, result)?;
            cpu.write(rd, old);
        }
        Instruction::AmomaxuW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old.max(cpu.read(rs2)))?;
            cpu.write(rd, old);
        }
        Instruction::AmominuW { rd, rs1, rs2 } => {
            let addr = cpu.read(rs1);
            let old = mem.dcache_read32(addr)?;
            mem.store32(addr, old.min(cpu.read(rs2)))?;
            cpu.write(rd, old);
        }
        _ => unreachable!(),
    }
    Ok(true)
}

// em src/falcon/exec.rs (logo abaixo de `step`)
#[allow(dead_code)]
pub fn run<B: crate::falcon::memory::Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut Console,
    max_steps: usize,
) -> Result<usize, FalconError> {
    let mut steps = 0;
    while steps < max_steps {
        match step(cpu, mem, console)? {
            true => steps += 1,
            false => break,
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::falcon::encoder;
    use crate::falcon::{instruction::Instruction, Ram};
    use crate::falcon::decoder::decode;

    // RV32A roundtrip: encode → decode must recover the same instruction
    #[test]
    fn amo_encode_decode_roundtrip() {
        let cases: &[Instruction] = &[
            Instruction::LrW      { rd: 1, rs1: 2 },
            Instruction::ScW      { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmoswapW { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmoaddW  { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmoxorW  { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmoandW  { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmoorW   { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmomaxW  { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmominW  { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmomaxuW { rd: 1, rs1: 2, rs2: 3 },
            Instruction::AmominuW { rd: 1, rs1: 2, rs2: 3 },
        ];
        for &instr in cases {
            let word = encoder::encode(instr).expect("encode failed");
            let decoded = decode(word).expect("decode failed");
            // compare discriminant and fields via Debug string (simplest roundtrip check)
            assert_eq!(format!("{instr:?}"), format!("{decoded:?}"), "roundtrip failed for {instr:?}");
        }
    }

    // LR/SC: successful reservation → sc stores and returns 0
    #[test]
    fn lr_sc_success() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        mem.store32(0x10, 0xABCD).unwrap();
        cpu.write(1, 0x10); // address in x1

        let lr   = encoder::encode(Instruction::LrW { rd: 2, rs1: 1 }).unwrap();
        let sc   = encoder::encode(Instruction::ScW { rd: 3, rs1: 1, rs2: 4 }).unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, lr).unwrap();
        mem.store32(4, sc).unwrap();
        mem.store32(8, halt).unwrap();

        cpu.write(4, 0x1234); // value to store via sc

        step(&mut cpu, &mut mem, &mut console).unwrap(); // lr.w x2, (x1)
        assert_eq!(cpu.read(2), 0xABCD);
        assert_eq!(cpu.lr_reservation, Some(0x10));

        step(&mut cpu, &mut mem, &mut console).unwrap(); // sc.w x3, x4, (x1)
        assert_eq!(cpu.read(3), 0); // success
        assert_eq!(mem.load32(0x10).unwrap(), 0x1234);
        assert_eq!(cpu.lr_reservation, None);
    }

    // SC without prior LR → failure (rd=1), memory unchanged
    #[test]
    fn sc_without_lr_fails() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        mem.store32(0x10, 0xABCD).unwrap();
        cpu.write(1, 0x10);
        cpu.write(4, 0x1234);

        let sc   = encoder::encode(Instruction::ScW { rd: 3, rs1: 1, rs2: 4 }).unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, sc).unwrap();
        mem.store32(4, halt).unwrap();

        step(&mut cpu, &mut mem, &mut console).unwrap();
        assert_eq!(cpu.read(3), 1); // failure
        assert_eq!(mem.load32(0x10).unwrap(), 0xABCD); // unchanged
    }

    // AMOADD: mem[addr] += rs2, rd = old value
    #[test]
    fn amoadd_w() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        mem.store32(0x10, 10).unwrap();
        cpu.write(1, 0x10); // address
        cpu.write(2, 5);    // operand

        let amo  = encoder::encode(Instruction::AmoaddW { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, amo).unwrap();
        mem.store32(4, halt).unwrap();

        step(&mut cpu, &mut mem, &mut console).unwrap();
        assert_eq!(cpu.read(3), 10);           // old value
        assert_eq!(mem.load32(0x10).unwrap(), 15); // new value
    }

    #[test]
    fn halt_halts() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let mut console = crate::ui::Console::default();
        let inst = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, inst).unwrap();
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn ebreak_halts() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let mut console = crate::ui::Console::default();
        let inst = encoder::encode(Instruction::Ebreak).unwrap();
        mem.store32(0, inst).unwrap();
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn halt_and_ebreak_encode_same() {
        let halt = encoder::encode(Instruction::Halt).unwrap();
        let ebreak = encoder::encode(Instruction::Ebreak).unwrap();
        assert_eq!(halt, 0x0010_0073);
        assert_eq!(ebreak, 0x0010_0073);
        assert_eq!(halt, ebreak);
    }

    #[test]
    fn decode_ebreak_is_canonical() {
        let inst = crate::falcon::decoder::decode(0x0010_0073).unwrap();
        assert!(matches!(inst, Instruction::Ebreak));
    }

    #[test]
    fn sw_stores_word() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        cpu.write(1, 0xDEADBEEF);
        cpu.write(2, 0x20);
        let sw = encoder::encode(Instruction::Sw {
            rs2: 1,
            rs1: 2,
            imm: 0,
        })
        .unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, sw).unwrap();
        mem.store32(4, halt).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(mem.load32(0x20).unwrap(), 0xDEADBEEF);
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn add_adds() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(8);
        let mut console = crate::ui::Console::default();
        cpu.write(1, 2);
        cpu.write(2, 3);
        let add = encoder::encode(Instruction::Add {
            rd: 3,
            rs1: 1,
            rs2: 2,
        })
        .unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, add).unwrap();
        mem.store32(4, halt).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(3), 5);
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn addi_adds_immediate() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(8);
        let mut console = crate::ui::Console::default();
        cpu.write(1, 5);
        let addi = encoder::encode(Instruction::Addi {
            rd: 2,
            rs1: 1,
            imm: 3,
        })
        .unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, addi).unwrap();
        mem.store32(4, halt).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(2), 8);
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn lw_loads_word() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        mem.store32(0x20, 0xCAFEBABE).unwrap();
        cpu.write(1, 0x20);
        let lw = encoder::encode(Instruction::Lw {
            rd: 2,
            rs1: 1,
            imm: 0,
        })
        .unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, lw).unwrap();
        mem.store32(4, halt).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(2), 0xCAFEBABE);
        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn syscall_print_int() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let mut console = crate::ui::Console::default();
        cpu.write(10, 42);
        cpu.write(17, 1000);
        let inst = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, inst).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.stdout, b"42");
    }

    #[test]
    fn syscall_print_string() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        let addr = 8u32;
        let msg = b"hi\0";
        for (i, b) in msg.iter().enumerate() {
            mem.store8(addr + i as u32, *b).unwrap();
        }
        cpu.write(10, addr);
        cpu.write(17, 1001);
        let inst = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, inst).unwrap();
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.stdout, b"hi");
    }

    #[test]
    fn syscall_read_string() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        console.push_input("hi");
        let addr = 8u32;
        cpu.write(10, addr);
        cpu.write(17, 1003);
        let inst = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, inst).unwrap();

        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(mem.load8(addr).unwrap(), b'h');
        assert_eq!(mem.load8(addr + 1).unwrap(), b'i');
        assert_eq!(mem.load8(addr + 2).unwrap(), 0);
    }

    #[test]
    fn syscall_read_waits_for_input() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        let addr = 8u32;
        cpu.write(10, addr);
        cpu.write(17, 1003);
        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        let halt = encoder::encode(Instruction::Halt).unwrap();
        mem.store32(0, ecall).unwrap();
        mem.store32(4, halt).unwrap();

        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.pc, 0);

        console.push_input("hi");
        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.pc, 4);
        assert_eq!(mem.load8(addr).unwrap(), b'h');
        assert_eq!(mem.load8(addr + 1).unwrap(), b'i');
        assert_eq!(mem.load8(addr + 2).unwrap(), 0);

        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    }

    #[test]
    fn linux_write_writes_stdout() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        let addr = 8u32;
        let bytes = b"hi\n";
        for (i, b) in bytes.iter().enumerate() {
            mem.store8(addr + i as u32, *b).unwrap();
        }

        cpu.write(17, 64); // write
        cpu.write(10, 1); // fd=stdout
        cpu.write(11, addr); // buf
        cpu.write(12, bytes.len() as u32); // count

        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, ecall).unwrap();

        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.stdout, bytes);
        assert_eq!(cpu.read(10), bytes.len() as u32);
    }

    #[test]
    fn linux_read_reads_line() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();
        console.push_input("hi");

        let addr = 8u32;
        cpu.write(17, 63); // read
        cpu.write(10, 0); // fd=stdin
        cpu.write(11, addr); // buf
        cpu.write(12, 8); // count

        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, ecall).unwrap();

        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(10), 3);
        assert_eq!(mem.load8(addr).unwrap(), b'h');
        assert_eq!(mem.load8(addr + 1).unwrap(), b'i');
        assert_eq!(mem.load8(addr + 2).unwrap(), b'\n');
    }

    #[test]
    fn linux_getrandom_writes_bytes() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        let addr = 8u32;
        for i in 0..8 {
            mem.store8(addr + i, 0xAA).unwrap();
        }

        cpu.write(17, 278); // getrandom
        cpu.write(10, addr); // buf
        cpu.write(11, 8); // buflen
        cpu.write(12, 0); // flags

        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, ecall).unwrap();

        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(10), 8);

        let mut out = Vec::new();
        for i in 0..8 {
            out.push(mem.load8(addr + i).unwrap());
        }
        assert_ne!(out, vec![0xAA; 8]);
    }

    #[test]
    fn linux_getrandom_invalid_flags_returns_einval() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(64);
        let mut console = crate::ui::Console::default();

        let addr = 8u32;
        cpu.write(17, 278); // getrandom
        cpu.write(10, addr); // buf
        cpu.write(11, 8); // buflen
        cpu.write(12, 0x8000); // invalid flags

        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, ecall).unwrap();

        assert!(step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.read(10), (-22i32) as u32);
    }

    #[test]
    fn linux_exit_sets_exit_code() {
        let mut cpu = Cpu::default();
        let mut mem = Ram::new(4);
        let mut console = crate::ui::Console::default();

        cpu.write(17, 93); // exit
        cpu.write(10, 7); // status

        let ecall = encoder::encode(Instruction::Ecall).unwrap();
        mem.store32(0, ecall).unwrap();

        assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
        assert_eq!(cpu.exit_code, Some(7));
    }
}
