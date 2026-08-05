//! Mockup navegável da aba **Run** redesenhada — roda no terminal de verdade,
//! não toca em nada do app.
//!
//! ```text
//! cargo run --example run_mockup            # interativo, q/Esc sai
//! cargo run --example run_mockup -- --dump  # grade 160x46 em texto puro
//! ```
//!
//! Os dados são fixos (um strlen pausado em `0x18`); o que está em jogo aqui é a
//! gramática visual, não o conteúdo.
//!
//! ## A regra que o redesenho fixa
//!
//! Hoje o colchete significa três coisas ao mesmo tempo: classe de instrução
//! (`[I]`), tecla (`[P]=pin`) e botão clicável (`[?]`). Por isso o botão de
//! informação se perde. Aqui:
//!
//! | forma                       | significa      |
//! |-----------------------------|----------------|
//! | `[s]` colchete apagado      | tecla          |
//! | ` ? Help ` com fundo sólido | botão de clique|
//! | letra nua colorida (`I`)    | dado           |
//!
//! Nada mais na tela tem fundo sólido em repouso, então o botão de ajuda é a
//! única superfície clicável — impossível confundir com dado.
//!
//! As cores são as de `src/ui/theme.rs`, repetidas aqui só para o exemplo ficar
//! autocontido (ele não linka a lib).

use std::io::{self, Stdout};

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

// ── Paleta (espelha src/ui/theme.rs) ─────────────────────────────────────────

const BG: Color = Color::Rgb(28, 28, 32);
const TXT: Color = Color::Rgb(210, 210, 218);
const DIM: Color = Color::Rgb(105, 105, 115);
const BD: Color = Color::Rgb(58, 58, 66);
const VIO: Color = Color::Rgb(145, 95, 250);
const RUN: Color = Color::Rgb(88, 200, 120);
const PAU: Color = Color::Rgb(220, 170, 55);
const DNG: Color = Color::Rgb(210, 72, 68);
const CYC: Color = Color::Rgb(110, 175, 220);
const CPI: Color = Color::Rgb(175, 135, 245);
const IPC: Color = Color::Rgb(225, 180, 80);
const IMM: Color = Color::Rgb(115, 178, 235);
const MNEM: Color = Color::Rgb(218, 178, 75);
const CMT: Color = Color::Rgb(98, 138, 112);
/// Superfície do botão em repouso — a única da tela.
const BTN_BG: Color = Color::Rgb(44, 44, 50);
/// Fundo da linha do PC.
const SEL_BG: Color = Color::Rgb(48, 42, 72);

// ── Helpers de span ──────────────────────────────────────────────────────────

fn sp(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(color))
}

fn spb(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn raw(text: impl Into<String>) -> Span<'static> {
    Span::raw(text.into())
}

fn pad(n: usize) -> Span<'static> {
    Span::raw(" ".repeat(n))
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Estica `spans` até `w` com espaços à direita (nunca encolhe).
fn fill(mut spans: Vec<Span<'static>>, w: usize) -> Vec<Span<'static>> {
    let used = spans_width(&spans);
    if used < w {
        spans.push(pad(w - used));
    }
    spans
}

// ── Chrome de painel ─────────────────────────────────────────────────────────
//
// `╭─ Título ─────────── estado ─╮` — o slot da direita carrega **estado**
// (banco, região, endereço), nunca atalho. A borda violeta marca o painel com
// foco, que é a informação que a tela de hoje não dá.

fn border(focus: bool) -> Color {
    if focus { VIO } else { BD }
}

fn ptop(w: usize, title: &str, right: &str, focus: bool) -> Vec<Span<'static>> {
    let bc = border(focus);
    let right_w = if right.is_empty() {
        0
    } else {
        right.chars().count() + 2
    };
    let dashes = w.saturating_sub(6 + title.chars().count() + right_w);

    let mut spans = vec![
        sp("╭─ ", bc),
        if focus {
            spb(title.to_string(), TXT)
        } else {
            sp(title.to_string(), DIM)
        },
        sp(" ", bc),
        sp("─".repeat(dashes), bc),
    ];
    if !right.is_empty() {
        spans.push(sp(" ", bc));
        spans.push(sp(right.to_string(), DIM));
        spans.push(sp(" ", bc));
    }
    spans.push(sp("─╮", bc));
    spans
}

fn pbot(w: usize, focus: bool) -> Vec<Span<'static>> {
    let bc = border(focus);
    vec![sp("╰", bc), sp("─".repeat(w - 2), bc), sp("╯", bc)]
}

fn prow(w: usize, inner: Vec<Span<'static>>, focus: bool) -> Vec<Span<'static>> {
    let bc = border(focus);
    let mut spans = vec![sp("│", bc)];
    spans.extend(fill(inner, w - 2));
    spans.push(sp("│", bc));
    spans
}

fn pblank(w: usize, focus: bool) -> Vec<Span<'static>> {
    prow(w, Vec::new(), focus)
}

/// Régua rotulada dentro de um painel: `   ENCODING ──────────`.
/// Substitui as caixas aninhadas — mesma separação, sem gastar duas linhas de
/// borda por seção.
fn section(w: usize, name: &str) -> Vec<Span<'static>> {
    let dashes = (w - 2).saturating_sub(7 + name.chars().count());
    vec![
        raw("   "),
        sp(name.to_string(), DIM),
        raw(" "),
        sp("─".repeat(dashes), BD),
        raw("   "),
    ]
}

/// Cola painéis lado a lado, linha por linha.
fn zip_panes(panes: &[Vec<Vec<Span<'static>>>]) -> Vec<Line<'static>> {
    let height = panes.iter().map(Vec::len).max().unwrap_or(0);
    (0..height)
        .map(|row| {
            let mut spans = Vec::new();
            for pane in panes {
                if let Some(line) = pane.get(row) {
                    spans.extend(line.clone());
                }
            }
            Line::from(spans)
        })
        .collect()
}

// ── Dados do mockup ──────────────────────────────────────────────────────────

const PC: u32 = 0x18;

/// `(abi, xN, valor, mudou-agora)`
const REGS: &[(&str, &str, u32, bool)] = &[
    ("zero", "x0", 0x0000_0000, false),
    ("ra", "x1", 0x0000_0000, false),
    ("sp", "x2", 0x0100_0000, false),
    ("gp", "x3", 0x0000_0000, false),
    ("tp", "x4", 0x0000_0000, false),
    ("t0", "x5", 0x0000_0048, true),
    ("t1", "x6", 0x0000_0000, false),
    ("t2", "x7", 0x0000_0000, false),
    ("s0", "x8", 0x0000_0000, false),
    ("s1", "x9", 0x0000_0000, false),
    ("a0", "x10", 0x0000_0000, false),
    ("a1", "x11", 0x0000_1000, false),
    ("a2", "x12", 0x0000_1003, false),
    ("a3", "x13", 0x0000_0000, false),
    ("a4", "x14", 0x0000_0000, false),
    ("a5", "x15", 0x0000_0000, false),
    ("a6", "x16", 0x0000_0000, false),
    ("a7", "x17", 0x0000_0040, false),
    ("s2", "x18", 0x0000_0000, false),
    ("s3", "x19", 0x0000_0000, false),
    ("s4", "x20", 0x0000_0000, false),
    ("s5", "x21", 0x0000_0000, false),
    ("s6", "x22", 0x0000_0000, false),
    ("s7", "x23", 0x0000_0000, false),
    ("s8", "x24", 0x0000_0000, false),
    ("s9", "x25", 0x0000_0000, false),
    ("s10", "x26", 0x0000_0000, false),
    ("s11", "x27", 0x0000_0000, false),
    ("t3", "x28", 0x0000_0000, false),
    ("t4", "x29", 0x0000_0000, false),
    ("t5", "x30", 0x0000_0000, false),
    ("t6", "x31", 0x0000_0000, false),
];

/// `(classe, addr, mnemônico, operandos, execuções)`
const IMEM: &[(char, u32, &str, &str, u32)] = &[
    ('I', 0x00, "addi ", "t0, zero, 0", 1),
    ('U', 0x04, "lui  ", "a1, 0x1", 1),
    ('I', 0x08, "addi ", "a1, a1, 0", 1),
    ('I', 0x0c, "addi ", "a2, a1, 0", 1),
    ('L', 0x10, "lbu  ", "t0, 0(a2)", 4),
    ('B', 0x14, "beq  ", "t0, zero, 12", 4),
    ('I', 0x18, "addi ", "a2, a2, 1", 3),
    ('J', 0x1c, "jal  ", "zero, -12", 3),
    ('R', 0x20, "sub  ", "a2, a2, a1", 0),
    ('I', 0x24, "addi ", "a0, zero, 1", 0),
    ('I', 0x28, "addi ", "a7, zero, 64", 0),
    ('I', 0x2c, "ecall", "", 0),
    ('I', 0x30, "addi ", "sp, sp, -16", 0),
    ('I', 0x34, "addi ", "t0, zero, 10", 0),
    ('S', 0x38, "sb   ", "t0, 0(sp)", 0),
    ('I', 0x3c, "addi ", "a0, zero, 1", 0),
    ('I', 0x40, "addi ", "a1, sp, 0", 0),
    ('I', 0x44, "addi ", "a2, zero, 1", 0),
    ('I', 0x48, "addi ", "a7, zero, 64", 0),
    ('I', 0x4c, "ecall", "", 0),
    ('I', 0x50, "addi ", "sp, sp, 16", 0),
    ('-', 0x54, "halt ", "", 0),
];

/// Cor por **família** de formato, não uma cor por letra: ALU, memória, fluxo,
/// upper. Assim a coluna vira escaneável em vez de arco-íris.
fn class_color(c: char) -> Color {
    match c {
        'R' | 'I' => CYC,
        'L' | 'S' => MNEM,
        'B' | 'J' => CPI,
        'U' => RUN,
        _ => DIM,
    }
}

/// `0x` + zeros à esquerda apagados, dígitos significativos acesos — truque de
/// disassembler que faz a coluna de endereço parar de competir com o código.
fn addr_spans(addr: u32, bright: Color, dark: Color) -> Vec<Span<'static>> {
    let hex = format!("{addr:08x}");
    let lead = hex.len() - hex.trim_start_matches('0').len();
    let lead = lead.min(hex.len() - 1);
    vec![
        sp("0x", dark),
        sp(hex[..lead].to_string(), dark),
        sp(hex[lead..].to_string(), bright),
    ]
}

// ── Painéis ──────────────────────────────────────────────────────────────────

const REG_W: usize = 38;
const IMEM_W: usize = 42;

fn reg_pane(rows: usize) -> Vec<Vec<Span<'static>>> {
    let mut out = vec![ptop(REG_W, "Registers", "int", false)];

    out.push(prow(
        REG_W,
        vec![raw("  "), sp("pc   ", DIM), raw("      "), spb("0x00000018", VIO)],
        false,
    ));
    out.push(prow(
        REG_W,
        vec![raw(" "), sp("─".repeat(REG_W - 4), BD), raw(" ")],
        false,
    ));

    for &(abi, num, value, changed) in REGS {
        // Zero fica apagado: 30 linhas de 0x00000000 em brilho total afogam os
        // quatro valores que interessam. O escrito agora ganha ponto violeta.
        let value_color = if changed {
            VIO
        } else if value == 0 {
            DIM
        } else {
            TXT
        };
        let name_color = if value == 0 && !changed { DIM } else { TXT };
        out.push(prow(
            REG_W,
            vec![
                raw("  "),
                sp(format!("{abi:<5}"), name_color), // ABI primeiro: é o que se lê
                raw(" "),
                sp(format!("{num:<4}"), DIM),
                raw(" "),
                if changed {
                    spb(format!("0x{value:08x}"), value_color)
                } else {
                    sp(format!("0x{value:08x}"), value_color)
                },
                raw("  "),
                if changed { spb("●", VIO) } else { raw(" ") },
            ],
            false,
        ));
    }

    while out.len() < rows - 1 {
        out.push(pblank(REG_W, false));
    }
    out.truncate(rows - 1);
    out.push(pbot(REG_W, false));
    out
}

fn imem_pane(rows: usize) -> Vec<Vec<Span<'static>>> {
    // Este é o painel com foco, então a borda é violeta.
    let mut out = vec![ptop(IMEM_W, "Instruction Memory", "pc", true)];

    for &(class, addr, mnem, ops, count) in IMEM {
        let is_pc = addr == PC;
        let count_text = if count == 0 {
            "  ·".to_string()
        } else {
            format!(" ×{count}")
        };

        let mut inner: Vec<Span<'static>> = Vec::new();
        if is_pc {
            // Barra na sarjeta + fundo, em vez de um `▶` disputando espaço com
            // o texto dentro da coluna.
            let bg = Style::default().bg(SEL_BG);
            inner.push(spb("▌", VIO));
            inner.push(Span::styled(
                class.to_string(),
                bg.fg(VIO).add_modifier(Modifier::BOLD),
            ));
            inner.push(Span::styled("  ", bg));
            inner.push(Span::styled(format!("0x{addr:08x}"), bg.fg(TXT)));
            inner.push(Span::styled("  ", bg));
            inner.push(Span::styled(mnem.to_string(), bg.fg(MNEM)));
            inner.push(Span::styled(format!(" {ops:<13} "), bg.fg(TXT)));
            inner.push(Span::styled(count_text, bg.fg(DIM)));
            // O realce tem de ir até a borda, senão vira um retângulo pela metade.
            let rest = (IMEM_W - 2).saturating_sub(spans_width(&inner));
            inner.push(Span::styled(" ".repeat(rest), bg));
        } else {
            inner.push(raw(" "));
            inner.push(sp(class.to_string(), class_color(class)));
            inner.push(raw("  "));
            inner.extend(addr_spans(addr, TXT, DIM));
            inner.push(raw("  "));
            inner.push(sp(mnem.to_string(), MNEM));
            inner.push(sp(format!(" {ops:<13} "), TXT));
            inner.push(sp(count_text, DIM));
        }
        out.push(prow(IMEM_W, inner, true));
    }

    while out.len() < rows - 1 {
        out.push(pblank(IMEM_W, true));
    }
    out.truncate(rows - 1);
    out.push(pbot(IMEM_W, true));
    out
}

fn detail_pane(w: usize, rows: usize) -> Vec<Vec<Span<'static>>> {
    let mut out = vec![ptop(w, "Instruction", "0x00000018 · pc", false)];
    let row = |inner: Vec<Span<'static>>| prow(w, inner, false);
    let blank = || pblank(w, false);

    out.push(blank());
    out.push(row(vec![
        raw("   "),
        sp("addi", MNEM),
        raw("   "),
        sp("a2, a2, ", TXT),
        sp("1", IMM),
        pad(29),
        sp("ALU", CYC),
        raw("   "),
        sp("~1 cycle", DIM),
    ]));
    out.push(row(vec![
        raw("   "),
        sp("0x00160613", TXT),
        raw("    "),
        sp("0000 0000 0001 0110 0000 0110 0001 0011", DIM),
    ]));
    out.push(blank());

    // Tabela de verdade no lugar de `imm[11:0]▮▮▮ rs1▮▮ fn3` — os blocos de
    // preenchimento simulavam colunas que nunca alinhavam com os bits.
    out.push(row(section(w, "ENCODING")));
    out.push(blank());
    out.push(row(vec![
        raw("   "),
        sp(" 31          20 19   15 14 12 11    7 6       0", DIM),
    ]));
    out.push(row(vec![
        raw("   "),
        sp("┌──────────────┬───────┬─────┬───────┬─────────┐", BD),
    ]));
    out.push(row(vec![
        raw("   "),
        sp("│", BD),
        sp("  imm[11:0]   ", DIM),
        sp("│", BD),
        sp("  rs1  ", DIM),
        sp("│", BD),
        sp(" fn3 ", DIM),
        sp("│", BD),
        sp("  rd   ", DIM),
        sp("│", BD),
        sp(" opcode  ", DIM),
        sp("│", BD),
    ]));
    out.push(row(vec![
        raw("   "),
        sp("│", BD),
        sp(" 000000000001 ", IMM),
        sp("│", BD),
        sp(" 01100 ", TXT),
        sp("│", BD),
        sp(" 000 ", DIM),
        sp("│", BD),
        sp(" 01100 ", TXT),
        sp("│", BD),
        sp(" 0010011 ", DIM),
        sp("│", BD),
    ]));
    out.push(row(vec![
        raw("   "),
        sp("└──────────────┴───────┴─────┴───────┴─────────┘", BD),
    ]));
    out.push(blank());

    out.push(row(section(w, "OPERANDS")));
    out.push(blank());
    out.push(row(vec![
        raw("    "),
        sp("rd ", DIM),
        raw("   "),
        sp("a2", TXT),
        raw("   "),
        sp("x12", DIM),
        raw("    "),
        sp("0x00001003", DIM),
        sp("  →  ", DIM),
        spb("0x00001004", VIO),
    ]));
    out.push(row(vec![
        raw("    "),
        sp("rs1", DIM),
        raw("   "),
        sp("a2", TXT),
        raw("   "),
        sp("x12", DIM),
        raw("    "),
        sp("0x00001003", TXT),
    ]));
    out.push(row(vec![
        raw("    "),
        sp("imm", DIM),
        raw("   "),
        sp("1", IMM),
        pad(11),
        sp("0x001", DIM),
    ]));
    out.push(blank());

    out.push(row(section(w, "SEMANTICS")));
    out.push(blank());
    out.push(row(vec![raw("    "), sp("rd ← rs1 + imm", TXT)]));
    out.push(row(vec![
        raw("    "),
        sp("advances the string pointer one byte", CMT),
    ]));
    out.push(blank());

    // Seção que não existe hoje e cabe porque as três caixas viraram uma.
    out.push(row(section(w, "NEXT")));
    out.push(blank());
    out.push(row(vec![
        raw("    "),
        sp("0x0000001c", DIM),
        raw("   "),
        sp("jal", MNEM),
        raw("   "),
        sp("zero, ", TXT),
        sp("-12", IMM),
        raw("     "),
        sp("loops back to ", DIM),
        sp("0x00000010", TXT),
    ]));

    while out.len() < rows - 1 {
        out.push(blank());
    }
    out.truncate(rows - 1);
    out.push(pbot(w, false));
    out
}

// ── Chrome do app ────────────────────────────────────────────────────────────

const TABS: &[&str] = &[
    "Editor",
    "Run",
    "Cache",
    "Virtual Memory",
    "Pipeline",
    "Docs",
    "Settings",
];
const ACTIVE_TAB: usize = 1;

/// Barra de abas em 2 linhas (hoje são 3): rótulos, depois uma régua única em
/// que o segmento pesado marca a aba ativa.
///
/// À direita, o botão de ajuda: fundo sólido e a palavra escrita por extenso.
/// É a única superfície da tela em repouso.
fn tab_bar(w: usize) -> Vec<Line<'static>> {
    let mut labels: Vec<Span<'static>> = vec![raw(" ")];
    let mut col = 1usize;
    let mut active_start = 1usize;

    for (i, name) in TABS.iter().enumerate() {
        if i > 0 {
            labels.push(raw("   "));
            col += 3;
        }
        if i == ACTIVE_TAB {
            active_start = col;
            labels.push(spb((*name).to_string(), TXT));
        } else {
            labels.push(sp((*name).to_string(), DIM));
        }
        col += name.chars().count();
    }

    let arch = "rv32 falcon";
    let help = " ? Help ";
    let used = col + arch.chars().count() + 3 + help.chars().count() + 1;
    labels.push(pad(w.saturating_sub(used).max(1)));
    labels.push(sp(arch, DIM));
    labels.push(raw("   "));
    labels.push(Span::styled(
        help,
        Style::default()
            .fg(VIO)
            .bg(BTN_BG)
            .add_modifier(Modifier::BOLD),
    ));
    labels.push(raw(" "));

    let start = active_start.saturating_sub(1);
    let len = TABS[ACTIVE_TAB].chars().count() + 2;
    let rule = vec![
        sp("─".repeat(start), BD),
        spb("━".repeat(len), VIO),
        sp("─".repeat(w.saturating_sub(start + len)), BD),
    ];

    vec![Line::from(labels), Line::from(rule)]
}

/// Barra de comando em 2 linhas + régua, no lugar do painel `Run Controls` de
/// 5 linhas.
///
/// Linha 1: selo de estado (bolinha colorida — forma constante, cor carrega o
/// sentido), transporte, métricas à direita. `state` sai da fila de toggles:
/// era leitura fingindo ser controle.
///
/// Linha 2: os toggles agrupados por função com `│`, em vez de dez chips de
/// peso idêntico em fila.
fn command_bar(w: usize) -> Vec<Line<'static>> {
    let status = vec![
        raw("  "),
        sp("●", PAU),
        spb(" PAUSED", PAU),
        raw("   "),
        sp("core", DIM),
        sp(" 0/3", TXT),
        raw("   "),
        sp("hart", DIM),
        sp(" 0", TXT),
    ];
    let transport = vec![
        sp("▶ run", RUN),
        raw("   "),
        sp("▌▌ pause", DIM), // apagado: já está pausado
        raw("   "),
        sp("▶▌ step", TXT),
        raw("   "),
        sp("▌◀ back", TXT),
        raw("   "),
        sp("⟲ reset", DNG),
    ];
    let metrics = vec![
        sp("cycles", DIM),
        sp(" 1 284", CYC),
        raw("   "),
        sp("cpi", DIM),
        sp(" 1.07", CPI),
        raw("   "),
        sp("ipc", DIM),
        sp(" 0.94", IPC),
        raw("  "),
    ];

    let mut line1 = fill(status, 46);
    line1.extend(transport);
    let rest = w.saturating_sub(spans_width(&line1) + spans_width(&metrics));
    line1.push(pad(rest));
    line1.extend(metrics);

    let sep = || sp("  │  ", BD);
    let mut line2 = vec![
        raw("  "),
        sp("view", DIM),
        sp(" regs", TXT),
        sep(),
        sp("fmt", DIM),
        sp(" hex", TXT),
        raw("   "),
        sp("sign", DIM),
        sp(" uns", DIM),
        sep(),
        sp("region", DIM),
        sp(" data", TXT),
        raw("   "),
        sp("bytes", DIM),
        sp(" 4b", TXT),
        sep(),
        sp("count", DIM),
        sp(" on", TXT),
        raw("   "),
        sp("type", DIM),
        sp(" on", TXT),
        raw("   "),
        sp("screen", DIM),
        sp(" off", DIM),
        sep(),
        sp("speed", DIM),
        sp(" 1x", TXT),
    ];
    let tail = vec![sp("instrs", DIM), sp(" 1 203", TXT), raw("  ")];
    let rest = w.saturating_sub(spans_width(&line2) + spans_width(&tail));
    line2.push(pad(rest));
    line2.extend(tail);

    vec![
        Line::from(line1),
        Line::from(line2),
        Line::from(sp("─".repeat(w), BD)),
    ]
}

fn console(w: usize) -> Vec<Line<'static>> {
    vec![
        Line::from(ptop(w, "Console", "clear", false)),
        Line::from(prow(
            w,
            vec![
                raw("  "),
                sp("loaded ", DIM),
                sp("strlen.bin", TXT),
                sp("  22 instructions, entry ", DIM),
                sp("0x00000000", TXT),
            ],
            false,
        )),
        Line::from(prow(
            w,
            vec![
                raw("  "),
                sp("breakpoint", PAU),
                sp(" reached at ", DIM),
                sp("0x00000018", TXT),
                sp("  —  6 instructions retired since resume", DIM),
            ],
            false,
        )),
        Line::from(pbot(w, false)),
    ]
}

/// Rodapé: aqui — e só aqui — colchete significa tecla.
fn footer() -> Line<'static> {
    let keys = [
        ("s", "step"),
        ("r", "restart"),
        ("space", "run/pause"),
        ("f", "speed"),
        ("v", "sidebar"),
        ("k", "region"),
        ("ctrl+f", "jump"),
        ("?", "help"),
    ];
    let mut spans = vec![raw(" ")];
    for (i, (key, label)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(raw("   "));
        }
        spans.push(sp("[", BD));
        spans.push(sp((*key).to_string(), VIO));
        spans.push(sp("]", BD));
        spans.push(sp(format!(" {label}"), DIM));
    }
    Line::from(spans)
}

// ── Montagem ─────────────────────────────────────────────────────────────────

fn draw(frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let w = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.extend(tab_bar(w));
    lines.extend(command_bar(w));

    // 2 (abas) + 3 (comando) + N (painéis) + 4 (console) + 1 (rodapé)
    let pane_rows = (area.height as usize).saturating_sub(2 + 3 + 4 + 1).max(3);
    let detail_w = w.saturating_sub(REG_W + IMEM_W).max(20);
    lines.extend(zip_panes(&[
        reg_pane(pane_rows),
        imem_pane(pane_rows),
        detail_pane(detail_w, pane_rows),
    ]));

    lines.extend(console(w));
    lines.push(footer());

    frame.render_widget(Paragraph::new(lines), area);
}

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--dump") {
        let mut t = Terminal::new(ratatui::backend::TestBackend::new(160, 46)).unwrap();
        t.draw(draw)?;
        let b = t.backend().buffer().clone();
        for y in 0..b.area.height {
            let row: String = (0..b.area.width).map(|x| b[(x, y)].symbol()).collect();
            println!("{:3}|{}|", y, row);
        }
        return Ok(());
    }
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    loop {
        terminal.draw(draw)?;
        if let Event::Key(key) = event::read()? {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }
    }
}
