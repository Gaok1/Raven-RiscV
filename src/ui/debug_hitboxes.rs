use ratatui::layout::Rect;

use crate::ui::app::{App, RunButton};
use crate::ui::input::mouse::{run_status_area, run_status_hit};
use crate::ui::tutorial::render::tutorial_popup_rect;
use crate::ui::tutorial::{get_steps, start_tutorial};
use crate::ui::view::run::run_controls_plain_text;
use crate::ui::view::{help_button_area, help_popup_rect};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

pub struct DebugRunControlsOptions {
    pub width: u16,
    pub height: u16,
    pub running: bool,
    pub selected_core: usize,
    pub max_cores: usize,
    pub view: DebugRunView,
}

#[derive(Clone, Copy)]
pub enum DebugRunView {
    Ram,
    Regs,
    Dyn,
}

pub struct DebugHelpLayoutOptions {
    pub width: u16,
    pub height: u16,
    pub tab: DebugUiTab,
}

#[derive(Clone, Copy)]
pub enum DebugUiTab {
    Editor,
    Run,
    Cache,
    Pipeline,
    Docs,
    Config,
}

pub struct DebugPipelineStageOptions {
    pub width: usize,
    pub stage: String,
    pub disasm: String,
    pub badges: Vec<String>,
    pub predicted_badge: Option<String>,
}

pub fn debug_run_controls_dump(opts: DebugRunControlsOptions) -> String {
    let mut app = App::new(None);
    app.max_cores = opts.max_cores.clamp(1, 32);
    app.rebuild_harts_for_debug();
    app.selected_core = opts.selected_core.min(app.max_cores.saturating_sub(1));
    app.sync_runtime_for_debug();
    app.run.is_running = opts.running;
    match opts.view {
        DebugRunView::Ram => {
            app.run.show_registers = false;
            app.run.show_dyn = false;
        }
        DebugRunView::Regs => {
            app.run.show_registers = true;
            app.run.show_dyn = false;
        }
        DebugRunView::Dyn => {
            app.run.show_registers = false;
            app.run.show_dyn = true;
        }
    }

    let root = Rect::new(0, 0, opts.width.max(40), opts.height.max(8));
    let status = run_status_area(&app, root);
    let line = run_controls_plain_text(&app);

    let mut ranges: Vec<(u16, u16, RunButton)> = Vec::new();
    let mut cur: Option<(u16, RunButton)> = None;
    for col in status.x..status.x + status.width {
        match (cur, run_status_hit(&app, status, col)) {
            (None, Some(btn)) => cur = Some((col, btn)),
            (Some((start, prev)), Some(btn)) if prev == btn => cur = Some((start, prev)),
            (Some((start, prev)), Some(btn)) => {
                ranges.push((start, col, prev));
                cur = Some((col, btn));
            }
            (Some((start, prev)), None) => {
                ranges.push((start, col, prev));
                cur = None;
            }
            (None, None) => {}
        }
    }
    if let Some((start, btn)) = cur {
        ranges.push((start, status.x + status.width, btn));
    }

    let mut out = String::new();
    out.push_str("Run Controls Debug\n");
    out.push_str("==================\n");
    out.push_str(&format!(
        "width={} height={} selected_core={} max_cores={} running={} view={}\n\n",
        opts.width,
        opts.height,
        app.selected_core,
        app.max_cores,
        app.run.is_running,
        match opts.view {
            DebugRunView::Ram => "ram",
            DebugRunView::Regs => "regs",
            DebugRunView::Dyn => "dyn",
        }
    ));
    out.push_str("line:\n");
    out.push_str(&line);
    out.push_str("\n\nhitboxes:\n");
    for (start, end, btn) in ranges {
        let name = match btn {
            RunButton::Core => "Core",
            RunButton::View => "View",
            RunButton::Format => "Format",
            RunButton::Sign => "Sign",
            RunButton::Bytes => "Bytes",
            RunButton::Region => "Region",
            RunButton::State => "State",
            RunButton::Speed => "Speed",
            RunButton::ExecCount => "ExecCount",
            RunButton::InstrType => "InstrType",
            RunButton::Reset => "Reset",
        };
        out.push_str(&format!("  {:<10} cols [{}..{})\n", name, start, end));
    }
    out
}

pub fn debug_pipeline_stage_dump(opts: DebugPipelineStageOptions) -> String {
    let width = opts.width.max(12);
    let predicted_w = opts
        .predicted_badge
        .as_ref()
        .map_or(0, |s| UnicodeWidthStr::width(s.as_str()));
    let mut badges: Vec<String> = opts
        .badges
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!(" [{}]", s.trim()))
        .collect();
    let reserved = predicted_w.saturating_add(9);
    while !badges.is_empty()
        && badges
            .iter()
            .map(|s| UnicodeWidthStr::width(s.as_str()))
            .sum::<usize>()
            + reserved
            > width
    {
        badges.pop();
    }
    let badge_w: usize = badges
        .iter()
        .map(|s| UnicodeWidthStr::width(s.as_str()))
        .sum();
    let disasm_w = width
        .saturating_sub(badge_w)
        .saturating_sub(predicted_w)
        .saturating_sub(1)
        .max(4);
    let (disasm_trunc, _) = opts.disasm.unicode_truncate(disasm_w);

    let mut line = disasm_trunc.to_string();
    for badge in &badges {
        line.push_str(badge);
    }
    if let Some(pred) = &opts.predicted_badge {
        line.push_str(pred);
    }

    let mut out = String::new();
    out.push_str("Pipeline Stage Debug\n");
    out.push_str("====================\n");
    out.push_str(&format!(
        "stage={} width={} disasm_width={} badge_width={} predicted_width={}\n\n",
        opts.stage, width, disasm_w, badge_w, predicted_w
    ));
    out.push_str("line:\n");
    out.push_str(&line);
    out.push_str("\n\nbadges_kept:\n");
    for badge in badges {
        out.push_str(&format!("  {badge}\n"));
    }
    out
}

pub fn debug_help_layout_dump(opts: DebugHelpLayoutOptions) -> String {
    let mut app = App::new(None);
    app.tab = match opts.tab {
        DebugUiTab::Editor => crate::ui::app::Tab::Editor,
        DebugUiTab::Run => crate::ui::app::Tab::Run,
        DebugUiTab::Cache => crate::ui::app::Tab::Cache,
        DebugUiTab::Pipeline => crate::ui::app::Tab::Pipeline,
        DebugUiTab::Docs => crate::ui::app::Tab::Docs,
        DebugUiTab::Config => crate::ui::app::Tab::Config,
    };
    let root = Rect::new(0, 0, opts.width.max(40), opts.height.max(12));
    let help_btn = help_button_area(root);

    let mut out = String::new();
    out.push_str("Help Layout Debug\n");
    out.push_str("=================\n");
    out.push_str(&format!(
        "tab={} width={} height={}\n",
        app.tab.label(),
        root.width,
        root.height
    ));
    out.push_str(&format!(
        "help_button: x={} y={} w={} h={}\n",
        help_btn.x, help_btn.y, help_btn.width, help_btn.height
    ));

    if !matches!(app.tab, crate::ui::app::Tab::Docs) && !get_steps(app.tab).is_empty() {
        start_tutorial(&mut app);
        let step = &get_steps(app.tab)[app.tutorial.step_idx];
        let target = (step.target)(root, &app);
        let max_w: u16 = 64.min(root.width.saturating_sub(2));
        let body_lines = step.body_en.lines().count() as u16;
        let popup_h = (body_lines + 8).min(root.height.saturating_sub(2));
        let popup = tutorial_popup_rect(target, max_w, popup_h, root);
        out.push_str("mode=tutorial\n");
        if let Some(target) = target {
            out.push_str(&format!(
                "target: x={} y={} w={} h={}\n",
                target.x, target.y, target.width, target.height
            ));
        }
        out.push_str(&format!(
            "popup: x={} y={} w={} h={}\n",
            popup.x, popup.y, popup.width, popup.height
        ));
    } else {
        app.help_open = true;
        let popup = help_popup_rect(root, &app);
        out.push_str("mode=help-popup\n");
        out.push_str(&format!(
            "popup: x={} y={} w={} h={}\n",
            popup.x, popup.y, popup.width, popup.height
        ));
    }

    out
}
