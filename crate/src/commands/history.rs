//! `dygi history [N]` — renders the last N cleanups for the slash command.

use crate::log::{self, model::Verdict};
use std::fmt::Write;

/// Returns a human-readable history block (newest last), capped at `limit`.
#[must_use]
pub fn run(limit: usize) -> String {
    let events = log::read_all().unwrap_or_default();
    if events.is_empty() {
        return "No cleanups yet — type something messy and I'll catch it.".into();
    }
    let start = events.len().saturating_sub(limit);
    let mut out = String::new();
    for e in &events[start..] {
        let flag = if e.verdict == Verdict::Interpret {
            "⚠ "
        } else {
            ""
        };
        // Writing to a `String` is infallible; ignore the formatter `Result`.
        let _ = write!(
            out,
            "{flag}{ts}\n  {original}\n  → {cleaned}\n",
            ts = e.ts,
            original = e.original,
            cleaned = e.cleaned,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::model::Event;
    use crate::test_support::with_temp_dir;

    fn ev(original: &str, v: Verdict) -> Event {
        Event {
            ts: "2026-05-31T00:00:00Z".into(),
            session: "s".into(),
            cwd: "/tmp".into(),
            original: original.into(),
            cleaned: "clean".into(),
            score: 0.5,
            edits: 1,
            verdict: v,
        }
    }

    #[test]
    fn empty_history_message() {
        with_temp_dir("dygi-test-hist-empty", || {
            assert!(run(10).contains("No cleanups yet"));
        });
    }

    #[test]
    fn flags_interpret_rows() {
        with_temp_dir("dygi-test-hist-flag", || {
            log::append(&ev("a", Verdict::Trivial)).unwrap();
            log::append(&ev("b", Verdict::Interpret)).unwrap();
            let out = run(10);
            assert!(out.contains("⚠"));
        });
    }

    #[test]
    fn respects_limit() {
        with_temp_dir("dygi-test-hist-limit", || {
            for _ in 0..5 {
                log::append(&ev("x", Verdict::Trivial)).unwrap();
            }
            let out = run(2);
            // 2 rows × 3 lines each.
            assert_eq!(out.lines().count(), 6);
        });
    }
}
