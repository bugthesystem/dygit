//! `dygi toggle [arg]` — flips config live. No arg prints current state.

use crate::config::{Aggressiveness, Config, Verbosity};

/// Applies a toggle argument and returns a confirmation line.
///
/// Accepted args: `on`, `off`, `verbose`, `quiet`, `aggressive`, `gentle`.
/// Anything else (including empty) just reports current state.
#[must_use]
pub fn run(arg: &str) -> String {
    let mut config = Config::load();
    let changed = match arg {
        "on" => {
            config.enabled = true;
            true
        }
        "off" => {
            config.enabled = false;
            true
        }
        "verbose" => {
            config.verbosity = Verbosity::Verbose;
            true
        }
        "quiet" => {
            config.verbosity = Verbosity::Quiet;
            true
        }
        "aggressive" => {
            config.aggressiveness = Aggressiveness::Aggressive;
            true
        }
        "gentle" => {
            config.aggressiveness = Aggressiveness::Gentle;
            true
        }
        _ => false,
    };
    if changed {
        // Save failures are reported but non-fatal.
        if config.save().is_err() {
            return "Could not save config (change not persisted).".into();
        }
    }
    format!(
        "did-you-get-it: {} · {} · {}",
        if config.enabled { "on" } else { "off" },
        match config.verbosity {
            Verbosity::Verbose => "verbose",
            Verbosity::Quiet => "quiet",
        },
        match config.aggressiveness {
            Aggressiveness::Gentle => "gentle",
            Aggressiveness::Aggressive => "aggressive",
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_temp_dir;

    #[test]
    fn off_then_state_shows_off() {
        with_temp_dir("dygi-test-toggle-off", || {
            let out = run("off");
            assert!(out.contains("off"));
            assert!(!Config::load().enabled);
        });
    }

    #[test]
    fn no_arg_reports_state_without_changing() {
        with_temp_dir("dygi-test-toggle-state", || {
            let _ = run("quiet"); // called here only to persist the change.
            let out = run("");
            assert!(out.contains("quiet"));
        });
    }
}
