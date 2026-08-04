//! Small helpers shared by more than one syscall ABI module.

use crate::{
    falcon::{errors::FalconError, memory::Bus},
    ui::{Console, console::ConsoleColor},
};

pub fn read_zstr<B: Bus>(mem: &mut B, mut addr: u32) -> Result<Vec<u8>, FalconError> {
    let mut bytes = Vec::new();
    loop {
        let b = mem.user_load8(addr)?;
        if b == 0 {
            break;
        }
        bytes.push(b);
        addr = addr.wrapping_add(1);
    }
    Ok(bytes)
}

pub fn console_write_bytes(console: &mut Console, bytes: &[u8]) {
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

pub fn console_write_bytes_colored(console: &mut Console, bytes: &[u8], color: ConsoleColor) {
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

pub fn parse_u64(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>()
            .ok()
            .and_then(|v| if v < 0 { None } else { Some(v as u64) })
    }
}

pub fn format_float(v: f32) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        }
    } else {
        let s = format!("{:.6}", v);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}
