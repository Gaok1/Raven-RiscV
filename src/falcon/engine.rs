//! Falcon — a small, ergonomic headless engine for driving RISC-V programs from
//! Rust (tests, tools, graders). Build it with a few optional setters, `run()`,
//! then inspect the final machine state:
//!
//! ```no_run
//! use raven::falcon::Falcon;
//!
//! let r = Falcon::new()
//!     .asm(".text\n li a0, 42\n li a7, 93\n ecall\n")
//!     .max_cycles(10_000)          // optional
//!     .run()
//!     .unwrap();
//!
//! assert_eq!(r.exit_code, Some(42));
//! assert_eq!(r.reg("a0"), 42);
//! ```
//!
//! Sensible defaults (16 MB RAM, cache on, VM off, single hart) mean the only
//! thing you *must* provide is a program. Everything else is a `set_*`-style
//! optional. For the full CLI-shaped runner (multi-hart, pipeline, file config)
//! use [`crate::cli::run_headless`].

use crate::falcon::cache::CacheConfig;
use crate::falcon::jit::{BackendKind, ExecCtx, ExecOutcome, make_backend};
use crate::falcon::{CacheController, Cpu};
use crate::ui::Console;

/// Builder + engine. See the module docs for an example.
pub struct Falcon {
    asm: Option<String>,
    mem_size: usize,
    icfg: CacheConfig,
    dcfg: CacheConfig,
    cache_enabled: bool,
    vm: bool,
    max_cycles: u64,
    harts: usize,
    stdin: Vec<String>,
}

impl Default for Falcon {
    fn default() -> Self {
        Self {
            asm: None,
            mem_size: 16 * 1024 * 1024,
            icfg: CacheConfig::default(),
            dcfg: CacheConfig::default(),
            cache_enabled: true,
            vm: false,
            max_cycles: 10_000_000,
            harts: 1,
            stdin: Vec::new(),
        }
    }
}

impl Falcon {
    pub fn new() -> Self {
        Self::default()
    }

    /// The program to run, as RISC-V assembly text (what a student compiler emits).
    pub fn asm(mut self, src: impl Into<String>) -> Self {
        self.asm = Some(src.into());
        self
    }

    /// Load the assembly program from a file path.
    pub fn asm_file(self, path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let src = std::fs::read_to_string(path)?;
        Ok(self.asm(src))
    }

    /// RAM size in bytes. Stack pointer starts at the top of RAM. Default 16 MB.
    pub fn mem_bytes(mut self, bytes: usize) -> Self {
        self.mem_size = bytes;
        self
    }

    /// RAM size in mebibytes. Default 16.
    pub fn mem_mb(self, mb: usize) -> Self {
        self.mem_bytes(mb * 1024 * 1024)
    }

    /// Override the instruction (I) and data (D) L1 cache configs.
    pub fn cache(mut self, icache: CacheConfig, dcache: CacheConfig) -> Self {
        self.icfg = icache;
        self.dcfg = dcache;
        self
    }

    /// Run with the cache model bypassed (memory access is untimed/ideal).
    pub fn no_cache(mut self) -> Self {
        self.cache_enabled = false;
        self
    }

    /// Enable virtual memory (installs an identity megapage map + satp).
    pub fn vm(mut self, on: bool) -> Self {
        self.vm = on;
        self
    }

    /// Number of harts. Only 1 is supported here; use [`crate::cli::run_headless`]
    /// for multi-hart headless runs.
    pub fn harts(mut self, n: usize) -> Self {
        self.harts = n;
        self
    }

    /// Safety cap on scheduler iterations before the run is declared timed out.
    pub fn max_cycles(mut self, n: u64) -> Self {
        self.max_cycles = n;
        self
    }

    /// Pre-seed stdin, one entry per `read_line`-style syscall.
    pub fn stdin_line(mut self, line: impl Into<String>) -> Self {
        self.stdin.push(line.into());
        self
    }

    /// Assemble, load, and run to completion. Returns the final machine state.
    pub fn run(self) -> Result<RunResult, String> {
        if self.harts != 1 {
            return Err(
                "Falcon supports 1 hart; use raven::cli::run_headless for multi-hart".into(),
            );
        }
        let src = self.asm.as_deref().ok_or("no program: call .asm(...) first")?;
        let prog = crate::falcon::asm::assemble(src, 0x0)
            .map_err(|e| format!("assembly error at line {}: {}", e.line + 1, e.msg))?;

        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(self.icfg, self.dcfg, vec![], self.mem_size);
        mem.bypass = !self.cache_enabled;
        mem.mmu.enabled = self.vm;
        mem.mmu.force_translate = self.vm;

        // Load: text at 0x0, data at its base, zero the bss. Mirrors cli::load_asm_text.
        use crate::falcon::program::{load_bytes, load_words, zero_bytes};
        load_words(&mut mem.ram, 0x0, &prog.text).map_err(|e| format!("load error: {e}"))?;
        if !prog.data.is_empty() {
            load_bytes(&mut mem.ram, prog.data_base, &prog.data)
                .map_err(|e| format!("data load error: {e}"))?;
        }
        let bss_base = prog.data_base.wrapping_add(prog.data.len() as u32);
        if prog.bss_size > 0 {
            zero_bytes(&mut mem.ram, bss_base, prog.bss_size)
                .map_err(|e| format!("bss error: {e}"))?;
        }
        cpu.pc = 0x0;
        cpu.write(2, self.mem_size as u32); // SP = top of RAM
        let bss_end = bss_base.wrapping_add(prog.bss_size);
        cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

        if self.vm {
            let root_pa = (self.mem_size as u32).saturating_sub(4096);
            crate::falcon::mmu::Mmu::install_identity_megapages(&mut mem.ram, root_pa);
            let satp = (1u32 << 31) | (root_pa >> 12);
            cpu.satp = satp;
            mem.mmu.satp = crate::falcon::mmu::Satp::new(satp);
        }

        mem.invalidate_all();
        mem.reset_stats();

        // VM is not JIT-safe yet, and the interpreter is what tests want anyway.
        let mut backend = make_backend(BackendKind::None).map_err(|e| e.to_string())?;
        let mut console = Console::default();
        let mut stdout = Vec::new();
        let mut stdin = self.stdin.into_iter();
        let mut cycles = 0u64;
        let mut timed_out = false;

        loop {
            if cycles >= self.max_cycles {
                timed_out = true;
                break;
            }
            let outcome = {
                let mut ctx = ExecCtx::new(&mut cpu, &mut mem, &mut console);
                backend.run_until_yield(&mut ctx)
            };
            // ponytail: ignore cpu.sleep_until — tests should not wall-clock sleep.
            match outcome {
                Ok(ExecOutcome::Stepped { .. }) => {}
                Ok(ExecOutcome::AwaitingInput) => match stdin.next() {
                    Some(line) => console.push_input(line),
                    None => break, // EOF
                },
                Ok(ExecOutcome::Halted) => {
                    if cpu.ebreak_hit || cpu.local_exit || cpu.exit_code.is_some() {
                        break;
                    }
                    return Err(format!("fault at PC=0x{:08X}", cpu.pc));
                }
                Err(e) => return Err(format!("fault at PC=0x{:08X}: {e}", cpu.pc)),
            }
            if !cpu.stdout.is_empty() {
                stdout.append(&mut cpu.stdout);
            }
            cycles += 1;
        }
        stdout.append(&mut cpu.stdout);
        if cpu.exit_code.is_none() && (cpu.ebreak_hit || cpu.local_exit) {
            cpu.exit_code = Some(0);
        }

        Ok(RunResult {
            exit_code: cpu.exit_code,
            cycles,
            timed_out,
            stdout,
            cpu,
            mem,
        })
    }
}

/// Final machine state after [`Falcon::run`]. Inspect registers, memory, stdout.
pub struct RunResult {
    /// Exit code from an `exit` syscall, or `None` if the program never exited.
    pub exit_code: Option<u32>,
    /// Scheduler iterations executed (a rough progress bound, not instruction count).
    pub cycles: u64,
    /// True if the run hit `max_cycles` instead of halting on its own.
    pub timed_out: bool,
    stdout: Vec<u8>,
    cpu: Cpu,
    mem: CacheController,
}

impl RunResult {
    /// Program stdout as bytes.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout
    }

    /// Program stdout as a UTF-8 string (lossy).
    pub fn stdout(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    /// Register value by ABI name (`"a0"`, `"sp"`, ...) or `xN`. Panics on a bad name.
    pub fn reg(&self, name: &str) -> u32 {
        let i = crate::falcon::asm::utils::parse_reg(name)
            .unwrap_or_else(|| panic!("unknown register '{name}'"));
        self.cpu.read(i)
    }

    /// Register value by index (0..32).
    pub fn reg_x(&self, i: u8) -> u32 {
        self.cpu.read(i)
    }

    /// Program counter at exit.
    pub fn pc(&self) -> u32 {
        self.cpu.pc
    }

    /// Word at `addr` (cache-coherent — sees values still in a writeback cache).
    /// Panics if the address is out of range (a test-bug guard).
    pub fn read_word(&self, addr: u32) -> u32 {
        use crate::falcon::memory::Bus;
        self.mem
            .load32(addr)
            .unwrap_or_else(|e| panic!("read_word(0x{addr:08X}): {e}"))
    }

    /// Byte at `addr` (cache-coherent). Panics if out of range.
    pub fn read_byte(&self, addr: u32) -> u8 {
        use crate::falcon::memory::Bus;
        self.mem
            .load8(addr)
            .unwrap_or_else(|e| panic!("read_byte(0x{addr:08X}): {e}"))
    }

    /// Escape hatch: the raw final CPU.
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Escape hatch: the raw final memory/cache controller (for cache stats etc.).
    pub fn mem(&self) -> &CacheController {
        &self.mem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_asm_and_exposes_state() {
        // store 42 to mem[0x100], print it, exit(42).
        let r = Falcon::new()
            .asm(
                "\
                .text\n\
                li   t0, 42\n\
                li   t1, 0x100\n\
                sw   t0, 0(t1)\n\
                li   a7, 1000\n\
                mv   a0, t0\n\
                ecall\n\
                li   a0, 42\n\
                li   a7, 93\n\
                ecall\n",
            )
            .run()
            .unwrap();

        assert_eq!(r.exit_code, Some(42));
        assert!(!r.timed_out);
        assert_eq!(r.reg("a0"), 42);
        assert_eq!(r.reg("t0"), 42);
        assert_eq!(r.read_word(0x100), 42);
        assert_eq!(r.stdout(), "42");
    }

    #[test]
    fn multihart_is_rejected() {
        let err = Falcon::new().asm(".text\n ecall\n").harts(2).run();
        assert!(err.is_err());
    }
}
