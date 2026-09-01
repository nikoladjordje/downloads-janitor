use std::io::{self, Stdout, Write, stdout};

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
        if let Err(error) = output.execute(EnterAlternateScreen) {
            return Err(initialization_error(
                "failed to enter alternate screen",
                error,
                restore_output(&mut output),
            ));
        }
        if let Err(error) = output.execute(Hide) {
            return Err(initialization_error(
                "failed to hide terminal cursor",
                error,
                restore_output(&mut output),
            ));
        }

        let terminal = match Terminal::new(CrosstermBackend::new(output)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut output = stdout();
                return Err(initialization_error(
                    "failed to initialize terminal",
                    error,
                    restore_output(&mut output),
                ));
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

        let restoration_result = restore_output(self.terminal.backend_mut());
        self.restored = true;

        restoration_result
            .map_err(|error| contextual_error("failed to restore terminal", error).into())
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

fn restore_output(output: &mut impl Write) -> io::Result<()> {
    let mut first_error = None;
    record_error(&mut first_error, output.execute(Show).map(|_| ()));
    record_error(
        &mut first_error,
        output.execute(LeaveAlternateScreen).map(|_| ()),
    );
    record_error(&mut first_error, disable_raw_mode());

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn initialization_error(
    context: &'static str,
    source: io::Error,
    restoration_result: io::Result<()>,
) -> Box<dyn std::error::Error> {
    let primary_error = contextual_error(context, source);
    match restoration_result {
        Ok(()) => primary_error.into(),
        Err(restoration_error) => {
            format!("{primary_error}; terminal restoration also failed: {restoration_error}").into()
        }
    }
}

fn contextual_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}
