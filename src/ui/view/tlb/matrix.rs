// ui/view/tlb/matrix.rs — the TLB as a sets × ways matrix.
//
//   ╭─ TLB ─────────────────────────────────────── 8 sets × 4 ways · LRU ─╮
//   │ access 0x7fffeff8 · VPN 0x7fffe → set 6 · a 4 MiB page would be in 7 │
//   │  set   way 0                    way 1                    way 2      │
//   │    0   4K 00400→00400 R-X- 2    4K 10000→10000 RW-U 1    ·          │
//   │  ▸ 6   4K 00406→00406 RW-U 2    4K 7fffe→00040 RW-U 1 ●  ·          │
//   │ set 6 · way 1 ───────────────────────────────────────────────────── │
//   │ VPN 0x7fffe → PPN 0x00040 · 4 KiB · ASID 0 · RW-U · dirty           │
//   ╰──────────────────────────────────────────────────────────────────────╯
//
// A matrix that does not fit the width becomes a row of set tabs over the
// selected set's ways, one per line — a fully associative TLB is that list.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use raven_engine::capability::{
    AddressTranslation, TlbEntryView, TranslationOutcome, TranslationRecordView, TranslationScheme,
};

use super::{chip, hex, perms_text, place, size_long, size_short};
use crate::ui::app::{App, VmHit};
use crate::ui::theme;
use crate::ui::view::components::Toolbar;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

/// Columns of one matrix cell, its separator included.
const CELL_W: u16 = 25;
/// Columns of the set label in front of each row.
const SET_W: u16 = 7;
/// Rows under the matrix for the selected entry.
const DETAIL_H: u16 = 5;

/// What the current access did to a slot.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Touch {
    None,
    Hit,
    Installed,
}

fn touch(rec: Option<&TranslationRecordView>, slot: usize) -> Touch {
    let Some(rec) = rec else {
        return Touch::None;
    };
    if rec.installed_entry == Some(slot) {
        Touch::Installed
    } else if rec.hit_entry == Some(slot) && rec.outcome != TranslationOutcome::Fault {
        Touch::Hit
    } else {
        Touch::None
    }
}

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let Some(core) = app.translation() else {
        return;
    };
    let translation: &dyn AddressTranslation = &core;
    let sets = translation.tlb_sets().max(1);
    let len = translation.tlb_len();
    let ways = (len / sets).max(1);
    let geometry = format!(
        "{} entries · {sets} sets × {ways} ways · {}",
        len,
        translation.tlb_policy()
    );
    let inner = render_panel(f, area, panel::panel_state(super::core_title(app, "TLB"), geometry, PanelKind::Plain));
    if inner.height < 3 || inner.width < 24 {
        return;
    }

    if !translation.tlb_enabled() {
        render_tlb_off(f, inner, app);
        return;
    }
    if !translation.enabled() && translation.recent().is_empty() {
        super::render_empty(f, inner, app, "");
        return;
    }

    let scheme = translation.scheme();
    let current = app.vm_current();
    let cursor = app.vm_tlb_cursor();

    // ── Where the current access goes ──
    f.render_widget(
        Paragraph::new(access_line(&scheme, current.as_ref(), sets, inner.width >= 100)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let detail_h = DETAIL_H.min(inner.height.saturating_sub(4));
    let body = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2 + detail_h),
    );
    app.tlb.hit(VmHit::TlbArea, body);

    let fits = SET_W + CELL_W * ways as u16 <= inner.width;
    if fits && sets > 1 {
        render_matrix(f, body, app, translation, current.as_ref(), cursor);
    } else {
        render_set_list(f, body, app, translation, current.as_ref(), cursor);
    }

    if detail_h > 0 {
        let detail = Rect::new(
            inner.x,
            inner.y + inner.height - detail_h,
            inner.width,
            detail_h,
        );
        render_detail(f, detail, translation, &scheme, current.as_ref(), cursor);
    }
}

fn render_tlb_off(f: &mut Frame, inner: Rect, app: &App) {
    let y = inner.y + inner.height.saturating_sub(3) / 2;
    let mut bar = Toolbar::new();
    bar.action(
        VmHit::TlbOn(true),
        "turn the TLB on",
        chip(app, VmHit::TlbOn(true), false),
        theme::ACCENT,
    );
    let mut action = vec![Span::raw(" ")];
    action.extend(place(app, &bar, inner.x + 1, y + 2, |id| id));
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                " The TLB is off: every translation walks the page table, and nothing is kept.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(action),
        ]),
        Rect::new(inner.x, y, inner.width, 3.min(inner.height)),
    );
}

/// `access 0x7fffeff8 · VPN 0x7fffe → set 6 · a 4 MiB page would be in set 7`
fn access_line(
    scheme: &TranslationScheme,
    rec: Option<&TranslationRecordView>,
    sets: usize,
    wide: bool,
) -> Line<'static> {
    let Some(rec) = rec else {
        return Line::from(Span::styled(
            " no access yet — each one shows here with the set it lands in",
            style::label(),
        ));
    };
    let vpn = rec.virtual_address >> scheme.offset_bits;
    let mut spans = vec![
        Span::styled(" access ", style::label()),
        Span::styled(hex(rec.virtual_address, 8), style::value()),
        Span::styled(" · VPN ", style::label()),
        Span::styled(hex(vpn, super::vpn_digits(scheme)), style::value()),
    ];
    match rec.probed_sets.as_slice() {
        [(small, _), (mega, mega_size), ..] => {
            spans.push(Span::styled(format!(" mod {sets} → "), style::label()));
            spans.push(Span::styled(
                format!("set {small}"),
                Style::default().fg(theme::ACCENT).bold(),
            ));
            if mega != small && wide {
                spans.push(Span::styled(
                    format!(" · a {} page would be in set {mega}", size_long(*mega_size)),
                    style::label(),
                ));
            }
        }
        _ => spans.push(Span::styled(" · TLB not consulted", style::label())),
    }
    let (word, color) = super::outcome_badge(rec.outcome);
    spans.push(Span::styled(" · ", style::label()));
    spans.push(Span::styled(word, Style::default().fg(color).bold()));
    Line::from(spans)
}

/// One cell's text: `4K 7fffe→00040 RW-U 1`.
fn cell_text(entry: &TlbEntryView, scheme: &TranslationScheme) -> String {
    let digits = super::vpn_digits(scheme);
    let size = size_short(entry.coverage(scheme.page_size()));
    format!(
        "{size:>2} {:0digits$x}→{:0digits$x} {} {}",
        entry.virtual_page,
        entry.physical_page,
        perms_text(entry.permissions),
        entry.recency.map_or(String::new(), |r| r.to_string()),
    )
}

fn cell_spans(
    entry: &TlbEntryView,
    scheme: &TranslationScheme,
    touched: Touch,
    selected: bool,
    hovered: bool,
    width: usize,
) -> Vec<Span<'static>> {
    let (text, base) = if entry.valid {
        (cell_text(entry, scheme), style::value())
    } else {
        ("·".to_string(), Style::default().fg(theme::BORDER))
    };
    let (mark, mark_style) = match touched {
        Touch::Installed => (" ●", Style::default().fg(theme::ACCENT).bold()),
        Touch::Hit => (" ✓", Style::default().fg(theme::RUNNING).bold()),
        Touch::None => ("", Style::default()),
    };
    let mut style = match touched {
        Touch::Installed => Style::default().fg(theme::ACCENT).bold(),
        Touch::Hit => Style::default().fg(theme::RUNNING).bold(),
        Touch::None => base,
    };
    if selected {
        style = style.bg(theme::SEL_ROW_BG);
    } else if hovered {
        style = style.bg(theme::HOVER_ROW_BG);
    }
    let body = format!("{text}{mark}");
    let pad = width.saturating_sub(body.chars().count());
    vec![
        Span::styled(text, style),
        Span::styled(mark, mark_style.patch(style)),
        Span::styled(" ".repeat(pad), Style::default()),
    ]
}

/// Sets as rows, ways as columns.
fn render_matrix(
    f: &mut Frame,
    area: Rect,
    app: &App,
    translation: &dyn AddressTranslation,
    rec: Option<&TranslationRecordView>,
    cursor: Option<usize>,
) {
    let scheme = translation.scheme();
    let sets = translation.tlb_sets().max(1);
    let ways = (translation.tlb_len() / sets).max(1);
    let probed: Vec<usize> = rec
        .map(|r| r.probed_sets.iter().map(|&(s, _)| s).collect())
        .unwrap_or_default();

    let mut header = vec![Span::styled(format!("{:>5}  ", "set"), style::idle())];
    for way in 0..ways {
        header.push(Span::styled(
            format!("{:<w$}", format!("way {way}"), w = CELL_W as usize),
            style::idle(),
        ));
    }
    let mut lines = vec![Line::from(header)];

    let visible = usize::from(area.height.saturating_sub(1)).max(1);
    let focus_set = cursor.map_or(0, |slot| slot / ways);
    let start = keep_visible(app.tlb.tlb_scroll, focus_set, visible, sets);
    for set in start..(start + visible).min(sets) {
        let y = area.y + 1 + (set - start) as u16;
        let is_probed = probed.contains(&set);
        let mut spans = vec![Span::styled(
            format!("{:>1}{:>4}  ", if is_probed { "▸" } else { " " }, set),
            if is_probed {
                Style::default().fg(theme::ACCENT).bold()
            } else {
                Style::default().fg(theme::BORDER)
            },
        )];
        for way in 0..ways {
            let slot = set * ways + way;
            let Some(entry) = translation.tlb_entry(slot) else {
                continue;
            };
            let x = area.x + SET_W + way as u16 * CELL_W;
            let hit = VmHit::Slot(slot);
            app.tlb.hit(hit, Rect::new(x, y, CELL_W - 1, 1));
            spans.extend(cell_spans(
                &entry,
                &scheme,
                touch(rec, slot),
                cursor == Some(slot),
                app.tlb.is_hovered(hit),
                CELL_W as usize,
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

/// Set tabs over the selected set's ways, one per line.
fn render_set_list(
    f: &mut Frame,
    area: Rect,
    app: &App,
    translation: &dyn AddressTranslation,
    rec: Option<&TranslationRecordView>,
    cursor: Option<usize>,
) {
    let scheme = translation.scheme();
    let sets = translation.tlb_sets().max(1);
    let ways = (translation.tlb_len() / sets).max(1);
    let set = cursor.map_or(0, |slot| slot / ways);
    let mut lines = Vec::new();
    let mut top = area.y;

    if sets > 1 {
        let mut bar = Toolbar::with_gap(1);
        bar.text(vec![Span::styled("set", style::idle())]);
        for s in 0..sets {
            let hit = VmHit::SetTab(s);
            bar.value(hit, &s.to_string(), chip(app, hit, s == set), theme::ACCENT);
        }
        let mut spans = vec![Span::raw(" ")];
        spans.extend(place(app, &bar, area.x + 1, area.y, |id| id));
        lines.push(Line::from(spans));
        top += 1;
    }
    lines.push(Line::from(Span::styled(
        format!("{:>5}  {:<26}", "way", "size  VPN → PPN      perms   recency"),
        style::idle(),
    )));
    top += 1;

    let visible = usize::from(area.height.saturating_sub(top - area.y)).max(1);
    let focus_way = cursor.map_or(0, |slot| slot % ways);
    let start = keep_visible(0, focus_way, visible, ways);
    for way in start..(start + visible).min(ways) {
        let slot = set * ways + way;
        let Some(entry) = translation.tlb_entry(slot) else {
            continue;
        };
        let y = top + (way - start) as u16;
        let hit = VmHit::Slot(slot);
        app.tlb.hit(hit, Rect::new(area.x, y, area.width, 1));
        let mut spans = vec![Span::styled(
            format!("{way:>5}  "),
            Style::default().fg(theme::BORDER),
        )];
        spans.extend(cell_spans(
            &entry,
            &scheme,
            touch(rec, slot),
            cursor == Some(slot),
            app.tlb.is_hovered(hit),
            usize::from(area.width.saturating_sub(SET_W)),
        ));
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

/// The first row to show so `focus` stays in a `visible`-row window.
fn keep_visible(scroll: usize, focus: usize, visible: usize, total: usize) -> usize {
    let max_start = total.saturating_sub(visible);
    let mut start = scroll.min(max_start);
    if focus < start {
        start = focus;
    } else if focus >= start + visible {
        start = focus + 1 - visible;
    }
    start.min(max_start)
}

/// The selected entry in full, and what the marks mean.
fn render_detail(
    f: &mut Frame,
    area: Rect,
    translation: &dyn AddressTranslation,
    scheme: &TranslationScheme,
    rec: Option<&TranslationRecordView>,
    cursor: Option<usize>,
) {
    let width = area.width;
    let Some(slot) = cursor else {
        return;
    };
    let Some(entry) = translation.tlb_entry(slot) else {
        return;
    };
    let mut lines = vec![panel::section_rule(
        &format!(" set {} · way {}", entry.set, entry.way),
        width,
    )];
    if entry.valid {
        let size = entry.coverage(scheme.page_size());
        let digits = super::vpn_digits(scheme);
        let va = entry.virtual_page << scheme.offset_bits;
        let pa = entry.physical_page << scheme.offset_bits;
        let mut first = vec![
            Span::styled(" VPN ", style::label()),
            Span::styled(hex(entry.virtual_page, digits), style::value()),
            Span::styled(" → PPN ", style::label()),
            Span::styled(hex(entry.physical_page, digits), style::value()),
            Span::styled(
                format!(" · {} page · {}", size_long(size), perms_text(entry.permissions)),
                style::label(),
            ),
        ];
        if let Some(asid) = entry.address_space {
            first.push(Span::styled(
                if entry.global {
                    " · global (every ASID)".to_string()
                } else {
                    format!(" · ASID {asid}")
                },
                style::label(),
            ));
        }
        if entry.dirty {
            first.push(Span::styled(" · dirty", style::label()));
        }
        lines.push(Line::from(first));
        lines.push(Line::from(vec![
            Span::styled(" maps VA ", style::label()),
            Span::styled(
                format!("{}–{}", hex(va, 8), hex(va + size - 1, 8)),
                style::value(),
            ),
            Span::styled(" to PA ", style::label()),
            Span::styled(
                format!("{}–{}", hex(pa, 8), hex(pa + size - 1, 8)),
                style::value(),
            ),
        ]));
        let rank = entry.recency.unwrap_or(0);
        let policy = translation.tlb_policy();
        let victim = match policy.as_str() {
            "MRU" => "number 1 goes first",
            "Random" => "the victim is random",
            "Clock" => "a second chance by reference bit",
            _ => "the highest number goes first",
        };
        lines.push(Line::from(Span::styled(
            format!(" recency {rank} in its set · {policy}: {victim}"),
            style::idle(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                " empty — the next miss filed under set {} can use it without evicting anything",
                entry.set
            ),
            style::label(),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::raw(""));
    }
    let evicted = rec
        .and_then(|r| r.evicted.map(|old| (r.installed_entry, old)))
        .filter(|(slot_now, _)| *slot_now == Some(slot));
    let legend = match evicted {
        Some((_, old)) => format!(
            " ● installed now, evicting VPN {}   ✓ hit now   ▸ probed",
            hex(old.virtual_page, super::vpn_digits(scheme))
        ),
        None => " ● installed now   ✓ hit now   ▸ probed set   n = recency".to_string(),
    };
    lines.push(Line::from(Span::styled(legend, style::idle())));
    f.render_widget(Paragraph::new(lines), area);
}
