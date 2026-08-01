//! Full Run-tab presentation for trait-driven architectures.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{List, ListItem, Paragraph},
};
use raven_riscv_engine::capability::{InstructionCodec, MemoryInspect, RegisterFile};
use raven_riscv_engine::{Machine, MachineSnapshot};

use super::{App, style};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};

const LISTING_ROWS: usize = 64;
const LISTING_LEAD: usize = 6;
const DUMP_BYTES: usize = 256;
const DUMP_STRIDE: usize = 8;

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let (Some(machine), Some(snapshot)) = (app.machine.as_deref(), app.machine_snapshot()) else {
        return;
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(area);
    render_status(f, rows[0], app, &snapshot);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.run.sidebar_width.max(20)),
            Constraint::Length(app.run.imem_width.max(24)),
            Constraint::Min(36),
        ])
        .split(rows[1]);
    render_sidebar(f, columns[0], app, machine, &snapshot);
    render_listing(f, columns[1], app, machine, &snapshot);
    render_details(f, columns[2], machine, &snapshot);
    render_console(f, rows[2], &snapshot);
}

fn render_status(f: &mut Frame, area: Rect, app: &App, snapshot: &MachineSnapshot) {
    let state = if app.run.is_running { "run" } else { "pause" };
    let state_color = if app.run.is_running { theme::RUNNING } else { theme::PAUSED };
    let cache_cycles = app.cache_hierarchy().map_or(0, |caches| {
        [
            raven_riscv_engine::capability::CacheRole::Instruction,
            raven_riscv_engine::capability::CacheRole::Data,
        ]
        .iter()
        .filter_map(|role| caches.cache(0, *role))
        .map(|cache| cache.stats.total_cycles)
        .sum::<u64>()
    });
    let cpi = if snapshot.instructions == 0 {
        0.0
    } else {
        cache_cycles as f64 / snapshot.instructions as f64
    };
    let lines = vec![
        Line::from(vec![
            chip("arch", app.architecture.descriptor().id, theme::ACCENT),
            Span::raw("  "),
            chip("view", if app.run.show_registers { "regs" } else { "ram" }, theme::TEXT),
            Span::raw("  "),
            chip("speed", app.run.speed.label(), theme::TEXT),
            Span::raw("  "),
            chip("state", state, state_color),
            Span::raw("  "),
            chip("count", if app.run.show_exec_count { "on" } else { "off" }, theme::TEXT),
            Span::raw("  "),
            chip("type", if app.run.show_instr_type { "on" } else { "off" }, theme::TEXT),
            Span::raw("  "),
            chip("reset", "r", theme::DANGER),
        ]),
        Line::from(vec![
            Span::styled(format!("PC:0x{:X}", snapshot.pc), style::value()),
            Span::raw("  "),
            Span::styled(format!("Cache cycles:{cache_cycles}"), style::metric(style::Metric::Cycles)),
            Span::raw("  "),
            Span::styled(format!("CPI:{cpi:.2}"), style::metric(style::Metric::Cpi)),
            Span::raw("  "),
            Span::styled(format!("Instrs:{}", snapshot.instructions), style::label()),
        ]),
        Line::from(Span::styled(
            "s=step  p/space=run/pause  r=reset  v=registers/memory  f=speed",
            style::label(),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(panel::panel_frame(PanelKind::Plain).title("Run Controls")),
        area,
    );
}

fn chip(label: &'static str, value: &str, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {label}:{value} "),
        Style::default().fg(color).bg(theme::BG_RAISED).bold(),
    )
}

fn render_sidebar(
    f: &mut Frame,
    area: Rect,
    app: &App,
    machine: &dyn Machine,
    snapshot: &MachineSnapshot,
) {
    if app.run.show_registers || app.run.show_dyn {
        render_registers(f, area, machine, snapshot);
    } else {
        render_memory(f, area, app, machine);
    }
}

fn render_registers(f: &mut Frame, area: Rect, machine: &dyn Machine, snapshot: &MachineSnapshot) {
    let mut lines = vec![
        field("State", &snapshot.state.to_string()),
        field("PC", &format!("0x{:X}", snapshot.pc)),
    ];
    if let Some(fault) = &snapshot.fault {
        lines.push(Line::from(Span::styled(fault.clone(), style::danger())));
    }
    if let Some(registers) = machine.registers() {
        lines.extend(register_lines(registers));
    } else {
        lines.extend(snapshot.registers.iter().map(|register| {
            field(&register.name, &format!("0x{:X}", register.value))
        }));
    }
    let inner = render_panel(f, area, panel::panel("Registers", PanelKind::Accent));
    f.render_widget(Paragraph::new(lines), inner);
}

fn register_lines(registers: &dyn RegisterFile) -> Vec<Line<'static>> {
    let entries = registers.entries();
    let mut lines = Vec::with_capacity(entries.len() + registers.banks().len());
    for (index, bank) in registers.banks().iter().enumerate() {
        lines.push(Line::from(Span::styled(bank.label, style::label())));
        lines.extend(entries.iter().filter(|entry| entry.id.bank == index).map(|entry| {
            let name = entry.alias.as_deref().unwrap_or(&entry.name);
            Line::from(vec![
                Span::styled(format!("{name:<8}"), style::idle()),
                Span::styled(entry.hex(), style::value()),
            ])
        }));
    }
    lines
}

fn render_memory(f: &mut Frame, area: Rect, app: &App, machine: &dyn Machine) {
    let Some(memory) = machine.memory() else {
        return;
    };
    let base = if matches!(app.run.mem_region, crate::ui::app::MemRegion::Custom) {
        u64::from(app.run.mem_view_addr)
    } else {
        memory.regions().first().map_or(0, |region| region.address)
    }
    .min(memory.size().saturating_sub(1));
    let inner = render_panel(f, area, panel::panel("Memory", PanelKind::Accent));
    f.render_widget(Paragraph::new(hex_dump(memory, base)), inner);
}

fn render_listing(
    f: &mut Frame,
    area: Rect,
    app: &App,
    machine: &dyn Machine,
    snapshot: &MachineSnapshot,
) {
    let (Some(code), Some(memory)) = (machine.code(), machine.memory()) else {
        return;
    };
    let last = app
        .editor
        .last_ok_image
        .as_ref()
        .map_or(memory.size(), |image| image.end_address().min(memory.size()));
    let mut address = listing_start(code, memory, snapshot.pc);
    let mut items = Vec::with_capacity(LISTING_ROWS);
    while items.len() < LISTING_ROWS && address < last {
        let bytes = memory.peek(address, 8);
        if bytes.is_empty() {
            break;
        }
        let width = code.instruction_width(address, &bytes).max(1);
        let text = code.disassemble(address, &bytes).unwrap_or_else(|| raw(&bytes[..width.min(bytes.len())]));
        let current = address == snapshot.pc;
        let count = app.run.exec_counts.get(&(address as u32)).copied().unwrap_or(0);
        let class = code.inspect(address, &bytes).map(|info| info.class).unwrap_or("?");
        let mut spans = vec![];
        if app.run.show_instr_type {
            spans.push(Span::styled(format!("[{class:.1}] "), Style::default().fg(theme::ACCENT)));
        }
        spans.push(Span::styled(if current { " ▶" } else { "  " }, Style::default().fg(theme::ACCENT)));
        spans.push(Span::styled(format!("0x{address:04X}:  {text}"), if current { Style::default().fg(Color::Black).bg(Color::Yellow) } else { style::value() }));
        if app.run.show_exec_count && count > 0 {
            spans.push(Span::styled(format!(" ×{count}"), Style::default().fg(Color::Cyan)));
        }
        items.push(ListItem::new(Line::from(spans)));
        address += width as u64;
    }
    let inner = render_panel(f, area, panel::panel("Instruction Memory", PanelKind::Accent));
    f.render_widget(List::new(items), inner);
}

fn render_details(f: &mut Frame, area: Rect, machine: &dyn Machine, snapshot: &MachineSnapshot) {
    let (Some(code), Some(memory)) = (machine.code(), machine.memory()) else {
        return;
    };
    let bytes = memory.peek(snapshot.pc, 8);
    let Some(info) = code.inspect(snapshot.pc, &bytes) else {
        let inner = render_panel(f, area, panel::panel("Instruction Inspector", PanelKind::Accent));
        f.render_widget(Paragraph::new("No structured decoder data."), inner);
        return;
    };
    let hex_width = usize::from(info.encoding_bits).div_ceil(4);
    let mut lines = vec![
        Line::from(Span::styled(info.mnemonic, Style::default().fg(Color::Yellow).bold())),
        Line::raw(""),
        field("Class", info.class),
        field("Address", &format!("0x{:X}", snapshot.pc)),
        field("Encoding", &format!("0x{:0hex_width$X}", info.encoding)),
        field("Binary", &format!("{:0width$b}", info.encoding, width = usize::from(info.encoding_bits))),
        Line::raw(""),
        Line::from(Span::styled("Decoded fields", style::label())),
    ];
    lines.extend(info.fields.into_iter().map(|field_info| field(field_info.name, &field_info.value)));
    let inner = render_panel(f, area, panel::panel("Instruction Inspector", PanelKind::Accent));
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_console(f: &mut Frame, area: Rect, snapshot: &MachineSnapshot) {
    let output = String::from_utf8_lossy(&snapshot.stdout);
    let text = if output.is_empty() { "(no output)" } else { output.as_ref() };
    let inner = render_panel(f, area, panel::panel("Console", PanelKind::Plain));
    f.render_widget(Paragraph::new(text.to_string()), inner);
}

fn listing_start(code: &dyn InstructionCodec, memory: &dyn MemoryInspect, pc: u64) -> u64 {
    let step = code.instruction_width(pc, &memory.peek(pc, 8)).max(1) as u64;
    pc.saturating_sub(step * LISTING_LEAD as u64)
}

fn hex_dump(memory: &dyn MemoryInspect, base: u64) -> Vec<Line<'static>> {
    memory
        .peek(base, DUMP_BYTES)
        .chunks(DUMP_STRIDE)
        .enumerate()
        .map(|(row, chunk)| {
            Line::from(vec![
                Span::styled(format!("{:04X}  ", base + (row * DUMP_STRIDE) as u64), style::idle()),
                Span::styled(chunk.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" "), style::value()),
            ])
        })
        .collect()
}

fn raw(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ")
}

fn field(name: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name:<12}"), style::label()),
        Span::styled(value.to_string(), style::value()),
    ])
}

#[cfg(test)]
mod tests {
    use crate::ui::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal.draw(|f| super::render(f, f.area(), app)).expect("render");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app(id: &str) -> App {
        App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, id).unwrap()
    }

    #[test]
    fn toy16_gets_the_full_run_layout_and_inspector() {
        let screen = screen(&app("toy16"), 150, 32);
        for text in ["Run Controls", "Registers", "Instruction Memory", "Instruction Inspector", "Console", "Encoding", "r0"] {
            assert!(screen.contains(text), "missing {text}:\n{screen}");
        }
    }

    #[test]
    fn sap_gets_the_full_run_layout_and_inspector() {
        let screen = screen(&app("sap"), 150, 32);
        for text in ["Run Controls", "Registers", "Instruction Memory", "Instruction Inspector", "Console", "opcode"] {
            assert!(screen.contains(text), "missing {text}:\n{screen}");
        }
    }

    #[test]
    fn trait_run_renders_small_terminals_and_output() {
        let mut app = app("toy16");
        for _ in 0..5 { app.single_step(); }
        for size in [(150, 40), (80, 24), (40, 12), (20, 6)] {
            let _ = screen(&app, size.0, size.1);
        }
        assert!(screen(&app, 150, 32).contains("42"));
    }
}
