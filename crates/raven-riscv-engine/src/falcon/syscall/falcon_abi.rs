//! Falcon teaching extensions (`a7` in `1000..`), used by the assembler's
//! pseudo-instructions (`print_int`, `read_int`, ...) and by multi-hart
//! control (`hart_start`, `hart_exit`, `map_exec`).

use super::abi::{SyscallAbi, SyscallCtx};
use super::errno::LINUX_EINVAL;
use super::helpers::{console_write_bytes, format_float, parse_u64, read_zstr};
use crate::falcon::{
    errors::FalconError,
    memory::Bus,
    registers::{ExecRegion, HartStartRequest},
};

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
pub const FALCON_HART_START: u32 = 1100;
pub const FALCON_HART_EXIT: u32 = 1101;
pub const FALCON_MAP_EXEC: u32 = 1102;

pub struct FalconAbi;

impl<B: Bus> SyscallAbi<B> for FalconAbi {
    fn name(&self, code: u32) -> Option<&'static str> {
        Some(match code {
            FALCON_PRINT_INT => "print_int",
            FALCON_PRINT_ZSTR => "print_zstr",
            FALCON_PRINT_ZSTR_LN => "print_zstr_ln",
            FALCON_READ_LINE_Z => "read_line_z",
            FALCON_PRINT_UINT => "print_uint",
            FALCON_PRINT_HEX => "print_hex",
            FALCON_PRINT_CHAR => "print_char",
            FALCON_PRINT_NEWLINE => "print_newline",
            FALCON_READ_U8 => "read_u8",
            FALCON_READ_U16 => "read_u16",
            FALCON_READ_U32 => "read_u32",
            FALCON_READ_INT => "read_int",
            FALCON_READ_FLOAT => "read_float",
            FALCON_PRINT_FLOAT => "print_float",
            FALCON_GET_INSTR_COUNT => "get_instr_count",
            FALCON_GET_CYCLE_COUNT => "get_cycle_count",
            FALCON_MEMSET => "memset",
            FALCON_MEMCPY => "memcpy",
            FALCON_STRLEN => "strlen",
            FALCON_STRCMP => "strcmp",
            FALCON_HART_START => "hart_start",
            FALCON_HART_EXIT => "hart_exit",
            FALCON_MAP_EXEC => "map_exec",
            _ => return None,
        })
    }

    fn handle(&self, code: u32, ctx: &mut SyscallCtx<B>) -> Option<Result<bool, FalconError>> {
        <Self as SyscallAbi<B>>::name(self, code)?;
        let cpu = &mut *ctx.cpu;
        let mem = &mut *ctx.mem;
        let console = &mut *ctx.console;
        let cycle_override = ctx.cycle_override;
        Some((|| -> Result<bool, FalconError> {
            match code {
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
                FALCON_GET_INSTR_COUNT => {
                    cpu.write(10, cpu.instr_count as u32);
                    cpu.write(11, (cpu.instr_count >> 32) as u32);
                    Ok(true)
                }
                FALCON_GET_CYCLE_COUNT => {
                    let cycles = cycle_override.unwrap_or_else(|| mem.total_cycles());
                    cpu.write(10, cycles as u32);
                    cpu.write(11, (cycles >> 32) as u32);
                    Ok(true)
                }
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
                        let b = mem.user_load8(src.wrapping_add(i as u32))?;
                        mem.store8(dst.wrapping_add(i as u32), b)?;
                    }
                    Ok(true)
                }
                FALCON_STRLEN => {
                    let mut addr = cpu.read(10);
                    let mut len: u32 = 0;
                    loop {
                        let b = mem.user_load8(addr)?;
                        if b == 0 {
                            break;
                        }
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
                        let ca = mem.user_load8(a)?;
                        let cb = mem.user_load8(b)?;
                        if ca != cb {
                            cpu.write(10, if ca < cb { (-1i32) as u32 } else { 1 });
                            return Ok(true);
                        }
                        if ca == 0 {
                            break;
                        }
                        a = a.wrapping_add(1);
                        b = b.wrapping_add(1);
                    }
                    cpu.write(10, 0);
                    Ok(true)
                }
                FALCON_HART_START => {
                    cpu.pending_hart_start = Some(HartStartRequest {
                        entry_pc: cpu.read(10),
                        stack_ptr: cpu.read(11),
                        arg: cpu.read(12),
                    });
                    cpu.write(10, 0);
                    Ok(true)
                }
                FALCON_HART_EXIT => {
                    cpu.local_exit = true;
                    Ok(false)
                }
                FALCON_MAP_EXEC => {
                    let start = cpu.read(10);
                    let len = cpu.read(11);
                    if len == 0 || (start & 3) != 0 || (len & 3) != 0 {
                        cpu.write(10, LINUX_EINVAL);
                        return Ok(true);
                    }
                    let Some(end) = start.checked_add(len) else {
                        cpu.write(10, LINUX_EINVAL);
                        return Ok(true);
                    };
                    if mem.user_load8(start).is_err() || mem.user_load8(end - 1).is_err() {
                        cpu.write(10, super::errno::LINUX_EFAULT);
                        return Ok(true);
                    }
                    cpu.pending_exec_map = Some(ExecRegion::new(start, end));
                    cpu.write(10, 0);
                    Ok(true)
                }
                _ => unreachable!("FalconAbi::handle called with unclaimed code"),
            }
        })())
    }
}

fn falcon_read_u8<B: Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut crate::ui::Console,
) -> Result<bool, FalconError> {
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

fn falcon_read_u16<B: Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut crate::ui::Console,
) -> Result<bool, FalconError> {
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

fn falcon_read_u32<B: Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut crate::ui::Console,
) -> Result<bool, FalconError> {
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

fn falcon_read_int<B: Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut crate::ui::Console,
) -> Result<bool, FalconError> {
    let addr = cpu.read(10);
    if let Some(line) = console.read_line() {
        let s = line.trim();
        let val: Option<i32> =
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
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

fn falcon_read_float<B: Bus>(
    cpu: &mut crate::falcon::registers::Cpu,
    mem: &mut B,
    console: &mut crate::ui::Console,
) -> Result<bool, FalconError> {
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
