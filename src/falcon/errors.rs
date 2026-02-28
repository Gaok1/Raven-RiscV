use thiserror::Error;

/// Errors that can occur within the Falcon emulator.
#[derive(Error, Debug)]
pub enum FalconError {
    /// Problems decoding an instruction.
    #[error("Decode error: {0}")]
    Decode(&'static str),

    /// Bus or memory access errors.
    #[error("Bus error: {0}")]
    Bus(&'static str),
}

