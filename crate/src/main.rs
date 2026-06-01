//! `dygi` CLI entry point.
//!
//! Subcommands map 1:1 to plugin surfaces. The entry point never panics on bad
//! input: `hook` swallows all errors (prints nothing, exits 0) so a broken
//! plugin is invisible; the read-only commands print a friendly line instead.

use dygi::commands::{history, hook, stats, toggle, undo};
use dygi::daemon;
use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let sub = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();

    match sub.as_str() {
        "hook" => run_hook(),
        "daemon" => {
            // The resident corrector. Errors (no dict, cannot bind) and the
            // "another daemon is already live" no-op both exit 0 — a daemon that
            // cannot start must never surface as a failure on the prompt path.
            let _ = daemon::run();
            ExitCode::SUCCESS
        }
        "history" => {
            let n = rest.first().and_then(|s| s.parse().ok()).unwrap_or(10);
            print!("{}", history::run(n));
            ExitCode::SUCCESS
        }
        "stats" => {
            print!("{}", stats::run());
            ExitCode::SUCCESS
        }
        "toggle" => {
            print!("{}", toggle::run(rest.first().map_or("", String::as_str)));
            ExitCode::SUCCESS
        }
        "undo" => {
            print!("{}", undo::run());
            ExitCode::SUCCESS
        }
        // Unknown subcommand: do nothing, succeed. Never obstruct.
        _ => ExitCode::SUCCESS,
    }
}

/// Reads the hook payload from stdin and prints the additionalContext JSON.
/// Every failure path returns SUCCESS with no output, by contract.
fn run_hook() -> ExitCode {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return ExitCode::SUCCESS;
    }
    let Ok(input) = serde_json::from_str::<hook::HookInput>(&buf) else {
        return ExitCode::SUCCESS;
    };
    let now = now_iso();
    // Pick the corrector (daemon if live, else table-only + background spawn).
    let corrector = hook::build_corrector();
    let output = hook::run(&input, &now, corrector.as_ref());
    if !output.is_empty() {
        print!("{output}");
    }
    ExitCode::SUCCESS
}

/// Current UTC time as an ISO-8601 string, without pulling in `chrono`.
/// Uses `SystemTime` since the engine itself is clock-free and only `main`
/// needs the wall clock.
fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Minimal civil-time formatting (UTC). Good enough for a log timestamp.
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    // Days-since-epoch fits i64 for any realistic wall clock; saturate rather
    // than wrap if the system clock is absurd.
    let (y, mo, d) = civil_from_days(i64::try_from(days).unwrap_or(i64::MAX));
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Converts days-since-Unix-epoch to a (year, month, day) civil date.
/// Algorithm from Howard Hinnant's `chrono`-compatible date math (public domain).
const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    // `d` (1..=31) and `m` (1..=12) are bounded by the algorithm, so neither
    // cast can truncate or lose sign. Mask to the low 32 bits to convert without
    // a panic in this `const fn` (`try_from` is not yet const-usable here); the
    // value is always small, so the mask is an identity.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "d is 1..=31 and m is 1..=12 by construction; the cast is exact"
    )]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "m is 1..=12 by construction; the cast is exact"
    )]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_known_epoch() {
        // Day 0 == 1970-01-01.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-05-31 is 20604 days after epoch.
        assert_eq!(civil_from_days(20_604), (2026, 5, 31));
    }
}
