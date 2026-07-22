// falcon/registers.rs
use crate::falcon::mmu::PrivMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HartStartRequest {
    pub entry_pc: u32,
    pub stack_ptr: u32,
    pub arg: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecRegion {
    pub start: u32,
    pub end: u32,
}

impl ExecRegion {
    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[inline]
    pub fn contains(&self, addr: u32) -> bool {
        addr >= self.start && addr < self.end
    }
}

#[derive(Default, Clone)]
pub struct Cpu {
    pub x: [u32; 32], // x0..x31 (integer registers) â€” write via write()/fwrite() to enforce x0=0
    pub f: [u32; 32], // f0..f31 (float registers, stored as IEEE 754 bits)
    pub fcsr: u32,           // float control/status register (fflags only; FRM=RNE)
    pub pc: u32,
    /// buffer emulado de entrada (STDIN)
    pub stdin: Vec<u8>,
    /// buffer emulado de saÃ­da (STDOUT)
    pub stdout: Vec<u8>,
    /// Exit status when program terminates via Linux `exit`/`exit_group`.
    pub exit_code: Option<u32>,
    /// Stable hart identifier used by the shared atomic/coherence backend.
    pub hart_id: u32,
    /// LR/SC reservation address (None = no active reservation).
    /// This is debug/local metadata only; shared validity lives in memory state.
    pub lr_reservation: Option<u32>,
    /// Set when execution paused at an `ebreak` instruction (not a fault).
    pub ebreak_hit: bool,
    /// Current program break (heap end). Set by the loader; advanced by SYS_BRK.
    pub heap_break: u32,
    /// Number of instructions executed since program start.
    pub instr_count: u64,
    /// Deferred multi-hart start request emitted by SYS_HART_START.
    pub pending_hart_start: Option<HartStartRequest>,
    /// Deferred executable-range registration emitted by SYS_RAVEN_MAP_EXEC.
    pub pending_exec_map: Option<ExecRegion>,
    /// Set by FALCON_HART_EXIT (1101): exit only this hart, not the whole program.
    pub local_exit: bool,
    /// GFX_SLEEP_MS (2007) parking deadline. While `Some`, this hart stays
    /// parked on its `ecall` (the syscall re-executes and clears it once the
    /// wall-clock deadline passes). Per-hart on purpose: other harts keep going.
    pub sleep_until: Option<std::time::Instant>,

    // â”€â”€ Machine-mode CSRs (Phase 2 subset) â”€â”€
    // satp lives on the MMU side of the bus; the Cpu mirror is convenient for
    // CSR reads but writes must also go through `mem.set_satp` so the MMU
    // updates its Satp and flushes the TLB.
    pub satp: u32,
    pub mstatus: u32,
    pub mtvec: u32,
    pub mepc: u32,
    pub mcause: u32,
    pub mtval: u32,
    pub priv_mode: PrivMode,

    // â”€â”€ Supervisor-mode CSRs (Phase C â€” trap delegation) â”€â”€
    // `sstatus` is modelled as its own register rather than a masked view of
    // `mstatus`. Real hardware aliases the shared bits (SIE/SPIE/SPP) into
    // `mstatus`; we keep them separate as a pedagogical simplification so the
    // delegation path is easy to read. The supervisor trap handler uses SPP
    // (bit 8), SPIE (bit 5) and SIE (bit 1) exactly like mstatus's M-mode bits.
    pub sstatus: u32,
    pub stvec: u32,
    pub sepc: u32,
    pub scause: u32,
    pub stval: u32,
    pub sscratch: u32,
    /// Machine exception delegation: bit `c` set â‡’ exceptions with cause `c`
    /// taken in S/U mode are delegated to the supervisor handler (`stvec`).
    pub medeleg: u32,
    /// Machine interrupt delegation (stored for completeness; the simulator
    /// has no asynchronous interrupts yet, so this is observable but unused).
    pub mideleg: u32,
}

impl Cpu {
    #[inline]
    pub fn read(&self, r: u8) -> u32 {
        if r == 0 { 0 } else { self.x[r as usize] }
    }
    #[inline]
    pub fn write(&mut self, r: u8, v: u32) {
        if r != 0 {
            self.x[r as usize] = v;
        }
    }

    // Float register helpers (all registers are writable, unlike x0)
    #[inline]
    pub fn fread(&self, r: u8) -> f32 {
        f32::from_bits(self.f[r as usize])
    }
    #[inline]
    pub fn fwrite(&mut self, r: u8, v: f32) {
        self.f[r as usize] = v.to_bits();
    }
    #[inline]
    pub fn fread_bits(&self, r: u8) -> u32 {
        self.f[r as usize]
    }
    #[inline]
    pub fn fwrite_bits(&mut self, r: u8, v: u32) {
        self.f[r as usize] = v;
    }
}

