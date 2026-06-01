//! `dygi undo` — prints the most recent ORIGINAL prompt verbatim so the user
//! can re-send it themselves if a cleanup misread them. It cannot un-send
//! Claude's turn; it only hands the raw text back.

use crate::log;

/// Returns the last original prompt, or a friendly note if there is none.
pub fn run() -> String {
    let events = log::read_all().unwrap_or_default();
    match events.last() {
        Some(e) => format!("Your last original prompt:\n{}", e.original),
        None => "Nothing to undo — no cleanups recorded.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::model::{Event, Verdict};
    use crate::test_support::with_temp_dir;

    #[test]
    fn empty_undo_message() {
        with_temp_dir("dygi-test-undo-empty", || {
            assert!(run().contains("Nothing to undo"));
        });
    }

    #[test]
    fn returns_last_original() {
        with_temp_dir("dygi-test-undo-last", || {
            let e = Event {
                ts: "t".into(),
                session: "s".into(),
                cwd: "/tmp".into(),
                original: "teh raw text".into(),
                cleaned: "the raw text".into(),
                score: 0.9,
                edits: 1,
                verdict: Verdict::Trivial,
            };
            log::append(&e).unwrap();
            assert!(run().contains("teh raw text"));
        });
    }
}
