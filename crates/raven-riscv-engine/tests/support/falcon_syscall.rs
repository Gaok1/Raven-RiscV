use super::{
    FALCON_HART_START, FALCON_MAP_EXEC, GFX_FILL_RECT, GFX_POLL_KEY, GFX_PRESENT,
    GFX_SCREEN_INIT, GFX_SET_PIXEL, GFX_SLEEP_MS, handle_syscall,
};
use crate::falcon::memory::{Bus, Ram};
use crate::falcon::registers::Cpu;
use crate::falcon::syscall::handle_syscall_with_cycle_override;
use crate::ui::{Console, console::ConsoleColor};

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

#[test]
fn map_exec_syscall_emits_pending_region() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 0x100);
    cpu.write(11, 0x20);

    let cont = handle_syscall(FALCON_MAP_EXEC, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10), 0);
    let region = cpu.pending_exec_map.expect("pending exec map");
    assert_eq!(region.start, 0x100);
    assert_eq!(region.end, 0x120);
}

#[test]
fn map_exec_syscall_rejects_unaligned_region() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 0x101);
    cpu.write(11, 0x20);

    let cont = handle_syscall(FALCON_MAP_EXEC, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10) as i32, -22);
    assert!(cpu.pending_exec_map.is_none());
}

#[test]
fn get_cycle_count_uses_bus_total_by_default() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    let cont = handle_syscall(1031, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10), 0);
    assert_eq!(cpu.read(11), 0);
}

#[test]
fn get_cycle_count_uses_override_when_provided() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    let cont = handle_syscall_with_cycle_override(1031, &mut cpu, &mut mem, &mut console, Some(7))
        .expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10), 7);
    assert_eq!(cpu.read(11), 0);
}

#[test]
fn syscall_trace_logs_non_io_calls_in_warning_color() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.trace_syscalls = true;

    let cont = handle_syscall_with_cycle_override(1031, &mut cpu, &mut mem, &mut console, Some(7))
        .expect("syscall");

    assert!(cont);
    let line = console.lines.last().expect("trace line");
    assert_eq!(line.color, ConsoleColor::Warning);
    assert!(line.text.contains("syscall 1031 (get_cycle_count)"));
}

// ── Graphics syscalls (2000+) ───────────────────────────────────────────────

const NEG_EINVAL: u32 = (-22i32) as u32;

fn gfx(code: u32, cpu: &mut Cpu, mem: &mut Ram, console: &mut Console) {
    let cont = handle_syscall(code, cpu, mem, console).expect("syscall");
    assert!(cont);
}

#[test]
fn screen_init_creates_device_and_rejects_bad_size() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    // Too small
    cpu.write(10, 4);
    cpu.write(11, 32);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), NEG_EINVAL);
    assert!(console.screen.is_none());

    // Valid
    cpu.write(10, 32);
    cpu.write(11, 16);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), 0);
    let screen = console.screen.as_ref().expect("screen created");
    assert_eq!((screen.width, screen.height), (32, 16));
}

#[test]
fn draw_before_init_returns_einval_and_warns_once() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    gfx(GFX_SET_PIXEL, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), NEG_EINVAL);
    gfx(GFX_PRESENT, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), NEG_EINVAL);
    let warnings = console
        .lines
        .iter()
        .filter(|l| l.text.contains("before screen_init"))
        .count();
    assert_eq!(warnings, 1);
}

#[test]
fn set_pixel_present_publishes_front_buffer_and_rejects_oob() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 16);
    cpu.write(11, 8);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);

    // set_pixel(3, 2, 0xFF8000)
    cpu.write(10, 3);
    cpu.write(11, 2);
    cpu.write(12, 0x00FF_8000);
    gfx(GFX_SET_PIXEL, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), 0);

    // Not visible until present
    assert_eq!(console.screen.as_ref().unwrap().front[2 * 16 + 3], 0);
    gfx(GFX_PRESENT, &mut cpu, &mut mem, &mut console);
    assert_eq!(
        console.screen.as_ref().unwrap().front[2 * 16 + 3],
        0x00FF_8000
    );

    // Out of bounds: -EINVAL, no fault
    cpu.write(10, 16);
    cpu.write(11, 0);
    cpu.write(12, 0x00FF_FFFF);
    gfx(GFX_SET_PIXEL, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), NEG_EINVAL);
}

#[test]
fn fill_rect_clips_to_screen_bounds() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 8);
    cpu.write(11, 8);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);

    // fill_rect(6, 6, 10, 10, white) — clipped to the 2x2 bottom-right corner
    cpu.write(10, 6);
    cpu.write(11, 6);
    cpu.write(12, 10);
    cpu.write(13, 10);
    cpu.write(14, 0x00FF_FFFF);
    gfx(GFX_FILL_RECT, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), 0);
    gfx(GFX_PRESENT, &mut cpu, &mut mem, &mut console);

    let screen = console.screen.as_ref().unwrap();
    assert_eq!(screen.front[7 * 8 + 7], 0x00FF_FFFF);
    assert_eq!(screen.front[6 * 8 + 6], 0x00FF_FFFF);
    assert_eq!(screen.front[5 * 8 + 5], 0);
}

#[test]
fn poll_key_is_fifo_and_zero_when_empty() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 8);
    cpu.write(11, 8);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);

    let screen = console.screen.as_mut().unwrap();
    screen.push_key(b'w' as u32);
    screen.push_key(crate::ui::screen::KEY_LEFT);

    gfx(GFX_POLL_KEY, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), b'w' as u32);
    gfx(GFX_POLL_KEY, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), crate::ui::screen::KEY_LEFT);
    gfx(GFX_POLL_KEY, &mut cpu, &mut mem, &mut console);
    assert_eq!(cpu.read(10), 0);
}

#[test]
fn sleep_ms_parks_the_ecall_then_resumes() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 8);
    cpu.write(11, 8);
    gfx(GFX_SCREEN_INIT, &mut cpu, &mut mem, &mut console);

    // ecall at pc=0; a7=GFX_SLEEP_MS, a0=0ms (deadline satisfied immediately,
    // but the first execution still parks — one full re-execution cycle).
    mem.store32(0, 0x0000_0073).unwrap();
    cpu.pc = 0;
    cpu.write(17, GFX_SLEEP_MS);
    cpu.write(10, 0);

    let alive = crate::falcon::exec::step(&mut cpu, &mut mem, &mut console).expect("step");
    assert!(alive);
    assert_eq!(cpu.pc, 0, "parked ecall keeps the PC");
    assert!(cpu.sleep_until.is_some());

    let alive = crate::falcon::exec::step(&mut cpu, &mut mem, &mut console).expect("step");
    assert!(alive);
    assert_eq!(cpu.pc, 4, "expired deadline lets the ecall complete");
    assert!(cpu.sleep_until.is_none());
}

#[test]
fn syscall_trace_skips_io_calls() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.trace_syscalls = true;

    cpu.write(10, 1);
    cpu.write(11, 0);
    cpu.write(12, 0);

    let cont = handle_syscall(64, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert!(console.lines.is_empty());
}
