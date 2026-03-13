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

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("\nPanic!: {info}");
    sys_exit(101)
}
