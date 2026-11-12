# Benchmarks (M-18)

This document gives the benchmark procedure of [SCOPE.md](../SCOPE.md) Section 10 and the results recorded so far. **All results apply only to the machine they were measured on.** Nothing here is a claim about classroom hardware (SCOPE 10.3).

## Tiers and scenarios

The benchmark scenarios are committed in [`bench/scenarios/`](../bench/scenarios). They use the baseline model at each tier's size, with run seed 1 (SCOPE 10.4):

| Tier | File | World | Terrain grid | Spatial grid | Terrain seed | Prey | Predators | Required speed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C (classroom) | `b-c.scenario.toml` | 512 × 512 | 256 × 256 | 32 × 32 | 1001 | 2,000 | 200 | 10× (must meet) |
| M (minimum scale) | `b-m.scenario.toml` | 1024 × 1024 | 512 × 512 | 64 × 64 | 1002 | 9,000 | 1,000 | 1× (must meet) |
| G (goal) | `b-g.scenario.toml` | 2048 × 2048 | 1024 × 1024 | 128 × 128 | 1003 | 45,000 | 5,000 | 1× (measured and reported) |

Each tier runs in two modes:

- **Frozen population:** births and deaths are suppressed while every other phase runs, giving a constant workload at the stated counts.
- **Ecological:** the full model runs, and the population is reported beside the timings.

## Simulation benchmark (`headless`, automated)

1. Build with `cargo build --release -p headless`. The release profile is Cargo's default (`opt-level = 3`, no LTO, 16 codegen units). The workspace adds only `[profile.dev.package.sim] opt-level = 3` for debug and test builds, which does not affect release.
2. Run:

   ```sh
   ./target/release/headless bench --reps 3 --out bench/results/<date>-<commit>.json
   ```

   - Each repetition builds a fresh simulation, runs 300 warm-up ticks, then times 3,000 measured ticks (P), in both modes, for `b-c`, `b-m` and `b-g`.
   - `--tiers c,m` limits the tiers, and `--ticks` and `--warmup` override the counts.
   - For phase timings (time per pipeline phase as a share of tick time), build with `--features profile`:

     ```sh
     cargo run --release -p headless --features profile -- bench --reps 1
     ```

3. Report the median of the three repetitions' median tick times, plus p99 and maximum. The JSON also holds:
   - p95 and maximum;
   - ticks per second, the real-time factor (ticks per second ÷ 30) and organism-ticks per second;
   - the population at start and end;
   - whether SCOPE 10.2's companion rules hold: real-time factor ≥ 2 × the tier's speed, and p99 ≤ 3 × median.
4. Run it on the development machine at the end of every week, and with the Windows `headless.exe` on the reference machine in Weeks 2, 4 and 5.

## Rendered benchmark (app, reference machine, manual)

1. Laptop on mains power, Windows power mode recorded, other applications closed.
2. Run `app --scenario bench/scenarios/<tier>.scenario.toml --seed 1`.
   - Set the tier's speed, press **Fit map** (or Home), and open the overlay with **F3** (or the **Perf** button).
   - The window opens at 1600 × 900. Keep the charts panel visible.
3. Confirm from the startup log that the graphics adapter is the RTX 4060, not the integrated GPU, and record it.
4. After 10 seconds of warm-up, wait 60 seconds and copy the overlay's values into the table below:
   - median and p95 frame time;
   - frames over 50 ms;
   - simulation milliseconds per frame;
   - achieved speed;
   - organisms.

   Repeat 3 times and report the median of each value.
5. Record CPU model, adapter, driver version, power mode and commit.

**Smooth** (SCOPE 10.2) means all of the following:

- median frame time ≤ 16.7 ms;
- p95 ≤ 33.3 ms;
- at most 1% of frames over 50 ms;
- achieved speed ≥ 95% of the requested speed.

## Soak test (release verification)

Run the baseline preset at 10× for 30 minutes on the reference machine. Record memory (Windows Task Manager working set) at minute 5 and minute 30. It passes with no crash and at most 100 MB of growth after minute 5 (T-PERF-3).

## Results

### Development machine (WSL2), simulation only

Measured with the procedure above on the development machine: an Intel Core i7-9700K (8 cores, as reported by Bevy's system information) running WSL2 on Windows, with 7.7 GB RAM given to WSL. These are **not** reference-machine results.

Date 2026-11-12; working tree on top of commit `7d1b1f1` (uncommitted). Release build; the `profile` feature was off. Each run used 300 warm-up ticks then 3,000 measured ticks, starting at run seed 1. Tiers C and M used 3 repetitions; tier G used 1 to save time. Raw JSON is in [`bench/results/`](../bench/results).

| Tier | Mode | Reps | Median tick | p95 | p99 | Max | Ticks/s | Real-time factor (needed) | p99 ≤ 3 × median | Organisms (start → end) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| b-c | frozen | 3 | 0.78 ms | 1.12 ms | 1.43 ms | 8.79 ms | 1,202 | 40.1 (20) met | yes | 2,200 → 2,200 |
| b-c | ecological | 3 | 1.57 ms | 2.21 ms | 2.73 ms | 10.80 ms | 609 | 20.3 (20) met | yes | 2,770 → 3,856 |
| b-m | frozen | 3 | 4.04 ms | 5.42 ms | 6.13 ms | 13.04 ms | 237 | 7.9 (2) met | yes | 10,000 → 10,000 |
| b-m | ecological | 3 | 8.37 ms | 10.52 ms | 12.18 ms | 18.51 ms | 120 | 4.0 (2) met | yes | 12,606 → 14,779 |
| b-g | frozen | 1 | 26.28 ms | 30.47 ms | 34.02 ms | 52.04 ms | 37 | 1.2 (2) **not met** | yes | 50,000 → 50,000 |
| b-g | ecological | 1 | 49.79 ms | 58.66 ms | 67.79 ms | 131.66 ms | 20 | 0.7 (2) **not met** | yes | 60,360 → 56,543 |

On this machine:

- **Tier C** meets the simulation-side real-time-factor rule at 10× in both modes. The ecological margin is narrow: the population grows from 2,200 to about 3,900 during the run.
- **Tier M** meets the rule at 1× in both modes.
- **Tier G** runs at 1.2× real time when frozen and 0.7× in ecological mode (the population grows past 56,000). It is below both the 2× headroom rule and real time.
- **p99 ≤ 3 × median** holds everywhere.

These are simulation-only numbers. Whether the app is smooth also depends on rendering, which must be measured on the reference machine (T-PERF-2). Per SCOPE 10.7, optimization (and only then multithreading) is considered only after reference-machine measurements.

### Reference machine

Not measured yet. The reference machine is an Intel Core i5 laptop (exact model to be recorded, Q2) with an RTX 4060 Laptop GPU, running Windows. Tiers C and M must be measured there (T-PERF-2). Tier G is measured and reported whatever the result.

| Date | Commit | Tier | Mode | Median frame | p95 frame | > 50 ms | Sim ms/frame | Achieved speed | Organisms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| – | – | – | – | – | – | – | – | – | – |
