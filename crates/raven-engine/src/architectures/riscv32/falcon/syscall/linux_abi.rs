//! Linux (RV32/RV64 generic) syscall ABI subset.
//!
//! `a7` = syscall number, `a0..a5` = args, `a0` = return value (negative
//! values mean `-errno`, represented as `u32`).
//!
//! Raven has no real process tree, signal delivery, or thread scheduler, so
//! most non-file syscalls here are deliberate no-ops that return a plausible
//! success/errno so well-behaved libc startup code (musl/glibc) doesn't trip
//! over a missing feature. Filesystem syscalls (openat/read/write/close/
//! lseek/fstat/unlinkat/mkdirat/faccessat/getcwd) are real: they operate on
//! actual host files through [`crate::host::fs_sim::FileSim`], sandboxed to
//! a root directory (like a chroot) so a guest program can only touch files
//! inside it. Anything not listed in this module falls through to the
//! dispatcher's ENOSYS default in `syscall.rs` — it never aborts the run.

use super::abi::{SyscallAbi, SyscallCtx};
use super::errno::*;
use super::helpers::{console_write_bytes, console_write_bytes_colored, read_zstr};
use crate::falcon::{errors::FalconError, memory::Bus, registers::Cpu};
use crate::host::fs_sim::FsError;
use crate::ui::Console;

fn fs_errno(e: FsError) -> u32 {
    match e {
        FsError::NotFound => LINUX_ENOENT,
        FsError::PermissionDenied => LINUX_EACCES,
        FsError::IsADirectory => LINUX_EISDIR,
        FsError::AlreadyExists => LINUX_EEXIST,
        FsError::InvalidFd => LINUX_EBADF,
        FsError::Io => LINUX_EIO,
    }
}

const SYS_GETCWD: u32 = 17;
const SYS_DUP: u32 = 23;
const SYS_DUP3: u32 = 24;
const SYS_FCNTL: u32 = 25;
const SYS_IOCTL: u32 = 29;
const SYS_MKDIRAT: u32 = 34;
const SYS_UNLINKAT: u32 = 35;
const SYS_FACCESSAT: u32 = 48;
const SYS_OPENAT: u32 = 56;
const SYS_CLOSE: u32 = 57;
const SYS_LSEEK: u32 = 62;
const SYS_READ: u32 = 63;
const SYS_WRITE: u32 = 64;
const SYS_READV: u32 = 65;
const SYS_WRITEV: u32 = 66;
const SYS_READLINKAT: u32 = 78;
const SYS_FSTAT: u32 = 80;
const SYS_EXIT: u32 = 93;
const SYS_EXIT_GROUP: u32 = 94;
const SYS_SET_TID_ADDRESS: u32 = 96;
const SYS_FUTEX: u32 = 98;
const SYS_SET_ROBUST_LIST: u32 = 99;
const SYS_NANOSLEEP: u32 = 101;
const SYS_SCHED_YIELD: u32 = 124;
const SYS_KILL: u32 = 129;
const SYS_TKILL: u32 = 130;
const SYS_TGKILL: u32 = 131;
const SYS_SIGALTSTACK: u32 = 132;
const SYS_RT_SIGACTION: u32 = 134;
const SYS_RT_SIGPROCMASK: u32 = 135;
const SYS_UNAME: u32 = 160;
const SYS_GETRLIMIT: u32 = 163;
const SYS_SETRLIMIT: u32 = 164;
const SYS_GETPID: u32 = 172;
const SYS_GETPPID: u32 = 173;
const SYS_GETUID: u32 = 174;
const SYS_GETEUID: u32 = 175;
const SYS_GETGID: u32 = 176;
const SYS_GETEGID: u32 = 177;
const SYS_GETTID: u32 = 178;
const SYS_GETTIMEOFDAY: u32 = 169;
const SYS_MPROTECT: u32 = 226;
const SYS_MADVISE: u32 = 233;
const SYS_PRLIMIT64: u32 = 261;
const SYS_GETRANDOM: u32 = 278;
const SYS_BRK: u32 = 214;
const SYS_MUNMAP: u32 = 215;
const SYS_MMAP: u32 = 222;
const SYS_CLOCK_GETTIME: u32 = 403; // clock_gettime64 (rv32 time64 ABI)

// Signals that plausibly mean "terminate the program" when raised against
// ourselves via kill/tkill/tgkill (e.g. libc's abort() -> raise(SIGABRT)).
// Anything else (SIGWINCH, real-time signals, ...) is a harmless no-op since
// Raven never delivers signals to a handler anyway.
fn terminating_signal(sig: u32) -> bool {
    matches!(sig, 2 | 3 | 4 | 6 | 7 | 8 | 9 | 11 | 15) // INT,QUIT,ILL,ABRT,BUS,FPE,KILL,SEGV,TERM
}

pub struct LinuxAbi;

impl<B: Bus> SyscallAbi<B> for LinuxAbi {
    fn name(&self, code: u32) -> Option<&'static str> {
        Some(match code {
            SYS_GETCWD => "getcwd",
            SYS_DUP => "dup",
            SYS_DUP3 => "dup3",
            SYS_FCNTL => "fcntl",
            SYS_IOCTL => "ioctl",
            SYS_MKDIRAT => "mkdirat",
            SYS_UNLINKAT => "unlinkat",
            SYS_FACCESSAT => "faccessat",
            SYS_OPENAT => "openat",
            SYS_CLOSE => "close",
            SYS_LSEEK => "lseek",
            SYS_READ => "read",
            SYS_WRITE => "write",
            SYS_READV => "readv",
            SYS_WRITEV => "writev",
            SYS_READLINKAT => "readlinkat",
            SYS_FSTAT => "fstat",
            SYS_EXIT => "exit",
            SYS_EXIT_GROUP => "exit_group",
            SYS_SET_TID_ADDRESS => "set_tid_address",
            SYS_FUTEX => "futex",
            SYS_SET_ROBUST_LIST => "set_robust_list",
            SYS_NANOSLEEP => "nanosleep",
            SYS_SCHED_YIELD => "sched_yield",
            SYS_KILL => "kill",
            SYS_TKILL => "tkill",
            SYS_TGKILL => "tgkill",
            SYS_SIGALTSTACK => "sigaltstack",
            SYS_RT_SIGACTION => "rt_sigaction",
            SYS_RT_SIGPROCMASK => "rt_sigprocmask",
            SYS_UNAME => "uname",
            SYS_GETRLIMIT => "getrlimit",
            SYS_SETRLIMIT => "setrlimit",
            SYS_GETPID => "getpid",
            SYS_GETPPID => "getppid",
            SYS_GETUID => "getuid",
            SYS_GETEUID => "geteuid",
            SYS_GETGID => "getgid",
            SYS_GETEGID => "getegid",
            SYS_GETTID => "gettid",
            SYS_GETTIMEOFDAY => "gettimeofday",
            SYS_MPROTECT => "mprotect",
            SYS_MADVISE => "madvise",
            SYS_PRLIMIT64 => "prlimit64",
            SYS_GETRANDOM => "getrandom",
            SYS_BRK => "brk",
            SYS_MUNMAP => "munmap",
            SYS_MMAP => "mmap",
            SYS_CLOCK_GETTIME => "clock_gettime",
            _ => return None,
        })
    }

    fn handle(&self, code: u32, ctx: &mut SyscallCtx<B>) -> Option<Result<bool, FalconError>> {
        <Self as SyscallAbi<B>>::name(self, code)?;
        let cpu = &mut *ctx.cpu;
        let mem = &mut *ctx.mem;
        let console = &mut *ctx.console;
        Some(match code {
            SYS_READ => linux_read(cpu, mem, console),
            SYS_WRITE => linux_write(cpu, mem, console),
            SYS_READV => linux_readv(cpu, mem, console),
            SYS_WRITEV => linux_writev(cpu, mem, console),
            SYS_BRK => {
                // brk(0) -> query current break; brk(addr) -> extend break to addr.
                // The break only grows: a request at or below the current break
                // (or past the end of memory) returns the break unchanged.
                let requested = cpu.read(10);
                if requested > cpu.heap_break && requested <= mem.mem_len() {
                    cpu.heap_break = requested;
                }
                cpu.write(10, cpu.heap_break);
                Ok(true)
            }
            SYS_MMAP => linux_mmap(cpu, mem, console),
            SYS_MUNMAP | SYS_MPROTECT | SYS_MADVISE => {
                // No real virtual memory management: accept and do nothing.
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_GETRANDOM => linux_getrandom(cpu, mem, console),
            SYS_EXIT | SYS_EXIT_GROUP => {
                let code = cpu.read(10);
                cpu.exit_code = Some(code);
                console.push_error(format!("Exit {}", code as i32));
                Ok(false)
            }
            SYS_KILL | SYS_TKILL | SYS_TGKILL => {
                // kill(pid,sig) / tkill(tid,sig) / tgkill(tgid,tid,sig): the
                // signal number is always the last of the (2 or 3) args.
                let sig = if code == SYS_TGKILL {
                    cpu.read(12)
                } else {
                    cpu.read(11)
                };
                if terminating_signal(sig) {
                    cpu.exit_code = Some(128u32.wrapping_add(sig));
                    console.push_error(format!("Terminated by signal {sig}"));
                    Ok(false)
                } else {
                    cpu.write(10, 0);
                    Ok(true)
                }
            }
            SYS_GETPID => {
                cpu.write(10, 1);
                Ok(true)
            }
            SYS_GETPPID => {
                cpu.write(10, 1);
                Ok(true)
            }
            SYS_GETUID | SYS_GETEUID | SYS_GETGID | SYS_GETEGID => {
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_GETTID => {
                cpu.write(10, cpu.hart_id.wrapping_add(1));
                Ok(true)
            }
            SYS_SET_TID_ADDRESS => {
                // Real Linux returns the caller's tid; no clear_child_tid support.
                cpu.write(10, cpu.hart_id.wrapping_add(1));
                Ok(true)
            }
            SYS_CLOCK_GETTIME => linux_clock_gettime(cpu, mem),
            SYS_GETTIMEOFDAY => linux_gettimeofday(cpu, mem),
            SYS_UNAME => linux_uname(cpu, mem),

            // --- File descriptor / filesystem subset ---
            // Real host files, sandboxed to a root directory (see
            // crate::host::fs_sim::FileSim) — fd 0/1/2 stay the console.
            SYS_OPENAT => linux_openat(cpu, mem, console),
            SYS_CLOSE => {
                let fd = cpu.read(10) as i32;
                if (0..=2).contains(&fd) || console.filesystem.close(fd) {
                    cpu.write(10, 0);
                } else {
                    cpu.write(10, LINUX_EBADF);
                }
                Ok(true)
            }
            SYS_DUP | SYS_DUP3 => {
                // No fd table beyond FileSim's own: echo the requested fd
                // back as "the same fd" (real files keep working through
                // their original fd; this only breaks a program that
                // expects dup'd fds to have independent seek positions).
                cpu.write(10, cpu.read(10));
                Ok(true)
            }
            SYS_LSEEK => {
                let fd = cpu.read(10) as i32;
                let offset = cpu.read(11) as i32 as i64;
                let whence = cpu.read(12);
                if (0..=2).contains(&fd) {
                    cpu.write(10, LINUX_ESPIPE);
                } else {
                    match console.filesystem.seek(fd, offset, whence) {
                        Ok(pos) => cpu.write(10, pos as u32),
                        Err(e) => cpu.write(10, fs_errno(e)),
                    }
                }
                Ok(true)
            }
            SYS_FSTAT => linux_fstat(cpu, mem, console),
            SYS_IOCTL => {
                let fd = cpu.read(10);
                cpu.write(
                    10,
                    if (0..=2).contains(&fd) {
                        0
                    } else {
                        LINUX_ENOTTY
                    },
                );
                Ok(true)
            }
            SYS_FCNTL => {
                // F_GETFL/F_SETFL/... : pretend success, no real fd flags to track.
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_FACCESSAT => linux_faccessat(cpu, mem, console),
            SYS_UNLINKAT => linux_unlinkat(cpu, mem, console),
            SYS_MKDIRAT => linux_mkdirat(cpu, mem, console),
            SYS_GETCWD => {
                // The guest's cwd is always the fs-sim root, presented as "/".
                let buf = cpu.read(10);
                let size = cpu.read(11) as usize;
                if size < 2 {
                    cpu.write(10, LINUX_EINVAL);
                } else if mem.store8(buf, b'/').is_err() || mem.store8(buf + 1, 0).is_err() {
                    cpu.write(10, LINUX_EFAULT);
                } else {
                    cpu.write(10, 2); // bytes written, including NUL (raw syscall convention)
                }
                Ok(true)
            }
            SYS_READLINKAT => {
                // No symlinks modeled: EINVAL is the correct errno for
                // "path exists but is not a symbolic link".
                cpu.write(10, LINUX_EINVAL);
                Ok(true)
            }

            // --- Threading/signals: no real scheduler or signal delivery ---
            SYS_SET_ROBUST_LIST | SYS_RT_SIGACTION | SYS_RT_SIGPROCMASK | SYS_SIGALTSTACK => {
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_FUTEX => {
                // Single-hart-per-address-space model: treat every futex op
                // as already satisfied (WAIT never actually blocks, WAKE
                // wakes nobody). Good enough since Raven has no real
                // concurrent waiters to synchronize.
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_SCHED_YIELD => {
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_NANOSLEEP => {
                // Best-effort: report success immediately (no host sleep),
                // and clear the remainder output pointer if given.
                let rem = cpu.read(11);
                if rem != 0 {
                    let _ = mem.store32(rem, 0);
                    let _ = mem.store32(rem.wrapping_add(4), 0);
                }
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_GETRLIMIT | SYS_SETRLIMIT => {
                cpu.write(10, 0);
                Ok(true)
            }
            SYS_PRLIMIT64 => linux_prlimit64(cpu, mem),

            _ => unreachable!("LinuxAbi::handle called with unclaimed code"),
        })
    }
}

fn linux_read<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let fd = cpu.read(10) as i32;
    let buf = cpu.read(11);
    let count = cpu.read(12) as usize;

    if fd != 0 {
        return linux_read_file(cpu, mem, console, fd, buf, count);
    }
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }

    if cpu.stdin.is_empty() {
        if let Some(line) = console.read_line() {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            cpu.stdin.extend_from_slice(&bytes);
            console.reading = false;
        } else {
            console.reading = true;
            return Ok(false);
        }
    }

    let n = count.min(cpu.stdin.len());
    for i in 0..n {
        let addr = buf.wrapping_add(i as u32);
        if let Err(e) = mem.store8(addr, cpu.stdin[i]) {
            cpu.write(10, LINUX_EFAULT);
            console.push_error(format!("read: {e}"));
            return Ok(true);
        }
    }

    cpu.stdin.drain(0..n);
    cpu.write(10, n as u32);
    Ok(true)
}

/// `read()` against a real fs-sim file (any fd other than stdin).
fn linux_read_file<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
    fd: i32,
    buf: u32,
    count: usize,
) -> Result<bool, FalconError> {
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }
    let mut tmp = vec![0u8; count];
    match console.filesystem.read(fd, &mut tmp) {
        Ok(n) => {
            for (i, &b) in tmp[..n].iter().enumerate() {
                if let Err(e) = mem.store8(buf.wrapping_add(i as u32), b) {
                    cpu.write(10, LINUX_EFAULT);
                    console.push_error(format!("read: {e}"));
                    return Ok(true);
                }
            }
            cpu.write(10, n as u32);
        }
        Err(e) => cpu.write(10, fs_errno(e)),
    }
    Ok(true)
}

fn linux_write<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let fd = cpu.read(10) as i32;
    let buf = cpu.read(11);
    let count = cpu.read(12) as usize;

    if fd != 1 && fd != 2 {
        return linux_write_file(cpu, mem, console, fd, buf, count);
    }
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }

    let mut bytes = Vec::with_capacity(count);
    for i in 0..count {
        let addr = buf.wrapping_add(i as u32);
        match mem.user_load8(addr) {
            Ok(b) => bytes.push(b),
            Err(e) => {
                cpu.write(10, LINUX_EFAULT);
                console.push_error(format!("write: {e}"));
                return Ok(true);
            }
        }
    }

    cpu.stdout.extend_from_slice(&bytes);
    if fd == 2 {
        console_write_bytes_colored(console, &bytes, crate::ui::console::ConsoleColor::Error);
    } else {
        console_write_bytes(console, &bytes);
    }
    cpu.write(10, count as u32);
    Ok(true)
}

/// `write()` against a real fs-sim file (any fd other than stdout/stderr).
fn linux_write_file<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
    fd: i32,
    buf: u32,
    count: usize,
) -> Result<bool, FalconError> {
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }
    let mut bytes = Vec::with_capacity(count);
    for i in 0..count {
        match mem.user_load8(buf.wrapping_add(i as u32)) {
            Ok(b) => bytes.push(b),
            Err(e) => {
                cpu.write(10, LINUX_EFAULT);
                console.push_error(format!("write: {e}"));
                return Ok(true);
            }
        }
    }
    match console.filesystem.write(fd, &bytes) {
        Ok(n) => cpu.write(10, n as u32),
        Err(e) => cpu.write(10, fs_errno(e)),
    }
    Ok(true)
}

fn linux_readv<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    // readv(fd=a0, iov=a1, iovcnt=a2): only fd=0, and only ever consumes one
    // buffered line per call (matches linux_read's line-based stdin model).
    let fd = cpu.read(10);
    let iov_ptr = cpu.read(11);
    let iovcnt = cpu.read(12) as usize;

    if fd != 0 {
        cpu.write(10, LINUX_EBADF);
        return Ok(true);
    }

    if cpu.stdin.is_empty() {
        if let Some(line) = console.read_line() {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            cpu.stdin.extend_from_slice(&bytes);
            console.reading = false;
        } else {
            console.reading = true;
            return Ok(false);
        }
    }

    let mut total: u32 = 0;
    for i in 0..iovcnt {
        if cpu.stdin.is_empty() {
            break;
        }
        let entry = iov_ptr.wrapping_add((i * 8) as u32);
        let base = match mem.user_load32(entry) {
            Ok(v) => v,
            Err(_) => {
                cpu.write(10, LINUX_EFAULT);
                return Ok(true);
            }
        };
        let len = match mem.user_load32(entry.wrapping_add(4)) {
            Ok(v) => v as usize,
            Err(_) => {
                cpu.write(10, LINUX_EFAULT);
                return Ok(true);
            }
        };
        let n = len.min(cpu.stdin.len());
        for j in 0..n {
            if let Err(e) = mem.store8(base.wrapping_add(j as u32), cpu.stdin[j]) {
                cpu.write(10, LINUX_EFAULT);
                console.push_error(format!("readv: {e}"));
                return Ok(true);
            }
        }
        cpu.stdin.drain(0..n);
        total += n as u32;
    }

    cpu.write(10, total);
    Ok(true)
}

fn linux_writev<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    // writev(fd=a0, iov=a1, iovcnt=a2) -> bytes written or -errno
    // struct iovec { void *base; size_t len; } - both u32 on RV32
    let fd = cpu.read(10);
    let iov_ptr = cpu.read(11);
    let iovcnt = cpu.read(12) as usize;

    if fd != 1 && fd != 2 {
        cpu.write(10, LINUX_EBADF);
        return Ok(true);
    }

    let mut total: u32 = 0;
    for i in 0..iovcnt {
        let entry = iov_ptr.wrapping_add((i * 8) as u32);
        let base = match mem.user_load32(entry) {
            Ok(v) => v,
            Err(_) => {
                cpu.write(10, LINUX_EFAULT);
                return Ok(true);
            }
        };
        let len = match mem.user_load32(entry.wrapping_add(4)) {
            Ok(v) => v as usize,
            Err(_) => {
                cpu.write(10, LINUX_EFAULT);
                return Ok(true);
            }
        };
        if len == 0 {
            continue;
        }

        let mut bytes = Vec::with_capacity(len);
        for j in 0..len {
            match mem.user_load8(base.wrapping_add(j as u32)) {
                Ok(b) => bytes.push(b),
                Err(_) => {
                    cpu.write(10, LINUX_EFAULT);
                    return Ok(true);
                }
            }
        }
        cpu.stdout.extend_from_slice(&bytes);
        if fd == 2 {
            console_write_bytes_colored(console, &bytes, crate::ui::console::ConsoleColor::Error);
        } else {
            console_write_bytes(console, &bytes);
        }
        total += len as u32;
    }

    cpu.write(10, total);
    Ok(true)
}

fn linux_getrandom<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let buf = cpu.read(10);
    let buflen = cpu.read(11) as usize;
    let flags = cpu.read(12);

    const GRND_NONBLOCK: u32 = 0x0001;
    const GRND_RANDOM: u32 = 0x0002;
    const SUPPORTED_FLAGS: u32 = GRND_NONBLOCK | GRND_RANDOM;

    if flags & !SUPPORTED_FLAGS != 0 {
        cpu.write(10, LINUX_EINVAL);
        console.push_error(format!("getrandom: unsupported flags 0x{flags:X}"));
        return Ok(true);
    }

    if buflen == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }

    let mut written: usize = 0;
    let mut tmp = [0u8; 256];
    while written < buflen {
        let chunk = (buflen - written).min(tmp.len());
        if let Err(e) = getrandom::fill(&mut tmp[..chunk]) {
            cpu.write(10, LINUX_EIO);
            console.push_error(format!("getrandom: {e}"));
            return Ok(true);
        }
        for (i, &b) in tmp[..chunk].iter().enumerate() {
            let addr = buf.wrapping_add((written + i) as u32);
            if let Err(e) = mem.store8(addr, b) {
                cpu.write(10, LINUX_EFAULT);
                console.push_error(format!("getrandom: {e}"));
                return Ok(true);
            }
        }
        written += chunk;
    }

    cpu.write(10, buflen as u32);
    Ok(true)
}

fn linux_mmap<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    // mmap(addr=a0, len=a1, prot=a2, flags=a3, fd=a4, offset=a5) -> ptr or -errno
    // Only anonymous mappings (MAP_ANONYMOUS=0x20) are supported.
    let len = cpu.read(11);
    let flags = cpu.read(13);
    let fd = cpu.read(14) as i32;

    const MAP_ANONYMOUS: u32 = 0x20;

    if flags & MAP_ANONYMOUS == 0 || fd != -1 {
        cpu.write(10, LINUX_EINVAL);
        console.push_error("mmap: only anonymous mappings supported (MAP_ANONYMOUS, fd=-1)");
        return Ok(true);
    }

    if len == 0 {
        cpu.write(10, LINUX_EINVAL);
        return Ok(true);
    }

    let aligned_len = (len.wrapping_add(3)) & !3;
    let ptr = cpu.heap_break;
    let new_break = ptr.wrapping_add(aligned_len);

    if new_break > mem.mem_len() || new_break < ptr {
        cpu.write(10, LINUX_ENOMEM);
        console.push_error("mmap: out of memory");
        return Ok(true);
    }

    cpu.heap_break = new_break;
    cpu.write(10, ptr);
    Ok(true)
}

fn linux_clock_gettime<B: Bus>(cpu: &mut Cpu, mem: &mut B) -> Result<bool, FalconError> {
    // clock_gettime(clockid=a0, timespec_ptr=a1) -> 0 or -errno
    // timespec: { tv_sec: u32, tv_nsec: u32 }
    // We report time as a fixed-frequency counter based on instr_count,
    // regardless of clockid (Raven has one clock).
    let tp = cpu.read(11);
    let ns_total = cpu.instr_count.wrapping_mul(10); // ~100 MHz equivalent
    let tv_sec = (ns_total / 1_000_000_000) as u32;
    let tv_nsec = (ns_total % 1_000_000_000) as u32;

    if mem.store32(tp, tv_sec).is_err() || mem.store32(tp.wrapping_add(4), tv_nsec).is_err() {
        cpu.write(10, LINUX_EFAULT);
        return Ok(true);
    }
    cpu.write(10, 0);
    Ok(true)
}

fn linux_gettimeofday<B: Bus>(cpu: &mut Cpu, mem: &mut B) -> Result<bool, FalconError> {
    // gettimeofday(timeval_ptr=a0, tz=a1 [ignored]) -> 0 or -errno
    // timeval: { tv_sec: u32, tv_usec: u32 }
    let tp = cpu.read(10);
    if tp == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }
    let ns_total = cpu.instr_count.wrapping_mul(10);
    let tv_sec = (ns_total / 1_000_000_000) as u32;
    let tv_usec = ((ns_total % 1_000_000_000) / 1_000) as u32;

    if mem.store32(tp, tv_sec).is_err() || mem.store32(tp.wrapping_add(4), tv_usec).is_err() {
        cpu.write(10, LINUX_EFAULT);
        return Ok(true);
    }
    cpu.write(10, 0);
    Ok(true)
}

fn linux_uname<B: Bus>(cpu: &mut Cpu, mem: &mut B) -> Result<bool, FalconError> {
    // uname(utsname_ptr=a0) -> 0 or -errno
    // struct utsname: six 65-byte NUL-terminated fields (sysname, nodename,
    // release, version, machine, domainname).
    let ptr = cpu.read(10);
    let fields: [&str; 6] = ["Linux", "raven", "6.1.0-raven", "#1 SMP", "riscv32", ""];
    for (i, field) in fields.iter().enumerate() {
        let base = ptr.wrapping_add((i * 65) as u32);
        let bytes = field.as_bytes();
        for (j, &b) in bytes.iter().take(64).enumerate() {
            if mem.store8(base.wrapping_add(j as u32), b).is_err() {
                cpu.write(10, LINUX_EFAULT);
                return Ok(true);
            }
        }
        if mem
            .store8(base.wrapping_add(bytes.len().min(64) as u32), 0)
            .is_err()
        {
            cpu.write(10, LINUX_EFAULT);
            return Ok(true);
        }
    }
    cpu.write(10, 0);
    Ok(true)
}

fn linux_fstat<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    // fstat(fd=a0, stat_ptr=a1) -> 0 or -errno
    // Layout matches the rv32/rv64 generic `struct stat` (128 bytes,
    // 8-byte-aligned 64-bit fields) closely enough for callers that check
    // `S_ISCHR`/`S_ISREG`/`S_ISDIR` on st_mode and st_size for real files.
    const ST_MODE_OFFSET: u32 = 16;
    const ST_NLINK_OFFSET: u32 = 20;
    const ST_SIZE_OFFSET: u32 = 48;
    const S_IFCHR: u32 = 0o020000;
    const S_IFDIR: u32 = 0o040000;
    const S_IFREG: u32 = 0o100000;

    let fd = cpu.read(10) as i32;
    let ptr = cpu.read(11);

    let (mode, size) = if (0..=2).contains(&fd) {
        (S_IFCHR | 0o666, 0u64)
    } else {
        match console.filesystem.stat_fd(fd) {
            Ok(st) if st.is_dir => (S_IFDIR | 0o755, 0),
            Ok(st) => (S_IFREG | 0o644, st.size),
            Err(e) => {
                cpu.write(10, fs_errno(e));
                return Ok(true);
            }
        }
    };

    for i in 0..128u32 {
        if mem.store8(ptr.wrapping_add(i), 0).is_err() {
            cpu.write(10, LINUX_EFAULT);
            return Ok(true);
        }
    }
    let size_bytes = size.to_le_bytes();
    let write_ok = mem.store32(ptr.wrapping_add(ST_MODE_OFFSET), mode).is_ok()
        && mem.store32(ptr.wrapping_add(ST_NLINK_OFFSET), 1).is_ok()
        && size_bytes.chunks(4).enumerate().all(|(i, chunk)| {
            let word = u32::from_le_bytes(chunk.try_into().unwrap());
            mem.store32(ptr.wrapping_add(ST_SIZE_OFFSET + i as u32 * 4), word)
                .is_ok()
        });
    if !write_ok {
        cpu.write(10, LINUX_EFAULT);
        return Ok(true);
    }
    cpu.write(10, 0);
    Ok(true)
}

fn linux_openat<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let path = read_zstr(mem, cpu.read(11))?;
    let path = String::from_utf8_lossy(&path).into_owned();
    let flags = cpu.read(12);
    match console.filesystem.open(&path, flags) {
        Ok(fd) => cpu.write(10, fd as u32),
        Err(e) => cpu.write(10, fs_errno(e)),
    }
    Ok(true)
}

fn linux_faccessat<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let path = read_zstr(mem, cpu.read(11))?;
    let path = String::from_utf8_lossy(&path);
    cpu.write(
        10,
        if console.filesystem.exists(&path) {
            0
        } else {
            LINUX_ENOENT
        },
    );
    Ok(true)
}

fn linux_unlinkat<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let path = read_zstr(mem, cpu.read(11))?;
    let path = String::from_utf8_lossy(&path);
    match console.filesystem.unlink(&path) {
        Ok(()) => cpu.write(10, 0),
        Err(e) => cpu.write(10, fs_errno(e)),
    }
    Ok(true)
}

fn linux_mkdirat<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    let path = read_zstr(mem, cpu.read(11))?;
    let path = String::from_utf8_lossy(&path);
    match console.filesystem.mkdir(&path) {
        Ok(()) => cpu.write(10, 0),
        Err(e) => cpu.write(10, fs_errno(e)),
    }
    Ok(true)
}

fn linux_prlimit64<B: Bus>(cpu: &mut Cpu, mem: &mut B) -> Result<bool, FalconError> {
    // prlimit64(pid=a0, resource=a1, new_limit=a2, old_limit=a3) -> 0 or -errno
    // struct rlimit64 { rlim_cur: u64, rlim_max: u64 }. We report "unlimited".
    let old_limit = cpu.read(13);
    if old_limit != 0 {
        const RLIM_INFINITY: u64 = u64::MAX;
        let bytes = RLIM_INFINITY.to_le_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            let _ = mem.store8(old_limit.wrapping_add(i as u32), b);
            let _ = mem.store8(old_limit.wrapping_add(8 + i as u32), b);
        }
    }
    cpu.write(10, 0);
    Ok(true)
}
