//! Named register banks.
//!
//! A host that wants to draw registers has to know how many there are, how wide
//! they are, and what to call them — without assuming RISC-V's 32 integer plus
//! 32 float registers. Backends describe their file in [`RegisterBank`]s and
//! the host walks whatever it finds.

use crate::MachineError;

/// One group of registers that share a width and a naming scheme.
///
/// Toy16 has a single bank of 8 sixteen-bit registers; RV32 has two banks of 32
/// thirty-two-bit ones. Both are just a list of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterBank {
    /// Prefix used to build a register's name: `"x"` gives `x0`, `x1`, …
    pub prefix: &'static str,
    /// Human-readable group name for a heading — "Integer", "Float".
    pub label: &'static str,
    pub count: usize,
    pub bits: u8,
}

impl RegisterBank {
    /// How many hex digits a value from this bank needs.
    pub fn hex_width(&self) -> usize {
        usize::from(self.bits).div_ceil(4)
    }
}

/// Identifies one register: which bank, and which index inside it.
///
/// Deliberately not a flat number — a flat numbering would force every backend
/// to agree on an order, and would silently mean something different the moment
/// a bank changed size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegisterId {
    pub bank: usize,
    pub index: usize,
}

impl RegisterId {
    pub fn new(bank: usize, index: usize) -> Self {
        Self { bank, index }
    }
}

/// One register, ready to render.
///
/// Produced by [`RegisterFile::entries`] so a host can draw a register pane
/// without calling four methods per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterEntry {
    pub id: RegisterId,
    /// Positional name, always present: `x10`, `r2`.
    pub name: String,
    /// Conventional name, when the ISA has one: `a0` for `x10`.
    pub alias: Option<String>,
    pub value: u64,
    pub bits: u8,
}

impl RegisterEntry {
    /// What to put in a narrow column: the alias if the ISA has one, else the
    /// positional name.
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// The value as fixed-width hex, sized to the register.
    pub fn hex(&self) -> String {
        format!("{:0width$X}", self.value, width = usize::from(self.bits).div_ceil(4))
    }
}

/// A backend's register file, described rather than assumed.
pub trait RegisterFile {
    /// The banks, in the order a host should display them.
    fn banks(&self) -> &[RegisterBank];

    fn read(&self, id: RegisterId) -> Option<u64>;

    /// Write a register. Backends reject writes the ISA forbids — RISC-V's
    /// hardwired `x0`, for instance — rather than accepting them silently.
    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError>;

    fn program_counter(&self) -> u64;

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError>;

    /// The conventional name for a register, when the ISA defines one.
    fn alias(&self, _id: RegisterId) -> Option<&'static str> {
        None
    }

    /// Find a register by any name a user might type — positional or alias,
    /// case-insensitively.
    ///
    /// The default walks the banks, so a backend only needs to override this if
    /// it accepts names the generated ones do not cover.
    fn resolve(&self, name: &str) -> Option<RegisterId> {
        let wanted = name.trim();
        self.banks()
            .iter()
            .enumerate()
            .flat_map(|(bank, spec)| (0..spec.count).map(move |index| RegisterId::new(bank, index)))
            .find(|id| {
                self.name(*id).is_some_and(|n| n.eq_ignore_ascii_case(wanted))
                    || self.alias(*id).is_some_and(|a| a.eq_ignore_ascii_case(wanted))
            })
    }

    /// The positional name of a register — `x10`, `r2` — or `None` when the id
    /// does not name a register in this file.
    fn name(&self, id: RegisterId) -> Option<String> {
        let bank = self.banks().get(id.bank)?;
        (id.index < bank.count).then(|| format!("{}{}", bank.prefix, id.index))
    }

    /// Every register in display order. The one call a register pane needs.
    fn entries(&self) -> Vec<RegisterEntry> {
        self.banks()
            .iter()
            .enumerate()
            .flat_map(|(bank, spec)| {
                (0..spec.count).map(move |index| (RegisterId::new(bank, index), spec.bits))
            })
            .filter_map(|(id, bits)| {
                Some(RegisterEntry {
                    id,
                    name: self.name(id)?,
                    alias: self.alias(id).map(str::to_string),
                    value: self.read(id)?,
                    bits,
                })
            })
            .collect()
    }
}
