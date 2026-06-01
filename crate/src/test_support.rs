//! Shared test scaffolding for env-mutating tests.
//!
//! Every unit test that exercises the data directory points `DYGI_DATA_DIR` at
//! a temp dir. That env var is process-global and Rust runs tests in parallel,
//! so without coordination concurrent tests clobber each other's value and fail
//! non-deterministically. [`with_temp_dir`] funnels all such tests through one
//! shared mutex, so only one test mutates `DYGI_DATA_DIR` at a time.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Serializes every `DYGI_DATA_DIR` mutation across the whole test binary.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Distinguishes temp dirs created in the same process, so concurrent runs
/// (and back-to-back calls) never alias the same directory.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Acquires the env lock, recovering from a poisoned mutex.
///
/// A test that panics while holding the guard poisons the mutex. That panic is
/// already reported as a test failure; we deliberately recover the guard so the
/// poisoning does not cascade into failures of every later test.
fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `f` with `DYGI_DATA_DIR` pointed at a fresh, unique temp dir.
///
/// Holds the shared env lock for the whole closure, so no other test observes
/// or overwrites the env var while `f` runs. The temp dir name embeds `name`
/// (for debuggability) plus the process id and a monotonic counter (for
/// uniqueness). The env var and the directory are always cleaned up afterwards,
/// even if `f` panics — the lock is then released as the guard unwinds.
pub fn with_temp_dir(name: &str, f: impl FnOnce()) {
    let _guard = lock_env();

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("{name}-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    std::env::set_var("DYGI_DATA_DIR", &tmp);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::remove_var("DYGI_DATA_DIR");

    let _ = std::fs::remove_dir_all(&tmp);

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
