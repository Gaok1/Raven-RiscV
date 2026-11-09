use super::super::TutorialStep;
use crate::ui::app::{App, VmSubtab};
use ratatui::layout::Rect;

fn whole(term: Rect, _app: &App) -> Option<Rect> {
    Some(term)
}

fn setup_walk(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Walk;
}
fn setup_tlb(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Tlb;
}
fn setup_map(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Map;
}
fn setup_stats(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Stats;
}
fn setup_config(app: &mut App) {
    app.tlb.vm_subtab = VmSubtab::Config;
    if let Some(rv32) = app.rv32() {
        app.tlb.pending = rv32.mem().mmu().tlb.config.clone();
    }
}

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Virtual memory in a nutshell",
        title_pt: "Memória virtual em poucas palavras",
        body_en: "Programs use virtual addresses; RAM (the main memory) uses physical ones. The MMU (Memory Management Unit) translates between them one page at a time \
(a page is a fixed-size block — 4 KiB in Sv32, RISC-V's paging scheme for 32-bit machines). \
\nThe top bits of an address are the page number, the bottom bits are the offset inside the page. \
Translation only rewrites the page number: the VPN (virtual page number) becomes a PPN (physical page number), while the offset passes through untouched. \
\nThe VPN→PPN map lives in RAM as a page table — a tree the MMU walks level by level. \
That walk costs several memory accesses, so a small cache called the TLB (Translation Lookaside Buffer) remembers recent translations. \
Hit the TLB → translate in ~1 cycle; miss → pay for the walk. This tab lets you watch every step. \
\nPress ? on this tab for the full guide.",
        body_pt: "Programas usam endereços virtuais; a RAM (a memória principal) usa físicos. A MMU (Memory Management Unit, unidade de gerenciamento de memória) traduz entre eles uma página por vez \
(uma página é um bloco de tamanho fixo — 4 KiB no Sv32, o esquema de paginação do RISC-V para máquinas de 32 bits). \
\nOs bits altos de um endereço são o número da página, os bits baixos são o offset dentro da página. \
A tradução só reescreve o número da página: o VPN (virtual page number, número da página virtual) vira um PPN (physical page number, número da página física), enquanto o offset passa intacto. \
\nO mapa VPN→PPN vive na RAM como uma tabela de páginas — uma árvore que a MMU percorre nível a nível. \
Esse percurso custa vários acessos à memória, então um cache pequeno chamado TLB (Translation Lookaside Buffer) guarda as traduções recentes. \
Hit na TLB → traduz em ~1 ciclo; miss → paga o percurso. Esta aba deixa você ver cada passo. \
\nAperte ? nesta aba para o guia completo.",
        target: whole,
        setup: Some(setup_walk),
    },
    TutorialStep {
        title_en: "Turning it on",
        title_pt: "Ligando",
        body_en: "Pick a mode in config (or click turn on in the header): \
\n• auto — the simulator writes the page table when your program loads and translates even in M-mode (machine mode, where programs start), \
so any program, even a three-line loop, shows the TLB at work with no setup code. \
\n• program — real RISC-V: your program writes its own tables and satp (the register that says where the root table is), and M-mode bypasses translation. \
\nThe header always says whether addresses are being translated, and when they are not, why — with the fix one click away.",
        body_pt: "Escolha um modo em config (ou clique em turn on no cabeçalho): \
\n• auto — o simulador escreve a tabela de páginas quando o programa carrega e traduz até em M-mode (modo máquina, onde os programas começam), \
então qualquer programa, até um loop de três linhas, mostra a TLB trabalhando sem código de setup. \
\n• program — RISC-V de verdade: seu programa escreve as próprias tabelas e o satp (o registrador que diz onde a tabela raiz está), e o M-mode não traduz. \
\nO cabeçalho sempre diz se os endereços estão sendo traduzidos e, quando não estão, por quê — com a correção a um clique.",
        target: whole,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "walk — one access, step by step",
        title_pt: "walk — um acesso, passo a passo",
        body_en: "The walk subtab replays the latest translation. The virtual address is split into VPN[1], VPN[0] and offset, \
each in its own colour — the same colour marks the table entry that field indexes, and the offset keeps its colour into the physical address. \
\nThen the steps: which TLB set was probed and whether a way matched; on a miss, every PTE (page-table entry) the walk read, with its address, its value, \
and where the read was served (L1, L2 or RAM) at what cost; the physical address it built; and what the TLB kept and evicted. A fault stops at the step that failed and says why.",
        body_pt: "A subaba walk mostra de novo a última tradução. O endereço virtual é quebrado em VPN[1], VPN[0] e offset, \
cada um com sua cor — a mesma cor marca a entrada da tabela que aquele campo indexa, e o offset leva a cor até o endereço físico. \
\nDepois vêm os passos: qual set da TLB foi consultado e se alguma via bateu; no miss, cada PTE (page-table entry, entrada da tabela de páginas) que o walk leu, com endereço, valor \
e onde a leitura foi atendida (L1, L2 ou RAM) e a que custo; o endereço físico montado; e o que a TLB guardou e despejou. Um fault para no passo que falhou e diz por quê.",
        target: whole,
        setup: Some(setup_walk),
    },
    TutorialStep {
        title_en: "Following, pinning, history",
        title_pt: "Seguir, fixar, histórico",
        body_en: "While the program runs, the views follow the newest data access (switch to fetch or any with the follow chips, or v). \
The history below keeps the last 64 translations — filter it to misses or faults with 1/2/3. \
\nClick a row, or step with ← →, to pin that translation: the walk, the TLB matrix and the map all show it. \
Press g anywhere to go back to following the current access.",
        body_pt: "Com o programa rodando, as vistas seguem o acesso a dados mais recente (troque para fetch ou any nos chips de follow, ou com v). \
O histórico abaixo guarda as últimas 64 traduções — filtre por misses ou faults com 1/2/3. \
\nClique numa linha, ou ande com ← →, para fixar aquela tradução: o walk, a matriz da TLB e o mapa passam a mostrá-la. \
Aperte g em qualquer lugar para voltar a seguir o acesso atual.",
        target: whole,
        setup: Some(setup_walk),
    },
    TutorialStep {
        title_en: "tlb — sets and ways",
        title_pt: "tlb — sets e vias",
        body_en: "The TLB drawn the way the Cache tab draws a cache: one row per set, one column per way. \
A 4 KiB page lives in set VPN mod sets; a 4 MiB megapage in (VPN >> 10) mod sets — so a lookup probes two sets, both marked ▸. \
\n● marks the entry the current access installed, ✓ the one it hit; the number in each cell is its recency (1 = most recent). \
Move with the arrows; the panel below spells out the selected entry in full.",
        body_pt: "A TLB desenhada como a aba Cache desenha um cache: uma linha por set, uma coluna por via. \
Uma página de 4 KiB fica no set VPN mod sets; uma megapágina de 4 MiB em (VPN >> 10) mod sets — por isso uma consulta olha dois sets, ambos marcados com ▸. \
\n● marca a entrada que o acesso atual instalou, ✓ a que ele acertou; o número em cada célula é a recência (1 = mais recente). \
Ande com as setas; o painel de baixo detalha a entrada selecionada.",
        target: whole,
        setup: Some(setup_tlb),
    },
    TutorialStep {
        title_en: "map — the page table itself",
        title_pt: "map — a própria tabela de páginas",
        body_en: "One column per level, read live from memory: the root (VPN[1]) on the left, the table it points to (VPN[0]) beside it, \
and the selected PTE decoded field by field on the right. Runs of alike entries fold into one row (Enter opens them). \
\nThe path of the current access is marked ▸; entries the TLB holds are marked ●. \
Press / to type any address and see the path it would take.",
        body_pt: "Uma coluna por nível, lida ao vivo da memória: a raiz (VPN[1]) à esquerda, a tabela para onde ela aponta (VPN[0]) ao lado, \
e a PTE selecionada decodificada campo a campo à direita. Sequências de entradas iguais viram uma linha só (Enter abre). \
\nO caminho do acesso atual fica marcado com ▸; entradas que estão na TLB, com ●. \
Aperte / para digitar qualquer endereço e ver o caminho que ele faria.",
        target: whole,
        setup: Some(setup_map),
    },
    TutorialStep {
        title_en: "stats — what translation cost",
        title_pt: "stats — quanto custou traduzir",
        body_en: "Hits, misses and the hit rate over time, plus the cost: cycles spent translating and their share of the whole run. \
Reach compares the pages the program has touched with the entries the TLB has. \
\nWhy it missed splits every miss by cause: a page's first touch (unavoidable), a page the TLB held and lost (a bigger TLB helps), \
a store to a clean page (the walk sets D), or no mapping at all (a page fault). \
Press s to capture a snapshot and compare runs — the list is shared with the Cache tab.",
        body_pt: "Hits, misses e a taxa de hit ao longo do tempo, mais o custo: ciclos gastos traduzindo e a fatia deles no total. \
O alcance compara as páginas que o programa tocou com as entradas que a TLB tem. \
\nWhy it missed separa cada miss pela causa: primeiro toque numa página (inevitável), página que a TLB tinha e perdeu (uma TLB maior ajuda), \
store numa página limpa (o walk liga o D), ou nenhum mapeamento (page fault). \
Aperte s para guardar um snapshot e comparar execuções — a lista é compartilhada com a aba Cache.",
        target: whole,
        setup: Some(setup_stats),
    },
    TutorialStep {
        title_en: "config — everything applies at once",
        title_pt: "config — tudo vale na hora",
        body_en: "Every option is on screen with the chosen one lit, and every change applies immediately — there is no apply button. \
\nIn auto mode the page map's permissions are yours to break: clear W and the next store page-faults, with the walk showing exactly where. \
Entries, ways and policy rebuild the TLB empty; hit and walk cycles change what each translation costs.",
        body_pt: "Todas as opções ficam na tela, com a escolhida acesa, e toda mudança vale na hora — não existe botão apply. \
\nNo modo auto as permissões do mapa são suas para quebrar: desligue o W e o próximo store dá page fault, com o walk mostrando exatamente onde. \
Entradas, vias e política reconstroem a TLB vazia; os ciclos de hit e de walk mudam quanto cada tradução custa.",
        target: whole,
        setup: Some(setup_config),
    },
    TutorialStep {
        title_en: "Demand paging — fault, map, retry",
        title_pt: "Demand paging — falha, mapeia, repete",
        body_en: "Program mode unlocks the classic OS (operating system) pattern. Delegate load/store page faults to supervisor mode \
by setting the matching bit in medeleg, the machine's exception delegation register (bit 13 = load fault, 15 = store), and pointing stvec, the supervisor's trap vector, at your handler. \
When U-mode (user mode) touches an unmapped page, the CPU vectors to stvec in S-mode (supervisor mode), saving where the faulting instruction is, why it faulted and the address it touched in sepc, scause and stval; \
the handler installs the missing PTE, runs sfence.vma, and `sret` returns to retry the faulting access — now it succeeds. \
Filter the history to faults to watch each one happen. The full runnable kernel is in the virtual-memory guide.",
        body_pt: "O modo program libera o padrão clássico de SO (sistema operacional). Delegue page faults de load/store ao modo supervisor \
setando o bit correspondente em medeleg, o registrador de delegação de exceções da máquina (bit 13 = load fault, 15 = store), e apontando stvec, o vetor de traps do supervisor, para seu handler. \
Quando o U-mode (modo usuário) toca uma página não-mapeada, a CPU vetoriza para stvec em S-mode (modo supervisor), salvando onde está a instrução que falhou, a causa e o endereço que ela tocou em sepc, scause e stval; \
o handler instala a PTE faltante, roda sfence.vma, e `sret` retorna para repetir o acesso — agora ele funciona. \
Filtre o histórico por faults para ver cada um acontecer. O kernel completo executável está no guia de memória virtual.",
        target: whole,
        setup: Some(setup_map),
    },
];
