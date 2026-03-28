use super::super::TutorialStep;
use crate::ui::app::{App, MemRegion};
use ratatui::layout::Rect;

// ── Layout helpers (mirror view/run/mod.rs) ─────────────────────────────────

fn content_area(term: Rect) -> Rect {
    // term = full terminal; tab bar = 3, footer = 1
    Rect {
        x: term.x,
        y: term.y + 3,
        width: term.width,
        height: term.height.saturating_sub(4),
    }
}

struct RunLayout {
    controls: Rect,
    main: Rect,
    console: Rect,
}

fn run_layout(term: Rect, console_height: u16) -> RunLayout {
    let c = content_area(term);
    let controls_h = 5.min(c.height);
    let remaining = c.height.saturating_sub(controls_h);
    let console_h = console_height.min(remaining);
    let main_h = remaining.saturating_sub(console_h);
    RunLayout {
        controls: Rect {
            x: c.x,
            y: c.y,
            width: c.width,
            height: controls_h,
        },
        main: Rect {
            x: c.x,
            y: c.y + controls_h,
            width: c.width,
            height: main_h,
        },
        console: Rect {
            x: c.x,
            y: c.y + controls_h + main_h,
            width: c.width,
            height: console_h,
        },
    }
}

fn sidebar_rect(term: Rect, app: &App) -> Rect {
    let rl = run_layout(term, app.run.console_height);
    let sidebar_w = if app.run.sidebar_collapsed {
        3
    } else {
        app.run.sidebar_width
    };
    Rect {
        x: rl.main.x,
        y: rl.main.y,
        width: sidebar_w,
        height: rl.main.height,
    }
}

fn imem_rect(term: Rect, app: &App) -> Rect {
    let rl = run_layout(term, app.run.console_height);
    let sidebar_w = if app.run.sidebar_collapsed {
        3
    } else {
        app.run.sidebar_width
    };
    let imem_w = if app.run.imem_collapsed {
        3
    } else {
        app.run.imem_width
    };
    Rect {
        x: rl.main.x + sidebar_w,
        y: rl.main.y,
        width: imem_w,
        height: rl.main.height,
    }
}

fn trace_rect(term: Rect, app: &App) -> Rect {
    let im = imem_rect(term, app);
    // Trace = bottom 40% of imem column when show_trace is on
    let trace_h = (im.height * 2 / 5).max(4);
    let imem_h = im.height.saturating_sub(trace_h);
    Rect {
        x: im.x,
        y: im.y + imem_h,
        width: im.width,
        height: trace_h,
    }
}

fn details_rect(term: Rect, app: &App) -> Rect {
    let rl = run_layout(term, app.run.console_height);
    let sidebar_w = if app.run.sidebar_collapsed {
        3
    } else {
        app.run.sidebar_width
    };
    let imem_w = if app.run.imem_collapsed {
        3
    } else {
        app.run.imem_width
    };
    let offset = sidebar_w + imem_w;
    Rect {
        x: rl.main.x + offset,
        y: rl.main.y,
        width: rl.main.width.saturating_sub(offset),
        height: rl.main.height,
    }
}

fn console_rect(term: Rect, app: &App) -> Rect {
    run_layout(term, app.run.console_height).console
}

// ── Target functions ────────────────────────────────────────────────────────

fn target_controls(term: Rect, app: &App) -> Option<Rect> {
    Some(run_layout(term, app.run.console_height).controls)
}

fn target_sidebar(term: Rect, app: &App) -> Option<Rect> {
    Some(sidebar_rect(term, app))
}

fn target_imem(term: Rect, app: &App) -> Option<Rect> {
    Some(imem_rect(term, app))
}

fn target_trace(term: Rect, app: &App) -> Option<Rect> {
    Some(trace_rect(term, app))
}

fn target_details(term: Rect, app: &App) -> Option<Rect> {
    Some(details_rect(term, app))
}

fn target_console(term: Rect, app: &App) -> Option<Rect> {
    Some(console_rect(term, app))
}

// ── Setup functions ─────────────────────────────────────────────────────────

fn setup_show_registers(app: &mut App) {
    app.run.show_registers = true;
    app.run.show_float_regs = false;
}

fn setup_show_ram(app: &mut App) {
    app.run.show_registers = false;
    app.run.mem_region = MemRegion::Data;
}

fn setup_show_trace(app: &mut App) {
    app.run.show_trace = true;
}

// ── Step definitions ────────────────────────────────────────────────────────

pub static STEPS: &[TutorialStep] = &[
    TutorialStep {
        title_en: "Execution Controls",
        title_pt: "Controles de execução",
        body_en: "Control bar at the top: buttons [s] Step, [r] Run/Stop, [p] Pause, [R] Restart.\
\n\nThe Speed button controls the rate: 1x → 2x → 4x → 8x → GO (maximum speed).\
\n\nThe State indicator shows whether the simulator is RUN or PAUSE.",
        body_pt: "Barra de controles no topo: botões [s] Step, [r] Run/Stop, [p] Pause, [R] Restart.\
\n\nO botão Speed controla a velocidade: 1x → 2x → 4x → 8x → GO (máxima velocidade).\
\n\nO indicador State mostra se o simulador está RUN ou PAUSE.",
        target: target_controls,
        setup: None,
    },
    TutorialStep {
        title_en: "Cores & Harts",
        title_pt: "Cores e harts",
        body_en: "The Run bar also shows the selected Core and the current Hart bound to it.\
\n\nA hart is a hardware thread: one hart occupies one core. Switching the Core selector changes which runtime state you are observing.\
\n\nIf a core is FREE, there is no hart bound to it yet, so the main panels show placeholders instead of a fake PC.",
        body_pt: "A barra do Run também mostra o Core selecionado e o Hart atualmente ligado a ele.\
\n\nUm hart é uma hardware thread: um hart ocupa um core. Trocar o seletor de Core muda qual estado de runtime você está observando.\
\n\nSe um core estiver FREE, ainda não existe hart ligado a ele, então os painéis principais mostram placeholders em vez de um PC falso.",
        target: target_controls,
        setup: None,
    },
    TutorialStep {
        title_en: "Register Panel",
        title_pt: "Painel de registradores",
        body_en: "The left sidebar shows the 32 RISC-V integer registers (x0–x31) with their current values.\
\n\nRegisters modified by the last instruction are highlighted. Use [v] to switch between RAM, Registers and Dyn mode.",
        body_pt: "O painel lateral esquerdo mostra os 32 registradores inteiros do RISC-V (x0–x31) com seus valores atuais.\
\n\nRegistradores modificados na última instrução ficam destacados. Use [v] para alternar entre RAM, Registradores e modo Dyn.",
        target: target_sidebar,
        setup: Some(setup_show_registers),
    },
    TutorialStep {
        title_en: "Registers — details",
        title_pt: "Registradores — detalhes",
        body_en: "Press [P] on a register to pin it at the top of the list — useful for monitoring important values.\
\n\nThe color indicates the \"age\" of the last write: yellow = recent, fading with time.\
\n\nPress [Tab] in the register panel to switch between integer and float banks (f0–f31).",
        body_pt: "Pressione [P] sobre um registrador para fixá-lo (pinado) no topo da lista — útil para monitorar valores importantes.\
\n\nA cor indica a \"idade\" do último write: amarelo = recente, esmaecendo com o tempo.\
\n\nPressione [Tab] no painel de registradores para alternar entre bancos inteiro e float (f0–f31).",
        target: target_sidebar,
        setup: Some(setup_show_registers),
    },
    TutorialStep {
        title_en: "RAM / Memory Panel",
        title_pt: "Painel RAM / memória",
        body_en: "Use [v] to switch the sidebar to RAM view. The [k] key cycles through regions: DATA → STACK → R/W → HEAP.\
\n\nUse Ctrl+F to jump to a specific memory address. The number of bytes per row is configurable.",
        body_pt: "Com [v] alterne o painel lateral para visualizar a RAM. A tecla [k] cicla entre regiões: DATA → STACK → R/W → HEAP.\
\n\nUse Ctrl+F para pular para um endereço de memória específico. O número de bytes por linha é configurável.",
        target: target_sidebar,
        setup: Some(setup_show_ram),
    },
    TutorialStep {
        title_en: "Instruction Memory",
        title_pt: "Memória de instruções",
        body_en: "Center panel showing the program's instructions with their addresses and opcodes.\
\n\nThe current instruction (PC) is highlighted. Breakpoints appear as red markers — press [F9] to toggle.\
\n\nUse Ctrl+G to jump to a specific label.\
\n\nClicking an instruction moves the PC to it, allowing jumps at runtime.",
        body_pt: "Painel central mostrando as instruções do programa com seus endereços e opcodes.\
\n\nA instrução atual (PC) é destacada. Breakpoints aparecem como marcadores vermelhos — pressione [F9] para alternar.\
\n\nUse Ctrl+G para pular para uma label específica.\
\n\nClicar em uma instrução move o PC para ela, permitindo jumps em tempo de execução.",
        target: target_imem,
        setup: None,
    },
    TutorialStep {
        title_en: "Trace Panel",
        title_pt: "Painel de trace",
        body_en: "The execution trace shows the last executed instructions in chronological order, useful for understanding program flow.\
\n\nPress [t] to enable or disable the trace panel. When active, the instruction panel is split vertically.",
        body_pt: "O trace de execução mostra as últimas instruções executadas em ordem cronológica, útil para entender o fluxo do programa.\
\n\nPressione [t] para ativar ou desativar o painel de trace. Quando ativo, o painel de instruções é dividido verticalmente.",
        target: target_trace,
        setup: Some(setup_show_trace),
    },
    TutorialStep {
        title_en: "Details Panel",
        title_pt: "Painel de detalhes",
        body_en: "The right panel shows detailed information about the hovered instruction: type, operands, binary format and description.\
\n\nHover over an instruction in the center panel to see its decoded details here. Also shows the values of the registers involved.",
        body_pt: "O painel direito exibe informações detalhadas sobre a instrução sob o cursor: tipo, operandos, formato binário e descrição.\
\n\nPasse o mouse sobre uma instrução no painel central para ver seus detalhes decodificados aqui. Mostra também o valor dos registradores envolvidos.",
        target: target_details,
        setup: None,
    },
    TutorialStep {
        title_en: "Console",
        title_pt: "Console",
        body_en: "The console at the bottom displays program output (print syscalls) and error messages.\
\n\nWhen the program performs an input read (read syscall), the console waits for keyboard input.\
\n\nDragging the console's top border resizes the panel.",
        body_pt: "O console na parte inferior exibe a saída do programa (syscalls de print) e mensagens de erro.\
\n\nQuando o programa faz uma leitura de entrada (syscall read), o console aguarda digitação.\
\n\nArrastar a borda superior do console redimensiona o painel.",
        target: target_console,
        setup: None,
    },
    TutorialStep {
        title_en: "Breakpoints & Step",
        title_pt: "Breakpoints & step",
        body_en: "Press [F9] on the desired line to add/remove a breakpoint. The simulator stops automatically when it hits one.\
\n\n[s] executes one instruction at a time (step). [r] starts or stops continuous execution. [p] pauses without resetting.\
\n\n[R] restarts from the beginning, reloading the program compiled in the Editor.",
        body_pt: "Pressione [F9] na linha desejada para adicionar/remover um breakpoint. O simulador para automaticamente ao atingi-lo.\
\n\n[s] executa uma instrução de cada vez (step). [r] inicia ou para a execução contínua. [p] pausa sem resetar.\
\n\n[R] reinicia do início, recarregando o programa compilado no Editor.",
        target: target_controls,
        setup: None,
    },
];
