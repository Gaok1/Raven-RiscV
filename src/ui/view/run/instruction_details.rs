use crate::falcon;
use crate::ui::app::{EncFormat, InstrFieldKind, RunEditTarget, cpi_class_label, detect_format};
use raven_riscv_engine::capability::{BitRole, InstructionInfo};
use crate::ui::theme;
use crate::ui::view::style;
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use super::App;
use super::memory::exec_address_in_range;
use super::registers::reg_name;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};

// ── Public entry point ───────────────────────────────────────────────────────

pub(super) fn render_instruction_details(f: &mut Frame, area: Rect, app: &App) {
    app.run.details_field_hitboxes.borrow_mut().clear();
    if area.width < 4 || area.height < 4 {
        return;
    }
    let ctx = detail_context(app);
    app.run.details_rendered_addr.set(ctx.addr);

    // Split into 3 sections: header (3 lines + border), field map (4 lines + border), rest
    let header_h = 5u16;
    let map_h = 6u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Length(map_h),
            Constraint::Min(4),
        ])
        .split(area);

    render_header(f, chunks[0], &ctx, app);
    // The bit map comes from the backend's own decoder, so this pane shows a
    // real field map for whichever architecture is loaded.
    render_field_map(f, chunks[1], inspect_at(app, ctx.addr).as_ref(), app);
    render_decoded(
        f,
        chunks[2],
        ctx.word,
        ctx.format,
        &ctx.disasm,
        ctx.comment.as_deref(),
        Some(app.run.cpu()),
        app,
        &ctx,
    );
}

/// Decode the instruction at `addr` through the active backend's codec.
///
/// Reads enough bytes for the widest instruction any backend declares and lets
/// the codec decide how many it actually needs, so a variable-width ISA works
/// here without this pane knowing it is variable-width.
fn inspect_at(app: &App, addr: u32) -> Option<InstructionInfo> {
    let (code, memory) = (app.code()?, app.memory()?);
    let bytes = memory.peek(u64::from(addr), 8);
    code.inspect(u64::from(addr), &bytes)
}

/// The field of `addr` the inline editor is currently open on, if any.
/// `Word` stands for the full hex word (`RunEditTarget::Instr`).
fn editing_field(app: &App, addr: u32) -> Option<InstrFieldKind> {
    match app.run.run_edit {
        Some(RunEditTarget::Instr { addr: a }) if a == addr => Some(InstrFieldKind::Word),
        Some(RunEditTarget::InstrField { addr: a, field }) if a == addr => Some(field),
        _ => None,
    }
}

/// Record a clickable field span, clipped to the section's inner rect so a
/// partially hidden value never produces a hitbox past the border.
fn push_hitbox(app: &App, inner: Rect, field: InstrFieldKind, y: u16, x0: u16, len: usize) {
    if y >= inner.y + inner.height || y < inner.y {
        return;
    }
    let right = inner.x + inner.width;
    if x0 >= right {
        return;
    }
    let x1 = (x0 + len as u16).min(right);
    app.run
        .details_field_hitboxes
        .borrow_mut()
        .push((field, y, x0, x1));
}

/// The buffer + pseudo-cursor span of the open inline editor.
fn edit_buf_span(app: &App) -> Span<'static> {
    Span::styled(
        format!("{}█", app.run.run_edit_buf),
        Style::default().fg(theme::ACCENT).bold(),
    )
}

pub(super) fn disasm_word(word: u32) -> String {
    match falcon::decoder::decode(word) {
        Ok(instruction) => pretty_instr(&instruction),
        Err(_) => format!(".word 0x{word:08x}"),
    }
}

// ── Context ──────────────────────────────────────────────────────────────────

struct DetailContext {
    addr: u32,
    word: u32,
    disasm: String,
    origin: &'static str,
    format: EncFormat,
    comment: Option<String>,
    jump_target: Option<(bool, u32, Option<String>)>, // (taken, target_addr, label)
}

fn compute_jump_target(word: u32, addr: u32, app: &App) -> Option<(bool, u32, Option<String>)> {
    use crate::falcon::decoder::decode;
    use crate::falcon::instruction::Instruction::*;
    let cpu = app.run.cpu();
    let (taken, target) = match decode(word) {
        Ok(Beq { rs1, rs2, imm }) => (
            cpu.x[rs1 as usize] == cpu.x[rs2 as usize],
            addr.wrapping_add(imm as u32),
        ),
        Ok(Bne { rs1, rs2, imm }) => (
            cpu.x[rs1 as usize] != cpu.x[rs2 as usize],
            addr.wrapping_add(imm as u32),
        ),
        Ok(Blt { rs1, rs2, imm }) => (
            (cpu.x[rs1 as usize] as i32) < (cpu.x[rs2 as usize] as i32),
            addr.wrapping_add(imm as u32),
        ),
        Ok(Bge { rs1, rs2, imm }) => (
            (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32),
            addr.wrapping_add(imm as u32),
        ),
        Ok(Bltu { rs1, rs2, imm }) => (
            cpu.x[rs1 as usize] < cpu.x[rs2 as usize],
            addr.wrapping_add(imm as u32),
        ),
        Ok(Bgeu { rs1, rs2, imm }) => (
            cpu.x[rs1 as usize] >= cpu.x[rs2 as usize],
            addr.wrapping_add(imm as u32),
        ),
        Ok(Jal { imm, .. }) => (true, addr.wrapping_add(imm as u32)),
        Ok(Jalr { rs1, imm, .. }) => (true, cpu.x[rs1 as usize].wrapping_add(imm as u32) & !1),
        _ => return None,
    };
    let label = app.run.labels.get(&target).and_then(|v| v.first()).cloned();
    Some((taken, target, label))
}

fn detail_context(app: &App) -> DetailContext {
    let pc = app.program_counter() as u32;
    // A click-selected row pins the panel; otherwise it follows the PC.
    let selected = app
        .run
        .details_addr
        .and_then(|addr| app.run.mem().peek32(addr).ok().map(|word| (addr, word)));
    let (addr, word, origin) = if let Some((addr, word)) = selected {
        (addr, word, "selected")
    } else if exec_address_in_range(app, pc) {
        (pc, app.run.mem().peek32(pc).unwrap_or(0), "PC")
    } else {
        return DetailContext {
            addr: pc,
            word: 0,
            disasm: "<PC out of RAM>".into(),
            origin: "PC",
            format: detect_format(0),
            comment: None,
            jump_target: None,
        };
    };

    let comment = app.run.comments.get(&addr).cloned();
    let jump_target = compute_jump_target(word, addr, app);

    DetailContext {
        addr,
        word,
        // Through the active backend's disassembler — running RV32's decoder
        // over another ISA's bytes is how this pane used to show a plausible
        // but entirely wrong instruction.
        disasm: app
            .disassemble_at(u64::from(addr))
            .unwrap_or_else(|| format!("0x{word:08x}")),
        origin,
        format: detect_format(word),
        comment,
        jump_target,
    }
}

/// What to title the Instruction panel: the class the backend's own decoder
/// reports (`I-type` for RV32, `Load` for toy16), falling back to the raw
/// address when nothing decodes.
fn instruction_title(app: &App, ctx: &DetailContext) -> String {
    match inspect_at(app, ctx.addr) {
        Some(info) => format!("Instruction  [{}]", info.class),
        None => "Instruction".to_string(),
    }
}

// ── Section 1 : Header ───────────────────────────────────────────────────────

fn render_header(f: &mut Frame, area: Rect, ctx: &DetailContext, app: &App) {
    let title = instruction_title(app, ctx);
    let block = panel::panel_frame(PanelKind::Plain)
        .title(Span::styled(title, style::value()))
        .title_alignment(Alignment::Left);
    let inner = render_panel(f, area, block);

    let editing = editing_field(app, ctx.addr);
    let origin_span = Span::styled(
        format!(" @ 0x{:08x} ({})", ctx.addr, ctx.origin),
        style::label(),
    );

    // Line 0 — mnemonic, editable as one line of assembly. While editing,
    // the typed buffer replaces the disasm and a dim preview shows what it
    // assembles to before Enter commits it.
    let mut mnemonic_line = vec![Span::styled("▶ ", Style::default().fg(Color::Green))];
    if editing == Some(InstrFieldKind::Asm) {
        mnemonic_line.push(edit_buf_span(app));
        let preview = match falcon::asm::assemble(&app.run.run_edit_buf, ctx.addr) {
            Ok(prog) if prog.text.len() == 1 => disasm_word(prog.text[0]),
            _ => "?".to_string(),
        };
        mnemonic_line.push(Span::styled(
            format!(" → {preview}"),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        push_hitbox(
            app,
            inner,
            InstrFieldKind::Asm,
            inner.y,
            inner.x + 2,
            ctx.disasm.chars().count(),
        );
        mnemonic_line.push(Span::styled(
            ctx.disasm.clone(),
            Style::default().fg(Color::Yellow).bold(),
        ));
        mnemonic_line.push(origin_span);
    }

    // Line 1 — the word in hex (edits through the full-word editor) and in
    // binary (edits as a 32-bit binary value).
    let mut word_line = vec![Span::styled("  word  ", Style::default().fg(theme::LABEL))];
    match editing {
        Some(InstrFieldKind::Word) => {
            word_line.push(edit_buf_span(app));
            let preview = match crate::falcon::machine::parse::parse_cell(
                &app.run.run_edit_buf,
                crate::falcon::machine::types::MemWidth::B4,
                app.cell_format(),
                app.run.show_signed,
            ) {
                Ok(value) => disasm_word(value as u32),
                Err(_) => "?".to_string(),
            };
            word_line.push(Span::styled(
                format!(" → {preview}"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        Some(InstrFieldKind::Bin) => {
            word_line.push(edit_buf_span(app));
        }
        _ => {
            // Width comes from the backend, so an 8-bit instruction shows eight
            // bits rather than being padded out to RV32's word.
            let info = inspect_at(app, ctx.addr);
            let bits = info.as_ref().map_or(32, |info| info.encoding_bits);
            let encoding = info.as_ref().map_or(u64::from(ctx.word), |info| info.encoding);
            let hex_width = usize::from(bits).div_ceil(4);
            let bin_width = usize::from(bits);

            let word_x = inner.x + 8;
            push_hitbox(
                app,
                inner,
                InstrFieldKind::Word,
                inner.y + 1,
                word_x,
                hex_width + 2,
            );
            push_hitbox(
                app,
                inner,
                InstrFieldKind::Bin,
                inner.y + 1,
                word_x + hex_width as u16 + 5,
                bin_width,
            );
            word_line.push(Span::styled(
                format!("0x{encoding:0hex_width$x}"),
                Style::default().fg(theme::IMM_COLOR),
            ));
            word_line.push(Span::styled(
                format!("  ({encoding:0bin_width$b})"),
                Style::default().fg(Color::Rgb(80, 80, 100)),
            ));
        }
    }

    let mut lines = vec![Line::from(mnemonic_line), Line::from(word_line)];

    // The cycle estimate comes from RV32's CPI model, which is tied to its
    // opcodes; other backends show the class their own decoder reported rather
    // than a number that would be made up.
    if app.architecture_id() == raven_riscv_engine::architectures::riscv32::ID {
        let base_cycles = crate::ui::app::classify_cpi_for_display(
            ctx.word,
            ctx.addr,
            app.run.cpu(),
            &app.run.cpi_config,
            app.run.pipeline().enabled,
        );
        lines.push(Line::from(vec![
            Span::styled("  cycles  ", style::label()),
            Span::styled(
                format!("~{base_cycles}"),
                Style::default().fg(theme::CPI_PANEL).bold(),
            ),
            Span::styled(format!("  [{}]", cpi_class_label(ctx.word)), style::label()),
        ]));
    } else if let Some(info) = inspect_at(app, ctx.addr) {
        lines.push(Line::from(vec![
            Span::styled("  class   ", style::label()),
            Span::styled(info.class, Style::default().fg(theme::CPI_PANEL).bold()),
        ]));
    }

    if let Some(ref comment) = ctx.comment {
        lines.push(Line::from(vec![
            Span::styled("  comment  ", style::label()),
            Span::styled(
                comment.clone(),
                Style::default().fg(Color::Rgb(180, 220, 130)),
            ),
        ]));
    }

    if let Some((taken, target, ref label)) = ctx.jump_target {
        let label_part = label
            .as_deref()
            .map(|l| format!(" <{l}>"))
            .unwrap_or_default();
        let (arrow, color) = if taken {
            (
                format!("→ 0x{target:08x}{label_part}  (taken)"),
                theme::RUNNING,
            )
        } else {
            (
                format!("→ 0x{target:08x}{label_part}  (not taken)"),
                Color::Rgb(120, 120, 120),
            )
        };
        let exec_count = ctx
            .addr
            .checked_add(0)
            .and_then(|a| app.run.exec_counts.get(&a))
            .copied()
            .unwrap_or(0);
        lines.push(Line::from(vec![
            Span::styled("  target   ", style::label()),
            Span::styled(arrow, Style::default().fg(color)),
        ]));
        if exec_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  executions ", style::label()),
                Span::styled(
                    format!("×{exec_count}"),
                    style::metric(style::Metric::Cycles),
                ),
            ]));
        }
    } else if let Some(&count) = app.run.exec_counts.get(&ctx.addr) {
        if count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  executions ", style::label()),
                Span::styled(format!("×{count}"), style::metric(style::Metric::Cycles)),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Section 2 : Field map ────────────────────────────────────────────────────

/// One contiguous run of bits, ready to draw.
///
/// Built from the backend's declared layout — the label and width come from the
/// ISA, the colour from the role, so nothing here is RV32-shaped.
pub(super) struct Seg {
    label: String,
    width: u8,
    color: Color,
}

/// The bit map, drawn from whatever layout the backend declared.
///
/// Nothing here knows an instruction is 32 bits wide or that it has an `rs2`:
/// the segments, their widths and their names all come from the codec, so an
/// 8-bit SAP instruction renders through the same code as a 32-bit RV32 one.
fn render_field_map(f: &mut Frame, area: Rect, info: Option<&InstructionInfo>, app: &App) {
    let inner = render_panel(f, area, panel::panel("Field Map", PanelKind::Plain));
    let segs = info.map(layout_segments).unwrap_or_default();
    let Some(info) = info.filter(|_| !segs.is_empty()) else {
        f.render_widget(
            Paragraph::new("No bit layout for this instruction.").style(style::label()),
            inner,
        );
        return;
    };

    // Each segment's bits row doubles as a click target for editing that
    // field (the editor itself renders in the header/Decoded sections).
    let bits_y = inner.y + 2;
    let mut x = inner.x;
    for seg in &segs {
        let w = display_width(seg);
        if let Some(field) = seg_field(&seg.label) {
            push_hitbox(app, inner, field, bits_y, x, w);
        }
        x = x.saturating_add(w as u16 + 1);
    }

    let lines = vec![
        // Row 1 — bit position markers
        bit_position_line(&segs, info.encoding_bits),
        // Row 2 — colored label blocks (▮▮… label)
        label_line(&segs),
        // Row 3 — actual bit values
        bits_line(info.encoding, info.encoding_bits, &segs),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Colour each segment by its role. Which hue means "immediate" is a theme
/// decision, so it lives here — an ISA should not need an opinion about it to
/// get a field map.
fn layout_segments(info: &InstructionInfo) -> Vec<Seg> {
    if !info.layout_is_complete() {
        return Vec::new();
    }
    info.layout
        .iter()
        .map(|field| Seg {
            label: field.label.clone(),
            width: field.width,
            color: match field.role {
                BitRole::Opcode => Color::Cyan,
                BitRole::Destination => Color::LightGreen,
                BitRole::Source => Color::LightMagenta,
                BitRole::Immediate => Color::Blue,
                BitRole::Function => Color::Yellow,
                BitRole::Other => Color::DarkGray,
            },
        })
        .collect()
}

/// Map a field-map segment label to its editable field. All immediate pieces
/// (`imm[...]`, `i12`, `i10:5`, …) edit the one logical immediate.
fn seg_field(label: &str) -> Option<InstrFieldKind> {
    match label {
        "—" => None,
        "funct7" => Some(InstrFieldKind::Funct7),
        "rs2" => Some(InstrFieldKind::Rs2),
        "rs1" => Some(InstrFieldKind::Rs1),
        "fn3" => Some(InstrFieldKind::Funct3),
        "rd" => Some(InstrFieldKind::Rd),
        "opcode" => Some(InstrFieldKind::Opcode),
        l if l.starts_with("imm") || l.starts_with('i') => Some(InstrFieldKind::Imm),
        _ => None,
    }
}

fn bit_position_line(segs: &[Seg], encoding_bits: u8) -> Line<'static> {
    let mut spans = Vec::new();
    let mut bit = i32::from(encoding_bits) - 1;
    for seg in segs {
        let w = display_width(seg);
        let hi = bit;
        let lo = bit - seg.width as i32 + 1;
        let marker = if w == 1 {
            format!("{hi:<w$}", w = w)
        } else {
            format!("{hi:<w$}", w = (w / 2).max(1))
        };
        let padded = format!("{marker:<w$} ", w = w);
        spans.push(Span::styled(
            padded,
            Style::default().fg(Color::Rgb(80, 80, 100)),
        ));
        bit = lo - 1;
    }
    Line::from(spans)
}

fn label_line(segs: &[Seg]) -> Line<'static> {
    let n = segs.len();
    segs.iter()
        .enumerate()
        .map(|(i, s)| {
            let is_last = i + 1 == n;
            let w = display_width(s);
            let label_len = s.label.chars().count();

            // Name always comes first; ▮ blocks fill any leftover columns.
            let content = if label_len < w {
                let blocks = "▮".repeat(w - label_len);
                format!("{}{blocks}", s.label)
            } else {
                s.label.clone()
            };

            // Non-last segments get one trailing separator space for alignment
            let padded = if is_last {
                content
            } else {
                format!("{content} ")
            };
            Span::styled(padded, Style::default().fg(s.color))
        })
        .collect::<Vec<_>>()
        .into()
}

fn bits_line(encoding: u64, encoding_bits: u8, segs: &[Seg]) -> Line<'static> {
    let width = usize::from(encoding_bits);
    let bit_str = format!("{encoding:0width$b}");
    let mut spans = Vec::new();
    let mut idx = 0usize;
    for (i, seg) in segs.iter().enumerate() {
        let end = (idx + seg.width as usize).min(bit_str.len());
        let slice = &bit_str[idx.min(end)..end];
        let disp_w = display_width(seg);
        let padded = if i + 1 < segs.len() {
            format!("{slice:<w$} ", w = disp_w)
        } else {
            format!("{slice:<w$}", w = disp_w)
        };
        spans.push(Span::styled(padded, Style::default().fg(seg.color).bold()));
        idx = end;
    }
    Line::from(spans)
}

fn display_width(seg: &Seg) -> usize {
    (seg.width as usize).max(seg.label.chars().count())
}

// ── Section 3 : Decoded fields + description ─────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_decoded(
    f: &mut Frame,
    area: Rect,
    word: u32,
    format: EncFormat,
    disasm: &str,
    comment: Option<&str>,
    cpu: Option<&crate::falcon::Cpu>,
    app: &App,
    ctx: &DetailContext,
) {
    let inner = render_panel(f, area, panel::panel("Decoded", PanelKind::Plain));

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(c) = comment {
        // Truncated to one row: a wrapped comment would shift every kv row
        // below it and break the recorded hitbox positions.
        let max = (inner.width as usize).saturating_sub(3);
        let c_fit: String = c.chars().take(max).collect();
        lines.push(Line::from(vec![
            Span::styled("#! ", Style::default().fg(Color::Rgb(100, 200, 100))),
            Span::styled(c_fit, Style::default().fg(Color::Rgb(180, 220, 130))),
        ]));
        lines.push(Line::from(""));
    }
    // RV32's rows carry inline editing, which is bit-splicing this ISA's
    // encoding and so cannot be shared. Every other backend renders the fields
    // its own decoder reported — same panel, same layout, no editing.
    let hits = if app.architecture_id() == raven_riscv_engine::architectures::riscv32::ID {
        let mut rows = DecodedRows {
            lines: &mut lines,
            hits: Vec::new(),
            editing: editing_field(app, ctx.addr),
            buf: &app.run.run_edit_buf,
        };
        push_fields(&mut rows, word, format, cpu);
        let hits = rows.hits;
        lines.push(Line::from(""));
        push_description(&mut lines, word, format, disasm);
        hits
    } else {
        push_decoded_fields(&mut lines, app, ctx.addr);
        Vec::new()
    };

    for (field, idx, len) in hits {
        push_hitbox(app, inner, field, inner.y + idx as u16, inner.x + 10, len);
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

/// The decoded fields of the instruction at `addr`, exactly as the backend's
/// decoder named them, plus the mnemonic and raw encoding.
///
/// This is the generic counterpart to RV32's `push_fields`: no assumption that
/// there is an `rd`, an `imm`, or 32 bits to show.
fn push_decoded_fields(lines: &mut Vec<Line<'static>>, app: &App, addr: u32) {
    let Some(info) = inspect_at(app, addr) else {
        lines.push(Line::from(Span::styled(
            "These bytes do not decode to an instruction.",
            style::label(),
        )));
        return;
    };
    let hex_width = usize::from(info.encoding_bits).div_ceil(4);
    let bin_width = usize::from(info.encoding_bits);
    lines.push(kv("word", format!("0x{:0hex_width$X}", info.encoding), Color::White));
    lines.push(kv(
        "bits",
        format!("{:0bin_width$b}", info.encoding),
        Color::Rgb(150, 150, 180),
    ));
    lines.push(Line::from(""));
    for field in &info.fields {
        lines.push(kv_owned(field.name, field.value.clone(), Color::LightCyan));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("⟹  {}", info.mnemonic),
        Style::default().fg(Color::Rgb(0, 200, 140)),
    )));
}

/// [`kv`] for a key that is not `'static`.
fn kv_owned(key: &str, val: String, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<10}"), style::label()),
        Span::styled(val, Style::default().fg(val_color)),
    ])
}

fn kv(key: &'static str, val: String, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<10}"), style::label()),
        Span::styled(val, Style::default().fg(val_color)),
    ])
}

/// Collects the Decoded section's kv rows, remembering which row holds which
/// editable field (for click hitboxes) and substituting the open editor's
/// buffer into the row it is editing.
struct DecodedRows<'a> {
    lines: &'a mut Vec<Line<'static>>,
    /// `(field, line index, value length)` for every editable row pushed.
    hits: Vec<(InstrFieldKind, usize, usize)>,
    editing: Option<InstrFieldKind>,
    buf: &'a str,
}

impl DecodedRows<'_> {
    fn field(&mut self, field: InstrFieldKind, key: &'static str, val: String, color: Color) {
        if self.editing == Some(field) {
            self.lines.push(Line::from(vec![
                Span::styled(format!("{key:<10}"), Style::default().fg(theme::LABEL)),
                Span::styled(
                    format!("{}█", self.buf),
                    Style::default().fg(theme::ACCENT).bold(),
                ),
            ]));
        } else {
            self.hits.push((field, self.lines.len(), val.chars().count()));
            self.lines.push(kv(key, val, color));
        }
    }
    fn plain(&mut self, key: &'static str, val: String, color: Color) {
        self.lines.push(kv(key, val, color));
    }
    fn reg(&mut self, field: InstrFieldKind, key: &'static str, reg: u8) {
        self.field(
            field,
            key,
            format!("x{reg} ({})", reg_name(reg)),
            Color::LightGreen,
        );
    }
    fn imm(&mut self, field: InstrFieldKind, key: &'static str, v: i32) {
        self.field(field, key, format!("{v}  (0x{v:x})"), theme::IMM_COLOR);
    }
}

fn push_fields(
    rows: &mut DecodedRows<'_>,
    word: u32,
    format: EncFormat,
    cpu: Option<&crate::falcon::Cpu>,
) {
    use InstrFieldKind::*;
    let opcode = word & 0x7f;
    match format {
        EncFormat::R => {
            let funct7 = (word >> 25) & 0x7f;
            let rs2 = ((word >> 20) & 0x1f) as u8;
            let rs1 = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let rd = ((word >> 7) & 0x1f) as u8;
            rows.reg(Rd, "rd", rd);
            rows.reg(Rs1, "rs1", rs1);
            rows.reg(Rs2, "rs2", rs2);
            rows.field(Funct3, "funct3", format!("0x{funct3:01x}"), Color::Yellow);
            rows.field(Funct7, "funct7", format!("0x{funct7:02x}"), Color::Red);
        }
        EncFormat::I => {
            let imm = (((word >> 20) as i32) << 20) >> 20;
            let rs1 = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let rd = ((word >> 7) & 0x1f) as u8;
            rows.reg(Rd, "rd", rd);
            rows.reg(Rs1, "rs1", rs1);
            rows.imm(Imm, "imm", imm);
            rows.field(Funct3, "funct3", format!("0x{funct3:01x}"), Color::Yellow);
            if matches!(funct3, 0x1 | 0x5) {
                let shamt = (word >> 20) & 0x1f;
                let funct7 = (word >> 25) & 0x7f;
                rows.field(Shamt, "shamt", format!("{shamt}"), Color::LightRed);
                rows.field(Funct7, "funct7", format!("0x{funct7:02x}"), Color::Red);
            }
            // Feature 5: effective address for loads (opcode 0x03)
            if opcode == 0x03 {
                if let Some(cpu) = cpu {
                    let ea = cpu.x[rs1 as usize].wrapping_add(imm as u32);
                    rows.plain("\u{2192} addr", format!("0x{ea:08x}"), Color::Rgb(255, 180, 80));
                }
            }
        }
        EncFormat::S => {
            let imm_lo = (word >> 7) & 0x1f;
            let funct3 = (word >> 12) & 0x7;
            let rs1 = ((word >> 15) & 0x1f) as u8;
            let rs2 = ((word >> 20) & 0x1f) as u8;
            let imm_hi = (word >> 25) & 0x7f;
            let imm = (((((imm_hi << 5) | imm_lo) as i32) << 20) >> 20) as i32;
            rows.reg(Rs1, "rs1 (base)", rs1);
            rows.reg(Rs2, "rs2 (src)", rs2);
            rows.imm(Imm, "offset", imm);
            rows.field(Funct3, "funct3", format!("0x{funct3:01x}"), Color::Yellow);
            // Feature 5: effective address for stores
            if let Some(cpu) = cpu {
                let ea = cpu.x[rs1 as usize].wrapping_add(imm as u32);
                rows.plain("\u{2192} addr", format!("0x{ea:08x}"), Color::Rgb(255, 180, 80));
            }
        }
        EncFormat::B => {
            let b12 = (word >> 31) & 1;
            let b10_5 = (word >> 25) & 0x3f;
            let rs2 = ((word >> 20) & 0x1f) as u8;
            let rs1 = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let b4_1 = (word >> 8) & 0xf;
            let b11 = (word >> 7) & 1;
            let imm = (((((b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1)) as i32) << 19)
                >> 19) as i32;
            rows.reg(Rs1, "rs1", rs1);
            rows.reg(Rs2, "rs2", rs2);
            rows.imm(Imm, "offset", imm);
            rows.field(Funct3, "funct3", format!("0x{funct3:01x}"), Color::Yellow);
        }
        EncFormat::U => {
            let rd = ((word >> 7) & 0x1f) as u8;
            let imm = ((word & 0xfffff000) as i32) >> 12;
            rows.reg(Rd, "rd", rd);
            rows.imm(Imm, "imm[31:12]", imm);
        }
        EncFormat::J => {
            let b20 = (word >> 31) & 1;
            let b10_1 = (word >> 21) & 0x3ff;
            let b11 = (word >> 20) & 1;
            let b19_12 = (word >> 12) & 0xff;
            let rd = ((word >> 7) & 0x1f) as u8;
            let imm = (((((b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1)) as i32) << 11)
                >> 11) as i32;
            rows.reg(Rd, "rd", rd);
            rows.imm(Imm, "offset", imm);
        }
    }
    // The opcode selects the format itself; editing it morphs the whole row
    // layout above, which is exactly the didactic point.
    rows.field(Opcode, "opcode", format!("0x{opcode:02x}"), Color::Cyan);
}

fn push_description(lines: &mut Vec<Line<'static>>, word: u32, _format: EncFormat, disasm: &str) {
    let opcode = word & 0x7f;
    let funct3 = (word >> 12) & 0x7;
    let funct7 = (word >> 25) & 0x7f;

    let desc: &str = match opcode {
        0x33 => match (funct3, funct7) {
            (0x0, 0x00) => "rd ← rs1 + rs2",
            (0x0, 0x20) => "rd ← rs1 − rs2",
            (0x7, 0x00) => "rd ← rs1 & rs2",
            (0x6, 0x00) => "rd ← rs1 | rs2",
            (0x4, 0x00) => "rd ← rs1 ^ rs2",
            (0x1, 0x00) => "rd ← rs1 << (rs2 & 31)",
            (0x5, 0x00) => "rd ← rs1 >> (rs2 & 31)  [logical]",
            (0x5, 0x20) => "rd ← rs1 >> (rs2 & 31)  [arithmetic]",
            (0x2, 0x00) => "rd ← (rs1 < rs2) ? 1 : 0  [signed]",
            (0x3, 0x00) => "rd ← (rs1 < rs2) ? 1 : 0  [unsigned]",
            (0x0, 0x01) => "rd ← rs1 × rs2  [lower 32 bits]",
            (0x1, 0x01) => "rd ← (rs1 × rs2) >> 32  [signed×signed]",
            (0x2, 0x01) => "rd ← (rs1 × rs2) >> 32  [signed×unsigned]",
            (0x3, 0x01) => "rd ← (rs1 × rs2) >> 32  [unsigned×unsigned]",
            (0x4, 0x01) => "rd ← rs1 ÷ rs2  [signed]",
            (0x5, 0x01) => "rd ← rs1 ÷ rs2  [unsigned]",
            (0x6, 0x01) => "rd ← rs1 mod rs2  [signed]",
            (0x7, 0x01) => "rd ← rs1 mod rs2  [unsigned]",
            _ => "R-type ALU operation",
        },
        0x13 => match funct3 {
            0x0 => "rd ← rs1 + imm  (addi; imm=0 → nop/mv)",
            0x7 => "rd ← rs1 & imm",
            0x6 => "rd ← rs1 | imm",
            0x4 => "rd ← rs1 ^ imm",
            0x2 => "rd ← (rs1 < imm) ? 1 : 0  [signed]",
            0x3 => "rd ← (rs1 < imm) ? 1 : 0  [unsigned]",
            0x1 => "rd ← rs1 << shamt",
            0x5 if funct7 == 0 => "rd ← rs1 >> shamt  [logical]",
            0x5 => "rd ← rs1 >> shamt  [arithmetic]",
            _ => "I-type ALU immediate",
        },
        0x03 => match funct3 {
            0x0 => "rd ← sign_ext(MEM8[rs1+imm])",
            0x1 => "rd ← sign_ext(MEM16[rs1+imm])",
            0x2 => "rd ← MEM32[rs1+imm]",
            0x4 => "rd ← zero_ext(MEM8[rs1+imm])",
            0x5 => "rd ← zero_ext(MEM16[rs1+imm])",
            _ => "Load from memory",
        },
        0x23 => match funct3 {
            0x0 => "MEM8[rs1+imm]  ← rs2[7:0]",
            0x1 => "MEM16[rs1+imm] ← rs2[15:0]",
            0x2 => "MEM32[rs1+imm] ← rs2",
            _ => "Store to memory",
        },
        0x63 => match funct3 {
            0x0 => "if rs1 == rs2  → PC += offset",
            0x1 => "if rs1 != rs2  → PC += offset",
            0x4 => "if rs1 <  rs2  → PC += offset  [signed]",
            0x5 => "if rs1 >= rs2  → PC += offset  [signed]",
            0x6 => "if rs1 <  rs2  → PC += offset  [unsigned]",
            0x7 => "if rs1 >= rs2  → PC += offset  [unsigned]",
            _ => "Conditional branch",
        },
        0x37 => "rd ← imm << 12  (upper 20 bits immediate)",
        0x17 => "rd ← PC + (imm << 12)  (PC-relative upper imm)",
        0x6f => "rd ← PC+4 ;  PC += offset  (jump and link)",
        0x67 => "rd ← PC+4 ;  PC ← (rs1+imm) & ~1  (jump register)",
        0x73 => match word {
            0x00000073 => "Transfer control to execution environment (syscall)",
            0x00100073 => "Breakpoint (resumable debug stop)",
            0x00200073 => "Halt this hart permanently",
            _ => "System instruction",
        },
        _ => "",
    };

    if !desc.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("⟹  ", style::label()),
            Span::styled(desc.to_string(), style::value()),
        ]));
    } else if !disasm.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("⟹  ", style::label()),
            Span::styled(disasm.to_string(), style::label()),
        ]));
    }
}

// ── Disassembly pretty-printer ────────────────────────────────────────────────

fn pretty_instr(instruction: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;
    match *instruction {
        Add { rd, rs1, rs2 } => fmt3("add", rd, rs1, rs2),
        Sub { rd, rs1, rs2 } => fmt3("sub", rd, rs1, rs2),
        And { rd, rs1, rs2 } => fmt3("and", rd, rs1, rs2),
        Or { rd, rs1, rs2 } => fmt3("or", rd, rs1, rs2),
        Xor { rd, rs1, rs2 } => fmt3("xor", rd, rs1, rs2),
        Sll { rd, rs1, rs2 } => fmt3("sll", rd, rs1, rs2),
        Srl { rd, rs1, rs2 } => fmt3("srl", rd, rs1, rs2),
        Sra { rd, rs1, rs2 } => fmt3("sra", rd, rs1, rs2),
        Slt { rd, rs1, rs2 } => fmt3("slt", rd, rs1, rs2),
        Sltu { rd, rs1, rs2 } => fmt3("sltu", rd, rs1, rs2),
        Mul { rd, rs1, rs2 } => fmt3("mul", rd, rs1, rs2),
        Mulh { rd, rs1, rs2 } => fmt3("mulh", rd, rs1, rs2),
        Mulhsu { rd, rs1, rs2 } => fmt3("mulhsu", rd, rs1, rs2),
        Mulhu { rd, rs1, rs2 } => fmt3("mulhu", rd, rs1, rs2),
        Div { rd, rs1, rs2 } => fmt3("div", rd, rs1, rs2),
        Divu { rd, rs1, rs2 } => fmt3("divu", rd, rs1, rs2),
        Rem { rd, rs1, rs2 } => fmt3("rem", rd, rs1, rs2),
        Remu { rd, rs1, rs2 } => fmt3("remu", rd, rs1, rs2),
        Addi { rd, rs1, imm } => fmt_ri("addi", rd, rs1, imm),
        Andi { rd, rs1, imm } => fmt_ri("andi", rd, rs1, imm),
        Ori { rd, rs1, imm } => fmt_ri("ori", rd, rs1, imm),
        Xori { rd, rs1, imm } => fmt_ri("xori", rd, rs1, imm),
        Slti { rd, rs1, imm } => fmt_ri("slti", rd, rs1, imm),
        Sltiu { rd, rs1, imm } => fmt_ri("sltiu", rd, rs1, imm),
        Slli { rd, rs1, shamt } => fmt_ri("slli", rd, rs1, shamt as i32),
        Srli { rd, rs1, shamt } => fmt_ri("srli", rd, rs1, shamt as i32),
        Srai { rd, rs1, shamt } => fmt_ri("srai", rd, rs1, shamt as i32),
        Lb { rd, rs1, imm } => fmt_load("lb", rd, rs1, imm),
        Lh { rd, rs1, imm } => fmt_load("lh", rd, rs1, imm),
        Lw { rd, rs1, imm } => fmt_load("lw", rd, rs1, imm),
        Lbu { rd, rs1, imm } => fmt_load("lbu", rd, rs1, imm),
        Lhu { rd, rs1, imm } => fmt_load("lhu", rd, rs1, imm),
        Sb { rs2, rs1, imm } => fmt_store("sb", rs2, rs1, imm),
        Sh { rs2, rs1, imm } => fmt_store("sh", rs2, rs1, imm),
        Sw { rs2, rs1, imm } => fmt_store("sw", rs2, rs1, imm),
        Beq { rs1, rs2, imm } => fmt_br("beq", rs1, rs2, imm),
        Bne { rs1, rs2, imm } => fmt_br("bne", rs1, rs2, imm),
        Blt { rs1, rs2, imm } => fmt_br("blt", rs1, rs2, imm),
        Bge { rs1, rs2, imm } => fmt_br("bge", rs1, rs2, imm),
        Bltu { rs1, rs2, imm } => fmt_br("bltu", rs1, rs2, imm),
        Bgeu { rs1, rs2, imm } => fmt_br("bgeu", rs1, rs2, imm),
        Lui { rd, imm } => format!("{:<5} {}, 0x{:x}", "lui", reg_name(rd), (imm as u32) >> 12),
        Auipc { rd, imm } => format!(
            "{:<5} {}, 0x{:x}",
            "auipc",
            reg_name(rd),
            (imm as u32) >> 12
        ),
        Jal { rd, imm } => format!("{:<5} {}, {}", "jal", reg_name(rd), imm),
        Jalr { rd, rs1, imm } => fmt_ri("jalr", rd, rs1, imm),
        Ecall => "ecall".into(),
        Ebreak => "ebreak".into(),
        Halt => "halt".into(),
        Fence => "fence".into(),
        Csrrw { rd, rs1, csr } => format!("csrrw {}, 0x{csr:03x}, {}", reg_name(rd), reg_name(rs1)),
        Csrrs { rd, rs1, csr } => format!("csrrs {}, 0x{csr:03x}, {}", reg_name(rd), reg_name(rs1)),
        Csrrc { rd, rs1, csr } => format!("csrrc {}, 0x{csr:03x}, {}", reg_name(rd), reg_name(rs1)),
        Csrrwi { rd, uimm, csr } => format!("csrrwi {}, 0x{csr:03x}, {uimm}", reg_name(rd)),
        Csrrsi { rd, uimm, csr } => format!("csrrsi {}, 0x{csr:03x}, {uimm}", reg_name(rd)),
        Csrrci { rd, uimm, csr } => format!("csrrci {}, 0x{csr:03x}, {uimm}", reg_name(rd)),
        Mret => "mret".into(),
        Sret => "sret".into(),
        SfenceVma { rs1, rs2 } => {
            format!("sfence.vma {}, {}", reg_name(rs1), reg_name(rs2))
        }
        // RV32F
        Flw { rd, rs1, imm } => format!("{:<9} {}, {imm}({})", "flw", freg_name(rd), reg_name(rs1)),
        Fsw { rs2, rs1, imm } => {
            format!("{:<9} {}, {imm}({})", "fsw", freg_name(rs2), reg_name(rs1))
        }
        FaddS { rd, rs1, rs2 } => fmt3f("fadd.s", rd, rs1, rs2),
        FsubS { rd, rs1, rs2 } => fmt3f("fsub.s", rd, rs1, rs2),
        FmulS { rd, rs1, rs2 } => fmt3f("fmul.s", rd, rs1, rs2),
        FdivS { rd, rs1, rs2 } => fmt3f("fdiv.s", rd, rs1, rs2),
        FsqrtS { rd, rs1 } => format!("{:<9} {}, {}", "fsqrt.s", freg_name(rd), freg_name(rs1)),
        FminS { rd, rs1, rs2 } => fmt3f("fmin.s", rd, rs1, rs2),
        FmaxS { rd, rs1, rs2 } => fmt3f("fmax.s", rd, rs1, rs2),
        FsgnjS { rd, rs1, rs2 } => fmt3f("fsgnj.s", rd, rs1, rs2),
        FsgnjnS { rd, rs1, rs2 } => fmt3f("fsgnjn.s", rd, rs1, rs2),
        FsgnjxS { rd, rs1, rs2 } => fmt3f("fsgnjx.s", rd, rs1, rs2),
        FeqS { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, {}",
            "feq.s",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FltS { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, {}",
            "flt.s",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FleS { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, {}",
            "fle.s",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FcvtWS { rd, rs1, .. } => format!("{:<9} {}, {}", "fcvt.w.s", reg_name(rd), freg_name(rs1)),
        FcvtWuS { rd, rs1, .. } => {
            format!("{:<9} {}, {}", "fcvt.wu.s", reg_name(rd), freg_name(rs1))
        }
        FcvtSW { rd, rs1 } => format!("{:<9} {}, {}", "fcvt.s.w", freg_name(rd), reg_name(rs1)),
        FcvtSWu { rd, rs1 } => format!("{:<9} {}, {}", "fcvt.s.wu", freg_name(rd), reg_name(rs1)),
        FmvXW { rd, rs1 } => format!("{:<9} {}, {}", "fmv.x.w", reg_name(rd), freg_name(rs1)),
        FmvWX { rd, rs1 } => format!("{:<9} {}, {}", "fmv.w.x", freg_name(rd), reg_name(rs1)),
        FclassS { rd, rs1 } => format!("{:<9} {}, {}", "fclass.s", reg_name(rd), freg_name(rs1)),
        FmaddS { rd, rs1, rs2, rs3 } => fmt4f("fmadd.s", rd, rs1, rs2, rs3),
        FmsubS { rd, rs1, rs2, rs3 } => fmt4f("fmsub.s", rd, rs1, rs2, rs3),
        FnmsubS { rd, rs1, rs2, rs3 } => fmt4f("fnmsub.s", rd, rs1, rs2, rs3),
        FnmaddS { rd, rs1, rs2, rs3 } => fmt4f("fnmadd.s", rd, rs1, rs2, rs3),

        FenceI => "fence.i".into(),

        // RV32A
        LrW { rd, rs1, aq, rl } => format!(
            "{:<9} {}, ({})",
            atomic_mnemonic("lr.w", aq, rl),
            reg_name(rd),
            reg_name(rs1)
        ),
        ScW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("sc.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoswapW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amoswap.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoaddW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amoadd.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoxorW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amoxor.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoandW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amoand.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoorW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amoor.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmomaxW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amomax.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmominW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amomin.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmomaxuW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amomaxu.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmominuW {
            rd,
            rs1,
            rs2,
            aq,
            rl,
        } => format!(
            "{:<9} {}, {}, ({})",
            atomic_mnemonic("amominu.w", aq, rl),
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
    }
}

fn atomic_mnemonic(base: &str, aq: bool, rl: bool) -> String {
    match (aq, rl) {
        (true, true) => format!("{base}.aqrl"),
        (true, false) => format!("{base}.aq"),
        (false, true) => format!("{base}.rl"),
        (false, false) => base.to_string(),
    }
}

fn fmt3(m: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!(
        "{m:<5} {}, {}, {}",
        reg_name(rd),
        reg_name(rs1),
        reg_name(rs2)
    )
}
fn fmt3f(m: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!(
        "{m:<9} {}, {}, {}",
        freg_name(rd),
        freg_name(rs1),
        freg_name(rs2)
    )
}
fn fmt4f(m: &str, rd: u8, rs1: u8, rs2: u8, rs3: u8) -> String {
    format!(
        "{m:<9} {}, {}, {}, {}",
        freg_name(rd),
        freg_name(rs1),
        freg_name(rs2),
        freg_name(rs3)
    )
}
fn freg_name(i: u8) -> &'static str {
    match i {
        0 => "ft0",
        1 => "ft1",
        2 => "ft2",
        3 => "ft3",
        4 => "ft4",
        5 => "ft5",
        6 => "ft6",
        7 => "ft7",
        8 => "fs0",
        9 => "fs1",
        10 => "fa0",
        11 => "fa1",
        12 => "fa2",
        13 => "fa3",
        14 => "fa4",
        15 => "fa5",
        16 => "fa6",
        17 => "fa7",
        18 => "fs2",
        19 => "fs3",
        20 => "fs4",
        21 => "fs5",
        22 => "fs6",
        23 => "fs7",
        24 => "fs8",
        25 => "fs9",
        26 => "fs10",
        27 => "fs11",
        28 => "ft8",
        29 => "ft9",
        30 => "ft10",
        31 => "ft11",
        _ => "f?",
    }
}
fn fmt_ri(m: &str, rd: u8, rs1: u8, imm: i32) -> String {
    format!("{m:<5} {}, {}, {imm}", reg_name(rd), reg_name(rs1))
}
fn fmt_load(m: &str, rd: u8, rs1: u8, imm: i32) -> String {
    format!("{m:<5} {}, {imm}({})", reg_name(rd), reg_name(rs1))
}
fn fmt_store(m: &str, rs2: u8, rs1: u8, imm: i32) -> String {
    format!("{m:<5} {}, {imm}({})", reg_name(rs2), reg_name(rs1))
}
fn fmt_br(m: &str, rs1: u8, rs2: u8, imm: i32) -> String {
    format!("{m:<5} {}, {}, {imm}", reg_name(rs1), reg_name(rs2))
}
