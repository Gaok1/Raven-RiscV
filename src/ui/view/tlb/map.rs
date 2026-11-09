// ui/view/tlb/map.rs — the page table, one column per level.
//
//   root 0x00ffc000 › [001] table 0x00ffa000 › [000] page        following #918
//   ╭─ VPN[1] · root ───────╮╭─ VPN[0] · 0x00ffa000 ──────╮╭─ PTE @ 0x00ffa000 ─╮
//   │   000      4M RWXU    ││ ▸ 000     4K R-X- 00400 ●  ││ 0x0010004b          │
//   │ ▸ 001      → table    ││   001     4K R-X- 00401    ││ PPN    RSW D A G … │
//   │   002–3ff  4M ×1022   ││   010–3ff invalid ×1008    ││ 00400  0   0 1 0 … │
//   ╰───────────────────────╯╰────────────────────────────╯╰─────────────────────╯
//
// The path of the current access is marked `▸` in every column; ↑↓ move
// within a column, → follows a pointer or opens a folded run, ← goes back.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use raven_engine::capability::{
    AddressTranslation, TableEntryKind, TableEntryView, TranslationScheme,
};

use super::{chip, hex, level_color, perms_text, place, size_long, size_short};
use crate::ui::app::{App, MapColumn, MapRow, VmHit};
use crate::ui::theme;
use crate::ui::view::components::Toolbar;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

const COLUMN_W: u16 = 34;
const DETAIL_MIN_W: u16 = 32;

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let Some(core) = app.translation() else {
        return;
    };
    let translation: &dyn AddressTranslation = &core;
    if area.height < 4 {
        return;
    }
    let columns = app.vm_map_columns();
    if columns.is_empty() {
        let inner = render_panel(f, area, panel::panel("Page table", PanelKind::Plain));
        super::render_empty(f, inner, app, "No page table to show.");
        return;
    }
    let scheme = translation.scheme();

    render_top_line(f, Rect::new(area.x, area.y, area.width, 1), app, &columns);
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);

    // As many level columns as fit beside the detail, keeping the focused one.
    let focus = app.tlb.map_column.min(columns.len() - 1);
    let fit = usize::from(body.width.saturating_sub(DETAIL_MIN_W) / COLUMN_W).max(1);
    let first = (focus + 1).saturating_sub(fit);
    let shown: Vec<&MapColumn> = columns.iter().skip(first).take(fit).collect();

    let mut x = body.x;
    for column in &shown {
        let rect = Rect::new(x, body.y, COLUMN_W.min(body.width), body.height);
        render_column(f, rect, app, translation, &scheme, column, column.level == focus);
        x += COLUMN_W;
    }
    if x + 8 < body.x + body.width {
        let rect = Rect::new(x, body.y, body.x + body.width - x, body.height);
        render_detail(f, rect, translation, &scheme, &columns[focus]);
    }
}

/// `root 0x00ffc000 › [001] table 0x00ffa000 › [000] page`, and on the right
/// what the map is tracing.
fn render_top_line(f: &mut Frame, area: Rect, app: &App, columns: &[MapColumn]) {
    let mut spans = vec![Span::raw(" ")];
    if let Some(query) = &app.tlb.map_query {
        spans.push(Span::styled("go to address  ", style::label()));
        spans.push(Span::styled(
            format!("0x{query}█"),
            Style::default().fg(theme::TEXT).bold(),
        ));
        spans.push(Span::styled(
            "   enter traces it · esc cancels",
            style::idle(),
        ));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    spans.push(Span::styled("root ", style::label()));
    spans.push(Span::styled(hex(columns[0].table, 8), style::value()));
    for column in columns {
        let Some(entry) = column.selected_entry() else {
            continue;
        };
        spans.push(Span::styled(" › ", Style::default().fg(theme::BORDER)));
        spans.push(Span::styled(
            format!("[{:03x}]", entry.index),
            Style::default().fg(level_color(column.level)).bold(),
        ));
        let what = match entry.kind {
            TableEntryKind::Pointer { table } => format!(" table {}", hex(table, 8)),
            TableEntryKind::Leaf { frame, .. } => format!(" page {}", hex(frame, 8)),
            TableEntryKind::Invalid => " invalid".into(),
            TableEntryKind::Reserved => " reserved".into(),
        };
        spans.push(Span::styled(what, style::label()));
    }

    // Right side: what is traced, and how to change it.
    let mut bar = Toolbar::new();
    let tracing = match (app.tlb.map_target, app.vm_current()) {
        (Some(target), _) => format!("tracing {}", hex(target, 8)),
        (None, Some(rec)) if rec.seq > 0 => {
            format!("following #{} · {}", rec.seq, hex(rec.virtual_address, 8))
        }
        _ => String::new(),
    };
    if !tracing.is_empty() {
        bar.text(vec![Span::styled(tracing, style::idle())]);
    }
    let pinned = app.tlb.map_target.is_some() || app.tlb.map_cursor.iter().any(Option::is_some);
    if pinned {
        bar.action(
            VmHit::MapFollow,
            "g current access",
            chip(app, VmHit::MapFollow, false),
            theme::ACCENT,
        );
    }
    bar.action(
        VmHit::MapQuery,
        "/ go to address",
        chip(app, VmHit::MapQuery, false),
        theme::ACCENT,
    );
    let used: u16 = spans.iter().map(|s| s.width() as u16).sum();
    let bar_x = (area.x + area.width).saturating_sub(bar.width() + 1);
    if bar_x > area.x + used + 2 {
        spans.push(Span::raw(" ".repeat(usize::from(bar_x - area.x - used))));
        spans.extend(place(app, &bar, bar_x, area.y, |id| id));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Pages the TLB holds, as `(page number >> superpage bits, superpage bits)`.
fn tlb_pages(translation: &dyn AddressTranslation) -> Vec<(u64, u8)> {
    (0..translation.tlb_len())
        .filter_map(|i| translation.tlb_entry(i))
        .filter(|e| e.valid)
        .map(|e| (e.virtual_page >> e.superpage_bits, e.superpage_bits))
        .collect()
}

fn in_tlb(
    pages: &[(u64, u8)],
    scheme: &TranslationScheme,
    va: u64,
    coverage: u64,
) -> bool {
    let bits = (coverage / scheme.page_size()).trailing_zeros() as u8;
    let vpn = va >> scheme.offset_bits;
    pages.contains(&(vpn >> bits, bits))
}

fn render_column(
    f: &mut Frame,
    area: Rect,
    app: &App,
    translation: &dyn AddressTranslation,
    scheme: &TranslationScheme,
    column: &MapColumn,
    focused: bool,
) {
    let level = &scheme.levels[column.level];
    let title = if column.level == 0 {
        format!("{} · root", level.name)
    } else {
        format!("{} · {}", level.name, hex(column.table, 8))
    };
    let kind = if focused {
        PanelKind::Accent
    } else {
        PanelKind::Plain
    };
    let inner = render_panel(f, area, panel::panel(title, kind));
    if inner.height == 0 {
        return;
    }
    app.tlb.hit(VmHit::MapColumn(column.level), inner);

    let pages = tlb_pages(translation);
    let visible = usize::from(inner.height);
    let start = {
        let max_start = column.rows.len().saturating_sub(visible);
        let mut start = app.tlb.map_scroll[column.level].min(max_start);
        if column.selected < start {
            start = column.selected;
        } else if column.selected >= start + visible {
            start = column.selected + 1 - visible;
        }
        start.min(max_start)
    };

    let mut lines = Vec::new();
    for (i, row) in column.rows.iter().enumerate().skip(start).take(visible) {
        let y = inner.y + (i - start) as u16;
        let hit = VmHit::MapRow {
            column: column.level,
            row: i,
        };
        app.tlb.hit(hit, Rect::new(inner.x, y, inner.width, 1));
        let on_path = column.path.is_some_and(|p| row.contains(p));
        let selected = i == column.selected;
        let mut line = row_line(row, column, scheme, &pages, on_path, usize::from(inner.width));
        if selected && focused {
            line = line.style(Style::default().bg(theme::SEL_ROW_BG));
        } else if selected {
            line = line.style(Style::default().bg(theme::BG_RAISED));
        } else if app.tlb.is_hovered(hit) {
            line = line.style(Style::default().bg(theme::HOVER_ROW_BG));
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn row_line(
    row: &MapRow,
    column: &MapColumn,
    scheme: &TranslationScheme,
    pages: &[(u64, u8)],
    on_path: bool,
    width: usize,
) -> Line<'static> {
    let color = level_color(column.level);
    let coverage = scheme.levels[column.level].coverage;
    let marker = Span::styled(
        if on_path { "▸ " } else { "  " },
        Style::default().fg(theme::ACCENT).bold(),
    );
    let index_style = if on_path {
        Style::default().fg(color).bold()
    } else {
        Style::default().fg(color)
    };
    let mut spans = vec![marker];
    match row {
        MapRow::One(entry) => {
            spans.push(Span::styled(format!("{:03x}       ", entry.index), index_style));
            spans.extend(entry_summary(entry, column, scheme, pages, coverage));
        }
        MapRow::Run { first, last, count } => {
            spans.push(Span::styled(
                format!("{:03x}–{:03x}   ", first.index, last.index),
                index_style,
            ));
            match first.kind {
                TableEntryKind::Leaf { permissions, .. } => spans.push(Span::styled(
                    format!("{} {} ×{count}", size_short(coverage), perms_text(permissions)),
                    style::label(),
                )),
                _ => spans.push(Span::styled(
                    format!("invalid ×{count}"),
                    Style::default().fg(theme::BORDER),
                )),
            }
        }
    }
    let used: usize = spans.iter().map(|s| s.width()).sum();
    if used > width {
        // Too narrow for the summary: keep the index, drop the rest.
        spans.truncate(2);
    }
    Line::from(spans)
}

fn entry_summary(
    entry: &TableEntryView,
    column: &MapColumn,
    scheme: &TranslationScheme,
    pages: &[(u64, u8)],
    coverage: u64,
) -> Vec<Span<'static>> {
    match entry.kind {
        TableEntryKind::Invalid => vec![Span::styled("invalid", Style::default().fg(theme::BORDER))],
        TableEntryKind::Reserved => vec![Span::styled("reserved", style::danger())],
        TableEntryKind::Pointer { table } => vec![
            Span::styled("→ table ", style::label()),
            Span::styled(hex(table, 8), style::value()),
        ],
        TableEntryKind::Leaf {
            frame, permissions, ..
        } => {
            let va = column.va_base + entry.index * coverage;
            let mut spans = vec![
                Span::styled(
                    format!("{} {} ", size_short(coverage), perms_text(permissions)),
                    style::label(),
                ),
                Span::styled(
                    format!("{:05x}", frame >> scheme.offset_bits),
                    style::value(),
                ),
            ];
            if in_tlb(pages, scheme, va, coverage) {
                spans.push(Span::styled(" ●", Style::default().fg(theme::ACCENT).bold()));
            }
            spans
        }
    }
}

/// The selected entry, decoded field by field.
fn render_detail(
    f: &mut Frame,
    area: Rect,
    translation: &dyn AddressTranslation,
    scheme: &TranslationScheme,
    column: &MapColumn,
) {
    let Some(row) = column.rows.get(column.selected) else {
        return;
    };
    let coverage = scheme.levels[column.level].coverage;
    let title = match row {
        MapRow::One(entry) => format!("PTE @ {}", hex(entry.address, 8)),
        MapRow::Run { first, last, .. } => {
            format!("entries {:03x}–{:03x}", first.index, last.index)
        }
    };
    let inner = render_panel(f, area, panel::panel(title, PanelKind::Plain));
    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    match row {
        MapRow::Run { first, last, count } => {
            let va_lo = column.va_base + first.index * coverage;
            let va_hi = column.va_base + (last.index + 1) * coverage - 1;
            lines.push(Line::from(Span::styled(
                format!(" {count} entries alike"),
                Style::default().fg(theme::TEXT).bold(),
            )));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled(" VA ", style::label()),
                Span::styled(format!("{}–{}", hex(va_lo, 8), hex(va_hi, 8)), style::value()),
            ]));
            match first.kind {
                TableEntryKind::Leaf {
                    frame, permissions, ..
                } => {
                    lines.push(Line::from(vec![
                        Span::styled(" PA ", style::label()),
                        Span::styled(
                            format!("{}–{}", hex(frame, 8), hex(frame + coverage * *count as u64 - 1, 8)),
                            style::value(),
                        ),
                    ]));
                    lines.push(Line::from(Span::styled(
                        format!(
                            " {} pages · {}",
                            size_long(coverage),
                            perms_text(permissions)
                        ),
                        style::label(),
                    )));
                }
                _ => lines.push(Line::from(Span::styled(
                    " nothing mapped: an access here faults",
                    style::label(),
                ))),
            }
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                " → or enter lists them one by one",
                style::idle(),
            )));
        }
        MapRow::One(entry) => lines.extend(entry_detail(entry, column, scheme, translation, coverage)),
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn entry_detail(
    entry: &TableEntryView,
    column: &MapColumn,
    scheme: &TranslationScheme,
    translation: &dyn AddressTranslation,
    coverage: u64,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(" value ", style::label()),
        Span::styled(
            hex(entry.raw, usize::from(scheme.entry_bits).div_ceil(4)),
            Style::default().fg(theme::TEXT).bold(),
        ),
    ])];
    lines.push(Line::raw(""));

    // Field names over their values, one column each.
    let mut names = vec![Span::raw(" ")];
    let mut values = vec![Span::raw(" ")];
    for field in &scheme.entry_fields {
        let value = field.extract(entry.raw);
        let text = if field.width() > 4 {
            hex(value, usize::from(field.width()).div_ceil(4))
        } else {
            format!("{value:b}")
        };
        let w = field.name.len().max(text.len());
        let lit = value != 0 && field.width() == 1;
        names.push(Span::styled(format!("{:<w$} ", field.name), style::idle()));
        values.push(Span::styled(
            format!("{text:<w$} "),
            if lit {
                Style::default().fg(theme::RUNNING).bold()
            } else {
                style::value()
            },
        ));
    }
    lines.push(Line::from(names));
    lines.push(Line::from(values));
    lines.push(Line::raw(""));

    let va = column.va_base + entry.index * coverage;
    match entry.kind {
        TableEntryKind::Invalid => lines.push(Line::from(Span::styled(
            " V=0: nothing is mapped — an access here page-faults",
            style::label(),
        ))),
        TableEntryKind::Reserved => lines.push(Line::from(Span::styled(
            " W=1 with R=0 is reserved — an access here page-faults",
            style::danger(),
        ))),
        TableEntryKind::Pointer { table } => {
            lines.push(Line::from(vec![
                Span::styled(" points to the table at ", style::label()),
                Span::styled(hex(table, 8), style::value()),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    " which maps VA {}–{}",
                    hex(va, 8),
                    hex(va + coverage - 1, 8)
                ),
                style::label(),
            )));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(" → opens it", style::idle())));
        }
        TableEntryKind::Leaf {
            frame,
            permissions,
            accessed,
            dirty,
            global,
        } => {
            lines.push(Line::from(vec![
                Span::styled(" maps VA ", style::label()),
                Span::styled(format!("{}–{}", hex(va, 8), hex(va + coverage - 1, 8)), style::value()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("   to PA ", style::label()),
                Span::styled(
                    format!("{}–{}", hex(frame, 8), hex(frame + coverage - 1, 8)),
                    style::value(),
                ),
            ]));
            let mut facts = vec![size_long(coverage) + " page", perms_text(permissions)];
            if global {
                facts.push("global".into());
            }
            facts.push(if accessed { "accessed" } else { "not yet accessed" }.into());
            if dirty {
                facts.push("dirty".into());
            }
            if in_tlb(&tlb_pages(translation), scheme, va, coverage) {
                facts.push("● in the TLB".into());
            }
            lines.push(Line::from(Span::styled(
                format!(" {}", facts.join(" · ")),
                style::label(),
            )));
        }
    }
    lines
}
