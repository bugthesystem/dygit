//! did-you-get-it engine.
//!
//! The crate is split so each unit has one job: [`clean`] turns a messy prompt
//! into a candidate reading, [`log`] persists what happened, [`config`] holds
//! user toggles, and [`commands`] wires the CLI subcommands to those pieces.
#![deny(missing_docs)]
#![deny(clippy::all)]

pub mod clean;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod error;
pub mod log;
pub mod platform;

#[cfg(test)]
mod test_support;
