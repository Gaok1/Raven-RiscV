use super::super::TutorialStep;
use crate::ui::app::App;
use crate::ui::pipeline::PipelineSubtab;
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
    let _ = app;
    let stages_h = 9;
    Some(Rect {
        x: content.x,
        y: content.y,
        width: content.width,
        height: stages_h.min(content.height),
    })
}

fn target_hazards(term: Rect, app: &App) -> Option<Rect> {
    let content = pipeline_layout(term).content;
    let stages_h = 9;
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
    let stages_h = 9;
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
        body_pt: "A barra superior alterna entre Main e Config. O seletor de core permite inspecionar o pipeline de um par específico de core/hart.\
\n\nRun e Pipeline compartilham o mesmo core selecionado, então mudar aqui também muda o core observado em execução.",
        target: target_subtab,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Execution Controls",
        body_en: "The controls bar drives the pipeline clock: Step advances one cycle, State runs or pauses, Reset restarts, and Speed changes the animation rate.\
\n\nPress [e] to toggle the pipeline on or off entirely. When disabled, the simulator falls back to sequential execution and the stage view becomes inactive.\
\n\nIn multi-core mode, these controls observe the selected core while the simulator advances the configured runtime model.",
        body_pt: "A barra de controles dirige o clock do pipeline: Step avança um ciclo, State executa ou pausa, Reset reinicia e Speed muda a taxa da animação.\
\n\nPressione [e] para ativar ou desativar o pipeline por completo. Quando desativado, o simulador volta à execução sequencial e a view de estágios fica inativa.\
\n\nNo modo multicore, esses controles observam o core selecionado enquanto o simulador avança o modelo de execução configurado.",
        target: target_controls,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Stage View",
        title_pt: "Stage View",
        body_en: "The stage boxes show what is currently in IF, ID, the functional-unit execution panel, MEM and WB.\
\n\nThe EX area always exposes the UFs so you can compare what is actually busy with what could have run in parallel. The UI also distinguishes a stalled instruction from an empty stage waiting on fetch, a control squash, and an injected bubble.",
        body_pt: "Os blocos de estágio mostram o que está em IF, ID, no painel de unidades funcionais de execução, MEM e WB.\
\n\nA área de EX sempre expõe as UFs para você comparar o que está realmente ocupado com o que poderia ter rodado em paralelo. A interface também distingue uma instrução parada de um estágio vazio aguardando fetch, de um squash de controle e de uma bolha injetada.",
        target: target_stages,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Hazard / Forwarding Map",
        title_pt: "Hazard / Forwarding Map",
        body_en: "This panel explains pipeline conflicts in a didactic way: RAW, load-use, branch flush and bypass paths are rendered as traces with matching colors.\
\n\nWhen a hart is idle or free, this area naturally becomes quieter because there are no active dependencies to draw.",
        body_pt: "Este painel explica conflitos do pipeline de forma didática: caminhos de RAW, load-use, branch flush e bypass são renderizados como traços com cores correspondentes.\
\n\nQuando um hart está ocioso ou livre, esta área naturalmente fica mais silenciosa porque não há dependências ativas para desenhar.",
        target: target_hazards,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "History / Gantt",
        title_pt: "History / Gantt",
        body_en: "The bottom history shows the last cycles and where each instruction spent time.\
\n\nFlushes, stalls and long-latency operations become visible here, which is especially useful when comparing different cores.",
        body_pt: "O histórico inferior mostra os últimos ciclos e onde cada instrução passou tempo.\
\n\nFlushes, stalls e operações de alta latência ficam visíveis aqui, o que é especialmente útil ao comparar cores diferentes.",
        target: target_gantt,
        setup: Some(setup_main),
    },
    TutorialStep {
        title_en: "Pipeline Config",
        title_pt: "Pipeline Config",
        body_en: "The Config subtab changes the simulator model itself: EX->EX, MEM->EX, WB->ID and Store->Load bypass paths, the execution model, branch resolution stage and prediction policy.\
\n\nThe execution model controls whether Raven serializes execution or allows parallel work across UFs when hazards permit it. Prediction includes static and dynamic modes — wrong-path work and flush rate change visibly as you compare Not-Taken, Always-Taken, BTFNT and 2-bit Dynamic.\
\n\nPress [b] anywhere in the Pipeline tab to quickly cycle the branch resolve stage: ID → EX → MEM, without opening Config.",
        body_pt: "A subtab Config altera o próprio modelo do simulador: caminhos de bypass EX->EX, MEM->EX, WB->ID e Store->Load, o modelo de execução, o estágio de resolução de branch e a política de predição.\
\n\nO modelo de execução controla se o Raven serializa a execução ou permite trabalho paralelo entre UFs quando os hazards permitem. A predição inclui modos estáticos e dinâmicos — trabalho em caminho errado e taxa de flush mudam visivelmente ao comparar Not-Taken, Always-Taken, BTFNT e 2-bit Dynamic.\
\n\nPressione [b] em qualquer lugar na aba Pipeline para ciclar rapidamente o estágio de resolução de branch: ID → EX → MEM, sem abrir a Config.",
        target: target_config,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Export & Import",
        title_pt: "Export & Import",
        body_en: "Three shortcuts manage pipeline data outside the session:\
\n\nCtrl+e — export the current pipeline configuration as a .pcfg file.\
\nCtrl+l — import a .pcfg file and apply it immediately.\
\nCtrl+r — export simulation results (stage timings, hazard counts) as .pstats or .csv.\
\n\nThese are also available as buttons in the controls bar at the bottom of the Pipeline tab.",
        body_pt: "Três atalhos gerenciam os dados do pipeline fora da sessão:\
\n\nCtrl+e — exporta a configuração atual do pipeline como arquivo .pcfg.\
\nCtrl+l — importa um arquivo .pcfg e aplica imediatamente.\
\nCtrl+r — exporta os resultados da simulação (timings de estágios, contagem de hazards) em .pstats ou .csv.\
\n\nEstes também estão disponíveis como botões na barra de controles na parte inferior da aba Pipeline.",
        target: target_controls,
        setup: Some(setup_config),
    },
];
