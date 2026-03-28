use super::*;

#[cfg(unix)]
pub fn run(
    terminal: &mut DefaultTerminal,
    mut app: App,
    quit_flag: Arc<AtomicBool>,
) -> io::Result<()> {
    run_inner(terminal, &mut app, Some(&quit_flag))
}

#[cfg(not(unix))]
pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    run_inner(terminal, &mut app, None::<&AtomicBool>)
}

fn run_inner(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    #[allow(unused)] quit_flag: Option<&AtomicBool>,
) -> io::Result<()> {
    execute!(
        terminal.backend_mut(),
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let mut last_draw = Instant::now();
    loop {
        #[cfg(unix)]
        if quit_flag.map_or(false, |f| f.load(Ordering::Relaxed)) {
            break;
        }

        match event::poll(Duration::from_millis(10)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) => {
                    if handle_key(app, key)? {
                        break;
                    }
                }
                Ok(Event::Mouse(me)) => {
                    let size = terminal.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    handle_mouse(app, me, area);
                    if app.should_quit {
                        break;
                    }
                }
                Ok(Event::Paste(text)) => {
                    if matches!(app.tab, Tab::Editor) {
                        use crate::ui::input::keyboard::paste_from_terminal;
                        app.last_bracketed_paste = Some(Instant::now());
                        paste_from_terminal(app, &text);
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            },
            Ok(false) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }

        if app.should_quit {
            break;
        }
        app.tick();
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, app))?;
            last_draw = Instant::now();
        }
    }
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    Ok(())
}
