use super::*;
use crate::ui::input::keyboard::KeyOutcome;

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
    // Kitty keyboard protocol — lets the terminal distinguish Ctrl+Enter from Enter.
    // Silently ignored on terminals that don't support it.
    let _ = execute!(
        terminal.backend_mut(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let mut last_draw = Instant::now();
    loop {
        #[cfg(unix)]
        if quit_flag.map_or(false, |f| f.load(Ordering::Relaxed)) {
            break;
        }

        let poll_timeout = if app.run.is_running && matches!(app.run.speed, RunSpeed::Instant) {
            Duration::ZERO
        } else {
            Duration::from_millis(10)
        };
        match event::poll(poll_timeout) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) => {
                    if matches!(handle_key(app, key)?, KeyOutcome::Quit) {
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
                    if matches!(app.tab, Tab::Editor)
                        || (matches!(app.tab, Tab::Run)
                            && (app.run.imem_search_open || app.run.mem_search_open))
                    {
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
        // The boot splash is fixed-length: hand off to the console by itself.
        if let Some(started) = app.splash_start {
            if started.elapsed().as_secs_f64() >= crate::ui::view::SPLASH_SECS {
                app.splash_start = None;
            }
        }
        app.tick();
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, app))?;
            last_draw = Instant::now();
        }
    }
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    Ok(())
}
