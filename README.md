# Natural Selection Simulator

A Rust desktop application that simulates natural selection among prey and predators on procedurally generated 2D terrain, for classroom demonstration and simple, repeatable experiments. The scope of record is [SCOPE.md](SCOPE.md) (version 2.2).

- **Model and limitations:** [docs/MODEL.md](docs/MODEL.md)
- **Scenario file format:** [docs/SCENARIO_FORMAT.md](docs/SCENARIO_FORMAT.md)
- **Benchmarks:** [docs/BENCHMARKS.md](docs/BENCHMARKS.md)

## Quick start

From the project folder, in PowerShell on Windows or in a WSL2 terminal:

```sh
cargo run -p app --release                 # the desktop app, opening the baseline classroom preset
cargo run -p headless --release -- --help  # the command-line runner
```

The first release build takes several minutes.

In WSL2, if the window fails with `NoCompositor`, run `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir cargo run -p app --release` (see [WSLg note](#wslg-note)).

## Current status

Implemented on 2026-11-12, ahead of the weekly plan. Automated tests pass; the manual and reference-machine checks listed under [Pending checks](#pending-checks) have **not** been performed yet.

| Feature | Status |
| --- | --- |
| M-01 Workspace | Done |
| M-02 Deterministic core | Done. The complete state hash covers every organism column, food, counters, the route queue, the terrain checksum and generator positions. T-DET-1, 2, 4; T-RNG-1; T-FT-1 to 3 |
| M-03 Organisms, traits, energy | Done, for both species. Safety-cap birth suppression with a UI warning. T-EN-1, T-EN-3, T-CAP-1 |
| M-04 Reproduction and inheritance | Done. T-REP-1 to 3, T-INH-1, 2, 4 (should: T-INH-3, T-INH-5, T-REP-4) |
| M-05 Predators | Done. T-PRED-1 to 3, T-PRED-6, T-SEL-1 (should: T-PRED-4, 5, 7) |
| M-06 Terrain | Done: six classes, fertility, speed multipliers, impassable cells, wall sliding, ASCII test maps. T-TF-1, T-TF-2, T-TM-1, T-IMP-1 (should: T-TF-3, T-TF-4, T-TM-2, T-DET-3) |
| M-07 Food grid | Done |
| M-08 Spatial grid | Done. T-GRID-1 |
| M-09 Navigation | Done: reachability labels, blocked-path check, windowed A*, route cache, per-tick budget and queue, stuck recovery. T-NAV-1, 2, 4, 5, 6, T-NAV-7a (tier C, always run). The full tier-M T-NAV-7 is `#[ignore]`d (run weekly) |
| M-10 World view | Done: terrain and food texture, prey circles, predator triangles, legend, pan, zoom toward the cursor, fit-to-map |
| M-11 Controls | Done: play, pause, step, reset, new seed, 1×/2×/5×/10×/Max, achieved speed and backlog |
| M-12 Inspector | Done: click picking through the spatial grid, highlight ring, details panel, "no longer alive". T-INSP-1 (should: T-INSP-2) |
| M-13 Live charts | Done: population and selectable trait means per species, generation readouts, births, deaths and predations, extinction notices. T-CHART-1 (should: T-CHART-2, 3) |
| M-14 Parameter UI | Done: simple controls, collapsed Advanced set, apply on Reset, read-only while running, validation messages. T-VAL-1 (should: T-VAL-2) |
| M-15 Presets | Done: presets embedded from `presets/` by `app/build.rs`; baseline (tuned, meets E2's counted conditions on seeds 1 to 5) and famine. T-SCN-4 |
| M-16 Scenario files | Done: TOML load and save, scenario browser and typed path, `format_version`, validation naming the field. T-SCN-1 to 3 |
| M-17 `headless run` | Done: `--scenario`, JSON `--out` with every X4 field and per-species births, deaths and predations. T-HL-1, T-HL-3, T-HL-6 (should: T-HL-4) |
| M-18 Performance | `headless bench` (frozen and ecological modes, JSON, phase timers with `--features profile`) and the in-app overlay (F3) are done. Development-machine results are in [BENCHMARKS.md](docs/BENCHMARKS.md); reference-machine measurements are pending |
| M-19 Builds | WSL2 build and tests done. Windows build and run on the reference machine pending |
| M-20 Documentation | Done: MODEL.md, SCENARIO_FORMAT.md, BENCHMARKS.md |
| M-21 CSV export | Done; predator, birth, predation and generation columns appended after the fixed Week 1 columns |
| S-01 Famine | Done (scenario block, UI and preset) |
| S-02 Old age | Done; off by default |
| S-03 Generation chart | Done (trait chart "Generation") |
| S-04 About panel | Done |
| S-05 Deaths by cause | Done (inspector panel and JSON) |
| S-08 Camera follow | Done ("Follow" in the inspector) |
| S-12 Remaining Advanced UI | Done ("More parameters") |
| C-01 Copy-and-paste scenarios | Done (Scenarios… window, "Share as text") |
| C-02 Experiment mode | Done (`headless experiment --seeds 1-5`) |
| C-05 Color by trait | Done ("Color" menu) |

Should-haves and could-haves were built in this one-session push even though SCOPE's Section 4 start conditions are tracked against a reserve that this session did not account for. Treat them as implemented but subject to the usual review.

## Desktop application

```sh
cargo run -p app --release
cargo run -p app --release -- --scenario presets/famine.scenario.toml --seed 7

# Save a picture of the window after 30 s at 10× and exit (for documentation and checks)
cargo run -p app --release -- --play --speed 10 --screenshot shot.png --after 30
```

The app opens the **baseline classroom preset**, paused at tick 0, in a 1600 × 900 window.

**Layout (SCOPE 7.3):**

- **Control bar (top):**
  - preset picker;
  - Play/Pause, Step, Reset, New seed;
  - speeds 1×, 2×, 5×, 10× and Max;
  - Fit map, Color (species, or a trait: C-05), Food overlay;
  - Scenarios…, About, Perf.
- **Parameters (left):**
  - simple controls: starting prey and predators, food abundance;
  - a collapsed **Advanced** set: starting trait means and spreads, mutation, size ratio, terrain seed and water level;
  - a collapsed **More parameters** section with everything else;
  - the run seed.

  Edits apply on **Apply and reset** (or Reset). The panel is read-only while running. Ranges come from `sim::limits`, and invalid combinations are listed in red.
- **Inspector (right):**
  - the clicked organism: species, ID, generation, age, energy bar, goal, terrain, and its traits beside the species mean;
  - population numbers: alive, mean and maximum generation, births, deaths by cause, predations;
  - the legend.
- **Charts (bottom):** population per species, and a selectable chart of mean speed, size, sense range or generation, against simulated minutes.
- **Status bar:** run state; requested and achieved speed; tick, simulated time and seed; the state hash (shown while paused); the backlog when behind; extinction notices; the population-cap warning.

**Controls:**

| Input | Effect |
| --- | --- |
| Space | Play / pause |
| Period (.) | Step one tick |
| Left click | Inspect the nearest organism (Esc clears) |
| Right- or middle-drag, WASD, arrows | Pan |
| Mouse wheel, + / − | Zoom (the wheel zooms toward the cursor) |
| Home | Fit the whole map |
| F3 | Performance overlay |

**Scenarios… window (M-16):**

- Edit the name, description and learning goal, and optionally store the run seed.
- See the `.toml` files in `scenarios/`, or type any path.
- **Load** opens a file; an invalid file leaves the current run untouched and names the bad field.
- **Save current parameters** writes versioned TOML, including unapplied changes.

**Rendering:**

- **Terrain and food:** one texture, one pixel per cell. Each class blends from a grazed color to a full-food color. The food overlay is refreshed every 15 frames while running.
- **Organisms:** a reused sprite pool, positioned each frame from `sim`'s slices. Prey are discs and predators are triangles pointing along their heading, both scaled by the size trait.
- **egui:** runs on its own full-window camera; the world camera gets the area between the panels (the bevy_egui side-panel pattern).

### WSLg note

If `cargo run -p app` panics with `Failed to build event loop ... WaylandError(Connection(NoCompositor))`, the WSLg Wayland socket link is missing from `/run/user/1000`, which can happen after a WSL restart. Run with `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir cargo run -p app --release`. WSL2 renders in software (`llvmpipe`), so expect a lower frame rate than on Windows.

## Headless runner

```sh
# Baseline preset (built in), 10 simulated minutes, summary only
cargo run --release -p headless -- run --seed 1 --ticks 18000

# A scenario file, JSON results and a CSV row per simulated second
cargo run --release -p headless -- run --scenario presets/famine.scenario.toml --seed 1 --ticks 18000 \
    --out results.json --sample-every 30 --csv results.csv

# Benchmark suite (SCOPE 10.6); see docs/BENCHMARKS.md
cargo run --release -p headless -- bench --reps 3 --out bench/results/<date>-<commit>.json

# Experiment mode (C-02): one scenario, several seeds, a row per seed and a summary
cargo run --release -p headless -- experiment --seeds 1-5 --ticks 180000 --out e2.json
```

| Option | Meaning |
| --- | --- |
| `--scenario <PATH>` | Scenario file; default the built-in baseline preset (the same one the app opens, so both print the same hash) |
| `--seed <U64>` | Run seed (required) |
| `--ticks <U64>` | Fixed ticks to run (required; 0 is valid) |
| `--out <PATH>` | JSON results: scenario name and format version, run and terrain seeds, ticks, final metrics per species (population, means, generations, births, deaths by cause, extinction tick), predations, food, state hash, elapsed time |
| `--sample-every <TICKS>` and `--csv <PATH>` | CSV time series; a row at tick 0, every interval and the final tick |
| `--overwrite` | Allow `--csv` and `--out` to replace existing files |

`headless experiment --seeds <list> --ticks <n> [--scenario <file>] [--out <json>]` runs one scenario once per seed (`1-5`, `1,4,9`, or both). It prints a row per seed (populations, maximum generation, births, predations, prey speed, predator extinction, hash) and writes per-run results with a summary (mean, minimum and maximum of each value; how many runs lost each species) as JSON. Each run is exactly `headless run` with that seed.

- **Exit status:** 0 on success; 1 for a failed run (invalid scenario, unreadable file, file destination problem), with the message naming the field or path; 2 for usage errors.
- **CSV columns:**
  - The 11 fixed Week 1 columns come first, unchanged and in order: `tick, sim_seconds, prey_population, avg_speed, avg_size, avg_sense, avg_energy, total_food, cumulative_food_consumed, cumulative_starvation_deaths, state_hash`. They hold prey values; `cumulative_starvation_deaths` counts prey.
  - Appended after them: `predator_population, avg_predator_speed, avg_predator_size, avg_predator_sense, avg_predator_energy, cumulative_prey_births, cumulative_predator_births, cumulative_predations, cumulative_predator_starvation_deaths, prey_max_generation, predator_max_generation`.
- **Formatting and file safety** are unchanged from Week 1:
  - shortest round-trip decimals, no exponent, locale-independent;
  - empty fields for missing averages;
  - a fixed-width `0x` hash;
  - writes go through a `.partial` file;
  - an existing file is never replaced without `--overwrite`.

## Baseline balancing record (M-15, E2)

The baseline preset was tuned with `headless` runs (P values; Q13). The changes from the SCOPE 8 illustrative values:

- energy: `base` 0.03, `size` 0.04, `move` 0.04, `sense` 0.003;
- predator speed mean 1.6;
- predation: efficiency 0.8, content 15;
- food: regrowth 0.001, starting fraction 0.5;
- terrain: water level 0.25, rock level 0.88;
- start counts: 2,000 prey and 150 predators.

Earlier candidates and why they were rejected:

- **SCOPE 8's illustrative values:** predators starved within 24 s, because a 1.2 versus 1.0 speed edge closes a chase at only 0.2 units/s.
- **`move` 0.06:** predators persisted 25 to 38 simulated minutes.
- **Higher capture efficiency, or cheaper reproduction:** predators died out sooner than with the adopted values.

**E2 counted conditions:** `headless run --seed N --ticks 180000` (10 minutes at 10×), seeds 1 to 5, with the final code and preset:

| Seed | Crash / cap | Prey max generation | Prey births | Successful predations | Predators extinct at | Final state hash |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | no / no | 239 | 458,109 | 293,236 | survived | `0xdbe5d284ce82f37e` |
| 2 | no / no | 167 | 252,875 | 38,595 | 62.6 min | `0xf55c0fadf5080432` |
| 3 | no / no | 162 | 242,324 | 47,116 | 58.7 min | `0x669fe51b6384084a` |
| 4 | no / no | 167 | 297,630 | 38,451 | 44.9 min | `0x5fa44170f8522d15` |
| 5 | no / no | 164 | 241,614 | 38,939 | 46.9 min | `0x6b7512df28d6bbbd` |

All five seeds meet the counted conditions:

- no crash and no safety cap;
- prey generation ≥ 5;
- prey births and deaths recorded;
- successful predation recorded.

**What a run shows:**

- An arms race while predators exist: mean prey speed rises from about 1.0 to about 1.5, and predator speed from 1.6 to about 2.7.
- In four of the five seeds, predators die out after about 45 to 63 simulated minutes; in seed 1 they persist through all 100 minutes.
- After that, relaxed selection: prey sense range falls toward the minimum, and prey shrink.
- Prey numbers then grow to about 10,000 by minute 100, so achieved speed late in a 10× run depends on the machine.

The chart and inspector parts of E2 need the manual teacher-flow check.

## Pending checks

These have **not** been performed:

- **Manual checklist:** T-CHART-4, T-INSP-3, T-CAM-3, T-VAL-3 and the teacher-flow check (E1, E2's chart and inspection parts), in the app.
- **Interactive check of this version of the app** in WSL2 and on Windows. It builds and its library tests pass, and three automatic screenshots taken in WSL2 (startup, 10×, an inspected organism) showed every panel, the map and both charts rendering. Nobody has used it by hand yet.
- **Windows build and run on the reference machine:** T-BUILD-3, T-SHARE-1, and T-DET-7 (should).
- **Reference-machine benchmarks and soak test:** T-PERF-1 (reference part), T-PERF-2, T-PERF-3.
- **T-BUILD-4:** the README steps on a clean checkout.

## Workspace structure

| Path | Kind | Role |
| --- | --- | --- |
| `sim/` | library | All simulation rules. No graphics, UI, windowing, file I/O or clock (the clock only behind the `profile` feature, for benchmark timers). |
| `app/` | binary + library | Desktop application (Bevy + egui). Window-free logic and the egui panels live in its library. `app/build.rs` embeds `presets/`. |
| `headless/` | binary + library | Command-line runner: `run` (summary, JSON, CSV) and `bench`. No Bevy or egui. |
| `presets/` | data | Embedded scenario presets: `baseline.scenario.toml`, `famine.scenario.toml`. |
| `bench/scenarios/`, `bench/results/` | data | Benchmark tiers and recorded results. |
| `docs/` | docs | MODEL.md, SCENARIO_FORMAT.md, BENCHMARKS.md. |

Architecture invariants for contributors and Claude Code sessions are in [CLAUDE.md](CLAUDE.md).

## Build and test (WSL2)

Requires [rustup](https://rustup.rs). The toolchain in `rust-toolchain.toml` is selected automatically.

```sh
cargo test --workspace -j 2
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace -j 2
cargo test -p sim --release -- --ignored    # weekly: the full tier-M navigation run (T-NAV-7)
```

The suite has **259 tests** plus one ignored weekly test, and runs in about 40 s once compiled:

- 164 in `sim`: 142 unit and 22 integration (determinism 2, navigation 6 plus 1 ignored, predation mechanisms 5, scenarios 5, terrain 4).
- 49 in `headless`: 32 unit and 17 process-level.
- 46 in `app`: library tests for the driver, session, camera, rendering, presets, charts, scenario files, frame statistics and T-DET-4.

To run only the tests for one SCOPE area:

```sh
cargo test -p sim --test determinism   # T-DET-1, T-DET-2
cargo test -p sim --test predation     # T-PRED-6, T-SEL-1
cargo test -p sim --test nav           # T-NAV-1, 2, 4, 5, 6, 7a
cargo test -p sim --test terrain       # T-TF-1, T-IMP-1, T-DET-3, T-TF-4
cargo test -p sim --test scenario      # T-SCN-2, T-SCN-4, T-DET-5, T-INH-5
cargo test -p sim t_rep_ t_inh_        # reproduction and inheritance
cargo test -p sim t_pred_              # predation unit tests
cargo test -p sim config::             # T-SCN-1, T-SCN-3, T-VAL-1, T-VAL-2
cargo test -p sim t_grid_1             # spatial grid
cargo test -p app                      # app library: driver, session, camera, rendering, presets, charts, scenario files, T-DET-4
cargo test -p headless                 # CLI, JSON, CSV, bench
```

**Memory note (WSL2):** the `app` executables link all of Bevy with debug info, about 1 GB each, and linking one needs several GB of RAM. Two such links at once crashed the 7.7 GB WSL2 VM. For that reason T-DET-4 is a unit test inside the `app` library (`app/src/t_det_4.rs`), not an integration test, and the `app` binary has `test = false`. If memory is tight, limit parallel jobs (`-j 2`).

**Dependency-boundary check (T-HL-6):** neither command below should list any `bevy` or `egui` crate.

```sh
cargo tree -p sim
cargo tree -p headless
```

## Decisions

| Date | Decision |
| --- | --- |
| 2026-11-10 | Rust toolchain pinned to 1.99.1 (`rust-toolchain.toml`). |
| 2026-11-10 | Bevy pinned to `=0.20.1`, with `default-features = false` and the `2d` feature collection. |
| 2026-11-10 | bevy_egui pinned to `=0.43.1` (requires bevy ^0.20 and egui ^0.36). |
| 2026-11-10 | egui pinned to `=0.36.2`, the egui version dictated by bevy_egui 0.43. |
| 2026-11-10 | egui_plot pinned to `=0.37.0` (requires egui ^0.36). egui_plot 0.38.0 requires egui 0.37 and is incompatible with bevy_egui 0.43. |
| 2026-11-10 | Rust 2024 edition: all four pinned UI crates are 2024-edition, and Bevy 0.20 example code uses 2024-only syntax such as let-chains. |
| 2026-11-10 | RNG: `rand_chacha =0.10.0` (`ChaCha8Rng`) with `rand_core =0.10.1`, default features off (no OS entropy). Chosen over `rand_pcg` because ChaCha output is documented as portable and its public `get_word_pos()` lets the state hash cover generator state without unsafe code. |
| 2026-11-10 | Random-domain seeding: each domain's 32-byte key is four SplitMix64 outputs from `base_seed XOR domain_constant`, where each domain constant is its 8-byte ASCII name read little-endian. Terrain uses the resolved terrain seed; initial population, behavior and mutation use the run seed. An omitted terrain seed is derived from the run seed (SCOPE 6.10). Details are in `sim/src/rng.rs`. |
| 2026-11-10 | Superseded (2026-11-12, maximum 240): Tick rate (provisional): minimum 1, default 30 ticks per simulated second. No maximum, because SCOPE.md defines none (Q5, end of Week 3); this is a known gap in I-07 until then. |
| 2026-11-10 | Superseded (same day): `run_seed` was a `ScenarioConfig` field. |
| 2026-11-10 | Run identity is separate from the scenario: `Simulation::new(&ScenarioConfig, run_seed)` (SCOPE 8). `ScenarioConfig` holds model and environment settings only. The resolved terrain seed is `ScenarioConfig::resolved_terrain_seed(run_seed)`: the explicit terrain seed if set, otherwise derived from the run seed by the existing rule. |
| 2026-11-10 | Seed-derivation compatibility vectors (run seed 1, no terrain seed) are frozen in `sim/src/lib.rs`. Changing them changes every deterministic run and needs a recorded decision. |
| 2026-11-10 | Stepping driver (`app/src/driver.rs`) uses an integer accumulator (`real ns × ticks per second × multiplier`), so frame deltas summing to exactly 1 s at 1× give exactly 30 ticks. Limits are SCOPE 7.3's provisional values: 600 ticks or 25 ms per frame, 12 ms in Max mode, 250 ms stall cap. The driver reads no clock; the app passes frame deltas and a work-elapsed closure. |
| 2026-11-10 | Achieved speed for the status bar is averaged over a 1 s real-time window (P); SCOPE defines no window for it (the overlay's 60 s statistics are M-18). Step leaves the driver paused; Reset keeps the paused or running state; the driver starts paused at 1×. |
| 2026-11-10 | Superseded (2026-11-12, serde and toml added): Serde is deferred until a TOML parser exists (M-16, M-17). `ScenarioConfig` uses `Default` from the `limits` module for now. |
| 2026-11-10 | `libm =0.2.16` pinned with default features off (portable software implementations) for `ln`, `sqrt` and `cos` in sampling (SCOPE 6.10). |
| 2026-11-10 | M-03 provisional limits (SCOPE gives no numbers; all P, in `sim/src/limits.rs`). World size 16–4,096 (default 512). Prey start count 0–150,000 (default 2,000). Start energy fraction 0.01–1 (default 0.5). Trait ranges: speed 0.1–5, size 0.2–5, sense 1–50. Spreads 0 to the range width. Energy coefficients: capacity 0.1–1,000; base, size, move and sense 0–10. Defaults follow SCOPE 8's illustrative scenario. |
| 2026-11-10 | Superseded (2026-11-12, counts or densities): Starting prey are given as a count only; SCOPE's density option needs the terrain grid and arrives with M-06. Zero starting prey is allowed (an empty, valid run). |
| 2026-11-10 | Superseded in part (2026-11-12, paths follow the SCOPE 8 layout, e.g. `species.prey.speed.mean`): `ValidationError` now carries the field path as a `String` (for example `prey.speed.mean`), and adds `AboveMaximum`, `NotFinite` and `OutOfRange` variants. |
| 2026-11-10 | The temporary M-02 behavior-domain probe is removed. Stepping now draws no random numbers; T-DET-2 compares real prey state. |
| 2026-11-10 | Movement energy uses realized effort: executed step ÷ (max distance per tick × terrain multiplier), clamped to [0, 1]. This refines SCOPE 6.3's per-behavior effort level and keeps its terrain rule. It affects only T-EN-2 (should-priority). Wandering at `wander_speed_fraction` 0.5 reproduces SCOPE's 0.5 wandering effort. |
| 2026-11-10 | M-07 and M-09 provisional values (in `sim/src/limits.rs`): cell size 1–16 (default 2); `max_per_cell` 0.01–100 (default 1); `regrowth_per_tick` 0–100 (default 0.002); `abundance` 0–10 (default 1); `initial_fraction` 0–1 (default 1, which SCOPE does not specify); `intake_per_tick` 0–100 (default 0.05); `energy_per_unit` 0.01–100 (default 1); `decision_interval` 1–300 (default 10); `wander_speed_fraction` 0–1 (default 0.5). |
| 2026-11-10 | Superseded (2026-11-12, cell ranking kept with reachability filters): Week 1 food seeking uses SCOPE's Week 1 rule (richest cell in sense range), not the region score of SCOPE 6.8, which needs the spatial grid (M-08). |
| 2026-11-10 | Superseded (2026-11-12, T-DET-1 uses the baseline model on 160 × 160): T-DET-1 runs on a smaller fixed scenario (300 prey, 128 × 128 world) so the must-priority suite stays fast in debug builds; T-DET-2 still uses the default scenario. |
| 2026-11-10 | Known precision limit: positions are `f32`. At x = 2,048 adjacent `f32` values are about 2.4 × 10⁻⁴ apart, while the slowest step is 0.0033 units per tick, so a single step can round by several percent. This is deterministic but coarse; revisit (for example with `f64` positions) if it shows up in tier G profiling or behavior. |
| 2026-11-10 | SCOPE.md updated to version 2.2: the minimal CSV export becomes Week 1 must-have M-21 (2.5 h, from reserve; totals 58.5 h implementation and 16.5 h reserve). C-04 now covers only advanced CSV and experiment export. If Week 1 runs short, graphical work is reduced to the minimum view before M-21 is cut. |
| 2026-11-10 | `clap =4.6.7` (derive) pinned for the `headless` command line (SCOPE 7.1). CSV is written with the standard library; no CSV crate is needed for the fixed numeric schema. |
| 2026-11-10 | Superseded (2026-11-12, JSON `--out` added): `headless run` prints its summary and elapsed time. SCOPE's JSON `--out` and T-HL-1 wait for scenario files (M-16), because the JSON must name the scenario and its format version. |
| 2026-11-10 | CSV files are written via `<name>.partial` and renamed into place; without `--overwrite` the destination is reserved with an exclusive create. Exit status 2 for usage errors, 1 for run failures. |
| 2026-11-10 | `cumulative_food_consumed` (`f64`, food units actually removed from cells) added to `sim` state and the state hash; feeding behavior is unchanged. |
| 2026-11-10 | T-DET-4 is an `app` library unit test rather than an integration test, to avoid a second ~1 GB Bevy link that exhausted WSL2 memory. |
| 2026-11-10 | T-CSV-10 passed manually in Microsoft Excel on the Windows reference machine, so M-21 is complete. T-HL-1 (scenario-aware JSON output) is deferred until M-16, because scenario identity and format version do not exist yet. |
| 2026-11-10 | Superseded (2026-11-11): the Week 1 app used the shared default scenario with no demonstration configuration. |
| 2026-11-10 | Max-mode emergency ceiling: `DriverLimits::max_mode_tick_ceiling` = 100,000 ticks per frame (P). It stops an infinite loop if the work timer never advances; the 12 ms work budget remains the real limit. |
| 2026-11-10 | Superseded in part (2026-11-12, the wheel now zooms toward the cursor): Camera (P): zoom from 0.01 to 16 world units per pixel in ×1.1 wheel steps, about the view center; fit margin 1.05; keyboard pan 600 pixels per second; the view center is clamped to the world. |
| 2026-11-10 | The app reads frame time from `Time<Real>`, not the default clock, so the driver's 250 ms stall cap and stall counter see real stalls. The `app` binary has `test = false` to avoid a second ~1 GB Bevy link during tests. |
| 2026-11-11 | Target reconsideration (Week 1 behavior correction, M-09): a food target is kept only while it holds at least one useful bite (`MIN_USEFUL_TARGET_BITES` = 1 × `intake_per_tick`, P) and at least `TARGET_SWITCH_FRACTION` = 0.75 (P) of the best useful cell in range. Cells under one bite are no longer candidates. Ties still go to more food, then nearer center, then lower index; no random numbers are drawn. Root cause: prey sat still indefinitely in cells that regrew to the cap. This changes every run's trajectory and hash. |
| 2026-11-11 | Superseded (2026-11-12, the app opens the baseline preset): Week 1 app demonstration scenario (`app::session::app_scenario()`): `ScenarioConfig::default()` with `food.initial_fraction` = 0.25 and `food.regrowth_per_tick` = 0.0005 (P), run seed 1. The shared default and `headless run` are unchanged. Measured with the fix: regrowth alone still left nearly all prey still after about 15 s. Regrowth plus lower starting food kept prey travelling for about 30 s, and afterwards 6–13% made moves over 6 units in each 15 s. The proposed `abundance` = 0.75 added little, and `start_count` = 1,000 reduced movement, so neither is used. |
| 2026-11-12 | `serde =1.0.229` (derive) and `toml =1.2.1` pinned for `sim` scenario files (M-16, SCOPE 7.1). Both versions were already in `Cargo.lock` through Bevy, so nothing new was downloaded. `serde_json` is not used: `headless` writes its JSON with a small in-crate writer. |
| 2026-11-12 | `[profile.dev.package.sim] opt-level = 3`: `sim` is optimized in dev and test builds (debug assertions and overflow checks stay on) so multi-thousand-tick tests and the debug app stay fast. Release builds are unaffected. |
| 2026-11-12 | Scenario files follow SCOPE 8's layout exactly (`[terrain] seed`, `[species.prey]`, `[species.predator]`, `[predation]`, `start = { count | density }`). `format_version` is required and checked first; unknown fields are rejected by name; a present species table must be complete. Seeds in files are at most `i64::MAX` (TOML integers). |
| 2026-11-12 | Q5 working answer: `ticks_per_second` maximum 240 (P), so T-VAL-1 covers every integer parameter. All new parameters have min, max and default in `sim::limits` (I-07); `MAX_GRID_CELLS` = 2,048² limits world size against cell size. |
| 2026-11-12 | Terrain (M-06): in-crate fractal value noise (four octaves, smoothstep), no noise crate. Elevation is ranked, so `water_level` and `rock_level` are shares of the map. Six classes (P): water and rock impassable; sand (speed 0.8, fertility 0.15); grassland (1, 1); forest (0.6, 0.6); hills (0.45, 0.3). Reachability labels use 4-neighbor flood fill, matching A*'s no-corner-cutting rule. |
| 2026-11-12 | Starting positions: one 64-bit draw picks a passable cell uniformly and one gives the in-cell offset, so there are still eight draws per organism. Prey are created first, then predators. |
| 2026-11-12 | Reproduction (M-04): a parent reproduces only if it keeps positive energy after paying. Offspring placement (a uniform point within `spawn_radius`, or the parent's spot if impassable) draws from the mutation domain, as part of the reproduction event; every birth takes exactly 11 draws whatever the parameters. At the cap, suppressed births draw nothing. |
| 2026-11-12 | Deaths: a per-tick flag ensures each organism is removed once; capture takes precedence over starvation in the same tick. Deaths are counted by cause and species (fixes review finding #1). |
| 2026-11-12 | Food seeking keeps the cell ranking (richest useful cell, then nearer, then lower index) instead of SCOPE 6.8's region score, now filtered to the same reachability region and away from targets marked unreachable. A full cell always counts as useful even when one bite exceeds its cap (fixes review finding #4). |
| 2026-11-12 | Navigation details (P): routes are smoothed by string pulling that never crosses slower terrain. A seeker re-plans when the segment to its next waypoint is blocked; a hunter when its prey ends up more than `drift_cells` from the route's end (at most every `hunt_replan_interval` ticks). Stuck progress is measured along the remaining route length, so walking around a wall counts as progress. A blocked wanderer turns by 2 radians (no random draw). |
| 2026-11-12 | T-NAV-7a's stuck state is defined as being blocked by terrain for at least 30 consecutive ticks (`sim::STUCK_REPORT_TICKS`, P) while seeking or hunting. Fleeing prey pressed into a corner do not count. |
| 2026-11-12 | Phase 1 (rebuilding the spatial grid) runs at the end of each tick and at construction, so between ticks the grids match stored positions and serve picking. The prey grid is rebuilt again after movement for capture. |
| 2026-11-12 | The state hash covers all state: every organism column (the column macro hashes each), counters by cause, food consumed, extinction ticks and flags, the route queue, the food grid, a terrain checksum, seeds and generator positions. The chart history and per-tick scratch are not hashed. |
| 2026-11-12 | The app opens the baseline classroom preset (embedded by `app/build.rs` from `presets/`), and `headless run` defaults to the same file, so the app and `headless` share one scenario (X2, T-DET-4; fixes review finding #3). The Week 1 demonstration overrides are removed. |
| 2026-11-12 | The app computes the state hash at most once per tick and shows it only while paused, and refreshes the food texture every 15 frames while running (SCOPE 7.3), so no full-state work runs every frame (fixes review finding #2). Organisms use a reused sprite pool. |
| 2026-11-12 | Baseline balancing (Q13, P): see the Baseline balancing record. The SCOPE 8 illustrative coefficients made predators starve within half a minute. The tuned preset meets E2's counted conditions on seeds 1 to 5, and predators die out after about 45 to 60 simulated minutes in an evolutionary arms race. |
| 2026-11-12 | CSV: 11 columns appended after the fixed Week 1 columns (predator population and means, births, predations, predator starvation, maximum generations). The Week 1 columns are unchanged and still describe prey. |
| 2026-11-12 | App "New seed" picks a seed from 1 to 999,999 using the clock; the app may read the clock, and the seed is shown and reproducible. |
| 2026-11-12 | Should-haves S-01, S-02, S-03, S-04, S-05, S-08, S-12 and could-haves C-01, C-02, C-05 were implemented in the same push as the remaining must-haves, at the user's request to cover as much of SCOPE as possible. SCOPE Section 4's reserve-based start conditions were not formally evaluated; review them like any other new code. |
