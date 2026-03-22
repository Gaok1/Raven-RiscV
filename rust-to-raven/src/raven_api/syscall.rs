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

// ── Falcon teaching extensions (syscalls 1000–1053) ───────────────────────────
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

/// Print unsigned 32-bit integer to console (no newline). — syscall 1004
#[inline(always)]
pub fn falcon_print_uint(n: u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1004_u32,
            in("a0") n,
        );
    }
}

/// Print value as hex (e.g. `0xDEADBEEF`) to console (no newline). — syscall 1005
#[inline(always)]
pub fn falcon_print_hex(n: u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1005_u32,
            in("a0") n,
        );
    }
}

/// Print a single ASCII character to console. — syscall 1006
#[inline(always)]
pub fn falcon_print_char(c: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1006_u32,
            in("a0") c as u32,
        );
    }
}

/// Print a newline to console. — syscall 1008
#[inline(always)]
pub fn falcon_print_newline() {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1008_u32,
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

/// Read one i32 from stdin (accepts negatives) and store at *dst. — syscall 1013
#[inline(always)]
pub fn falcon_read_int(dst: *mut i32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1013_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one f32 from stdin and store at *dst. — syscall 1014
#[inline(always)]
pub fn falcon_read_float(dst: *mut f32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1014_u32,
            in("a0") dst as usize,
        );
    }
}

/// Print f32 in fa0 to console (no newline). — syscall 1015
#[inline(always)]
pub fn falcon_print_float(v: f32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1015_u32,
            in("fa0") v,
        );
    }
}

/// Return the number of instructions executed so far (low 32 bits). — syscall 1030
#[inline(always)]
pub fn falcon_get_instr_count() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1030_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// Return the simulated cycle count (low 32 bits, same as instr_count). — syscall 1031
#[inline(always)]
pub fn falcon_get_cycle_count() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1031_u32,
            lateout("a0") ret,
        );
    }
    ret
}

// ── Falcon memory utilities (syscalls 1050–1053) ──────────────────────────────

/// Fill `len` bytes at `dst` with `byte`. — syscall 1050
#[inline(always)]
pub unsafe fn falcon_memset(dst: *mut u8, byte: u8, len: usize) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1050_u32,
            in("a0") dst as usize,
            in("a1") byte as u32,
            in("a2") len,
        );
    }
}

/// Copy `len` bytes from `src` to `dst`. — syscall 1051
#[inline(always)]
pub unsafe fn falcon_memcpy(dst: *mut u8, src: *const u8, len: usize) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1051_u32,
            in("a0") dst as usize,
            in("a1") src as usize,
            in("a2") len,
        );
    }
}

/// Return the length of NUL-terminated string at `s`. — syscall 1052
#[inline(always)]
pub unsafe fn falcon_strlen(s: *const u8) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1052_u32,
            in("a0") s as usize,
            lateout("a0") ret,
        );
    }
    ret
}

/// Compare NUL-terminated strings `s1` and `s2`. Returns negative, 0, or positive. — syscall 1053
#[inline(always)]
pub unsafe fn falcon_strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1053_u32,
            in("a0") s1 as usize,
            in("a1") s2 as usize,
            lateout("a0") ret,
        );
    }
    ret
}

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("\nPanic!: {info}");
    sys_exit(101)
}
