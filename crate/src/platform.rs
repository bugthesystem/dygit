//! Filesystem paths and platform detection.
//!
//! Data lives in a single global directory so stats accumulate across every
//! project. We resolve it from `$HOME` rather than a crate like `dirs` to keep
//! the dependency set at zero beyond serde.

use crate::error::DygiError;
use std::path::PathBuf;

/// Returns the plugin's global data directory, creating it if needed.
///
/// Layout: `$HOME/.claude/plugins/data/did-you-get-it/`. We honour
/// `DYGI_DATA_DIR` first so tests can point at a temp dir.
pub fn data_dir() -> Result<PathBuf, DygiError> {
    if let Ok(override_dir) = std::env::var("DYGI_DATA_DIR") {
        let p = PathBuf::from(override_dir);
        std::fs::create_dir_all(&p)?;
        return Ok(p);
    }
    let home = std::env::var("HOME").map_err(|_| DygiError::NoDataDir)?;
    let p = PathBuf::from(home)
        .join(".claude")
        .join("plugins")
        .join("data")
        .join("did-you-get-it");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Path to the append-only event log.
pub fn events_path() -> Result<PathBuf, DygiError> {
    Ok(data_dir()?.join("events.jsonl"))
}

/// Path to the config file.
pub fn config_path() -> Result<PathBuf, DygiError> {
    Ok(data_dir()?.join("config.json"))
}

/// Path to the daemon's unix domain socket, under the data dir so it shares the
/// data dir's lifetime and `DYGI_DATA_DIR` override (tests point both at a temp
/// dir). The daemon binds here; the hook connects here.
pub fn socket_path() -> Result<PathBuf, DygiError> {
    Ok(data_dir()?.join("dygi.sock"))
}

/// Resolves the path to the 82k-word frequency dictionary the daemon loads.
///
/// The dictionary ships inside the plugin, not the data dir, so its location is
/// not derivable from `$HOME`. Resolution order:
/// 1. `DYGI_DICT_PATH` — set by the hook wrapper from
///    `${CLAUDE_PLUGIN_ROOT}/crate/data/freq_dict_en.txt`. This is the path used
///    in production.
/// 2. Fallbacks relative to the running binary, so a developer invoking the
///    binary directly (or a relocated layout) still finds the dict:
///    `<exe_dir>/../crate/data/freq_dict_en.txt` (binary in `bin/`) and
///    `<exe_dir>/data/freq_dict_en.txt` (binary beside a `data/` dir).
///
/// Returns the first candidate that exists, or `NoDataDir` if none do — callers
/// on the hot path treat that as "no daemon", never an error.
pub fn dict_path() -> Result<PathBuf, DygiError> {
    if let Ok(p) = std::env::var("DYGI_DICT_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join("../crate/data/freq_dict_en.txt"),
                dir.join("data/freq_dict_en.txt"),
            ];
            for c in candidates {
                if c.exists() {
                    return Ok(c);
                }
            }
        }
    }
    Err(DygiError::NoDataDir)
}

/// The platform triple slug used to name the prebuilt binary (`dygi-<slug>`).
///
/// Kept here so the build script and the runtime wrapper agree on one source
/// of truth for naming.
pub fn platform_slug() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_temp_dir;

    #[test]
    fn data_dir_honours_override() {
        // Share the env lock with every other DYGI_DATA_DIR test. The helper
        // sets the var to a unique temp dir; we read it back to learn the path.
        with_temp_dir("dygi-test-datadir", || {
            let expected = PathBuf::from(std::env::var("DYGI_DATA_DIR").unwrap());
            let d = data_dir().unwrap();
            assert_eq!(d, expected);
            assert!(d.exists());
        });
    }

    #[test]
    fn slug_is_known() {
        // On the host this must resolve to a real slug, never "unsupported".
        assert_ne!(platform_slug(), "unsupported");
    }
}
