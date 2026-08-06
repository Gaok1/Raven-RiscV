//! The register namespace an out-of-order model schedules against.
//!
//! This is the piece that used to tie the models to one ISA. RV32 tracked a
//! fixed sixty-four entry space — thirty-two integer registers, thirty-two
//! float — and had to classify every instruction to decide which half an
//! operand named. None of that is needed here: a [`PipelineOp`] already reports
//! what it reads and writes by architectural name, so the table learns the
//! names as it meets them. An eight-register teaching machine gets eight rows;
//! x86-64 gets `rax` and `rflags` alongside each other without anyone deciding
//! what a "register file" is.
//!
//! [`PipelineOp`]: super::super::PipelineOp

use std::collections::HashMap;

/// Which in-flight instruction will produce a register's next value.
///
/// A scoreboard stores the functional unit; Tomasulo stores the reorder-buffer
/// entry. Both are "the thing that owes me this value", so one type serves.
pub type Producer = usize;

/// Names seen so far, and who owes each one a value.
///
/// Interning matters because the models compare operands every cycle and
/// against every unit: doing that on strings would make the inner loop hash a
/// name per comparison, where an index is a machine word.
#[derive(Clone, Debug, Default)]
pub struct RegisterTable {
    ids: HashMap<String, usize>,
    names: Vec<String>,
    /// Per register: the producer that will write it, if one is in flight.
    producer: Vec<Option<Producer>>,
}

impl RegisterTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// The index for `name`, assigning one the first time it is seen.
    pub fn intern(&mut self, name: &str) -> usize {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }
        let id = self.names.len();
        self.ids.insert(name.to_string(), id);
        self.names.push(name.to_string());
        self.producer.push(None);
        id
    }

    /// The index for `name`, or `None` when nothing has ever used it.
    pub fn get(&self, name: &str) -> Option<usize> {
        self.ids.get(name).copied()
    }

    pub fn name(&self, register: usize) -> &str {
        self.names.get(register).map_or("?", String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Who will write `register`, or `None` when the architectural value is
    /// current — which is the whole question an operand asks.
    pub fn producer(&self, register: usize) -> Option<Producer> {
        self.producer.get(register).copied().flatten()
    }

    /// Claim `register` for `producer`. Returns whoever held it before, which
    /// a renaming model keeps and a scoreboard treats as a hazard.
    pub fn claim(&mut self, register: usize, producer: Producer) -> Option<Producer> {
        let slot = self.producer.get_mut(register)?;
        slot.replace(producer)
    }

    /// Release `register`, but only if `producer` is still the one that owes
    /// it. A younger instruction may have claimed it in the meantime, and its
    /// claim is the one that counts.
    pub fn release(&mut self, register: usize, producer: Producer) {
        if let Some(slot) = self.producer.get_mut(register)
            && *slot == Some(producer)
        {
            *slot = None;
        }
    }

    /// Drop every outstanding claim, keeping the names. Used on a flush, where
    /// nothing in flight owes anything any more.
    pub fn clear_claims(&mut self) {
        self.producer.fill(None);
    }

    /// Every register with a claim on it, for a host drawing a rename table.
    pub fn outstanding(&self) -> impl Iterator<Item = (usize, Producer)> + '_ {
        self.producer
            .iter()
            .enumerate()
            .filter_map(|(register, held)| held.map(|producer| (register, producer)))
    }
}

/// An instruction's operands, resolved into the table's index space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Operands {
    /// Registers read. Any number: x86's `div` reads three.
    pub sources: Vec<usize>,
    /// Registers written. More than one is ordinary — a flags word, a stack
    /// pointer an addressing mode updates.
    pub destinations: Vec<usize>,
}

impl RegisterTable {
    /// Intern an op's operands, so the model can work in indices from here on.
    pub fn resolve<P>(&mut self, op: &super::super::PipelineOp<P>) -> Operands {
        Operands {
            sources: op.sources.iter().map(|name| self.intern(name)).collect(),
            destinations: op.writes.iter().map(|name| self.intern(name)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_keeps_one_index_however_often_it_is_seen() {
        let mut table = RegisterTable::new();
        let first = table.intern("rax");
        assert_eq!(table.intern("rax"), first);
        assert_ne!(table.intern("rflags"), first);
        assert_eq!(table.len(), 2);
        assert_eq!(table.name(first), "rax");
        assert_eq!(table.get("rbx"), None, "never seen, never invented");
    }

    #[test]
    fn a_younger_claim_wins_and_an_older_release_does_not_undo_it() {
        let mut table = RegisterTable::new();
        let a = table.intern("a");

        assert_eq!(table.claim(a, 1), None);
        assert_eq!(table.producer(a), Some(1));
        // A second writer renames over the first — the older one no longer
        // owes anybody this value.
        assert_eq!(table.claim(a, 2), Some(1));

        table.release(a, 1);
        assert_eq!(
            table.producer(a),
            Some(2),
            "the stale release must not free the younger claim"
        );
        table.release(a, 2);
        assert_eq!(table.producer(a), None);
    }

    #[test]
    fn a_flush_drops_claims_but_keeps_the_names() {
        let mut table = RegisterTable::new();
        let a = table.intern("a");
        table.claim(a, 7);
        assert_eq!(table.outstanding().count(), 1);

        table.clear_claims();
        assert_eq!(table.outstanding().count(), 0);
        assert_eq!(table.len(), 1);
        assert_eq!(table.get("a"), Some(a));
    }
}
