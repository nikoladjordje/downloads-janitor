use std::io::{self, Stdout, stdout};

use crossterm::{
    ExecutableCommand,
    cursor::{Hide, Show},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::Result;

pub type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub struct TerminalSession {
    terminal: AppTerminal,
    restored: bool,
}

impl TerminalSession {
    pub fn start() -> Result<Self> {
        enable_raw_mode().map_err(|error| contextual_error("failed to enable raw mode", error))?;

        let mut output = stdout();
        if let Err(error) = output
            .execute(EnterAlternateScreen)
            .and_then(|output| output.execute(Hide))
        {
            let _ = disable_raw_mode();
            return Err(contextual_error("failed to enter alternate screen", error).into());
        }

        let terminal = match Terminal::new(CrosstermBackend::new(output)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut output = stdout();
                let _ = output.execute(Show);
                let _ = output.execute(LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(contextual_error("failed to initialize terminal", error).into());
            }
        };

        Ok(Self {
            terminal,
            restored: false,
        })
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> io::Result<()> {
        self.terminal.draw(render).map(|_| ())
    }

    pub fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }

        let mut first_error = None;
        record_error(&mut first_error, disable_raw_mode());
        record_error(
            &mut first_error,
            self.terminal.backend_mut().execute(Show).map(|_| ()),
        );
        record_error(
            &mut first_error,
            self.terminal
                .backend_mut()
                .execute(LeaveAlternateScreen)
                .map(|_| ()),
        );
        self.restored = true;

        match first_error {
            Some(error) => Err(contextual_error("failed to restore terminal", error).into()),
            None => Ok(()),
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn record_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

fn contextual_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}
