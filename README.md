# Natural Selection Simulator

A Rust desktop application that simulates natural selection among prey and predators on procedurally generated 2D terrain, for classroom demonstration and simple, repeatable experiments. The scope of record is [SCOPE.md](SCOPE.md) (version 2.2).

## Current status

M-01 (workspace foundation) is complete. M-02 (deterministic core) is complete for Week 1 except one pending check:

- `sim`: configuration, four random domains, fixed tick pipeline, state hash (T-DET-1, T-DET-2, T-RNG-1).
- `app`: pure stepping driver in `app/src/driver.rs` (T-FT-1 to T-FT-3, and the driver portion of T-DET-4), not yet connected to a Bevy window.
- **Pending:** the T-DET-4 comparison of the app driver against `headless run --ticks 1000`, blocked on M-17.
- **Week 2 (SCOPE 6.10):** the state hash must keep growing to cover all state as later columns arrive; T-DET-1 and T-DET-2 are then rerun.

M-03 (prey organisms) is complete for prey only (T-EN-1, T-EN-3, T-CAP-1). The safety cap's birth suppression arrives with births (M-04); for now 150,000 is only the maximum starting count.

M-07 (food grid, regrowth and feeding) is complete with uniform fertility. The Week 1 part of M-09 is complete: prey sense the richest food cell in range, move straight toward it in an open world, otherwise wander. Prey that find enough food survive; prey that do not still starve.

**Current limitations:**

- No terrain yet: every cell has fertility 1, and the terrain speed multiplier is fixed at 1.0 (`behavior::terrain_speed_multiplier`) until M-06.
- No barriers, routes or spatial grid. Food seeking uses the Week 1 rule (richest cell), not SCOPE 6.8's region score, which needs M-08.
- No reproduction, so a run can only lose prey.
- Contention for a shared cell favors lower storage indices.

No rendering or UI exists yet; the `app` and `headless` binaries are still placeholders.

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
  - It targets the cell with the most food whose center is within sense range, breaking ties by nearer center, then lower cell index. Only the cells in the sensing circle's bounding box are examined.
  - With no food in range it clears its target and draws a new wandering heading: one behavior-domain draw per wandering decision.
- **Movement (phase 4):** a prey with a target stays put once inside the target cell. Otherwise it moves straight toward the cell center at full speed, without overshooting.
  - A wanderer moves `wander_speed_fraction` (default 0.5) of its maximum along its heading.
  - Positions are clamped to the world edges. A wanderer pressed against an edge stays there until its next decision.

## Workspace structure

| Path | Kind | Role |
| --- | --- | --- |
| `sim/` | library | All simulation rules. No graphics, UI, windowing, file I/O or clock. |
| `app/` | binary + library | Desktop application (Bevy + egui). Window-free logic such as the stepping driver lives in its library target. |
| `headless/` | binary | Command-line runner. No Bevy or egui. |

Architecture invariants for contributors and Claude Code sessions are in [CLAUDE.md](CLAUDE.md).

## Build and test (WSL2)

Requires [rustup](https://rustup.rs). The toolchain in `rust-toolchain.toml` is selected automatically.

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p headless
```

The suite has 108 tests: 93 in `sim` (91 unit, 2 integration) and 15 in `app`. Run only the tests for one SCOPE area:

```sh
cargo test -p sim --test determinism   # T-DET-1, T-DET-2
cargo test -p sim t_rng_1              # T-RNG-1
cargo test -p sim t_en_                # T-EN-1, T-EN-3
cargo test -p sim t_cap_1              # T-CAP-1
cargo test -p app driver               # T-FT-1 to T-FT-3, T-DET-4 (driver portion)
```

Dependency-boundary check (T-HL-6): neither command below should list any `bevy` or `egui` crate.

```sh
cargo tree -p sim
cargo tree -p headless
```

## Week 1 progress (Nov 10–16, 2026)

- [x] M-01 Cargo workspace, pinned versions, CLAUDE.md, README skeleton
- [x] M-02 Deterministic fixed-step core and stepping driver (T-DET-4 against `headless` pending M-17)
- [x] M-03 Prey organisms, traits, energy, starvation, age (safety-cap birth suppression waits for births in M-04)
- [x] M-07, M-09 (basic) Food grid, feeding, simple food seeking (uniform fertility; open world)
- [ ] M-17 Minimal `headless run`
- [ ] M-21 Minimal CSV export from `headless run` (`--sample-every`, `--csv`, `--overwrite`; T-CSV-1 to T-CSV-10), built with M-17 as one workflow. Not started.
- [ ] M-10, M-11 Minimal Bevy view with play, pause, reset, pan, zoom

## Reserve-hours ledger

| Date | Hours used | Reason | Remaining |
| --- | --- | --- | --- |
| 2026-11-10 | 0 | Initial reserve | 19 |
| 2026-11-10 | 2.5 (planned, not spent) | SCOPE v2.2: minimal CSV export promoted to must-have M-21; funded from Week 1 reserve (Week 1 is now 14 h + 1 h reserve) | 16.5 |

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
