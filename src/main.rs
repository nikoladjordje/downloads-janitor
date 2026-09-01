mod app;
mod inbox;
mod terminal;
mod ui;

use std::error::Error;

use app::App;
use terminal::TerminalSession;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let mut terminal = TerminalSession::start()?;
    let application_result =
        inbox::scan_downloads().and_then(|entries| App::new(entries).run(&mut terminal));
    let restoration_result = terminal.restore();

    finish(application_result, restoration_result)
}

fn finish(application_result: Result<()>, restoration_result: Result<()>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::{Result, finish};

    fn failure(message: &'static str) -> Result<()> {
        Err(io::Error::other(message).into())
    }

    #[test]
    fn application_error_remains_primary_when_restoration_also_fails() {
        let error = finish(failure("scan failed"), failure("cleanup failed"))
            .expect_err("combined failures should be returned");

        assert_eq!(
            error.to_string(),
            "scan failed; terminal restoration also failed: cleanup failed"
        );
    }

    #[test]
    fn restoration_error_is_returned_when_it_is_the_only_failure() {
        let error = finish(Ok(()), failure("cleanup failed"))
            .expect_err("restoration failure should be returned");

        assert_eq!(error.to_string(), "cleanup failed");
    }
}
