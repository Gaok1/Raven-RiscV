use thiserror::Error;

/// Errors that can occur within the Falcon emulator.
#[derive(Error, Debug)]
pub enum FalconError {
    /// Problems decoding an instruction.
    #[error("Decode error: {0}")]
    Decode(&'static str),

    /// Bus or memory access errors.
    #[error("Bus error: {0}")]
    Bus(String),

    /// Feature or backend not implemented in the current build.
    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// A RISC-V trap raised mid-execution (page fault, etc.). The CPU step
    /// loop catches this, vectors through `mtvec`, and resumes.
    #[error("trap: cause={cause} tval=0x{tval:08X}")]
    Trap { cause: u32, tval: u32, vaddr: u32 },
}
