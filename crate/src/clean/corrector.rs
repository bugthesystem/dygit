//! The corrector abstraction the cleaning pipeline asks for generalisation.
//!
//! The curated table in [`super::typos`] handles a handful of domain typos
//! exactly; everything beyond it is delegated to a `Corrector`. Making this a
//! trait keeps [`super::clean`] pure and testable: unit tests inject a fake
//! corrector (a closure/map), so no socket or dictionary is needed to test the
//! pipeline's logic. In production the corrector is the resident symspell
//! [`DaemonCorrector`]; when the daemon is not yet up, [`TableOnly`] is the
//! cold-start fallback that adds nothing (the table already ran).

use crate::platform;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// How long to wait when connecting to the daemon socket. The daemon is local
/// and answers in microseconds, so any real delay means it is not there — fail
/// fast and fall back rather than risk blocking the prompt.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(50);

/// Generalises a single token beyond the curated table.
///
/// Contract: `correct` is only asked about tokens the table left unchanged, and
/// it must NEVER block beyond a tight timeout or panic. On any uncertainty or
/// error it returns the token unchanged — silence beats a wrong guess.
pub trait Corrector {
    /// Returns a correction for `token`, or `token` itself when it has none.
    fn correct(&self, token: &str) -> String;
}

/// The cold-start fallback: no generalisation at all. Used on the first prompt
/// of a session (or while the daemon is still loading the dictionary). The
/// curated table and segmentation have already run, so this just passes tokens
/// through untouched — instant and offline.
#[derive(Debug, Default, Clone, Copy)]
pub struct TableOnly;

impl Corrector for TableOnly {
    fn correct(&self, token: &str) -> String {
        token.to_string()
    }
}

/// Asks the resident daemon to correct each token over its unix socket.
///
/// One short-lived connection per token (the daemon's protocol is one token per
/// round-trip). Every failure path — no socket, connect timeout, I/O error —
/// degrades to returning the token unchanged, so a missing or wedged daemon can
/// never block or corrupt a prompt.
pub struct DaemonCorrector {
    socket: PathBuf,
}

impl DaemonCorrector {
    /// Builds a corrector if a daemon is reachable *right now*, else `None`.
    ///
    /// Probing here (a single connect) lets the hook decide up front whether to
    /// use the daemon or fall back to [`TableOnly`] and spawn one, without
    /// paying a failed-connect cost on every token.
    pub fn connect() -> Option<Self> {
        let socket = platform::socket_path().ok()?;
        // A successful probe connect proves a live daemon; drop it immediately.
        UnixStream::connect(&socket).ok()?;
        Some(Self { socket })
    }

    /// Round-trips one token through the daemon, returning `None` on any error.
    fn query(&self, token: &str) -> Option<String> {
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        // Bound both halves so a wedged daemon cannot hang the prompt.
        stream.set_read_timeout(Some(CONNECT_TIMEOUT)).ok()?;
        stream.set_write_timeout(Some(CONNECT_TIMEOUT)).ok()?;
        writeln!(stream, "{token}").ok()?;
        let mut resp = String::new();
        BufReader::new(&stream).read_line(&mut resp).ok()?;
        let reply = resp.trim_end_matches(['\n', '\r']).to_string();
        // An empty reply means the daemon had nothing useful; keep the token.
        if reply.is_empty() {
            None
        } else {
            Some(reply)
        }
    }
}

impl Corrector for DaemonCorrector {
    fn correct(&self, token: &str) -> String {
        self.query(token).unwrap_or_else(|| token.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_only_passes_through() {
        assert_eq!(TableOnly.correct("anything"), "anything");
        assert_eq!(TableOnly.correct(""), "");
    }

    #[test]
    fn daemon_corrector_absent_socket_is_none() {
        // Point at a socket that cannot exist; connect must fail, not panic.
        let dc = DaemonCorrector {
            socket: std::env::temp_dir().join("dygi-nonexistent-xyzzy.sock"),
        };
        assert_eq!(dc.correct("teh"), "teh");
    }
}
