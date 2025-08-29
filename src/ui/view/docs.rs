use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::cmp::min;

use super::App;
use crate::ui::app::Lang;

pub(super) fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    // Split header (1 row) and body
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .split(area);

    // Header: language toggle on the right
    let lang_label = match app.lang { Lang::EN => "[EN]", Lang::PT => "[PT]" };
    // Button width is 4, pad with a space left and right area
    let btn_width = 4u16;
    let header_area = chunks[0];
    // Place the button at the far right
    let btn_rect = Rect::new(
        header_area.x + header_area.width.saturating_sub(btn_width),
        header_area.y,
        btn_width,
        1,
    );

    // Draw left header hint
    let header_hint = match app.lang {
        Lang::EN => "Docs • Up/Down/PageUp/PageDown",
        Lang::PT => "Docs • Cima/Baixo/PagUp/PagDn",
    };
    let left = Paragraph::new(header_hint);
    f.render_widget(left, header_area);

    // Draw language button with colors
    let button = Paragraph::new(Line::from(Span::styled(
        lang_label,
        Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD),
    )));
    f.render_widget(button, btn_rect);

    // Body: build styled lines with a bordered block
    let (title, text) = match app.lang {
        Lang::EN => ("Instruction Guide (EN)", DOC_TEXT_EN),
        Lang::PT => ("Guia de Instruções (PT-BR)", DOC_TEXT_PT),
    };
    let all_lines: Vec<&str> = text.lines().collect();
    let h = chunks[1].height.saturating_sub(2) as usize; // minus borders
    let start = app.docs_scroll.min(all_lines.len());
    let end = min(all_lines.len(), start + h);
    let mut styled: Vec<Line> = Vec::with_capacity(end.saturating_sub(start));
    for &ln in &all_lines[start..end] {
        let line = if ln.trim().is_empty() {
            Line::raw("")
        } else if ln.ends_with(":") || ln == "Overview" || ln == "Visão geral" || ln.starts_with("Type ") || ln.starts_with("Tipo ") {
            Line::from(Span::styled(ln, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        } else if ln.starts_with("Syntax:") || ln.starts_with("Sintaxe:") {
            Line::from(Span::styled(ln, Style::default().fg(Color::Yellow)))
        } else if ln.starts_with("Notes") || ln.starts_with("Notas") {
            Line::from(Span::styled(ln, Style::default().fg(Color::Gray)))
        } else if ln.starts_with("- ") {
            Line::from(Span::styled(ln, Style::default().fg(Color::White)))
        } else {
            Line::raw(ln)
        };
        styled.push(line);
    }
    let para = Paragraph::new(styled)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    f.render_widget(para, chunks[1]);
}

const DOC_TEXT_EN: &str = r#"Falcon ASM – Instruction Guide (RV32I)

Overview
- Cycle: fetch → decode → execute. PC advances +4 per instruction.
- Endianness: little-endian. x0 is hardwired to 0.
- Addressing: branches/jumps use byte offsets; least significant bit must be 0.

R-type (opcode 0x33):
Syntax: add rd, rs1, rs2 | sub rd, rs1, rs2 | and rd, rs1, rs2 | or rd, rs1, rs2 | xor rd, rs1, rs2 | sll rd, rs1, rs2 | srl rd, rs1, rs2 | sra rd, rs1, rs2 | slt rd, rs1, rs2 | sltu rd, rs1, rs2
M-ext: mul rd, rs1, rs2 | mulh rd, rs1, rs2 | mulhsu rd, rs1, rs2 | mulhu rd, rs1, rs2 | div rd, rs1, rs2 | divu rd, rs1, rs2 | rem rd, rs1, rs2 | remu rd, rs1, rs2

I-type (opcode 0x13):
Syntax: addi rd, rs1, imm | xori rd, rs1, imm | ori rd, rs1, imm | andi rd, rs1, imm | slli rd, rs1, shamt | srli rd, rs1, shamt | srai rd, rs1, shamt
Immediates are 12-bit signed. Shift shamt uses 5 bits.

Loads (opcode 0x03):
Syntax: lb rd, imm(rs1) | lh rd, imm(rs1) | lw rd, imm(rs1) | lbu rd, imm(rs1) | lhu rd, imm(rs1)

S-type Stores (opcode 0x23):
Syntax: sb rs2, imm(rs1) | sh rs2, imm(rs1) | sw rs2, imm(rs1)

B-type Branches (0x63):
Syntax: beq rs1, rs2, label | bne rs1, rs2, label | blt rs1, rs2, label | bge rs1, rs2, label | bltu rs1, rs2, label | bgeu rs1, rs2, label

U-type (LUI/AUIPC):
Syntax: lui rd, imm20 | auipc rd, imm20

Jumps:
Syntax: jal rd, label | jalr rd, rs1, imm

Pseudo‑instructions:
- nop → addi x0, x0, 0
- mv rd, rs → addi rd, rs, 0
- li rd, imm12 → addi rd, x0, imm
- subi rd, rs1, imm → addi rd, rs1, -imm
- j label → jal x0, label; call label → jal ra, label
- jr rs → jalr x0, rs, 0; ret → jalr x0, ra, 0
- la rd, label → lui/addi to load absolute address
- push rs → addi sp, sp, -4; sw rs, 0(sp)
- pop rd → lw rd, 0(sp); addi sp, sp, 4
- print rd | printString label|rd | read → set a7 and ecall (see Syscalls)

Notes
- Assembler supports .text/.data segments, labels, and data directives (.byte/.half/.word/.dword/.ascii/.asciz/.space).
- See docs/format.md for bit layouts and tables.
"#;

const DOC_TEXT_PT: &str = r#"Falcon ASM – Guia de Instruções (RV32I)

Visão geral
- Ciclo: busca → decodifica → executa. O PC avança +4 por instrução.
- Endianness: little-endian. x0 é fixo em 0.
- Endereçamento: desvios/saltos usam deslocamento em bytes; bit menos significativo deve ser 0.

Tipo R (opcode 0x33):
Sintaxe: add rd, rs1, rs2 | sub rd, rs1, rs2 | and rd, rs1, rs2 | or rd, rs1, rs2 | xor rd, rs1, rs2 | sll rd, rs1, rs2 | srl rd, rs1, rs2 | sra rd, rs1, rs2 | slt rd, rs1, rs2 | sltu rd, rs1, rs2
Ext. M: mul rd, rs1, rs2 | mulh rd, rs1, rs2 | mulhsu rd, rs1, rs2 | mulhu rd, rs1, rs2 | div rd, rs1, rs2 | divu rd, rs1, rs2 | rem rd, rs1, rs2 | remu rd, rs1, rs2

Tipo I (opcode 0x13):
Sintaxe: addi rd, rs1, imm | xori rd, rs1, imm | ori rd, rs1, imm | andi rd, rs1, imm | slli rd, rs1, shamt | srli rd, rs1, shamt | srai rd, rs1, shamt
Imediatos são de 12 bits com sinal. Shifts usam shamt de 5 bits.

Loads (opcode 0x03):
Sintaxe: lb rd, imm(rs1) | lh rd, imm(rs1) | lw rd, imm(rs1) | lbu rd, imm(rs1) | lhu rd, imm(rs1)

Tipo S Stores (opcode 0x23):
Sintaxe: sb rs2, imm(rs1) | sh rs2, imm(rs1) | sw rs2, imm(rs1)

Tipo B Branches (0x63):
Sintaxe: beq rs1, rs2, label | bne rs1, rs2, label | blt rs1, rs2, label | bge rs1, rs2, label | bltu rs1, rs2, label | bgeu rs1, rs2, label

Tipo U (LUI/AUIPC):
Sintaxe: lui rd, imm20 | auipc rd, imm20

Saltos:
Sintaxe: jal rd, label | jalr rd, rs1, imm

Pseudoinstruções:
- nop → addi x0, x0, 0
- mv rd, rs → addi rd, rs, 0
- li rd, imm12 → addi rd, x0, imm
- subi rd, rs1, imm → addi rd, rs1, -imm
- j label → jal x0, label; call label → jal ra, label
- jr rs → jalr x0, rs, 0; ret → jalr x0, ra, 0
- la rd, label → usa lui/addi para carregar endereço absoluto
- push rs → addi sp, sp, -4; sw rs, 0(sp)
- pop rd → lw rd, 0(sp); addi sp, sp, 4
- print rd | printString label|rd | read → define a7 e chama ecall (ver Syscalls)

Notas
- O assembler suporta seções .text/.data, rótulos e diretivas de dados (.byte/.half/.word/.dword/.ascii/.asciz/.space).
- Veja docs/format.md para bitfields e tabelas detalhadas.
"#;
