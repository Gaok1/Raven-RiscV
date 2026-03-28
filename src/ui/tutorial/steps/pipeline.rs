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
        title_pt: "Subtabs e core do pipeline",
        body_en: "The top bar switches between Main and Config. The Core selector lets you inspect the pipeline of a specific core/hart pair.\
\n\nRun and Pipeline share the same selected core, so changing it here also changes the observed runtime core.",
        body_pt: "A barra superior alterna entre Main e Config. O seletor de Core permite inspecionar o pipeline de um par core/hart específico.\
\n\nRun e Pipeline compartilham o mesmo core selecionado, então trocar aqui também muda o core observado no runtime.",
        target: target_subtab,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Controles de execução",
        body_en: "The controls bar drives the pipeline clock: Step advances one cycle, State runs or pauses, Reset restarts, and Speed changes the animation rate.\
\n\nIn multi-core mode, these controls observe the selected core while the simulator advances the configured runtime model.",
        body_pt: "A barra de controles dirige o clock do pipeline: Step avança um ciclo, State executa ou pausa, Reset reinicia e Speed altera a taxa da animação.\
\n\nEm modo multi-core, esses controles observam o core selecionado enquanto o simulador avança o modelo de runtime configurado.",
        target: target_controls,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Stage View",
        title_pt: "Visão de estágios",
        body_en: "The stage boxes show what is currently in IF, ID, EX, MEM and WB.\
\n\nSpeculative instructions, hazards, bubbles and squashed work are all marked directly in the stage titles and badges.",
        body_pt: "Os blocos de estágios mostram o que está atualmente em IF, ID, EX, MEM e WB.\
\n\nInstruções especulativas, hazards, bubbles e trabalho descartado são marcados diretamente nos títulos e badges dos estágios.",
        target: target_stages,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Hazard / Forwarding Map",
        title_pt: "Mapa de hazard / forwarding",
        body_en: "This panel explains pipeline conflicts in a didactic way: RAW, load-use, branch flush and bypass/forwarding paths are rendered as traces with matching colors.\
\n\nWhen a hart is idle or free, this area naturally becomes quieter because there are no active dependencies to draw.",
        body_pt: "Este painel explica conflitos do pipeline de forma didática: RAW, load-use, branch flush e caminhos de bypass/forwarding são renderizados como traces com cores coerentes.\
\n\nQuando um hart está parado ou livre, esta área naturalmente fica mais vazia porque não há dependências ativas para desenhar.",
        target: target_hazards,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "History / Gantt",
        title_pt: "Histórico / Gantt",
        body_en: "The bottom history shows the last cycles and where each instruction spent time.\
\n\nFlushes, stalls and long-latency operations become visible here, which is especially useful when comparing different cores.",
        body_pt: "O histórico inferior mostra os últimos ciclos e onde cada instrução passou tempo.\
\n\nFlushes, stalls e operações de longa latência ficam visíveis aqui, o que é especialmente útil ao comparar cores diferentes.",
        target: target_gantt,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Pipeline Config",
        title_pt: "Configuração do pipeline",
        body_en: "The Config subtab changes the simulator model itself: forwarding, pipeline mode, branch resolution stage and prediction policy.\
\n\nThese options affect how hazards are resolved and how much wrong-path work appears in the Main view.",
        body_pt: "A subtab Config altera o próprio modelo do simulador: forwarding, modo do pipeline, estágio de resolução de branch e política de predição.\
\n\nEssas opções afetam como hazards são resolvidos e quanto trabalho de caminho errado aparece na visão Main.",
        target: target_config,
        setup: Some(setup_config),
    },
];
