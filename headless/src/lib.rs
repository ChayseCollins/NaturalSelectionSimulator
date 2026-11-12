//! Command-line runner for the Natural Selection Simulator (M-17, M-18, M-21).
//!
//! A thin shell over `sim`'s public API (I-06): it reads scenario files, steps a
//! `sim::Simulation`, prints a summary, and writes the optional JSON results and CSV time
//! series; `bench` times the benchmark tiers. All file I/O and wall-clock timing live
//! here, never in `sim` (I-08). This crate never depends on Bevy or egui (T-HL-6).

pub mod bench;
pub mod cli;
pub mod csv;
pub mod experiment;
pub mod json;
pub mod output;
pub mod run;

use std::fmt;
use std::io;
use std::path::PathBuf;

use sim::{ScenarioError, ValidationError};

/// The baseline classroom preset, embedded at build time: the default scenario of both
/// `headless run` and the app, so both print the same hash for the same seed.
pub const BASELINE_PRESET: &str = include_str!("../../presets/baseline.scenario.toml");

/// State hash as `0x` followed by exactly 16 lowercase hexadecimal digits, zero-padded.
pub fn format_state_hash(hash: u64) -> String {
    format!("0x{hash:016x}")
}

/// Every way a `headless run` can fail after its arguments were parsed.
#[derive(Debug)]
pub enum HeadlessError {
    /// The scenario failed validation.
    InvalidConfig(Vec<ValidationError>),
    /// The scenario file could not be read.
    ScenarioRead { path: PathBuf, source: io::Error },
    /// The scenario file is not a valid scenario.
    Scenario {
        path: Option<PathBuf>,
        error: ScenarioError,
    },
    /// Options that must be used together were not (normally caught by the parser).
    Usage(&'static str),
    /// The `--seeds` list could not be parsed.
    Seeds(String),
    /// The CSV destination exists and `--overwrite` was not given.
    DestinationExists(PathBuf),
    /// The CSV destination is a directory.
    DestinationIsDirectory(PathBuf),
    /// A file-system operation on `path` failed.
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// Writing CSV rows or JSON to `path` failed.
    Export {
        path: PathBuf,
        source: csv::ExportError,
    },
    /// Printing the summary failed.
    Stdout(io::Error),
}

impl fmt::Display for HeadlessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(errors) => {
                write!(f, "invalid scenario:")?;
                for error in errors {
                    write!(f, " {error};")?;
                }
                Ok(())
            }
            Self::ScenarioRead { path, source } => {
                write!(f, "could not read --scenario {}: {source}", path.display())
            }
            Self::Scenario { path, error } => {
                match path {
                    Some(path) => write!(f, "invalid scenario {}: ", path.display())?,
                    None => write!(f, "invalid built-in scenario: ")?,
                }
                write!(f, "{}", error.to_string().replace('\n', "; "))
            }
            Self::Usage(message) => write!(f, "{message}"),
            Self::Seeds(message) => write!(f, "--seeds: {message}"),
            Self::DestinationExists(path) => write!(
                f,
                "{} already exists; pass --overwrite to replace it",
                path.display()
            ),
            Self::DestinationIsDirectory(path) => {
                write!(f, "{} is a directory, not a file", path.display())
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(f, "could not {action} {}: {source}", path.display()),
            Self::Export { path, source } => {
                write!(f, "could not write to {}: {source}", path.display())
            }
            Self::Stdout(source) => write!(f, "could not print the summary: {source}"),
        }
    }
}

impl std::error::Error for HeadlessError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_hash_is_fixed_width_lowercase_hex() {
        assert_eq!(format_state_hash(0), "0x0000000000000000");
        assert_eq!(format_state_hash(1), "0x0000000000000001");
        assert_eq!(format_state_hash(0xABCD), "0x000000000000abcd");
        assert_eq!(format_state_hash(u64::MAX), "0xffffffffffffffff");
        for hash in [0, 1, 0xABCD, u64::MAX] {
            let text = format_state_hash(hash);
            assert_eq!(text.len(), 18);
            assert_eq!(u64::from_str_radix(&text[2..], 16), Ok(hash));
        }
    }
}
