//! Screen device backing the graphics syscalls (2000+).
//!
//! The framebuffer lives host-side (never in guest RAM): draw syscalls write
//! the back buffer, `screen_present` publishes it to the front buffer, and the
//! front buffer is what the Run-tab "Screen" view or the OS window shows.
//! Pixels are `0x00RRGGBB`.
//!
//! ponytail: the framebuffer is not journaled â€” step-back rewinds CPU/memory
//! but not pixels. Journal it only if someone actually asks for visual rewind.

use std::collections::VecDeque;
use std::time::Instant;

/// Where a newly created screen shows up.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ScreenTarget {
    /// "Screen" sub-view of the Run tab. Also the headless default: syscalls
    /// draw into the in-memory buffer and nothing is displayed.
    #[default]
    Tui,
    /// Native OS window on its own thread (TUI Settings or CLI `--screen`).
    Window,
}

impl ScreenTarget {
    pub fn cycle(self) -> Self {
        match self {
            Self::Tui => Self::Window,
            Self::Window => Self::Tui,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tui => "TUI",
            Self::Window => "WINDOW",
        }
    }
}

// Key codes delivered by `screen_poll_key` (2005). Printable keys arrive as
// lowercase ASCII; specials get codes above the ASCII range.
pub const KEY_ENTER: u32 = 13;
pub const KEY_BACKSPACE: u32 = 8;
pub const KEY_ESC: u32 = 27; // OS-window mode only; the TUI reserves Esc
pub const KEY_UP: u32 = 256;
pub const KEY_DOWN: u32 = 257;
pub const KEY_LEFT: u32 = 258;
pub const KEY_RIGHT: u32 = 259;

pub const MIN_DIM: u32 = 8;
pub const MAX_DIM: u32 = 1024;
const MAX_QUEUED_KEYS: usize = 64;

pub struct Screen {
    pub width: u32,
    pub height: u32,
    /// Back buffer â€” every draw syscall writes here.
    buf: Vec<u32>,
    /// Front buffer â€” last `screen_present`; this is what gets displayed.
    pub front: Vec<u32>,
    /// Pending key presses, oldest first.
    keys: VecDeque<u32>,
    /// Wall-clock zero for `screen_time_ms`.
    t0: Instant,
    /// Bumped on every `screen_present`; the TUI repaints when it changes.
    pub frames: u64,
    window: Option<window::WindowBridge>,
}

impl Screen {
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;
        Self {
            width,
            height,
            buf: vec![0; len],
            front: vec![0; len],
            keys: VecDeque::new(),
            t0: Instant::now(),
            frames: 0,
            window: None,
        }
    }

    /// Spawn the OS window thread. On failure (macOS, musl, no display) the
    /// screen keeps working buffer-only and the error explains why.
    pub fn open_window(&mut self) -> Result<(), String> {
        self.window = Some(window::spawn(self.width, self.height)?);
        Ok(())
    }

    pub fn clear(&mut self, color: u32) {
        self.buf.fill(color & 0x00FF_FFFF);
    }

    /// Returns false when (x, y) is out of bounds (nothing drawn, no fault).
    pub fn set_pixel(&mut self, x: u32, y: u32, color: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.buf[(y * self.width + x) as usize] = color & 0x00FF_FFFF;
        true
    }

    /// Fill a rectangle, clipped to the screen bounds.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x1 = x.saturating_add(w).min(self.width);
        let y1 = y.saturating_add(h).min(self.height);
        let color = color & 0x00FF_FFFF;
        for row in y.min(self.height)..y1 {
            let base = (row * self.width) as usize;
            self.buf[base + x as usize..base + x1 as usize].fill(color);
        }
    }

    /// Publish the back buffer (front = what the display shows).
    pub fn present(&mut self) {
        self.front.copy_from_slice(&self.buf);
        self.frames += 1;
        if let Some(win) = &self.window {
            win.present(&self.front);
        }
    }

    /// Queue a key press (dropped when the queue is full).
    pub fn push_key(&mut self, code: u32) {
        if self.keys.len() < MAX_QUEUED_KEYS {
            self.keys.push_back(code);
        }
    }

    /// Oldest pending key press, or `None`. Non-blocking.
    pub fn poll_key(&mut self) -> Option<u32> {
        if let Some(win) = &self.window {
            win.drain_keys(&mut self.keys, MAX_QUEUED_KEYS);
        }
        self.keys.pop_front()
    }

    /// Wall-clock milliseconds since `screen_init`.
    pub fn time_ms(&self) -> u32 {
        self.t0.elapsed().as_millis() as u32
    }

    /// Whether the OS window is (still) open.
    pub fn window_alive(&self) -> bool {
        self.window.as_ref().is_some_and(|w| w.is_alive())
    }

    pub fn has_window(&self) -> bool {
        self.window.is_some()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if let Some(win) = &self.window {
            win.shutdown();
        }
    }
}

// â”€â”€ OS window (minifb), gated exactly like `rfd` in Cargo.toml â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(not(all(target_os = "linux", target_env = "musl")))]
mod window {
    use super::{KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP};
    use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    struct WinShared {
        front: Vec<u32>,
        dirty: bool,
        keys: VecDeque<u32>,
    }

    pub struct WindowBridge {
        shared: Arc<Mutex<WinShared>>,
        alive: Arc<AtomicBool>,
    }

    impl WindowBridge {
        pub fn present(&self, front: &[u32]) {
            let mut s = self.shared.lock().unwrap();
            s.front.copy_from_slice(front);
            s.dirty = true;
        }

        pub fn drain_keys(&self, out: &mut VecDeque<u32>, cap: usize) {
            let mut s = self.shared.lock().unwrap();
            while let Some(k) = s.keys.pop_front() {
                if out.len() < cap {
                    out.push_back(k);
                }
            }
        }

        pub fn is_alive(&self) -> bool {
            self.alive.load(Ordering::Relaxed)
        }

        pub fn shutdown(&self) {
            self.alive.store(false, Ordering::Relaxed);
        }
    }

    pub fn spawn(width: u32, height: u32) -> Result<WindowBridge, String> {
        if cfg!(target_os = "macos") {
            // AppKit only allows windows on the main thread, which the TUI owns.
            return Err(
                "OS window is not supported on macOS â€” showing the Run tab Screen view instead"
                    .into(),
            );
        }
        let shared = Arc::new(Mutex::new(WinShared {
            front: vec![0; (width * height) as usize],
            dirty: true,
            keys: VecDeque::new(),
        }));
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();

        {
            let shared = Arc::clone(&shared);
            let alive = Arc::clone(&alive);
            std::thread::spawn(move || {
                let scale = pick_scale(width.max(height));
                let mut win = match Window::new(
                    &format!("Raven â€” {width}x{height}"),
                    width as usize,
                    height as usize,
                    WindowOptions {
                        scale,
                        ..WindowOptions::default()
                    },
                ) {
                    Ok(w) => {
                        let _ = tx.send(Ok(()));
                        w
                    }
                    Err(e) => {
                        let _ = tx.send(Err(format!("cannot open OS window: {e}")));
                        return;
                    }
                };
                let mut local = vec![0u32; (width * height) as usize];
                while alive.load(Ordering::Relaxed) && win.is_open() {
                    let mut redraw = false;
                    {
                        let mut s = shared.lock().unwrap();
                        for key in win.get_keys_pressed(KeyRepeat::Yes) {
                            if let Some(code) = map_key(key) {
                                if s.keys.len() < super::MAX_QUEUED_KEYS {
                                    s.keys.push_back(code);
                                }
                            }
                        }
                        if s.dirty {
                            local.copy_from_slice(&s.front);
                            s.dirty = false;
                            redraw = true;
                        }
                    }
                    if redraw {
                        let _ = win.update_with_buffer(&local, width as usize, height as usize);
                    } else {
                        win.update();
                    }
                    std::thread::sleep(Duration::from_millis(16));
                }
                alive.store(false, Ordering::Relaxed);
            });
        }

        match rx.recv() {
            Ok(Ok(())) => Ok(WindowBridge { shared, alive }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("cannot open OS window: window thread died".into()),
        }
    }

    /// Integer scale so small framebuffers (a 80x60 snake) get a visible window.
    fn pick_scale(max_dim: u32) -> Scale {
        match max_dim {
            0..=96 => Scale::X8,
            97..=192 => Scale::X4,
            193..=384 => Scale::X2,
            _ => Scale::X1,
        }
    }

    fn map_key(key: Key) -> Option<u32> {
        use Key::*;
        Some(match key {
            Up => KEY_UP,
            Down => KEY_DOWN,
            Left => KEY_LEFT,
            Right => KEY_RIGHT,
            Enter => super::KEY_ENTER,
            Backspace => super::KEY_BACKSPACE,
            Escape => super::KEY_ESC,
            Space => b' ' as u32,
            Key0 => b'0' as u32,
            Key1 => b'1' as u32,
            Key2 => b'2' as u32,
            Key3 => b'3' as u32,
            Key4 => b'4' as u32,
            Key5 => b'5' as u32,
            Key6 => b'6' as u32,
            Key7 => b'7' as u32,
            Key8 => b'8' as u32,
            Key9 => b'9' as u32,
            A => b'a' as u32,
            B => b'b' as u32,
            C => b'c' as u32,
            D => b'd' as u32,
            E => b'e' as u32,
            F => b'f' as u32,
            G => b'g' as u32,
            H => b'h' as u32,
            I => b'i' as u32,
            J => b'j' as u32,
            K => b'k' as u32,
            L => b'l' as u32,
            M => b'm' as u32,
            N => b'n' as u32,
            O => b'o' as u32,
            P => b'p' as u32,
            Q => b'q' as u32,
            R => b'r' as u32,
            S => b's' as u32,
            T => b't' as u32,
            U => b'u' as u32,
            V => b'v' as u32,
            W => b'w' as u32,
            X => b'x' as u32,
            Y => b'y' as u32,
            Z => b'z' as u32,
            _ => return None,
        })
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
mod window {
    use std::collections::VecDeque;

    /// Stub: static musl builds have no windowing libs (same gate as `rfd`).
    pub struct WindowBridge;

    impl WindowBridge {
        pub fn present(&self, _front: &[u32]) {}
        pub fn drain_keys(&self, _out: &mut VecDeque<u32>, _cap: usize) {}
        pub fn is_alive(&self) -> bool {
            false
        }
        pub fn shutdown(&self) {}
    }

    pub fn spawn(_width: u32, _height: u32) -> Result<WindowBridge, String> {
        Err("OS window is not available in this build â€” showing the TUI Screen view".into())
    }
}

