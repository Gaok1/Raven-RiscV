use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::ui::App;
use crate::ui::theme;

use super::GuidedPreset;

pub fn render_guided_learning(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Atividade Guiada ",
            Style::default().fg(theme::ACCENT).bold(),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split: preset list on the left, info panel on the right
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(38)])
        .split(inner);

    render_preset_list(f, chunks[0], app);
    render_info_panel(f, chunks[1], app);
}

fn render_preset_list(f: &mut Frame, area: Rect, app: &App) {
    let presets = GuidedPreset::all();
    let cursor = app.activity.cursor.min(presets.len().saturating_sub(1));

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, &preset) in presets.iter().enumerate() {
        // Section header before the first item in each domain
        if let Some(header) = preset.section_header() {
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            lines.push(Line::from(Span::styled(
                header.to_string(),
                Style::default().fg(theme::LABEL_Y).bold(),
            )));
        }

        let is_cursor = i == cursor;
        let is_applied = app.activity.last_applied == Some(preset);

        let prefix = if is_cursor { "▶ " } else { "  " };
        let label = preset.label();
        let desc = preset.description();

        let label_style = if is_cursor {
            Style::default().fg(theme::ACTIVE).bold()
        } else if is_applied {
            Style::default().fg(theme::RUNNING)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let desc_style = if is_cursor {
            Style::default().fg(theme::TEXT)
        } else {
            Style::default().fg(theme::IDLE)
        };

        let applied_marker = if is_applied {
            Span::styled(" ✓", Style::default().fg(theme::RUNNING))
        } else {
            Span::raw("  ")
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{prefix}{label:<6}"), label_style),
            Span::styled(desc.to_string(), desc_style),
            applied_marker,
        ]));
    }

    // Status / error at the bottom
    lines.push(Line::raw(""));
    if let Some(ref err) = app.activity.status_err {
        lines.push(Line::from(Span::styled(
            format!("✗ {err}"),
            Style::default().fg(theme::DANGER),
        )));
    } else if let Some(ref msg) = app.activity.status_msg {
        lines.push(Line::from(Span::styled(
            format!("✓ {msg}"),
            Style::default().fg(theme::RUNNING),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Enter = aplicar preset selecionado",
            Style::default().fg(theme::IDLE),
        )));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_info_panel(f: &mut Frame, area: Rect, app: &App) {
    let presets = GuidedPreset::all();
    if presets.is_empty() || area.width < 4 {
        return;
    }
    let cursor = app.activity.cursor.min(presets.len().saturating_sub(1));
    let preset = presets[cursor];

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let configs: &str = match preset {
        GuidedPreset::D3_01 => "R300 + P101 + C311",
        GuidedPreset::D3_02 => "R300 + P101 + C312",
        GuidedPreset::D3_03 => "R300 + P101 + C321",
        GuidedPreset::D3_04 => "R300 + P101 + C322",
        GuidedPreset::D3_05 => "R300 + P101 + C331",
        GuidedPreset::D3_06 => "R300 + P101 + C332",
        GuidedPreset::D5_01 => "R500 + P101",
        GuidedPreset::D1_01 | GuidedPreset::D1_02 => "R100 + P100",
        GuidedPreset::D2_01 | GuidedPreset::D2_02 => "R100 + P100",
        GuidedPreset::D4_01 => "R100 + P101",
        GuidedPreset::D6_01 => "R100 + P101",
        GuidedPreset::D6_02 => "R100 + P100",
    };

    let program: &str = match preset {
        GuidedPreset::D1_01 => "D101.fas",
        GuidedPreset::D1_02 | GuidedPreset::D6_01 | GuidedPreset::D6_02 => "D102.fas",
        GuidedPreset::D2_01 => "D201.fas",
        GuidedPreset::D2_02 => "D202.fas",
        GuidedPreset::D3_01
        | GuidedPreset::D3_02
        | GuidedPreset::D3_03
        | GuidedPreset::D3_04 => "D301.fas",
        GuidedPreset::D3_05 | GuidedPreset::D3_06 => "D302.fas",
        GuidedPreset::D4_01 => "D401.fas",
        GuidedPreset::D5_01 => "D501.fas",
    };

    let tab = preset.suggested_tab().label();

    let lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            preset.label().to_string(),
            Style::default().fg(theme::ACCENT).bold(),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Config: ", Style::default().fg(theme::LABEL)),
            Span::styled(configs.to_string(), Style::default().fg(theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Prog:   ", Style::default().fg(theme::LABEL)),
            Span::styled(program.to_string(), Style::default().fg(theme::TEXT)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Tab:    ", Style::default().fg(theme::LABEL)),
            Span::styled(tab.to_string(), Style::default().fg(theme::ACCENT)),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}
