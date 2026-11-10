//! Safe creation of the CSV output file (M-21).
//!
//! The CSV is written to a sibling file `<name>.partial` and renamed over the destination
//! only after every row has been written and flushed. A failed run therefore never
//! replaces or truncates an existing file, and never leaves a destination that looks
//! complete but is not.
//!
//! - Without `--overwrite`, the destination is first reserved with an exclusive create, so
//!   an existing file is never replaced. The empty reservation is removed if the write
//!   fails.
//! - With `--overwrite`, the finished file replaces the destination by rename.
//! - A destination that is a directory, or whose parent directory is missing, is an error
//!   naming the path.
//! - If a `<name>.partial` file already exists, the command stops instead of replacing it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::HeadlessError;
use crate::csv::ExportError;

/// Writes a file at `path` through `body`, as described in the module documentation.
pub fn write_file_safely<T>(
    path: &Path,
    overwrite: bool,
    body: impl FnOnce(&mut dyn Write) -> Result<T, ExportError>,
) -> Result<T, HeadlessError> {
    if path.is_dir() {
        return Err(HeadlessError::DestinationIsDirectory(path.to_path_buf()));
    }
    let partial = partial_path(path)?;

    let reserved = if overwrite {
        false
    } else {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(HeadlessError::DestinationExists(path.to_path_buf()));
            }
            Err(source) => return Err(io_error("create", path, source)),
        }
    };
    let cleanup = |remove_partial: bool| {
        if remove_partial {
            let _ = fs::remove_file(&partial);
        }
        if reserved {
            let _ = fs::remove_file(path);
        }
    };

    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
    {
        Ok(file) => file,
        Err(source) => {
            cleanup(false);
            return Err(io_error("create", &partial, source));
        }
    };

    let mut writer = BufWriter::new(file);
    let result = body(&mut writer).and_then(|value| {
        writer.flush()?;
        Ok(value)
    });
    let value = match result {
        Ok(value) => value,
        Err(source) => {
            drop(writer);
            cleanup(true);
            return Err(HeadlessError::Export {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let file: File = match writer.into_inner() {
        Ok(file) => file,
        Err(error) => {
            cleanup(true);
            return Err(io_error("write", path, error.into_error()));
        }
    };
    drop(file);

    if let Err(source) = fs::rename(&partial, path) {
        cleanup(true);
        return Err(io_error("move the finished file to", path, source));
    }
    Ok(value)
}

/// `<destination file name>.partial` in the same directory.
fn partial_path(path: &Path) -> Result<PathBuf, HeadlessError> {
    let Some(name) = path.file_name() else {
        return Err(io_error(
            "use",
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "not a file path"),
        ));
    };
    let mut partial_name = name.to_os_string();
    partial_name.push(".partial");
    Ok(path.with_file_name(partial_name))
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> HeadlessError {
    HeadlessError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("nss-headless-output-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test directory");
        dir
    }

    fn write_text(path: &Path, overwrite: bool, text: &str) -> Result<(), HeadlessError> {
        write_file_safely(path, overwrite, |w| {
            w.write_all(text.as_bytes())?;
            Ok(())
        })
    }

    #[test]
    fn writes_new_file_and_leaves_no_partial() {
        let dir = test_dir("new");
        let path = dir.join("out.csv");
        write_text(&path, false, "a\n").expect("new file");
        assert_eq!(fs::read_to_string(&path).expect("read"), "a\n");
        assert!(!dir.join("out.csv.partial").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_file_is_protected_unless_overwrite() {
        let dir = test_dir("existing");
        let path = dir.join("out.csv");
        fs::write(&path, "keep me").expect("seed file");
        let error = write_text(&path, false, "new").expect_err("protected");
        assert!(matches!(error, HeadlessError::DestinationExists(_)));
        assert_eq!(fs::read_to_string(&path).expect("read"), "keep me");

        write_text(&path, true, "new").expect("overwrite");
        assert_eq!(fs::read_to_string(&path).expect("read"), "new");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_body_keeps_existing_file_and_removes_reservation() {
        let dir = test_dir("failure");
        let existing = dir.join("existing.csv");
        fs::write(&existing, "valid").expect("seed file");
        let failing = |_: &mut dyn Write| -> Result<(), ExportError> {
            Err(ExportError::Io(io::Error::other("simulated")))
        };
        assert!(write_file_safely(&existing, true, failing).is_err());
        assert_eq!(fs::read_to_string(&existing).expect("read"), "valid");

        let fresh = dir.join("fresh.csv");
        assert!(write_file_safely(&fresh, false, failing).is_err());
        assert!(!fresh.exists(), "reservation removed");
        assert!(!dir.join("fresh.csv.partial").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn directories_and_missing_parents_are_rejected() {
        let dir = test_dir("invalid");
        let error = write_text(&dir, true, "x").expect_err("directory");
        assert!(matches!(error, HeadlessError::DestinationIsDirectory(_)));
        let missing = dir.join("no-such-dir").join("out.csv");
        let error = write_text(&missing, false, "x").expect_err("missing parent");
        assert!(error.to_string().contains("no-such-dir"), "{error}");
        let _ = fs::remove_dir_all(dir);
    }
}
