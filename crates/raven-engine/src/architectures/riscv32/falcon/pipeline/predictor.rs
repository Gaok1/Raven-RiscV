#![allow(clippy::collapsible_if, clippy::match_like_matches_macro)]

use super::{HazardType, InstrClass, PipeSlot, PipelineSimState, Stage, TraceKind};
use crate::falcon::instruction::Instruction;

/// The saturating-counter table is the same hardware on every machine, so RV32
/// borrows it and keeps only the part that needs a decoder: recognising a branch
/// and working out where it goes.
pub type PredictorState = crate::pipeline::TwoBitPredictor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Prediction {
    pub taken: bool,
    pub target: u32,
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
        let taken =
            state
                .predictor
                .direction(state.predict, u64::from(slot.pc), u64::from(target));
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
        state
            .predictor
            .update(u64::from(slot.pc), slot.branch_taken);
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
    state.stall_by_type[HazardType::BranchFlush.as_stall_index().unwrap()] = state.stall_by_type
        [HazardType::BranchFlush.as_stall_index().unwrap()]
    .saturating_add(state.branch_resolve.flush_depth() as u64);
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
    let mut flushed_fu_slots = 0usize;
    for group in &mut state.fu_bank {
        for fu in group.iter_mut() {
            let should_flush = fu
                .slot
                .as_ref()
                .map(|s| !s.is_bubble && s.is_speculative)
                .unwrap_or(false);
            if should_flush {
                fu.slot = None;
                fu.busy_cycles_left = 0;
                flushed_fu_slots += 1;
            }
        }
        group.retain(|fu| fu.slot.is_some());
    }
    for _ in 0..flushed_fu_slots {
        super::sim::push_trace(
            state,
            TraceKind::Hazard(HazardType::BranchFlush),
            resolve_stage,
            Stage::EX as usize,
            detail.clone(),
        );
    }
    state.fetch_pc = actual_target;
}
