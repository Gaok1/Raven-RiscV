use super::{HazardType, PipeSlot, PipelineBypassConfig, PipelineSimState, Stage, TraceKind};
use crate::falcon::instruction::Instruction;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BypassPath {
    ExToEx,
    MemToEx,
    WbToId,
    FuToId,
    StoreToLoad,
}

impl BypassPath {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ExToEx => "EX->EX",
            Self::MemToEx => "MEM->EX",
            Self::WbToId => "WB->ID",
            Self::FuToId => "FU->ID",
            Self::StoreToLoad => "Store->Load",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RegFile {
    Int,
    Float,
}

pub(super) fn operand_reg_file(instr: Instruction, operand: u8) -> Option<RegFile> {
    use Instruction::*;
    match instr {
        FaddS { .. }
        | FsubS { .. }
        | FmulS { .. }
        | FdivS { .. }
        | FminS { .. }
        | FmaxS { .. }
        | FsgnjS { .. }
        | FsgnjnS { .. }
        | FsgnjxS { .. }
        | FeqS { .. }
        | FltS { .. }
        | FleS { .. } => match operand {
            1 | 2 => Some(RegFile::Float),
            _ => None,
        },
        FmaddS { .. } | FmsubS { .. } | FnmsubS { .. } | FnmaddS { .. } => match operand {
            1 | 2 | 3 => Some(RegFile::Float),
            _ => None,
        },
        FsqrtS { .. } | FmvXW { .. } | FclassS { .. } | FcvtWS { .. } | FcvtWuS { .. } => {
            match operand {
                1 => Some(RegFile::Float),
                _ => None,
            }
        }
        FmvWX { .. } | FcvtSW { .. } | FcvtSWu { .. } | Flw { .. } => match operand {
            1 => Some(RegFile::Int),
            _ => None,
        },
        Fsw { .. } => match operand {
            1 => Some(RegFile::Int),
            2 => Some(RegFile::Float),
            _ => None,
        },
        _ => Some(RegFile::Int),
    }
}

pub(super) fn slot_result(slot: &PipeSlot) -> Option<(RegFile, u8, u32)> {
    use Instruction::*;
    let instr = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())?;
    match instr {
        Add { rd, .. }
        | Sub { rd, .. }
        | And { rd, .. }
        | Or { rd, .. }
        | Xor { rd, .. }
        | Sll { rd, .. }
        | Srl { rd, .. }
        | Sra { rd, .. }
        | Slt { rd, .. }
        | Sltu { rd, .. }
        | Mul { rd, .. }
        | Mulh { rd, .. }
        | Mulhsu { rd, .. }
        | Mulhu { rd, .. }
        | Div { rd, .. }
        | Divu { rd, .. }
        | Rem { rd, .. }
        | Remu { rd, .. }
        | Addi { rd, .. }
        | Andi { rd, .. }
        | Ori { rd, .. }
        | Xori { rd, .. }
        | Slti { rd, .. }
        | Sltiu { rd, .. }
        | Slli { rd, .. }
        | Srli { rd, .. }
        | Srai { rd, .. }
        | Lui { rd, .. }
        | Auipc { rd, .. }
        | Jal { rd, .. }
        | Jalr { rd, .. }
        | FeqS { rd, .. }
        | FltS { rd, .. }
        | FleS { rd, .. }
        | FcvtWS { rd, .. }
        | FcvtWuS { rd, .. }
        | FmvXW { rd, .. }
        | FclassS { rd, .. }
        | ScW { rd, .. }
        | AmoswapW { rd, .. }
        | AmoaddW { rd, .. }
        | AmoxorW { rd, .. }
        | AmoandW { rd, .. }
        | AmoorW { rd, .. }
        | AmomaxW { rd, .. }
        | AmominW { rd, .. }
        | AmomaxuW { rd, .. }
        | AmominuW { rd, .. } => Some((RegFile::Int, rd, slot.alu_result)),
        Lb { rd, .. }
        | Lh { rd, .. }
        | Lw { rd, .. }
        | Lbu { rd, .. }
        | Lhu { rd, .. }
        | LrW { rd, .. } => slot.mem_result.map(|v| (RegFile::Int, rd, v)),
        Flw { rd, .. }
        | FaddS { rd, .. }
        | FsubS { rd, .. }
        | FmulS { rd, .. }
        | FdivS { rd, .. }
        | FsqrtS { rd, .. }
        | FminS { rd, .. }
        | FmaxS { rd, .. }
        | FsgnjS { rd, .. }
        | FsgnjnS { rd, .. }
        | FsgnjxS { rd, .. }
        | FmaddS { rd, .. }
        | FmsubS { rd, .. }
        | FnmsubS { rd, .. }
        | FnmaddS { rd, .. }
        | FcvtSW { rd, .. }
        | FcvtSWu { rd, .. }
        | FmvWX { rd, .. } => Some((
            RegFile::Float,
            rd,
            if matches!(instr, Flw { .. }) {
                slot.mem_result?
            } else {
                slot.alu_result
            },
        )),
        _ => None,
    }
}

pub(super) fn slot_destination(slot: &PipeSlot) -> Option<(RegFile, u8)> {
    use Instruction::*;
    let instr = slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())?;
    match instr {
        Add { rd, .. }
        | Sub { rd, .. }
        | And { rd, .. }
        | Or { rd, .. }
        | Xor { rd, .. }
        | Sll { rd, .. }
        | Srl { rd, .. }
        | Sra { rd, .. }
        | Slt { rd, .. }
        | Sltu { rd, .. }
        | Mul { rd, .. }
        | Mulh { rd, .. }
        | Mulhsu { rd, .. }
        | Mulhu { rd, .. }
        | Div { rd, .. }
        | Divu { rd, .. }
        | Rem { rd, .. }
        | Remu { rd, .. }
        | Addi { rd, .. }
        | Andi { rd, .. }
        | Ori { rd, .. }
        | Xori { rd, .. }
        | Slti { rd, .. }
        | Sltiu { rd, .. }
        | Slli { rd, .. }
        | Srli { rd, .. }
        | Srai { rd, .. }
        | Lui { rd, .. }
        | Auipc { rd, .. }
        | Jal { rd, .. }
        | Jalr { rd, .. }
        | Lb { rd, .. }
        | Lh { rd, .. }
        | Lw { rd, .. }
        | Lbu { rd, .. }
        | Lhu { rd, .. }
        | LrW { rd, .. }
        | FeqS { rd, .. }
        | FltS { rd, .. }
        | FleS { rd, .. }
        | FcvtWS { rd, .. }
        | FcvtWuS { rd, .. }
        | FmvXW { rd, .. }
        | FclassS { rd, .. }
        | ScW { rd, .. }
        | AmoswapW { rd, .. }
        | AmoaddW { rd, .. }
        | AmoxorW { rd, .. }
        | AmoandW { rd, .. }
        | AmoorW { rd, .. }
        | AmomaxW { rd, .. }
        | AmominW { rd, .. }
        | AmomaxuW { rd, .. }
        | AmominuW { rd, .. } => Some((RegFile::Int, rd)),
        Flw { rd, .. }
        | FaddS { rd, .. }
        | FsubS { rd, .. }
        | FmulS { rd, .. }
        | FdivS { rd, .. }
        | FsqrtS { rd, .. }
        | FminS { rd, .. }
        | FmaxS { rd, .. }
        | FsgnjS { rd, .. }
        | FsgnjnS { rd, .. }
        | FsgnjxS { rd, .. }
        | FmaddS { rd, .. }
        | FmsubS { rd, .. }
        | FnmsubS { rd, .. }
        | FnmaddS { rd, .. }
        | FcvtSW { rd, .. }
        | FcvtSWu { rd, .. }
        | FmvWX { rd, .. } => Some((RegFile::Float, rd)),
        _ => None,
    }
}

fn forward_value(
    reg: Option<u8>,
    reg_file: Option<RegFile>,
    producers: &[PipeSlot],
) -> Option<u32> {
    let reg = reg?;
    let reg_file = reg_file?;
    if reg == 0 {
        return None;
    }
    producers
        .iter()
        .filter(|producer| !producer.is_bubble)
        .filter_map(|producer| {
            let (prod_file, prod_rd, value) = slot_result(producer)?;
            if prod_rd == reg && prod_file == reg_file {
                Some((producer.seq, value))
            } else {
                None
            }
        })
        .max_by_key(|(seq, _)| *seq)
        .map(|(_, value)| value)
}

pub(super) fn ready_fu_producers(state: &PipelineSimState) -> Vec<PipeSlot> {
    if !matches!(state.mode, super::PipelineMode::FunctionalUnits) {
        return Vec::new();
    }
    state
        .fu_bank
        .iter()
        .flat_map(|group| group.iter())
        .filter_map(|fu| fu.slot.as_ref())
        .filter(|slot| !slot.is_bubble && slot.fu_cycles_left <= 1 && slot_result(slot).is_some())
        .cloned()
        .collect()
}

pub(super) fn slot_reads_register(slot: &PipeSlot, reg_file: RegFile, reg: u8) -> bool {
    if slot.is_bubble || reg == 0 {
        return false;
    }
    let instr = match slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    {
        Some(instr) => instr,
        None => return false,
    };

    if matches!(instr, Instruction::Ecall) && reg_file == RegFile::Int && (10..=17).contains(&reg) {
        return true;
    }

    let rs1_match = slot.rs1 == Some(reg) && operand_reg_file(instr, 1) == Some(reg_file);
    let rs2_match = slot.rs2 == Some(reg) && operand_reg_file(instr, 2) == Some(reg_file);
    let rs3_match = match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. } => reg_file == RegFile::Float && rs3 == reg,
        _ => false,
    };

    rs1_match || rs2_match || rs3_match
}

pub(super) fn slot_reads_store_data_register(slot: &PipeSlot, reg_file: RegFile, reg: u8) -> bool {
    if slot.is_bubble || reg == 0 {
        return false;
    }
    let instr = match slot
        .instr
        .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    {
        Some(instr) => instr,
        None => return false,
    };

    matches!(
        instr,
        Instruction::Sb { .. }
            | Instruction::Sh { .. }
            | Instruction::Sw { .. }
            | Instruction::Fsw { .. }
            | Instruction::ScW { .. }
            | Instruction::AmoswapW { .. }
            | Instruction::AmoaddW { .. }
            | Instruction::AmoxorW { .. }
            | Instruction::AmoandW { .. }
            | Instruction::AmoorW { .. }
            | Instruction::AmomaxW { .. }
            | Instruction::AmominW { .. }
            | Instruction::AmomaxuW { .. }
            | Instruction::AmominuW { .. }
    ) && slot.rs2 == Some(reg)
        && operand_reg_file(instr, 2) == Some(reg_file)
}

pub(super) fn slot_has_late_mem_result(slot: &PipeSlot) -> bool {
    if slot.is_bubble {
        return false;
    }
    matches!(
        slot.instr,
        Some(
            Instruction::Lb { .. }
                | Instruction::Lh { .. }
                | Instruction::Lw { .. }
                | Instruction::Lbu { .. }
                | Instruction::Lhu { .. }
                | Instruction::Flw { .. }
                | Instruction::LrW { .. }
        )
    )
}

pub(super) fn slot_has_wb_only_syscall_result(slot: &PipeSlot) -> bool {
    !slot.is_bubble && matches!(slot.instr, Some(Instruction::Ecall))
}

pub(super) fn apply_forwarding_to_ex(
    slot: &mut PipeSlot,
    bypass: PipelineBypassConfig,
    ex_ready_prod: &Option<PipeSlot>,
    mem_ready_prod: &Option<PipeSlot>,
) {
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    let mut producers = Vec::new();
    if bypass.ex_to_ex {
        if let Some(prod) = ex_ready_prod.clone() {
            producers.push(prod);
        }
    }
    if bypass.mem_to_ex {
        if let Some(prod) = mem_ready_prod.clone() {
            producers.push(prod);
        }
    }
    if producers.is_empty() {
        return;
    }
    if let Some(v) = forward_value(slot.rs1, operand_reg_file(instr, 1), &producers) {
        slot.rs1_val = v;
    }
    if let Some(v) = forward_value(slot.rs2, operand_reg_file(instr, 2), &producers) {
        slot.rs2_val = v;
    }
    if matches!(
        instr,
        Instruction::FmaddS { .. }
            | Instruction::FmsubS { .. }
            | Instruction::FnmsubS { .. }
            | Instruction::FnmaddS { .. }
    ) {
        let rs3 = match instr {
            Instruction::FmaddS { rs3, .. }
            | Instruction::FmsubS { rs3, .. }
            | Instruction::FnmsubS { rs3, .. }
            | Instruction::FnmaddS { rs3, .. } => rs3,
            _ => 0,
        };
        if let Some(v) = forward_value(Some(rs3), operand_reg_file(instr, 3), &producers) {
            slot.mem_addr = Some(v);
        }
    }
}

pub(super) fn apply_forwarding_to_id(
    slot: &mut PipeSlot,
    bypass: PipelineBypassConfig,
    wb_prod: &Option<PipeSlot>,
    fu_producers: &[PipeSlot],
) {
    if !bypass.wb_to_id {
        return;
    }
    let instr = match slot.instr {
        Some(instr) => instr,
        None => return,
    };
    let mut producers = Vec::new();
    if let Some(prod) = wb_prod.clone() {
        producers.push(prod);
    }
    producers.extend_from_slice(fu_producers);
    if let Some(v) = forward_value(slot.rs1, operand_reg_file(instr, 1), &producers) {
        slot.rs1_val = v;
    }
    if let Some(v) = forward_value(slot.rs2, operand_reg_file(instr, 2), &producers) {
        slot.rs2_val = v;
    }
    match instr {
        Instruction::FmaddS { rs3, .. }
        | Instruction::FmsubS { rs3, .. }
        | Instruction::FnmsubS { rs3, .. }
        | Instruction::FnmaddS { rs3, .. } => {
            if let Some(v) = forward_value(Some(rs3), Some(RegFile::Float), &producers) {
                slot.mem_addr = Some(v);
            }
        }
        _ => {}
    }
}

pub(super) fn report_forward_hazards(state: &mut PipelineSimState) {
    let ready_fu_producers = ready_fu_producers(state);
    if state.bypass.wb_to_id {
        if let Some(id) = state.stages[Stage::ID as usize]
            .as_ref()
            .filter(|s| !s.is_bubble)
            .cloned()
        {
            emit_forward_trace(
                state,
                &id,
                Stage::ID as usize,
                Stage::WB as usize,
                BypassPath::WbToId,
            );
            for prod in &ready_fu_producers {
                emit_forward_trace_for_slot(
                    state,
                    &id,
                    Stage::ID as usize,
                    prod,
                    Stage::EX as usize,
                    BypassPath::FuToId,
                );
            }
        }
    }
    if state.bypass.ex_to_ex {
        if let Some(ex) = state.stages[Stage::EX as usize]
            .as_ref()
            .filter(|s| !s.is_bubble)
            .cloned()
        {
            emit_forward_trace(
                state,
                &ex,
                Stage::EX as usize,
                Stage::MEM as usize,
                BypassPath::ExToEx,
            );
        }
    }
    if state.bypass.mem_to_ex {
        if let Some(ex) = state.stages[Stage::EX as usize]
            .as_ref()
            .filter(|s| !s.is_bubble)
            .cloned()
        {
            emit_forward_trace(
                state,
                &ex,
                Stage::EX as usize,
                Stage::WB as usize,
                BypassPath::MemToEx,
            );
        }
    }
}

fn traceable_id_forward(
    consumer: &PipeSlot,
    consumer_stage: usize,
    prod_file: RegFile,
    p_rd: u8,
) -> bool {
    if consumer_stage == Stage::ID as usize
        && slot_reads_store_data_register(consumer, prod_file, p_rd)
    {
        return false;
    }
    true
}

fn emit_forward_trace(
    state: &mut PipelineSimState,
    consumer: &PipeSlot,
    consumer_stage: usize,
    prod_idx: usize,
    path: BypassPath,
) {
    let Some(prod) = state.stages[prod_idx].as_ref() else {
        return;
    };
    if prod.is_bubble {
        return;
    }
    let Some(p_rd) = prod.rd else {
        return;
    };
    if p_rd == 0 {
        return;
    }
    let Some((prod_file, _, _)) = slot_result(prod) else {
        return;
    };
    if !slot_reads_register(consumer, prod_file, p_rd) {
        return;
    }
    if !traceable_id_forward(consumer, consumer_stage, prod_file, p_rd) {
        return;
    }
    let prod_name = prod.disasm.split_whitespace().next().unwrap_or("?");
    let consumer_name = consumer.disasm.split_whitespace().next().unwrap_or("?");
    let detail = format!(
        "{}:{} -> {}:{} ({}, {})",
        Stage::all()[prod_idx].label(),
        prod_name,
        Stage::all()[consumer_stage].label(),
        consumer_name,
        super::sim::reg_name(p_rd),
        path.label(),
    );
    super::sim::push_trace(state, TraceKind::Forward, prod_idx, consumer_stage, detail);
    state.hazard_msgs.push((
        HazardType::Raw,
        format!(
            "BYPASS: {} via {} into {}:{} [RAW covered]",
            super::sim::reg_name(p_rd),
            path.label(),
            Stage::all()[consumer_stage].label(),
            consumer_name,
        ),
    ));
}

fn emit_forward_trace_for_slot(
    state: &mut PipelineSimState,
    consumer: &PipeSlot,
    consumer_stage: usize,
    prod: &PipeSlot,
    prod_idx: usize,
    path: BypassPath,
) {
    if prod.is_bubble {
        return;
    }
    let Some(p_rd) = prod.rd else {
        return;
    };
    if p_rd == 0 {
        return;
    }
    let Some((prod_file, _, _)) = slot_result(prod) else {
        return;
    };
    if !slot_reads_register(consumer, prod_file, p_rd) {
        return;
    }
    if !traceable_id_forward(consumer, consumer_stage, prod_file, p_rd) {
        return;
    }
    let prod_name = prod.disasm.split_whitespace().next().unwrap_or("?");
    let consumer_name = consumer.disasm.split_whitespace().next().unwrap_or("?");
    let detail = format!(
        "{}:{} -> {}:{} ({}, {})",
        Stage::all()[prod_idx].label(),
        prod_name,
        Stage::all()[consumer_stage].label(),
        consumer_name,
        super::sim::reg_name(p_rd),
        path.label(),
    );
    super::sim::push_trace(state, TraceKind::Forward, prod_idx, consumer_stage, detail);
    state.hazard_msgs.push((
        HazardType::Raw,
        format!(
            "BYPASS: {} via {} into {}:{} [RAW covered]",
            super::sim::reg_name(p_rd),
            path.label(),
            Stage::all()[consumer_stage].label(),
            consumer_name,
        ),
    ));
}

pub(super) fn try_store_to_load_forward(
    slot: &PipeSlot,
    bypass: PipelineBypassConfig,
    store_prod: &Option<PipeSlot>,
) -> Option<u32> {
    if !bypass.store_to_load {
        return None;
    }
    let Some(prod) = store_prod.as_ref() else {
        return None;
    };
    if prod.is_bubble {
        return None;
    }

    let load_addr = slot.mem_addr?;
    let store_addr = prod.mem_addr?;
    let (load_size, signed) = match slot.instr? {
        Instruction::Lb { .. } => (1u32, true),
        Instruction::Lbu { .. } => (1u32, false),
        Instruction::Lh { .. } => (2u32, true),
        Instruction::Lhu { .. } => (2u32, false),
        Instruction::Lw { .. } => (4u32, false),
        Instruction::Flw { .. } => (4u32, false),
        _ => return None,
    };
    let store_size = match prod.instr? {
        Instruction::Sb { .. } => 1u32,
        Instruction::Sh { .. } => 2u32,
        Instruction::Sw { .. } | Instruction::Fsw { .. } => 4u32,
        _ => return None,
    };
    let load_end = load_addr.checked_add(load_size)?;
    let store_end = store_addr.checked_add(store_size)?;
    if load_addr < store_addr || load_end > store_end {
        return None;
    }

    let shift = ((load_addr - store_addr) * 8) as u32;
    let mask = match load_size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => u32::MAX,
        _ => return None,
    };
    let raw = (prod.rs2_val >> shift) & mask;
    let forwarded = match (load_size, signed) {
        (1, true) => (raw as u8 as i8 as i32) as u32,
        (2, true) => (raw as u16 as i16 as i32) as u32,
        _ => raw,
    };
    Some(forwarded)
}
