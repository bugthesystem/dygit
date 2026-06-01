//! Word-level typo correction: the curated exact-match table.
//!
//! This layer fixes a small, hand-picked set of unambiguous keyboard slips
//! (transpositions, dropped/doubled letters, and a couple of safe shorthands)
//! whose misspelling is essentially never a legitimate token. It runs BEFORE
//! symspell because symspell's frequency ranking occasionally prefers a
//! different real word for these (`promot→promote` rather than `prompt`); the
//! table is the authority for the entries it lists, symspell generalises to the
//! rest.
//!
//! The previous edit-distance-against-a-small-dictionary fallback lived here and
//! was REMOVED: a ~190-word dictionary cannot tell a real word from a typo, so
//! it corrupted ordinary words (`form→for`, `route→router`, `stable→table`).
//! symspell — backed by the 82k-word frequency dictionary — replaces it, and
//! because real words are in that dictionary they are no longer "corrected".
//!
//! Punctuation: callers may hand us tokens with leading/trailing ASCII
//! punctuation (`"teh."`, `"(funtion"`). We peel that off, correct the
//! alphabetic core, and re-attach it, so real prose works without the pipeline
//! having to pre-strip anything.

/// Common typo → intended word. Lowercased keys; matching is case-insensitive
/// and restores the original case shape of the input token.
///
/// Membership rule: an entry must be something that is *almost never* a
/// legitimate token — a transposition (`teh`), a doubled/dropped letter
/// (`lenght`, `recieve`), a metathesis (`waht`), or an unambiguous shorthand
/// whose standalone whitespace-token form is effectively always the expansion
/// (`usr`, `lgs`). Genuinely ambiguous short slips (`th`, `kn`, `ut` — which
/// double as real fragments) are deliberately excluded; symspell handles the
/// general case and we stay silent on the rest.
const COMMON: &[(&str, &str)] = &[
    ("teh", "the"),
    ("taht", "that"),
    ("waht", "what"),
    ("wehn", "when"),
    ("usr", "user"),
    ("lgs", "logs"),
    ("becuase", "because"),
    ("recieve", "receive"),
    ("seperate", "separate"),
    ("isseus", "issues"),
    ("promot", "prompt"),
    ("funtion", "function"),
    ("retrun", "return"),
    ("lenght", "length"),
    ("widht", "width"),
    ("hieght", "height"),
];

/// Returns the table-corrected form of a single token, or the token unchanged.
///
/// This is the *table-only* pass: it consults [`COMMON`] and nothing else. The
/// pipeline asks an injected `Corrector` (symspell daemon, or a no-op fallback)
/// for anything the table leaves untouched.
///
/// Leading/trailing ASCII punctuation is preserved: `"teh."` → `"the."`,
/// `"Teh"` → `"The"`.
#[must_use]
pub fn fix_token(token: &str) -> String {
    let (prefix, core, suffix) = split_punctuation(token);
    if core.is_empty() {
        return token.to_string();
    }
    let fixed = fix_core(core);
    if fixed == core {
        // Nothing changed; avoid a needless reallocation of the same bytes.
        return token.to_string();
    }
    format!("{prefix}{fixed}{suffix}")
}

/// Corrects the bare alphabetic core of a token against the table only.
fn fix_core(core: &str) -> String {
    let lower = core.to_lowercase();
    if let Some((_, fixed)) = COMMON.iter().find(|(k, _)| *k == lower) {
        return restore_case(core, fixed);
    }
    core.to_string()
}

/// Splits `token` into (leading punctuation, alphabetic core, trailing
/// punctuation).
///
/// The core runs from the first to the last alphabetic char, so interior
/// punctuation stays inside the core (we do not try to fix those).
pub fn split_punctuation(token: &str) -> (&str, &str, &str) {
    let is_alpha = |c: char| c.is_alphabetic();
    let Some(start) = token.find(is_alpha) else {
        // No letters at all — treat the whole token as untouched punctuation.
        return (token, "", "");
    };
    // A first alphabetic char exists, so `rfind` is guaranteed `Some`; fall back
    // to `start` only to keep the expression total (the branch is unreachable).
    let last_char = token.rfind(is_alpha).unwrap_or(start);
    // The core ends one char past the last alphabetic char.
    let end = last_char + token[last_char..].chars().next().map_or(0, char::len_utf8);
    (&token[..start], &token[start..end], &token[end..])
}

/// Re-applies the leading-capital shape of `source` to `fixed`.
pub fn restore_case(source: &str, fixed: &str) -> String {
    let leading_upper = source.chars().next().is_some_and(char::is_uppercase);
    if leading_upper {
        let mut chars = fixed.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    } else {
        fixed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_known_typo_from_table() {
        assert_eq!(fix_token("teh"), "the");
        assert_eq!(fix_token("wehn"), "when");
        assert_eq!(fix_token("funtion"), "function");
        // Unambiguous shorthand belongs in the exact table even though it is
        // short — its standalone form is effectively always the expansion.
        assert_eq!(fix_token("usr"), "user");
        assert_eq!(fix_token("lgs"), "logs");
    }

    #[test]
    fn preserves_leading_case() {
        assert_eq!(fix_token("Teh"), "The");
        assert_eq!(fix_token("Becuase"), "Because");
    }

    #[test]
    fn leaves_table_misses_alone() {
        // The table only knows its own entries; everything else passes through
        // untouched here (symspell, not this pass, generalises).
        assert_eq!(fix_token("function"), "function");
        assert_eq!(fix_token("auth"), "auth");
        assert_eq!(fix_token("xyzzy"), "xyzzy");
        // Crucially, real words that the OLD edit-distance pass corrupted now
        // survive the table pass untouched.
        assert_eq!(fix_token("form"), "form");
        assert_eq!(fix_token("route"), "route");
        assert_eq!(fix_token("stable"), "stable");
    }

    #[test]
    fn preserves_trailing_punctuation() {
        assert_eq!(fix_token("teh."), "the.");
        assert_eq!(fix_token("teh,"), "the,");
        assert_eq!(fix_token("(funtion)"), "(function)");
    }

    #[test]
    fn pure_punctuation_is_untouched() {
        assert_eq!(fix_token("..."), "...");
        assert_eq!(fix_token("!?"), "!?");
    }
}
