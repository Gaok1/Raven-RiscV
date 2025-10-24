use crate::{
    falcon::{errors::FalconError, memory::Bus, registers::Cpu},
    ui::Console,
};

/// Emula syscalls simples baseadas em códigos em `a7`.
/// Retorna `Ok(true)` se o código é reconhecido e deve continuar,
/// `Ok(false)` para parar, ou `Err` se ocorrer um erro de memória.
pub fn handle_syscall<B: Bus>(
    code: u32,
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    Ok(match code {
        // 1: imprimir inteiro contido em a0 (sem quebra de linha)
        1 => {
            let s = (cpu.read(10) as i32).to_string();
            cpu.stdout.extend_from_slice(s.as_bytes());
            console.append_str(&s);
            true
        }
        // 2: imprimir string NUL-terminada apontada por a0, sem nova linha (append)
        2 => {
            let mut addr = cpu.read(10);
            let mut bytes = Vec::new();
            loop {
                let b = mem.load8(addr)?;
                if b == 0 {
                    break;
                }
                cpu.stdout.push(b);
                bytes.push(b);
                addr = addr.wrapping_add(1);
            }
            if let Ok(s) = std::str::from_utf8(&bytes) { console.append_str(s); }
            true
        }
        // 4: imprimir string NUL-terminada e adicionar '\n' (linha)
        4 => {
            let mut addr = cpu.read(10);
            let mut bytes = Vec::new();
            loop {
                let b = mem.load8(addr)?;
                if b == 0 { break; }
                cpu.stdout.push(b);
                bytes.push(b);
                addr = addr.wrapping_add(1);
            }
            if let Ok(s) = std::str::from_utf8(&bytes) {
                console.append_str(s);
                console.newline();
            }
            true
        }
        // 3: ler string de stdin e gravar na memória apontada por a0
        3 => {
            let mut addr = cpu.read(10);
            if let Some(line) = console.read_line() {
                for b in line.as_bytes() {
                    mem.store8(addr, *b)?;
                    addr = addr.wrapping_add(1);
                }
                mem.store8(addr, 0)?; // NUL
                                     // Input has been consumed; stop requesting console input
                console.reading = false;
                true
            } else {
                console.reading = true;
                false
            }
        }
        // 64: readByte -> parse number and store 1 byte at [a0]
        64 => {
            let mut addr = cpu.read(10);
            if let Some(line) = console.read_line() {
                let s = line.trim();
                let val = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    s.parse::<i64>().ok().and_then(|v| if v < 0 { None } else { Some(v as u64) })
                };
                if let Some(v) = val {
                    if v <= 0xFF {
                        mem.store8(addr, v as u8)?;
                        console.reading = false;
                        true
                    } else {
                        console.push_error("readByte: value out of range (0..255)");
                        console.reading = true; false
                    }
                } else {
                    console.push_error("readByte: invalid number");
                    console.reading = true; false
                }
            } else { console.reading = true; false }
        }
        // 65: readHalf -> parse number and store 2 bytes little-endian at [a0]
        65 => {
            let mut addr = cpu.read(10);
            if let Some(line) = console.read_line() {
                let s = line.trim();
                let val = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    s.parse::<i64>().ok().and_then(|v| if v < 0 { None } else { Some(v as u64) })
                };
                if let Some(v) = val {
                    if v <= 0xFFFF {
                        mem.store16(addr, v as u16)?;
                        console.reading = false;
                        true
                    } else {
                        console.push_error("readHalf: value out of range (0..65535)");
                        console.reading = true; false
                    }
                } else { console.push_error("readHalf: invalid number"); console.reading = true; false }
            } else { console.reading = true; false }
        }
        // 66: readWord -> parse number and store 4 bytes little-endian at [a0]
        66 => {
            let mut addr = cpu.read(10);
            if let Some(line) = console.read_line() {
                let s = line.trim();
                let val = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    s.parse::<i64>().ok().and_then(|v| if v < 0 { None } else { Some(v as u64) })
                };
                if let Some(v) = val {
                    if v <= 0xFFFF_FFFF {
                        mem.store32(addr, v as u32)?;
                        console.reading = false;
                        true
                    } else {
                        console.push_error("readWord: value out of range (0..4294967295)");
                        console.reading = true; false
                    }
                } else { console.push_error("readWord: invalid number"); console.reading = true; false }
            } else { console.reading = true; false }
        }
        _ => {
            console.push_error(format!("Unknown syscall code {code}"));
            false
        }
    })
}
