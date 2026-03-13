/// Raw ecall wrappers for the Raven simulator (RISC-V 32IM no_std).

/// write(fd, buf, len) — syscall 64
#[inline(always)]
pub unsafe fn sys_write(fd: u32, buf: *const u8, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 64u32,
            in("a0") fd,
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
            in("a7") 93u32,
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
            in("a7") 278u32,
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
            in("a7") 214u32,
            in("a0") addr,
            lateout("a0") ret,
        );
    }
    ret
}

