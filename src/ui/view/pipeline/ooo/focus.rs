//! Turning the row under the cursor into the set of rows every table lights up.
//!
//! The renderer resolves the hovered [`OooFocus`] once per frame into a
//! [`Highlight`]: the instruction being pointed at, the entries it is waiting
//! on, and the entries waiting on it. Every table then asks [`Highlight::role`]
//! how to paint each row, the wire layer draws the same relationships as lines
//! between the panels, and the Gantt lights the same instruction's row — so one
//! hover answers "what is this blocked on, and who is blocked on it?" across
//! the whole screen at once.
//!
//! Rows are keyed by the tag the model reports: a reorder-buffer entry under
//! Tomasulo, a functional unit under the scoreboard. One key space serves both
//! because the two never share a frame.

use crate::ui::pipeline::{OooFocus, OooPanel};
use raven_engine::capability::PipelineDynamicInspect;

/// How a row relates to the focused instruction — decided once, applied by
/// every table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Role {
    /// Nothing is focused: everything renders at full contrast.
    Neutral,
    /// The row under the cursor.
    Primary,
    /// The focused row is waiting on this one.
    Producer,
    /// This row is waiting on the focused one.
    Consumer,
    /// Outside the focused chain.
    Dim,
}

impl Role {
    /// Whether the row keeps its own colours. A dimmed row drops to one muted
    /// grey so the chain is what the eye finds.
    pub(super) fn keeps_color(self) -> bool {
        self != Role::Dim
    }

    /// Whether this row is part of the chain a wire should be drawn to.
    pub(super) fn in_chain(self) -> bool {
        matches!(self, Role::Primary | Role::Producer | Role::Consumer)
    }
}

/// The focused dependency chain, resolved from a hover.
#[derive(Default)]
pub(super) struct Highlight {
    pub focus: Option<OooFocus>,
    /// Tag of the focused instruction, once resolved. `None` while pointing at
    /// something with no instruction behind it — a free station, a register
    /// nobody owes.
    pub primary: Option<u64>,
    /// Tags the focused instruction is waiting on.
    pub producers: Vec<u64>,
    /// Tags waiting on the focused instruction.
    pub consumers: Vec<u64>,
    /// The instruction the Gantt should light up, by address.
    pub address: Option<u64>,
}

impl Highlight {
    /// Whether a hover is active at all. With none, every row is neutral — the
    /// screen must not look dimmed when the mouse is somewhere else.
    pub(super) fn active(&self) -> bool {
        self.focus.is_some()
    }

    pub(super) fn role(&self, tag: Option<u64>) -> Role {
        if !self.active() {
            return Role::Neutral;
        }
        match tag {
            Some(tag) if self.primary == Some(tag) => Role::Primary,
            Some(tag) if self.producers.contains(&tag) => Role::Producer,
            Some(tag) if self.consumers.contains(&tag) => Role::Consumer,
            _ => Role::Dim,
        }
    }

    /// A queue row holds an instruction that has not issued, so nothing can be
    /// waiting on it yet and only the hovered one is ever lit.
    pub(super) fn queue_role(&self, row: usize) -> Role {
        match self.focus {
            None => Role::Neutral,
            Some(focus) if focus.panel == OooPanel::Queue && focus.row == row => Role::Primary,
            _ => Role::Dim,
        }
    }

    fn push(list: &mut Vec<u64>, tag: u64) {
        if !list.contains(&tag) {
            list.push(tag);
        }
    }
}

/// Resolve a hover against whichever model is running.
///
/// Producers come from the focused station's own operands — the tags it says it
/// is waiting for. Consumers come from every station that names the focused tag
/// in one of its operands. Both directions are read off the same field, which
/// is why one function serves both models.
pub(super) fn resolve(model: &dyn PipelineDynamicInspect, focus: Option<OooFocus>) -> Highlight {
    let mut highlight = Highlight {
        focus,
        ..Default::default()
    };
    let Some(focus) = focus else {
        return highlight;
    };

    // What is the cursor actually on?
    match focus.panel {
        OooPanel::Stations => {
            let Some(station) = model.station(focus.row) else {
                return highlight;
            };
            highlight.primary = station.tag;
            highlight.address = station.slot.map(|slot| slot.address);
        }
        OooPanel::Buffer => {
            let Some(entry) = model.buffer_entry(focus.row) else {
                return highlight;
            };
            highlight.primary = Some(entry.tag);
            highlight.address = Some(entry.slot.address);
        }
        OooPanel::Registers => {
            let Some(rename) = model.rename(focus.row) else {
                return highlight;
            };
            // A register points at whoever owes it, which is the instruction
            // worth lighting up — the register itself is only the way in.
            highlight.primary = Some(rename.producer);
        }
        OooPanel::Queue => {
            // Not issued: no tag, but it still owns a row in the history.
            highlight.address = model.queued(focus.row).map(|slot| slot.address);
            return highlight;
        }
    }

    let Some(primary) = highlight.primary else {
        return highlight;
    };
    if highlight.address.is_none() {
        highlight.address = address_of(model, primary);
    }

    for index in 0..model.station_count() {
        let Some(station) = model.station(index) else {
            continue;
        };
        let waits_for: Vec<u64> = station
            .operands
            .iter()
            .flatten()
            .filter_map(|operand| operand.producer)
            .collect();
        if station.tag == Some(primary) {
            for tag in waits_for {
                Highlight::push(&mut highlight.producers, tag);
            }
        } else if waits_for.contains(&primary)
            && let Some(tag) = station.tag
        {
            Highlight::push(&mut highlight.consumers, tag);
        }
    }
    highlight
}

/// Where the instruction carrying `tag` sits, so the Gantt can find its row.
fn address_of(model: &dyn PipelineDynamicInspect, tag: u64) -> Option<u64> {
    (0..model.buffer_count())
        .filter_map(|index| model.buffer_entry(index))
        .find(|entry| entry.tag == tag)
        .map(|entry| entry.slot.address)
        .or_else(|| {
            (0..model.station_count())
                .filter_map(|index| model.station(index))
                .find(|station| station.tag == Some(tag))
                .and_then(|station| station.slot)
                .map(|slot| slot.address)
        })
}
