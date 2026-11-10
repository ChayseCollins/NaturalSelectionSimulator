//! Process-level tests of the `headless` binary: exit status, stderr, and real files
//! (M-17, M-21, T-CSV-1 to T-CSV-4 and T-CSV-7 to T-CSV-9, T-DET-4's headless part).
//! Files go under Cargo's per-target test directory, never a system path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use headless::format_state_hash;
use sim::{ScenarioConfig, Simulation};

const HEADER: &str = "tick,sim_seconds,prey_population,avg_speed,avg_size,avg_sense,avg_energy,\
                      total_food,cumulative_food_consumed,cumulative_starvation_deaths,state_hash";

fn headless(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_headless"))
        .args(args)
        .output()
        .expect("spawn the headless binary")
}

fn test_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("headless-cli")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn csv_run(seed: &str, ticks: &str, every: &str, path: &Path, extra: &[&str]) -> Output {
    let path = path.to_str().expect("UTF-8 test path");
    let mut args = vec![
        "run",
        "--seed",
        seed,
        "--ticks",
        ticks,
        "--sample-every",
        every,
        "--csv",
        path,
    ];
    args.extend_from_slice(extra);
    headless(&args)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn assert_failure_mentions(output: &Output, needle: &str) {
    assert!(!output.status.success(), "expected failure");
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains(needle),
        "stderr lacks {needle:?}:\n{stderr}"
    );
}

/// Data rows of a CSV file, split into fields.
fn rows(path: &Path) -> (String, Vec<Vec<String>>) {
    let content = fs::read_to_string(path).expect("read CSV");
    let mut lines = content.lines();
    let header = lines.next().expect("header").to_string();
    let rows = lines
        .map(|line| line.split(',').map(str::to_string).collect())
        .collect();
    (header, rows)
}

fn ticks_column(rows: &[Vec<String>]) -> Vec<u64> {
    rows.iter()
        .map(|row| row[0].parse().expect("integer tick"))
        .collect()
}

fn printed_hash(output: &Output) -> String {
    text(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("state hash:"))
        .expect("summary prints the state hash")
        .trim()
        .to_string()
}

/// T-CSV-1: 300 ticks sampled every 30 give the fixed header and 11 rows, 0 to 300.
#[test]
fn t_csv_1_three_hundred_ticks_every_thirty() {
    let dir = test_dir("t_csv_1");
    let path = dir.join("results.csv");
    let output = csv_run("1", "300", "30", &path, &[]);
    assert_success(&output);
    let (header, rows) = rows(&path);
    assert_eq!(header, HEADER);
    assert_eq!(rows.len(), 11);
    assert_eq!(
        ticks_column(&rows),
        (0..=10).map(|k| k * 30).collect::<Vec<u64>>()
    );
    assert!(rows.iter().all(|row| row.len() == 11));
    assert!(text(&output.stdout).contains("11 rows written to"));
    let hash = &rows[10][10];
    assert!(hash.starts_with("0x") && hash.len() == 18, "{hash}");
}

/// T-CSV-2: 305 ticks add a final row for tick 305.
#[test]
fn t_csv_2_final_partial_interval_gets_a_row() {
    let dir = test_dir("t_csv_2");
    let path = dir.join("results.csv");
    assert_success(&csv_run("1", "305", "30", &path, &[]));
    let (_, rows) = rows(&path);
    let mut expected: Vec<u64> = (0..=10).map(|k| k * 30).collect();
    expected.push(305);
    assert_eq!(ticks_column(&rows), expected);
}

#[test]
fn zero_tick_run_writes_header_and_tick_zero() {
    let dir = test_dir("zero_ticks");
    let path = dir.join("results.csv");
    assert_success(&csv_run("1", "0", "30", &path, &[]));
    let (header, rows) = rows(&path);
    assert_eq!(header, HEADER);
    assert_eq!(ticks_column(&rows), [0]);
}

/// T-CSV-3: two identical runs write byte-identical files.
#[test]
fn t_csv_3_identical_runs_are_byte_identical() {
    let dir = test_dir("t_csv_3");
    let (a, b) = (dir.join("a.csv"), dir.join("b.csv"));
    assert_success(&csv_run("7", "305", "30", &a, &[]));
    assert_success(&csv_run("7", "305", "30", &b, &[]));
    assert_eq!(fs::read(&a).expect("read a"), fs::read(&b).expect("read b"));
}

/// T-CSV-4: the same run with and without CSV recording ends in the same state hash.
#[test]
fn t_csv_4_csv_recording_does_not_change_the_final_hash() {
    let dir = test_dir("t_csv_4");
    let path = dir.join("results.csv");
    let with_csv = csv_run("3", "305", "30", &path, &[]);
    assert_success(&with_csv);
    let without_csv = headless(&["run", "--seed", "3", "--ticks", "305"]);
    assert_success(&without_csv);
    let (_, rows) = rows(&path);
    let last_row_hash = rows.last().expect("final row")[10].clone();
    assert_eq!(printed_hash(&with_csv), printed_hash(&without_csv));
    assert_eq!(printed_hash(&without_csv), last_row_hash);
}

/// T-CSV-7: a zero interval is rejected and nothing is written.
#[test]
fn t_csv_7_zero_interval_is_rejected() {
    let dir = test_dir("t_csv_7");
    let path = dir.join("results.csv");
    let output = csv_run("1", "10", "0", &path, &[]);
    assert_failure_mentions(&output, "--sample-every");
    assert!(!path.exists());
}

/// T-CSV-8: an existing file is kept without --overwrite and replaced with it.
#[test]
fn t_csv_8_existing_destination_is_protected() {
    let dir = test_dir("t_csv_8");
    let path = dir.join("results.csv");
    fs::write(&path, "keep me\n").expect("seed file");
    let output = csv_run("1", "30", "30", &path, &[]);
    assert_failure_mentions(&output, "--overwrite");
    assert_failure_mentions(&output, path.to_str().expect("UTF-8"));
    assert_eq!(fs::read_to_string(&path).expect("read"), "keep me\n");

    assert_success(&csv_run("1", "30", "30", &path, &["--overwrite"]));
    let (header, rows) = rows(&path);
    assert_eq!(header, HEADER);
    assert_eq!(rows.len(), 2);
    assert!(!dir.join("results.csv.partial").exists());
}

/// T-CSV-9: an invalid destination fails with a non-zero status naming the path.
#[test]
fn t_csv_9_invalid_destinations_fail_and_name_the_path() {
    let dir = test_dir("t_csv_9");
    let missing_parent = dir.join("no-such-directory").join("results.csv");
    let output = csv_run("1", "30", "30", &missing_parent, &[]);
    assert_failure_mentions(&output, "no-such-directory");
    assert!(text(&output.stdout).is_empty(), "no success message");

    let directory = dir.join("a-directory");
    fs::create_dir_all(&directory).expect("create directory");
    let output = csv_run("1", "30", "30", &directory, &["--overwrite"]);
    assert_failure_mentions(&output, "a-directory");
    assert!(directory.is_dir(), "directory untouched");
}

#[test]
fn usage_errors_exit_non_zero_with_help() {
    let no_args = headless(&[]);
    assert!(!no_args.status.success());
    assert!(text(&no_args.stderr).contains("Usage"));

    assert_failure_mentions(&headless(&["experiment"]), "unrecognized subcommand");
    assert_failure_mentions(
        &headless(&["run", "--seed", "1", "--ticks", "5", "--scenario", "x"]),
        "--scenario",
    );
    assert_failure_mentions(&headless(&["run", "--seed", "x", "--ticks", "5"]), "--seed");
    assert_failure_mentions(&headless(&["run", "--ticks", "5"]), "--seed");
    assert_failure_mentions(
        &headless(&["run", "--seed", "1", "--ticks", "5", "--csv", "x.csv"]),
        "--sample-every",
    );
    assert_failure_mentions(
        &headless(&["run", "--seed", "1", "--ticks", "5", "--overwrite"]),
        "--csv",
    );
}

#[test]
fn help_lists_every_option() {
    let output = headless(&["run", "--help"]);
    assert_success(&output);
    let help = text(&output.stdout);
    for option in [
        "--seed",
        "--ticks",
        "--sample-every",
        "--csv",
        "--overwrite",
    ] {
        assert!(help.contains(option), "{option} missing:\n{help}");
    }
}

#[test]
fn summary_reports_every_required_value() {
    let output = headless(&["run", "--seed", "1", "--ticks", "30"]);
    assert_success(&output);
    let stdout = text(&output.stdout);
    for label in [
        "run seed:",
        "ticks:",
        "simulated seconds:",
        "prey population:",
        "average speed:",
        "average size:",
        "average sense range:",
        "average energy:",
        "total food:",
        "cumulative food consumed:",
        "cumulative starvation deaths:",
        "state hash:",
    ] {
        assert!(stdout.contains(label), "{label} missing:\n{stdout}");
    }
    assert!(stdout.contains("30 requested, 30 completed"));
}

/// T-DET-4, headless part: the binary's `run --ticks 1000` hash equals stepping the same
/// scenario directly. The app-driver comparison is in `app/tests/t_det_4.rs`.
#[test]
fn t_det_4_binary_matches_direct_stepping() {
    let output = headless(&["run", "--seed", "1", "--ticks", "1000"]);
    assert_success(&output);
    let mut direct = Simulation::new(&ScenarioConfig::default(), 1).expect("valid");
    direct.step_n(1_000);
    assert_eq!(
        printed_hash(&output),
        format_state_hash(direct.state_hash())
    );
}
