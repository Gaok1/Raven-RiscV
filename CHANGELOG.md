# Changelog

## v1.7.0

### Aba Cache — Métricas Acadêmicas

- **AMAT** (Average Memory Access Time) — calculado hierarquicamente: `hit_lat + miss_rate × AMAT_próximo_nível`; exibido por cache no painel de métricas
- **IPC** (Instructions Per Cycle) — exibido ao lado do CPI no sumário de programa
- **CPI breakdown** — contribuição individual de cada nível de cache já estava calculada; agora exibida com mais destaque

### Aba Cache — Exportação de Resultados

- **`Ctrl+R` — exportar resultados** — salva snapshot completo em dois formatos:
  - `.fstats` — flat key=value reimportável (configuração + estatísticas + histórico + top miss PCs)
  - `.csv` — planilha com seções Program Summary, I-Cache, D-Cache, L2+, Miss Hotspots; ideal para comparações acadêmicas
- **`Ctrl+M` — carregar baseline** — importa um `.fstats` anterior para comparação lado a lado
- **Delta comparison** — quando um baseline está carregado, cada bloco de cache exibe `Vs base: ΔHit ΔMPKi ΔAMAT ΔCycles` em azul
- **Banner de comparação** — faixa `Comparing with: arquivo.fstats  [c] clear` no topo do painel de stats
- **`c` (Stats)** — limpa o baseline carregado
- **Botões `[⬆ Export]` e `[⬇ Compare]`** na barra de controles compartilhada (clicáveis com mouse)

### Aba Cache — Run Controls integrados

- **Run Controls sempre visível** — a barra de controles da aba Run (View / Region / Format / Sign / Bytes / Speed / State / Count / Type) aparece em todas as sub-abas da Cache (Stats, View e Config), sem precisar trocar de aba
- **Hotkeys espelhados** — os mesmos atalhos da aba Run agora funcionam na Cache (fora do subtab Config):
  - `v` — alterna sidebar: RAM → REGS → BP
  - `k` — alterna região: DATA ↔ STACK
  - `f` — avança velocidade: 1× → 2× → 4× → Instant
  - `e` — ativa/desativa contador de execuções
  - `y` — ativa/desativa badge de tipo de instrução
- **Mouse** — hover e click nos botões da Run Controls funcionam na aba Cache da mesma forma que na aba Run

### Aba Run — Sidebar de Memória

- **Indicador de nível de cache** — endereços marcados como `●` (dirty, roxo) agora exibem o nível onde estão cacheados: `● L1 0xADDR: VALUE` ou `● L2 0xADDR: VALUE`, em vez de apenas `● 0xADDR`

---

## Unreleased → v1.6.0

### Aba Run — Instrução Memory

- **Badge de tipo** — cada instrução exibe uma tag colorida `[R]` `[I]` `[S]` `[B]` `[U]` `[J]` indicando o formato de encoding
- **Toggle raw hex** (`x`) — alterna entre exibir o word bruto `0x00A50513` ou o valor formatado ao lado do disasm
- **Heat coloring** — o sufixo `×N` de contagem de execuções muda de cor conforme a frequência (ciano → verde → amarelo → vermelho)
- **Labels como headers** — rótulos de label são exibidos como cabeçalho acima da primeira instrução que pertencem
- **Branch outcome** — na instrução corrente (PC), mostra `→ 0xADDR (taken)` ou `↛ (not taken)` com o endereço de destino
- **Comentários visíveis `#!`** — adicione `#! texto` ao final de qualquer instrução; aparece inline na instrução memory e no painel decoded
- **Block comments `##!`** — uma linha `##!` no fonte gera um separador visual verde acima da próxima instrução
- **Jump to address** (`g`) — abre barra de entrada na base do painel para navegar direto a qualquer endereço (hex com ou sem prefixo `0x`)

### Aba Run — Decoded Details

- **Endereço efetivo** — para `lw`/`sw`/`lb`/`sb` etc., calcula e exibe `rs1 + imm = 0xADDR` com o valor atual de `rs1`
- **Detecção de hazard RAW** — aviso `⚠ RAW` quando a instrução atual lê um registrador que foi escrito pela instrução anterior
- **Jump target preview** — para branches e jumps, exibe `→ 0xADDR <label> (taken/not taken)` com nome de label quando disponível
- **Contagem de execuções** — mostra `×N` no header indicando quantas vezes a instrução foi executada

### Aba Run — Sidebar / Registradores

- **Formato duplo** — o painel de registradores exibe hex `0x00000000` e decimal com sinal simultaneamente em colunas separadas
- **Fading por idade** — registradores piscam amarelo ao serem escritos e voltam ao branco progressivamente ao longo de 4 passos
- **Navegação por teclado** (`↑`/`↓`) e **pinning** (`P`) — navegue entre registradores e fixe os mais importantes no topo com o marcador `◉`
- **Write trace** — o título do painel mostra `[last write @ 0x...]` para o registrador selecionado
- **Stack view** (`k` / `v`) — exibe palavras ao redor do SP com offset relativo (`SP+0 ◀`, `SP-4`, `SP+8`…) auto-seguindo o ponteiro
- **Breakpoint list** (`v` — 4º modo) — lista todos os breakpoints ativos com endereço, label e disasm; PC atual destacado em amarelo

### Aba Run — Trace e Controles

- **Execution trace** (`t`) — divide o painel de instrução memory 60/40 verticalmente mostrando o histórico das últimas instruções executadas (até 200)
- **Cycle de view** (`v`) — alterna entre 4 modos: RAM → REGS → STACK → BP
- **Ciclo de velocidade** (`f`) — 1× → 2× → 4× → Instant (execução em bulk de 8 ms/frame)

### Editor / IDE

- **Highlight de label** — todas as ocorrências do label sob o cursor são sublinhadas automaticamente
- **Go-to-definition** (`F12`) — pula para a linha do fonte onde o label é definido
- **Address hints** (`F2`) — mostra gutter `0xADDR │` ao lado de cada linha com o endereço compilado
- **Indicador de linha/coluna** — barra de status exibe `Ln X, Col Y` em tempo real
- **Comentários `#!` em verde** — estilizados em verde brilhante no editor para distinguir dos comentários normais (cinza escuro)
- **Ctrl+D** — seleciona a próxima ocorrência da palavra sob o cursor

### CI/CD

- **Release condicional** — o pipeline só empacota e publica uma nova release quando a mensagem do commit contém o padrão `vN.N.N`; commits sem versão apenas compilam e testam

---

## v1.5.7

Versão anterior — emulador RISC-V com assembler, cache hierárquico (L1–Ln), TUI ratatui com abas Editor / Run / Cache / Docs.
