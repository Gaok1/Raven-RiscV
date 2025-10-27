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
    let expected_li = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 1 })
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
    assert!(
        err.msg.contains("printStr: expected 'label'")
            || err.msg.contains("printString: expected 'label'")
    );
}

#[test]
fn print_string_label_expands_correctly() {
    let asm = ".data\nmsg: .asciz \"hi\"\n.text\nprintString msg";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 4);
    let expected_li = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 2 })
        .expect("encode addi");
    let expected_lui = encode(Instruction::Lui { rd: 10, imm: 0x1000 })
        .expect("encode lui");
    let expected_addi = encode(Instruction::Addi { rd: 10, rs1: 10, imm: 0 })
        .expect("encode addi");
    let expected_ecall = encode(Instruction::Ecall).expect("encode ecall");
    assert_eq!(prog.text[0], expected_li);
    assert_eq!(prog.text[1], expected_lui);
    assert_eq!(prog.text[2], expected_addi);
    assert_eq!(prog.text[3], expected_ecall);
}

#[test]
fn read_label_expands_correctly() {
    let asm = ".data\nbuf: .space 4\n.text\nread buf";
    let prog = assemble(asm, 0).expect("assemble");
    assert_eq!(prog.text.len(), 4);
    let expected_li = encode(Instruction::Addi { rd: 17, rs1: 0, imm: 3 })
        .expect("encode addi");
    let expected_lui = encode(Instruction::Lui { rd: 10, imm: 0x1000 })
        .expect("encode lui");
    let expected_addi = encode(Instruction::Addi { rd: 10, rs1: 10, imm: 0 })
        .expect("encode addi");
    let expected_ecall = encode(Instruction::Ecall).expect("encode ecall");
    assert_eq!(prog.text[0], expected_li);
    assert_eq!(prog.text[1], expected_lui);
    assert_eq!(prog.text[2], expected_addi);
    assert_eq!(prog.text[3], expected_ecall);
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
