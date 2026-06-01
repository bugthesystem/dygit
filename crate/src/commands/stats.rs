//! `dygi stats` — aggregate insight over all logged cleanups.

use crate::log::{self, model::Verdict};
use std::collections::HashMap;

/// Percentage of prompts that needed interpretation, rendered for humans.
///
/// Two properties f32 `.round()` got wrong:
/// * round-*half-up*, not banker's rounding (so 0.5% → 1%, not 0%);
/// * any non-zero rate shows as at least 1% — reporting "0%" while the count is
///   "1" reads as a bug. Integer math throughout keeps it exact.
fn interpret_percent(interpreted: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    // Round half up: (n*100 + total/2) / total, all in integer space.
    let pct = (interpreted * 100 + total / 2) / total;
    if interpreted > 0 && pct == 0 {
        // A real-but-tiny rate must never collapse to 0%.
        1
    } else {
        pct as u32
    }
}

/// Returns a rendered stats block.
pub fn run() -> String {
    let events = log::read_all().unwrap_or_default();
    if events.is_empty() {
        return "No stats yet — nothing has needed cleaning.".into();
    }

    let total = events.len();
    let interpreted = events
        .iter()
        .filter(|e| e.verdict == Verdict::Interpret)
        .count();
    let total_edits: u32 = events.iter().map(|e| e.edits).sum();

    // Most common original tokens that differ from their cleaned form is
    // expensive to compute exactly; approximate "top typos" by counting raw
    // whitespace tokens in originals that are known-short noise.
    let mut token_counts: HashMap<String, usize> = HashMap::new();
    for e in &events {
        for tok in e.original.split_whitespace() {
            *token_counts.entry(tok.to_lowercase()).or_default() += 1;
        }
    }
    let mut top: Vec<(String, usize)> = token_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(5);

    let pct = interpret_percent(interpreted, total);
    let mut out = String::new();
    out.push_str(&format!("Prompts cleaned: {total}\n"));
    out.push_str(&format!("Needed interpretation: {interpreted} ({pct}%)\n"));
    out.push_str(&format!("Total token fixes: {total_edits}\n"));
    out.push_str("Most frequent input tokens:\n");
    for (tok, n) in top {
        out.push_str(&format!("  {tok} ×{n}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::model::Event;
    use crate::test_support::with_temp_dir;

    fn ev(v: Verdict, edits: u32) -> Event {
        Event {
            ts: "2026-05-31T00:00:00Z".into(),
            session: "s".into(),
            cwd: "/tmp".into(),
            original: "teh teh bug".into(),
            cleaned: "the the bug".into(),
            score: 0.5,
            edits,
            verdict: v,
        }
    }

    #[test]
    fn one_in_two_hundred_rounds_to_one_percent() {
        // 1/200 = 0.5%: banker's rounding gave 0%, which reads as a bug. A
        // single interpreted prompt must surface as at least 1%.
        assert_eq!(interpret_percent(1, 200), 1);
    }

    #[test]
    fn tiny_nonzero_rate_never_shows_zero() {
        // Even 1/10000 (0.01%) must show 1%, never 0%, while the count is 1.
        assert_eq!(interpret_percent(1, 10_000), 1);
    }

    #[test]
    fn percent_rounds_half_up_and_handles_extremes() {
        assert_eq!(interpret_percent(0, 5), 0); // genuinely zero
        assert_eq!(interpret_percent(1, 2), 50); // exact half of the whole
        assert_eq!(interpret_percent(3, 8), 38); // 37.5 → 38 (half up)
        assert_eq!(interpret_percent(5, 5), 100); // all
    }

    #[test]
    fn empty_stats_message() {
        with_temp_dir("dygi-test-stats-empty", || {
            assert!(run().contains("No stats yet"));
        });
    }

    #[test]
    fn counts_and_percent() {
        with_temp_dir("dygi-test-stats-count", || {
            log::append(&ev(Verdict::Trivial, 1)).unwrap();
            log::append(&ev(Verdict::Interpret, 2)).unwrap();
            let out = run();
            assert!(out.contains("Prompts cleaned: 2"));
            assert!(out.contains("Needed interpretation: 1 (50%)"));
            assert!(out.contains("Total token fixes: 3"));
            // 'teh' appears twice per event × 2 events = 4, so it tops the list.
            assert!(out.contains("teh ×4"));
        });
    }
}
