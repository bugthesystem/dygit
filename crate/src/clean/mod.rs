//! Prompt-cleaning engine: ties typos, segmentation, and confidence together.
//!
//! Pipeline per prompt: tokenize → re-split misplaced spaces → fix per-token
//! typos (curated table first, then an injected [`Corrector`] for the rest) →
//! recombine → classify by how much changed. The engine is pure (no clock, no
//! I/O of its own) so it is trivially testable; the corrector is injected so the
//! pipeline can be tested with a fake (no socket, no dictionary). Callers stamp
//! the timestamp.

pub mod confidence;
pub mod corrector;
pub mod segment;
pub mod typos;

use crate::config::Aggressiveness;
use crate::log::model::Verdict;
use corrector::Corrector;

/// The result of cleaning one prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The cleaned reading. Equals the input when nothing changed.
    pub cleaned: String,
    /// Confidence bucket.
    pub verdict: Verdict,
    /// Confidence score 0.0..=1.0.
    pub score: f32,
    /// Tokens changed.
    pub edits: u32,
}

/// Cleans `prompt` using `corrector` for generalisation beyond the curated
/// table. `aggressiveness` is accepted for forward-compatibility; the gentle
/// path (table + validated re-splits + corrector) is already conservative, so
/// today both settings behave identically.
///
/// Per token the order is: (1) curated table via [`typos::fix_token`]; (2) if
/// the table left it unchanged, ask `corrector`. The corrector is responsible
/// for never blocking and never guessing — it returns the token unchanged when
/// unsure (see [`corrector::Corrector`]).
pub fn clean(prompt: &str, _aggressiveness: Aggressiveness, corrector: &dyn Corrector) -> Outcome {
    // 1. Split into tokens; an empty prompt is trivially clean.
    let tokens: Vec<&str> = prompt.split_whitespace().collect();
    if tokens.is_empty() {
        return Outcome {
            cleaned: prompt.to_string(),
            verdict: Verdict::Clean,
            score: 1.0,
            edits: 0,
        };
    }

    // 2. Re-split misplaced spaces by scanning adjacent pairs left to right.
    let mut words: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if i + 1 < tokens.len() {
            if let Some((l, r)) = segment::resplit_pair(tokens[i], tokens[i + 1]) {
                words.push(l);
                words.push(r);
                i += 2;
                continue;
            }
        }
        words.push(tokens[i].to_string());
        i += 1;
    }

    // 3. Fix per-token typos: curated table first, corrector for the remainder.
    let fixed: Vec<String> = words.iter().map(|w| fix_token(w, corrector)).collect();

    // 4. Measure change against the original token stream and classify.
    let original_join = tokens.join(" ");
    let cleaned_join = fixed.join(" ");
    let changed = count_changed(&tokens, &fixed);
    let (verdict, score) = confidence::classify(tokens.len().max(fixed.len()), changed);

    // If recombination is byte-identical, force Clean regardless of token math.
    let verdict = if original_join == cleaned_join {
        Verdict::Clean
    } else {
        verdict
    };

    Outcome {
        cleaned: cleaned_join,
        verdict,
        score,
        edits: changed as u32,
    }
}

/// Corrects one word: the curated table is authoritative; only when it leaves
/// the word unchanged do we defer to the corrector. Punctuation is preserved by
/// peeling it off, correcting the core, and re-attaching — mirroring the table's
/// own punctuation handling so the corrector sees a bare word.
fn fix_token(word: &str, corrector: &dyn Corrector) -> String {
    let table = typos::fix_token(word);
    if table != *word {
        return table; // table fired; it wins.
    }
    let (prefix, core, suffix) = typos::split_punctuation(word);
    if core.is_empty() {
        return word.to_string();
    }
    let suggestion = corrector.correct(&core.to_lowercase());
    if suggestion.is_empty() || suggestion == core.to_lowercase() {
        return word.to_string();
    }
    format!("{prefix}{}{suffix}", typos::restore_case(core, &suggestion))
}

/// Counts how many positions differ between original and cleaned token streams.
/// Length changes (from re-splits) count every surplus token as a change.
fn count_changed(original: &[&str], cleaned: &[String]) -> usize {
    let common = original.len().min(cleaned.len());
    let mut changed = original.len().abs_diff(cleaned.len());
    for k in 0..common {
        if original[k] != cleaned[k] {
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fake corrector backed by a fixed map — the pipeline's stand-in for the
    /// symspell daemon in unit tests. Tokens not in the map pass through.
    struct Fake(HashMap<String, String>);
    impl Fake {
        fn new(pairs: &[(&str, &str)]) -> Self {
            Fake(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            )
        }
    }
    impl Corrector for Fake {
        fn correct(&self, token: &str) -> String {
            self.0
                .get(token)
                .cloned()
                .unwrap_or_else(|| token.to_string())
        }
    }

    /// A corrector that panics if asked about a *table-handled* token, proving
    /// the table short-circuits before the corrector for words it owns. Other
    /// tokens pass through, since the corrector is legitimately consulted for
    /// anything the table leaves unchanged.
    struct RejectTableWords;
    impl Corrector for RejectTableWords {
        fn correct(&self, token: &str) -> String {
            assert_ne!(token, "teh", "table-handled token reached the corrector");
            token.to_string()
        }
    }

    /// A corrector that always errors out by returning the token unchanged,
    /// modelling a daemon failure — the pipeline must not break.
    struct AlwaysUnchanged;
    impl Corrector for AlwaysUnchanged {
        fn correct(&self, token: &str) -> String {
            token.to_string()
        }
    }

    #[test]
    fn clean_prompt_is_untouched() {
        let out = clean("fix the auth bug", Aggressiveness::Gentle, &AlwaysUnchanged);
        assert_eq!(out.cleaned, "fix the auth bug");
        assert_eq!(out.verdict, Verdict::Clean);
        assert_eq!(out.edits, 0);
    }

    #[test]
    fn table_runs_before_corrector() {
        // `teh` is a table entry; the corrector must never be asked about it
        // (RejectTableWords asserts that). `bug` is not a table word, so the
        // corrector is legitimately consulted and passes it through.
        let out = clean("teh bug", Aggressiveness::Gentle, &RejectTableWords);
        assert_eq!(out.cleaned, "the bug");
    }

    #[test]
    fn corrector_handles_table_misses() {
        // `funtcion` is NOT in the table; the corrector supplies the fix.
        let fake = Fake::new(&[("funtcion", "function")]);
        let out = clean("fix funtcion now", Aggressiveness::Gentle, &fake);
        assert_eq!(out.cleaned, "fix function now");
        assert_ne!(out.verdict, Verdict::Clean);
    }

    #[test]
    fn corrector_preserves_case_and_punctuation() {
        let fake = Fake::new(&[("funtcion", "function")]);
        let out = clean("Funtcion.", Aggressiveness::Gentle, &fake);
        assert_eq!(out.cleaned, "Function.");
    }

    #[test]
    fn corrector_failure_never_blocks_or_corrupts() {
        // The corrector returns everything unchanged (modelling a dead daemon);
        // the prompt flows through with only the table's edits.
        let out = clean(
            "fix teh form route bug",
            Aggressiveness::Gentle,
            &AlwaysUnchanged,
        );
        // `teh` → `the` (table); `form`/`route` survive (corrector unchanged).
        assert_eq!(out.cleaned, "fix the form route bug");
    }

    #[test]
    fn fixes_typos_and_spacing() {
        // Table fixes `teh`/`wehn`/`usr`/`lgs`; segmentation re-cuts `aut hbug`.
        // `ut` is a genuinely ambiguous 2-char token; the corrector returns it
        // unchanged, so the engine stays silent on it.
        let out = clean(
            "fix teh aut hbug wehn usr lgs ut",
            Aggressiveness::Gentle,
            &AlwaysUnchanged,
        );
        assert_eq!(out.cleaned, "fix the auth bug when user logs ut");
        assert_ne!(out.verdict, Verdict::Clean);
    }

    #[test]
    fn empty_prompt_is_clean() {
        let out = clean("   ", Aggressiveness::Gentle, &AlwaysUnchanged);
        assert_eq!(out.verdict, Verdict::Clean);
    }

    #[test]
    fn corrects_token_with_trailing_punctuation() {
        let out = clean("fix teh. bug", Aggressiveness::Gentle, &AlwaysUnchanged);
        assert_eq!(out.cleaned, "fix the. bug");
        assert_ne!(out.verdict, Verdict::Clean);
    }
}
