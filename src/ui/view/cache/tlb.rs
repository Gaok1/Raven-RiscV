// ui/view/cache/tlb.rs — TLB subtab (Stats / Config / Entries) inside Cache tab.
use ratatui::{
    Frame,
    prelude::*,
    widgets::{
        Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph,
        Row, Table,
    },
};

use crate::falcon::cache::ReplacementPolicy;
use crate::ui::app::{App, CacheHoverTarget, TlbConfigField, TlbSubview};
use crate::ui::theme;
use crate::ui::view::components::{dense_action, dense_value};

pub(super) fn render_tlb(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_subview_header(f, layout[0], app);
    match app.cache.tlb_subview {
        TlbSubview::Stats => render_stats(f, layout[1], app),
        TlbSubview::Config => render_config(f, layout[1], app),
        TlbSubview::Entries => render_entries(f, layout[1], app),
    }
}

fn render_subview_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style = btn_style(
        matches!(app.cache.tlb_subview, TlbSubview::Stats),
        matches!(app.cache.hover, Some(CacheHoverTarget::TlbSubviewStats)),
    );
    let config_style = btn_style(
        matches!(app.cache.tlb_subview, TlbSubview::Config),
        matches!(app.cache.hover, Some(CacheHoverTarget::TlbSubviewConfig)),
    );
    let entries_style = btn_style(
        matches!(app.cache.tlb_subview, TlbSubview::Entries),
        matches!(app.cache.hover, Some(CacheHoverTarget::TlbSubviewEntries)),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Unified TLB",
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(area);
    let row_y = inner.y;
    let mut x = inner.x + 1;
    let stats_x0 = x;
    x += "stats".len() as u16;
    let stats_x1 = x;
    x += 3;
    let config_x0 = x;
    x += "config".len() as u16;
    let config_x1 = x;
    x += 3;
    let entries_x0 = x;
    x += "entries".len() as u16;
    let entries_x1 = x;
    app.cache
        .tlb_subview_stats_btn
        .set((row_y, stats_x0, stats_x1));
    app.cache
        .tlb_subview_config_btn
        .set((row_y, config_x0, config_x1));
    app.cache
        .tlb_subview_entries_btn
        .set((row_y, entries_x0, entries_x1));

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("stats", stats_style),
        Span::raw("   "),
        Span::styled("config", config_style),
        Span::raw("   "),
        Span::styled("entries", entries_style),
        Span::raw("   "),
        Span::styled(
            if app.run.vm_enabled {
                "vm=on"
            } else {
                "vm=off (toggle in Settings)"
            },
            Style::default().fg(if app.run.vm_enabled {
                theme::RUNNING
            } else {
                theme::PAUSED
            }),
        ),
    ]);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

// ── Stats ────────────────────────────────────────────────────────────────────

fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(area);

    render_stats_metrics(f, layout[0], app);
    render_hit_chart(f, layout[1], app);
}

fn render_stats_metrics(f: &mut Frame, area: Rect, app: &App) {
    let mmu = app.run.mem.mmu();
    let stats = &mmu.tlb.stats;
    let total = stats.hits + stats.misses;
    let hit_rate = if total == 0 {
        0.0
    } else {
        stats.hits as f64 / total as f64 * 100.0
    };
    let valid_entries = mmu.tlb.entries.iter().filter(|e| e.valid).count();
    let lines = vec![
        Line::from(vec![
            Span::styled(" Hits:       ", Style::default().fg(theme::LABEL)),
            Span::styled(format!("{}", stats.hits), Style::default().fg(theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" Misses:     ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", stats.misses),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Hit Rate:   ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{:.1}%", hit_rate),
                Style::default().fg(if hit_rate >= 80.0 {
                    theme::RUNNING
                } else if hit_rate >= 50.0 {
                    theme::ACCENT
                } else {
                    theme::PAUSED
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Evictions:  ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", stats.evictions),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Page Faults:", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!(" {}", stats.page_faults),
                Style::default().fg(if stats.page_faults > 0 {
                    theme::DANGER
                } else {
                    theme::TEXT
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Valid Entries: ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{} / {}", valid_entries, mmu.tlb.entries.len()),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Sets:       ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", mmu.tlb.num_sets()),
                Style::default().fg(theme::BORDER),
            ),
            Span::raw("   "),
            Span::styled(" Ways:       ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", mmu.tlb.config.associativity),
                Style::default().fg(theme::BORDER),
            ),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Metrics", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_hit_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Hit Rate History",
            Style::default().fg(theme::LABEL),
        ));

    let pts: Vec<(f64, f64)> = app.run.mem.mmu().tlb.stats.history.iter().copied().collect();
    if pts.is_empty() || area.height < 5 {
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if !app.run.vm_enabled {
                    "  (enable Virtual Memory in Settings to populate the TLB)"
                } else {
                    "  (no data yet — run a program that touches paged memory)"
                },
                Style::default().fg(theme::LABEL),
            ))),
            inner,
        );
        return;
    }
    let x_min = pts.first().map(|p| p.0).unwrap_or(0.0);
    let x_max = pts.last().map(|p| p.0).unwrap_or(1.0).max(x_min + 1.0);
    let datasets = vec![
        Dataset::default()
            .name("hit %")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::RUNNING))
            .data(&pts),
    ];
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{:.0}", x_min)),
                    Span::raw(format!("{:.0}", x_max)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("100%")]),
        );
    f.render_widget(chart, area);
}

// ── Config ───────────────────────────────────────────────────────────────────

fn render_config(f: &mut Frame, area: Rect, app: &App) {
    app.cache.tlb_config_hitboxes.set([(0, 0, 0); 5]);
    app.cache.tlb_preset_btns.set([(0, 0, 0); 3]);
    app.cache.tlb_apply_btn.set((0, 0, 0));
    app.cache.tlb_flush_btn.set((0, 0, 0));

    let col_w = area.width.min(60);
    let col_x = area.x + (area.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, area.y, col_w, area.height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "TLB Config",
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(col_area);
    f.render_widget(block, col_area);
    if inner.height == 0 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3), // presets
            Constraint::Length(3), // apply + flush
        ])
        .split(inner);

    record_config_hitboxes(app, layout[0]);
    render_fields(f, layout[0], app);
    render_presets(f, layout[1], app);
    render_apply_row(f, layout[2], app);
}

fn record_config_hitboxes(app: &App, area: Rect) {
    let mut hits = [(0, 0, 0); 5];
    for &field in TlbConfigField::all_editable() {
        let row_y = area.y.saturating_add(field.list_row() as u16);
        if row_y < area.y.saturating_add(area.height) {
            hits[field.hitbox_index()] = (row_y, area.x, area.x.saturating_add(area.width));
        }
    }
    app.cache.tlb_config_hitboxes.set(hits);
}

fn render_fields(f: &mut Frame, area: Rect, app: &App) {
    let pending = &app.cache.pending_tlb;
    let active = app.cache.tlb_edit_field;
    let edit_buf = app.cache.tlb_edit_buf.as_str();
    let hovered = match app.cache.hover {
        Some(CacheHoverTarget::TlbConfigField(f)) => Some(f),
        _ => None,
    };

    let current = &app.run.mem.mmu().tlb.config;
    let value_color = |same: bool| if same { theme::TEXT } else { theme::LABEL_Y };

    let entry_ok = pending.entry_count >= pending.associativity as u16;
    let assoc_ok = pending.associativity >= 1;
    let mark = |ok: bool| if ok { "" } else { " ✗" };

    let field_item = |field: TlbConfigField,
                      label: &'static str,
                      value: String,
                      same: bool|
     -> ListItem<'static> {
        let label_style = if active == Some(field) {
            Style::default().fg(theme::ACCENT).bold()
        } else if hovered == Some(field) {
            Style::default().fg(theme::TEXT).bold()
        } else {
            Style::default().fg(theme::LABEL)
        };
        if active == Some(field) {
            if field.is_numeric() {
                let display = format!("{edit_buf}█");
                ListItem::new(Line::from(vec![
                    Span::styled(label, label_style),
                    dense_value(&display, false, true, theme::ACCENT),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(label, label_style),
                    dense_value(&format!("< {value} >"), false, true, theme::ACCENT),
                ]))
            }
        } else {
            ListItem::new(Line::from(vec![
                Span::styled(label, label_style),
                dense_value(&value, hovered == Some(field), true, value_color(same)),
            ]))
        }
    };

    // Computed Sets readout (row 2).
    let assoc = pending.associativity.max(1) as usize;
    let raw = (pending.entry_count.max(1) as usize).next_power_of_two().max(assoc);
    let n_entries = ((raw + assoc - 1) / assoc) * assoc;
    let n_sets = (n_entries / assoc).max(1);

    let items: Vec<ListItem> = vec![
        field_item(
            TlbConfigField::EntryCount,
            "  Entries:       ",
            format!("{}{}", pending.entry_count, mark(entry_ok)),
            pending.entry_count == current.entry_count,
        ),
        field_item(
            TlbConfigField::Associativity,
            "  Associativity: ",
            format!("{}-way{}", pending.associativity, mark(assoc_ok)),
            pending.associativity == current.associativity,
        ),
        ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(theme::LABEL)),
            Span::styled(format!("{}", n_sets), Style::default().fg(theme::BORDER)),
        ])),
        field_item(
            TlbConfigField::Replacement,
            "  Replacement:   ",
            replacement_label(pending.replacement).to_string(),
            pending.replacement == current.replacement,
        ),
        field_item(
            TlbConfigField::HitLatency,
            "  Hit Latency:   ",
            format!("{} cyc", pending.hit_latency),
            pending.hit_latency == current.hit_latency,
        ),
        field_item(
            TlbConfigField::MissPenalty,
            "  Miss Penalty:  ",
            format!("{} cyc", pending.miss_penalty),
            pending.miss_penalty == current.miss_penalty,
        ),
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(Span::styled(
            if active.is_some() {
                "  Enter=confirm  Esc=cancel  ←→=cycle  Tab/↑↓=move"
            } else {
                "  Click to edit  ←→=cycle"
            },
            Style::default().fg(theme::LABEL),
        ))),
    ];
    f.render_widget(List::new(items), area);
}

fn render_presets(f: &mut Frame, area: Rect, app: &App) {
    let hovered = match app.cache.hover {
        Some(CacheHoverTarget::TlbPreset(i)) => Some(i),
        _ => None,
    };
    let style = |on: bool| {
        if on {
            Style::default().fg(theme::TEXT).bold()
        } else {
            Style::default().fg(theme::ACCENT).bold()
        }
    };
    let labels = ["small 16", "med 32", "large 64"];
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("presets", Style::default().fg(theme::IDLE)),
        Span::raw(" "),
        Span::styled(labels[0], style(hovered == Some(0))),
        Span::raw(" "),
        Span::styled(labels[1], style(hovered == Some(1))),
        Span::raw(" "),
        Span::styled(labels[2], style(hovered == Some(2))),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let x0 = inner.x + 9;
    let mut x = x0;
    let mut btns = [(0u16, 0u16, 0u16); 3];
    for (i, lbl) in labels.iter().enumerate() {
        btns[i] = (inner.y, x, x + lbl.len() as u16);
        x += lbl.len() as u16 + 1;
    }
    app.cache.tlb_preset_btns.set(btns);
    f.render_widget(Paragraph::new(line), inner);
}

fn render_apply_row(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(ref err) = app.cache.tlb_config_error {
        Line::from(Span::styled(
            format!(" ✗ {err}"),
            Style::default().fg(theme::DANGER),
        ))
    } else if let Some(ref status) = app.cache.tlb_config_status {
        Line::from(Span::styled(
            format!(" ✓ {status}"),
            Style::default().fg(theme::RUNNING),
        ))
    } else {
        Line::from(vec![
            Span::raw(" "),
            dense_action(
                "apply",
                theme::RUNNING,
                matches!(app.cache.hover, Some(CacheHoverTarget::TlbApply)),
            ),
            Span::raw("   "),
            dense_action(
                "flush tlb",
                theme::DANGER,
                matches!(app.cache.hover, Some(CacheHoverTarget::TlbFlush)),
            ),
        ])
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.cache
        .tlb_apply_btn
        .set((inner.y, inner.x + 1, inner.x + 1 + "apply".len() as u16));
    let flush_x0 = inner.x + 1 + "apply".len() as u16 + 3;
    app.cache.tlb_flush_btn.set((
        inner.y,
        flush_x0,
        flush_x0 + "flush tlb".len() as u16,
    ));
    f.render_widget(Paragraph::new(line), inner);
}

// ── Entries ──────────────────────────────────────────────────────────────────

fn render_entries(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "TLB Entries",
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem.mmu();
    let rows: Vec<Row> = mmu
        .tlb
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mark = |on: bool, c: &str| {
                if on {
                    Span::styled(c.to_string(), Style::default().fg(theme::RUNNING))
                } else {
                    Span::styled("-".to_string(), Style::default().fg(theme::IDLE))
                }
            };
            let perms = Line::from(vec![
                mark(e.perms.r, "R"),
                mark(e.perms.w, "W"),
                mark(e.perms.x, "X"),
                mark(e.perms.u, "U"),
            ]);
            if e.valid {
                Row::new(vec![
                    Line::from(Span::styled(
                        format!("{i:>3}"),
                        Style::default().fg(theme::BORDER),
                    )),
                    Line::from(Span::styled(
                        format!("0x{:05x}", e.vpn),
                        Style::default().fg(theme::TEXT),
                    )),
                    Line::from(Span::styled(
                        format!("0x{:05x}", e.ppn),
                        Style::default().fg(theme::TEXT),
                    )),
                    perms,
                    Line::from(Span::styled(
                        format!("{}", e.asid),
                        Style::default().fg(theme::LABEL),
                    )),
                    Line::from(mark(e.global, "G")),
                    Line::from(mark(e.accessed, "A")),
                    Line::from(mark(e.dirty, "D")),
                    Line::from(mark(e.megapage, "M")),
                ])
            } else {
                Row::new(vec![
                    Line::from(Span::styled(
                        format!("{i:>3}"),
                        Style::default().fg(theme::BORDER),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                    Line::from(Span::styled(
                        "—".to_string(),
                        Style::default().fg(theme::IDLE),
                    )),
                ])
            }
        })
        .collect();

    let widths = [
        Constraint::Length(4),  // #
        Constraint::Length(9),  // VPN
        Constraint::Length(9),  // PPN
        Constraint::Length(5),  // RWXU
        Constraint::Length(5),  // ASID
        Constraint::Length(2),  // G
        Constraint::Length(2),  // A
        Constraint::Length(2),  // D
        Constraint::Length(3),  // mega
    ];
    let header = Row::new(vec![
        Span::styled(" # ", Style::default().fg(theme::LABEL)),
        Span::styled("VPN", Style::default().fg(theme::LABEL)),
        Span::styled("PPN", Style::default().fg(theme::LABEL)),
        Span::styled("RWXU", Style::default().fg(theme::LABEL)),
        Span::styled("ASID", Style::default().fg(theme::LABEL)),
        Span::styled(" G", Style::default().fg(theme::LABEL)),
        Span::styled(" A", Style::default().fg(theme::LABEL)),
        Span::styled(" D", Style::default().fg(theme::LABEL)),
        Span::styled("Mp", Style::default().fg(theme::LABEL)),
    ])
    .style(Style::default().fg(theme::LABEL));

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn btn_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)
    } else if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    }
}

pub fn replacement_label(r: ReplacementPolicy) -> &'static str {
    match r {
        ReplacementPolicy::Lru => "LRU (Least Recently Used)",
        ReplacementPolicy::Mru => "MRU (Most Recently Used)",
        ReplacementPolicy::Fifo => "FIFO (First In First Out)",
        ReplacementPolicy::Random => "Random",
        ReplacementPolicy::Lfu => "LFU (Least Frequently Used)",
        ReplacementPolicy::Clock => "Clock (Second Chance)",
    }
}
