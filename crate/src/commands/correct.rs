//! `dygi correct` — a side-effect-free query that returns a structured cleanup.
//!
//! Unlike [`super::hook`], `correct` is not the live Claude Code prompt path: it
//! does not consult the user config, does not gate on enabled/verbosity, and
//! never writes the event log. It exists for *other* editors (e.g. opencode) that
//! cannot inject `additionalContext` and instead want a machine-readable answer
//! they can act on themselves. It reuses the exact same corrector selection as
//! the hook (daemon if live, else table-only + a detached daemon spawn), so the
//! correction quality is identical.
//!
//! Output is a single line of JSON:
//! `{"original":..,"cleaned":..,"verdict":"clean|trivial|interpret","changed":bool}`
//!
//! Like the hook, `correct` is fail-invisible: on any error the public [`run`]
//! prints nothing and the caller exits 0.

use crate::clean::corrector::Corrector;
use crate::clean::{self};
use crate::commands::hook;
use crate::config::Aggressiveness;
use crate::log::model::Verdict;
use serde::Serialize;

/// The structured result emitted on stdout, one JSON object per line.
///
/// `changed` is a convenience for thin clients: it is simply `verdict != Clean`,
/// so a caller can branch on a single boolean without re-deriving it from the
/// verdict string.
#[derive(Debug, Serialize)]
struct CorrectOutput<'a> {
    /// The prompt exactly as received on stdin (trailing newline trimmed).
    original: &'a str,
    /// The cleaned reading. Equals `original` when nothing changed.
    cleaned: String,
    /// Confidence bucket, serialized lowercase by [`Verdict`]'s serde rename.
    verdict: Verdict,
    /// `true` iff the verdict is not `Clean` (i.e. the engine changed something).
    changed: bool,
}

/// Reads the prompt from `stdin`, builds the production corrector, and prints the
/// JSON line. Always returns/contracts to exit 0; never panics, never logs.
///
/// On any failure (unreadable stdin) we print nothing — the same fail-invisible
/// contract as the hook. Building the corrector may spawn the daemon detached
/// (via [`hook::build_corrector`]); that is the only side effect, mirroring the
/// hook so the next prompt is warm.
pub fn run() {
    use std::io::Read as _;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return;
    }
    let prompt = extract_prompt(&buf);
    let corrector = hook::build_corrector();
    print!("{}", run_with(&prompt, corrector.as_ref()));
}

/// The pure core: cleans `prompt` with the injected `corrector` and returns the
/// JSON line (no trailing newline). Factored out of [`run`] so it can be unit
/// tested with a fake corrector — no daemon, no socket, no stdin.
#[must_use]
pub fn run_with(prompt: &str, corrector: &dyn Corrector) -> String {
    let outcome = clean::clean(prompt, Aggressiveness::Gentle, corrector);
    let out = CorrectOutput {
        original: prompt,
        cleaned: outcome.cleaned,
        verdict: outcome.verdict,
        changed: outcome.verdict != Verdict::Clean,
    };
    // serde cannot fail to serialize this fixed shape; fall back to an empty
    // string rather than ever panicking, preserving the fail-invisible contract.
    serde_json::to_string(&out).unwrap_or_default()
}

/// Pulls the prompt text out of stdin.
///
/// Accepts EITHER a JSON object carrying a `prompt` field (so a caller can hand
/// us the same payload shape the hook reads) OR raw text. We try JSON first; if
/// it is not an object with a string `prompt`, we treat the entire stdin as the
/// raw prompt. A single trailing newline is trimmed either way (raw text piped
/// in usually carries one).
fn extract_prompt(buf: &str) -> String {
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(buf) {
        if let Some(serde_json::Value::String(p)) = map.get("prompt") {
            return p.clone();
        }
    }
    buf.strip_suffix('\n').unwrap_or(buf).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fake corrector backed by a fixed map — mirrors the pattern in
    /// `clean::tests` so `run_with` is testable with no daemon or socket. Tokens
    /// not in the map pass through unchanged.
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

    /// Parses the JSON line for assertions without pulling typed structs into
    /// the test (the output type is private and borrows `original`).
    fn parse(line: &str) -> serde_json::Value {
        serde_json::from_str(line).expect("run_with must emit valid JSON")
    }

    #[test]
    fn typo_prompt_is_corrected_and_flagged_changed() {
        // `teh` is a curated-table word; the table fixes it with no corrector.
        let out = run_with("fix teh bug", &Fake::new(&[]));
        let v = parse(&out);
        assert_eq!(v["original"], "fix teh bug");
        assert_eq!(v["cleaned"], "fix the bug");
        assert_ne!(v["cleaned"], v["original"]);
        // A clear table fix is trivial; either way it must not be `clean`.
        assert_eq!(v["verdict"], "trivial");
        assert_eq!(v["changed"], true);
    }

    #[test]
    fn corrector_miss_is_corrected_via_fake() {
        // `funtcion` is not in the table; the injected corrector supplies it.
        let out = run_with("fix funtcion now", &Fake::new(&[("funtcion", "function")]));
        let v = parse(&out);
        assert_eq!(v["cleaned"], "fix function now");
        assert_eq!(v["changed"], true);
        assert_ne!(v["verdict"], "clean");
    }

    #[test]
    fn clean_prompt_is_unchanged_and_not_flagged() {
        let out = run_with("fix the auth bug", &Fake::new(&[]));
        let v = parse(&out);
        assert_eq!(v["cleaned"], "fix the auth bug");
        assert_eq!(v["cleaned"], v["original"]);
        assert_eq!(v["verdict"], "clean");
        assert_eq!(v["changed"], false);
    }

    #[test]
    fn extract_prompt_reads_json_prompt_field() {
        assert_eq!(
            extract_prompt(r#"{"prompt":"fix teh bug","cwd":"/x"}"#),
            "fix teh bug"
        );
    }

    #[test]
    fn extract_prompt_falls_back_to_raw_text_and_trims_newline() {
        assert_eq!(extract_prompt("fix teh bug\n"), "fix teh bug");
        // Not a JSON object with a prompt field → whole stdin is the prompt.
        assert_eq!(extract_prompt("fix teh bug"), "fix teh bug");
        assert_eq!(extract_prompt(r#"{"other":1}"#), r#"{"other":1}"#);
    }
}
