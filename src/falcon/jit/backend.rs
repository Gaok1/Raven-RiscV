use crate::falcon::errors::FalconError;
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

use super::profile::HotProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    None,
    Hot,
    Full,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::None => "none",
            BackendKind::Hot => "hot",
            BackendKind::Full => "full",
        }
    }
}

impl Default for BackendKind {
    fn default() -> Self {
        BackendKind::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecOutcome {
    Stepped { instructions: u32 },
    Halted,
    AwaitingInput,
}

pub struct ExecCtx<'a, B: Bus> {
    pub cpu: &'a mut Cpu,
    pub mem: &'a mut B,
    pub console: &'a mut Console,
}

impl<'a, B: Bus> ExecCtx<'a, B> {
    pub fn new(cpu: &'a mut Cpu, mem: &'a mut B, console: &'a mut Console) -> Self {
        Self { cpu, mem, console }
    }
}

pub trait ExecutionBackend<B: Bus>: Send {
    fn kind(&self) -> BackendKind;

    fn run_until_yield(&mut self, ctx: &mut ExecCtx<'_, B>) -> Result<ExecOutcome, FalconError>;

    fn invalidate(&mut self, _start: u32, _end: u32) {}

    fn hot_profile(&self) -> Option<&HotProfile> {
        None
    }
}
