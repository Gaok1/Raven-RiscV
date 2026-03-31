
use crate::{eprintln, raven_api::ENABLED_DEBUG_MESSAGES};




#[repr(i32)]
pub enum RavenFD {
    STDIN = 0,
    STDOUT = 1,
    STDERR = 2,
}


/// Raw ecall wrappers for the Raven simulator (RISC-V 32IM no_std).

/// read(fd, buf, len) — syscall 63
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn read(fd: RavenFD, buf: *mut u8, len: usize) -> isize {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn write(fd: RavenFD, buf: *const u8, len: usize) -> isize {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub fn exit(code: i32) -> ! {
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
/// Identical to exit in Raven (single-threaded), but matches the Linux ABI.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn exit_group(code: i32) -> ! {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn brk(addr: usize) -> usize {
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

#[unsafe(no_mangle)]
pub fn pause_sim() {
    unsafe {
        core::arch::asm!("ebreak;");
    }
}

/// writev(fd, iov, iovcnt) — syscall 66
/// Each iovec entry is { u32 base, u32 len } (8 bytes).
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn writev(fd: RavenFD, iov: *const u32, iovcnt: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 66_u32,
            in("a0") fd as i32,
            in("a1") iov as usize,
            in("a2") iovcnt,
            lateout("a0") ret,
        );
    }
    ret
}

/// getpid() — syscall 172 (always returns 1)
#[unsafe(no_mangle)]
#[inline(always)]
pub fn getpid() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 172_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// getuid() — syscall 174 (always returns 0)
#[unsafe(no_mangle)]
#[inline(always)]
pub fn getuid() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 174_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// getgid() — syscall 176 (always returns 0)
#[unsafe(no_mangle)]
#[inline(always)]
pub fn getgid() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 176_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// munmap(addr, len) — syscall 215 (no-op; always returns 0)
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn munmap(addr: usize, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 215_u32,
            in("a0") addr,
            in("a1") len,
            lateout("a0") ret,
        );
    }
    ret
}

/// mmap(addr, len, prot, flags, fd, offset) — syscall 222
/// Only anonymous mappings (flags=0x22, fd=-1) are supported.
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn mmap(addr: usize, len: usize, prot: u32, flags: u32, fd: i32, offset: usize) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 222_u32,
            in("a0") addr,
            in("a1") len,
            in("a2") prot,
            in("a3") flags,
            in("a4") fd,
            in("a5") offset,
            lateout("a0") ret,
        );
    }
    ret
}

/// clock_gettime(clockid, tp) — syscall 403
/// Writes { tv_sec: u32, tv_nsec: u32 } at tp (instruction-based time).
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn clock_gettime(clockid: u32, tp: *mut u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 403_u32,
            in("a0") clockid,
            in("a1") tp as usize,
            lateout("a0") ret,
        );
    }
    ret
}

// ── Raven teaching extensions (syscalls 1000–1053) ───────────────────────────
// Raven-specific shortcuts — no strlen loop, no fd argument.

/// Print signed 32-bit integer to console (no newline). — syscall 1000
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_int(n: i32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1000_u32,
            in("a0") n,
        );
    }
}

/// Print NUL-terminated string (no newline). — syscall 1001
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_str(s: *const u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1001_u32,
            in("a0") s as usize,
        );
    }
}

/// Print NUL-terminated string followed by newline. — syscall 1002
#[unsafe(no_mangle)]
#[inline(always)]
pub fn println_str(s: *const u8) {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_line(buf: *mut u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1003_u32,
            in("a0") buf as usize,
        );
    }
}

/// Print unsigned 32-bit integer to console (no newline). — syscall 1004
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_uint(n: u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1004_u32,
            in("a0") n,
        );
    }
}

/// Print value as hex (e.g. `0xDEADBEEF`) to console (no newline). — syscall 1005
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_hex(n: u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1005_u32,
            in("a0") n,
        );
    }
}

/// Print a single ASCII character to console. — syscall 1006
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_char(c: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1006_u32,
            in("a0") c as u32,
        );
    }
}

/// Print a newline to console. — syscall 1008
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_newline() {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1008_u32,
        );
    }
}

/// Read one u8 from stdin and store at *dst (decimal or 0x hex). — syscall 1010
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_u8(dst: *mut u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1010_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one u16 from stdin and store at *dst (little-endian). — syscall 1011
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_u16(dst: *mut u16) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1011_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one u32 from stdin and store at *dst (little-endian). — syscall 1012
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_u32(dst: *mut u32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1012_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one i32 from stdin (accepts negatives) and store at *dst. — syscall 1013
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_int(dst: *mut i32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1013_u32,
            in("a0") dst as usize,
        );
    }
}

/// Read one f32 from stdin and store at *dst. — syscall 1014
#[unsafe(no_mangle)]
#[inline(always)]
pub fn read_float(dst: *mut f32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1014_u32,
            in("a0") dst as usize,
        );
    }
}

/// Print f32 to console (no newline). — syscall 1015
/// The bit pattern of `v` is passed in a0; Raven reinterprets it as f32.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn print_float(v: f32) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1015_u32,
            in("a0") v.to_bits(),
        );
    }
}

/// Return the number of instructions executed so far. — syscall 1030
/// Low 32 bits in a0, high 32 bits in a1.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn get_instr_count() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1030_u32,
            lateout("a0") lo,
            lateout("a1") hi,
        );
    }
    (hi as u64) << 32 | lo as u64
}

/// Return the simulated cycle count (icache + dcache + CPI cycles). — syscall 1031
/// Low 32 bits in a0, high 32 bits in a1.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn get_cycle_count() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1031_u32,
            lateout("a0") lo,
            lateout("a1") hi,
        );
    }
    (hi as u64) << 32 | lo as u64
}

// ── Raven memory utilities (syscalls 1050–1053) ──────────────────────────────

/// Fill `len` bytes at `dst` with `byte`. — syscall 1050
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn memset(dst: *mut u8, byte: u8, len: usize) {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn memcpy(dst: *mut u8, src: *const u8, len: usize) {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn strlen(s: *const u8) -> usize {
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
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
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

// ── Hart management (syscall 1100) ────────────────────────────────────────────

/// Spawn a new hart starting at `entry_pc` with stack pointer `stack_ptr`.
///
/// `arg` is placed in `a0` of the new hart so the entry point receives a
/// single `u32` argument. Returns 0 on success.
///
/// `stack_ptr` must point to the **top** (high address) of a valid stack region.
///
/// # Example
/// ```rust
/// static mut HART1_STACK: [u8; 4096] = [0; 4096];
///
/// extern "C" fn worker(id: u32) -> ! { /* ... */ exit(0) }
///
/// let sp = unsafe { HART1_STACK.as_ptr().add(4096) as u32 };
/// hart_start(worker as u32, sp, /*arg=*/1);
/// ```
#[unsafe(no_mangle)]
#[inline(always)]
pub unsafe fn hart_start(entry_pc: u32, stack_ptr: u32, arg: u32) -> i32 {
    let code: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1100_u32,
            in("a0") entry_pc,
            in("a1") stack_ptr,
            in("a2") arg,
            lateout("a0") code,
        );
    }
    code
}

/// Terminate **only this hart** without affecting any other running harts.
/// Equivalent to returning from the top-level hart function; use this instead
/// of `exit()` inside a spawned worker so the main hart keeps running.
#[unsafe(no_mangle)]
pub unsafe fn hart_exit() -> ! {
    if ENABLED_DEBUG_MESSAGES {
        eprintln!("Hart going OUT!");
    }
    unsafe {
        core::arch::asm!("ecall", in("a7") 1101_u32, options(noreturn));
    }
}

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("\nPanic!: {info}");
    exit(101)
}
