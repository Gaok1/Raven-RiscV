use crate::{
    falcon::{errors::FalconError, memory::Bus, registers::Cpu},
    ui::{console::ConsoleColor, Console},
};

// Linux ABI syscall numbers
const SYS_WRITEV: u32 = 66;
const SYS_READ: u32 = 63;
const SYS_WRITE: u32 = 64;
const SYS_EXIT: u32 = 93;
const SYS_EXIT_GROUP: u32 = 94;
const SYS_GETPID: u32 = 172;
const SYS_GETUID: u32 = 174;
const SYS_GETGID: u32 = 176;
const SYS_BRK:       u32 = 214;
const SYS_MUNMAP: u32 = 215;
const SYS_MMAP: u32 = 222;
const SYS_GETRANDOM: u32 = 278;
const SYS_CLOCK_GETTIME: u32 = 403;

// Falcon teaching extensions
const FALCON_PRINT_INT: u32 = 1000;
const FALCON_PRINT_ZSTR: u32 = 1001;
const FALCON_PRINT_ZSTR_LN: u32 = 1002;
const FALCON_READ_LINE_Z: u32 = 1003;
const FALCON_PRINT_UINT: u32 = 1004;
const FALCON_PRINT_HEX: u32 = 1005;
const FALCON_PRINT_CHAR: u32 = 1006;
const FALCON_PRINT_NEWLINE: u32 = 1008;
const FALCON_READ_U8: u32 = 1010;
const FALCON_READ_U16: u32 = 1011;
const FALCON_READ_U32: u32 = 1012;
const FALCON_READ_INT: u32 = 1013;
const FALCON_READ_FLOAT: u32 = 1014;
const FALCON_PRINT_FLOAT: u32 = 1015;
const FALCON_GET_INSTR_COUNT: u32 = 1030;
const FALCON_GET_CYCLE_COUNT: u32 = 1031;
const FALCON_MEMSET: u32 = 1050;
const FALCON_MEMCPY: u32 = 1051;
const FALCON_STRLEN: u32 = 1052;
const FALCON_STRCMP: u32 = 1053;

// Linux errno values
const LINUX_EBADF: u32 = (-9i32) as u32;
const LINUX_EFAULT: u32 = (-14i32) as u32;
const LINUX_EIO: u32 = (-5i32) as u32;
const LINUX_EINVAL: u32 = (-22i32) as u32;
const LINUX_ENOMEM: u32 = (-12i32) as u32;

/// Handles syscalls invoked via `ecall`.
///
/// - Linux-like subset: `read(63)`, `write(64)`, `exit(93)`, `exit_group(94)`
/// - Falcon teaching extensions: `1000..` (used by assembler pseudos)
///
/// ABI (Linux-style):
/// - `a7` = syscall number
/// - `a0..a5` = args
/// - `a0` = return value (negative values mean `-errno`, represented as `u32`)
pub fn handle_syscall<B: Bus>(
    code: u32,
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    match code {
        // --- Linux ABI subset ---
        SYS_READ => linux_read(cpu, mem, console),
        SYS_WRITE => linux_write(cpu, mem, console),
        SYS_WRITEV => linux_writev(cpu, mem, console),
        SYS_BRK => {
            // brk(0) → query current break; brk(addr) → extend break to addr.
            // Returns the new (or current) break; returns current break on failure.
            let requested = cpu.read(10);
            if requested == 0 || requested <= cpu.heap_break {
                cpu.write(10, cpu.heap_break);
            } else {
                cpu.heap_break = requested;
                cpu.write(10, requested);
            }
            Ok(true)
        }
        SYS_MMAP => linux_mmap(cpu, mem, console),
        SYS_MUNMAP => {
            // munmap: nop — Raven has no real virtual memory management.
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
        SYS_GETPID => {
            cpu.write(10, 1);
            Ok(true)
        }
        SYS_GETUID | SYS_GETGID => {
            cpu.write(10, 0);
            Ok(true)
        }
        SYS_CLOCK_GETTIME => linux_clock_gettime(cpu, mem),

        // --- Falcon teaching extensions (used by pseudos) ---
        FALCON_PRINT_INT => {
            let s = (cpu.read(10) as i32).to_string();
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
            Ok(true)
        }
        FALCON_PRINT_UINT => {
            let s = cpu.read(10).to_string();
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
            Ok(true)
        }
        FALCON_PRINT_HEX => {
            let s = format!("0x{:08X}", cpu.read(10));
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
            Ok(true)
        }
        FALCON_PRINT_CHAR => {
            let b = cpu.read(10) as u8;
            cpu.stdout.push(b);
            if b == b'\n' {
                console.newline();
            } else {
                console.append_str(&String::from_utf8_lossy(&[b]));
            }
            Ok(true)
        }
        FALCON_PRINT_NEWLINE => {
            cpu.stdout.push(b'\n');
            console.newline();
            Ok(true)
        }
        FALCON_PRINT_ZSTR => {
            let bytes = read_zstr(mem, cpu.read(10))?;
            cpu.stdout.extend_from_slice(&bytes);
            console_write_bytes(console, &bytes);
            Ok(true)
        }
        FALCON_PRINT_ZSTR_LN => {
            let bytes = read_zstr(mem, cpu.read(10))?;
            cpu.stdout.extend_from_slice(&bytes);
            console_write_bytes(console, &bytes);
            cpu.stdout.push(b'\n');
            console.newline();
            Ok(true)
        }
        FALCON_PRINT_FLOAT => {
            let v = cpu.fread(10); // fa0
            let s = format_float(v);
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
            Ok(true)
        }
        FALCON_READ_LINE_Z => {
            let mut addr = cpu.read(10);
            if let Some(line) = console.read_line() {
                for b in line.as_bytes() {
                    mem.store8(addr, *b)?;
                    addr = addr.wrapping_add(1);
                }
                mem.store8(addr, 0)?; // NUL
                console.reading = false;
                Ok(true)
            } else {
                console.reading = true;
                Ok(false)
            }
        }
        FALCON_READ_U8 => falcon_read_u8(cpu, mem, console),
        FALCON_READ_U16 => falcon_read_u16(cpu, mem, console),
        FALCON_READ_U32 => falcon_read_u32(cpu, mem, console),
        FALCON_READ_INT => falcon_read_int(cpu, mem, console),
        FALCON_READ_FLOAT => falcon_read_float(cpu, mem, console),
        FALCON_GET_INSTR_COUNT | FALCON_GET_CYCLE_COUNT => {
            cpu.write(10, cpu.instr_count as u32);
            Ok(true)
        }

        // --- Falcon memory utilities ---
        FALCON_MEMSET => {
            let addr = cpu.read(10);
            let byte = cpu.read(11) as u8;
            let len = cpu.read(12) as usize;
            for i in 0..len {
                mem.store8(addr.wrapping_add(i as u32), byte)?;
            }
            Ok(true)
        }
        FALCON_MEMCPY => {
            let dst = cpu.read(10);
            let src = cpu.read(11);
            let len = cpu.read(12) as usize;
            for i in 0..len {
                let b = mem.load8(src.wrapping_add(i as u32))?;
                mem.store8(dst.wrapping_add(i as u32), b)?;
            }
            Ok(true)
        }
        FALCON_STRLEN => {
            let mut addr = cpu.read(10);
            let mut len: u32 = 0;
            loop {
                let b = mem.load8(addr)?;
                if b == 0 { break; }
                len += 1;
                addr = addr.wrapping_add(1);
            }
            cpu.write(10, len);
            Ok(true)
        }
        FALCON_STRCMP => {
            let mut a = cpu.read(10);
            let mut b = cpu.read(11);
            loop {
                let ca = mem.load8(a)?;
                let cb = mem.load8(b)?;
                if ca != cb {
                    cpu.write(10, if ca < cb { (-1i32) as u32 } else { 1 });
                    return Ok(true);
                }
                if ca == 0 { break; }
                a = a.wrapping_add(1);
                b = b.wrapping_add(1);
            }
            cpu.write(10, 0);
            Ok(true)
        }

        _ => {
            console.push_error(format!("Unimplemented syscall {code}"));
            Ok(false)
        }
    }
}

fn linux_read<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    // Linux: read(fd=a0, buf=a1, count=a2) -> a0 = n or -errno
    let fd = cpu.read(10);
    let buf = cpu.read(11);
    let count = cpu.read(12) as usize;

    if fd != 0 {
        cpu.write(10, LINUX_EBADF);
        console.push_error(format!("read: unsupported fd {fd} (only fd=0 supported)"));
        return Ok(true);
    }
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }

    // If we have buffered bytes from a previous read, use them first.
    if cpu.stdin.is_empty() {
        if let Some(line) = console.read_line() {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n'); // terminal-like
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

fn linux_write<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    // Linux: write(fd=a0, buf=a1, count=a2) -> a0 = n or -errno
    let fd = cpu.read(10);
    let buf = cpu.read(11);
    let count = cpu.read(12) as usize;

    if fd != 1 && fd != 2 {
        cpu.write(10, LINUX_EBADF);
        console.push_error(format!("write: unsupported fd {fd} (only fd=1/2 supported)"));
        return Ok(true);
    }
    if count == 0 {
        cpu.write(10, 0);
        return Ok(true);
    }

    let mut bytes = Vec::with_capacity(count);
    for i in 0..count {
        let addr = buf.wrapping_add(i as u32);
        match mem.load8(addr) {
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
        console_write_bytes_colored(console, &bytes, ConsoleColor::Error);
    } else {
        console_write_bytes(console, &bytes);
    }
    cpu.write(10, count as u32);
    Ok(true)
}

fn linux_getrandom<B: Bus>(
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    // Linux: getrandom(buf=a0, buflen=a1, flags=a2) -> a0 = n or -errno
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

fn read_zstr(mem: &impl Bus, mut addr: u32) -> Result<Vec<u8>, FalconError> {
    let mut bytes = Vec::new();
    loop {
        let b = mem.load8(addr)?;
        if b == 0 {
            break;
        }
        bytes.push(b);
        addr = addr.wrapping_add(1);
    }
    Ok(bytes)
}

fn console_write_bytes(console: &mut Console, bytes: &[u8]) {
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            if start < i {
                console.append_str(&String::from_utf8_lossy(&bytes[start..i]));
            }
            console.newline();
            start = i + 1;
        }
    }
    if start < bytes.len() {
        console.append_str(&String::from_utf8_lossy(&bytes[start..]));
    }
}

fn console_write_bytes_colored(console: &mut Console, bytes: &[u8], color: ConsoleColor) {
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            if start < i {
                console.append_str_colored(&String::from_utf8_lossy(&bytes[start..i]), color);
            }
            console.newline();
            start = i + 1;
        }
    }
    if start < bytes.len() {
        console.append_str_colored(&String::from_utf8_lossy(&bytes[start..]), color);
    }
}

fn falcon_read_u8<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        let val = parse_u64(s);
        if let Some(v) = val {
            if v <= 0xFF {
                mem.store8(addr, v as u8)?;
                console.reading = false;
                Ok(true)
            } else {
                console.push_error("readByte: value out of range (0..255)");
                console.reading = true;
                Ok(false)
            }
        } else {
            console.push_error("readByte: invalid number");
            console.reading = true;
            Ok(false)
        }
    } else {
        console.reading = true;
        Ok(false)
    }
}

fn falcon_read_u16<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        let val = parse_u64(s);
        if let Some(v) = val {
            if v <= 0xFFFF {
                mem.store16(addr, v as u16)?;
                console.reading = false;
                Ok(true)
            } else {
                console.push_error("readHalf: value out of range (0..65535)");
                console.reading = true;
                Ok(false)
            }
        } else {
            console.push_error("readHalf: invalid number");
            console.reading = true;
            Ok(false)
        }
    } else {
        console.reading = true;
        Ok(false)
    }
}

fn falcon_read_u32<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        let val = parse_u64(s);
        if let Some(v) = val {
            if v <= 0xFFFF_FFFF {
                mem.store32(addr, v as u32)?;
                console.reading = false;
                Ok(true)
            } else {
                console.push_error("readWord: value out of range (0..4294967295)");
                console.reading = true;
                Ok(false)
            }
        } else {
            console.push_error("readWord: invalid number");
            console.reading = true;
            Ok(false)
        }
    } else {
        console.reading = true;
        Ok(false)
    }
}

fn parse_u64(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>()
            .ok()
            .and_then(|v| if v < 0 { None } else { Some(v as u64) })
    }
}

fn falcon_read_int<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        // Parse as signed decimal or 0x hex
        let val: Option<i32> = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok().map(|v| v as i32)
        } else {
            s.parse::<i32>().ok()
        };
        if let Some(v) = val {
            mem.store32(addr, v as u32)?;
            console.reading = false;
            Ok(true)
        } else {
            console.push_error("readInt: invalid integer");
            console.reading = true;
            Ok(false)
        }
    } else {
        console.reading = true;
        Ok(false)
    }
}

fn falcon_read_float<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        if let Ok(v) = s.parse::<f32>() {
            mem.store32(addr, v.to_bits())?;
            console.reading = false;
            Ok(true)
        } else {
            console.push_error("readFloat: invalid float");
            console.reading = true;
            Ok(false)
        }
    } else {
        console.reading = true;
        Ok(false)
    }
}

fn linux_writev<B: Bus>(cpu: &mut Cpu, mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
    // writev(fd=a0, iov=a1, iovcnt=a2) -> bytes written or -errno
    // struct iovec { void *base; size_t len; } — both u32 on RV32
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
        let base = match mem.load32(entry) {
            Ok(v) => v,
            Err(_) => { cpu.write(10, LINUX_EFAULT); return Ok(true); }
        };
        let len = match mem.load32(entry.wrapping_add(4)) {
            Ok(v) => v as usize,
            Err(_) => { cpu.write(10, LINUX_EFAULT); return Ok(true); }
        };
        if len == 0 { continue; }

        let mut bytes = Vec::with_capacity(len);
        for j in 0..len {
            match mem.load8(base.wrapping_add(j as u32)) {
                Ok(b) => bytes.push(b),
                Err(_) => { cpu.write(10, LINUX_EFAULT); return Ok(true); }
            }
        }
        cpu.stdout.extend_from_slice(&bytes);
        if fd == 2 {
            console_write_bytes_colored(console, &bytes, ConsoleColor::Error);
        } else {
            console_write_bytes(console, &bytes);
        }
        total += len as u32;
    }

    cpu.write(10, total);
    Ok(true)
}

fn linux_mmap<B: Bus>(cpu: &mut Cpu, _mem: &mut B, console: &mut Console) -> Result<bool, FalconError> {
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

    // Align len up to 4 bytes
    let aligned_len = (len.wrapping_add(3)) & !3;
    let ptr = cpu.heap_break;
    let new_break = ptr.wrapping_add(aligned_len);

    // Simple overflow / out-of-range check (Raven RAM is 128 KB = 0x20000)
    if new_break > 0x0002_0000 || new_break < ptr {
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
    // We report time as a fixed-frequency counter based on instr_count.
    let tp = cpu.read(11);
    // Approximate: 1 instruction ≈ 10 ns (100 MHz equivalent)
    let ns_total = cpu.instr_count.wrapping_mul(10);
    let tv_sec = (ns_total / 1_000_000_000) as u32;
    let tv_nsec = (ns_total % 1_000_000_000) as u32;

    if mem.store32(tp, tv_sec).is_err() || mem.store32(tp.wrapping_add(4), tv_nsec).is_err() {
        cpu.write(10, LINUX_EFAULT);
        return Ok(true);
    }
    cpu.write(10, 0);
    Ok(true)
}

fn format_float(v: f32) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else {
        // Use up to 6 significant digits, strip trailing zeros
        let s = format!("{:.6}", v);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}
