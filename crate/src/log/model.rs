//! The shape of one logged cleanup.

use serde::{Deserialize, Serialize};

/// The engine's confidence in a cleanup, persisted with each event.
///
/// `Clean` results are never logged (nothing changed), so only `Trivial` and
/// `Interpret` ever reach disk — but the enum carries all three so the engine
/// and the log share one vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// No changes were needed.
    Clean,
    /// Obvious typo/spacing fixes; high confidence.
    Trivial,
    /// Garbled or ambiguous; Claude was asked to interpret in context.
    Interpret,
}

/// One cleanup, appended to `events.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// ISO-8601 UTC timestamp supplied by the caller (the engine is clock-free).
    pub ts: String,
    /// Session id from the hook payload, if present.
    pub session: String,
    /// Working directory from the hook payload.
    pub cwd: String,
    /// What the user typed.
    pub original: String,
    /// What the local pass read it as.
    pub cleaned: String,
    /// Confidence 0.0..=1.0.
    pub score: f32,
    /// Number of tokens changed (drives stats).
    pub edits: u32,
    /// Confidence bucket. Never `Clean` on disk.
    pub verdict: Verdict,
}
