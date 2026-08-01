use super::{
    FALCON_HART_START, FALCON_MAP_EXEC, GFX_FILL_RECT, GFX_POLL_KEY, GFX_PRESENT, GFX_SCREEN_INIT,
    GFX_SET_PIXEL, GFX_SLEEP_MS, handle_syscall,
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

// ── Linux ABI coverage: unknown codes never halt, common syscalls behave ───

#[test]
fn unknown_but_plausible_syscall_never_halts() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    // 999 is not claimed by any ABI module: must report -ENOSYS and keep
    // the run alive instead of stopping it (the core contract of this
    // module — see crates/raven-riscv-engine/src/falcon/syscall.rs).
    let cont = handle_syscall(999, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10) as i32, -38); // -ENOSYS
}

#[test]
fn fstat_reports_stdio_as_char_device() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 1); // fd = stdout
    cpu.write(11, 0x100); // stat buf
    let cont = handle_syscall(80, &mut cpu, &mut mem, &mut console).expect("syscall");
    assert!(cont);
    assert_eq!(cpu.read(10), 0);
    let st_mode = mem.user_load32(0x100 + 16).unwrap();
    assert_eq!(st_mode & 0o170000, 0o020000); // S_IFCHR

    cpu.write(10, 5); // unknown fd
    let cont = handle_syscall(80, &mut cpu, &mut mem, &mut console).expect("syscall");
    assert!(cont);
    assert_eq!(cpu.read(10) as i32, -9); // -EBADF
}

#[test]
fn uname_fills_sysname() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 0x200);
    let cont = handle_syscall(160, &mut cpu, &mut mem, &mut console).expect("syscall");
    assert!(cont);
    assert_eq!(cpu.read(10), 0);
    let mut bytes = Vec::new();
    let mut addr = 0x200u32;
    loop {
        let b = mem.user_load8(addr).unwrap();
        if b == 0 {
            break;
        }
        bytes.push(b);
        addr += 1;
    }
    assert_eq!(String::from_utf8(bytes).unwrap(), "Linux");
}

#[test]
fn gettimeofday_succeeds() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 0x300);
    let cont = handle_syscall(169, &mut cpu, &mut mem, &mut console).expect("syscall");
    assert!(cont);
    assert_eq!(cpu.read(10), 0);
}

#[test]
fn futex_never_blocks() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    let cont = handle_syscall(98, &mut cpu, &mut mem, &mut console).expect("syscall");
    assert!(cont);
    assert_eq!(cpu.read(10), 0);
}

#[test]
fn tgkill_with_sigabrt_terminates_like_exit() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 1); // tgid
    cpu.write(11, 1); // tid
    cpu.write(12, 6); // SIGABRT
    let cont = handle_syscall(131, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(!cont);
    assert_eq!(cpu.exit_code, Some(128 + 6));
}

#[test]
fn tgkill_with_harmless_signal_is_a_noop() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();

    cpu.write(10, 1);
    cpu.write(11, 1);
    cpu.write(12, 28); // SIGWINCH: not in the terminating set
    let cont = handle_syscall(131, &mut cpu, &mut mem, &mut console).expect("syscall");

    assert!(cont);
    assert_eq!(cpu.read(10), 0);
    assert!(cpu.exit_code.is_none());
}

// ── File simulation (openat/read/write/close/lseek/fstat/unlinkat/mkdirat) ──

fn scratch_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "raven_fs_test_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_cstr(mem: &mut Ram, addr: u32, s: &str) {
    for (i, b) in s.bytes().enumerate() {
        mem.store8(addr + i as u32, b).unwrap();
    }
    mem.store8(addr + s.len() as u32, 0).unwrap();
}

const O_WRONLY: u32 = 0o1;
const O_RDWR: u32 = 0o2;
const O_CREAT: u32 = 0o100;
const O_TRUNC: u32 = 0o1000;

#[test]
fn openat_write_read_close_roundtrip() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.filesystem.set_root(scratch_root("roundtrip"));

    let path_addr = 0x100;
    write_cstr(&mut mem, path_addr, "hello.txt");

    // openat(AT_FDCWD, "hello.txt", O_CREAT|O_WRONLY|O_TRUNC, 0644)
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, O_CREAT | O_WRONLY | O_TRUNC);
    cpu.write(13, 0o644);
    assert!(handle_syscall(56, &mut cpu, &mut mem, &mut console).unwrap());
    let fd = cpu.read(10) as i32;
    assert!(fd >= 3, "expected a real fd, got {fd}");

    // write(fd, "hi", 2)
    let data_addr = 0x200;
    write_cstr(&mut mem, data_addr, "hi");
    cpu.write(10, fd as u32);
    cpu.write(11, data_addr);
    cpu.write(12, 2);
    assert!(handle_syscall(64, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 2);

    // close(fd)
    cpu.write(10, fd as u32);
    assert!(handle_syscall(57, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 0);

    // openat again, read-only this time
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, 0); // O_RDONLY
    cpu.write(13, 0);
    assert!(handle_syscall(56, &mut cpu, &mut mem, &mut console).unwrap());
    let fd2 = cpu.read(10) as i32;
    assert!(fd2 >= 3);

    let read_addr = 0x300;
    cpu.write(10, fd2 as u32);
    cpu.write(11, read_addr);
    cpu.write(12, 16);
    assert!(handle_syscall(63, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 2);
    assert_eq!(mem.load8(read_addr).unwrap(), b'h');
    assert_eq!(mem.load8(read_addr + 1).unwrap(), b'i');
}

#[test]
fn openat_rejects_path_traversal_above_root() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.filesystem.set_root(scratch_root("traversal"));

    let path_addr = 0x100;
    write_cstr(&mut mem, path_addr, "../../../etc/passwd");

    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, O_RDWR);
    cpu.write(13, 0);
    assert!(handle_syscall(56, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10) as i32, -13); // -EACCES: blocked at the root
}

#[test]
fn fstat_reports_real_file_size() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.filesystem.set_root(scratch_root("fstat"));

    let path_addr = 0x100;
    write_cstr(&mut mem, path_addr, "sized.txt");
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, O_CREAT | O_WRONLY);
    cpu.write(13, 0o644);
    assert!(handle_syscall(56, &mut cpu, &mut mem, &mut console).unwrap());
    let fd = cpu.read(10) as i32;

    let data_addr = 0x200;
    write_cstr(&mut mem, data_addr, "abcd");
    cpu.write(10, fd as u32);
    cpu.write(11, data_addr);
    cpu.write(12, 4);
    assert!(handle_syscall(64, &mut cpu, &mut mem, &mut console).unwrap());

    let stat_addr = 0x300;
    cpu.write(10, fd as u32);
    cpu.write(11, stat_addr);
    assert!(handle_syscall(80, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 0);
    let size = mem.user_load32(stat_addr + 48).unwrap();
    assert_eq!(size, 4);
    let mode = mem.user_load32(stat_addr + 16).unwrap();
    assert_eq!(mode & 0o170000, 0o100000); // S_IFREG
}

#[test]
fn mkdirat_then_unlinkat_roundtrip() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    let root = scratch_root("mkdir_unlink");
    console.filesystem.set_root(root.clone());

    // mkdirat(AT_FDCWD, "sub", 0755)
    let path_addr = 0x100;
    write_cstr(&mut mem, path_addr, "sub");
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, 0o755);
    assert!(handle_syscall(34, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 0);
    assert!(root.join("sub").is_dir());

    // faccessat(AT_FDCWD, "sub", F_OK, 0) -> exists
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, path_addr);
    cpu.write(12, 0);
    cpu.write(13, 0);
    assert!(handle_syscall(48, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 0);

    // a plain file to unlink
    let file_addr = 0x200;
    write_cstr(&mut mem, file_addr, "todelete.txt");
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, file_addr);
    cpu.write(12, O_CREAT | O_WRONLY);
    cpu.write(13, 0o644);
    assert!(handle_syscall(56, &mut cpu, &mut mem, &mut console).unwrap());
    let fd = cpu.read(10) as i32;
    cpu.write(10, fd as u32);
    assert!(handle_syscall(57, &mut cpu, &mut mem, &mut console).unwrap()); // close
    assert!(root.join("todelete.txt").is_file());

    // unlinkat(AT_FDCWD, "todelete.txt", 0)
    cpu.write(10, (-100i32) as u32);
    cpu.write(11, file_addr);
    cpu.write(12, 0);
    assert!(handle_syscall(35, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 0);
    assert!(!root.join("todelete.txt").exists());
}

#[test]
fn getcwd_reports_sandbox_root_as_slash() {
    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut console = Console::default();
    console.filesystem.set_root(scratch_root("getcwd"));

    let buf_addr = 0x100;
    cpu.write(10, buf_addr);
    cpu.write(11, 64);
    assert!(handle_syscall(17, &mut cpu, &mut mem, &mut console).unwrap());
    assert_eq!(cpu.read(10), 2); // "/" + NUL
    assert_eq!(mem.load8(buf_addr).unwrap(), b'/');
    assert_eq!(mem.load8(buf_addr + 1).unwrap(), 0);
}
