mod falcon;

use falcon::asm::assemble;
use falcon::program::load_words;

fn main() {
    let asm = r#"
        addi x1, x0, 5
        addi x2, x0, 7
    loop:
        add  x3, x1, x2
        beq  x3, x0, loop   # só pra testar label (não vai tomar)
        halt
    "#;

    let mut mem = falcon::Ram::new(64*1024);
    let mut cpu = falcon::Cpu::default();
    cpu.pc = 0;

    let words = assemble(asm, cpu.pc).expect("assemble");
    load_words(&mut mem, cpu.pc, &words);

    while falcon::exec::step(&mut cpu, &mut mem) {}
    println!("x3 = {}", cpu.x[3]); // 12
}
