//! Command-line runner for the Natural Selection Simulator (M-17, M-21).
//!
//! A thin shell over `sim`'s public API (I-06): it parses arguments, steps a
//! `sim::Simulation`, prints a summary, and writes the optional CSV time series. All file
//! I/O and the wall-clock timing of a run live here, never in `sim` (I-08). This crate
//! never depends on Bevy or egui (T-HL-6).

pub mod cli;
pub mod csv;
pub mod output;
pub mod run;

use std::fmt;
use std::io;
use std::path::PathBuf;

use sim::ValidationError;

/// State hash as `0x` followed by exactly 16 lowercase hexadecimal digits, zero-padded.
pub fn format_state_hash(hash: u64) -> String {
    format!("0x{hash:016x}")
}

/// Every way a `headless run` can fail after its arguments were parsed.
#[derive(Debug)]
pub enum HeadlessError {
    /// The scenario failed validation.
    InvalidConfig(Vec<ValidationError>),
    /// Options that must be used together were not (normally caught by the parser).
    Usage(&'static str),
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
    /// Writing CSV rows to `path` failed.
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
            Self::Usage(message) => write!(f, "{message}"),
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
                write!(f, "could not write CSV to {}: {source}", path.display())
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
