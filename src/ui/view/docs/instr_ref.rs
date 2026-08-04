use super::chrome::{
    filter_items, render_filter_bar, render_page_tabs, render_tab_hint, separator_line,
};
use crate::ui::theme;
use crate::ui::view::App;
use crate::ui::view::components::{
    SbGeom, horizontal_scrollbar, vertical_scrollbar, visible_window,
};
use crate::ui::view::style;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use raven_engine::InstructionDoc;

/// Narrowest Type column, wide enough for `[Branch]`. An architecture whose
/// type names are longer widens it — the column used to be fixed at this, so
/// `[Directive]` pushed every column after it out of line.
const TY_W_MIN: usize = 8;
const MNE_W: usize = 13;
const OPS_W: usize = 21;
const EXP_W: usize = 26;

/// The filter bit the active architecture assigns to `kind`.
fn ty_bit(app: &App, kind: &str) -> u16 {
    filter_items(app)
        .iter()
        .find(|(label, ..)| *label == kind)
        .map_or(0, |(_, bit, _)| *bit)
}

fn ty_color(app: &App, kind: &str) -> Color {
    filter_items(app)
        .iter()
        .find(|(label, ..)| *label == kind)
        .map_or(Color::White, |(.., color)| *color)
}

fn filtered_rows(app: &App, query: &str, type_filter: u16) -> Vec<&'static InstructionDoc> {
    let q = query.to_lowercase();
    app.architecture
        .assembler()
        .documented_instructions()
        .iter()
        .filter(|row| (type_filter & ty_bit(app, row.kind)) != 0)
        .filter(|row| {
            q.is_empty()
                || row.mnemonic.to_lowercase().contains(&q)
                || row.operands.to_lowercase().contains(&q)
                || row.summary.to_lowercase().contains(&q)
                || row.expands_to.to_lowercase().contains(&q)
                || row.kind.to_lowercase().contains(&q)
        })
        .collect()
}

pub(crate) fn docs_body_line_count(app: &App, query: &str, type_filter: u16) -> usize {
    if type_filter == 0 {
        return 0;
    }
    filtered_rows(app, query, type_filter).len()
}

fn style_for_token(token: &str, assembler: &dyn raven_engine::Assembler) -> Option<Style> {
    match token {
        "rd" | "rd2" => Some(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        "rs1" | "rs2" | "rs3" | "rs" | "rt" => Some(Style::default().fg(Color::Cyan)),
        "frd" | "frd2" => Some(Style::default().fg(Color::Yellow)),
        "frs" | "frs1" | "frs2" | "frs3" => Some(Style::default().fg(Color::LightYellow)),
        "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" | "n" => {
            Some(Style::default().fg(Color::LightGreen))
        }
        "label" => Some(Style::default().fg(Color::Magenta)),
        "rm" => Some(Style::default().fg(Color::LightYellow)),
        "sym" => Some(Style::default().fg(Color::LightBlue)),
        _ if assembler.is_register(token) => Some(Style::default().fg(Color::LightBlue)),
        _ => None,
    }
}

fn color_text(s: &str, assembler: &dyn raven_engine::Assembler) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut token = String::new();
    let mut sep = String::new();

    let flush_sep = |spans: &mut Vec<Span<'static>>, sep: &mut String| {
        if !sep.is_empty() {
            spans.push(Span::raw(std::mem::take(sep)));
        }
    };
    let flush_token = |spans: &mut Vec<Span<'static>>, token: &mut String| {
        if token.is_empty() {
            return;
        }
        let t = std::mem::take(token);
        if let Some(style) = style_for_token(&t, assembler) {
            spans.push(Span::styled(t, style));
        } else {
            spans.push(Span::raw(t));
        }
    };

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            flush_sep(&mut spans, &mut sep);
            token.push(ch);
        } else {
            flush_token(&mut spans, &mut token);
            sep.push(ch);
        }
    }
    flush_token(&mut spans, &mut token);
    flush_sep(&mut spans, &mut sep);

    spans
}

fn pad_or_truncate(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let len = s.chars().count();
    if len > width {
        let n = width.saturating_sub(1);
        let truncated: String = s.chars().take(n).collect();
        format!("{truncated}…")
    } else {
        format!("{s:<width$}")
    }
}

/// Everything except the flexible Description column: the 4 fixed columns plus
/// the 4 single-space gaps between the 5 columns.
/// The Type column for `app`: whatever its longest `[kind]` badge needs.
fn ty_w(app: &App) -> usize {
    filter_items(app)
        .iter()
        .skip(1)
        .map(|(label, ..)| label.chars().count() + 2)
        .max()
        .unwrap_or(0)
        .max(TY_W_MIN)
}

fn non_desc_w(ty_w: usize) -> usize {
    ty_w + 1 + MNE_W + 1 + OPS_W + 1 + 1 + EXP_W
}
/// Smallest Description width before the table scrolls horizontally.
const DESC_MIN: usize = 20;

/// Pick the Description width for `content_w` columns of space and report the
/// table's full natural width. Description flexes to fill spare room; once the
/// terminal is narrower than the natural minimum the table keeps all columns and
/// scrolls horizontally instead of hiding any (`natural_w > content_w`).
fn col_dims(content_w: usize, ty_w: usize) -> (usize, usize) {
    let fixed = non_desc_w(ty_w);
    if content_w > fixed + DESC_MIN {
        (content_w - fixed, content_w)
    } else {
        (DESC_MIN, fixed + DESC_MIN)
    }
}

fn render_col_header(desc_w: usize, ty_w: usize) -> Line<'static> {
    let h = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(format!("{:<ty_w$}", "Type"), h),
        Span::raw(" "),
        Span::styled(format!("{:<13}", "Mnemonic"), h),
        Span::raw(" "),
        Span::styled(format!("{:<21}", "Operands"), h),
        Span::raw(" "),
        Span::styled(pad_or_truncate("Description", desc_w), h),
        Span::raw(" "),
        Span::styled(format!("{:<26}", "Expands to"), h),
    ])
}

fn render_doc_row(app: &App, row: &InstructionDoc, desc_w: usize, ty_w: usize) -> Line<'static> {
    let color = ty_color(app, row.kind);
    let badge = format!("{:>ty_w$}", format!("[{}]", row.kind));

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(
            badge,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<13}", row.mnemonic),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    let ops_len = row.operands.chars().count();
    let mut ops_spans = color_text(row.operands, app.architecture.assembler());
    if ops_len < OPS_W {
        ops_spans.push(Span::raw(" ".repeat(OPS_W - ops_len)));
    }
    spans.extend(ops_spans);
    spans.push(Span::raw(" "));

    spans.push(Span::raw(pad_or_truncate(row.summary, desc_w)));
    spans.push(Span::raw(" "));

    if row.expands_to.is_empty() {
        spans.push(Span::raw(" ".repeat(EXP_W)));
    } else {
        let exp_text = format!("→ {}", row.expands_to);
        spans.push(Span::styled(
            pad_or_truncate(&exp_text, EXP_W),
            Style::default().fg(Color::Rgb(100, 100, 120)),
        ));
    }

    Line::from(spans)
}

/// What the operand placeholders in the table mean, listing only the ones this
/// architecture actually writes.
///
/// The legend was a fixed line naming RV32's `rd`, `frd` and `rs1/rs2`, which
/// explained nothing on a machine whose operands are an address and a
/// character.
fn operand_legend(app: &App) -> Line<'static> {
    /// (what to print, the tokens that make it apply, what it means)
    const ENTRIES: &[(&str, &[&str], &str)] = &[
        ("rd", &["rd", "rd2"], "dst"),
        ("rs1/rs2", &["rs", "rs1", "rs2", "rs3", "rt"], "src"),
        ("frd", &["frd", "frd2"], "float dst"),
        ("frs1/frs2", &["frs", "frs1", "frs2", "frs3"], "float src"),
        (
            "imm",
            &["imm", "imm12", "imm20", "shamt", "hi", "lo", "n"],
            "immediate",
        ),
        ("label", &["label"], "symbol"),
        ("address", &["address", "addr"], "memory address"),
        ("char", &["char", "ascii"], "character"),
    ];

    let assembler = app.architecture.assembler();
    let written: Vec<&str> = assembler
        .documented_instructions()
        .iter()
        .flat_map(|row| {
            row.operands
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        })
        .filter(|token| !token.is_empty())
        .collect();

    let mut spans = Vec::new();
    for (display, tokens, meaning) in ENTRIES {
        if !tokens.iter().any(|token| written.contains(token)) {
            continue;
        }
        let style = tokens
            .iter()
            .find_map(|token| style_for_token(token, assembler))
            .unwrap_or_default();
        spans.push(Span::styled((*display).to_string(), style));
        spans.push(Span::styled(format!("={meaning}  "), style::label()));
    }
    Line::from(spans)
}

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let search_bar_h: u16 = if app.docs.search_open { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(search_bar_h),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let tab_area = chunks[0];
    let meta_area = chunks[1];
    let search_area = chunks[2];
    let filter_area = chunks[3];
    let table_area = chunks[4];

    let search_hint = if app.docs.search_open {
        "  Ctrl+f=search"
    } else {
        ""
    };
    let filter_hint = if !app.docs.search_open {
        "  ←/→=filter  Space=toggle"
    } else {
        ""
    };
    let tab_hint = format!("{search_hint}{filter_hint}  ↑/↓=scroll");
    render_page_tabs(f, tab_area, app);
    render_tab_hint(f, tab_area, app, tab_hint);

    let meta_lines = vec![operand_legend(app), separator_line(area.width)];
    f.render_widget(Paragraph::new(meta_lines), meta_area);

    if app.docs.search_open {
        let bar_style = style::label().bg(Color::Rgb(30, 30, 50));
        let bar_line = Line::from(vec![
            Span::styled(
                " Find: ",
                Style::default()
                    .fg(theme::ACCENT)
                    .bg(Color::Rgb(30, 30, 50)),
            ),
            Span::styled(
                app.docs.search_query.clone(),
                Style::default()
                    .fg(theme::LABEL_Y)
                    .bg(Color::Rgb(30, 30, 50)),
            ),
            Span::styled("  Esc=close", style::label().bg(Color::Rgb(30, 30, 50))),
        ]);
        f.render_widget(Paragraph::new(bar_line).style(bar_style), search_area);

        let prefix_len = " Find: ".len() as u16;
        let cursor_x = (search_area.x + prefix_len + app.docs.search_query.chars().count() as u16)
            .min(search_area.x + search_area.width.saturating_sub(1));
        if search_area.height > 0 {
            f.set_cursor_position((cursor_x, search_area.y));
        }
    }

    render_filter_bar(f, filter_area, app);

    if table_area.height == 0 || table_area.width == 0 {
        app.docs.sb_v.set(None);
        app.docs.sb_h.set(None);
        return;
    }

    let rows = if app.docs.type_filter == 0 {
        vec![]
    } else {
        let q = if app.docs.search_open {
            app.docs.search_query.as_str()
        } else {
            ""
        };
        filtered_rows(app, q, app.docs.type_filter)
    };

    // Decide which bars are needed. Vertical: rows overflow the body height
    // (header + separator = 2 rows, minus a horizontal bar if present). Reserve
    // a right column for it. Horizontal: the table keeps every column and scrolls
    // when its natural width exceeds the space (no column hiding).
    let ty_w = ty_w(app);
    let h_bar_est =
        u16::from(col_dims(table_area.width as usize, ty_w).1 > table_area.width as usize);
    let body_h = table_area.height.saturating_sub(2 + h_bar_est) as usize;
    let needs_v = body_h > 0 && rows.len() > body_h;
    let content_w = (table_area.width as usize).saturating_sub(usize::from(needs_v));
    let (desc_w, natural_w) = col_dims(content_w, ty_w);
    let needs_h = natural_w > content_w;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),              // column header
            Constraint::Length(1),              // separator
            Constraint::Min(0),                 // data rows
            Constraint::Length(needs_h.into()), // horizontal scrollbar
        ])
        .split(table_area);
    let header_area = chunks[0];
    let sep_area = chunks[1];
    let data_area = chunks[2];
    let hbar_area = chunks[3];

    let content_w_u16 = content_w as u16;
    let viewport_h = data_area.height as usize;
    let max_h = natural_w.saturating_sub(content_w);
    let h_off = app.docs.h_scroll.min(max_h) as u16;

    // Header + separator scroll horizontally in lock-step with the rows.
    let text_rect = |a: Rect| Rect::new(a.x, a.y, content_w_u16, a.height);
    f.render_widget(
        Paragraph::new(render_col_header(desc_w, ty_w)).scroll((0, h_off)),
        text_rect(header_area),
    );
    f.render_widget(
        Paragraph::new(separator_line(natural_w as u16)).scroll((0, h_off)),
        text_rect(sep_area),
    );

    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                "  (no results — adjust filter or search query)",
                Style::default().fg(Color::DarkGray),
            )),
            data_area,
        );
        app.docs.sb_v.set(None);
        app.docs.sb_h.set(None);
        return;
    }

    let (start, end) = visible_window(rows.len(), viewport_h, app.docs.scroll);
    let lines: Vec<Line<'static>> = rows[start..end]
        .iter()
        .map(|r| render_doc_row(app, r, desc_w, ty_w))
        .collect();
    f.render_widget(
        Paragraph::new(lines).scroll((0, h_off)),
        text_rect(data_area),
    );

    // Vertical scrollbar in the reserved right column; register its track for
    // mouse drag (or clear it when absent).
    if needs_v {
        let sb_area = Rect::new(data_area.x, data_area.y, table_area.width, data_area.height);
        let max_v = rows.len().saturating_sub(viewport_h);
        vertical_scrollbar(f, sb_area, rows.len(), viewport_h, start);
        app.docs.sb_v.set(Some(SbGeom {
            start: data_area.y,
            len: data_area.height,
            cross: data_area.x + content_w_u16,
            content: rows.len(),
            viewport: viewport_h,
            offset: start,
            max: max_v,
        }));
    } else {
        app.docs.sb_v.set(None);
    }

    // Horizontal scrollbar along the bottom, spanning the content columns only.
    if needs_h {
        let hbar = Rect::new(hbar_area.x, hbar_area.y, content_w_u16, 1);
        horizontal_scrollbar(f, hbar, natural_w, content_w, h_off as usize);
        app.docs.sb_h.set(Some(SbGeom {
            start: hbar_area.x,
            len: content_w_u16,
            cross: hbar_area.y,
            content: natural_w,
            viewport: content_w,
            offset: h_off as usize,
            max: max_h,
        }));
    } else {
        app.docs.sb_h.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::DocsPage;
    use crate::ui::view::docs::{all_mask, visible_pages};

    fn app(id: &str) -> App {
        App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, id).unwrap()
    }

    /// The reference lists the loaded ISA's instructions. It used to hold one
    /// hard-coded RISC-V table, so every other backend was told to write
    /// `addi`.
    #[test]
    fn every_architecture_documents_its_own_instructions() {
        for (id, own, foreign) in [
            ("riscv32", "addi", "putc"),
            ("toy16", "putc", "addi"),
            ("sap", "lda", "addi"),
            ("x86_64", "syscall", "addi"),
        ] {
            let app = app(id);
            let rows = filtered_rows(&app, "", all_mask(&app));
            assert!(
                rows.iter().any(|row| row.mnemonic == own),
                "{id} does not document {own}"
            );
            assert!(
                !rows.iter().any(|row| row.mnemonic == foreign),
                "{id} documents {foreign}, which belongs to another ISA"
            );
        }
    }

    /// Every documented mnemonic must be one the assembler really takes —
    /// which is the point of the reference living next to the assembler. RV32
    /// is exempt: it documents pseudo-instructions and directives its
    /// `instruction_forms` table does not list.
    #[test]
    fn documented_mnemonics_are_ones_the_assembler_accepts() {
        for id in crate::arch::registry().ids() {
            if id == "riscv32" {
                continue;
            }
            let assembler = crate::arch::lookup(id).unwrap().assembler();
            for row in assembler.documented_instructions() {
                assert!(
                    !assembler.instruction_forms(row.mnemonic).is_empty(),
                    "{id}: {} is documented but the assembler has no form for it",
                    row.mnemonic
                );
            }
        }
    }

    /// A page is offered only when the architecture has the thing it explains,
    /// so SAP no longer gets RV32's syscall table or memory map.
    #[test]
    fn pages_follow_what_the_architecture_supports() {
        let rv32 = visible_pages(&app("riscv32"));
        assert!(rv32.contains(&DocsPage::Syscalls));
        assert!(rv32.contains(&DocsPage::MemoryMap));

        for id in ["toy16", "sap"] {
            let pages = visible_pages(&app(id));
            assert!(pages.contains(&DocsPage::InstrRef), "{id}");
            assert!(!pages.contains(&DocsPage::Syscalls), "{id}");
            assert!(!pages.contains(&DocsPage::MemoryMap), "{id}");
        }

        // x86-64 has syscalls but no cache and no paging, so it gets a
        // different set again — which is the whole point of gating per page.
        let x86 = visible_pages(&app("x86_64"));
        assert!(x86.contains(&DocsPage::Syscalls));
        assert!(!x86.contains(&DocsPage::FcacheRef));
        assert!(!x86.contains(&DocsPage::MemoryMap));
    }

    /// Switching architecture must leave the tab on a page that is drawn and a
    /// filter mask whose bits mean what this ISA says they mean.
    #[test]
    fn switching_architecture_leaves_the_tab_consistent() {
        let mut app = app("riscv32");
        app.docs.page = DocsPage::Syscalls;
        app.activate_architecture("sap", true).unwrap();

        assert!(visible_pages(&app).contains(&app.docs.page));
        assert_eq!(app.docs.type_filter, all_mask(&app));
        assert!(!filtered_rows(&app, "", app.docs.type_filter).is_empty());
    }
}
