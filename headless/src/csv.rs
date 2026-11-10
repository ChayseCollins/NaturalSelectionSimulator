//! CSV time-series writer (M-21), on any [`std::io::Write`] target.
//!
//! Format (fixed; tested):
//! - One header row, then one row per observation, each ending in `\n`. No metadata,
//!   timestamps or other run-dependent data.
//! - Integers in plain decimal.
//! - Floats in Rust's `Display` form: the shortest decimal that reads back as the same
//!   `f64`, with a `.` separator and never an exponent, independent of locale. Whole
//!   values print without a fraction (`1`, not `1.0`); `-0` is written as `0`.
//! - Averages are empty fields when there are no prey. A non-finite value is an error,
//!   never written.
//! - `state_hash` is `0x` followed by exactly 16 lowercase hexadecimal digits, so
//!   spreadsheets keep it as text.

use std::fmt::{self, Write as _};
use std::io::{self, Write};

use sim::SummaryMetrics;

use crate::format_state_hash;

/// Column names, in output order.
pub const COLUMNS: [&str; 11] = [
    "tick",
    "sim_seconds",
    "prey_population",
    "avg_speed",
    "avg_size",
    "avg_sense",
    "avg_energy",
    "total_food",
    "cumulative_food_consumed",
    "cumulative_starvation_deaths",
    "state_hash",
];

#[derive(Debug)]
pub enum ExportError {
    /// The underlying writer failed (including on flush).
    Io(io::Error),
    /// A value was NaN or infinite; it is reported instead of being written.
    NonFinite { column: &'static str, tick: u64 },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NonFinite { column, tick } => {
                write!(f, "{column} at tick {tick} is not a finite number")
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::NonFinite { .. } => None,
        }
    }
}

impl From<io::Error> for ExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Streaming CSV writer: the header is written on creation, rows as they are sampled.
pub struct CsvWriter<W: Write> {
    writer: W,
    rows: u64,
}

impl<W: Write> CsvWriter<W> {
    /// Writes the header row.
    pub fn new(mut writer: W) -> Result<Self, ExportError> {
        let mut header = COLUMNS.join(",");
        header.push('\n');
        writer.write_all(header.as_bytes())?;
        Ok(Self { writer, rows: 0 })
    }

    /// Writes one observation row.
    pub fn write_row(&mut self, metrics: &SummaryMetrics) -> Result<(), ExportError> {
        let line = format_row(metrics)?;
        self.writer.write_all(line.as_bytes())?;
        self.rows += 1;
        Ok(())
    }

    /// Flushes the writer, propagating any error, and returns the number of rows written.
    pub fn finish(mut self) -> Result<u64, ExportError> {
        self.writer.flush()?;
        Ok(self.rows)
    }
}

/// Writes the header and every observation, then flushes.
pub fn write_csv<W: Write>(writer: W, observations: &[SummaryMetrics]) -> Result<(), ExportError> {
    let mut csv = CsvWriter::new(writer)?;
    for metrics in observations {
        csv.write_row(metrics)?;
    }
    csv.finish().map(|_| ())
}

/// One data row, including its trailing `\n`, in [`COLUMNS`] order.
pub fn format_row(m: &SummaryMetrics) -> Result<String, ExportError> {
    let tick = m.tick;
    let number = |column: &'static str, value: f64| -> Result<String, ExportError> {
        if !value.is_finite() {
            return Err(ExportError::NonFinite { column, tick });
        }
        let value = if value == 0.0 { 0.0 } else { value };
        Ok(format!("{value}"))
    };
    let optional = |column: &'static str, value: Option<f64>| match value {
        Some(value) => number(column, value),
        None => Ok(String::new()),
    };
    let fields = [
        tick.to_string(),
        number("sim_seconds", m.sim_seconds)?,
        m.prey_population.to_string(),
        optional("avg_speed", m.avg_speed)?,
        optional("avg_size", m.avg_size)?,
        optional("avg_sense", m.avg_sense)?,
        optional("avg_energy", m.avg_energy)?,
        number("total_food", m.total_food)?,
        number("cumulative_food_consumed", m.cumulative_food_consumed)?,
        m.cumulative_starvation_deaths.to_string(),
        format_state_hash(m.state_hash),
    ];
    let mut line = String::with_capacity(160);
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        // Writing to a String cannot fail.
        let _ = line.write_str(field);
    }
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use super::*;

    fn metrics(tick: u64) -> SummaryMetrics {
        SummaryMetrics {
            tick,
            sim_seconds: tick as f64 / 30.0,
            prey_population: 3,
            avg_speed: Some(1.25),
            avg_size: Some(1.0),
            avg_sense: Some(10.5),
            avg_energy: Some(4.0),
            total_food: 65_536.0,
            cumulative_food_consumed: 0.375,
            cumulative_starvation_deaths: 2,
            state_hash: 0x0000_00AB_CDEF_0123,
        }
    }

    /// A writer that accepts `budget` bytes, then fails every write; flush succeeds.
    struct FailAfter {
        budget: usize,
        written: Vec<u8>,
    }

    impl Write for FailAfter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.budget == 0 {
                return Err(io::Error::other("simulated disk full"));
            }
            let n = buf.len().min(self.budget);
            self.budget -= n;
            self.written.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A writer whose writes succeed but whose flush fails.
    struct FailOnFlush;

    impl Write for FailOnFlush {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("simulated flush failure"))
        }
    }

    #[test]
    fn header_has_the_fixed_column_order() {
        let mut out = Vec::new();
        write_csv(&mut out, &[]).expect("in-memory write");
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            "tick,sim_seconds,prey_population,avg_speed,avg_size,avg_sense,avg_energy,\
             total_food,cumulative_food_consumed,cumulative_starvation_deaths,state_hash\n"
        );
    }

    #[test]
    fn rows_use_locale_independent_shortest_formatting() {
        assert_eq!(
            format_row(&metrics(45)).expect("finite"),
            "45,1.5,3,1.25,1,10.5,4,65536,0.375,2,0x000000abcdef0123\n"
        );
        let mut m = metrics(1);
        m.sim_seconds = 1.0 / 30.0;
        m.avg_energy = Some(-0.0);
        m.total_food = 1e20;
        let row = format_row(&m).expect("finite");
        assert!(row.starts_with("1,0.03333333333333333,"), "{row}");
        assert!(
            row.contains(",0,100000000000000000000,"),
            "-0 and no exponent: {row}"
        );
    }

    /// T-CSV-6: no prey gives empty average fields; non-finite values are refused.
    #[test]
    fn t_csv_6_empty_population_writes_empty_averages() {
        let mut m = metrics(30);
        m.prey_population = 0;
        m.avg_speed = None;
        m.avg_size = None;
        m.avg_sense = None;
        m.avg_energy = None;
        assert_eq!(
            format_row(&m).expect("finite"),
            "30,1,0,,,,,65536,0.375,2,0x000000abcdef0123\n"
        );
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut m = metrics(7);
            m.avg_speed = Some(bad);
            assert!(matches!(
                format_row(&m),
                Err(ExportError::NonFinite {
                    column: "avg_speed",
                    tick: 7
                })
            ));
        }
    }

    #[test]
    fn whole_file_is_header_plus_one_line_per_observation() {
        let mut out = Vec::new();
        write_csv(&mut out, &[metrics(0), metrics(30)]).expect("in-memory write");
        let text = String::from_utf8(out).expect("utf-8");
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("0,0,3,"));
        assert!(lines[2].starts_with("30,1,3,"));
        assert!(!text.contains('\r'));
    }

    /// T-CSV-9 (unit part): an error part-way through writing is returned, not ignored.
    #[test]
    fn t_csv_9_mid_write_error_propagates() {
        let mut writer = FailAfter {
            budget: 200,
            written: Vec::new(),
        };
        let observations: Vec<SummaryMetrics> = (0..10).map(|t| metrics(t * 30)).collect();
        let error = write_csv(&mut writer, &observations).expect_err("must fail");
        assert!(matches!(error, ExportError::Io(_)), "{error:?}");
        assert_eq!(writer.written.len(), 200, "failed after the budget");
    }

    /// T-CSV-9 (unit part): a failure that only surfaces when buffered data is flushed is
    /// still reported, and a failing flush itself is reported.
    #[test]
    fn t_csv_9_flush_errors_are_not_swallowed() {
        let buffered = BufWriter::new(FailAfter {
            budget: 10,
            written: Vec::new(),
        });
        let mut csv = CsvWriter::new(buffered).expect("buffered header write succeeds");
        csv.write_row(&metrics(0))
            .expect("buffered row write succeeds");
        assert!(matches!(csv.finish(), Err(ExportError::Io(_))));

        assert!(matches!(
            write_csv(FailOnFlush, &[metrics(0)]),
            Err(ExportError::Io(_))
        ));
    }
}
