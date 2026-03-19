use ratatui::layout::Rect;
use crate::ui::app::{App, CacheSubtab};
use super::super::TutorialStep;

// ── Layout helpers (mirror view/cache/mod.rs) ────────────────────────────────
// Cache tab layout:
//   content_area = term minus tab_bar(3) and footer(1)
//   within content_area:
//     layout[0] = level selector     (1 row)
//     layout[1] = subtab header      (3 rows)
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
    level_sel:    Rect,
    subtab_hdr:   Rect,
    exec_ctrl:    Rect,
    content:      Rect,
    controls_bar: Rect,
}

fn cache_layout(term: Rect) -> CacheLayout {
    let c = content_area(term);
    let total = c.height;
    // rows: 1 + 3 + 4 + min + 3
    let fixed = 1u16 + 3 + 4 + 3; // = 11
    let content_h = total.saturating_sub(fixed);
    CacheLayout {
        level_sel:    Rect { x: c.x, y: c.y,      width: c.width, height: 1 },
        subtab_hdr:   Rect { x: c.x, y: c.y + 1,  width: c.width, height: 3 },
        exec_ctrl:    Rect { x: c.x, y: c.y + 4,  width: c.width, height: 4 },
        content:      Rect { x: c.x, y: c.y + 8,  width: c.width, height: content_h },
        controls_bar: Rect { x: c.x, y: c.y + 8 + content_h, width: c.width, height: 3 },
    }
}

/// Config layout — 3 columns at 38% / 38% / 24%
fn config_left(content: Rect) -> Rect {
    let w = (content.width * 38 / 100).max(1);
    Rect { x: content.x, y: content.y, width: w, height: content.height }
}

fn config_mid(content: Rect) -> Rect {
    let left_w = (content.width * 38 / 100).max(1);
    let w = (content.width * 38 / 100).max(1);
    Rect { x: content.x + left_w, y: content.y, width: w, height: content.height }
}

fn config_right(content: Rect) -> Rect {
    let left_w = (content.width * 38 / 100).max(1);
    let mid_w  = (content.width * 38 / 100).max(1);
    let w = content.width.saturating_sub(left_w + mid_w);
    Rect { x: content.x + left_w + mid_w, y: content.y, width: w, height: content.height }
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

fn target_config_right(term: Rect, _app: &App) -> Option<Rect> {
    Some(config_right(cache_layout(term).content))
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
        body_en:  "This bar selects the cache level to configure: L1 (always present), L2, L3 and so on.\
\n\nUse the [+ Add] and [- Remove] buttons to add or remove hierarchical cache levels. The [+/-] shortcuts also work.",
        body_pt:  "Esta barra seleciona o nível de cache a configurar: L1 (sempre presente), L2, L3 e assim por diante.\
\n\nUse os botões [+ Add] e [- Remove] para adicionar ou remover níveis de cache hierárquicos. Os atalhos [+/-] também funcionam.",
        target: target_level_sel,
        setup:  None,
    },
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Controles de execução",
        body_en:  "Cache simulation controls: [Reset] resets the statistics, Speed sets the rate, State pauses/resumes.\
\n\nThe panel shows total cycles, CPI (Cycles Per Instruction) and the number of executed instructions.",
        body_pt:  "Controles de simulação do cache: [Reset] reinicia as estatísticas, Speed define a velocidade, State pausa/retoma.\
\n\nO painel mostra o total de ciclos, CPI (Ciclos Por Instrução) e o número de instruções executadas.",
        target: target_exec_ctrl,
        setup:  None,
    },
    TutorialStep {
        title_en: "Stats Subtab — overview",
        title_pt: "Subtab Stats — visão geral",
        body_en:  "The Stats subtab shows detailed cache performance metrics: hit rate, miss rate, total accesses and average memory access time (AMAT).\
\n\nUse [Tab] to switch between subtabs, or click the Stats / View / Config buttons directly.",
        body_pt:  "A subtab Stats mostra métricas detalhadas de desempenho do cache: hit rate, miss rate, acessos totais e tempo médio de acesso (AMAT).\
\n\nUse [Tab] para alternar entre subtabs, ou clique diretamente nos botões Stats / View / Config.",
        target: target_subtab_and_content,
        setup:  Some(setup_stats),
    },
    TutorialStep {
        title_en: "Stats — metrics",
        title_pt: "Stats — métricas",
        body_en:  "For each cache level: hits, misses, hit rate (%), writebacks and the calculated AMAT are displayed.\
\n\nMetrics are separated by scope: I-Cache (instructions), D-Cache (data) and combined totals.\
\n\nUse [i], [d], [b] to filter the displayed scope.",
        body_pt:  "Para cada nível de cache são exibidos: hits, misses, taxa de hit (%), writebacks e o AMAT calculado.\
\n\nAs métricas são separadas por escopo: I-Cache (instruções), D-Cache (dados) e totais combinados.\
\n\nUse [i], [d], [b] para filtrar o escopo exibido.",
        target: target_content,
        setup:  Some(setup_stats),
    },
    TutorialStep {
        title_en: "Stats — session history",
        title_pt: "Stats — histórico de sessões",
        body_en:  "The history panel records snapshots of previous simulation sessions for comparison.\
\n\nPress [s] to capture the current state as a baseline. Use [Ctrl+M] to load a baseline saved to file.\
\n\nDifferences between sessions are highlighted for easy analysis.",
        body_pt:  "O painel de histórico registra snapshots de sessões anteriores de simulação para comparação.\
\n\nPressione [s] para capturar o estado atual como baseline. Use [Ctrl+M] para carregar um baseline salvo em arquivo.\
\n\nAs diferenças entre sessões ficam destacadas para facilitar a análise.",
        target: target_content,
        setup:  Some(setup_stats),
    },
    TutorialStep {
        title_en: "View Subtab — visualization",
        title_pt: "Subtab View — visualização",
        body_en:  "The View subtab shows the physical contents of cache lines in real time during execution.\
\n\nEach line displays the address (tag), stored data, validity bit and dirty bit (for write-back).\
\n\nScroll with ↑/↓ to navigate through lines. Use ← → to scroll horizontally.",
        body_pt:  "A subtab View mostra o conteúdo físico das linhas de cache em tempo real durante a execução.\
\n\nCada linha exibe o endereço (tag), dados armazenados, bit de validade e bit dirty (para write-back).\
\n\nRole com ↑/↓ para navegar pelas linhas. Use ← → para rolar horizontalmente.",
        target: target_subtab_and_content,
        setup:  Some(setup_view),
    },
    TutorialStep {
        title_en: "Config Subtab — overview",
        title_pt: "Subtab Config — visão geral",
        body_en:  "The Config subtab lets you configure the parameters of each cache level before starting simulation.\
\n\nSettings are divided into three columns: I-Cache (instructions), D-Cache (data) and CPI parameters.\
\n\nAfter editing, click [Apply] to apply the changes.",
        body_pt:  "A subtab Config permite configurar os parâmetros de cada nível de cache antes de iniciar a simulação.\
\n\nAs configurações são divididas em três colunas: I-Cache (instruções), D-Cache (dados) e parâmetros de CPI.\
\n\nApós editar, clique [Apply] para aplicar as mudanças.",
        target: target_subtab_and_content,
        setup:  Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — I-Cache",
        title_pt: "Config — I-Cache",
        body_en:  "Instruction cache (I-Cache) parameters: total size, line size (block size), associativity, replacement policy (LRU/FIFO/Random) and hit latency.\
\n\nClick a field or use ↑/↓ to navigate and Enter to edit.",
        body_pt:  "Parâmetros da cache de instruções (I-Cache): tamanho total, tamanho da linha (block size), associatividade, política de substituição (LRU/FIFO/Random) e latência de hit.\
\n\nClique em um campo ou use ↑/↓ para navegar e Enter para editar.",
        target: target_config_left,
        setup:  Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — D-Cache",
        title_pt: "Config — D-Cache",
        body_en:  "Data cache (D-Cache) parameters: same fields as I-Cache, plus write policy (Write-Back / Write-Through) and write allocate (Yes/No).\
\n\nIn unified caches (L2+), I and D parameters are shared.",
        body_pt:  "Parâmetros da cache de dados (D-Cache): mesmos campos do I-Cache, mais write policy (Write-Back / Write-Through) e write allocate (Yes/No).\
\n\nEm caches unificadas (L2+), os parâmetros I e D são compartilhados.",
        target: target_config_mid,
        setup:  Some(setup_config),
    },
    TutorialStep {
        title_en: "Config — CPI",
        title_pt: "Config — CPI",
        body_en:  "The CPI column defines the miss penalty, associativity penalty and the transfer width between levels.\
\n\nThese parameters directly influence the AMAT (Average Memory Access Time) calculation shown in Stats.",
        body_pt:  "A coluna CPI define a penalidade de miss, penalidade de associatividade e a largura de transferência entre níveis.\
\n\nEsses parâmetros influenciam diretamente o cálculo do AMAT (Average Memory Access Time) exibido em Stats.",
        target: target_config_right,
        setup:  Some(setup_config),
    },
    TutorialStep {
        title_en: "Controls Bar",
        title_pt: "Barra de controles",
        body_en:  "Bottom bar with global actions: [Export Results] saves statistics as .fstats or .csv, [Export Config] and [Import Config] save/load cache configurations as .fcache.\
\n\nThe scope buttons [I] [D] [Both] filter which cache is shown in the Stats and View subtabs.",
        body_pt:  "Barra inferior com ações globais: [Export Results] salva estatísticas em .fstats ou .csv, [Export Config] e [Import Config] salvam/carregam configurações de cache em .fcache.\
\n\nOs botões de escopo [I] [D] [Both] filtram qual cache é mostrada nas subtabs Stats e View.",
        target: target_controls_bar,
        setup:  None,
    },
];
