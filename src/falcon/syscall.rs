use crate::{
    falcon::{errors::FalconError, memory::Bus, registers::Cpu},
    ui::{console::ConsoleColor, Console},
};

const SYS_READ: u32 = 63;
const SYS_WRITE: u32 = 64;
const SYS_EXIT: u32 = 93;
const SYS_EXIT_GROUP: u32 = 94;
const SYS_BRK:       u32 = 214;
const SYS_GETRANDOM: u32 = 278;

const FALCON_PRINT_INT: u32 = 1000;
const FALCON_PRINT_ZSTR: u32 = 1001;
const FALCON_PRINT_ZSTR_LN: u32 = 1002;
const FALCON_READ_LINE_Z: u32 = 1003;
const FALCON_READ_U8: u32 = 1010;
const FALCON_READ_U16: u32 = 1011;
const FALCON_READ_U32: u32 = 1012;

const LINUX_EBADF: u32 = (-9i32) as u32;
const LINUX_EFAULT: u32 = (-14i32) as u32;
const LINUX_EIO: u32 = (-5i32) as u32;
const LINUX_EINVAL: u32 = (-22i32) as u32;

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
        SYS_GETRANDOM => linux_getrandom(cpu, mem, console),
        SYS_EXIT | SYS_EXIT_GROUP => {
            let code = cpu.read(10);
            cpu.exit_code = Some(code);
            console.push_error(format!("Exit {}", code as i32));
            Ok(false)
        }

        // --- Falcon teaching extensions (used by pseudos) ---
        FALCON_PRINT_INT => {
            let s = (cpu.read(10) as i32).to_string();
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
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
