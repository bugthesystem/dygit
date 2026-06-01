//! Spacing/merge repair, e.g. `aut hbug` → `auth bug`.
//!
//! The pattern this targets is a *misplaced space*: the user typed the right
//! letters but the gap landed one slot early or late. We only re-cut a pair of
//! adjacent tokens when BOTH resulting words are in the known-word set — that
//! keeps us from inventing splits that merely look plausible.

/// Words we are confident about, used to validate a re-split. Intentionally
/// small and domain-flavoured; growing it is cheap and safe.
///
/// MUST stay sorted: lookups use [`slice::binary_search`], which keeps this hot
/// path allocation-free (no per-call `HashSet` rebuild). A `debug_assert` in the
/// tests guards the ordering invariant.
const KNOWN: &[&str] = &[
    "auth", "because", "bug", "comes", "fix", "issues", "keyboard", "logs", "lots", "out",
    "plugin", "prompt", "the", "user", "well", "when",
];

/// Whether `word` is in the confident-words set. O(log n), zero allocation.
fn is_known(word: &str) -> bool {
    KNOWN.binary_search(&word).is_ok()
}

/// Given two adjacent tokens, returns `Some((left, right))` re-cut at the
/// boundary that makes both halves known words, or `None` to leave them as-is.
///
/// Example: `("aut", "hbug")` → `("auth", "bug")` because moving the space one
/// char right yields two known words.
#[must_use]
pub fn resplit_pair(a: &str, b: &str) -> Option<(String, String)> {
    let joined: Vec<char> = a.chars().chain(b.chars()).collect();
    // Try every internal cut point; accept the first that yields two known words.
    for cut in 1..joined.len() {
        let left: String = joined[..cut].iter().collect();
        let right: String = joined[cut..].iter().collect();
        if is_known(&left) && is_known(&right) {
            // Only report a *change*; identical re-cut is not interesting.
            if left != a || right != b {
                return Some((left, right));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recuts_misplaced_space() {
        assert_eq!(
            resplit_pair("aut", "hbug"),
            Some(("auth".into(), "bug".into()))
        );
    }

    #[test]
    fn leaves_good_pairs_alone() {
        assert_eq!(resplit_pair("auth", "bug"), None);
    }

    #[test]
    fn declines_when_no_known_split() {
        assert_eq!(resplit_pair("xq", "zzt"), None);
    }

    #[test]
    fn known_list_is_sorted_for_binary_search() {
        // `is_known` relies on `binary_search`, which is only correct if the
        // backing slice is sorted. Guard the invariant so future edits can't
        // silently break lookups.
        let mut sorted = KNOWN.to_vec();
        sorted.sort_unstable();
        assert_eq!(KNOWN, sorted.as_slice());
    }
}
