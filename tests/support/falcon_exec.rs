
use super::*;
use crate::falcon::decoder::decode;
use crate::falcon::encoder;
use crate::falcon::{Ram, instruction::Instruction};

// RV32A roundtrip: encode → decode must recover the same instruction
#[test]
fn amo_encode_decode_roundtrip() {
    let cases: &[Instruction] = &[
        Instruction::LrW { rd: 1, rs1: 2 },
        Instruction::ScW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmoswapW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmoaddW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmoxorW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmoandW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmoorW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmomaxW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmominW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmomaxuW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
        Instruction::AmominuW {
            rd: 1,
            rs1: 2,
            rs2: 3,
        },
    ];
    for &instr in cases {
        let word = encoder::encode(instr).expect("encode failed");
        let decoded = decode(word).expect("decode failed");
        // compare discriminant and fields via Debug string (simplest roundtrip check)
        assert_eq!(
            format!("{instr:?}"),
            format!("{decoded:?}"),
            "roundtrip failed for {instr:?}"
        );
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

    let lr = encoder::encode(Instruction::LrW { rd: 2, rs1: 1 }).unwrap();
    let sc = encoder::encode(Instruction::ScW {
        rd: 3,
        rs1: 1,
        rs2: 4,
    })
    .unwrap();
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

    let sc = encoder::encode(Instruction::ScW {
        rd: 3,
        rs1: 1,
        rs2: 4,
    })
    .unwrap();
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
    cpu.write(2, 5); // operand

    let amo = encoder::encode(Instruction::AmoaddW {
        rd: 3,
        rs1: 1,
        rs2: 2,
    })
    .unwrap();
    let halt = encoder::encode(Instruction::Halt).unwrap();
    mem.store32(0, amo).unwrap();
    mem.store32(4, halt).unwrap();

    step(&mut cpu, &mut mem, &mut console).unwrap();
    assert_eq!(cpu.read(3), 10); // old value
    assert_eq!(mem.load32(0x10).unwrap(), 15); // new value
}

#[test]
fn halt_sets_local_exit() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4);
    let mut console = crate::ui::Console::default();
    let inst = encoder::encode(Instruction::Halt).unwrap();
    assert_eq!(inst, 0x0020_0073, "halt must encode to 0x0020_0073");
    mem.store32(0, inst).unwrap();
    assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    assert!(cpu.local_exit, "halt must set local_exit (permanent stop)");
    assert!(!cpu.ebreak_hit, "halt must not set ebreak_hit");
    assert_eq!(cpu.exit_code, None);
}

#[test]
fn ebreak_sets_ebreak_hit() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4);
    let mut console = crate::ui::Console::default();
    let inst = encoder::encode(Instruction::Ebreak).unwrap();
    assert_eq!(inst, 0x0010_0073, "ebreak must encode to 0x0010_0073");
    mem.store32(0, inst).unwrap();
    assert!(!step(&mut cpu, &mut mem, &mut console).unwrap());
    assert!(cpu.ebreak_hit, "ebreak must set ebreak_hit (resumable)");
    assert!(!cpu.local_exit, "ebreak must not set local_exit");
}

#[test]
fn halt_and_ebreak_have_distinct_encodings() {
    let halt = encoder::encode(Instruction::Halt).unwrap();
    let ebreak = encoder::encode(Instruction::Ebreak).unwrap();
    assert_eq!(halt, 0x0020_0073);
    assert_eq!(ebreak, 0x0010_0073);
    assert_ne!(halt, ebreak, "halt and ebreak must encode differently");
}

#[test]
fn decode_ebreak_is_canonical() {
    let inst = crate::falcon::decoder::decode(0x0010_0073).unwrap();
    assert!(matches!(inst, Instruction::Ebreak));
}

#[test]
fn decode_halt_is_canonical() {
    let inst = crate::falcon::decoder::decode(0x0020_0073).unwrap();
    assert!(matches!(inst, Instruction::Halt));
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
