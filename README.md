# Natural Selection Simulator

A Rust desktop application that simulates natural selection among prey and predators on procedurally generated 2D terrain, for classroom demonstration and simple, repeatable experiments. The scope of record is [SCOPE.md](SCOPE.md) (version 2.2).

## Current status

M-01 (workspace foundation) is complete. M-02 (deterministic core) is complete for Week 1:

- `sim`: configuration, four random domains, fixed tick pipeline, state hash (T-DET-1, T-DET-2, T-RNG-1).
- `app`: pure stepping driver in `app/src/driver.rs` (T-FT-1 to T-FT-3), not yet connected to a Bevy window.
- **T-DET-4 is complete:** two driver frame sequences (one capped with a backlog), the `headless run` path, the `headless` binary and direct stepping all reach the same state at tick 1,000.
- **Week 2 (SCOPE 6.10):** the state hash must keep growing to cover all state as later columns arrive; T-DET-1 and T-DET-2 are then rerun.

M-03 (prey organisms) is complete for prey only (T-EN-1, T-EN-3, T-CAP-1). The safety cap's birth suppression arrives with births (M-04); for now 150,000 is only the maximum starting count.

M-07 (food grid, regrowth and feeding) is complete with uniform fertility. The Week 1 part of M-09 is complete: prey sense the richest useful food cell in range, move straight toward it in an open world, otherwise wander. At each decision they keep their current target only while it is still worthwhile (see [Food and seeking](#food-and-seeking-as-implemented-m-07-week-1-part-of-m-09-provisional)). Prey that find enough food survive; prey that do not still starve.

**Current limitations:**

- No terrain yet: every cell has fertility 1, and the terrain speed multiplier is fixed at 1.0 (`behavior::terrain_speed_multiplier`) until M-06.
- No barriers, routes or spatial grid. Food seeking uses the Week 1 rule (richest cell), not SCOPE 6.8's region score, which needs M-08.
- No reproduction, so a run can only lose prey.
- Contention for a shared cell favors lower storage indices.

The M-17 Week 1 minimum (`headless run` with a printed summary) is complete. M-21 (minimal CSV export) is complete: T-CSV-1 to T-CSV-10 pass. See [Headless runner and CSV export](#headless-runner-and-csv-export-m-17-m-21).

- **T-CSV-10 passed** (manual, Microsoft Excel on the Windows reference machine; Q15 answered with its working default). The 18,000-tick example opened as 11 separate columns, with 601 data rows plus the header.
  - `tick` ran from 0 to 18000 and `sim_seconds` ended at 600.
  - Decimal metric fields were recognized as numbers.
  - `state_hash` showed its complete hexadecimal value, and sorting that column showed no truncation or precision loss.
  - The average trait columns were constant across the run. This is expected: without reproduction or mutation, the population's traits do not change.
- **T-HL-1 is deferred until M-16.** SCOPE's full M-17 includes scenario-aware JSON output (`--out`), which must name the scenario and its format version. Neither exists until scenario files arrive in M-16.

The minimum Week 1 graphical application (M-10, M-11) is implemented. It shows prey and food, has Play, Pause and Reset, and supports pan and zoom; see [Desktop application](#desktop-application-m-10-m-11-week-1-minimum).

- **WSL2:** the window opened and ran for 25 s without errors, using software rendering (`llvmpipe`). The visual and interactive checks are **pending**.
- **Native Windows:** not tested yet.

### Prey model as implemented (M-03, provisional)

- **Storage:** `sim::Organisms` holds one vector per column: `id` (`u64`); `x`, `y`, `energy` (`f32`); `age` in ticks and `generation` (`u32`); `speed`, `size`, `sense` (`f32`); `heading` (`f32`, radians); `target` (`u32` food cell, or `NO_TARGET`).
  - Energy capacity is derived from size and not stored. Movement effort is recomputed every tick and not stored.
  - IDs are assigned in creation order from 0 and never reused.
  - The starting population is generation 0.
- **Numeric types (P):** `f32` for per-organism continuous values; `u32` for age and generation.
- **Energy formula (SCOPE 6.3):** `E_max = c_capacity × size`. Cost per tick is `(c_base + c_size × size + c_move × effort × size × speed² + c_sense × sense) / ticks_per_second`.
  - Units are abstract and the model is qualitative.
  - Effort is the **realized movement effort** of the tick: executed step length ÷ (`speed / ticks_per_second` × terrain speed multiplier), clamped to [0, 1].
  - A standing prey pays no movement cost. A full step costs effort 1, and wandering at half speed costs effort 0.5.
  - When a world edge cuts a step short, effort is measured from the actual change in position.
  - When terrain arrives, full effort on slow terrain still costs the same per tick while covering less ground, so slow terrain costs more energy per unit distance.
- **Starvation:** the energy phase clamps energy at 0. The aging phase increments every age, then marks organisms with energy ≤ 0. Phase 9 removes marked organisms in descending index order with swap-remove and counts each one in `starvation_deaths`.
- **Initial population:** uses only the initial-population random domain. Each organism in ID order takes eight 64-bit draws:
  - x and y: one draw each, using the top 24 bits as a uniform value in [0, 1) times the world size.
  - speed, size and sense: two draws each, a Box–Muller normal sample using `libm`, then mean + spread × z, clamped to the trait range.
  - Starting energy is `start_energy_fraction × capacity`.
- **State hash:** the M-02 fields, then next ID, organism count, every column in storage order (floats as IEEE bits with -0.0 normalized), cumulative starvation deaths, food grid width, height and every cell's food, and the four generators' word positions.

### Food and seeking as implemented (M-07, Week 1 part of M-09, provisional)

- **Food grid:** one contiguous `f32` per cell, indexed `y × width + x`. Cells are `world.cell_size` units square (default 2, SCOPE 6.1), so the 512 × 512 world has a 256 × 256 grid.
  - A position maps to cell `floor(position / cell_size)`, clamped so the far world edge maps to the last cell.
  - If the world is not a multiple of the cell size, the last row and column are partial cells.
- **Caps and initial food:** cap = fertility (1 everywhere until terrain) × `max_per_cell` × `abundance`. Food starts uniform at `initial_fraction` × cap and uses no random numbers.
- **Regrowth (phase 10):** every cell gains `regrowth_per_tick`, stopping at its cap.
- **Feeding (phase 5, before the energy charge and the starvation check):**
  - A prey eats from the cell it stands in. It eats the least of: the cell's food, `intake_per_tick`, and the food needed to fill its capacity.
  - The cell loses exactly that amount, and the prey gains `amount × energy_per_unit`, capped at capacity.
  - Prey sharing a cell eat in ascending storage index. This is a provisional order advantage for lower indices; feeding order is not randomized.
- **Decisions (phase 2):** each prey decides on its first tick, then every `decision_interval` ticks (default 10), staggered by ID (SCOPE 6.8).
  - **Useful food:** a cell is a candidate only if its center is within sense range and it holds at least one useful bite, `MIN_USEFUL_TARGET_BITES` (1, P) × `intake_per_tick`. Only the cells in the sensing circle's bounding box are examined.
  - **Best candidate:** the most food, breaking ties by nearer center, then lower cell index.
  - **Target reconsideration:** the current target is kept while it holds at least one useful bite **and** at least `TARGET_SWITCH_FRACTION` (0.75, P) of the best candidate's food. Otherwise the prey switches to the best candidate. Smaller differences never cause a switch. Both constants are fixed provisional values in `sim/src/behavior.rs`, not scenario parameters.
  - With no useful food in range it clears its target and draws a new wandering heading: one behavior-domain draw per wandering decision. Keeping or switching a target draws no random numbers.
  - **Stationary feeding is valid.** A prey feeding in its target cell stays put until that cell stops being worthwhile. A sated prey eats only its standing energy cost (about 0.0013 food per tick for size 1 under the default coefficients). Where a cell regrows at least that fast, as under the shared default regrowth of 0.002, the cell stays at its cap and the prey can feed there indefinitely.
- **Movement (phase 4):** a prey with a target stays put once inside the target cell. Otherwise it moves straight toward the cell center at full speed, without overshooting.
  - A wanderer moves `wander_speed_fraction` (default 0.5) of its maximum along its heading.
  - Positions are clamped to the world edges. A wanderer pressed against an edge stays there until its next decision.

## Headless runner and CSV export (M-17, M-21)

`headless run` runs the built-in default scenario (scenario files arrive with M-16) for a fixed number of ticks, prints a summary, and optionally writes a CSV time series.

```sh
# Summary only
cargo run -p headless -- run --seed 1 --ticks 18000

# 10-minute run (30 ticks = 1 simulated second) with one CSV row per simulated second
cargo run -p headless -- run --seed 1 --ticks 18000 --sample-every 30 --csv results.csv

# Replace an existing file deliberately
cargo run -p headless -- run --seed 1 --ticks 18000 --sample-every 30 --csv results.csv --overwrite

cargo run -p headless -- run --help
```

**Options:**

| Option | Meaning |
| --- | --- |
| `--seed <U64>` | Run seed (required) |
| `--ticks <U64>` | Fixed ticks to run (required; 0 is valid) |
| `--sample-every <TICKS>` | CSV row interval, at least 1; requires `--csv` |
| `--csv <PATH>` | CSV output file; requires `--sample-every` |
| `--overwrite` | Allow `--csv` to replace an existing file; requires `--csv` |

**Summary:** the summary goes to stdout. It lists:

- seed, requested and completed ticks, and simulated seconds
- prey population and the average speed, size, sense range and energy
- total food, cumulative food consumed and cumulative starvation deaths
- the state hash, plus wall-clock elapsed time

Elapsed time is reported only. It is never part of the results or the CSV. Averages print `n/a` when there are no prey.

**Exit status:** 0 on success, 1 when a run fails (for example a CSV destination problem, reported on stderr naming the path), 2 for command-line usage errors (with usage help).

**CSV schema (fixed order, one header row):** `tick, sim_seconds, prey_population, avg_speed, avg_size, avg_sense, avg_energy, total_food, cumulative_food_consumed, cumulative_starvation_deaths, state_hash`.

**Sampling:** a row at tick 0 (before any step), at every multiple of `--sample-every`, and at the final tick when it is not a multiple. No row is duplicated, and a 0-tick run writes the header and the tick-0 row.
  - So 300 ticks every 30 give 11 rows (0 to 300); 305 ticks give 12, ending at 305; the 18,000-tick example gives 601.
  - Sampling only reads state. A run with CSV executes the same ticks, draws the same random numbers and ends in the same state as one without.

**Formatting:**
- Lines end in `\n`.
- Integers are plain decimal.
- Floats use Rust's shortest round-trip decimal form, with a `.` separator and no exponent, independent of locale. Whole values print without a fraction (`600`, not `600.0`).
- Averages are empty fields when there are no prey; NaN or infinity is an error, never written.
- `state_hash` is `0x` plus exactly 16 lowercase, zero-padded hexadecimal digits, so spreadsheets keep it as text.
- The CSV contains no timestamps or other run-dependent data, so identical commands give byte-identical files.

**File safety:**
- The CSV is written to a sibling `<name>.partial` file and renamed into place only after every row is written and flushed. A failed run never truncates or replaces an existing file.
- Without `--overwrite` the destination is reserved with an exclusive create, so an existing file is never replaced.
- A missing parent directory, a directory used as the file name, or an existing `<name>.partial` file stops the run with an error naming the path.

**Summary values in `sim`:** `Simulation::summary_metrics()` returns `sim::SummaryMetrics`. It reads state only, draws no random numbers, and sums in storage order with `f64` accumulation.
  - `cumulative_food_consumed` is new authoritative state in `sim`: food units actually removed from cells (not energy), and part of the state hash.

**T-CSV-10 manual check (passed, 2026-11-10):** the 18,000-tick example opened correctly in Microsoft Excel on the Windows reference machine; see Current status for what was verified. To repeat it, generate the example and open it in Excel (or LibreOffice Calc). Each column should be in its own column, decimals should read as numbers, and `state_hash` should show intact.

## Desktop application (M-10, M-11 Week 1 minimum)

```sh
cargo run -p app            # WSL2 or Linux; see the WSLg note below
cargo run -p app --release  # much smoother, especially with software rendering
```

**What it shows:**
- One run of the **Week 1 demonstration scenario** (`app::session::app_scenario()`) with the fixed provisional run seed **1**. It is `ScenarioConfig::default()` with two app-only overrides (P):

  | Field | Demo value | Shared default |
  | --- | --- | --- |
  | `food.initial_fraction` | 0.25 | 1.0 |
  | `food.regrowth_per_tick` | 0.0005 | 0.002 |

  `headless run` and `ScenarioConfig::default()` are unchanged. Under the shared default, regrowth exceeds a sated prey's standing demand, so nearly every prey sits still in a full cell from about 15 s on. Regrowth below that demand makes occupied cells slowly deplete, so prey move on. Lower starting food makes prey graze across several cells to fill up. Neither override targets an evolutionary outcome. Reset rebuilds exactly this scenario's tick-0 state and hash.
- The app opens **paused at tick 0** in a 1280 × 720 window titled "Natural Selection Simulator". It needs no asset files.
- **Food** is one texture with one pixel per grid cell. It blends linearly from dark soil (empty) to green (at cap), with nearest-neighbor sampling. It is updated in place after ticks run.
- **Prey** are orange squares 3 world units wide (one sprite per living prey), drawn above the food.

**Controls:**

| Control | Effect |
| --- | --- |
| Play / Pause buttons | Run or stop the simulation at 1× through the existing `SteppingDriver`; a pause clears the backlog, so resuming never bursts |
| Reset button | Rebuild the simulation from the same scenario and seed, back to tick 0 and the start hash; the backlog is cleared and the paused or running state is kept (M-02 semantics) |
| Right- or middle-drag | Pan |
| WASD or arrow keys | Pan (600 screen pixels per second) |
| Mouse wheel | Zoom about the view center, from 0.01 to 16 world units per pixel |

Pan and zoom work while paused and never touch the simulation. The view center is kept inside the world, so the map cannot be lost.

**Panel readouts:** running or paused, seed, tick, simulated time, prey, starvation deaths, food consumed, state hash (`0x` + 16 hex digits), and achieved speed. The backlog appears when the driver falls behind, and the stall-capped frame count once it is nonzero.

**How it works:**
- `app::session::Session` owns the one authoritative `sim::Simulation` and the existing `SteppingDriver`.
- Each frame passes Bevy's real frame time (`Time<Real>`) and an `Instant`-based work timer to the driver. `Time<Real>` is used instead of the default clock because the default already clamps frame time at 250 ms, which would hide stalls from the driver's own stall counter.
- Visuals are refreshed only after ticks run, and rebuilt after a reset. Prey sprites are matched to organisms by stable ID, never by storage index, so swap-removal cannot attach a sprite to the wrong prey.
- Nothing in `app` writes to the simulation.

**WSLg note:** if `cargo run -p app` panics with `Failed to build event loop ... WaylandError(Connection(NoCompositor))`, the WSLg Wayland socket link is missing from `/run/user/1000`, which can happen after a WSL restart. Run with `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir cargo run -p app`. The X11 fallback needs the `libxkbcommon-x11` system package, which is not installed.

**Manual Week 1 smoke test (pending):**

1. The window opens with the right title, and the whole world is in view.
2. Food is visible, and prey show clearly against it.
3. Play: the tick advances and prey move. Cells that are eaten turn darker and regrow, and the hash changes. The food starts at a quarter of its cap, so the map starts dim green.
4. Pause: the tick, prey and food all stop. Pan and zoom still work and leave the tick and hash unchanged. After a pause of several seconds, resuming shows no jump.
5. Reset: the tick returns to 0, the start layout returns, and the hash equals the start hash.
6. Zoom stops at both limits, and the view can't be lost off screen.
7. Watch prey seek and eat for at least 2 simulated minutes. For about the first 30 s almost all prey travel and leave dark trails. After that many prey feed in place for several seconds at a time (valid), but a visible share keeps leaving cells and moving to new ones. **Fail** if nearly all prey stop for good, as they did before the target-reconsideration fix. Zoom in on a group to see the short hops.
8. Note the frame rate at the default 2,000 prey.

**Native Windows (pending):** build and run with `cargo run -p app --release` from a native Windows Rust install (the toolchain in `rust-toolchain.toml`), then repeat the checklist.

**Known limitations (Week 1):**
- **Movement is slow at the fitted zoom.** Prey move about 1 world unit per simulated second. At the fitted zoom that's about 1.3 pixels per second, so their movement is easiest to see zoomed in; eaten cells show more clearly.
- **Calm grazing after the first half-minute.** Once prey are sated, most moves are hops into a neighboring cell, often less than a pixel at the fitted zoom. Without reproduction (M-04) the population cannot grow to use the food supply, so this is expected for Week 1.
- **No starvation observed.** In the demo scenario no prey starved in the first 3 simulated minutes, and none starve within 10 minutes under the shared default (Week 3 balancing).
- **Not in this task:** speed controls beyond 1× (M-11, Week 2), legend and fit-to-map button or Home key (M-10, Week 2), zoom toward the cursor (T-CAM-3), the performance overlay (M-18).
- **Prey marker size.** It is fixed in world units, so markers shrink when you zoom out.
- **Slow rendering in WSL2.** WSL2 rendering here is software only (`llvmpipe`).

## Workspace structure

| Path | Kind | Role |
| --- | --- | --- |
| `sim/` | library | All simulation rules. No graphics, UI, windowing, file I/O or clock. |
| `app/` | binary + library | Desktop application (Bevy + egui). Window-free logic such as the stepping driver lives in its library target. |
| `headless/` | binary + library | Command-line runner and CSV export. Its library holds the testable run logic. No Bevy or egui. |

Architecture invariants for contributors and Claude Code sessions are in [CLAUDE.md](CLAUDE.md).

## Build and test (WSL2)

Requires [rustup](https://rustup.rs). The toolchain in `rust-toolchain.toml` is selected automatically.

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p headless -- run --seed 1 --ticks 300
```

The suite has 182 tests:

- 110 in `sim`: 108 unit (including 7 target-reconsideration unit tests and 4 controlled retargeting runs) and 2 integration.
- 38 in `headless`: 26 unit and 12 process-level.
- 34 in `app`: 16 driver tests (including the Max-mode ceiling), T-DET-4, 9 session tests (including the demo scenario and its reset), 3 camera tests and 5 rendering tests.

To run only the tests for one SCOPE area:

```sh
cargo test -p sim --test determinism   # T-DET-1, T-DET-2
cargo test -p sim t_rng_1              # T-RNG-1
cargo test -p sim t_en_                # T-EN-1, T-EN-3
cargo test -p sim t_cap_1              # T-CAP-1
cargo test -p sim behavior::           # seeking, target keep/switch rules and tie-breaking
cargo test -p sim sated_prey           # diagnostic: sated prey feed, retarget and move on
cargo test -p app driver               # T-FT-1 to T-FT-3
cargo test -p app t_det_4              # T-DET-4 (driver, headless path, direct stepping)
cargo test -p app session              # play, pause, reset, demo scenario and camera purity through the app session
cargo test -p app camera               # camera fit, pan and zoom bounds
cargo test -p app render               # food colors, texture rows, ID-to-sprite mapping
cargo test -p headless t_csv_          # T-CSV-1 to T-CSV-9 (unit and process level)
cargo test -p headless --test cli      # headless binary: exit codes, files, T-DET-4 binary check
```

**Memory note (WSL2):** the `app` executables link all of Bevy with debug info, about 1 GB each, and linking one needs several GB of RAM. Two such links at once crashed the 7.7 GB WSL2 VM. For that reason T-DET-4 is a unit test inside the `app` library (`app/src/t_det_4.rs`), not an integration test, which would make Cargo link the `app` binary as well. For the same reason the `app` binary has `test = false`; it has no tests, and all testable app logic is in the library. If memory is tight, limit parallel jobs, for example `cargo test --workspace -j 2`.

Dependency-boundary check (T-HL-6): neither command below should list any `bevy` or `egui` crate.

```sh
cargo tree -p sim
cargo tree -p headless
```

## Milestone 1 progress (Nov 10–16, 2026)

- [x] M-01 Cargo workspace, pinned versions, CLAUDE.md, README skeleton
- [x] M-02 Deterministic fixed-step core and stepping driver (T-DET-4 complete)
- [x] M-03 Prey organisms, traits, energy, starvation, age (safety-cap birth suppression waits for births in M-04)
- [x] M-07, M-09 (basic) Food grid, feeding, simple food seeking (uniform fertility; open world)
- [x] M-17 Milestone 1 minimum: `headless run` with printed summary. Scenario-aware JSON `--out` (T-HL-1) is deferred to M-16.
- [x] M-21 Minimal CSV export (T-CSV-1 to T-CSV-10 pass; T-CSV-10 checked manually in Excel)
- [X] M-10, M-11 Minimal Bevy view with play, pause, reset, pan, zoom. Implemented, and the WSL2 window opens; manual smoke test and native Windows run pending.

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
| 2026-11-10 | Tick rate (provisional): minimum 1, default 30 ticks per simulated second. No maximum, because SCOPE.md defines none (Q5, end of Week 3); this is a known gap in I-07 until then. |
| 2026-11-10 | Superseded (same day): `run_seed` was a `ScenarioConfig` field. |
| 2026-11-10 | Run identity is separate from the scenario: `Simulation::new(&ScenarioConfig, run_seed)` (SCOPE 8). `ScenarioConfig` holds model and environment settings only. The resolved terrain seed is `ScenarioConfig::resolved_terrain_seed(run_seed)`: the explicit terrain seed if set, otherwise derived from the run seed by the existing rule. |
| 2026-11-10 | Seed-derivation compatibility vectors (run seed 1, no terrain seed) are frozen in `sim/src/lib.rs`. Changing them changes every deterministic run and needs a recorded decision. |
| 2026-11-10 | Stepping driver (`app/src/driver.rs`) uses an integer accumulator (`real ns × ticks per second × multiplier`), so frame deltas summing to exactly 1 s at 1× give exactly 30 ticks. Limits are SCOPE 7.3's provisional values: 600 ticks or 25 ms per frame, 12 ms in Max mode, 250 ms stall cap. The driver reads no clock; the app passes frame deltas and a work-elapsed closure. |
| 2026-11-10 | Achieved speed for the status bar is averaged over a 1 s real-time window (P); SCOPE defines no window for it (the overlay's 60 s statistics are M-18). Step leaves the driver paused; Reset keeps the paused or running state; the driver starts paused at 1×. |
| 2026-11-10 | Serde is deferred until a TOML parser exists (M-16, M-17). `ScenarioConfig` uses `Default` from the `limits` module for now. |
| 2026-11-10 | `libm =0.2.16` pinned with default features off (portable software implementations) for `ln`, `sqrt` and `cos` in sampling (SCOPE 6.10). |
| 2026-11-10 | M-03 provisional limits (SCOPE gives no numbers; all P, in `sim/src/limits.rs`). World size 16–4,096 (default 512). Prey start count 0–150,000 (default 2,000). Start energy fraction 0.01–1 (default 0.5). Trait ranges: speed 0.1–5, size 0.2–5, sense 1–50. Spreads 0 to the range width. Energy coefficients: capacity 0.1–1,000; base, size, move and sense 0–10. Defaults follow SCOPE 8's illustrative scenario. |
| 2026-11-10 | Starting prey are given as a count only; SCOPE's density option needs the terrain grid and arrives with M-06. Zero starting prey is allowed (an empty, valid run). |
| 2026-11-10 | `ValidationError` now carries the field path as a `String` (for example `prey.speed.mean`), and adds `AboveMaximum`, `NotFinite` and `OutOfRange` variants. |
| 2026-11-10 | The temporary M-02 behavior-domain probe is removed. Stepping now draws no random numbers; T-DET-2 compares real prey state. |
| 2026-11-10 | Movement energy uses realized effort: executed step ÷ (max distance per tick × terrain multiplier), clamped to [0, 1]. This refines SCOPE 6.3's per-behavior effort level and keeps its terrain rule. It affects only T-EN-2 (should-priority). Wandering at `wander_speed_fraction` 0.5 reproduces SCOPE's 0.5 wandering effort. |
| 2026-11-10 | M-07 and M-09 provisional values (in `sim/src/limits.rs`): cell size 1–16 (default 2); `max_per_cell` 0.01–100 (default 1); `regrowth_per_tick` 0–100 (default 0.002); `abundance` 0–10 (default 1); `initial_fraction` 0–1 (default 1, which SCOPE does not specify); `intake_per_tick` 0–100 (default 0.05); `energy_per_unit` 0.01–100 (default 1); `decision_interval` 1–300 (default 10); `wander_speed_fraction` 0–1 (default 0.5). |
| 2026-11-10 | Week 1 food seeking uses SCOPE's Week 1 rule (richest cell in sense range), not the region score of SCOPE 6.8, which needs the spatial grid (M-08). |
| 2026-11-10 | T-DET-1 runs on a smaller fixed scenario (300 prey, 128 × 128 world) so the must-priority suite stays fast in debug builds; T-DET-2 still uses the default scenario. |
| 2026-11-10 | Known precision limit: positions are `f32`. At x = 2,048 adjacent `f32` values are about 2.4 × 10⁻⁴ apart, while the slowest step is 0.0033 units per tick, so a single step can round by several percent. This is deterministic but coarse; revisit (for example with `f64` positions) if it shows up in tier G profiling or behavior. |
| 2026-11-10 | SCOPE.md updated to version 2.2: the minimal CSV export becomes Week 1 must-have M-21 (2.5 h, from reserve; totals 58.5 h implementation and 16.5 h reserve). C-04 now covers only advanced CSV and experiment export. If Week 1 runs short, graphical work is reduced to the minimum view before M-21 is cut. |
| 2026-11-10 | `clap =4.6.7` (derive) pinned for the `headless` command line (SCOPE 7.1). CSV is written with the standard library; no CSV crate is needed for the fixed numeric schema. |
| 2026-11-10 | `headless run` prints its summary and elapsed time. SCOPE's JSON `--out` and T-HL-1 wait for scenario files (M-16), because the JSON must name the scenario and its format version. |
| 2026-11-10 | CSV files are written via `<name>.partial` and renamed into place; without `--overwrite` the destination is reserved with an exclusive create. Exit status 2 for usage errors, 1 for run failures. |
| 2026-11-10 | `cumulative_food_consumed` (`f64`, food units actually removed from cells) added to `sim` state and the state hash; feeding behavior is unchanged. |
| 2026-11-10 | T-DET-4 is an `app` library unit test rather than an integration test, to avoid a second ~1 GB Bevy link that exhausted WSL2 memory. |
| 2026-11-10 | T-CSV-10 passed manually in Microsoft Excel on the Windows reference machine, so M-21 is complete. T-HL-1 (scenario-aware JSON output) is deferred until M-16, because scenario identity and format version do not exist yet. |
| 2026-11-10 | Superseded (2026-11-11): the Week 1 app used the shared default scenario with no demonstration configuration. |
| 2026-11-10 | Max-mode emergency ceiling: `DriverLimits::max_mode_tick_ceiling` = 100,000 ticks per frame (P). It stops an infinite loop if the work timer never advances; the 12 ms work budget remains the real limit. |
| 2026-11-10 | Camera (P): zoom from 0.01 to 16 world units per pixel in ×1.1 wheel steps, about the view center; fit margin 1.05; keyboard pan 600 pixels per second; the view center is clamped to the world. |
| 2026-11-10 | The app reads frame time from `Time<Real>`, not the default clock, so the driver's 250 ms stall cap and stall counter see real stalls. The `app` binary has `test = false` to avoid a second ~1 GB Bevy link during tests. |
| 2026-11-11 | Target reconsideration (Week 1 behavior correction, M-09): a food target is kept only while it holds at least one useful bite (`MIN_USEFUL_TARGET_BITES` = 1 × `intake_per_tick`, P) and at least `TARGET_SWITCH_FRACTION` = 0.75 (P) of the best useful cell in range. Cells under one bite are no longer candidates. Ties still go to more food, then nearer center, then lower index; no random numbers are drawn. Root cause: prey sat still indefinitely in cells that regrew to the cap. This changes every run's trajectory and hash. |
| 2026-11-11 | Week 1 app demonstration scenario (`app::session::app_scenario()`): `ScenarioConfig::default()` with `food.initial_fraction` = 0.25 and `food.regrowth_per_tick` = 0.0005 (P), run seed 1. The shared default and `headless run` are unchanged. Measured with the fix: regrowth alone still left nearly all prey still after about 15 s. Regrowth plus lower starting food kept prey travelling for about 30 s, and afterwards 6–13% made moves over 6 units in each 15 s. The proposed `abundance` = 0.75 added little, and `start_count` = 1,000 reduced movement, so neither is used. |
