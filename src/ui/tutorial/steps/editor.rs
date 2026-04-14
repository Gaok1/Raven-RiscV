use super::super::TutorialStep;
use crate::ui::app::App;
use ratatui::layout::Rect;

// ── Layout helpers (mirror view/mod.rs + view/editor.rs) ───────────────────

/// Top-level: [tab_bar(3), content(min), footer(1)]
fn tab_bar(term: Rect) -> Rect {
    Rect {
        x: term.x,
        y: term.y,
        width: term.width,
        height: 3,
    }
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
    Rect {
        x: c.x,
        y: c.y,
        width: c.width,
        height: 5.min(c.height),
    }
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
    Rect {
        x: s.x + 1,
        y: s.y + 2,
        width: s.width.saturating_sub(2),
        height: 1,
    }
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
        body_en: "Some Raven shortcuts are case-sensitive and some are not.\
\n\nWhen upper and lower case mean different commands, the tutorial shows the exact key. Example: `r` runs in the Run tab while `R` restarts.\
\n\nWhen a shortcut accepts the lowercase form, this tutorial shows it in lowercase. Example: `Ctrl+e` toggles the encoding overlay; plain `e` toggles execution count in runtime views.",
        body_pt: "Alguns atalhos do Raven diferenciam maiúsculas e minúsculas, e outros não.\
\n\nQuando maiúscula e minúscula têm comandos diferentes, o tutorial mostra a tecla exata. Exemplo: `r` executa na aba Run enquanto `R` reinicia.\
\n\nQuando o atalho aceita a forma minúscula, este tutorial exibe em minúsculo. Exemplo: `Ctrl+e` alterna a sobreposição de encoding; `e` sem modificador alterna a contagem de execuções nas views de runtime.",
        target: target_footer,
        setup: None,
    },
    TutorialStep {
        title_en: "Navigation Tabs",
        title_pt: "Abas de navegação",
        body_en: "This bar at the top contains the main tabs: Editor, Run, Cache, Pipeline and Docs.\
\n\nClick a tab or use the mouse to navigate between them. The [?] button in the top-right corner opens this tour.",
        body_pt: "Esta barra no topo contém as abas principais: Editor, Run, Cache, Pipeline e Docs.\
\n\nClique em uma aba ou use o mouse para navegar entre elas. O botão [?] no canto direito abre este tour.",
        target: target_tab_bar,
        setup: None,
    },
    TutorialStep {
        title_en: "Status Bar",
        title_pt: "Barra de status",
        body_en: "Shows the current build status, cursor line and column, and whether there are syntax errors in the code.\
\n\nThe code is compiled automatically after a pause in typing.",
        body_pt: "Mostra o estado atual da compilação (Build status), o número de linha e coluna do cursor, e se há erros de sintaxe no código.\
\n\nO código é compilado automaticamente após uma pausa na digitação.",
        target: target_status,
        setup: None,
    },
    TutorialStep {
        title_en: "Import / Export Buttons",
        title_pt: "Botões Import / Export",
        body_en: "Import [BIN] loads an ELF binary. Import [CODE] opens a .fas (assembly) file.\
\n\nExport [BIN] saves the compiled binary. Export [CODE] saves the source code.\
\n\nThe [▶ RUN] and [FORMAT] buttons execute and format the code respectively.\
\n\nCtrl+o opens a file picker for import; Ctrl+s opens one for export.",
        body_pt: "Import [BIN] carrega um binário ELF. Import [CODE] abre um arquivo .fas (assembly).\
\n\nExport [BIN] salva o binário compilado. Export [CODE] salva o código-fonte.\
\n\nOs botões [▶ RUN] e [FORMAT] executam e formatam o código respectivamente.\
\n\nCtrl+o abre o seletor de arquivo para importação; Ctrl+s abre para exportação.",
        target: target_actions,
        setup: None,
    },
    TutorialStep {
        title_en: "ELF Binary Mode",
        title_pt: "Modo binário ELF",
        body_en: "When an ELF binary is loaded the editor becomes read-only and shows a prompt with three choices:\
\n\n[ Cancel ] — keep the ELF loaded and stay in read-only view.\
\n[ Edit opcodes ] — disassemble the ELF and open it for opcode-level editing.\
\n[ Discard ELF ] — remove the binary so you can edit assembly source freely again.",
        body_pt: "Quando um binário ELF é carregado o editor vira somente-leitura e exibe um prompt com três opções:\
\n\n[ Cancel ] — mantém o ELF carregado na view somente-leitura.\
\n[ Edit opcodes ] — desmonta o ELF e abre para edição no nível de opcodes.\
\n[ Discard ELF ] — remove o binário para você editar o código assembly livremente de novo.",
        target: target_actions,
        setup: None,
    },
    TutorialStep {
        title_en: "Code Editor",
        title_pt: "Editor de código",
        body_en: "Main editing area for RISC-V assembly code.\
\n\nThe left column displays line numbers. Highlighted numbers indicate execution counts (heatmap).\
\n\nUse Ctrl+f to search, Ctrl+g to go to a line, Ctrl+z/y to undo/redo.",
        body_pt: "Área principal de edição do código assembly RISC-V.\
\n\nA coluna da esquerda exibe números de linha. Números em destaque indicam contagem de execuções (heatmap).\
\n\nUse Ctrl+f para buscar, Ctrl+g para ir a uma linha, Ctrl+z/y para desfazer/refazer.",
        target: target_editor_body,
        setup: None,
    },
    TutorialStep {
        title_en: "Editing Shortcuts",
        title_pt: "Atalhos de edição",
        body_en: "Additional shortcuts inside the editor:\
\n\nCtrl+h — find & replace bar (type search term, Tab to replacement field, Enter to replace).\
\nCtrl+/ — toggle line comment for the current line.\
\n#! text — visible annotation attached to an instruction in the Run tab.\
\n##! text — block comment shown above the next emitted instruction in the Run tab.\
\nCtrl+a — select all  •  Ctrl+c — copy  •  Ctrl+x — cut  •  Ctrl+v — paste.",
        body_pt: "Atalhos adicionais dentro do editor:\
\n\nCtrl+h — barra de busca e substituição (digite o termo, Tab para o campo de substituição, Enter para substituir).\
\nCtrl+/ — alternar comentário na linha atual.\
\n#! texto — anotação visível anexada à instrução na aba Run.\
\n##! texto — comentário de bloco mostrado acima da próxima instrução emitida na aba Run.\
\nCtrl+a — selecionar tudo  •  Ctrl+c — copiar  •  Ctrl+x — recortar  •  Ctrl+v — colar.",
        target: target_editor_body,
        setup: None,
    },
    TutorialStep {
        title_en: "Encoding Overlay",
        title_pt: "Sobreposição de encoding",
        body_en: "Press Ctrl+e to enable the binary encoding view for instructions.\
\n\nEach instruction appears with its binary opcode overlaid, useful for studying RISC-V instruction formats.",
        body_pt: "Pressione Ctrl+e para ativar a visualização de encoding binário das instruções.\
\n\nCada instrução aparece com seu opcode em binário sobreposto, útil para estudar o formato das instruções RISC-V.",
        target: target_editor_body,
        setup: None,
    },
    TutorialStep {
        title_en: "Footer Bar",
        title_pt: "Barra de rodapé",
        body_en: "Displays the current mode (INSERT or COMMAND) and available keyboard shortcuts.\
\n\nPress Esc to enter COMMAND mode. Click in the editor to return to INSERT mode.",
        body_pt: "Exibe o modo atual (INSERT ou COMMAND) e os atalhos de teclado disponíveis.\
\n\nPressione Esc para entrar no modo COMMAND. Clique no editor para voltar ao modo INSERT.",
        target: target_footer,
        setup: None,
    },
];
