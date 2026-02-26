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
