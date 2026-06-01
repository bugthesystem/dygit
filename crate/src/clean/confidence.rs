//! Turns "how much did we change?" into a verdict.
//!
//! The score is the fraction of tokens we left *unchanged* — a proxy for how
//! sure we are that the cleaned reading matches intent. Few changes → trivial
//! and safe; many changes → we are guessing, defer to the model to interpret.

use crate::log::model::Verdict;

/// Classifies a cleanup from token counts.
///
/// * `total` — tokens in the original prompt (must be > 0; callers guarantee it).
/// * `changed` — how many tokens the local pass altered.
///
/// Returns the verdict and the confidence score in `0.0..=1.0`.
#[must_use]
pub fn classify(total: usize, changed: usize) -> (Verdict, f32) {
    let total = total.max(1); // defensive; never divide by zero
    let changed = changed.min(total); // a clamp; `changed` can never exceed `total`.
    let unchanged = total - changed;
    // Verdict from integer counts so the boundaries are exact (no float rounding
    // can nudge an input across a threshold):
    // * nothing changed → Clean (score == 1.0);
    // * changed >= 40% of tokens → Interpret. "changed >= 40%" is equivalent to
    //   "unchanged <= 60%", i.e. `unchanged/total <= 3/5`, i.e.
    //   `unchanged * 5 <= total * 3` — the exact 0.6 boundary lands on Interpret;
    // * otherwise → Trivial.
    let verdict = if changed == 0 {
        Verdict::Clean
    } else if unchanged * 5 <= total * 3 {
        Verdict::Interpret
    } else {
        Verdict::Trivial
    };
    // The verdict above is exact (integer math); the score is only a display
    // proxy. These two `usize as f32` casts can in principle lose precision, but
    // the values are prompt token counts (always tiny — far under f32's 24-bit
    // exact-integer range), so the conversion is exact in practice.
    #[allow(
        clippy::cast_precision_loss,
        reason = "token counts are tiny; far below f32's exact-integer range"
    )]
    let score = unchanged as f32 / total as f32;
    (verdict, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes_is_clean() {
        let (v, s) = classify(5, 0);
        assert_eq!(v, Verdict::Clean);
        // Exact equality is intended: with `changed == 0` the score is the exact
        // integer ratio `5/5`, which is representable as `1.0` with no rounding.
        assert_eq!(s, 1.0);
    }

    #[test]
    fn few_changes_is_trivial() {
        let (v, _) = classify(10, 2); // 80% unchanged
        assert_eq!(v, Verdict::Trivial);
    }

    #[test]
    fn many_changes_is_interpret() {
        let (v, _) = classify(10, 7); // 30% unchanged
        assert_eq!(v, Verdict::Interpret);
    }

    #[test]
    fn exactly_forty_percent_changed_is_interpret() {
        // 4 of 10 changed → score exactly 0.6, the boundary. "changed >= 40%"
        // must land on Interpret, not Trivial.
        let (v, s) = classify(10, 4);
        // Exact equality is intended: `6/10` rounds to the same `f32` as the
        // literal `0.6`, so this pins the score, not just the verdict.
        assert_eq!(s, 0.6);
        assert_eq!(v, Verdict::Interpret);
    }

    #[test]
    fn just_under_forty_percent_changed_is_trivial() {
        // 3 of 10 changed → 30% changed, below the boundary → Trivial.
        let (v, _) = classify(10, 3);
        assert_eq!(v, Verdict::Trivial);
    }
}
