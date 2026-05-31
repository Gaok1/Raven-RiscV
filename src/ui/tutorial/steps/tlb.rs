use super::super::TutorialStep;
use crate::ui::app::{App, TlbSubtab};
use ratatui::layout::Rect;

fn whole(term: Rect, _app: &App) -> Option<Rect> {
    Some(term)
}

fn setup_stats(app: &mut App) {
    app.tlb.subtab = TlbSubtab::Stats;
}
fn setup_config(app: &mut App) {
    app.tlb.subtab = TlbSubtab::Config;
}
fn setup_entries(app: &mut App) {
    app.tlb.subtab = TlbSubtab::Entries;
}
fn setup_status(app: &mut App) {
    app.tlb.subtab = TlbSubtab::Status;
}

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "TLB tab — what it shows",
        title_pt: "Aba TLB — o que aparece",
        body_en: "The TLB tab visualizes the Sv32 virtual-memory translation layer. \
Enable it by toggling vm_enabled=on in the Settings tab, then assembling your program. \
\nThe simulator automatically installs an identity page map and activates translation, \
so any program — even a simple loop — will immediately show TLB hits and misses \
with no extra setup code required.",
        body_pt: "A aba TLB mostra a camada de tradução Sv32. \
Ative com vm_enabled=on na aba Settings e monte o programa. \
\nO simulador instala automaticamente um mapeamento de identidade e ativa a tradução, \
então qualquer programa — até um loop simples — já mostra hits e misses na TLB \
sem nenhum código de configuração adicional.",
        target: whole,
        setup: Some(setup_status),
    },
    TutorialStep {
        title_en: "Stats — hit rate over time",
        title_pt: "Stats — taxa de hit ao longo do tempo",
        body_en: "Counters: hits, misses, evictions, page faults. \
The chart is a 300-cycle rolling window of hit-rate, sampled once per committed instruction. \
Hit rate is meaningful only after a few hundred cycles with VM active.",
        body_pt: "Contadores: hits, misses, evictions, page faults. \
O gráfico é uma janela de 300 ciclos da taxa de hit, amostrada por instrução. \
Só faz sentido após algumas centenas de ciclos com VM ativa.",
        target: whole,
        setup: Some(setup_stats),
    },
    TutorialStep {
        title_en: "Settings — sizing the TLB",
        title_pt: "Settings — dimensionando a TLB",
        body_en: "Click a field to edit. Constraints: entry_count ≥ associativity, both ≥ 1. \
Hit Latency adds cycles on every TLB hit; Miss Penalty is charged on a walk. \
Presets small/med/large set sensible (entry_count, associativity) defaults. \
Apply commits the change and resets the TLB; flush only invalidates entries.",
        body_pt: "Clique num campo para editar. Restrições: entry_count ≥ associativity, ambos ≥ 1. \
Hit Latency soma ciclos em cada hit; Miss Penalty é cobrado em cada walk. \
Os presets small/med/large preenchem combinações de (entries, associativity). \
Apply aplica e reseta a TLB; flush só invalida as entradas.",
        target: whole,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Entries — see installed translations",
        title_pt: "Entries — translations instaladas",
        body_en: "Each row is a TLB slot. Columns: VPN→PPN, R/W/X/U perms, ASID, G (global), A (accessed), D (dirty), Mp (megapage = 4 MiB). \
Run a program with VM on to populate the table. Use ↑/↓ or the mouse wheel to scroll.",
        body_pt: "Cada linha é um slot da TLB. Colunas: VPN→PPN, perms R/W/X/U, ASID, G (global), A (accessed), D (dirty), Mp (megapage = 4 MiB). \
Rode um programa com VM ligada para popular. Use ↑/↓ ou a roda do mouse para rolar.",
        target: whole,
        setup: Some(setup_entries),
    },
    TutorialStep {
        title_en: "vm — why is translation idle?",
        title_pt: "vm — por que a tradução está parada?",
        body_en: "If the TLB looks empty, this subview tells you why: \
vm_enabled flag, satp mode, current privilege level. \
A common gotcha: the test program runs in M-mode and never touches satp, \
so even with vm_enabled=on the MMU stays in identity mode.",
        body_pt: "Se a TLB parece vazia, esta subaba explica o porquê: \
o flag vm_enabled, o modo do satp e o nível de privilégio atual. \
Pegadinha comum: o programa de teste roda em M-mode e nunca toca satp, \
então mesmo com vm_enabled=on a MMU continua em identidade.",
        target: whole,
        setup: Some(setup_status),
    },
];
