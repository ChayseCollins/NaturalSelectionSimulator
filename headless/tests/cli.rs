//! Process-level tests of the `headless` binary: exit status, stderr, and real files
//! (M-17, M-18, M-21: T-HL-1, T-HL-3, T-HL-4, T-CSV-1 to T-CSV-4 and T-CSV-7 to T-CSV-9,
//! T-DET-4's headless part). Files go under Cargo's per-target test directory, never a
//! system path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use headless::format_state_hash;
use sim::Simulation;

const HEADER: &str = "tick,sim_seconds,prey_population,avg_speed,avg_size,avg_sense,avg_energy,\
                      total_food,cumulative_food_consumed,cumulative_starvation_deaths,state_hash,\
                      predator_population,avg_predator_speed,avg_predator_size,avg_predator_sense,\
                      avg_predator_energy,cumulative_prey_births,cumulative_predator_births,\
                      cumulative_predations,cumulative_predator_starvation_deaths,\
                      prey_max_generation,predator_max_generation";

const COLUMN_COUNT: usize = 22;

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
    assert!(rows.iter().all(|row| row.len() == COLUMN_COUNT));
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

    assert_failure_mentions(&headless(&["simulate"]), "unrecognized subcommand");
    assert_failure_mentions(&headless(&["experiment", "--ticks", "5"]), "--seeds");
    assert_failure_mentions(
        &headless(&[
            "run",
            "--seed",
            "1",
            "--ticks",
            "5",
            "--scenario",
            "no-such-file.toml",
        ]),
        "--scenario no-such-file.toml",
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
        "--scenario",
        "--seed",
        "--ticks",
        "--out",
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
        "predator population:",
        "cumulative prey births:",
        "cumulative predations:",
        "prey max generation:",
        "state hash:",
    ] {
        assert!(stdout.contains(label), "{label} missing:\n{stdout}");
    }
    assert!(stdout.contains("30 requested, 30 completed"));
}

/// T-DET-4, headless part: the binary's `run --ticks 1000` hash equals stepping the same
/// scenario (the built-in baseline preset) directly. The app-driver comparison is in
/// `app/src/t_det_4.rs`.
#[test]
fn t_det_4_binary_matches_direct_stepping() {
    let output = headless(&["run", "--seed", "1", "--ticks", "1000"]);
    assert_success(&output);
    let config = headless::run::load_scenario(None).expect("baseline");
    let mut direct = Simulation::new(&config, 1).expect("valid");
    direct.step_n(1_000);
    assert_eq!(
        printed_hash(&output),
        format_state_hash(direct.state_hash())
    );
}

fn preset_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../presets/baseline.scenario.toml")
        .to_str()
        .expect("UTF-8 path")
        .to_string()
}

/// T-HL-1: `run` exits 0 and writes JSON with every required field (X4); the file and
/// the built-in scenario give the same hash.
#[test]
fn t_hl_1_run_writes_json_with_every_required_field() {
    let dir = test_dir("t_hl_1");
    let path = dir.join("run.json");
    let preset = preset_path();
    let output = headless(&[
        "run",
        "--scenario",
        &preset,
        "--seed",
        "2",
        "--ticks",
        "120",
        "--out",
        path.to_str().expect("UTF-8"),
    ]);
    assert_success(&output);
    let json = fs::read_to_string(&path).expect("JSON written");
    for key in [
        "\"name\": \"Baseline predator-prey\"",
        "\"format_version\": 1",
        "\"run_seed\": 2",
        "\"terrain_seed\": 1001",
        "\"ticks\": 120",
        "\"state_hash\": \"0x",
        "\"prey\": {",
        "\"predator\": {",
        "\"cumulative_births\"",
        "\"cumulative_deaths\"",
        "\"successful_predations\"",
        "\"elapsed_seconds\"",
    ] {
        assert!(json.contains(key), "{key} missing:\n{json}");
    }
    let built_in = headless(&["run", "--seed", "2", "--ticks", "120"]);
    assert_eq!(printed_hash(&built_in), printed_hash(&output));
}

/// T-HL-3: an invalid scenario exits non-zero with a message naming the problem.
#[test]
fn t_hl_3_invalid_scenarios_fail_naming_the_problem() {
    let dir = test_dir("t_hl_3");
    let cases = [
        (
            "range.toml",
            "format_version = 1\n[predation]\nsize_ratio = 99.0\n",
            "predation.size_ratio",
        ),
        (
            "unknown.toml",
            "format_version = 1\n[food]\nsugar = 1.0\n",
            "sugar",
        ),
        ("version.toml", "format_version = 7\n", "format_version"),
        (
            "syntax.toml",
            "format_version = 1\n[world\n",
            "invalid scenario",
        ),
    ];
    for (name, text, needle) in cases {
        let path = dir.join(name);
        fs::write(&path, text).expect("write scenario");
        let output = headless(&[
            "run",
            "--scenario",
            path.to_str().expect("UTF-8"),
            "--seed",
            "1",
            "--ticks",
            "5",
        ]);
        assert_failure_mentions(&output, needle);
        assert_failure_mentions(&output, name);
    }
}

/// T-HL-4 (should): the same command twice gives identical JSON apart from timing.
#[test]
fn t_hl_4_json_is_identical_apart_from_timing() {
    let dir = test_dir("t_hl_4");
    let run = |name: &str| {
        let path = dir.join(name);
        assert_success(&headless(&[
            "run",
            "--seed",
            "5",
            "--ticks",
            "90",
            "--out",
            path.to_str().expect("UTF-8"),
        ]));
        fs::read_to_string(&path)
            .expect("JSON")
            .lines()
            .filter(|line| !line.contains("elapsed_seconds"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(run("a.json"), run("b.json"));
}

#[test]
fn json_destination_is_protected_like_the_csv() {
    let dir = test_dir("json_protect");
    let path = dir.join("run.json");
    fs::write(&path, "keep\n").expect("seed");
    let output = headless(&[
        "run",
        "--seed",
        "1",
        "--ticks",
        "3",
        "--out",
        path.to_str().expect("UTF-8"),
    ]);
    assert_failure_mentions(&output, "--overwrite");
    assert_eq!(fs::read_to_string(&path).expect("read"), "keep\n");
    assert_success(&headless(&[
        "run",
        "--seed",
        "1",
        "--ticks",
        "3",
        "--out",
        path.to_str().expect("UTF-8"),
        "--overwrite",
    ]));
}

/// M-18: a tiny benchmark run exits 0 and writes its JSON report.
#[test]
fn bench_writes_a_report() {
    let dir = test_dir("bench");
    let path = dir.join("bench.json");
    let output = headless(&[
        "bench",
        "--tiers",
        "c",
        "--reps",
        "1",
        "--warmup",
        "1",
        "--ticks",
        "2",
        "--out",
        path.to_str().expect("UTF-8"),
    ]);
    assert_success(&output);
    let json = fs::read_to_string(&path).expect("report");
    assert!(json.contains("\"tier\": \"b-c\""), "{json}");
    assert!(json.contains("\"mode\": \"frozen\""));
    assert!(text(&output.stdout).contains("ecological"));
}
