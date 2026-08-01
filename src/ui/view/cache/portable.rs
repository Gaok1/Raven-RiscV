use ratatui::{
    Frame,
    prelude::*,
    widgets::{Gauge, Paragraph},
};
use raven_riscv_engine::capability::{CacheLevelView, CacheRole};

use crate::ui::app::{App, CacheScope};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let Some(caches) = app.cache_hierarchy() else {
        return;
    };
    let roles: &[CacheRole] = match app.cache.scope {
        CacheScope::ICache => &[CacheRole::Instruction],
        CacheScope::DCache => &[CacheRole::Data],
        CacheScope::Both => &[CacheRole::Instruction, CacheRole::Data],
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(roles.iter().map(|_| Constraint::Ratio(1, roles.len() as u32)))
        .split(area);
    for (column, role) in columns.iter().zip(roles) {
        if let Some(cache) = caches.cache(app.cache.selected_level, *role) {
            render_level_stats(f, *column, &cache);
        }
    }
}

fn render_level_stats(f: &mut Frame, area: Rect, cache: &CacheLevelView<'_>) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    let rate = cache.stats.hit_rate().clamp(0.0, 100.0);
    f.render_widget(
        Gauge::default()
            .block(panel::panel_frame(PanelKind::Accent).title(cache.name.clone()))
            .gauge_style(Style::default().fg(theme::ACCENT))
            .ratio(rate / 100.0)
            .label(format!("hit rate {rate:.1}%")),
        rows[0],
    );
    let lines = vec![
        metric("Hits", cache.stats.hits),
        metric("Misses", cache.stats.misses),
        metric("Accesses", cache.stats.total_accesses()),
        metric("Evictions", cache.stats.evictions),
        metric("Cycles", cache.stats.total_cycles),
        metric("Bytes loaded", cache.stats.bytes_loaded),
        metric("Bytes stored", cache.stats.bytes_stored),
        Line::raw(""),
        Line::from(vec![
            Span::styled("AMAT  ", style::label()),
            Span::styled(
                format!(
                    "{:.2} cycles",
                    if cache.stats.total_accesses() == 0 {
                        0.0
                    } else {
                        cache.stats.total_cycles as f64 / cache.stats.total_accesses() as f64
                    }
                ),
                style::value(),
            ),
        ]),
    ];
    let inner = render_panel(f, rows[1], panel::panel("Statistics", PanelKind::Plain));
    f.render_widget(Paragraph::new(lines), inner);
}

fn metric(name: &'static str, value: u64) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name:<14}"), style::label()),
        Span::styled(value.to_string(), style::value()),
    ])
}

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    app.cache.config_hitboxes_i.set([(0, 0, 0); 11]);
    app.cache.config_hitboxes_d.set([(0, 0, 0); 11]);
    app.cache.config_hitboxes_u.set([(0, 0, 0); 11]);
    app.cache.config_preset_origin_i.set((0, 0));
    app.cache.config_preset_origin_d.set((0, 0));
    app.cache.config_preset_origin_u.set((0, 0));
    app.cache.config_apply_origin.set((0, 0));

    let Some(caches) = app.cache_hierarchy() else {
        return;
    };
    let roles: &[CacheRole] = match app.cache.scope {
        CacheScope::ICache => &[CacheRole::Instruction],
        CacheScope::DCache => &[CacheRole::Data],
        CacheScope::Both => &[CacheRole::Instruction, CacheRole::Data],
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(roles.iter().map(|_| Constraint::Ratio(1, roles.len() as u32)))
        .split(area);
    for (column, role) in columns.iter().zip(roles) {
        let Some(cache) = caches.cache(app.cache.selected_level, *role) else {
            continue;
        };
        let cfg = cache.config;
        let lines = vec![
            setting("Size", format!("{} bytes", cfg.size)),
            setting("Line size", format!("{} bytes", cfg.line_size)),
            setting("Associativity", format!("{}-way", cfg.associativity)),
            setting("Sets", cfg.num_sets.to_string()),
            setting("Address", format!("{} bits", cfg.address_bits)),
            setting("Replacement", format!("{:?}", cfg.replacement)),
            setting("Write policy", format!("{:?}", cfg.write_policy)),
            setting("Allocation", format!("{:?}", cfg.write_allocation)),
            setting("Hit latency", format!("{} cycles", cfg.hit_latency)),
            setting("Miss penalty", format!("{} cycles", cfg.miss_penalty)),
            Line::raw(""),
            Line::from(Span::styled(
                "Configuration is supplied by this ISA's cache model.",
                style::label(),
            )),
        ];
        let inner = render_panel(f, *column, panel::panel(cache.name, PanelKind::Accent));
        f.render_widget(Paragraph::new(lines), inner);
    }
}

fn setting(name: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name:<16}"), style::label()),
        Span::styled(value, style::value()),
    ])
}

#[cfg(test)]
mod tests {
    use crate::ui::app::{App, CacheSubtab};
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 34)).unwrap();
        terminal
            .draw(|f| super::super::render_cache(f, f.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn teaching_isas_render_cache_stats_view_and_configuration() {
        for id in ["toy16", "sap"] {
            let mut app = App::new_with_architecture(
                None,
                crate::falcon::jit::BackendKind::None,
                id,
            )
            .unwrap();
            for _ in 0..3 {
                app.single_step();
            }

            app.cache.subtab = CacheSubtab::Stats;
            assert!(screen(&app).contains("hit rate"));
            app.cache.subtab = CacheSubtab::View;
            assert!(screen(&app).contains("I-Cache"));
            app.cache.subtab = CacheSubtab::Config;
            let config = screen(&app);
            assert!(config.contains("Line size"));
            assert!(config.contains("Associativity"));
        }
    }
}
