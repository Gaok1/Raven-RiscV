use super::super::TutorialStep;
use crate::ui::app::{App, VmSubtab};
use ratatui::layout::Rect;

fn whole(term: Rect, _app: &App) -> Option<Rect> {
    Some(term)
}

fn setup_stats(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Stats;
}
fn setup_config(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Settings;
    app.tlb.pending = app.run.mem().mmu().tlb.config.clone();
}
fn setup_entries(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Tlb;
}
fn setup_status(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Overview;
}
fn setup_vm_settings(app: &mut App) {
    // The walkthrough only makes sense with the scheme editable, so drop into
    // Custom mode and open the Settings panel. The [+]/[-] level controls and
    // the per-field edit boxes only appear in Custom.
    app.set_vm_mode(crate::falcon::mmu::VmMode::Custom);
    app.tlb.vm_subtab = VmSubtab::Settings;
}
fn setup_page_tree(app: &mut App) {
    // The demand-paging arc only makes sense under hardware-accurate semantics,
    // where the program drives satp and handles its own faults — switch to
    // Manual mode and open the live page-table view.
    app.set_vm_mode(crate::falcon::mmu::VmMode::Manual);
    app.tlb.vm_subtab = VmSubtab::Map;
}

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Virtual memory in a nutshell",
        title_pt: "Memória virtual em poucas palavras",
        body_en: "Programs use virtual addresses; RAM uses physical ones. The MMU translates between them one page at a time \
(a page is a fixed-size block — 4 KiB in classic Sv32). \
\nThe top bits of an address are the page number, the bottom bits are the offset inside the page. \
Translation only rewrites the page number: virtual VPN → physical PPN, while the offset passes through untouched. \
\nThe VPN→PPN map lives in RAM as a page table — a tree the MMU walks level by level. \
That walk costs several memory accesses, so a small cache called the TLB remembers recent translations. \
Hit the TLB → translate in ~1 cycle; miss → pay for the walk. This tab lets you watch and shape that whole machinery.",
        body_pt: "Programas usam endereços virtuais; a RAM usa físicos. A MMU traduz entre eles uma página por vez \
(uma página é um bloco de tamanho fixo — 4 KiB no Sv32 clássico). \
\nOs bits altos de um endereço são o número da página, os bits baixos são o offset dentro da página. \
A tradução só reescreve o número da página: VPN virtual → PPN físico, enquanto o offset passa intacto. \
\nO mapa VPN→PPN vive na RAM como uma tabela de páginas — uma árvore que a MMU percorre nível a nível. \
Esse passeio custa vários acessos à memória, então um cache pequeno chamado TLB guarda as traduções recentes. \
Hit na TLB → traduz em ~1 ciclo; miss → paga o passeio. Esta aba deixa você observar e moldar toda essa maquinaria.",
        target: whole,
        setup: Some(setup_status),
    },
    TutorialStep {
        title_en: "TLB tab — what it shows",
        title_pt: "Aba TLB — o que aparece",
        body_en: "The TLB tab visualizes the virtual-memory translation layer. \
Pick a VM mode in the global Settings tab — or in this tab's own settings subview — among Off / Sv32 / Custom / Manual, then assemble your program. \
\nSv32 and Custom are didactic: the simulator installs a page map and translates even M-mode accesses, \
so any program — even a simple loop — immediately shows TLB hits and misses with no setup code. \
Custom lets you reshape the paging scheme (levels + index/offset bits). \
Manual mode is hardware-accurate: your program drives satp and its own page tables.",
        body_pt: "A aba TLB mostra a camada de tradução de memória virtual. \
Escolha um modo de VM na aba Settings global — ou na subaba settings desta aba — entre Off / Sv32 / Custom / Manual e monte o programa. \
\nSv32 e Custom são didáticos: o simulador instala um mapeamento e traduz até acessos em M-mode, \
então qualquer programa — até um loop simples — já mostra hits e misses na TLB sem código extra. \
O modo Custom permite remodelar o esquema de paginação (níveis + bits de índice/offset). \
O modo Manual é fiel ao hardware: seu programa controla o satp e as próprias tabelas.",
        target: whole,
        setup: Some(setup_status),
    },
    TutorialStep {
        title_en: "Stats — hit rate over time",
        title_pt: "Stats — taxa de hit ao longo do tempo",
        body_en: "Counters: hits, misses, evictions, page faults. \
The chart is a 300-cycle rolling window of hit-rate, sampled once per committed instruction. \
Hit rate is meaningful only after a few hundred cycles with VM active. \
\nPress s to capture the current window as a session snapshot — the history table below lets you \
compare runs (↑↓ select, Enter opens the details popup, D deletes). Snapshots are shared with the \
Cache tab and land in the results export.",
        body_pt: "Contadores: hits, misses, evictions, page faults. \
O gráfico é uma janela de 300 ciclos da taxa de hit, amostrada por instrução. \
Só faz sentido após algumas centenas de ciclos com VM ativa. \
\nAperte s para capturar a janela atual como snapshot da sessão — a tabela de histórico abaixo permite \
comparar execuções (↑↓ seleciona, Enter abre o popup de detalhes, D apaga). Os snapshots são compartilhados \
com a aba Cache e entram no export de resultados.",
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
        title_en: "tlb — see installed translations",
        title_pt: "tlb — translations instaladas",
        body_en: "Each row is a TLB slot. Columns: VPN→PPN, R/W/X/U perms, ASID, G (global), A (accessed), D (dirty), Mp (megapage = 4 MiB). \
Run a program with VM on to populate the table. Use ↑/↓ or the mouse wheel to scroll.",
        body_pt: "Cada linha é um slot da TLB. Colunas: VPN→PPN, perms R/W/X/U, ASID, G (global), A (accessed), D (dirty), Mp (megapage = 4 MiB). \
Rode um programa com VM ligada para popular. Use ↑/↓ ou a roda do mouse para rolar.",
        target: whole,
        setup: Some(setup_entries),
    },
    TutorialStep {
        title_en: "overview — why is translation idle?",
        title_pt: "overview — por que a tradução está parada?",
        body_en: "The overview is the landing subtab: quick Mode and TLB controls at the top \
(click Mode and pick sv32 — no other setup needed), plus the live satp mode and privilege level. \
If the TLB looks empty, this page tells you why. \
A common gotcha in Manual mode: the program runs in M-mode and never touches satp, \
so the MMU stays in identity mode and nothing populates the TLB.",
        body_pt: "O overview é a subaba inicial: controles rápidos de Mode e TLB no topo \
(clique em Mode e escolha sv32 — nenhum outro setup é preciso), além do modo do satp e do privilégio ao vivo. \
Se a TLB parece vazia, esta página explica o porquê. \
Pegadinha comum no modo Manual: o programa roda em M-mode e nunca toca satp, \
então a MMU continua em identidade e nada popula a TLB.",
        target: whole,
        setup: Some(setup_status),
    },
    TutorialStep {
        title_en: "VM Settings — the control panel",
        title_pt: "VM Settings — o painel de controle",
        body_en: "This subview (now in Custom mode) is where you reshape virtual memory without writing a line of code. \
Three blocks, top to bottom: the paging scheme (the shape of the address translation), \
the page map (which virtual pages get installed, and with what permissions), and the TLB geometry. \
\nEverything is staged: edits go into a pending copy and only take effect when you hit apply (which reinstalls the map \
and re-points satp). flush tlb just drops cached translations without touching the map. \
Click any field to edit or toggle it; the next two steps walk the scheme and the map in detail.",
        body_pt: "Esta subaba (agora em modo Custom) é onde você remodela a memória virtual sem escrever uma linha de código. \
Três blocos, de cima para baixo: o esquema de paginação (o formato da tradução de endereços), \
o mapa de páginas (quais páginas virtuais são instaladas, e com quais permissões) e a geometria da TLB. \
\nTudo é preparado em rascunho: as edições vão para uma cópia pendente e só surtem efeito quando você aperta apply (que reinstala o mapa \
e reaponta o satp). flush tlb apenas descarta as traduções cacheadas sem mexer no mapa. \
Clique em qualquer campo para editar ou alternar; os dois próximos passos detalham o esquema e o mapa.",
        target: whole,
        setup: Some(setup_vm_settings),
    },
    TutorialStep {
        title_en: "Custom scheme — the paging math",
        title_pt: "Esquema Custom — a matemática da paginação",
        body_en: "A scheme splits the 32-bit virtual address into fields. offset bits decides the page size (page = 2^offset bytes; \
12 → 4 KiB). Each L# index is how many bits select an entry at that page-table level — one level per tree depth. \
Use levels [+]/[-] to add or drop a level. \
\nThe one hard rule: offset + every level's index bits must sum to exactly 32. The live readout shows the page size, \
the depth, and Σ with ✓/✗ — if it's red, apply is refused. \
\nWorked example: offset 12 + L1 10 + L0 10 = 32 → that's plain Sv32 (4 KiB pages, 2 levels). \
Bump the offset to 22 and drop to one level (L0 10) → 4 MiB megapages, a single-level walk. \
Fewer/larger pages mean shorter walks but coarser mapping — exactly the trade-off real ISAs make.",
        body_pt: "Um esquema fatia o endereço virtual de 32 bits em campos. offset bits define o tamanho da página (página = 2^offset bytes; \
12 → 4 KiB). Cada L# index é quantos bits selecionam uma entrada naquele nível da tabela — um nível por profundidade da árvore. \
Use levels [+]/[-] para adicionar ou remover um nível. \
\nA única regra rígida: offset + os bits de índice de cada nível devem somar exatamente 32. O readout ao vivo mostra o tamanho da página, \
a profundidade e o Σ com ✓/✗ — se estiver vermelho, o apply é recusado. \
\nExemplo resolvido: offset 12 + L1 10 + L0 10 = 32 → isso é o Sv32 puro (páginas de 4 KiB, 2 níveis). \
Suba o offset para 22 e fique com um nível (L0 10) → megapáginas de 4 MiB, um walk de nível único. \
Páginas maiores/menos numerosas significam walks mais curtos mas mapeamento mais grosso — exatamente o trade-off que as ISAs reais fazem.",
        target: whole,
        setup: Some(setup_vm_settings),
    },
    TutorialStep {
        title_en: "Page map — what gets mapped",
        title_pt: "Page map — o que é mapeado",
        body_en: "In Sv32/Custom the simulator auto-installs a page map so any program translates immediately. \
This block controls it. kind: identity maps VA→VA (PPN = VPN); offset shifts physical by a fixed offset MiB \
so you can watch VPN and PPN diverge in the Entries table. \
\nperms R/W/X/U are the permission bits stamped on every PTE — clear W to make a page read-only, set U to allow user-mode access. \
G marks entries global (shared across ASIDs, immune to ASID-scoped flushes); ASID tags the address space. \
\nChange anything, hit apply, then reassemble or step — the Entries and Page-tree subviews show the result. \
This is the safe sandbox: break the mapping here and nothing crashes, you just see faults you can reason about.",
        body_pt: "Em Sv32/Custom o simulador instala automaticamente um mapa de páginas para que qualquer programa já traduza. \
Este bloco o controla. kind: identity mapeia VA→VA (PPN = VPN); offset desloca o físico por um offset MiB fixo \
para você ver VPN e PPN divergirem na tabela Entries. \
\nperms R/W/X/U são os bits de permissão gravados em cada PTE — limpe o W para deixar a página somente-leitura, marque U para permitir acesso em modo usuário. \
G marca entradas como globais (compartilhadas entre ASIDs, imunes a flushes por ASID); ASID identifica o espaço de endereçamento. \
\nMude qualquer coisa, aperte apply, e remonte ou avance — as subabas Entries e Árvore de páginas mostram o resultado. \
Este é o sandbox seguro: quebre o mapeamento aqui e nada trava, você só vê faults que consegue raciocinar.",
        target: whole,
        setup: Some(setup_vm_settings),
    },
    TutorialStep {
        title_en: "Page tree — the live page table",
        title_pt: "Árvore de páginas — a tabela ao vivo",
        body_en: "The map subview walks the real page table rooted at satp.PPN, straight from RAM, \
following the active scheme's N levels: pointer PTEs expand into child tables, leaves at any level are (super)pages; \
long runs of uniform leaves collapse into one summary line; PTEs cached in the TLB are marked ●TLB. \
The tree is read-only — edit the map and scheme in the settings subview. \
This is your window into what the MMU actually sees, invaluable when a mapping isn't taking effect.",
        body_pt: "A subaba map percorre a tabela de páginas real ancorada em satp.PPN, direto da RAM, \
seguindo os N níveis do esquema ativo: PTEs ponteiro expandem em tabelas filhas, folhas em qualquer nível são (super)páginas; \
sequências longas de folhas uniformes colapsam numa linha-resumo; PTEs cacheadas na TLB ganham a marca ●TLB. \
A árvore é somente-leitura — edite o mapa e o esquema na subaba settings. \
É sua janela para o que a MMU realmente enxerga, essencial quando um mapeamento não surte efeito.",
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
The full runnable kernel is laid out step by step in the virtual-memory guide.",
        body_pt: "O modo Manual libera o padrão clássico de SO. Delegue page faults de load/store ao modo supervisor \
setando o bit correspondente em medeleg (bit 13 = load fault, 15 = store) e apontando stvec para seu handler. \
Quando o U-mode toca uma página não-mapeada, a CPU vetoriza para stvec em S-mode (salvando sepc/scause/stval), \
o handler instala a PTE faltante, roda sfence.vma, e `sret` retorna para repetir o acesso — agora ele funciona. \
O kernel completo executável está detalhado passo a passo no guia de memória virtual.",
        target: whole,
        setup: Some(setup_page_tree),
    },
];
