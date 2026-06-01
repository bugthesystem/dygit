//! Turns "how much did we change?" into a verdict.
//!
//! The score is the fraction of tokens we left *unchanged* — a proxy for how
//! sure we are that the cleaned reading matches intent. Few changes → trivial
//! and safe; many changes → we are guessing, hand it to Claude to interpret.

use crate::log::model::Verdict;

/// Thresholds, named so the policy reads in one glance.
const CLEAN_SCORE: f32 = 1.0; // nothing changed
const INTERPRET_AT_OR_BELOW: f32 = 0.6; // changed >= 40% of tokens → low confidence

/// Classifies a cleanup from token counts.
///
/// * `total` — tokens in the original prompt (must be > 0; callers guarantee it).
/// * `changed` — how many tokens the local pass altered.
///
/// Returns the verdict and the confidence score in `0.0..=1.0`.
pub fn classify(total: usize, changed: usize) -> (Verdict, f32) {
    let total = total.max(1); // defensive; never divide by zero
    let score = (total - changed) as f32 / total as f32;
    // Boundary rule: "changed >= 40% of tokens → Interpret". At exactly 40%
    // changed the score is 0.6, which must classify as Interpret — hence
    // `<=`. The small epsilon absorbs float-division rounding so the boundary
    // stays inclusive regardless of how `score` was computed.
    let verdict = if (score - CLEAN_SCORE).abs() < f32::EPSILON {
        Verdict::Clean
    } else if score <= INTERPRET_AT_OR_BELOW + f32::EPSILON {
        Verdict::Interpret
    } else {
        Verdict::Trivial
    };
    (verdict, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes_is_clean() {
        let (v, s) = classify(5, 0);
        assert_eq!(v, Verdict::Clean);
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
