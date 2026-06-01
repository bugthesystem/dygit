//! User configuration, persisted as `config.json`.
//!
//! A missing or unreadable file is *not* an error: we fall back to defaults so
//! the hook keeps working even if the user hand-edited the file into garbage.

use crate::error::DygiError;
use crate::platform::config_path;
use serde::{Deserialize, Serialize};

/// How loudly the plugin announces what it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    /// Confirm every cleanup with a one-line "✓ understood · …".
    Verbose,
    /// Only confirm low-confidence (`interpret`) cleanups.
    Quiet,
}

/// How eager the local pass is to change borderline tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggressiveness {
    /// Only fix high-confidence typos.
    Gentle,
    /// Also fix borderline tokens.
    Aggressive,
}

/// Persisted user settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Master on/off switch. When `false`, the hook emits nothing.
    pub enabled: bool,
    /// Confirmation verbosity.
    pub verbosity: Verbosity,
    /// Cleanup aggressiveness.
    pub aggressiveness: Aggressiveness,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            verbosity: Verbosity::Verbose,
            aggressiveness: Aggressiveness::Gentle,
        }
    }
}

impl Config {
    /// Loads config, falling back to [`Config::default`] on any problem.
    ///
    /// Infallible by design: the hot path must never break because of config.
    pub fn load() -> Config {
        let Ok(path) = config_path() else {
            return Config::default();
        };
        let Ok(bytes) = std::fs::read(path) else {
            return Config::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    /// Writes config to disk. Used by the `toggle` command, not the hot path.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the data dir cannot be resolved, the config cannot be
    /// serialised, or the file cannot be written.
    pub fn save(&self) -> Result<(), DygiError> {
        let path = config_path()?;
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_temp_dir;

    #[test]
    fn default_is_enabled_verbose_gentle() {
        let c = Config::default();
        assert!(c.enabled);
        assert_eq!(c.verbosity, Verbosity::Verbose);
        assert_eq!(c.aggressiveness, Aggressiveness::Gentle);
    }

    #[test]
    fn save_then_load_roundtrips() {
        with_temp_dir("dygi-test-config", || {
            let c = Config {
                enabled: false,
                verbosity: Verbosity::Quiet,
                ..Config::default()
            };
            c.save().unwrap();
            let loaded = Config::load();
            assert_eq!(loaded, c);
        });
    }

    #[test]
    fn garbage_file_falls_back_to_default() {
        with_temp_dir("dygi-test-config-garbage", || {
            std::fs::write(config_path().unwrap(), b"not json").unwrap();
            assert_eq!(Config::load(), Config::default());
        });
    }
}
