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
fn setup_page_tree(app: &mut App) {
    // The demand-paging arc only makes sense under hardware-accurate semantics,
    // where the program drives satp and handles its own faults — switch to
    // Manual mode and open the live page-table view.
    app.set_vm_mode(crate::falcon::mmu::VmMode::Manual);
    app.tlb.subtab = TlbSubtab::PageTree;
}

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "TLB tab — what it shows",
        title_pt: "Aba TLB — o que aparece",
        body_en: "The TLB tab visualizes the Sv32 virtual-memory translation layer. \
Enable it by picking a VM mode in the Settings tab (Off / Didactic / Manual), then assembling your program. \
\nIn Didactic mode the simulator installs an identity page map and translates even M-mode accesses, \
so any program — even a simple loop — immediately shows TLB hits and misses with no setup code. \
Manual mode is hardware-accurate: your program drives satp and its own page tables.",
        body_pt: "A aba TLB mostra a camada de tradução Sv32. \
Ative escolhendo um modo de VM na aba Settings (Off / Didactic / Manual) e monte o programa. \
\nNo modo Didactic o simulador instala um mapeamento de identidade e traduz até acessos em M-mode, \
então qualquer programa — até um loop simples — já mostra hits e misses na TLB sem código extra. \
O modo Manual é fiel ao hardware: seu programa controla o satp e as próprias tabelas.",
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
the active VM mode, satp mode, and current privilege level. \
A common gotcha in Manual mode: the program runs in M-mode and never touches satp, \
so the MMU stays in identity mode and nothing populates the TLB.",
        body_pt: "Se a TLB parece vazia, esta subaba explica o porquê: \
o modo de VM ativo, o modo do satp e o nível de privilégio atual. \
Pegadinha comum no modo Manual: o programa roda em M-mode e nunca toca satp, \
então a MMU continua em identidade e nada popula a TLB.",
        target: whole,
        setup: Some(setup_status),
    },
    TutorialStep {
        title_en: "Page tree — the live Sv32 table",
        title_pt: "Árvore de páginas — a tabela Sv32 ao vivo",
        body_en: "The tree subview walks the real page table rooted at satp.PPN, straight from RAM. \
L1 pointers expand into their L0 leaves; long runs of identity megapages collapse into one summary line; \
PTEs currently cached in the TLB are marked ●TLB. This is your window into what the MMU actually sees \
— invaluable when a mapping you wrote isn't taking effect.",
        body_pt: "A subaba de árvore percorre a tabela de páginas real ancorada em satp.PPN, direto da RAM. \
Ponteiros L1 expandem em suas folhas L0; sequências longas de megapáginas de identidade colapsam numa linha-resumo; \
PTEs cacheadas na TLB ganham a marca ●TLB. É sua janela para o que a MMU realmente enxerga \
— essencial quando um mapeamento que você escreveu não surte efeito.",
        target: whole,
        setup: Some(setup_page_tree),
    },
    TutorialStep {
        title_en: "Demand paging — fault, map, retry",
        title_pt: "Demand paging — falha, mapeia, repete",
        body_en: "Manual mode unlocks the classic OS pattern. Delegate load/store page faults to supervisor mode \
by setting the matching bit in medeleg (bit 13 = load fault, 15 = store) and pointing stvec at your handler. \
When U-mode touches an unmapped page, the CPU vectors to stvec in S-mode (saving sepc/scause/stval), \
the handler installs the missing PTE, runs sfence.vma, and `sret` returns to retry the faulting access — now it succeeds. \
The full runnable kernel lives in docs/virtual-memory.md and is exercised by tests/mmu_traps.rs.",
        body_pt: "O modo Manual libera o padrão clássico de SO. Delegue page faults de load/store ao modo supervisor \
setando o bit correspondente em medeleg (bit 13 = load fault, 15 = store) e apontando stvec para seu handler. \
Quando o U-mode toca uma página não-mapeada, a CPU vetoriza para stvec em S-mode (salvando sepc/scause/stval), \
o handler instala a PTE faltante, roda sfence.vma, e `sret` retorna para repetir o acesso — agora ele funciona. \
O kernel completo executável está em docs/virtual-memory.md e é testado em tests/mmu_traps.rs.",
        target: whole,
        setup: Some(setup_page_tree),
    },
];
