// ui/view/tlb/config.rs — how virtual memory is set up.
//
//   ╭─ Configuration ──────────────────────────────── every change applies at once ─╮
//   │ TRANSLATION ───────────────────────────  TLB ────────────────────────────────  │
//   │ mode      off   auto   program           state     on   off                    │
//   │           the simulator writes …        entries   4  8  16  32  64  128       │
//   │ PAGE MAP ──────────────────────────────  ways      1  2  4  8  all             │
//   │ perms     R  W  X  U                     policy    LRU  FIFO  Random  …        │
//   │ global    off                            cycles    hit 1   walk 20             │
//   │ ASID      0                                                                     │
//   ╰─────────────────────────────────────────────────────────────────────────────────╯
//
// Every option is on screen, the chosen one lit: a reader sees the choice
// instead of cycling through it to find out what there is.

use ratatui::{Frame, prelude::*, widgets::Paragraph};

use super::{hex, place, size_long};
use crate::falcon::cache::ReplacementPolicy;
use crate::falcon::mmu::VmMode;
use crate::ui::app::{App, VmField, VmHit};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{Toolbar, dense_value, edit_value};
use crate::ui::view::style;

/// Columns of a row label.
const LABEL_W: usize = 10;
/// Width a column needs before two of them sit side by side.
const COL_W: u16 = 54;

const ENTRY_CHOICES: [u16; 6] = [4, 8, 16, 32, 64, 128];
const WAY_CHOICES: [u8; 4] = [1, 2, 4, 8];
const POLICIES: [(ReplacementPolicy, &str); 6] = [
    (ReplacementPolicy::Lru, "LRU"),
    (ReplacementPolicy::Fifo, "FIFO"),
    (ReplacementPolicy::Random, "Random"),
    (ReplacementPolicy::Clock, "Clock"),
    (ReplacementPolicy::Mru, "MRU"),
    (ReplacementPolicy::Lfu, "LFU"),
];

/// Rows of one column, laid out top to bottom; each knows how to draw itself
/// at a given origin so its controls register where they land.
enum Row {
    Blank,
    Rule(&'static str),
    Text(Vec<Span<'static>>),
    /// An explanation under a control — the first thing to go when the
    /// screen is short.
    Note(String),
    Controls(&'static str, Toolbar<VmHit>),
}

/// Drop notes, then blank rows, until `rows` fit in `height`.
fn fit(rows: Vec<Row>, height: u16) -> Vec<Row> {
    let height = usize::from(height);
    if rows.len() <= height {
        return rows;
    }
    let rows: Vec<Row> = rows.into_iter().filter(|r| !matches!(r, Row::Note(_))).collect();
    if rows.len() <= height {
        return rows;
    }
    rows.into_iter().filter(|r| !matches!(r, Row::Blank)).collect()
}

/// A segmented control: every option a pill, the chosen one lit.
fn segmented(app: &App, options: impl IntoIterator<Item = (VmHit, String, bool)>) -> Toolbar<VmHit> {
    let mut bar = Toolbar::with_gap(1);
    for (hit, label, selected) in options {
        bar.span(
            hit,
            None,
            dense_value(&label, app.tlb.is_hovered(hit), selected, theme::ACCENT),
            true,
        );
    }
    bar
}

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel_state(
            "Configuration",
            "every change applies at once",
            PanelKind::Accent,
        ),
    );
    if inner.height == 0 || inner.width < 20 {
        return;
    }

    let left = translation_rows(app);
    let right = tlb_rows(app);
    if inner.width >= COL_W * 2 {
        let w = inner.width / 2;
        let (left, right) = (fit(left, inner.height), fit(right, inner.height));
        draw_rows(f, app, Rect::new(inner.x + 1, inner.y, w - 2, inner.height), &left);
        draw_rows(
            f,
            app,
            Rect::new(inner.x + w + 1, inner.y, inner.width - w - 2, inner.height),
            &right,
        );
    } else {
        let mut all = left;
        all.push(Row::Blank);
        all.extend(right);
        let all = fit(all, inner.height);
        draw_rows(
            f,
            app,
            Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height),
            &all,
        );
    }
}

fn draw_rows(f: &mut Frame, app: &App, area: Rect, rows: &[Row]) {
    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate().take(usize::from(area.height)) {
        let y = area.y + i as u16;
        lines.push(match row {
            Row::Blank => Line::raw(""),
            Row::Rule(name) => panel::section_rule(name, area.width),
            Row::Text(spans) => Line::from(spans.clone()),
            Row::Note(text) => Line::from(vec![
                Span::raw(" ".repeat(LABEL_W + 1)),
                Span::styled(text.clone(), style::idle()),
            ]),
            Row::Controls(label, bar) => {
                let mut spans = vec![Span::styled(format!("{label:<LABEL_W$}"), style::label())];
                spans.extend(place(app, bar, area.x + LABEL_W as u16, y, |id| id));
                Line::from(spans)
            }
        });
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn dim(text: impl Into<String>) -> Row {
    Row::Note(text.into())
}

/// A number field: its value, or the text being typed with a cursor.
fn number(app: &App, field: VmField) -> Span<'static> {
    let editing = app
        .tlb
        .edit
        .as_ref()
        .filter(|(f, _)| *f == field)
        .map(|(_, text)| text.as_str());
    edit_value(
        &app.vm_field_value(field),
        editing,
        app.tlb.is_hovered(VmHit::Edit(field)),
        theme::LABEL_Y,
    )
}

fn translation_rows(app: &App) -> Vec<Row> {
    let mode = app.vm_mode();
    let mut rows = vec![Row::Rule("TRANSLATION")];

    let modes = segmented(
        app,
        VmMode::ALL
            .into_iter()
            .map(|m| (VmHit::Mode(m), m.as_str().to_string(), m == mode)),
    );
    rows.push(Row::Controls("mode", modes));
    let about: &[&str] = match mode {
        VmMode::Off => &["addresses go straight to memory; nothing is translated"],
        VmMode::Auto => &[
            "the simulator writes an identity page table when a",
            "program loads, and translates even in M-mode",
        ],
        VmMode::Program => &[
            "your program writes satp and its own page tables;",
            "M-mode bypasses translation, as on real hardware",
        ],
    };
    rows.extend(about.iter().map(|line| dim(*line)));
    rows.push(Row::Blank);

    let auto = mode == VmMode::Auto;
    rows.push(Row::Rule(if auto {
        "PAGE MAP"
    } else {
        "PAGE MAP · auto mode only"
    }));
    let spec = app.tlb.page_map;
    let mut perms = Toolbar::with_gap(1);
    for (i, (on, name)) in [
        (spec.perms.r, "R"),
        (spec.perms.w, "W"),
        (spec.perms.x, "X"),
        (spec.perms.u, "U"),
    ]
    .into_iter()
    .enumerate()
    {
        let hit = VmHit::Perm(i);
        perms.span(
            hit,
            None,
            dense_value(name, app.tlb.is_hovered(hit), on, theme::RUNNING),
            auto,
        );
    }
    rows.push(Row::Controls("perms", perms));

    let mut global = Toolbar::new();
    global.span(
        VmHit::Global,
        None,
        dense_value(
            if spec.global { "on" } else { "off" },
            app.tlb.is_hovered(VmHit::Global),
            spec.global,
            theme::RUNNING,
        ),
        auto,
    );
    global.text(vec![Span::styled("every leaf is shared by all ASIDs", style::idle())]);
    rows.push(Row::Controls("global", global));

    let mut asid = Toolbar::new();
    asid.span(VmHit::Edit(VmField::Asid), None, number(app, VmField::Asid), auto);
    asid.text(vec![Span::styled("0–511, written into satp", style::idle())]);
    rows.push(Row::Controls("ASID", asid));
    rows.push(Row::Blank);

    let layout = app.rv32().and_then(|rv32| rv32.mem().mmu().auto_map);
    match layout {
        Some(layout) => {
            rows.push(Row::Text(vec![
                Span::styled(format!("{:<w$}", "tables", w = LABEL_W + 1), style::label()),
                Span::styled(
                    format!("{}–{}", hex(u64::from(layout.tables_lo), 8), hex(u64::from(layout.tables_hi), 8)),
                    style::value(),
                ),
                Span::styled(
                    format!(" · {}, below the stacks", size_long(u64::from(layout.bytes()))),
                    style::idle(),
                ),
            ]));
            let (lo, hi) = app.vm_window();
            if hi > lo {
                rows.push(Row::Text(vec![
                    Span::styled(format!("{:<w$}", "program", w = LABEL_W + 1), style::label()),
                    Span::styled(
                        format!("{}–{}", hex(u64::from(lo), 8), hex(u64::from(hi), 8)),
                        style::value(),
                    ),
                    Span::styled(" in 4 KiB pages · the rest in 4 MiB pages", style::idle()),
                ]));
            }
        }
        None if auto => rows.push(dim("written at the next load, or with install map")),
        None => {}
    }
    rows
}

fn tlb_rows(app: &App) -> Vec<Row> {
    let cfg = &app.tlb.pending;
    let on = app.session.tlb_enabled;
    let mut rows = vec![Row::Rule("TLB")];

    let mut state = segmented(
        app,
        [(true, "on"), (false, "off")]
            .into_iter()
            .map(|(value, label)| (VmHit::TlbOn(value), label.to_string(), on == value)),
    );
    if !on {
        state.text(vec![Span::styled("every access walks", style::idle())]);
    }
    rows.push(Row::Controls("state", state));

    let entries = segmented(
        app,
        ENTRY_CHOICES
            .into_iter()
            .map(|n| (VmHit::Entries(n), n.to_string(), cfg.entry_count == n)),
    );
    rows.push(Row::Controls("entries", entries));

    let full = u8::try_from(cfg.entry_count).unwrap_or(u8::MAX);
    let ways = segmented(
        app,
        WAY_CHOICES
            .into_iter()
            .filter(|&n| n < full)
            .map(|n| (VmHit::Ways(n), n.to_string(), cfg.associativity == n))
            .chain([(VmHit::Ways(full), "all".to_string(), cfg.associativity >= full)]),
    );
    rows.push(Row::Controls("ways", ways));
    let sets = (cfg.entry_count / u16::from(cfg.associativity.max(1))).max(1);
    rows.push(dim(if sets == 1 {
        "one set: fully associative".to_string()
    } else {
        format!("{sets} sets of {} ways", cfg.associativity)
    }));

    let policy = segmented(
        app,
        POLICIES
            .into_iter()
            .map(|(p, name)| (VmHit::Policy(p), name.to_string(), cfg.replacement == p)),
    );
    rows.push(Row::Controls("policy", policy));
    rows.push(dim(super::replacement_label(cfg.replacement)));

    let mut cycles = Toolbar::with_gap(1);
    cycles.text(vec![Span::styled(" hit", style::idle())]);
    cycles.span(
        VmHit::Edit(VmField::HitCycles),
        None,
        number(app, VmField::HitCycles),
        true,
    );
    cycles.text(vec![Span::styled("  walk", style::idle())]);
    cycles.span(
        VmHit::Edit(VmField::WalkCycles),
        None,
        number(app, VmField::WalkCycles),
        true,
    );
    rows.push(Row::Controls("cycles", cycles));
    rows.push(dim(if app.session.cache_enabled {
        "walk is the walker's own; each PTE read adds its cache cost"
    } else {
        "walk is the whole walk while the caches are off"
    }));
    rows.push(dim("entries, ways and policy rebuild the TLB empty"));
    if app.tlb.edit.is_some() {
        rows.push(dim("enter keeps · esc drops · tab next field"));
    }
    rows
}
