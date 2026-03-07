use crate::falcon;
use crate::ui::app::cpi_class_label;
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use super::App;
use super::memory::imem_address_in_range;
use super::registers::reg_name;

// ── Public entry point ───────────────────────────────────────────────────────

pub(super) fn render_instruction_details(f: &mut Frame, area: Rect, app: &App) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let ctx = detail_context(app);

    // Split into 3 sections: header (3 lines + border), field map (4 lines + border), rest
    let header_h = 5u16;
    let map_h    = 6u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Length(map_h),
            Constraint::Min(4),
        ])
        .split(area);

    render_header(f, chunks[0], &ctx, app);
    render_field_map(f, chunks[1], ctx.word, ctx.format);
    render_decoded(f, chunks[2], ctx.word, ctx.format, &ctx.disasm, ctx.comment.as_deref(), Some(&app.run.cpu));
}

pub(super) fn disasm_word(word: u32) -> String {
    match falcon::decoder::decode(word) {
        Ok(instruction) => pretty_instr(&instruction),
        Err(error) => format!("<decode error: {error}>"),
    }
}

// ── Context ──────────────────────────────────────────────────────────────────

struct DetailContext {
    addr:           u32,
    word:           u32,
    disasm:         String,
    origin:         &'static str,
    format:         EncFormat,
    comment:        Option<String>,
    jump_target:    Option<(bool, u32, Option<String>)>, // (taken, target_addr, label)
}

fn compute_jump_target(word: u32, addr: u32, app: &App) -> Option<(bool, u32, Option<String>)> {
    use crate::falcon::instruction::Instruction::*;
    use crate::falcon::decoder::decode;
    let cpu = &app.run.cpu;
    let (taken, target) = match decode(word) {
        Ok(Beq  { rs1, rs2, imm }) => (cpu.x[rs1 as usize] == cpu.x[rs2 as usize], addr.wrapping_add(imm as u32)),
        Ok(Bne  { rs1, rs2, imm }) => (cpu.x[rs1 as usize] != cpu.x[rs2 as usize], addr.wrapping_add(imm as u32)),
        Ok(Blt  { rs1, rs2, imm }) => ((cpu.x[rs1 as usize] as i32) <  (cpu.x[rs2 as usize] as i32), addr.wrapping_add(imm as u32)),
        Ok(Bge  { rs1, rs2, imm }) => ((cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32), addr.wrapping_add(imm as u32)),
        Ok(Bltu { rs1, rs2, imm }) => (cpu.x[rs1 as usize] <  cpu.x[rs2 as usize], addr.wrapping_add(imm as u32)),
        Ok(Bgeu { rs1, rs2, imm }) => (cpu.x[rs1 as usize] >= cpu.x[rs2 as usize], addr.wrapping_add(imm as u32)),
        Ok(Jal  { imm, .. })       => (true, addr.wrapping_add(imm as u32)),
        Ok(Jalr { rs1, imm, .. })  => (true, cpu.x[rs1 as usize].wrapping_add(imm as u32) & !1),
        _ => return None,
    };
    let label = app.run.labels.get(&target).and_then(|v| v.first()).cloned();
    Some((taken, target, label))
}

fn detail_context(app: &App) -> DetailContext {
    let (addr, word, origin) = if let Some(addr) = app.run.hover_imem_addr {
        let word = app.run.mem.peek32(addr).unwrap_or(0);
        (addr, word, "hover")
    } else if imem_address_in_range(app, app.run.cpu.pc) {
        let word = app.run.mem.peek32(app.run.cpu.pc).unwrap_or(0);
        (app.run.cpu.pc, word, "PC")
    } else {
        return DetailContext {
            addr: app.run.cpu.pc, word: 0, disasm: "<PC out of RAM>".into(),
            origin: "PC", format: detect_format(0), comment: None,
            jump_target: None,
        };
    };

    let comment = app.run.comments.get(&addr).cloned();
    let jump_target = compute_jump_target(word, addr, app);

    DetailContext {
        addr, word, disasm: disasm_word(word), origin,
        format: detect_format(word), comment, jump_target,
    }
}

// ── Section 1 : Header ───────────────────────────────────────────────────────

fn render_header(f: &mut Frame, area: Rect, ctx: &DetailContext, app: &App) {
    let fmt_name = ctx.format.name();
    let title = format!("Instruction  [{fmt_name}]");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(Color::White)))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let origin_span = Span::styled(
        format!(" @ 0x{:08x} ({})", ctx.addr, ctx.origin),
        Style::default().fg(Color::DarkGray),
    );
    let word_span = Span::styled(
        format!("0x{:08x}", ctx.word),
        Style::default().fg(Color::Rgb(120, 180, 255)),
    );
    let disasm_span = Span::styled(
        ctx.disasm.clone(),
        Style::default().fg(Color::Yellow).bold(),
    );

    // Compute base CPI cycles for current instruction
    let cpi = &app.run.cpi_config;
    let base_cycles = crate::ui::app::classify_cpi_for_display(ctx.word, ctx.addr, &app.run.cpu, cpi);
    let class_label = cpi_class_label(ctx.word);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("▶ ", Style::default().fg(Color::Green)),
            disasm_span,
            origin_span,
        ]),
        Line::from(vec![
            Span::styled("  word  ", Style::default().fg(Color::DarkGray)),
            word_span,
            Span::styled(
                format!("  ({:032b})", ctx.word),
                Style::default().fg(Color::Rgb(80, 80, 100)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  cycles  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("~{base_cycles}"), Style::default().fg(Color::Rgb(100, 220, 180)).bold()),
            Span::styled(format!("  [{class_label}]"), Style::default().fg(Color::DarkGray)),
        ]),
    ];

    if let Some(ref comment) = ctx.comment {
        lines.push(Line::from(vec![
            Span::styled("  comment  ", Style::default().fg(Color::DarkGray)),
            Span::styled(comment.clone(), Style::default().fg(Color::Rgb(180, 220, 130))),
        ]));
    }

    if let Some((taken, target, ref label)) = ctx.jump_target {
        let label_part = label.as_deref().map(|l| format!(" <{l}>")).unwrap_or_default();
        let (arrow, color) = if taken {
            (format!("→ 0x{target:08x}{label_part}  (taken)"), Color::Rgb(0, 210, 100))
        } else {
            (format!("→ 0x{target:08x}{label_part}  (not taken)"), Color::Rgb(120, 120, 120))
        };
        let exec_count = ctx.addr.checked_add(0).and_then(|a| app.run.exec_counts.get(&a)).copied().unwrap_or(0);
        lines.push(Line::from(vec![
            Span::styled("  target   ", Style::default().fg(Color::DarkGray)),
            Span::styled(arrow, Style::default().fg(color)),
        ]));
        if exec_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  executions ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("×{exec_count}"), Style::default().fg(Color::Cyan)),
            ]));
        }
    } else if let Some(&count) = app.run.exec_counts.get(&ctx.addr) {
        if count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  executions ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("×{count}"), Style::default().fg(Color::Cyan)),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Section 2 : Field map ────────────────────────────────────────────────────

fn render_field_map(f: &mut Frame, area: Rect, word: u32, format: EncFormat) {
    let segs = format.segments();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("Field Map", Style::default().fg(Color::DarkGray)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Row 1 — bit position markers
    let pos_line = bit_position_line(&segs);
    // Row 2 — colored label blocks (▮▮… label)
    let label_line = label_line(&segs);
    // Row 3 — actual bit values
    let bits_line = bits_line(word, &segs);

    let lines = vec![pos_line, label_line, bits_line];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn bit_position_line(segs: &[Seg]) -> Line<'static> {
    let mut spans = Vec::new();
    let mut bit = 31i32;
    for seg in segs {
        let w = seg.width as usize;
        let hi = bit;
        let lo = bit - w as i32 + 1;
        let marker = if w == 1 {
            format!("{hi:<w$}", w = w)
        } else {
            format!("{hi:<w$}", w = (w / 2).max(1))
        };
        let padded = format!("{marker:<w$} ", w = w);
        spans.push(Span::styled(padded, Style::default().fg(Color::Rgb(80, 80, 100))));
        bit = lo - 1;
    }
    Line::from(spans)
}

fn label_line(segs: &[Seg]) -> Line<'static> {
    let n = segs.len();
    segs.iter().enumerate().map(|(i, s)| {
        let is_last = i + 1 == n;
        let w = s.width as usize; // display columns for this field's content
        let label_len = s.label.chars().count();

        // Name always comes first; ▮ blocks fill any leftover columns
        let content: String = if label_len <= w {
            let blocks = "▮".repeat(w - label_len);
            format!("{}{blocks}", s.label)
        } else {
            // Truncate label to fit exactly w columns
            s.label.chars().take(w).collect()
        };

        // Non-last segments get one trailing separator space for alignment
        let padded = if is_last { content } else { format!("{content} ") };
        Span::styled(padded, Style::default().fg(s.color))
    }).collect::<Vec<_>>().into()
}

fn bits_line(word: u32, segs: &[Seg]) -> Line<'static> {
    let bit_str = format!("{word:032b}");
    let mut spans = Vec::new();
    let mut idx = 0usize;
    for (i, seg) in segs.iter().enumerate() {
        let end = idx + seg.width as usize;
        let slice = &bit_str[idx..end];
        let padded = if i + 1 < segs.len() {
            format!("{slice:<w$} ", w = seg.width as usize)
        } else {
            slice.to_string()
        };
        spans.push(Span::styled(padded, Style::default().fg(seg.color).bold()));
        idx = end;
    }
    Line::from(spans)
}

// ── Section 3 : Decoded fields + description ─────────────────────────────────

fn render_decoded(f: &mut Frame, area: Rect, word: u32, format: EncFormat, disasm: &str, comment: Option<&str>, cpu: Option<&crate::falcon::Cpu>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("Decoded", Style::default().fg(Color::DarkGray)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(c) = comment {
        lines.push(Line::from(vec![
            Span::styled("#! ", Style::default().fg(Color::Rgb(100, 200, 100))),
            Span::styled(c.to_string(), Style::default().fg(Color::Rgb(180, 220, 130))),
        ]));
        lines.push(Line::from(""));
    }
    push_fields(&mut lines, word, format, cpu);
    // blank separator
    lines.push(Line::from(""));
    // Semantic description
    push_description(&mut lines, word, format, disasm);

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn kv(key: &'static str, val: String, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<10}"), Style::default().fg(Color::DarkGray)),
        Span::styled(val, Style::default().fg(val_color)),
    ])
}

fn reg_kv(key: &'static str, reg: u8) -> Line<'static> {
    kv(key, format!("x{reg} ({})", reg_name(reg)), Color::LightGreen)
}

fn imm_kv(key: &'static str, v: i32) -> Line<'static> {
    kv(key, format!("{v}  (0x{v:x})"), Color::Rgb(120, 180, 255))
}

fn push_fields(lines: &mut Vec<Line<'static>>, word: u32, format: EncFormat, cpu: Option<&crate::falcon::Cpu>) {
    let opcode = word & 0x7f;
    match format {
        EncFormat::R => {
            let funct7 = (word >> 25) & 0x7f;
            let rs2    = ((word >> 20) & 0x1f) as u8;
            let rs1    = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let rd     = ((word >> 7) & 0x1f) as u8;
            lines.push(reg_kv("rd", rd));
            lines.push(reg_kv("rs1", rs1));
            lines.push(reg_kv("rs2", rs2));
            lines.push(kv("funct3", format!("0x{funct3:01x}"), Color::Yellow));
            lines.push(kv("funct7", format!("0x{funct7:02x}"), Color::Red));
        }
        EncFormat::I => {
            let imm    = (((word >> 20) as i32) << 20) >> 20;
            let rs1    = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let rd     = ((word >> 7) & 0x1f) as u8;
            lines.push(reg_kv("rd", rd));
            lines.push(reg_kv("rs1", rs1));
            lines.push(imm_kv("imm", imm));
            lines.push(kv("funct3", format!("0x{funct3:01x}"), Color::Yellow));
            if matches!(funct3, 0x1 | 0x5) {
                let shamt  = (word >> 20) & 0x1f;
                let funct7 = (word >> 25) & 0x7f;
                lines.push(kv("shamt", format!("{shamt}"), Color::LightRed));
                lines.push(kv("funct7", format!("0x{funct7:02x}"), Color::Red));
            }
            // Feature 5: effective address for loads (opcode 0x03)
            if opcode == 0x03 {
                if let Some(cpu) = cpu {
                    let ea = cpu.x[rs1 as usize].wrapping_add(imm as u32);
                    lines.push(kv("\u{2192} addr", format!("0x{ea:08x}"), Color::Rgb(255, 180, 80)));
                }
            }
        }
        EncFormat::S => {
            let imm_lo = (word >> 7) & 0x1f;
            let funct3 = (word >> 12) & 0x7;
            let rs1    = ((word >> 15) & 0x1f) as u8;
            let rs2    = ((word >> 20) & 0x1f) as u8;
            let imm_hi = (word >> 25) & 0x7f;
            let imm    = (((((imm_hi << 5) | imm_lo) as i32) << 20) >> 20) as i32;
            lines.push(reg_kv("rs1 (base)", rs1));
            lines.push(reg_kv("rs2 (src)", rs2));
            lines.push(imm_kv("offset", imm));
            lines.push(kv("funct3", format!("0x{funct3:01x}"), Color::Yellow));
            // Feature 5: effective address for stores
            if let Some(cpu) = cpu {
                let ea = cpu.x[rs1 as usize].wrapping_add(imm as u32);
                lines.push(kv("\u{2192} addr", format!("0x{ea:08x}"), Color::Rgb(255, 180, 80)));
            }
        }
        EncFormat::B => {
            let b12    = (word >> 31) & 1;
            let b10_5  = (word >> 25) & 0x3f;
            let rs2    = ((word >> 20) & 0x1f) as u8;
            let rs1    = ((word >> 15) & 0x1f) as u8;
            let funct3 = (word >> 12) & 0x7;
            let b4_1   = (word >> 8) & 0xf;
            let b11    = (word >> 7) & 1;
            let imm    = (((((b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1)) as i32) << 19) >> 19) as i32;
            lines.push(reg_kv("rs1", rs1));
            lines.push(reg_kv("rs2", rs2));
            lines.push(imm_kv("offset", imm));
            lines.push(kv("funct3", format!("0x{funct3:01x}"), Color::Yellow));
        }
        EncFormat::U => {
            let rd  = ((word >> 7) & 0x1f) as u8;
            let imm = ((word & 0xfffff000) as i32) >> 12;
            lines.push(reg_kv("rd", rd));
            lines.push(imm_kv("imm[31:12]", imm));
        }
        EncFormat::J => {
            let b20    = (word >> 31) & 1;
            let b10_1  = (word >> 21) & 0x3ff;
            let b11    = (word >> 20) & 1;
            let b19_12 = (word >> 12) & 0xff;
            let rd     = ((word >> 7) & 0x1f) as u8;
            let imm    = (((((b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1)) as i32) << 11) >> 11) as i32;
            lines.push(reg_kv("rd", rd));
            lines.push(imm_kv("offset", imm));
        }
    }
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
            0x5              => "rd ← rs1 >> shamt  [arithmetic]",
            _ => "I-type ALU immediate",
        },
        0x03 => match funct3 {
            0x0 => "rd ← sign_ext(MEM8[rs1+imm])",
            0x1 => "rd ← sign_ext(MEM16[rs1+imm])",
            0x2 => "rd ← MEM32[rs1+imm]",
            0x4 => "rd ← zero_ext(MEM8[rs1+imm])",
            0x5 => "rd ← zero_ext(MEM16[rs1+imm])",
            _   => "Load from memory",
        },
        0x23 => match funct3 {
            0x0 => "MEM8[rs1+imm]  ← rs2[7:0]",
            0x1 => "MEM16[rs1+imm] ← rs2[15:0]",
            0x2 => "MEM32[rs1+imm] ← rs2",
            _   => "Store to memory",
        },
        0x63 => match funct3 {
            0x0 => "if rs1 == rs2  → PC += offset",
            0x1 => "if rs1 != rs2  → PC += offset",
            0x4 => "if rs1 <  rs2  → PC += offset  [signed]",
            0x5 => "if rs1 >= rs2  → PC += offset  [signed]",
            0x6 => "if rs1 <  rs2  → PC += offset  [unsigned]",
            0x7 => "if rs1 >= rs2  → PC += offset  [unsigned]",
            _   => "Conditional branch",
        },
        0x37 => "rd ← imm << 12  (upper 20 bits immediate)",
        0x17 => "rd ← PC + (imm << 12)  (PC-relative upper imm)",
        0x6f => "rd ← PC+4 ;  PC += offset  (jump and link)",
        0x67 => "rd ← PC+4 ;  PC ← (rs1+imm) & ~1  (jump register)",
        0x73 => match word {
            0x00000073 => "Transfer control to execution environment (syscall)",
            0x00100073 => "Breakpoint / halt",
            _          => "System instruction",
        },
        _ => "",
    };

    if !desc.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("⟹  ", Style::default().fg(Color::DarkGray)),
            Span::styled(desc.to_string(), Style::default().fg(Color::White)),
        ]));
    } else if !disasm.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("⟹  ", Style::default().fg(Color::DarkGray)),
            Span::styled(disasm.to_string(), Style::default().fg(Color::DarkGray)),
        ]));
    }
}

// ── Format detection + segments ──────────────────────────────────────────────

#[derive(Copy, Clone)]
enum EncFormat { R, I, S, B, U, J }

impl EncFormat {
    fn name(self) -> &'static str {
        match self {
            EncFormat::R => "R-type",
            EncFormat::I => "I-type",
            EncFormat::S => "S-type",
            EncFormat::B => "B-type",
            EncFormat::U => "U-type",
            EncFormat::J => "J-type",
        }
    }
    fn segments(self) -> Vec<Seg> {
        seg_list(self)
    }
}

fn detect_format(word: u32) -> EncFormat {
    match word & 0x7f {
        0x03 | 0x13 | 0x1b | 0x67 | 0x73 => EncFormat::I,
        0x23 => EncFormat::S,
        0x63 => EncFormat::B,
        0x37 | 0x17 => EncFormat::U,
        0x6f => EncFormat::J,
        _ => EncFormat::R,
    }
}

struct Seg {
    label: &'static str,
    width: u8,
    color: Color,
}

fn seg_list(format: EncFormat) -> Vec<Seg> {
    macro_rules! s {
        ($l:expr, $w:expr, $c:expr) => { Seg { label: $l, width: $w, color: $c } };
    }
    use Color::*;
    match format {
        EncFormat::R => vec![
            s!("funct7", 7, Red), s!("rs2", 5, LightRed), s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow), s!("rd", 5, LightGreen), s!("opcode", 7, Cyan),
        ],
        EncFormat::I => vec![
            s!("imm[11:0]", 12, Blue), s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow), s!("rd", 5, LightGreen), s!("opcode", 7, Cyan),
        ],
        EncFormat::S => vec![
            s!("imm[11:5]", 7, Blue), s!("rs2", 5, LightRed), s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow), s!("imm[4:0]", 5, Blue), s!("opcode", 7, Cyan),
        ],
        EncFormat::B => vec![
            s!("i12", 1, Blue), s!("i10:5", 6, Blue), s!("rs2", 5, LightRed),
            s!("rs1", 5, LightMagenta), s!("fn3", 3, Yellow),
            s!("i4:1", 4, Blue), s!("i11", 1, Blue), s!("opcode", 7, Cyan),
        ],
        EncFormat::U => vec![
            s!("imm[31:12]", 20, Blue), s!("rd", 5, LightGreen), s!("opcode", 7, Cyan),
        ],
        EncFormat::J => vec![
            s!("i20", 1, Blue), s!("i10:1", 10, Blue), s!("i11", 1, Blue),
            s!("i19:12", 8, Blue), s!("rd", 5, LightGreen), s!("opcode", 7, Cyan),
        ],
    }
}

// ── Disassembly pretty-printer ────────────────────────────────────────────────

fn pretty_instr(instruction: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;
    match *instruction {
        Add{rd,rs1,rs2}    => fmt3("add",  rd, rs1, rs2),
        Sub{rd,rs1,rs2}    => fmt3("sub",  rd, rs1, rs2),
        And{rd,rs1,rs2}    => fmt3("and",  rd, rs1, rs2),
        Or{rd,rs1,rs2}     => fmt3("or",   rd, rs1, rs2),
        Xor{rd,rs1,rs2}    => fmt3("xor",  rd, rs1, rs2),
        Sll{rd,rs1,rs2}    => fmt3("sll",  rd, rs1, rs2),
        Srl{rd,rs1,rs2}    => fmt3("srl",  rd, rs1, rs2),
        Sra{rd,rs1,rs2}    => fmt3("sra",  rd, rs1, rs2),
        Slt{rd,rs1,rs2}    => fmt3("slt",  rd, rs1, rs2),
        Sltu{rd,rs1,rs2}   => fmt3("sltu", rd, rs1, rs2),
        Mul{rd,rs1,rs2}    => fmt3("mul",  rd, rs1, rs2),
        Mulh{rd,rs1,rs2}   => fmt3("mulh", rd, rs1, rs2),
        Mulhsu{rd,rs1,rs2} => fmt3("mulhsu",rd,rs1, rs2),
        Mulhu{rd,rs1,rs2}  => fmt3("mulhu",rd, rs1, rs2),
        Div{rd,rs1,rs2}    => fmt3("div",  rd, rs1, rs2),
        Divu{rd,rs1,rs2}   => fmt3("divu", rd, rs1, rs2),
        Rem{rd,rs1,rs2}    => fmt3("rem",  rd, rs1, rs2),
        Remu{rd,rs1,rs2}   => fmt3("remu", rd, rs1, rs2),
        Addi{rd,rs1,imm}   => fmt_ri("addi",  rd, rs1, imm),
        Andi{rd,rs1,imm}   => fmt_ri("andi",  rd, rs1, imm),
        Ori{rd,rs1,imm}    => fmt_ri("ori",   rd, rs1, imm),
        Xori{rd,rs1,imm}   => fmt_ri("xori",  rd, rs1, imm),
        Slti{rd,rs1,imm}   => fmt_ri("slti",  rd, rs1, imm),
        Sltiu{rd,rs1,imm}  => fmt_ri("sltiu", rd, rs1, imm),
        Slli{rd,rs1,shamt} => fmt_ri("slli",  rd, rs1, shamt as i32),
        Srli{rd,rs1,shamt} => fmt_ri("srli",  rd, rs1, shamt as i32),
        Srai{rd,rs1,shamt} => fmt_ri("srai",  rd, rs1, shamt as i32),
        Lb{rd,rs1,imm}     => fmt_load("lb",  rd, rs1, imm),
        Lh{rd,rs1,imm}     => fmt_load("lh",  rd, rs1, imm),
        Lw{rd,rs1,imm}     => fmt_load("lw",  rd, rs1, imm),
        Lbu{rd,rs1,imm}    => fmt_load("lbu", rd, rs1, imm),
        Lhu{rd,rs1,imm}    => fmt_load("lhu", rd, rs1, imm),
        Sb{rs2,rs1,imm}    => fmt_store("sb", rs2, rs1, imm),
        Sh{rs2,rs1,imm}    => fmt_store("sh", rs2, rs1, imm),
        Sw{rs2,rs1,imm}    => fmt_store("sw", rs2, rs1, imm),
        Beq{rs1,rs2,imm}   => fmt_br("beq",  rs1, rs2, imm),
        Bne{rs1,rs2,imm}   => fmt_br("bne",  rs1, rs2, imm),
        Blt{rs1,rs2,imm}   => fmt_br("blt",  rs1, rs2, imm),
        Bge{rs1,rs2,imm}   => fmt_br("bge",  rs1, rs2, imm),
        Bltu{rs1,rs2,imm}  => fmt_br("bltu", rs1, rs2, imm),
        Bgeu{rs1,rs2,imm}  => fmt_br("bgeu", rs1, rs2, imm),
        Lui{rd,imm}        => format!("{:<5} {}, {}", "lui",   reg_name(rd), imm),
        Auipc{rd,imm}      => format!("{:<5} {}, {}", "auipc", reg_name(rd), imm),
        Jal{rd,imm}        => format!("{:<5} {}, {}", "jal",   reg_name(rd), imm),
        Jalr{rd,rs1,imm}   => fmt_ri("jalr", rd, rs1, imm),
        Ecall              => "ecall".into(),
        Ebreak | Halt      => "ebreak".into(),
        Fence              => "fence".into(),
        // RV32F
        Flw{rd,rs1,imm}    => format!("{:<9} {}, {imm}({})", "flw",    freg_name(rd),  reg_name(rs1)),
        Fsw{rs2,rs1,imm}   => format!("{:<9} {}, {imm}({})", "fsw",    freg_name(rs2), reg_name(rs1)),
        FaddS{rd,rs1,rs2}  => fmt3f("fadd.s",   rd, rs1, rs2),
        FsubS{rd,rs1,rs2}  => fmt3f("fsub.s",   rd, rs1, rs2),
        FmulS{rd,rs1,rs2}  => fmt3f("fmul.s",   rd, rs1, rs2),
        FdivS{rd,rs1,rs2}  => fmt3f("fdiv.s",   rd, rs1, rs2),
        FsqrtS{rd,rs1}     => format!("{:<9} {}, {}", "fsqrt.s", freg_name(rd), freg_name(rs1)),
        FminS{rd,rs1,rs2}  => fmt3f("fmin.s",   rd, rs1, rs2),
        FmaxS{rd,rs1,rs2}  => fmt3f("fmax.s",   rd, rs1, rs2),
        FsgnjS{rd,rs1,rs2} => fmt3f("fsgnj.s",  rd, rs1, rs2),
        FsgnjnS{rd,rs1,rs2}=> fmt3f("fsgnjn.s", rd, rs1, rs2),
        FsgnjxS{rd,rs1,rs2}=> fmt3f("fsgnjx.s", rd, rs1, rs2),
        FeqS{rd,rs1,rs2}   => format!("{:<9} {}, {}, {}", "feq.s",    reg_name(rd),  freg_name(rs1), freg_name(rs2)),
        FltS{rd,rs1,rs2}   => format!("{:<9} {}, {}, {}", "flt.s",    reg_name(rd),  freg_name(rs1), freg_name(rs2)),
        FleS{rd,rs1,rs2}   => format!("{:<9} {}, {}, {}", "fle.s",    reg_name(rd),  freg_name(rs1), freg_name(rs2)),
        FcvtWS{rd,rs1,..}  => format!("{:<9} {}, {}", "fcvt.w.s",  reg_name(rd),   freg_name(rs1)),
        FcvtWuS{rd,rs1,..} => format!("{:<9} {}, {}", "fcvt.wu.s", reg_name(rd),   freg_name(rs1)),
        FcvtSW{rd,rs1}     => format!("{:<9} {}, {}", "fcvt.s.w",  freg_name(rd),  reg_name(rs1)),
        FcvtSWu{rd,rs1}    => format!("{:<9} {}, {}", "fcvt.s.wu", freg_name(rd),  reg_name(rs1)),
        FmvXW{rd,rs1}      => format!("{:<9} {}, {}", "fmv.x.w",   reg_name(rd),   freg_name(rs1)),
        FmvWX{rd,rs1}      => format!("{:<9} {}, {}", "fmv.w.x",   freg_name(rd),  reg_name(rs1)),
        FclassS{rd,rs1}    => format!("{:<9} {}, {}", "fclass.s",  reg_name(rd),   freg_name(rs1)),
        FmaddS{rd,rs1,rs2,rs3}  => fmt4f("fmadd.s",   rd, rs1, rs2, rs3),
        FmsubS{rd,rs1,rs2,rs3}  => fmt4f("fmsub.s",   rd, rs1, rs2, rs3),
        FnmsubS{rd,rs1,rs2,rs3} => fmt4f("fnmsub.s",  rd, rs1, rs2, rs3),
        FnmaddS{rd,rs1,rs2,rs3} => fmt4f("fnmadd.s",  rd, rs1, rs2, rs3),
    }
}

fn fmt3(m: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!("{m:<5} {}, {}, {}", reg_name(rd), reg_name(rs1), reg_name(rs2))
}
fn fmt3f(m: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!("{m:<9} {}, {}, {}", freg_name(rd), freg_name(rs1), freg_name(rs2))
}
fn fmt4f(m: &str, rd: u8, rs1: u8, rs2: u8, rs3: u8) -> String {
    format!("{m:<9} {}, {}, {}, {}", freg_name(rd), freg_name(rs1), freg_name(rs2), freg_name(rs3))
}
fn freg_name(i: u8) -> &'static str {
    match i {
        0  => "ft0",  1  => "ft1",  2  => "ft2",  3  => "ft3",
        4  => "ft4",  5  => "ft5",  6  => "ft6",  7  => "ft7",
        8  => "fs0",  9  => "fs1",
        10 => "fa0",  11 => "fa1",  12 => "fa2",  13 => "fa3",
        14 => "fa4",  15 => "fa5",  16 => "fa6",  17 => "fa7",
        18 => "fs2",  19 => "fs3",  20 => "fs4",  21 => "fs5",
        22 => "fs6",  23 => "fs7",  24 => "fs8",  25 => "fs9",
        26 => "fs10", 27 => "fs11",
        28 => "ft8",  29 => "ft9",  30 => "ft10", 31 => "ft11",
        _  => "f?",
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
