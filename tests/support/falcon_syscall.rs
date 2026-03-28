
use super::{FALCON_HART_START, handle_syscall};
use crate::falcon::memory::Ram;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

#[test]
fn hart_start_syscall_emits_pending_request() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 0x100);
    cpu.write(11, 0x200);
    cpu.write(12, 0x300);

    let cont =
        handle_syscall(FALCON_HART_START, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    let req = cpu.pending_hart_start.expect("pending request");
    assert_eq!(req.entry_pc, 0x100);
    assert_eq!(req.stack_ptr, 0x200);
    assert_eq!(req.arg, 0x300);
}
