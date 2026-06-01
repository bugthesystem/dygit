//! `dygi hook` — runs on every `UserPromptSubmit`.
//!
//! Contract with Claude Code (verified): we read a JSON payload on stdin, and
//! the ONLY way to influence the model is to print JSON whose
//! `hookSpecificOutput.additionalContext` is injected as context. We can NOT
//! rewrite the prompt or draw terminal UI, so the visible "✓ understood" line
//! is produced by *the model*, instructed via that context. On any error we
//! print nothing and the caller exits 0 — a broken hook must be invisible.

use crate::clean::corrector::{Corrector, DaemonCorrector, TableOnly};
use crate::clean::{self, Outcome};
use crate::config::{Config, Verbosity};
use crate::error::DygiError;
use crate::log::model::{Event, Verdict};
use serde::Deserialize;

/// The subset of the `UserPromptSubmit` stdin payload we read.
#[derive(Debug, Deserialize)]
pub struct HookInput {
    /// The user's submitted prompt text.
    pub prompt: String,
    /// Session id (optional; defaults to empty).
    #[serde(default)]
    pub session_id: String,
    /// Working directory (optional; defaults to empty).
    #[serde(default)]
    pub cwd: String,
}

/// Selects the corrector for this prompt and background-spawns the daemon when
/// it is missing — the I/O-bearing companion to [`run`].
///
/// * If the daemon answers a probe connect → use [`DaemonCorrector`] (full
///   symspell quality).
/// * Otherwise → use [`TableOnly`] (instant, offline cold-start fallback) AND
///   spawn the daemon detached so the *next* prompt is warm. Spawning never
///   blocks: we fire-and-forget and ignore any failure.
///
/// Returned boxed so callers hold one type regardless of which arm fired.
#[must_use]
pub fn build_corrector() -> Box<dyn Corrector> {
    if let Some(daemon) = DaemonCorrector::connect() {
        return Box::new(daemon);
    }
    crate::daemon::spawn_detached();
    Box::new(TableOnly)
}

/// Runs the hook against an already-parsed input, a timestamp, and a corrector,
/// returning the exact string to print to stdout (empty string == print
/// nothing).
///
/// The corrector is injected so this is fully unit-testable with a fake: no
/// stdin, no clock, no socket, no files except the best-effort log append
/// (ignored on failure).
pub fn run(input: &HookInput, now_iso: &str, corrector: &dyn Corrector) -> String {
    let config = Config::load();
    if !config.enabled {
        return String::new();
    }

    let outcome = clean::clean(&input.prompt, config.aggressiveness, corrector);

    // Nothing changed → stay silent.
    if outcome.verdict == Verdict::Clean {
        return String::new();
    }

    // Best-effort log; never let a logging failure affect the hook.
    let _ = log_event(input, &outcome, now_iso);

    // Decide whether to ask the model to confirm aloud. Verbose confirms every
    // cleanup; quiet confirms only low-confidence (`Interpret`) ones.
    let confirm = match (config.verbosity, outcome.verdict) {
        (Verbosity::Verbose, _) | (Verbosity::Quiet, Verdict::Interpret) => true,
        (Verbosity::Quiet, _) => false,
    };

    additional_context(&outcome, confirm)
}

/// Builds the `additionalContext` JSON string for the model.
///
/// Contract: only ever called for `Trivial` or `Interpret`. `run` early-returns
/// on `Clean` (nothing changed → stay silent), so the `Clean` case never reaches
/// here; the quiet-`Trivial` branch is the catch-all, which makes the match
/// exhaustive without a dead arm or a reachable panic.
fn additional_context(outcome: &Outcome, confirm: bool) -> String {
    let instruction = match outcome.verdict {
        Verdict::Interpret => format!(
            "The user's prompt is garbled and may be ambiguous. Best local \
             reading: \"{cleaned}\". Confidence is low. Interpret it using \
             conversation context, open your reply with \"✓ understood · <your \
             reading>\", and if genuinely ambiguous ask one brief clarifying \
             question instead of guessing.",
            cleaned = outcome.cleaned
        ),
        Verdict::Trivial if confirm => format!(
            "The user's prompt contained typos. Most-likely intended reading: \
             \"{cleaned}\". Begin your reply with exactly one line — \
             \"✓ understood · {cleaned}\" — then carry out that request.",
            cleaned = outcome.cleaned
        ),
        // Trivial + quiet (and, by contract, never Clean): nudge the reading
        // without demanding an echo.
        _ => format!(
            "The user's prompt contained typos. Most-likely intended reading: \
             \"{cleaned}\". Proceed with that request.",
            cleaned = outcome.cleaned
        ),
    };

    // Compose the required wrapper shape. serde guarantees correct escaping.
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": instruction,
        }
    });
    payload.to_string()
}

/// Appends the event to the log. Returns `Err` for the caller to ignore.
fn log_event(input: &HookInput, outcome: &Outcome, now_iso: &str) -> Result<(), DygiError> {
    let event = Event {
        ts: now_iso.to_string(),
        session: input.session_id.clone(),
        cwd: input.cwd.clone(),
        original: input.prompt.clone(),
        cleaned: outcome.cleaned.clone(),
        score: outcome.score,
        edits: outcome.edits,
        verdict: outcome.verdict,
    };
    crate::log::append(&event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(prompt: &str) -> HookInput {
        HookInput {
            prompt: prompt.into(),
            session_id: "s".into(),
            cwd: "/tmp".into(),
        }
    }

    use crate::test_support::with_temp_dir;

    /// Cold-start corrector: pass tokens through, so tests exercise the
    /// table-only behaviour deterministically without a daemon.
    fn no_corrector() -> TableOnly {
        TableOnly
    }

    #[test]
    fn clean_prompt_emits_nothing() {
        with_temp_dir("dygi-test-hook-clean", || {
            let out = run(
                &input("fix the auth bug"),
                "2026-05-31T00:00:00Z",
                &no_corrector(),
            );
            assert!(out.is_empty());
        });
    }

    #[test]
    fn messy_prompt_emits_additional_context() {
        with_temp_dir("dygi-test-hook-messy", || {
            let out = run(
                &input("fix teh aut hbug wehn usr lgs ut"),
                "2026-05-31T00:00:00Z",
                &no_corrector(),
            );
            assert!(out.contains("additionalContext"));
            // Table fixes `teh`/`wehn`/`usr`/`lgs`, segmentation re-cuts
            // `aut hbug`; only the ambiguous `ut` survives.
            assert!(out.contains("fix the auth bug when user logs ut"));
            assert!(out.contains("✓ understood"));
        });
    }

    #[test]
    fn logs_the_event() {
        with_temp_dir("dygi-test-hook-log", || {
            run(&input("teh bug"), "2026-05-31T00:00:00Z", &no_corrector());
            let events = crate::log::read_all().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].original, "teh bug");
        });
    }

    #[test]
    fn disabled_config_is_silent() {
        with_temp_dir("dygi-test-hook-disabled", || {
            let c = Config {
                enabled: false,
                ..Config::default()
            };
            c.save().unwrap();
            let out = run(&input("teh bug"), "2026-05-31T00:00:00Z", &no_corrector());
            assert!(out.is_empty());
        });
    }
}
