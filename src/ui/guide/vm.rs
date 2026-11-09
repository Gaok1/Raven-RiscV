// ui/guide/vm.rs — the Virtual Memory tab's guide, in English and Portuguese.
//
// Written to be read top to bottom: each acronym is spelled out where it first
// appears, the sections follow the subtabs in order, and the glossary at the
// end repeats every acronym for a reader who jumped in halfway.

use super::Block::{H, Keys, List, P, Pre};
use super::Section;

const ADDRESS_EN: &str = "\
 31          22 21          12 11              0
┌──────────────┬──────────────┬─────────────────┐
│    VPN[1]    │    VPN[0]    │      offset     │
│   10 bits    │   10 bits    │     12 bits     │
└──────────────┴──────────────┴─────────────────┘
   which root    which entry      which byte
     entry        of level 2      of the page";

const ADDRESS_PT: &str = "\
 31          22 21          12 11              0
┌──────────────┬──────────────┬─────────────────┐
│    VPN[1]    │    VPN[0]    │      offset     │
│   10 bits    │   10 bits    │     12 bits     │
└──────────────┴──────────────┴─────────────────┘
    entrada        entrada           byte
    da raiz       do nível 2       da página";

// ── English ──────────────────────────────────────────────────────────────────

pub(super) const EN: &[Section] = &[
    Section {
        title: "Start here",
        blocks: &[
            P("This tab shows **virtual memory** at work: every address the program uses is translated before it reaches memory, and you can watch each translation happen."),
            P("The translating is done by the **MMU** (Memory Management Unit). An address the program uses is a **VA** (virtual address); the MMU turns it into a **PA** (physical address), and only the PA goes on to the caches and to RAM (the main memory)."),
            H("Getting started"),
            List(&[
                "Virtual memory starts off. Click `turn on` in the header, or choose a mode in `config`.",
                "Press `s` to step to the next translation, or `p` to run and pause. Every instruction is translated once to be fetched, and once more if it loads or stores.",
                "`walk` then shows the newest translation step by step. The other subtabs show the same moment from other angles.",
            ]),
            H("The subtabs"),
            Keys(&[
                ("walk", "one translation, step by step, and the ones before it"),
                ("tlb", "the TLB (Translation Lookaside Buffer): a small cache of recent translations"),
                ("map", "the page table the translations are read from"),
                ("stats", "how often the TLB hits, what the misses cost, and why they happen"),
                ("config", "the mode, the TLB's shape and what the pages allow"),
            ]),
            P("Switch with `Tab` and `Shift+Tab`, or click a name. What you select in one subtab (an access, a TLB entry, a table entry) stays selected in the others. `g` goes back to following the newest access."),
        ],
    },
    Section {
        title: "How translation works",
        blocks: &[
            P("Raven uses **Sv32**, the paging scheme RISC-V defines for 32-bit machines. Memory is split into **pages** of 4 KiB, and a VA is read as three fields:"),
            Pre(ADDRESS_EN),
            P("Together, the two upper fields are the **VPN** (virtual page number): which page. The **offset** says where inside the page; it is never translated."),
            H("1. Look in the TLB"),
            P("The **TLB** keeps recent translations, each from a VPN to a **PPN** (physical page number). If it holds this VPN, that is a **hit**, and the PA is ready almost at once."),
            H("2. On a miss, walk the page table"),
            P("The **page table** sits in memory in two levels, each a table of 1024 entries. The **satp** register (Supervisor Address Translation and Protection) says where the first one, the **root**, starts. Reading the table is called a **walk**: the walker reads one **PTE** (page-table entry) per level."),
            List(&[
                "VPN[1] picks an entry of the root. It either points to a second-level table, or is itself a **leaf** that maps a whole 4 MiB **megapage**.",
                "VPN[0] picks an entry of that second table: a leaf that maps one 4 KiB page.",
            ]),
            P("Each PTE read is a load like any other. It goes through the D-cache (data cache), so a walk costs more when its PTEs are not cached."),
            H("3. Check, then remember"),
            P("The leaf's permission bits must allow the access: **R** read, **W** write, **X** execute, and **U** for user mode. If they do not, the access raises a **page fault**. Otherwise the walker sets **A** (accessed) in the PTE, and **D** (dirty) on a store, then stores the translation in the TLB so the next access to that page hits."),
            P("The PA is the leaf's frame plus the offset."),
        ],
    },
    Section {
        title: "walk: one translation",
        blocks: &[
            P("The **Access** panel shows one translation. Its first lines say which instruction asked for it and where that instruction is (its **PC**, program counter), whether it was a fetch, a load or a store, and the privilege mode it ran in: **M** (machine), **S** (supervisor) or **U** (user)."),
            P("Next comes the VA split into its fields, in hex and in bits. Each field keeps its colour on every line that uses it."),
            P("Then the numbered steps:"),
            Keys(&[
                ("TLB", "the set it looked in, and whether a way matched: HIT or MISS"),
                ("root", "where satp says the root table is"),
                ("VPN[1]", "the root PTE: its index, its address, its raw value and what it means"),
                ("VPN[0]", "the second-level PTE, when the walk got that far"),
                ("PA", "the physical address as frame + offset, and what the whole translation cost"),
                ("TLB ←", "where the translation was stored, and the entry it pushed out"),
                ("fault", "why the translation failed, and the exception it raised"),
            ]),
            P("Each PTE line ends with the level that answered the read and its cost: `L1 +1` is a hit in the L1 (level 1) cache, `RAM +12` a trip to memory. With the caches off (the default; turn them on in Settings) a read has no cost of its own and the line just says `RAM`: the walk then costs the walker's own cycles, set in `config`."),
            H("Following and pinning"),
            P("By default the panel follows the newest access of one kind: `data` (loads and stores), `fetch`, or `any`. Click one, or press `v`. `←` and `→` move to older and newer translations; that **pins** the one you land on, and every subtab shows it. `g` follows again."),
            H("History"),
            P("Below the access, the last translations, newest first. `1`, `2` and `3` show all of them, only the misses, or only the faults. Click a row to pin it."),
        ],
    },
    Section {
        title: "tlb: the TLB itself",
        blocks: &[
            P("The TLB as a grid: one row per **set**, one column per **way**. A page can only be cached in one set, its VPN modulo the number of sets, so a lookup only compares the ways of that set."),
            P("A megapage is filed by VPN[1] alone, so every lookup checks two sets: the one a 4 KiB page would be in, and the one a 4 MiB page would be in."),
            Keys(&[
                ("4K  4M", "the size of the page an entry maps"),
                ("00400→00400", "the VPN, and the PPN it translates to"),
                ("R W X U", "what the page allows"),
                ("1 2 3", "recency in its set: 1 was used last. Under LRU (least recently used), the highest number is the next to go"),
                ("●", "stored by the current access"),
                ("✓", "hit by the current access"),
                ("▸", "the set the current access looked in"),
            ]),
            P("Move with the arrow keys or click an entry; the panel under the grid explains it. When the grid does not fit the screen, it turns into a row of sets with the chosen set's ways listed below."),
        ],
    },
    Section {
        title: "map: the page table",
        blocks: &[
            P("The page table, read the way the walker reads it: one column per level, root first. Select an entry; if it points to a table, the next column opens on that table. `→` or `Enter` goes in, `←` comes back."),
            P("Long runs of similar entries, such as invalid ones or leaves mapping consecutive frames, fold into one row, so you see the table's shape instead of a thousand lines. `Enter` unfolds a run."),
            Keys(&[
                ("▸", "on the path of the access the views are showing"),
                ("●", "the TLB holds this translation"),
                ("×1022", "a folded run of that many similar entries"),
            ]),
            P("The detail panel takes the selected PTE apart, field by field:"),
            Keys(&[
                ("PPN", "where the page's frame, or the next table, is"),
                ("RSW", "reserved for software: two bits left free for the operating system"),
                ("D", "dirty: the page has been written"),
                ("A", "accessed: the page has been used"),
                ("G", "global: valid in every address space"),
                ("U", "user mode may use the page"),
                ("X W R", "execute, write, read"),
                ("V", "valid: without it, nothing is mapped"),
            ]),
            P("Press `/` and type an address in hex: the columns follow the path its translation takes."),
        ],
    },
    Section {
        title: "stats: what it cost",
        blocks: &[
            Keys(&[
                ("hit rate", "hits out of all lookups"),
                ("cost", "cycles spent walking, and their share of the whole run"),
                ("reach", "how many pages the TLB can hold, against how many the program has touched"),
                ("shape", "sets × ways, and the replacement policy"),
            ]),
            H("Why it missed"),
            Keys(&[
                ("first touch", "the page had never been used. No TLB avoids these"),
                ("held before, lost", "the TLB had the page and lost it: evicted, flushed, or pushed out by another address space. A bigger TLB, more ways or another policy help here"),
                ("store to a clean page", "the TLB had the page, but the store walked again to set D"),
                ("no mapping", "the walk found nothing valid: a page fault"),
            ]),
            P("The chart is the hit rate over time. `s` saves a snapshot of these numbers; `↑` and `↓` pick one, `Enter` opens it and `d` deletes it."),
        ],
    },
    Section {
        title: "config: modes and shape",
        blocks: &[
            P("Every change takes effect at once; there is no apply button."),
            H("Mode"),
            Keys(&[
                ("off", "no translation: every address is already physical"),
                ("auto", "Raven builds an identity page table, where each VA maps to the same PA, and translates even in M-mode, so any program can be watched. The program's code and data get 4 KiB pages; the rest of memory gets megapages"),
                ("program", "real RISC-V: the program writes satp and switches to S- or U-mode itself. Nothing is translated until it does"),
            ]),
            H("TLB"),
            Keys(&[
                ("state", "`off` makes every access walk"),
                ("entries", "how many translations it holds"),
                ("ways", "entries per set: 1 is direct-mapped, `all` is fully associative"),
                ("policy", "which way a full set gives up: LRU, FIFO (first in, first out), Random, Clock, MRU (most recently used) or LFU (least frequently used)"),
                ("cycles", "what a hit costs, and the walker's own cycles on a miss. The PTE reads are charged by the caches on top of that"),
            ]),
            P("A new number of entries, ways or policy rebuilds the TLB: its entries and counters start over. New cycle counts keep what it holds."),
            H("Page map (auto mode)"),
            P("What the pages allow (R, W, X, U), whether they are global, and the **ASID** (address-space identifier) they belong to. Changing any of them rewrites the table and flushes the TLB."),
            P("`m` cycles the mode. Click a number, or press `e`, to type a new one: `Enter` keeps it, `Esc` drops it."),
        ],
    },
    Section {
        title: "Several cores",
        blocks: &[
            P("Each core has a TLB of its own, as in hardware. `[` and `]` choose the core whose TLB, statistics and translations this tab shows."),
            P("`flush tlb` and a new TLB shape apply to every core. A program's `sfence.vma` flushes only the TLB of the core that ran it."),
        ],
    },
    Section {
        title: "Keys and mouse",
        blocks: &[
            Keys(&[
                ("Tab  Shift+Tab", "next / previous subtab"),
                ("s", "step to the next translation (in stats: save a snapshot)"),
                ("b", "go back one step"),
                ("p  Space", "run / pause"),
                ("r", "restart the program"),
                ("f", "speed"),
                ("g", "follow the newest access again"),
                ("[  ]", "previous / next core"),
                ("←  →", "walk: older / newer translation"),
                ("v", "walk: which kind of access to follow"),
                ("1  2  3", "walk: show all / misses / faults"),
                ("arrows", "tlb and map: move the selection"),
                ("Enter", "map: open a run or a table · stats: open a snapshot"),
                ("/", "map: find an address"),
                ("m  e", "config: next mode / edit a number"),
                ("Ctrl+e  Ctrl+l", "export / import the cache and TLB setup (.fcache)"),
                ("Ctrl+r", "export the results (.fstats or .csv)"),
                ("?", "this guide"),
            ]),
            P("Everything is clickable as well, and the mouse wheel scrolls whatever is under the pointer."),
        ],
    },
    Section {
        title: "Glossary",
        blocks: &[Keys(&[
            ("A  D", "accessed and dirty: PTE bits the walker sets on a page's first use and first write"),
            ("ASID", "address-space identifier: tags TLB entries so several programs' pages can share the TLB"),
            ("CPI", "cycles per instruction"),
            ("D-cache", "data cache: where loads, stores and PTE reads go"),
            ("FIFO", "first in, first out"),
            ("KiB  MiB", "1024 bytes; 1024 KiB"),
            ("L1  L2", "level 1 and level 2 caches; L1 is the closest and fastest"),
            ("LFU", "least frequently used"),
            ("LRU", "least recently used"),
            ("M  S  U", "machine, supervisor and user: RISC-V's privilege modes"),
            ("megapage", "a 4 MiB page, mapped by a leaf in the root table"),
            ("MMU", "Memory Management Unit: the hardware that translates addresses"),
            ("MRU", "most recently used"),
            ("PA", "physical address"),
            ("page fault", "the exception raised when a translation fails"),
            ("PC", "program counter: the address of the instruction running"),
            ("PPN", "physical page number"),
            ("PTE", "page-table entry"),
            ("RAM", "main memory"),
            ("satp", "Supervisor Address Translation and Protection: the register holding the root table's address, the ASID and the mode"),
            ("sfence.vma", "the instruction that flushes TLB entries"),
            ("Sv32", "RISC-V's two-level paging scheme for 32-bit addresses"),
            ("TLB", "Translation Lookaside Buffer: a cache of recent translations"),
            ("VA", "virtual address"),
            ("VPN", "virtual page number"),
            ("walk", "reading the page table, level by level, to translate a VA"),
        ])],
    },
];

// ── Português ────────────────────────────────────────────────────────────────

pub(super) const PT: &[Section] = &[
    Section {
        title: "Comece aqui",
        blocks: &[
            P("Esta aba mostra a **memória virtual** funcionando: todo endereço que o programa usa é traduzido antes de chegar à memória, e dá para ver cada tradução acontecer."),
            P("Quem traduz é a **MMU** (Memory Management Unit, unidade de gerenciamento de memória). O endereço que o programa usa é um **VA** (virtual address, endereço virtual); a MMU o transforma num **PA** (physical address, endereço físico), e só o PA segue para as caches e para a RAM (a memória principal)."),
            H("Para começar"),
            List(&[
                "A memória virtual começa desligada. Clique em `turn on` no cabeçalho ou escolha um modo em `config`.",
                "Aperte `s` para avançar até a próxima tradução, ou `p` para rodar e pausar. Toda instrução é traduzida uma vez para ser buscada, e mais uma se fizer load ou store.",
                "A `walk` mostra então a tradução mais recente, passo a passo. As outras subabas mostram o mesmo momento por outros ângulos.",
            ]),
            H("As subabas"),
            Keys(&[
                ("walk", "uma tradução passo a passo, e as anteriores"),
                ("tlb", "a TLB (Translation Lookaside Buffer): uma pequena cache das traduções recentes"),
                ("map", "a tabela de páginas de onde as traduções saem"),
                ("stats", "quantas vezes a TLB acerta, quanto custam as falhas e por que elas acontecem"),
                ("config", "o modo, o formato da TLB e o que as páginas permitem"),
            ]),
            P("Troque com `Tab` e `Shift+Tab`, ou clique no nome. O que você seleciona numa subaba (um acesso, uma entrada da TLB, uma entrada da tabela) continua selecionado nas outras. `g` volta a acompanhar o acesso mais recente."),
        ],
    },
    Section {
        title: "Como a tradução funciona",
        blocks: &[
            P("O Raven usa o **Sv32**, o esquema de paginação que o RISC-V define para máquinas de 32 bits. A memória é dividida em **páginas** de 4 KiB, e o VA é lido como três campos:"),
            Pre(ADDRESS_PT),
            P("Juntos, os dois campos de cima formam o **VPN** (virtual page number, número da página virtual): qual página. O **offset** diz onde dentro da página, e nunca é traduzido."),
            H("1. Procurar na TLB"),
            P("A **TLB** guarda traduções recentes, cada uma de um VPN para um **PPN** (physical page number, número da página física). Se ela tem esse VPN, é um **hit** (acerto), e o PA sai quase na hora."),
            H("2. Numa falha, percorrer a tabela de páginas"),
            P("A **tabela de páginas** fica na memória, em dois níveis, cada um uma tabela de 1024 entradas. O registrador **satp** (Supervisor Address Translation and Protection) diz onde começa a primeira, a **raiz**. Ler a tabela se chama **walk** (percurso): o walker lê uma **PTE** (page-table entry, entrada da tabela de páginas) por nível."),
            List(&[
                "O VPN[1] escolhe uma entrada da raiz. Ela aponta para uma tabela de segundo nível, ou é ela mesma uma **folha** que mapeia uma **megapágina** inteira de 4 MiB.",
                "O VPN[0] escolhe uma entrada dessa segunda tabela: uma folha que mapeia uma página de 4 KiB.",
            ]),
            P("Cada leitura de PTE é um load como qualquer outro. Ela passa pela D-cache (cache de dados), então um walk custa mais quando as PTEs não estão em cache."),
            H("3. Conferir e guardar"),
            P("Os bits de permissão da folha precisam permitir o acesso: **R** leitura, **W** escrita, **X** execução, e **U** para o modo usuário. Se não permitirem, o acesso gera um **page fault** (falta de página). Senão, o walker marca **A** (accessed, acessada) na PTE, e **D** (dirty, suja) num store, e guarda a tradução na TLB, para o próximo acesso a essa página acertar."),
            P("O PA é o frame da folha mais o offset."),
        ],
    },
    Section {
        title: "walk: uma tradução",
        blocks: &[
            P("O painel **Access** mostra uma tradução. As primeiras linhas dizem qual instrução pediu e onde ela está (o **PC**, program counter), se foi fetch, load ou store, e o modo de privilégio em que ela rodou: **M** (máquina), **S** (supervisor) ou **U** (usuário)."),
            P("Em seguida vem o VA dividido nos seus campos, em hexa e em bits. Cada campo mantém a mesma cor em todas as linhas que o usam."),
            P("Depois, os passos numerados:"),
            Keys(&[
                ("TLB", "o set consultado, e se alguma via bateu: HIT ou MISS"),
                ("root", "onde o satp diz que a tabela raiz está"),
                ("VPN[1]", "a PTE da raiz: índice, endereço, valor bruto e o que ela significa"),
                ("VPN[0]", "a PTE do segundo nível, quando o walk chegou lá"),
                ("PA", "o endereço físico como frame + offset, e quanto a tradução inteira custou"),
                ("TLB ←", "onde a tradução foi guardada, e a entrada que ela expulsou"),
                ("fault", "por que a tradução falhou, e a exceção que ela gerou"),
            ]),
            P("Cada linha de PTE termina com o nível que respondeu a leitura e o custo: `L1 +1` é um acerto na cache L1 (nível 1), `RAM +12` é uma ida à memória. Com as caches desligadas (o padrão; ligue em Settings) a leitura não tem custo próprio e a linha diz só `RAM`: o walk custa então os ciclos do próprio walker, definidos em `config`."),
            H("Acompanhar e fixar"),
            P("Por padrão o painel acompanha o acesso mais recente de um tipo: `data` (loads e stores), `fetch` ou `any`. Clique num deles ou aperte `v`. `←` e `→` vão para traduções mais antigas e mais novas; isso **fixa** a tradução escolhida, e todas as subabas passam a mostrá-la. `g` volta a acompanhar."),
            H("Histórico"),
            P("Abaixo do acesso ficam as últimas traduções, da mais nova para a mais antiga. `1`, `2` e `3` mostram todas, só as falhas ou só os page faults. Clique numa linha para fixá-la."),
        ],
    },
    Section {
        title: "tlb: a TLB",
        blocks: &[
            P("A TLB em forma de grade: uma linha por **set** (conjunto), uma coluna por **via**. Uma página só pode ficar num set, o VPN módulo o número de sets, então uma busca só compara as vias desse set."),
            P("Uma megapágina é guardada só pelo VPN[1], por isso toda busca olha dois sets: o de uma página de 4 KiB e o de uma de 4 MiB."),
            Keys(&[
                ("4K  4M", "o tamanho da página que a entrada mapeia"),
                ("00400→00400", "o VPN, e o PPN para o qual ele traduz"),
                ("R W X U", "o que a página permite"),
                ("1 2 3", "recência no set: 1 foi a usada por último. Com LRU (least recently used, a usada há mais tempo sai primeiro), o maior número é o próximo a sair"),
                ("●", "guardada pelo acesso atual"),
                ("✓", "acertada pelo acesso atual"),
                ("▸", "o set que o acesso atual consultou"),
            ]),
            P("Mova com as setas ou clique numa entrada; o painel abaixo da grade a explica. Quando a grade não cabe na tela, ela vira uma fileira de sets com as vias do set escolhido listadas embaixo."),
        ],
    },
    Section {
        title: "map: a tabela de páginas",
        blocks: &[
            P("A tabela de páginas lida como o walker a lê: uma coluna por nível, a raiz primeiro. Selecione uma entrada; se ela aponta para uma tabela, a coluna seguinte abre nessa tabela. `→` ou `Enter` entram, `←` volta."),
            P("Sequências longas de entradas parecidas, como as inválidas ou folhas que mapeiam frames consecutivos, viram uma linha só, para você ver o formato da tabela em vez de mil linhas. `Enter` abre a sequência."),
            Keys(&[
                ("▸", "no caminho do acesso que as vistas estão mostrando"),
                ("●", "a TLB guarda essa tradução"),
                ("×1022", "uma sequência dobrada com essa quantidade de entradas parecidas"),
            ]),
            P("O painel de detalhe desmonta a PTE selecionada, campo por campo:"),
            Keys(&[
                ("PPN", "onde está o frame da página, ou a próxima tabela"),
                ("RSW", "reserved for software: dois bits livres para o sistema operacional"),
                ("D", "dirty: a página foi escrita"),
                ("A", "accessed: a página foi usada"),
                ("G", "global: vale em todos os espaços de endereçamento"),
                ("U", "o modo usuário pode usar a página"),
                ("X W R", "execução, escrita, leitura"),
                ("V", "valid: sem ele, nada está mapeado"),
            ]),
            P("Aperte `/` e digite um endereço em hexa: as colunas seguem o caminho que a tradução dele faz."),
        ],
    },
    Section {
        title: "stats: quanto custou",
        blocks: &[
            Keys(&[
                ("hit rate", "acertos sobre o total de buscas"),
                ("cost", "ciclos gastos em walks, e a parcela deles na execução inteira"),
                ("reach", "quantas páginas a TLB comporta, contra quantas o programa já tocou"),
                ("shape", "sets × vias, e a política de substituição"),
            ]),
            H("Why it missed (por que errou)"),
            Keys(&[
                ("first touch", "a página nunca tinha sido usada. Nenhuma TLB evita essas"),
                ("held before, lost", "a TLB tinha a página e a perdeu: expulsa, apagada por um flush ou empurrada por outro espaço de endereçamento. TLB maior, mais vias ou outra política ajudam aqui"),
                ("store to a clean page", "a TLB tinha a página, mas o store fez outro walk para marcar D"),
                ("no mapping", "o walk não achou nada válido: page fault"),
            ]),
            P("O gráfico é o hit rate ao longo do tempo. `s` salva um snapshot desses números; `↑` e `↓` escolhem um, `Enter` abre e `d` apaga."),
        ],
    },
    Section {
        title: "config: modos e formato",
        blocks: &[
            P("Toda mudança vale na hora; não existe botão de aplicar."),
            H("Modo"),
            Keys(&[
                ("off", "sem tradução: todo endereço já é físico"),
                ("auto", "o Raven monta uma tabela de páginas identidade, em que cada VA vira o mesmo PA, e traduz até no modo M, para qualquer programa poder ser observado. O código e os dados do programa ganham páginas de 4 KiB; o resto da memória, megapáginas"),
                ("program", "RISC-V de verdade: o próprio programa escreve o satp e passa para o modo S ou U. Nada é traduzido até ele fazer isso"),
            ]),
            H("TLB"),
            Keys(&[
                ("state", "`off` faz todo acesso passar por um walk"),
                ("entries", "quantas traduções ela guarda"),
                ("ways", "entradas por set: 1 é mapeamento direto, `all` é totalmente associativa"),
                ("policy", "qual via um set cheio entrega: LRU, FIFO (first in, first out, a primeira a entrar sai primeiro), Random, Clock, MRU (most recently used, a usada por último sai primeiro) ou LFU (least frequently used, a menos usada sai primeiro)"),
                ("cycles", "quanto custa um acerto, e os ciclos do próprio walker numa falha. As leituras de PTE são cobradas pelas caches por cima disso"),
            ]),
            P("Mudar entradas, vias ou política reconstrói a TLB: as entradas e os contadores recomeçam. Mudar os ciclos mantém o que ela guarda."),
            H("Mapa de páginas (modo auto)"),
            P("O que as páginas permitem (R, W, X, U), se são globais e o **ASID** (address-space identifier, identificador do espaço de endereçamento) a que pertencem. Mudar qualquer um reescreve a tabela e esvazia a TLB."),
            P("`m` troca o modo. Clique num número, ou aperte `e`, para digitar outro: `Enter` confirma, `Esc` descarta."),
        ],
    },
    Section {
        title: "Vários cores",
        blocks: &[
            P("Cada core tem a sua própria TLB, como no hardware. `[` e `]` escolhem o core cuja TLB, estatísticas e traduções esta aba mostra."),
            P("`flush tlb` e um novo formato de TLB valem para todos os cores. O `sfence.vma` de um programa esvazia só a TLB do core que o executou."),
        ],
    },
    Section {
        title: "Teclas e mouse",
        blocks: &[
            Keys(&[
                ("Tab  Shift+Tab", "próxima / anterior subaba"),
                ("s", "avança até a próxima tradução (em stats: salva um snapshot)"),
                ("b", "volta um passo"),
                ("p  Space", "roda / pausa"),
                ("r", "reinicia o programa"),
                ("f", "velocidade"),
                ("g", "volta a acompanhar o acesso mais recente"),
                ("[  ]", "core anterior / próximo"),
                ("←  →", "walk: tradução mais antiga / mais nova"),
                ("v", "walk: que tipo de acesso acompanhar"),
                ("1  2  3", "walk: mostra todas / falhas / page faults"),
                ("setas", "tlb e map: movem a seleção"),
                ("Enter", "map: abre uma sequência ou tabela · stats: abre um snapshot"),
                ("/", "map: procura um endereço"),
                ("m  e", "config: próximo modo / edita um número"),
                ("Ctrl+e  Ctrl+l", "exporta / importa a configuração de cache e TLB (.fcache)"),
                ("Ctrl+r", "exporta os resultados (.fstats ou .csv)"),
                ("?", "este guia"),
            ]),
            P("Tudo também é clicável, e a roda do mouse rola o que estiver embaixo do ponteiro."),
        ],
    },
    Section {
        title: "Glossário",
        blocks: &[Keys(&[
            ("A  D", "accessed e dirty: bits da PTE que o walker marca no primeiro uso e na primeira escrita da página"),
            ("ASID", "address-space identifier: marca as entradas da TLB para as páginas de vários programas dividirem a TLB"),
            ("CPI", "ciclos por instrução"),
            ("D-cache", "cache de dados: para onde vão loads, stores e leituras de PTE"),
            ("FIFO", "first in, first out: a primeira a entrar é a primeira a sair"),
            ("KiB  MiB", "1024 bytes; 1024 KiB"),
            ("L1  L2", "caches de nível 1 e 2; a L1 é a mais próxima e mais rápida"),
            ("LFU", "least frequently used: a menos usada sai primeiro"),
            ("LRU", "least recently used: a usada há mais tempo sai primeiro"),
            ("M  S  U", "máquina, supervisor e usuário: os modos de privilégio do RISC-V"),
            ("megapágina", "uma página de 4 MiB, mapeada por uma folha na tabela raiz"),
            ("MMU", "Memory Management Unit: o hardware que traduz endereços"),
            ("MRU", "most recently used: a usada por último sai primeiro"),
            ("PA", "physical address, endereço físico"),
            ("page fault", "a exceção gerada quando uma tradução falha"),
            ("PC", "program counter: o endereço da instrução em execução"),
            ("PPN", "physical page number, número da página física"),
            ("PTE", "page-table entry, entrada da tabela de páginas"),
            ("RAM", "a memória principal"),
            ("satp", "Supervisor Address Translation and Protection: o registrador com o endereço da tabela raiz, o ASID e o modo"),
            ("sfence.vma", "a instrução que apaga entradas da TLB"),
            ("Sv32", "o esquema de paginação em dois níveis do RISC-V para endereços de 32 bits"),
            ("TLB", "Translation Lookaside Buffer: uma cache das traduções recentes"),
            ("VA", "virtual address, endereço virtual"),
            ("VPN", "virtual page number, número da página virtual"),
            ("walk", "ler a tabela de páginas, nível a nível, para traduzir um VA"),
        ])],
    },
];
