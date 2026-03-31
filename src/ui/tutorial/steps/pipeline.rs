use super::super::TutorialStep;
use crate::ui::app::App;
use crate::ui::pipeline::{PipelineMode, PipelineSubtab};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

fn content_area(term: Rect) -> Rect {
    Rect {
        x: term.x,
        y: term.y + 3,
        width: term.width,
        height: term.height.saturating_sub(4),
    }
}

struct PipelineLayout {
    subtab: Rect,
    controls: Rect,
    content: Rect,
}

fn pipeline_layout(term: Rect) -> PipelineLayout {
    let c = content_area(term);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(c);
    PipelineLayout {
        subtab: chunks[0],
        controls: chunks[1],
        content: chunks[2],
    }
}

fn target_subtab(term: Rect, _app: &App) -> Option<Rect> {
    Some(pipeline_layout(term).subtab)
}

fn target_controls(term: Rect, _app: &App) -> Option<Rect> {
    Some(pipeline_layout(term).controls)
}

fn target_stages(term: Rect, app: &App) -> Option<Rect> {
    let content = pipeline_layout(term).content;
    let stages_h = if app.pipeline.mode == PipelineMode::FunctionalUnits {
        9
    } else {
        6
    };
    Some(Rect {
        x: content.x,
        y: content.y,
        width: content.width,
        height: stages_h.min(content.height),
    })
}

fn target_hazards(term: Rect, app: &App) -> Option<Rect> {
    let content = pipeline_layout(term).content;
    let stages_h = if app.pipeline.mode == PipelineMode::FunctionalUnits {
        9
    } else {
        6
    };
    let max_trace_rows = content
        .height
        .saturating_sub(stages_h)
        .saturating_sub(5)
        .clamp(3, 8);
    let trace_rows = app
        .pipeline
        .hazard_traces
        .len()
        .min(max_trace_rows as usize) as u16;
    let legend_rows = if app.pipeline.hazard_traces.is_empty() {
        0
    } else {
        1
    };
    let msg_rows = if app.pipeline.hazard_msgs.is_empty() {
        1
    } else {
        app.pipeline.hazard_msgs.len().min(2) as u16
    };
    let hazards_h = (2 + trace_rows + legend_rows + msg_rows)
        .min(content.height.saturating_sub(stages_h).saturating_sub(3))
        .clamp(4, 13);
    Some(Rect {
        x: content.x,
        y: content.y + stages_h.min(content.height),
        width: content.width,
        height: hazards_h.min(content.height.saturating_sub(stages_h.min(content.height))),
    })
}

fn target_gantt(term: Rect, app: &App) -> Option<Rect> {
    let content = pipeline_layout(term).content;
    let stages_h = if app.pipeline.mode == PipelineMode::FunctionalUnits {
        9
    } else {
        6
    };
    let max_trace_rows = content
        .height
        .saturating_sub(stages_h)
        .saturating_sub(5)
        .clamp(3, 8);
    let trace_rows = app
        .pipeline
        .hazard_traces
        .len()
        .min(max_trace_rows as usize) as u16;
    let legend_rows = if app.pipeline.hazard_traces.is_empty() {
        0
    } else {
        1
    };
    let msg_rows = if app.pipeline.hazard_msgs.is_empty() {
        1
    } else {
        app.pipeline.hazard_msgs.len().min(2) as u16
    };
    let hazards_h = (2 + trace_rows + legend_rows + msg_rows)
        .min(content.height.saturating_sub(stages_h).saturating_sub(3))
        .clamp(4, 13);
    Some(Rect {
        x: content.x,
        y: content.y + stages_h.min(content.height) + hazards_h,
        width: content.width,
        height: content
            .height
            .saturating_sub(stages_h.min(content.height))
            .saturating_sub(hazards_h),
    })
}

fn target_config(term: Rect, _app: &App) -> Option<Rect> {
    Some(pipeline_layout(term).content)
}

fn setup_main(app: &mut App) {
    app.pipeline.subtab = PipelineSubtab::Main;
}

fn setup_config(app: &mut App) {
    app.pipeline.subtab = PipelineSubtab::Config;
}

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Pipeline Tabs & Core",
        title_pt: "Pipeline Tabs & Core",
        body_en: "The top bar switches between Main and Config. The Core selector lets you inspect the pipeline of a specific core/hart pair.\
\n\nRun and Pipeline share the same selected core, so changing it here also changes the observed runtime core.",
        body_pt: "The top bar switches between Main and Config. The Core selector lets you inspect the pipeline of a specific core/hart pair.\
\n\nRun and Pipeline share the same selected core, so changing it here also changes the observed runtime core.",
        target: target_subtab,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Execution Controls",
        body_en: "The controls bar drives the pipeline clock: Step advances one cycle, State runs or pauses, Reset restarts, and Speed changes the animation rate.\
\n\nIn multi-core mode, these controls observe the selected core while the simulator advances the configured runtime model.",
        body_pt: "The controls bar drives the pipeline clock: Step advances one cycle, State runs or pauses, Reset restarts, and Speed changes the animation rate.\
\n\nIn multi-core mode, these controls observe the selected core while the simulator advances the configured runtime model.",
        target: target_controls,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Stage View",
        title_pt: "Stage View",
        body_en: "The stage boxes show what is currently in IF, ID, EX, MEM and WB.\
\n\nThe UI distinguishes a stalled instruction from an empty stage waiting on fetch, a control squash, and an injected bubble, so front-end waits are easier to read.",
        body_pt: "The stage boxes show what is currently in IF, ID, EX, MEM and WB.\
\n\nA interface distingue uma instrução parada de um estágio vazio aguardando fetch, de um squash de controle e de uma bolha injetada, para deixar esperas do front-end mais claras.",
        target: target_stages,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Hazard / Forwarding Map",
        title_pt: "Hazard / Forwarding Map",
        body_en: "This panel explains pipeline conflicts in a didactic way: RAW, load-use, branch flush and bypass paths are rendered as traces with matching colors.\
\n\nWhen a hart is idle or free, this area naturally becomes quieter because there are no active dependencies to draw.",
        body_pt: "This panel explains pipeline conflicts in a didactic way: RAW, load-use, branch flush and bypass paths are rendered as traces with matching colors.\
\n\nWhen a hart is idle or free, this area naturally becomes quieter because there are no active dependencies to draw.",
        target: target_hazards,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "History / Gantt",
        title_pt: "History / Gantt",
        body_en: "The bottom history shows the last cycles and where each instruction spent time.\
\n\nFlushes, stalls and long-latency operations become visible here, which is especially useful when comparing different cores.",
        body_pt: "The bottom history shows the last cycles and where each instruction spent time.\
\n\nFlushes, stalls and long-latency operations become visible here, which is especially useful when comparing different cores.",
        target: target_gantt,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Pipeline Config",
        title_pt: "Pipeline Config",
        body_en: "The Config subtab changes the simulator model itself: EX->EX, MEM->EX, WB->ID and Store->Load bypass paths, pipeline mode, branch resolution stage and prediction policy.\
\n\nPrediction now includes static and dynamic modes, so wrong-path work and flush rate visibly change as you compare Not-Taken, Always-Taken, BTFNT and 2-bit Dynamic.",
        body_pt: "The Config subtab changes the simulator model itself: EX->EX, MEM->EX, WB->ID and Store->Load bypass paths, pipeline mode, branch resolution stage and prediction policy.\
\n\nPrediction now includes static and dynamic modes, so wrong-path work and flush rate visibly change as you compare Not-Taken, Always-Taken, BTFNT and 2-bit Dynamic.",
        target: target_config,
        setup: Some(setup_config),
    },
];
