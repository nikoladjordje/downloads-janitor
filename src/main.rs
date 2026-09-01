mod app;
mod inbox;
mod terminal;
mod ui;

use std::error::Error;

use app::App;
use terminal::TerminalSession;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let entries = inbox::scan_downloads()?;
    let mut terminal = TerminalSession::start()?;
    let application_result = App::new(entries).run(&mut terminal);
    let restoration_result = terminal.restore();

    match (application_result, restoration_result) {
        (Err(application_error), Err(restoration_error)) => Err(format!(
            "{application_error}; terminal restoration also failed: {restoration_error}"
        )
        .into()),
        (Err(application_error), Ok(())) => Err(application_error),
        (Ok(()), Err(restoration_error)) => Err(restoration_error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
