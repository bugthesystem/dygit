//! Append-only event log.
//!
//! Append-only is deliberate: the hot path only ever *adds* a line, so there is
//! no read-modify-write to corrupt under a crash, and writes stay O(1).

pub mod model;

use crate::error::DygiError;
use crate::platform::events_path;
use model::Event;
use std::io::Write;

/// Appends one event as a single JSON line. Best-effort by contract: callers on
/// the hot path ignore the `Err`, but we still return it so commands can report.
pub fn append(event: &Event) -> Result<(), DygiError> {
    let path = events_path()?;
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&line)?;
    Ok(())
}

/// Reads every event, oldest first. Malformed lines are skipped, not fatal —
/// one bad line must never hide the rest of the user's history.
pub fn read_all() -> Result<Vec<Event>, DygiError> {
    let path = events_path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new()); // no log yet == empty history
    };
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str::<Event>(l).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::model::Verdict;
    use super::*;

    fn sample(original: &str) -> Event {
        Event {
            ts: "2026-05-31T00:00:00Z".into(),
            session: "s1".into(),
            cwd: "/tmp".into(),
            original: original.into(),
            cleaned: "the auth bug".into(),
            score: 0.9,
            edits: 2,
            verdict: Verdict::Trivial,
        }
    }

    use crate::test_support::with_temp_dir;

    #[test]
    fn append_then_read_roundtrips() {
        with_temp_dir("dygi-test-log", || {
            append(&sample("teh aut hbug")).unwrap();
            append(&sample("aut hbug 2")).unwrap();
            let all = read_all().unwrap();
            assert_eq!(all.len(), 2);
            assert_eq!(all[0].original, "teh aut hbug");
        });
    }

    #[test]
    fn read_skips_malformed_lines() {
        with_temp_dir("dygi-test-log-bad", || {
            append(&sample("good")).unwrap();
            // Inject a broken line between good ones.
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(events_path().unwrap())
                .unwrap();
            f.write_all(b"{ not json }\n").unwrap();
            append(&sample("good2")).unwrap();
            let all = read_all().unwrap();
            assert_eq!(all.len(), 2);
        });
    }

    #[test]
    fn missing_log_reads_empty() {
        with_temp_dir("dygi-test-log-empty", || {
            assert!(read_all().unwrap().is_empty());
        });
    }
}
