//! The resident spell-correction daemon.
//!
//! Loading the 82k-word frequency dictionary into symspell costs ~500ms — far
//! over the per-prompt hook budget. The hook spawns a fresh process per prompt,
//! so there is no in-process cache to reuse. The fix is a *resident* daemon that
//! loads the dictionary **once** and answers per-token lookups over a unix
//! domain socket in 0.02–0.15ms each.
//!
//! ## Protocol (line-based, one token per round-trip)
//!
//! The client opens a connection, writes one request line, reads one response
//! line, and closes. Keeping it one token per connection keeps both ends trivial
//! and is still sub-millisecond; the hook simply loops over a prompt's tokens.
//!
//! * Request `<token>\n` → Response `<corrected-or-same>\n`.
//! * Request `__quit__\n` → the daemon shuts down (no response).
//!
//! A malformed or empty request is answered with the request echoed back
//! unchanged; it never crashes the daemon.
//!
//! ## Lifecycle / parent-death
//!
//! A leaked daemon would hold ~50MB forever, so it must die when its session
//! does. std gives us no portable `PR_SET_PDEATHSIG`, and we add no `libc`, so
//! two dependency-free mechanisms are combined:
//!
//! 1. **Idle backstop (the guarantee).** If no request arrives for
//!    [`IDLE_TIMEOUT`], the daemon exits. A live session sends a request every
//!    prompt, keeping it warm; an abandoned session lets it lapse. This bounds a
//!    leaked daemon's lifetime to the idle window regardless of *how* the parent
//!    went away, which is why it is the primary mechanism here: the hook that
//!    spawns the daemon is short-lived, so there is no long-lived parent to tie
//!    to. This IS the honest tradeoff — there is no immediate parent-death
//!    signal in the current wiring; the daemon can outlive its session by up to
//!    the idle window.
//! 2. **stdin-EOF watchdog (dormant; reserved for a future wiring).** If a
//!    spawner connected the daemon's stdin to a pipe it held open, the read end
//!    would see EOF when that holder died, and a background thread blocked on
//!    stdin would trigger immediate shutdown. The current [`spawn_detached`]
//!    gives the child `/dev/null` stdin (which reports EOF at once), so it
//!    deliberately does NOT arm this watchdog — it would otherwise kill the
//!    daemon instantly. The watchdog only arms when `DYGI_PARENT_PID` is set,
//!    signalling a deliberate pipe-holding parent. No such spawner exists today.
//!
//! `__quit__` (over the socket) is the explicit, immediate shutdown.
//!
//! ## Atomic publish (why the canonical socket appears only when ready)
//!
//! Single-instance and "never serve a half-loaded daemon" are both solved by
//! **binding a temp socket, loading the dictionary, then atomically renaming the
//! temp onto the canonical path**. The ordering is:
//!
//! 1. Probe the canonical path. Under this design the canonical socket exists
//!    *only* once a daemon has finished loading, so a successful connect there
//!    means a live, fully-loaded peer — we defer (exit 0) and load nothing.
//! 2. Bind a unique temp socket (`dygi.sock.<pid>`) in the **same directory** as
//!    the canonical one, so the later `rename` is same-filesystem and atomic.
//! 3. Load the dictionary (the slow ~500ms–2s step).
//! 4. `rename(temp, canonical)`. `rename(2)` is atomic and replaces any existing
//!    file in one step ("last writer wins"), with no unlink-then-bind gap — so a
//!    probing client never sees the path missing or pointing at a dead inode.
//!    The bound listener fd is independent of its filesystem name: renaming the
//!    path does **not** disturb the already-listening socket, which keeps
//!    accepting under the new name (Linux and macOS both keep the fd valid).
//!
//! Two cold daemons racing: each binds its own *distinct* temp path (keyed by
//! pid), so neither blocks the other at bind time. Both load, then both rename
//! onto the canonical path; the second rename atomically replaces the first.
//! The loser's listener is unlinked-by-rename and its accept loop drains to the
//! idle backstop. This wastes at most one redundant load in the rare exact-tie
//! case, but never produces two *visible* daemons and never leaves the canonical
//! path stale. The common case (one hook fires, then the next prompt's hook sees
//! the published socket via step 1) loads exactly once.
//!
//! On shutdown we remove the canonical socket only if we are the daemon that
//! published it (we own the path), and we remove our temp path if the rename
//! never happened (e.g. a load failure). We never remove a socket we did not
//! publish, so a racing winner's live socket is left untouched.

use crate::error::DygiError;
use crate::platform;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use symspell::{AsciiStringStrategy, SymSpell, Verbosity};

/// Max edit distance for a lookup. symspell's default index is built for 2, and
/// `lookup` panics if asked for more, so this must stay ≤ 2.
const MAX_EDIT_DISTANCE: i64 = 2;

/// Shut down after this long with no requests. A live session sends one request
/// per prompt, so this only fires on an abandoned session. It is the guaranteed
/// upper bound on how long a leaked daemon survives.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How often the idle-watchdog thread wakes to compare now vs. last activity.
const IDLE_POLL: Duration = Duration::from_secs(60);

/// Per-connection read budget on the *server* side. The serve loop is single-
/// threaded, so a client that connects and writes a partial line without a
/// newline would otherwise block `read_line` forever and wedge the daemon for
/// every other client. A well-behaved client writes its one line in microseconds
/// over a local socket, so this is generously short; on timeout we drop the bad
/// connection and keep serving.
const READ_TIMEOUT: Duration = Duration::from_millis(200);

/// A loaded dictionary that can correct one token.
///
/// Abstracted as a trait so the socket-serving loop can be tested against a
/// trivial fake corrector without loading the real 1.3MB dictionary.
trait Speller: Send + Sync + 'static {
    /// Returns the best correction for `token`, or `token` unchanged when the
    /// speller has no better suggestion.
    fn correct(&self, token: &str) -> String;
}

/// symspell-backed speller over the bundled frequency dictionary.
struct SymSpeller {
    inner: SymSpell<AsciiStringStrategy>,
}

impl SymSpeller {
    /// Loads `dict` (a `word<sep>count` per line file) into a fresh symspell.
    /// The ~500ms cost is paid here, once, at daemon start.
    fn load(dict: &Path) -> Result<Self, DygiError> {
        let mut inner: SymSpell<AsciiStringStrategy> = SymSpell::default();
        let path = dict.to_str().ok_or(DygiError::NoDataDir)?;
        if !inner.load_dictionary(path, 0, 1, " ") {
            return Err(DygiError::NoDataDir);
        }
        Ok(Self { inner })
    }
}

impl Speller for SymSpeller {
    fn correct(&self, token: &str) -> String {
        self.inner
            .lookup(token, Verbosity::Top, MAX_EDIT_DISTANCE)
            .first()
            .map_or_else(|| token.to_string(), |s| s.term.clone())
    }
}

/// Background-spawns `dygi daemon` detached from this (short-lived) hook
/// process, so the next prompt finds a warm daemon.
///
/// Fire-and-forget and never blocking: the child is fully detached (its own
/// session, stdio redirected to null), and we drop the handle without waiting.
/// Any failure is swallowed — a hook must never break because a spawn failed.
/// The spawner is the hook, which exits immediately, so the daemon's *effective*
/// parent becomes the init/launchd reaper. Lifecycle is therefore governed by
/// the idle backstop (and `__quit__`), not by this short-lived parent.
///
/// We deliberately do NOT set `DYGI_PARENT_PID` here: the child's stdin is
/// `/dev/null`, which reports EOF immediately, so arming the stdin-EOF watchdog
/// would shut the daemon down the instant it started. The watchdog is reserved
/// for a future wiring where a long-lived parent holds a real stdin pipe; in
/// this detached-spawn path the idle backstop is the lifecycle guarantee. See
/// the module docs for the full rationale.
pub fn spawn_detached() {
    use std::process::{Command, Stdio};

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach into its own session so it survives the hook's exit. Best-effort:
    // if the platform lacks `setsid`, the child still runs, just less isolated.
    detach(&mut cmd);
    let _ = cmd.spawn();
}

/// Puts the spawned daemon in its own session via `setsid(2)` in a pre-exec
/// hook, so a terminal/process-group teardown does not take the daemon with it.
///
/// `setsid` lives in libc, which is always linked, so we declare it directly —
/// this adds no new crate dependency. Adding no dependency is the whole point;
/// `nix`/`libc` would be cleaner but are out of scope here.
#[cfg(unix)]
fn detach(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid(2)` is async-signal-safe and touches no Rust state, so it
    // is a valid pre-exec action between fork and exec. Its return value is
    // ignored: if it fails (already a session leader), the child still runs.
    unsafe {
        cmd.pre_exec(|| {
            extern "C" {
                fn setsid() -> i32;
            }
            setsid();
            Ok(())
        });
    }
}

/// Runs the daemon: probe, bind a temp socket, load the dictionary, then
/// atomically publish the canonical socket and serve.
///
/// Returns `Ok(())` on a clean shutdown *and* on the "another daemon is already
/// live" no-op — both are success from the spawner's view. Errors are only the
/// genuinely fatal ones (no dict, cannot bind for a non-race reason), which the
/// caller logs/ignores.
///
/// Ordering (see the module docs for the full race analysis): we **probe the
/// canonical path first, bind a per-pid temp socket, load, then rename onto the
/// canonical path**. The canonical socket exists only once loading is done, so a
/// probing client (the hook) that finds it connectable is guaranteed a daemon
/// that can answer immediately — it never adopts a half-loaded daemon and then
/// stalls a whole prompt waiting on an `accept()` that has not happened yet.
///
/// # Errors
///
/// Returns `Err` only on genuinely fatal setup failures: the socket or
/// dictionary path cannot be resolved, the dictionary cannot be loaded, or the
/// temp socket cannot be bound or published. A live peer already serving is a
/// success (`Ok`), not an error.
pub fn run() -> Result<(), DygiError> {
    let socket = platform::socket_path()?;
    let dict = platform::dict_path()?; // resolve before binding so a missing dict fails clean.

    // A connectable canonical socket means a fully-loaded peer (the canonical
    // path only appears post-load). Defer without paying the load cost.
    if socket.exists() && UnixStream::connect(&socket).is_ok() {
        return Ok(());
    }

    // Bind a unique temp socket in the SAME directory as the canonical one, so
    // the publishing rename below is same-filesystem and therefore atomic.
    let temp = temp_socket_path(&socket);
    let _ = std::fs::remove_file(&temp); // clear a leftover from a crashed prior pid-reuse.
    let listener = UnixListener::bind(&temp)?;

    // Load AFTER binding the temp socket but BEFORE publishing: the canonical
    // path stays absent for the whole ~500ms–2s load, so no client can adopt a
    // daemon that cannot yet serve, and a racing daemon binds its own distinct
    // temp path rather than colliding with ours.
    let speller = SymSpeller::load(&dict).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp); // load failed: don't leak the temp socket.
    })?;

    // Atomic publish. `rename` over an existing canonical socket replaces it in
    // one step (last writer wins) with no unlink-then-bind window, and — because
    // the listener fd is independent of its filesystem name — the already-bound
    // listener keeps accepting under the new canonical name. This fixes both the
    // half-loaded-visibility race and the stale-reclaim TOCTOU at once.
    if let Err(e) = std::fs::rename(&temp, &socket) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.into());
    }

    serve(&listener, &speller, &socket);
    Ok(())
}

/// The per-pid temp path the listener is bound to before publishing. It sits in
/// the same directory as `socket` (so `rename` onto `socket` is atomic) and is
/// keyed by pid so two racing daemons never collide on the same temp name.
fn temp_socket_path(socket: &Path) -> std::path::PathBuf {
    let mut name = socket.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}", std::process::id()));
    socket.with_file_name(name)
}

/// Tracks the last-activity instant as whole seconds since an epoch, so the
/// idle watchdog can compare lock-free against the serving thread.
struct Activity {
    started: Instant,
    last_secs: AtomicU64,
}

impl Activity {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            last_secs: AtomicU64::new(0),
        })
    }
    /// Stamps "now" as the most recent activity.
    fn touch(&self) {
        self.last_secs
            .store(self.started.elapsed().as_secs(), Ordering::Relaxed);
    }
    /// Whole seconds since the last [`touch`](Self::touch).
    fn idle_for(&self) -> Duration {
        let now = self.started.elapsed().as_secs();
        Duration::from_secs(now.saturating_sub(self.last_secs.load(Ordering::Relaxed)))
    }
}

/// Serves connections until shutdown is requested, then removes the canonical
/// socket file (which this daemon published via the rename in [`run`], so it is
/// ours to remove).
///
/// Shutdown is requested by any of: a `__quit__` socket request, the stdin-EOF
/// watchdog, or the idle backstop. All three nudge a self-connection so the
/// blocking `accept()` returns and the loop can observe the flag and exit.
fn serve<S: Speller>(listener: &UnixListener, speller: &S, socket: &Path) {
    let activity = Activity::new();
    activity.touch();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    spawn_stdin_watchdog(stop.clone(), socket.to_path_buf());
    spawn_idle_watchdog(stop.clone(), activity.clone(), socket.to_path_buf());

    for stream in listener.incoming() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(stream) = stream else { continue }; // transient accept error: keep serving.
        activity.touch();
        // One token per connection; handle errors per-connection so a malformed
        // request can never take the daemon down.
        if handle(stream, speller) == Control::Quit {
            stop.store(true, Ordering::Relaxed);
            break;
        }
    }

    let _ = std::fs::remove_file(socket);
}

/// Whether a handled connection asked the daemon to keep serving or to quit.
#[derive(PartialEq)]
enum Control {
    Continue,
    Quit,
}

/// Reads one request line, writes one response line. Returns [`Control::Quit`]
/// on a `__quit__` request. Any I/O error is swallowed: a dropped client must
/// not affect the daemon.
///
/// A [`READ_TIMEOUT`] bounds the read so a client that connects and stalls
/// (e.g. writes a partial line without a newline and holds the connection open)
/// is dropped rather than wedging the single-threaded serve loop. A timeout
/// surfaces as an `Err` (`WouldBlock`/`TimedOut`) from `read_line`, which we
/// treat like any other read error: drop this connection and keep serving.
fn handle<S: Speller>(stream: UnixStream, speller: &S) -> Control {
    let Ok(read_half) = stream.try_clone() else {
        return Control::Continue;
    };
    // Bound the read before we block on it; a failure to set the timeout just
    // means we proceed without one (best-effort), which the timeout exists to
    // guard against, so bail to be safe.
    if read_half.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return Control::Continue;
    }
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return Control::Continue; // includes WouldBlock/TimedOut: drop the stalled client.
    }
    let token = line.trim_end_matches(['\n', '\r']);
    if token == "__quit__" {
        return Control::Quit;
    }
    let reply = correct_token(token, speller);
    let mut writer = stream;
    let _ = writeln!(writer, "{reply}");
    Control::Continue
}

/// Corrects one token, treating an empty token as a no-op. Kept separate so the
/// "empty stays empty" guard is unit-testable.
fn correct_token<S: Speller>(token: &str, speller: &S) -> String {
    if token.is_empty() {
        return String::new();
    }
    speller.correct(token)
}

/// Background thread: blocks reading stdin. On EOF (parent that held the write
/// end died) or an explicit `__quit__` line, signals shutdown and wakes the
/// accept loop via a throwaway self-connection.
///
/// When stdin is `/dev/null` (the current detached-spawn wiring), the first read
/// returns EOF immediately. That would shut the daemon down at once, defeating
/// the point — so we only treat EOF as a death signal if stdin is **not** a
/// regular empty/null source we can detect. In practice we cannot reliably tell
/// "/dev/null" from "a pipe whose writer just closed", so this watchdog is armed
/// only when `DYGI_PARENT_PID` is set, signalling that the spawner intends to
/// hold the pipe. Otherwise the idle backstop is the sole lifecycle guard.
fn spawn_stdin_watchdog(stop: Arc<std::sync::atomic::AtomicBool>, socket: std::path::PathBuf) {
    if std::env::var("DYGI_PARENT_PID").is_err() {
        return; // no parent pipe promised; rely on the idle backstop.
    }
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 64];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break, // EOF: parent gone.
                Ok(n) if buf[..n].contains(&b'q') && line_is_quit(&buf[..n]) => break,
                Ok(_) => {}      // ignore other input; keep reading.
                Err(_) => break, // treat a broken stdin as parent-gone too.
            }
        }
        request_shutdown(&stop, &socket);
    });
}

/// True if the chunk read from stdin is exactly a `__quit__` line.
fn line_is_quit(chunk: &[u8]) -> bool {
    let s = String::from_utf8_lossy(chunk);
    s.trim() == "__quit__"
}

/// Background thread: periodically checks the idle timer and signals shutdown
/// once it exceeds [`IDLE_TIMEOUT`].
fn spawn_idle_watchdog(
    stop: Arc<std::sync::atomic::AtomicBool>,
    activity: Arc<Activity>,
    socket: std::path::PathBuf,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(IDLE_POLL);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if activity.idle_for() >= IDLE_TIMEOUT {
            request_shutdown(&stop, &socket);
            return;
        }
    });
}

/// Sets the stop flag and pokes the accept loop with a throwaway connection so
/// its blocking `accept()` returns and observes the flag.
fn request_shutdown(stop: &std::sync::atomic::AtomicBool, socket: &Path) {
    stop.store(true, Ordering::Relaxed);
    let _ = UnixStream::connect(socket); // wake accept(); errors are fine.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fake speller backed by a fixed map, so the socket loop can be tested
    /// without loading the real dictionary.
    struct FakeSpeller(HashMap<String, String>);
    impl Speller for FakeSpeller {
        fn correct(&self, token: &str) -> String {
            self.0
                .get(token)
                .cloned()
                .unwrap_or_else(|| token.to_string())
        }
    }

    fn fake(pairs: &[(&str, &str)]) -> Arc<FakeSpeller> {
        Arc::new(FakeSpeller(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ))
    }

    /// Round-trips a single token against an already-running listener.
    fn ask(socket: &Path, token: &str) -> String {
        let mut c = UnixStream::connect(socket).unwrap();
        writeln!(c, "{token}").unwrap();
        let mut resp = String::new();
        BufReader::new(&c).read_line(&mut resp).unwrap();
        resp.trim_end().to_string()
    }

    fn temp_socket(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dygi-daemon-test-{tag}-{}.sock",
            std::process::id()
        ))
    }

    #[test]
    fn empty_token_corrects_to_empty() {
        let s = fake(&[]);
        assert_eq!(correct_token("", s.as_ref()), "");
    }

    #[test]
    fn serves_corrections_and_quits() {
        let socket = temp_socket("serve");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let speller = fake(&[("teh", "the"), ("funtion", "function")]);
        let sock_for_thread = socket.clone();
        let server =
            std::thread::spawn(move || serve(&listener, speller.as_ref(), &sock_for_thread));

        assert_eq!(ask(&socket, "teh"), "the");
        assert_eq!(ask(&socket, "funtion"), "function");
        // An unknown token comes back unchanged.
        assert_eq!(ask(&socket, "form"), "form");

        // __quit__ ends the daemon and removes the socket.
        let mut c = UnixStream::connect(&socket).unwrap();
        writeln!(c, "__quit__").unwrap();
        server.join().unwrap();
        assert!(!socket.exists(), "socket file should be cleaned up on quit");
    }

    #[test]
    fn malformed_request_does_not_crash() {
        let socket = temp_socket("malformed");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let speller = fake(&[("teh", "the")]);
        let sock_for_thread = socket.clone();
        let server =
            std::thread::spawn(move || serve(&listener, speller.as_ref(), &sock_for_thread));

        // Connect and drop without writing a newline-terminated request.
        {
            let mut c = UnixStream::connect(&socket).unwrap();
            let _ = c.write_all(b"no newline then close");
        }
        // Daemon survives; a well-formed request still works.
        assert_eq!(ask(&socket, "teh"), "the");

        let mut c = UnixStream::connect(&socket).unwrap();
        writeln!(c, "__quit__").unwrap();
        server.join().unwrap();
    }

    /// A client that connects, writes a partial line WITHOUT a newline, and holds
    /// the connection open must NOT wedge the single-threaded serve loop: the
    /// server-side [`READ_TIMEOUT`] drops the stalled connection so a second,
    /// well-formed client is still served. Without the timeout, `read_line` on
    /// the held connection would block forever and starve everyone else.
    #[test]
    fn held_open_partial_line_does_not_wedge_serve_loop() {
        let socket = temp_socket("wedge");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let speller = fake(&[("teh", "the")]);
        let sock_for_thread = socket.clone();
        let server =
            std::thread::spawn(move || serve(&listener, speller.as_ref(), &sock_for_thread));

        // Connect, write a partial line with NO newline, and KEEP the stream
        // alive for the rest of the test by binding it (not dropping it). The
        // serve loop accepts this first, blocks in read_line, and must time out.
        let mut stuck = UnixStream::connect(&socket).unwrap();
        stuck.write_all(b"partial-no-newline").unwrap();

        // While `stuck` is still held open, a well-formed client must be served.
        // The READ_TIMEOUT (200ms) is short, so this returns promptly once the
        // stalled connection is dropped. We keep `stuck` borrowed past this line.
        assert_eq!(ask(&socket, "teh"), "the");
        // Touch `stuck` after the assertion so it provably outlives the served
        // request, proving the timeout (not a client disconnect) freed the loop.
        let _ = stuck.write_all(b"");

        let mut c = UnixStream::connect(&socket).unwrap();
        writeln!(c, "__quit__").unwrap();
        server.join().unwrap();
    }

    #[test]
    fn temp_socket_path_is_sibling_keyed_by_pid() {
        let canonical = std::path::Path::new("/some/dir/dygi.sock");
        let temp = temp_socket_path(canonical);
        // Same directory (so rename is atomic) and the pid-suffixed name.
        assert_eq!(temp.parent(), canonical.parent());
        assert_eq!(
            temp.file_name().unwrap().to_str().unwrap(),
            format!("dygi.sock.{}", std::process::id())
        );
    }

    /// End-to-end through the real `run()`: load a TINY symspell dictionary,
    /// bind the real socket path, round-trip a token, then `__quit__`.
    ///
    /// A 5-word dict loads instantly, so this stays fast and reliable. We drive
    /// it through `with_temp_dir` (which sets `DYGI_DATA_DIR`, so `socket_path()`
    /// lands in the temp dir and holds the env lock) and point `DYGI_DICT_PATH`
    /// at a dict we write into that same dir. The stdin watchdog stays disarmed
    /// because `DYGI_PARENT_PID` is unset, so the test runner's stdin is safe.
    #[test]
    fn run_loads_tiny_dict_and_serves_real_symspell() {
        crate::test_support::with_temp_dir("dygi-daemon-e2e", || {
            // `with_temp_dir` only sets the env var; the dir is created lazily
            // by `data_dir()`. Materialise it now so we can write the dict into
            // it. `socket_path()` calls `data_dir()`, which is idempotent.
            let dir = crate::platform::data_dir().unwrap();
            // SymSpell frequency dict: `word count` per line. Give `the` the
            // highest count so `teh` (edit-distance 1) resolves to it.
            let dict = dir.join("tiny_dict.txt");
            std::fs::write(
                &dict,
                "the 1000000\nfunction 5000\nuser 4000\nbug 3000\nfix 2000\n",
            )
            .unwrap();
            std::env::set_var("DYGI_DICT_PATH", &dict);

            let server = std::thread::spawn(|| {
                let _ = super::run();
            });

            // Wait for the socket to appear (tiny dict loads in well under a
            // second; poll briefly rather than sleeping a fixed duration).
            let socket = crate::platform::socket_path().unwrap();
            for _ in 0..200 {
                if socket.exists() && UnixStream::connect(&socket).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }

            // `teh` → `the` via real symspell; an in-dict word is unchanged.
            assert_eq!(ask(&socket, "teh"), "the");
            assert_eq!(ask(&socket, "function"), "function");

            let mut c = UnixStream::connect(&socket).unwrap();
            writeln!(c, "__quit__").unwrap();
            server.join().unwrap();

            std::env::remove_var("DYGI_DICT_PATH");
        });
    }
}
