use super::super::TutorialStep;
use crate::ui::app::{App, CacheSubtab};
use ratatui::layout::Rect;

// ── Layout helpers (mirror view/cache/mod.rs) ────────────────────────────────
// Cache tab layout:
//   content_area = term minus tab_bar(3) and footer(1)
//   within content_area:
//     layout[0] = level selector     (1 row)
//     layout[1] = subtab header      (4 rows)
//     layout[2] = exec controls      (4 rows)
//     layout[3] = content            (min)
//     layout[4] = controls bar       (3 rows)

fn content_area(term: Rect) -> Rect {
    Rect {
        x: term.x,
        y: term.y + 3,
        width: term.width,
        height: term.height.saturating_sub(4),
    }
}

struct CacheLayout {
    level_sel: Rect,
    subtab_hdr: Rect,
    exec_ctrl: Rect,
    content: Rect,
    controls_bar: Rect,
}

fn cache_layout(term: Rect) -> CacheLayout {
    let c = content_area(term);
    let total = c.height;
    // rows: 1 + 4 + 4 + min + 3
    let fixed = 1u16 + 4 + 4 + 3; // = 12
    let content_h = total.saturating_sub(fixed);
    CacheLayout {
        level_sel: Rect {
            x: c.x,
            y: c.y,
            width: c.width,
            height: 1,
        },
        subtab_hdr: Rect {
            x: c.x,
            y: c.y + 1,
            width: c.width,
            height: 4,
        },
        exec_ctrl: Rect {
            x: c.x,
            y: c.y + 5,
            width: c.width,
            height: 4,
        },
        content: Rect {
            x: c.x,
            y: c.y + 9,
            width: c.width,
            height: content_h,
        },
        controls_bar: Rect {
            x: c.x,
            y: c.y + 9 + content_h,
            width: c.width,
            height: 3,
        },
    }
}

/// Config layout — L1 uses two equal columns, unified levels use one centered column.
fn config_left(content: Rect) -> Rect {
    let w = content.width / 2;
    Rect {
        x: content.x,
        y: content.y,
        width: w,
        height: content.height,
    }
}

fn config_mid(content: Rect) -> Rect {
    let left_w = content.width / 2;
    let w = content.width.saturating_sub(left_w);
    Rect {
        x: content.x + left_w,
        y: content.y,
        width: w,
        height: content.height,
    }
}

fn config_bottom(content: Rect) -> Rect {
    let h = content.height.min(6);
    Rect {
        x: content.x,
        y: content.y + content.height.saturating_sub(h),
        width: content.width,
        height: h,
    }
}

// ── Target functions ─────────────────────────────────────────────────────────

fn target_level_sel(term: Rect, _app: &App) -> Option<Rect> {
    Some(cache_layout(term).level_sel)
}

fn target_exec_ctrl(term: Rect, _app: &App) -> Option<Rect> {
    Some(cache_layout(term).exec_ctrl)
}

fn target_content(term: Rect, _app: &App) -> Option<Rect> {
    Some(cache_layout(term).content)
}

fn target_subtab_and_content(term: Rect, _app: &App) -> Option<Rect> {
    let cl = cache_layout(term);
    // Span from subtab_hdr top all the way to content bottom (includes exec_ctrl in between).
    Some(Rect {
        x: cl.subtab_hdr.x,
        y: cl.subtab_hdr.y,
        width: cl.subtab_hdr.width,
        height: (cl.content.y + cl.content.height).saturating_sub(cl.subtab_hdr.y),
    })
}

fn target_config_left(term: Rect, _app: &App) -> Option<Rect> {
    Some(config_left(cache_layout(term).content))
}

fn target_config_mid(term: Rect, _app: &App) -> Option<Rect> {
    Some(config_mid(cache_layout(term).content))
}

fn target_config_bottom(term: Rect, _app: &App) -> Option<Rect> {
    Some(config_bottom(cache_layout(term).content))
}

fn target_controls_bar(term: Rect, _app: &App) -> Option<Rect> {
    Some(cache_layout(term).controls_bar)
}

// ── Setup functions ──────────────────────────────────────────────────────────

fn setup_stats(app: &mut App) {
    app.cache.subtab = CacheSubtab::Stats;
}

fn setup_view(app: &mut App) {
    app.cache.subtab = CacheSubtab::View;
}

fn setup_config(app: &mut App) {
    app.cache.subtab = CacheSubtab::Config;
}

// ── Step definitions ─────────────────────────────────────────────────────────

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Level Selector",
        title_pt: "Seletor de nível",
        body_en: "This bar selects the cache level to configure: L1 (always present), L2, L3 and so on.\
\n\nUse the [+ Add] and [- Remove] buttons to add or remove hierarchical cache levels. The [+/-] shortcuts also work.",
        body_pt: "Esta barra seleciona o nível de cache a configurar: L1 (sempre presente), L2, L3 e assim por diante.\
\n\nUse os botões [+ Add] e [- Remove] para adicionar ou remover níveis de cache hierárquicos. Os atalhos [+/-] também funcionam.",
        target: target_level_sel,
        setup: None,
    },
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Controles de execução",
        body_en: "Cache simulation controls: [Reset] resets the statistics, Speed sets the rate, State pauses/resumes.\
\n\nThe panel shows total cycles, CPI (Cycles Per Instruction) and the number of executed instructions.",
        body_pt: "Controles de simulação do cache: [Reset] reinicia as estatísticas, Speed define a velocidade, State pausa/retoma.\
\n\nO painel mostra o total de ciclos, CPI (Ciclos Por Instrução) e o número de instruções executadas.",
        target: target_exec_ctrl,
        setup: None,
    },
    TutorialStep {
        title_en: "Stats Subtab — overview",
        title_pt: "Subtab Stats — visão geral",
        body_en: "The Stats subtab shows detailed cache performance metrics: hit rate, miss rate, total accesses and average memory access time (AMAT).\
\n\nUse [Tab] to switch between subtabs, or click the Stats / View / Config buttons directly.",
        body_pt: "A subtab Stats mostra métricas detalhadas de desempenho do cache: hit rate, miss rate, acessos totais e tempo médio de acesso (AMAT).\
\n\nUse [Tab] para alternar entre subtabs, ou clique diretamente nos botões Stats / View / Config.",
        target: target_subtab_and_content,
        setup: Some(setup_stats),
    },
    TutorialStep {
        title_en: "Stats — metrics",
        title_pt: "Stats — métricas",
        body_en: "For each cache level: hits, misses, hit rate (%), writebacks and the calculated AMAT are displayed.\
\n\nMetrics are separated by scope: I-Cache (instructions), D-Cache (data) and combined totals.\
\n\nUse [i], [d], [b] to filter the displayed scope.",
        body_pt: "Para cada nível de cache são exibidos: hits, misses, taxa de hit (%), writebacks e o AMAT calculado.\
\n\nAs métricas são separadas por escopo: I-Cache (instruções), D-Cache (dados) e totais combinados.\
\n\nUse [i], [d], [b] para filtrar o escopo exibido.",
        target: target_content,
        setup: Some(setup_stats),
    },
    TutorialStep {
        title_en: "Stats — session history",
        title_pt: "Stats — histórico de sessões",
        body_en: "The history panel records snapshots of previous simulation sessions for comparison.\
\n\nPress [s] to capture the current state as a baseline. Use [Ctrl+M] to load a baseline saved to file.\
\n\nDifferences between sessions are highlighted for easy analysis.",
        body_pt: "O painel de histórico registra snapshots de sessões anteriores de simulação para comparação.\
\n\nPressione [s] para capturar o estado atual como baseline. Use [Ctrl+M] para carregar um baseline salvo em arquivo.\
\n\nAs diferenças entre sessões ficam destacadas para facilitar a análise.",
        target: target_content,
        setup: Some(setup_stats),
    },
    TutorialStep {
        title_en: "View Subtab — visualization",
        title_pt: "Subtab View — visualização",
        body_en: "The View subtab shows the physical contents of cache lines in real time during execution.\
\n\nEach line displays the address (tag), stored data, validity bit and dirty bit (for write-back).\
\n\nScroll with ↑/↓ to navigate through lines. Use ← → to scroll horizontally.",
        body_pt: "A subtab View mostra o conteúdo físico das linhas de cache em tempo real durante a execução.\
\n\nCada linha exibe o endereço (tag), dados armazenados, bit de validade e bit dirty (para write-back).\
\n\nRole com ↑/↓ para navegar pelas linhas. Use ← → para rolar horizontalmente.",
        target: target_subtab_and_content,
        setup: Some(setup_view),
    },
    TutorialStep {
        title_en: "Config Subtab — overview",
        title_pt: "Subtab Config — visão geral",
        body_en: "The Config subtab lets you configure the parameters of each cache level before starting simulation.\
\n\nOn L1 you edit I-Cache and D-Cache side by side. On unified levels (L2+) the editor collapses to a single centered column.\
\n\nAfter editing, use the apply actions at the bottom to commit the changes.",
        body_pt: "A subtab Config permite configurar os parâmetros de cada nível de cache antes de iniciar a simulação.\
\n\nNo L1 você edita I-Cache e D-Cache lado a lado. Em níveis unificados (L2+) o editor vira uma única coluna centralizada.\
\n\nApós editar, use as ações de apply na parte inferior para confirmar as mudanças.",
        target: target_subtab_and_content,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — I-Cache",
        title_pt: "Config — I-Cache",
        body_en: "Instruction cache (I-Cache) parameters: total size, line size (block size), associativity, replacement policy (LRU/FIFO/Random) and hit latency.\
\n\nClick a field or use ↑/↓ to navigate and Enter to edit.",
        body_pt: "Parâmetros da cache de instruções (I-Cache): tamanho total, tamanho da linha (block size), associatividade, política de substituição (LRU/FIFO/Random) e latência de hit.\
\n\nClique em um campo ou use ↑/↓ para navegar e Enter para editar.",
        target: target_config_left,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — D-Cache",
        title_pt: "Config — D-Cache",
        body_en: "Data cache (D-Cache) parameters: same fields as I-Cache, plus write policy (Write-Back / Write-Through) and write allocate (Yes/No).\
\n\nIn unified caches (L2+), I and D parameters are shared.",
        body_pt: "Parâmetros da cache de dados (D-Cache): mesmos campos do I-Cache, mais write policy (Write-Back / Write-Through) e write allocate (Yes/No).\
\n\nEm caches unificadas (L2+), os parâmetros I e D são compartilhados.",
        target: target_config_mid,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — Presets & Apply",
        title_pt: "Config — Presets e Apply",
        body_en: "The lower rows provide quick presets and the apply actions.\
\n\nUse presets to jump to small, medium or large cache profiles, then choose whether applying the config should reset statistics or keep history.",
        body_pt: "As linhas inferiores oferecem presets rápidos e as ações de apply.\
\n\nUse os presets para saltar para perfis small, medium ou large, depois escolha se aplicar a config deve resetar as estatísticas ou preservar o histórico.",
        target: target_config_bottom,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Controls Bar",
        title_pt: "Barra de controles",
        body_en: "Bottom bar with global actions: [Export Results] saves statistics as .fstats or .csv, [Export Config] and [Import Config] save/load cache configurations as .fcache.\
\n\nThe scope buttons [I] [D] [Both] filter which cache is shown in the Stats and View subtabs.",
        body_pt: "Barra inferior com ações globais: [Export Results] salva estatísticas em .fstats ou .csv, [Export Config] e [Import Config] salvam/carregam configurações de cache em .fcache.\
\n\nOs botões de escopo [I] [D] [Both] filtram qual cache é mostrada nas subtabs Stats e View.",
        target: target_controls_bar,
        setup: None,
    },
];
