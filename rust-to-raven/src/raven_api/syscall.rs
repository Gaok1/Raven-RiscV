use crate::eprintln;

#[repr(i32)]
pub enum RavenFD {
    STDIN = 0,
    STDOUT = 1,
    STDERR = 2,
}


/// Raw ecall wrappers for the Raven simulator (RISC-V 32IM no_std).

/// read(fd, buf, len) — syscall 63
#[inline(always)]
pub unsafe fn sys_read(fd: RavenFD, buf: *mut u8, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 63_u32,
            in("a0") fd as i32,
            in("a1") buf as usize,
            in("a2") len,
            lateout("a0") ret,
        );
    }
    ret
}

/// write(fd, buf, len) — syscall 64
#[inline(always)]
pub unsafe fn sys_write(fd: RavenFD, buf: *const u8, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 64_u32,
            in("a0") fd as i32, 
            in("a1") buf as usize,
            in("a2") len,
            lateout("a0") ret,
        );
    }
    ret
}

/// exit(code) — syscall 93
#[inline(always)]
pub fn sys_exit(code: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 93_u32,
            in("a0") code,
            options(noreturn),
        );
    }
}

/// exit_group(code) — syscall 94
/// Identical to sys_exit in Raven (single-threaded), but matches the Linux ABI.
#[inline(always)]
pub fn sys_exit_group(code: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 94_u32,
            in("a0") code,
            options(noreturn),
        );
    }
}

/// getrandom(buf, buflen, flags) — syscall 278
#[inline(always)]
pub unsafe fn sys_getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 278_u32,
            in("a0") buf as usize,
            in("a1") buflen,
            in("a2") flags,
            lateout("a0") ret,
        );
    }
    ret
}

/// brk(addr) — syscall 214
/// Pass 0 to query the current program break.
/// Returns the new (or current) break on success, or a value < addr on failure.
#[inline(always)]
pub unsafe fn sys_brk(addr: usize) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 214_u32,
            in("a0") addr,
            lateout("a0") ret,
        );
    }
    ret
}

pub fn sys_pause_sim() {
    unsafe {
        core::arch::asm!("ebreak;");
    }
}

// ── Falcon teaching extensions (syscalls 1000–1012) ───────────────────────────
// Raven-specific shortcuts — no strlen loop, no fd argument.

/// Print signed 32-bit integer to console (no newline). — syscall 1000
#[inline(always)]
pub fn falcon_print_int(n: i32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1000_u32,
            in("a0") n,
        );
    }
}

/// Print NUL-terminated string (no newline). — syscall 1001
#[inline(always)]
pub fn falcon_print_str(s: *const u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1001_u32,
            in("a0") s as usize,
        );
    }
}

/// Print NUL-terminated string followed by newline. — syscall 1002
#[inline(always)]
pub fn falcon_println_str(s: *const u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1002_u32,
            in("a0") s as usize,
        );
    }
}

/// Read one line from console into buf (NUL-terminated, newline excluded).
/// Caller must ensure the buffer is large enough. — syscall 1003
#[inline(always)]
pub fn falcon_read_line(buf: *mut u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1003_u32,
            in("a0") buf as usize,
        );
    }
}

/// Read one u8 from stdin and store at *dst (decimal or 0x hex). — syscall 1010
#[inline(always)]
pub fn falcon_read_u8(dst: *mut u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1010_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one u16 from stdin and store at *dst (little-endian). — syscall 1011
#[inline(always)]
pub fn falcon_read_u16(dst: *mut u16) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1011_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one u32 from stdin and store at *dst (little-endian). — syscall 1012
#[inline(always)]
pub fn falcon_read_u32(dst: *mut u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1012_u32,
            in("a0") dst as usize,
        );
    }
}

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("\nPanic!: {info}");
    sys_exit(101)
}
