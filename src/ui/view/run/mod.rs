use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use super::components::panel::{self, PanelKind, render_panel};
use super::components::render_console;
pub(super) use super::{App, MemRegion, RunButton};
pub(super) use crate::ui::app::FormatMode;
use crate::ui::app::HartLifecycle;
use crate::ui::theme;
use crate::ui::view::style;

mod formatting;
mod instruction_details;
mod instruction_list;
mod memory;
mod registers;
mod sidebar;
mod status;

use instruction_details::render_instruction_details;
use instruction_list::{render_exec_trace, render_instruction_memory};
use sidebar::render_sidebar;
pub(crate) use status::{
    RUN_BAR_INDENT, RUN_DISPLAY_ROW, build_run_display_toolbar, build_run_toolbar,
    render_run_status,
    run_controls_plain_text,
};

pub(crate) const RUN_COLLAPSED_RAIL_W: u16 = 2;
pub(crate) const RUN_SIDEBAR_MIN_W: u16 = 20;
pub(crate) const RUN_IMEM_MIN_W: u16 = 20;
pub(crate) const RUN_DETAILS_MIN_W: u16 = 46;

/// The Run tab's vertical split: `[command bar, panes, console]`.
///
/// Shared by the renderer and `input::mouse` so a click lands where the pixel
/// is. The command bar's height used to be spelled `Constraint::Length(5)` in
/// five separate places, three of them in the mouse router.
pub(crate) fn run_rows(app: &App, area: Rect) -> [Rect; 3] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(status::RUN_COMMAND_BAR_H),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(area);
    [rows[0], rows[1], rows[2]]
}

pub(crate) fn run_panel_constraints(app: &App) -> [Constraint; 3] {
    let sidebar = if app.run.sidebar_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Length(app.run.sidebar_width)
    };
    let imem = if app.run.imem_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Length(app.run.imem_width)
    };
    let details = if app.run.details_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Min(RUN_DETAILS_MIN_W)
    };
    [sidebar, imem, details]
}

pub(super) fn render_run(f: &mut Frame, area: Rect, app: &App) {
    // Cleared every frame; each list re-registers its scrollbar track when (and
    // only when) the bar is actually drawn. Without this a collapsed pane would
    // leave its last track behind for the mouse to hit.
    app.run.regs_sb.set(None);
    app.run.mem_sb.set(None);
    app.run.imem_sb.set(None);

    let layout = run_rows(app, area);

    render_run_status(f, layout[0], app);

    let main = layout[1];

    // Screen sub-view: the program's framebuffer replaces the CPU columns
    // (status bar and console stay visible for controls and prints).
    if app.run.show_screen {
        if let Some(screen) = &app.console.screen {
            render_screen_canvas(f, main, screen);
            render_console(f, layout[2], app);
            return;
        }
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(run_panel_constraints(app))
        .split(main);

    if app.core_status(app.selected_core) == HartLifecycle::Free {
        render_free_core_panel(
            f,
            columns[0],
            "Core State",
            "No hart is bound to this core.",
        );
        render_free_core_panel(
            f,
            columns[1],
            "Instruction Memory",
            "This core is currently FREE, so there is no PC or instruction stream to follow.",
        );
        render_free_core_panel(
            f,
            columns[2],
            "Details",
            "Spawn a hart onto this core first. If spawn failed, check the console for the reason.",
        );
        render_console(f, layout[2], app);
        return;
    }

    if app.run.sidebar_collapsed {
        render_collapsed(f, columns[0], "S", "collapsed", '▶');
    } else {
        render_sidebar(f, columns[0], app);
        render_sidebar_drag_arrow(f, columns[0], app);
    }

    if app.run.imem_collapsed {
        render_collapsed(f, columns[1], "I", "collapsed", '▶');
    } else if app.run.show_trace && columns[1].height >= 10 {
        // Split instruction memory column: top = imem, bottom = trace
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(columns[1]);
        render_instruction_memory(f, split[0], app);
        render_exec_trace(f, split[1], app);
    } else {
        render_instruction_memory(f, columns[1], app);
    }

    if app.run.details_collapsed {
        render_collapsed(f, columns[2], "D", "collapsed", '◀');
    } else {
        render_instruction_details(f, columns[2], app);
    }

    render_console(f, layout[2], app);
}

/// Draw the front buffer with half-block cells (▀: fg = top pixel, bg = bottom
/// pixel → two vertical pixels per terminal cell), nearest-neighbor downscaled
/// to fit and centered. Keys go to the program while this view is open (Esc
/// returns to the CPU view).
fn render_screen_canvas(f: &mut Frame, area: Rect, screen: &crate::ui::screen::Screen) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT))
        .title(format!(
            "Screen {}x{} — keys go to the program, Esc returns",
            screen.width, screen.height
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // One terminal cell = 1 pixel wide x 2 pixels tall. Scale down (never up)
    // preserving aspect, nearest neighbor.
    let (sw, sh) = (screen.width as u64, screen.height as u64);
    let (aw, ah) = (inner.width as u64, (inner.height as u64) * 2);
    let num = sw
        .max(1)
        .div_ceil(aw.max(1))
        .max(sh.max(1).div_ceil(ah.max(1)));
    let step = num.max(1); // guest pixels per cell-pixel
    let out_w = (sw / step).max(1).min(aw) as u16;
    let out_h_px = (sh / step).max(1).min(ah);
    let out_h = out_h_px.div_ceil(2) as u16; // terminal rows

    let x0 = inner.x + (inner.width - out_w) / 2;
    let y0 = inner.y + (inner.height - out_h) / 2;
    let px = |x: u64, y: u64| -> Color {
        if x >= sw || y >= sh {
            return Color::Black;
        }
        let c = screen.front[(y * sw + x) as usize];
        Color::Rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
    };

    let buf = f.buffer_mut();
    for row in 0..out_h {
        for col in 0..out_w {
            let gx = col as u64 * step;
            let gy_top = (row as u64 * 2) * step;
            let gy_bot = (row as u64 * 2 + 1) * step;
            let cell = &mut buf[(x0 + col, y0 + row)];
            cell.set_symbol("▀");
            cell.set_fg(px(gx, gy_top));
            cell.set_bg(if (row as u64 * 2 + 1) < out_h_px {
                px(gx, gy_bot)
            } else {
                Color::Black
            });
        }
    }
}

fn render_collapsed(
    f: &mut Frame,
    area: Rect,
    short: &'static str,
    state: &'static str,
    arrow: char,
) {
    let bg = Style::default().bg(Color::Rgb(20, 26, 38));
    f.render_widget(Block::default().style(bg), area);

    if area.height == 0 || area.width == 0 {
        return;
    }

    let title_style = Style::default()
        .fg(theme::ACCENT)
        .bg(Color::Rgb(28, 38, 54))
        .add_modifier(Modifier::BOLD);
    let title = if area.width >= 2 { "··" } else { "·" };
    f.render_widget(
        Paragraph::new(title).style(title_style),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let mid = area.y + area.height / 2;
    if mid < area.y + area.height {
        f.render_widget(
            Paragraph::new(short).style(
                Style::default()
                    .fg(theme::LABEL_Y)
                    .bg(Color::Rgb(20, 26, 38))
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(area.x, mid.saturating_sub(1).max(area.y), area.width, 1),
        );
        f.render_widget(
            Paragraph::new(arrow.to_string()).style(
                style::success()
                    .bg(Color::Rgb(20, 26, 38))
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(area.x, mid, area.width, 1),
        );
        if mid + 1 < area.y + area.height {
            let state_label = if area.width >= 2 { "×" } else { state };
            f.render_widget(
                Paragraph::new(state_label).style(style::warning().bg(Color::Rgb(20, 26, 38))),
                Rect::new(area.x, mid + 1, area.width, 1),
            );
        }
    }
}

fn render_free_core_panel(f: &mut Frame, area: Rect, title: &'static str, body: &'static str) {
    let inner = render_panel(f, area, panel::panel(title, PanelKind::Plain));
    f.render_widget(
        Paragraph::new(body)
            .style(style::label())
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The resize grip on the sidebar's right edge.
///
/// Only drawn while the edge is hovered. It sits *on* the panel border, so an
/// always-visible `>` punched a hole in the frame of every screenshot to
/// advertise an affordance the user only needs once they are already there.
fn render_sidebar_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    if !app.run.hover_sidebar_bar {
        return;
    }
    let grip = Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y + area.height / 2,
        1,
        1,
    );
    f.render_widget(
        Paragraph::new("┃").style(Style::default().fg(theme::ACCENT)),
        grip,
    );
}

#[cfg(test)]
mod tests {
    use crate::ui::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|f| super::render_run(f, f.area(), app))
            .expect("render");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app(id: &str) -> App {
        App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, id).unwrap()
    }

    /// The Run tab is one view now, so every architecture gets the same panes —
    /// the point of the whole refactor. A backend that only got a cut-down
    /// version would fail here.
    #[test]
    fn every_architecture_gets_the_same_run_panes() {
        for id in crate::arch::registry().ids() {
            let screen = screen(&app(id), 160, 40);
            // `Field Map` and `Decoded` are labelled rules inside the one
            // Instruction panel now rather than two more nested boxes.
            for text in ["Registers", "Instruction Memory", "ENCODING", "DECODED"] {
                assert!(screen.contains(text), "{id} is missing {text}:\n{screen}");
            }
        }
    }

    /// Every pane must show the *loaded* architecture's decode, not RV32's.
    ///
    /// This is the regression that matters: running RV32's decoder over SAP
    /// bytes produced a plausible-looking `addi a0, zero, 1` with RV32 field
    /// names, which is worse than showing nothing.
    #[test]
    fn each_backend_decodes_with_its_own_isa() {
        // (architecture, a field name only this ISA has, its instruction width
        //  in binary digits)
        for (id, own_field, bits) in [
            ("riscv32", "funct3", 32),
            ("toy16", "rd", 16),
            ("sap", "address", 8),
        ] {
            let screen = screen(&app(id), 160, 40);
            assert!(
                screen.contains(own_field),
                "{id}: no {own_field} in its own decode:\n{screen}"
            );
            // The header prints the encoding in binary at the ISA's real width.
            // Padding an 8-bit instruction out to RV32's 32 bits — the old
            // behaviour — would fail this.
            let binary_runs: Vec<usize> = screen
                .split(|c: char| !matches!(c, '0' | '1'))
                .map(str::len)
                .filter(|len| *len >= 8)
                .collect();
            assert!(
                binary_runs.contains(&bits),
                "{id}: expected a {bits}-bit encoding, saw runs {binary_runs:?}:\n{screen}"
            );
        }
    }

    /// Render `tab` and return its rows, minus the footer.
    ///
    /// The footer is the one place a key hint belongs, so the rule below has to
    /// exclude it or it would flag the very thing it is protecting.
    fn tab_body(app: &mut App, tab: crate::ui::app::Tab, w: u16, h: u16) -> Vec<String> {
        app.tab = tab;
        app.splash_start = None;
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
        terminal
            .draw(|f| crate::ui::view::ui(f, app))
            .expect("render");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height - 1)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// The affordance rule, enforced across the whole app: a keyboard key is
    /// written `[key]` **in the footer**, and nowhere else.
    ///
    /// Rather than banning the bracket character — which would fight `imm[11:0]`
    /// in RISC-V field names and `[rel label]` in Intel syntax — this looks for
    /// the shapes that only ever mean "this is a key": the help glyph, the
    /// `[Tab]=Float` equals-suffix, and a capitalised modifier name, which the
    /// footer never emits because it writes keys in lower case.
    #[test]
    fn no_tab_writes_a_key_hint_outside_the_footer() {
        // Chrome shapes — a bracketed key name. No prose writes these.
        const CHROME_SHAPES: &[&str] = &["[?]", "]=", "[Tab]", "[Space]", "[Esc]", "[Enter]"];
        // Bare key names. Chrome must not spell these out, but the Docs tab is
        // reference material and naming a key is its entire job, so it is exempt
        // from this second list only.
        const NAMED_KEYS: &[&str] = &["Ctrl+", "PgUp"];

        for id in crate::arch::registry().ids() {
            let mut app = app(id);
            app.set_cache_enabled(true);
            for (tab, sub, what) in every_subtab(&app) {
                sub.apply(&mut app);
                let shapes: Vec<&str> = if matches!(tab, crate::ui::app::Tab::Docs) {
                    CHROME_SHAPES.to_vec()
                } else {
                    CHROME_SHAPES.iter().chain(NAMED_KEYS).copied().collect()
                };
                for line in tab_body(&mut app, tab, 160, 44) {
                    for shape in &shapes {
                        assert!(
                            !line.contains(shape),
                            "{id} {what}: `{shape}` is a key hint, and keys live in the footer:\n  {}",
                            line.trim_end()
                        );
                    }
                }
            }
        }
    }

    /// Which subtab to park on before rendering. Applied at render time rather
    /// than while enumerating, or every entry would end up showing whichever
    /// subtab the enumeration happened to stop on.
    #[derive(Clone, Copy)]
    enum Subtab {
        Only,
        Cache(crate::ui::app::CacheSubtab),
        Docs(crate::ui::app::DocsPage),
        Pipeline(crate::ui::pipeline::PipelineSubtab),
        Vm(crate::ui::app::VmSubtab),
    }

    impl Subtab {
        fn apply(self, app: &mut App) {
            match self {
                Subtab::Only => {}
                Subtab::Cache(sub) => app.cache.subtab = sub,
                Subtab::Docs(page) => app.docs.page = page,
                Subtab::Pipeline(sub) => app.run.pipeline_view_mut().subtab = sub,
                Subtab::Vm(sub) => app.tlb.vm_subtab = sub,
            }
        }
    }

    /// Every tab, and every subtab within the tabs that have them.
    ///
    /// A tab's default subtab is the one a smoke test would see; the rest is
    /// where stale chrome survives a design pass unnoticed.
    fn every_subtab(app: &App) -> Vec<(crate::ui::app::Tab, Subtab, String)> {
        use crate::ui::app::{CacheSubtab, Tab};
        use crate::ui::pipeline::PipelineSubtab;

        let mut out = Vec::new();
        for tab in app.visible_tabs().to_vec() {
            match tab {
                Tab::Cache => {
                    for (sub, name) in [
                        (CacheSubtab::Stats, "stats"),
                        (CacheSubtab::Config, "config"),
                        (CacheSubtab::View, "view"),
                    ] {
                        out.push((tab, Subtab::Cache(sub), format!("Cache/{name}")));
                    }
                }
                Tab::Docs => {
                    for page in crate::ui::view::docs::visible_pages(app) {
                        out.push((tab, Subtab::Docs(page), format!("Docs/{}", page.label())));
                    }
                }
                Tab::Pipeline => {
                    for (sub, name) in [
                        (PipelineSubtab::Main, "main"),
                        (PipelineSubtab::Config, "config"),
                    ] {
                        out.push((tab, Subtab::Pipeline(sub), format!("Pipeline/{name}")));
                    }
                }
                Tab::Tlb => {
                    for sub in crate::ui::app::VmSubtab::ALL {
                        out.push((tab, Subtab::Vm(sub), format!("VM/{}", sub.label())));
                    }
                }
                _ => out.push((tab, Subtab::Only, tab.label().to_string())),
            }
        }
        out
    }

    /// Every tab must survive a render at the sizes people actually use, on
    /// every backend — the design-system pass moved geometry in all of them.
    #[test]
    fn every_tab_renders_at_every_size() {
        for id in crate::arch::registry().ids() {
            let mut app = app(id);
            app.splash_start = None;
            for tab in app.visible_tabs().to_vec() {
                app.tab = tab;
                for (w, h) in [(200, 60), (150, 40), (100, 30), (80, 24), (40, 12), (20, 6)] {
                    let mut terminal =
                        Terminal::new(TestBackend::new(w, h)).expect("terminal");
                    terminal
                        .draw(|f| crate::ui::view::ui(f, &app))
                        .unwrap_or_else(|_| panic!("{id} {:?} panicked at {w}x{h}", tab.label()));
                }
            }
        }
    }

    /// SAP has a second register bank; the sidebar must offer it by the name
    /// the backend gave it rather than RV32's "float".
    ///
    /// The banks are advertised as *state* in the panel title now rather than as
    /// a `[Tab]=Flags` key hint — a bracket means a keyboard key and nothing
    /// else, so the key itself lives in the footer. What must not regress is
    /// that the name comes from the backend.
    #[test]
    fn the_bank_toggle_is_named_by_the_backend() {
        let sap = screen(&app("sap"), 160, 40);
        assert!(sap.contains("Flags"), "sap does not offer its bank:\n{sap}");
        // A single-bank ISA has nothing to offer beside the one it shows.
        let toy = screen(&app("toy16"), 160, 40);
        assert!(!toy.contains(" · "), "toy16 invented a second bank:\n{toy}");
    }

    /// The affordance rule, enforced on the chrome: a `[bracket]` is how the UI
    /// writes a keyboard key, and keys live in the footer — so no bracket may
    /// appear on a panel title bar.
    ///
    /// The bracket used to mean three things at once — the instruction class
    /// (`[I]`), a key (`[P]=pin`), and the help button (`[?]`) — which is
    /// exactly why the help button was impossible to pick out. Data is bare
    /// coloured text and a button has a filled background.
    ///
    /// Only the *chrome* is checked. Disassembly is content, and plenty of ISAs
    /// bracket legitimately — `imm[11:0]` in RISC-V field names, `[rel label]`
    /// in Intel syntax. Banning the character outright would fight the ISAs.
    #[test]
    fn no_panel_title_spends_a_bracket_on_a_key() {
        for id in crate::arch::registry().ids() {
            let offenders: Vec<String> = screen(&app(id), 160, 40)
                .lines()
                .flat_map(|line| line.split('╭').skip(1))
                .filter(|title| title.contains('['))
                .map(|title| format!("  ╭{}", title.trim_end()))
                .collect();
            assert!(
                offenders.is_empty(),
                "{id}: a key hint is still living in a panel title:\n{}",
                offenders.join("\n")
            );
        }
    }

    /// The help button is a button, so it wears a surface and a word. `[?]` in a
    /// bordered box wore the same chrome as the boxes that hold data.
    ///
    /// Only the tab row is inspected: the footer legitimately writes `[?] help`,
    /// because that is a key, which is the one thing a bracket is allowed to be.
    #[test]
    fn the_help_button_is_a_labelled_button_not_a_bracketed_glyph() {
        let mut app = app("riscv32");
        app.splash_start = None;
        let mut terminal = Terminal::new(TestBackend::new(160, 40)).expect("terminal");
        terminal
            .draw(|f| crate::ui::view::ui(f, &app))
            .expect("render");
        let buffer = terminal.backend().buffer().clone();
        let tab_row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol())
            .collect();
        assert!(!tab_row.contains("[?]"), "{tab_row}");
        assert!(tab_row.contains(" ? Help "), "{tab_row}");
    }

    /// Small terminals must not panic on any backend.
    #[test]
    fn the_run_tab_survives_small_terminals_on_every_architecture() {
        for id in crate::arch::registry().ids() {
            let app = app(id);
            for (w, h) in [(160, 40), (80, 24), (40, 12), (20, 6)] {
                let _ = screen(&app, w, h);
            }
        }
    }
}
