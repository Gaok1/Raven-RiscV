# Plano — Toolkit de UI padronizado e modular (ratatui)

## Context

A UI do Raven cresceu orgânica: cada view (settings, cache, tlb, pipeline, run, docs)
reimplementa à mão os mesmos padrões de ratatui. O levantamento (5 agentes, varredura
completa de `src/ui`) mediu a repetição:

- **~69** `Block::default()` montando o mesmo painel arredondado+título (48 sites quase idênticos).
- **~13** cópias open-coded do triângulo de estilo `selecionado→ACCENT.bold / hover→TEXT.bold / normal→LABEL`, mais **~17** variantes de valor.
- **842** `Span::styled` / **964** `Style::default().fg(...)`, dos quais ~6–9 estilos semânticos cobrem a maioria; o estilo `label` aparece ~200×, `title` ~45×.
- **6** funções `*_btn_style`/`dense_style`/`btn_style` que são a MESMA "toggle chip" duplicada por arquivo, + duplicatas mortas em `run/status.rs`.
- **3** cópias de `centered_rect` + **~10** recomputações do mesmo layout raiz (tab/body/status) entre `view` e `input::mouse` (risco de hitbox driftar).
- Tabelas: só **3** usam o widget `Table` real; o resto é alinhamento manual com `format!("{:<N}")` (docs, cache/stats, tlb/stats) com constantes de largura duplicadas.

**Objetivo:** criar um toolkit modular de componentes ratatui sob `src/ui/view/components/`
(+ `src/ui/view/style.rs`), padronizar **comportamento e estilo** (botões, toggles, seletores,
campos editáveis, painéis, tabelas, listas, layouts, overlays), e **migrar todos os call sites**.
Padronizar visual quando há divergência (uma convenção por arquétipo). Geometria de layout
vira `pub(crate)` e passa a ser compartilhada por view **e** mouse, eliminando a duplicação.

Resultado esperado: trocar estilo/comportamento de um controle = editar 1 função; encolher
drasticamente settings/config; eliminar drift view↔mouse; um "jeito Raven" documentado de
escrever UI.

### Decisões do usuário
- **Escopo:** construir o toolkit **e migrar tudo**.
- **Visual:** **unificar** as inconsistências (uma convenção por arquétipo).
- **Mouse/layout:** **compartilhar** a geometria (`pub(crate)`), matando as duplicações.

### Convenções unificadas (decididas agora, aplicadas na migração)
- **Cursor de edição:** `█` (bloco) em todos os campos (settings passa de `_` para `█`).
- **Booleano:** `true`/`false` em verde(`RUNNING`)/vermelho(`DANGER`) como padrão; `on/off` e `[on]/[off]` convergem pra isso (mantém-se só o caso de letras de permissão `R W X U`, que é semanticamente diferente).
- **Título de painel:** `ACCENT.bold` para painéis ativos/primários e popups; `LABEL` para painéis de conteúdo secundário — codificado em `PanelKind`, decisão explícita por chamada.
- **Popups:** todos `BorderType::Rounded` (help_popup e splash deixam de ser quadrados).
- **bg de seleção/hover** em listas: centralizadas em `theme` (hoje há `Rgb(50,50,80)`, `Rgb(40,60,40)`, `BG_HOVER` espalhados).

### Princípios de arquitetura
- `theme.rs` continua **só paleta** (constantes `Color`). Builders de `Style`/`Span` ficam em `view/style.rs`; widgets em `components/`.
- Helpers que precisam ser compartilhados com `input::mouse` (geometria pura, sem `Frame`) são `pub(crate)`. Imitar o padrão bom já existente `run_panel_constraints` (`src/ui/view/run/mod.rs:29`).
- Cada fase **compila e é commitável** sozinha (uma fase = um commit, como no refactor anterior).
- Fora de escopo: cores inline intencionais de `docs/content/fcache_ref.rs` e `syscalls.rs` (conteúdo didático), e os módulos de syntax highlight do editor (`editor/highlight.rs`, `editor/encoding.rs`).

### Branch
Trabalho grande — criar branch novo `refactor/ui-component-toolkit` a partir de `main`
(o branch atual `refactor/modularize-large-files` já está fechado e pronto pra PR).

---

## Fase 0 — Fundação (scaffolding, sem mudança de comportamento)

Cria os módulos-base que todo o resto consome. Nada migrado ainda; só compila.

**Novos arquivos:**
- `src/ui/view/style.rs` — estilos semânticos + badges + métricas + toggle + hints:
  - `label() value() idle() danger() success() warning() title() key() -> Style`
  - `title_span(s) -> Span`
  - `enum Metric { Cycles, Cpi, Ipc }`; `metric(k) -> Style`; `metric_span(label, val, k) -> Span`
  - `enum Badge { Accent, Danger, Idle, Success }`; `badge(text, kind) -> Span`
  - `toggle(active, hovered, active_color) -> Style` (a chip de 3 estados unificada)
  - `key_hint(key, desc) -> Vec<Span>`; `hint_bar(&[(&str,&str)]) -> Line`
- `src/ui/view/components/layout.rs` — geometria pura, **`pub(crate)`** (sem `Frame`):
  - `centered_rect(w, h, area)`, `centered_pct(pw, ph, area)`, `centered_width(area, pref, min, margin)`
  - `centered_column(area, content_w)` (via `Fill/Length/Fill`)
  - `body_footer(area, footer_h) -> (Rect, Rect)`, `header_body(area, &[u16]) -> (Vec<Rect>, Rect)`
  - `app_frame(area) -> (tabs, body, status)` (o frame raiz `[Length(3), Min(5), Length(1)]`)
  - `even_columns(area, n) -> Vec<Rect>`
  - `anchored_popup(anchor, pw, ph, term)` (unifica `best_popup_rect` + `tutorial_popup_rect`)

**Editar:** `src/ui/view/components/mod.rs` (declarar/re-exportar `layout`); `src/ui/view/mod.rs` (declarar `style`).

**Doc deliverable:** doc-comment de módulo em `components/mod.rs` descrevendo o "jeito Raven"
de escrever UI (qual helper usar pra cada caso). Doc de desenvolvedor via rustdoc — **não**
em `docs/` (esse é didático/estudante).

---

## Fase 1 — Painéis & overlays (chrome)

Maior ganho de legibilidade por linha. Substitui ~48 painéis e 6 popups.

**Novos arquivos:**
- `src/ui/view/components/panel.rs`:
  - `enum PanelKind { Plain, Accent, Warning, Danger, Custom(Color) }`
  - `panel(title, kind) -> Block` (rounded + ALL + border + título estilado)
  - `panel_frame(kind) -> Block` (sem título), `panel_square(title_line, border) -> Block` (caixas do pipeline/gantt)
  - `handle_bar(border) -> Block` (só `Borders::TOP` — barras de preset/apply/console colapsado)
  - `render_panel(f, area, block) -> Rect` (faz `inner()` + `render_widget`, dobra a dança de 3 linhas que aparece ~40×)
- `src/ui/view/components/overlay.rs`:
  - `struct OverlayStyle { border, title, bottom }`; `overlay(f, rect, style) -> Rect` (Clear + block + retorna inner)

**Migrar (representativos — padrão repetido em ~30 arquivos):**
`cache/{config,mod,stats,view/matrix}.rs`, `tlb/{config,entries,mod,page_tree,stats,status,vm_settings}.rs`,
`run/{status,mod,instruction_list,instruction_details/*}.rs`, `pipeline/{mod,config_view,main_view/*}.rs`,
`settings.rs`. Popups: `mod.rs` (exit/help/ELF), `path_input_overlay.rs`, `cache/stats.rs` (snapshot), `tutorial/render.rs`.

**Cuidado (hitbox):** `input/mouse/cache.rs:137,153` constroem `Block` só pra `.inner()`. Devem usar
o mesmo `panel*()`/`render_panel` que o render, senão a geometria de hit-test diverge. Verificar clique a clique.

---

## Fase 2 — Estilos semânticos, badges & métricas

Migra os ~600 sites open-coded que mapeiam para os estilos semânticos da Fase 0.

- Substituir `Style::default().fg(theme::LABEL|TEXT|IDLE|DANGER|RUNNING|PAUSED)` por `style::label()|value()|idle()|danger()|success()|warning()`.
- Títulos de painel via `style::title()`/`title_span()` (acopla com Fase 1).
- Métricas (`Cycles:/CPI:/IPC:`, ~30 sites) → `style::metric_span(...)`.
- Badges/pills (`fg(Black).bg(...)`, 8 sites em `mod.rs`, `path_input_overlay.rs`, `build.rs`) → `style::badge(...)`.
- Hint bars / legendas (footer global em `mod.rs:128-198`, help popup, search overlays) → `style::hint_bar()`/`key_hint()`.
- **Deletar duplicatas mortas:** `run/status.rs` `push_dense_pair`/`value_btn`/`action_btn` (cópias literais de `controls.rs`).
- **Colapsar as 6 toggle-chips:** `cache/mod.rs` `dense_style`+wrappers, `pipeline/mod.rs` `subtab_style`, `tlb/mod.rs` `btn_style` → todos para `style::toggle`.

Pode ser feito por cluster de arquivos (run / cache / tlb / pipeline / docs-chrome) — vários commits se ficar grande.

---

## Fase 3 — Controles interativos (settings + configs)

Onde mora o "código mais porco". Aqui a unificação visual acontece.

**Editar/estender `src/ui/view/components/controls.rs`** (mantém `dense_value`/`dense_action` como primitivas):
- `enum ControlState { Normal, Hovered, Selected, Disabled }` + `from(sel, hov)` + `disabled_if(off)`
- `label_style(state, base_color) -> Style` (o triângulo, com base override pra CPI=`CPI_PANEL`, pipeline=`IDLE`)
- `toggle_row(label, value, state) -> ListItem` (bool unificado true/false verde/vermelho)
- `selector_row(label, value, state, editing) -> ListItem` (enum ciclável; `< v >` quando editando)
- `edit_row(label, display, edit_buf: Option<&str>, state) -> ListItem` (cursor `█` unificado)
- `action(label, color, hovered) -> Span` (absorve os `preset_btn_style` locais duplicados)
- Suportar sufixos compostos: `[?]`, "(no effect — VM off)", marca de validade ` ✗`, hint JIT.

**Migrar:** `settings.rs` (10 linhas → builders), `cache/config.rs` (closure `field_item` vira adaptador fino),
`tlb/config.rs`, `tlb/vm_settings.rs` (closures `num_val`/`hov`/`editing`), `pipeline/config_view.rs` (`bool_span`).

**Cuidado:** estado `Disabled` em `vm_settings.rs` também **pula registro de hitbox** (`:177,187,267`).
Render-só nos builders; geometria/hitbox continua no caller (sistema de Cell/`record_*_hitboxes` atual).

---

## Fase 4 — Tabelas & listas

**Novos arquivos:**
- `src/ui/view/components/tables.rs`:
  - `struct Col { header, width: Constraint, align }`, `enum Align`
  - `struct RowStyle { selected, hover, zebra, base }`
  - `DataTable` builder (sobre o `Table` real: header + zebra + seleção + alinhamento grátis)
  - `kv_table(&[(&str,&str)], key_w) -> Vec<Line>`, `kv_styled(&[(Span,Span)]) -> Vec<Line>`
- `src/ui/view/components/lists.rs`:
  - `struct ListRow { line, selected, hover }`, `selectable_list(rows) -> List` (bg de seleção/hover centralizada)
  - `visible_window(total, view_h, cursor, scroll) -> (start, end)` (a matemática de scroll repetida em regs/mem/imem/history/entries/page_tree)

**Migrar (manuais `format!` → toolkit):** docs `instr_ref/render.rs` (apaga `pad_or_truncate`/`render_col_header`/separador manual),
`docs/content/common.rs` (`kv/thead/tsep/trow_wrapped` reexpressos sobre o toolkit) + `syscalls.rs`/`memory_map.rs`,
`cache/stats.rs` (history + metrics), `tlb/stats.rs`; listas: `run/instruction_list.rs`, `run/sidebar.rs` (mem/ELF), `path_input_overlay.rs`.
Centralizar constantes de largura (`TY_W/MNE_W/...`, `COL_*_W`, `set_col_w=5`) e cores de seleção em `theme`.

**Fora (caso especial, fica bespoke):** matriz sets×ways (`cache/view/matrix.rs` — cores por célula, wrap, h-scroll)
e árvore de páginas (`tlb/page_tree.rs` — indentação/colapso). No máximo reaproveitam `visible_window` e perm-spans.

---

## Fase 5 — Layout & dedup view↔mouse

Consome `components/layout.rs` (Fase 0) dos dois lados, matando a duplicação.

- **View renderers** passam a usar `app_frame`, `body_footer`, `header_body`, `even_columns`, `centered_column`:
  `mod.rs:50`, `run/mod.rs:49`, `settings.rs:38`, `cache/{config,mod,view,stats}.rs`, `tlb/{config,mod,stats}.rs`, `pipeline/mod.rs`, etc.
- **`input::mouse`** passa a chamar os MESMOS helpers em vez de recomputar:
  frame raiz em `mouse/run.rs` (×5), `mouse/cache.rs`, `mouse/run_status.rs`, `mouse/docs.rs`, `mouse.rs`;
  e os 3 `centered_rect` (`view/mod.rs:702`, `input/mouse/popups.rs:5`, `tutorial/render.rs:339`) → um só.
- Unificar `best_popup_rect`/`tutorial_popup_rect` → `anchored_popup`.
- Garantir um único valor pro `Min(5)` do body (hoje há divergência `Min(5)` vs sub-split `Min(3)`).

**Cuidado:** esta é a fase de maior risco de regressão de clique. Testar cada tab/popup com mouse.

---

## Fase 6 — Limpeza, doc e verificação final

- Remover helpers locais obsoletos que sobraram (closures de estilo por-arquivo, consts duplicadas).
- Finalizar o doc-comment do "jeito Raven" em `components/mod.rs` com exemplos before/after.
- `cargo clippy --all-targets` limpo; rodar com e sem `--features jit`.

---

## Verificação (cada fase)

1. **Compila/lint:** `cargo build` e `cargo clippy --all-targets -- -D warnings`.
2. **Testes:** `cargo test`.
3. **Visual/manual (skill `run`):** abrir a TUI e percorrer cada tab — Settings, Cache (config/stats/matrix), TLB (config/entries/page-tree/stats/vm), Pipeline (config/main), Run (sidebar/imem/details/console), Docs (instr_ref/syscalls/memory_map). Conferir bordas, títulos, toggles (true/false), seletores, campos editáveis (cursor `█`), métricas, badges.
4. **Mouse (Fases 1,3,5 — crítico):** clicar em toggles/seletores/botões/preset/apply, abrir/fechar popups (exit, help, ELF, path input, snapshot, tutorial), e verificar que hitboxes batem com o render (sem drift).
5. **Diff sanity:** confirmar que o LOC caiu em settings.rs/configs e que nenhum `Block::default()`/triângulo de estilo sobrou nos arquivos migrados (`grep`).

## Ordem de dependência (resumo)
Fase 0 (fundação) → 1 (painel/overlay, usa style) → 2 (estilos) → 3 (controles, usa style) → 4 (tabelas/listas) → 5 (layout + mouse) → 6 (limpeza/doc). Cada fase = 1+ commit, compilando isolada.
