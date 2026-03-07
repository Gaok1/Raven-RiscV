use super::*;
use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;

#[test]
fn la_generates_lui_addi_pair() {
    // Assemble a simple program using 'la' for a symbol in .data
    let asm = ".data\nvar: .word 0\n.text\nla t0, var";
    let prog = assemble(asm, 0).expect("assemble");

    // Two instructions should be emitted: LUI and ADDI
    assert_eq!(prog.text.len(), 2);

    let expected_lui = encode(Instruction::Lui { rd: 5, imm: 0x1000 }).expect("encode lui");
    let expected_addi = encode(Instruction::Addi { rd: 5, rs1: 5, imm: 0 })
        .expect("encode addi");

    assert_eq!(prog.text[0], expected_lui);
    assert_eq!(prog.text[1], expected_addi);
}

#[test]
fn la_handles_negative_low_part() {
    // Force a label address with low bits >= 0x800 so the ADDI low part becomes negative.
    // With a data base of 0x1000, adding a 2048-byte pad yields 0x1800.
    let asm = ".data\npad: .space 2048\ntarget: .word 0\n.text\nla t0, target";
    let prog = assemble(asm, 0).expect("assemble");

    assert_eq!(prog.text.len(), 2);

    let expected_lui = encode(Instruction::Lui { rd: 5, imm: 0x2000 }).expect("encode lui");
    let expected_addi =
        encode(Instruction::Addi { rd: 5, rs1: 5, imm: -2048 }).expect("encode addi");

    assert_eq!(prog.text[0], expected_lui);
    assert_eq!(prog.text[1], expected_addi);
}

#[test]
fn lui_imm20_is_unshifted() {
    // ISA semantics: rd = imm20 << 12
    let prog = assemble(".text\nlui t0, 0x1", 0).expect("assemble");
    assert_eq!(prog.text.len(), 1);

    let expected = encode(Instruction::Lui { rd: 5, imm: 0x1000 }).expect("encode lui");
    assert_eq!(prog.text[0], expected);
}

#[test]
fn auipc_imm20_is_unshifted() {
    // ISA semantics: rd = pc + (imm20 << 12)
    let prog = assemble(".text\nauipc t0, 0x1", 0).expect("assemble");
    assert_eq!(prog.text.len(), 1);

    let expected = encode(Instruction::Auipc { rd: 5, imm: 0x1000 }).expect("encode auipc");
    assert_eq!(prog.text[0], expected);
}

#[test]
fn lui_immediate_range_error() {
    let asm = ".text\nlui t0, 0x100000";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains("20-bit"));
}

#[test]
fn call_expands_to_jal_ra() {
    // Simple program with a call to a local label
    let asm = ".text\ncall func\nfunc: halt";
    let prog = assemble(asm, 0).expect("assemble");

    // Should emit: JAL ra, func; HALT
    assert_eq!(prog.text.len(), 2);

    let expected_jal = encode(Instruction::Jal { rd: 1, imm: 4 }).expect("encode jal");
    assert_eq!(prog.text[0], expected_jal);
}

#[test]
fn ebreak_is_alias_of_halt() {
    let prog_halt = assemble(".text\nhalt", 0).expect("assemble halt");
    let prog_ebreak = assemble(".text\nebreak", 0).expect("assemble ebreak");

    assert_eq!(prog_halt.text, prog_ebreak.text);
    assert_eq!(prog_ebreak.text.len(), 1);
    assert_eq!(
        prog_ebreak.text[0],
        encode(Instruction::Ebreak).expect("encode ebreak")
    );
    assert_eq!(prog_ebreak.text[0], 0x0010_0073);
}

#[test]
fn push_expands_correctly() {
    let asm = ".text\npush a0";
    let prog = assemble(asm, 0).expect("assemble");
    println!("Program text: {:?}", prog.text);
    assert_eq!(prog.text.len(), 2);
    let expected_addi = encode(Instruction::Addi { rd: 2, rs1: 2, imm: -4 })
        .expect("encode addi");
    let expected_sw = encode(Instruction::Sw { rs2: 10, rs1: 2, imm: 4 })
        .expect("encode sw");
    println!("Expected SW: {}, Expected ADDI: {}", expected_sw, expected_addi);
    assert_eq!(prog.text[0], expected_addi);
    assert_eq!(prog.text[1], expected_sw);
}

#[test]
fn pop_expands_correctly() {
    let asm = ".text\npop a0";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 2);
    let expected_lw = encode(Instruction::Lw { rd: 10, rs1: 2, imm: 4 })
        .expect("encode lw");
    let expected_addi = encode(Instruction::Addi { rd: 2, rs1: 2, imm: 4 })
        .expect("encode addi");
    assert_eq!(prog.text[0], expected_lw);
    assert_eq!(prog.text[1], expected_addi);
}

#[test]
fn print_expands_correctly() {
    let asm = ".text\nprint a1";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 3);
    let expected_li = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 1000 })
        .expect("encode addi");
    let expected_mv = encode(Instruction::Addi { rd: 10, rs1: 11, imm: 0 })
        .expect("encode addi");
    let expected_ecall = encode(Instruction::Ecall).expect("encode ecall");
    assert_eq!(prog.text[0], expected_li);
    assert_eq!(prog.text[1], expected_mv);
    assert_eq!(prog.text[2], expected_ecall);
}

#[test]
fn print_string_register_errors() {
    let asm = ".text\nprintString a1";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains("print_str: expected 'label'"));
}

#[test]
fn print_string_label_expands_correctly() {
    // print_str/printString expands to: la a1, label + strlen loop + write(1, buf, len)
    // = 11 instructions total; syscall 64 (Linux write)
    let asm = ".data\nmsg: .asciz \"hi\"\n.text\nprintString msg";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 11);
    // Last 3 instructions: addi a0,x0,1 | addi a7,x0,64 | ecall
    let expected_fd   = encode(Instruction::Addi { rd: 10, rs1: 0, imm: 1 }).unwrap();
    let expected_sys  = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 64 }).unwrap();
    let expected_call = encode(Instruction::Ecall).unwrap();
    assert_eq!(prog.text[8],  expected_fd);
    assert_eq!(prog.text[9],  expected_sys);
    assert_eq!(prog.text[10], expected_call);
}

#[test]
fn read_label_expands_correctly() {
    // read expands to: addi a0,x0,0 | la a1, buf | addi a2,x0,256 | addi a7,x0,63 | ecall
    // = 6 instructions total; syscall 63 (Linux read)
    let asm = ".data\nbuf: .space 4\n.text\nread buf";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 6);
    // First instruction: addi a0, x0, 0 (fd=stdin)
    let expected_fd  = encode(Instruction::Addi { rd: 10, rs1: 0, imm: 0 }).unwrap();
    // Last 3: addi a2,x0,256 | addi a7,x0,63 | ecall
    let expected_cnt = encode(Instruction::Addi { rd: 12, rs1: 0, imm: 256 }).unwrap();
    let expected_sys = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 63 }).unwrap();
    let expected_ec  = encode(Instruction::Ecall).unwrap();
    assert_eq!(prog.text[0], expected_fd);
    assert_eq!(prog.text[3], expected_cnt);
    assert_eq!(prog.text[4], expected_sys);
    assert_eq!(prog.text[5], expected_ec);
}

#[test]
fn addi_immediate_range_error() {
    let asm = ".text\naddi x1, x0, 4096";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains("12-bit"));
}

#[test]
fn beq_offset_range_error() {
    let asm = ".text\nbeq x0, x0, 8192";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains("13-bit"));
}

#[test]
fn section_directives_equivalent() {
    let asm_section = ".section .data\nval: .word 1\n.section .text\n la t0, val\n ecall";
    let asm_traditional = ".data\nval: .word 1\n.text\n la t0, val\n ecall";
    let prog_section = assemble(asm_section, 0).expect("assemble section");
    let prog_traditional = assemble(asm_traditional, 0).expect("assemble traditional");
    assert_eq!(prog_section.text, prog_traditional.text);
    assert_eq!(prog_section.data, prog_traditional.data);
}

#[test]
fn unknown_section_errors() {
    let asm = ".section .unknown";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains("unknown section"));
}

#[test]
fn bss_space_and_la() {
    let asm = ".data\nmsg: .asciz \"hi\"\n.section .bss\nbuffer: .space 256\n.section .text\nla t0, buffer";
    let prog = assemble(asm, 0).expect("assemble bss");
    assert_eq!(prog.data.len(), 3);
    assert_eq!(prog.bss_size, 256);
    assert_eq!(prog.text.len(), 2);
    let expected_lui = encode(Instruction::Lui { rd: 5, imm: 0x1000 }).expect("encode lui");
    let expected_addi = encode(Instruction::Addi { rd: 5, rs1: 5, imm: 3 }).expect("encode addi");
    assert_eq!(prog.text[0], expected_lui);
    assert_eq!(prog.text[1], expected_addi);
}

#[test]
fn bss_align_and_size() {
    let asm = ".section .bss\na: .space 1\n.align 4\nb: .space 1\n.section .text\nhalt";
    let prog = assemble(asm, 0).expect("assemble bss align");
    assert_eq!(prog.bss_size, 5);
}

#[test]
fn bss_rejects_explicit_data() {
    let asm = ".section .bss\nx: .word 1";
    let err = assemble(asm, 0).err().expect("expected error");
    assert!(err.msg.contains(".bss does not store explicit data"));
}

#[test]
fn globl_directive_is_accepted_as_noop() {
    let asm = ".text\n.globl _start\n_start: halt";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 1);
}

#[test]
fn equate_dot_expression_works_with_li() {
    // len = . - msg should yield 3 for an .asciz "hi"
    let asm = ".data\nmsg: .asciz \"hi\"\nlen = . - msg\n.text\nli a0, len\nhalt";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 2);
    let expected_li =
        encode(Instruction::Addi { rd: 10, rs1: 0, imm: 3 }).expect("encode addi");
    assert_eq!(prog.text[0], expected_li);
}

#[test]
fn char_literal_in_addi() {
    // addi a0, x0, '0'  →  addi a0, x0, 48
    let prog = assemble(".text\naddi a0, x0, '0'", 0).expect("assemble");
    assert_eq!(prog.text.len(), 1);
    let expected = encode(Instruction::Addi { rd: 10, rs1: 0, imm: 48 }).unwrap();
    assert_eq!(prog.text[0], expected);
}

#[test]
fn char_literal_li_newline_escape() {
    // li a0, '\n'  →  addi a0, x0, 10
    let prog = assemble(".text\nli a0, '\\n'", 0).expect("assemble");
    assert_eq!(prog.text.len(), 1);
    let expected = encode(Instruction::Addi { rd: 10, rs1: 0, imm: 10 }).unwrap();
    assert_eq!(prog.text[0], expected);
}

#[test]
fn char_literal_non_ascii_error() {
    let err = assemble(".text\nli a0, '\u{00e9}'", 0).err().expect("expected error");
    assert!(err.msg.contains("ASCII"), "error should mention ASCII: {}", err.msg);
}
