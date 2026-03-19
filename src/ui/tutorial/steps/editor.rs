use ratatui::layout::Rect;
use crate::ui::app::App;
use super::super::TutorialStep;

// ── Layout helpers (mirror view/mod.rs + view/editor.rs) ───────────────────

/// Top-level: [tab_bar(3), content(min), footer(1)]
fn tab_bar(term: Rect) -> Rect {
    Rect { x: term.x, y: term.y, width: term.width, height: 3 }
}

fn content_area(term: Rect) -> Rect {
    Rect {
        x: term.x,
        y: term.y + 3,
        width: term.width,
        height: term.height.saturating_sub(4),
    }
}

fn footer_area(term: Rect) -> Rect {
    Rect {
        x: term.x,
        y: term.y + term.height.saturating_sub(1),
        width: term.width,
        height: 1,
    }
}

/// Editor content: [status(5), editor_body(min)]
fn editor_status(term: Rect) -> Rect {
    let c = content_area(term);
    Rect { x: c.x, y: c.y, width: c.width, height: 5.min(c.height) }
}

fn editor_body(term: Rect) -> Rect {
    let c = content_area(term);
    let status_h = 5.min(c.height);
    Rect {
        x: c.x,
        y: c.y + status_h,
        width: c.width,
        height: c.height.saturating_sub(status_h),
    }
}

/// The action button row is row 2 inside the status block (y+1 for block border, +1 for build line)
fn editor_actions(term: Rect) -> Rect {
    let s = editor_status(term);
    Rect { x: s.x + 1, y: s.y + 2, width: s.width.saturating_sub(2), height: 1 }
}

// ── Target functions ────────────────────────────────────────────────────────

fn target_tab_bar(term: Rect, _app: &App) -> Option<Rect> {
    Some(tab_bar(term))
}

fn target_status(term: Rect, _app: &App) -> Option<Rect> {
    Some(editor_status(term))
}

fn target_actions(term: Rect, _app: &App) -> Option<Rect> {
    Some(editor_actions(term))
}

fn target_editor_body(term: Rect, _app: &App) -> Option<Rect> {
    Some(editor_body(term))
}

fn target_footer(term: Rect, _app: &App) -> Option<Rect> {
    Some(footer_area(term))
}

// ── Step definitions ────────────────────────────────────────────────────────

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Keys & Shortcuts",
        title_pt: "Teclas e atalhos",
        body_en:  "Keyboard shortcuts in Raven are case-sensitive.\
\n\n`r` runs the simulation, `R` restarts it — they are different keys. `Ctrl+E` activates the encoding overlay; plain `e` toggles execution count.\
\n\nShortcuts shown as `[key]` are case-exact. When a modifier is shown (Ctrl+, Shift+), it is required.",
        body_pt:  "Os atalhos de teclado no Raven são sensíveis a maiúsculas/minúsculas.\
\n\n`r` inicia a execução, `R` reinicia — são teclas diferentes. `Ctrl+E` ativa a sobreposição de encoding; `e` sem modificador alterna a contagem de execuções.\
\n\nAtalhos exibidos como `[tecla]` são exatos quanto à capitalização. Quando um modificador é mostrado (Ctrl+, Shift+), ele é obrigatório.",
        target: target_footer,
        setup:  None,
    },
    TutorialStep {
        title_en: "Navigation Tabs",
        title_pt: "Abas de navegação",
        body_en:  "This bar at the top contains the main tabs: Editor, Run, Cache and Docs.\
\n\nClick a tab or use the mouse to navigate between them. The [?] button in the top-right corner opens this tour.",
        body_pt:  "Esta barra no topo contém as abas principais: Editor, Run, Cache e Docs.\
\n\nClique em uma aba ou use o mouse para navegar entre elas. O botão [?] no canto direito abre este tour.",
        target: target_tab_bar,
        setup:  None,
    },
    TutorialStep {
        title_en: "Status Bar",
        title_pt: "Barra de status",
        body_en:  "Shows the current build status, cursor line and column, and whether there are syntax errors in the code.\
\n\nThe code is compiled automatically after a pause in typing.",
        body_pt:  "Mostra o estado atual da compilação (Build status), o número de linha e coluna do cursor, e se há erros de sintaxe no código.\
\n\nO código é compilado automaticamente após uma pausa na digitação.",
        target: target_status,
        setup:  None,
    },
    TutorialStep {
        title_en: "Import / Export Buttons",
        title_pt: "Botões Import / Export",
        body_en:  "Import [BIN] loads an ELF binary. Import [CODE] opens a .fas (assembly) file.\
\n\nExport [BIN] saves the compiled binary. Export [CODE] saves the source code.\
\n\nThe [▶ RUN] and [FORMAT] buttons execute and format the code respectively.",
        body_pt:  "Import [BIN] carrega um binário ELF. Import [CODE] abre um arquivo .fas (assembly).\
\n\nExport [BIN] salva o binário compilado. Export [CODE] salva o código-fonte.\
\n\nOs botões [▶ RUN] e [FORMAT] executam e formatam o código respectivamente.",
        target: target_actions,
        setup:  None,
    },
    TutorialStep {
        title_en: "Code Editor",
        title_pt: "Editor de código",
        body_en:  "Main editing area for RISC-V assembly code.\
\n\nThe left column displays line numbers. Highlighted numbers indicate execution counts (heatmap).\
\n\nUse Ctrl+F to search, Ctrl+G to go to a line, Ctrl+Z/Y to undo/redo.",
        body_pt:  "Área principal de edição do código assembly RISC-V.\
\n\nA coluna da esquerda exibe números de linha. Números em destaque indicam contagem de execuções (heatmap).\
\n\nUse Ctrl+F para buscar, Ctrl+G para ir a uma linha, Ctrl+Z/Y para desfazer/refazer.",
        target: target_editor_body,
        setup:  None,
    },
    TutorialStep {
        title_en: "Encoding Overlay",
        title_pt: "Sobreposição de encoding",
        body_en:  "Press Ctrl+E to enable the binary encoding view for instructions.\
\n\nEach instruction appears with its binary opcode overlaid, useful for studying RISC-V instruction formats.",
        body_pt:  "Pressione Ctrl+E para ativar a visualização de encoding binário das instruções.\
\n\nCada instrução aparece com seu opcode em binário sobreposto, útil para estudar o formato das instruções RISC-V.",
        target: target_editor_body,
        setup:  None,
    },
    TutorialStep {
        title_en: "Footer Bar",
        title_pt: "Barra de rodapé",
        body_en:  "Displays the current mode (INSERT or COMMAND) and available keyboard shortcuts.\
\n\nPress Esc to enter COMMAND mode. Click in the editor to return to INSERT mode.",
        body_pt:  "Exibe o modo atual (INSERT ou COMMAND) e os atalhos de teclado disponíveis.\
\n\nPressione Esc para entrar no modo COMMAND. Clique no editor para voltar ao modo INSERT.",
        target: target_footer,
        setup:  None,
    },
];
