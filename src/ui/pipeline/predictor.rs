use super::{BranchPredict, HazardType, InstrClass, PipeSlot, PipelineSimState, Stage, TraceKind};
use crate::falcon::instruction::Instruction;

const TWO_BIT_TABLE_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TwoBitEntry {
    valid: bool,
    tag: u32,
    counter: u8,
}

impl Default for TwoBitEntry {
    fn default() -> Self {
        Self {
            valid: false,
            tag: 0,
            counter: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Prediction {
    pub taken: bool,
    pub target: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredictorState {
    two_bit: [TwoBitEntry; TWO_BIT_TABLE_SIZE],
}

impl Default for PredictorState {
    fn default() -> Self {
        Self {
            two_bit: [TwoBitEntry::default(); TWO_BIT_TABLE_SIZE],
        }
    }
}

impl PredictorState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn two_bit_index(pc: u32) -> usize {
        ((pc >> 2) as usize) & (TWO_BIT_TABLE_SIZE - 1)
    }

    fn predict_two_bit(&self, pc: u32) -> bool {
        let entry = self.two_bit[Self::two_bit_index(pc)];
        if entry.valid && entry.tag == pc {
            entry.counter >= 2
        } else {
            false
        }
    }

    fn update_two_bit(&mut self, pc: u32, taken: bool) {
        let idx = Self::two_bit_index(pc);
        let entry = &mut self.two_bit[idx];
        if !entry.valid || entry.tag != pc {
            *entry = TwoBitEntry {
                valid: true,
                tag: pc,
                counter: 1,
            };
        }
        entry.counter = if taken {
            entry.counter.saturating_add(1).min(3)
        } else {
            entry.counter.saturating_sub(1)
        };
    }
}

fn conditional_branch_target(instr: Instruction, slot: &PipeSlot) -> Option<u32> {
    match instr {
        Instruction::Beq { imm, .. }
        | Instruction::Bne { imm, .. }
        | Instruction::Blt { imm, .. }
        | Instruction::Bge { imm, .. }
        | Instruction::Bltu { imm, .. }
        | Instruction::Bgeu { imm, .. } => Some(slot.pc.wrapping_add(imm as u32)),
        _ => None,
    }
}

fn is_conditional_branch(instr: Instruction) -> bool {
    matches!(
        instr,
        Instruction::Beq { .. }
            | Instruction::Bne { .. }
            | Instruction::Blt { .. }
            | Instruction::Bge { .. }
            | Instruction::Bltu { .. }
            | Instruction::Bgeu { .. }
    )
}

pub(super) fn predict_control(slot: &PipeSlot, state: &PipelineSimState) -> Option<Prediction> {
    let instr = slot.instr?;
    if let Some(target) = conditional_branch_target(instr, slot) {
        let taken = match state.predict {
            BranchPredict::NotTaken => false,
            BranchPredict::Taken => true,
            BranchPredict::Btfnt => target < slot.pc,
            BranchPredict::TwoBit => state.predictor.predict_two_bit(slot.pc),
        };
        return Some(Prediction {
            taken,
            target: if taken {
                target
            } else {
                slot.pc.wrapping_add(4)
            },
        });
    }

    match instr {
        Instruction::Jal { imm, .. } => Some(Prediction {
            taken: true,
            target: slot.pc.wrapping_add(imm as u32),
        }),
        // JALR depends on rs1, and ID-stage prediction only has architectural
        // register values plus WB->ID forwarding. Patterns like
        // `auipc ra, ...; jalr ..., ra, ...` are common in Rust/ELF code and
        // require EX/MEM forwarding before the target is trustworthy.
        //
        // Predicting JALR here can redirect fetch to a bogus address before EX
        // gets a chance to resolve the real target, so leave it unresolved
        // until the configured branch-resolve stage computes it with the usual
        // forwarding paths.
        Instruction::Jalr { .. } => None,
        _ => None,
    }
}

pub(super) fn apply_branch_prediction(state: &mut PipelineSimState) {
    let id_idx = Stage::ID as usize;
    let should_predict = match state.stages[id_idx].as_ref() {
        Some(slot)
            if !slot.is_bubble
                && matches!(slot.class, InstrClass::Branch | InstrClass::Jump)
                && slot.predicted_target.is_none() =>
        {
            true
        }
        _ => false,
    };
    if !should_predict {
        return;
    }

    let prediction = {
        let slot = state.stages[id_idx].as_ref().unwrap();
        match predict_control(slot, state) {
            Some(v) => v,
            None => return,
        }
    };

    if let Some(ref mut slot) = state.stages[id_idx] {
        slot.predicted_taken = prediction.taken;
        slot.predicted_target = Some(prediction.target);
    }

    if prediction.taken {
        state.fetch_pc = prediction.target;
        state.stages[Stage::IF as usize] = None;
    }
}

pub(super) fn update_predictor(state: &mut PipelineSimState, resolve_stage: usize) {
    let Some(slot) = state.stages[resolve_stage].as_ref() else {
        return;
    };
    let Some(instr) = slot.instr else {
        return;
    };
    if is_conditional_branch(instr) {
        state.predictor.update_two_bit(slot.pc, slot.branch_taken);
    }
}

pub(super) fn resolve_branch(state: &mut PipelineSimState, resolve_stage: usize) {
    let (actual_taken, actual_target, predicted_taken, predicted_target, detail) =
        match state.stages[resolve_stage].as_ref() {
            Some(s) if !s.is_bubble && matches!(s.class, InstrClass::Branch | InstrClass::Jump) => {
                let actual_taken = s.branch_taken;
                let actual_target = if actual_taken {
                    s.branch_target.unwrap_or(s.pc.wrapping_add(4))
                } else {
                    s.pc.wrapping_add(4)
                };
                let predicted_taken = s.predicted_taken;
                let predicted_target = s.predicted_target.unwrap_or(s.pc.wrapping_add(4));
                let detail = format!(
                    "{} flush",
                    s.disasm.split_whitespace().next().unwrap_or("?")
                );
                (
                    actual_taken,
                    actual_target,
                    predicted_taken,
                    predicted_target,
                    detail,
                )
            }
            _ => return,
        };

    update_predictor(state, resolve_stage);

    let mispredicted =
        actual_taken != predicted_taken || (actual_taken && actual_target != predicted_target);
    if !mispredicted {
        return;
    }

    state.flush_count += 1;
    for i in 0..resolve_stage {
        let should_flush = state.stages[i]
            .as_ref()
            .map(|s| !s.is_bubble)
            .unwrap_or(false);
        if should_flush {
            super::sim::push_trace(
                state,
                TraceKind::Hazard(HazardType::BranchFlush),
                resolve_stage,
                i,
                detail.clone(),
            );
        }
        if let Some(ref mut s) = state.stages[i] {
            if !s.is_bubble {
                s.is_bubble = true;
                s.hazard = Some(HazardType::BranchFlush);
            }
        }
    }
    state.fetch_pc = actual_target;
}
