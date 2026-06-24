//! Terminal setup and teardown for the TUI.

use std::io::{self, Stdout, Write};

use base64ct::{Base64, Encoding};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Copies `text` to the system clipboard using the OSC 52 terminal escape sequence.
///
/// OSC 52 asks the terminal emulator itself to set the clipboard, which works both locally
/// and over SSH (unlike OS-level clipboard APIs), provided the terminal supports it (most
/// modern terminals do). Returns an error if writing to the terminal fails; it cannot
/// detect whether the terminal actually honoured the request.
pub(crate) fn copy_to_clipboard(text: &str) -> io::Result<()> {
    let encoded = Base64::encode_string(text.as_bytes());
    let mut stdout = io::stdout();
    // `\x1b]52;c;<base64>\x07` — selection `c` is the clipboard.
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()
}

/// Alias for the concrete terminal type used by the TUI.
pub(crate) type Tui = Terminal<CrosstermBackend<Stdout>>;

/// An RAII guard that places the terminal into raw mode and the alternate screen on
/// construction, and restores it on drop.
///
/// Restoring on drop ensures the user's terminal is not left in a broken state if the UI
/// panics. [`TerminalGuard::restore`] can be called explicitly to restore early (e.g. so
/// that an error message is printed to a normal terminal); it is idempotent.
pub(crate) struct TerminalGuard {
    terminal: Option<Tui>,
    restored: bool,
}

impl TerminalGuard {
    /// Enters raw mode and the alternate screen, returning a guard that owns the terminal.
    ///
    /// If setup fails partway through (e.g. entering the alternate screen or constructing
    /// the terminal fails after raw mode was enabled), any state already changed is rolled
    /// back before returning the error, so the user's terminal is not left in raw mode.
    pub(crate) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;

        // From here on, undo raw mode (and the alternate screen) if anything fails, since
        // no `Drop` guard exists yet to restore it.
        let setup = (|| {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(stdout);
            Terminal::new(backend)
        })();

        match setup {
            Ok(terminal) => Ok(Self {
                terminal: Some(terminal),
                restored: false,
            }),
            Err(e) => {
                // Best-effort rollback; preserve and return the original error.
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                Err(e)
            }
        }
    }

    /// Returns a mutable reference to the underlying terminal.
    pub(crate) fn terminal_mut(&mut self) -> &mut Tui {
        self.terminal
            .as_mut()
            .expect("terminal is present until the guard is dropped")
    }

    /// Restores the terminal to its original state. Idempotent.
    pub(crate) fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;

        // Best-effort restoration; nothing useful can be done if these fail.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        if let Some(terminal) = self.terminal.as_mut() {
            let _ = terminal.show_cursor();
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
