# Scenario file format (version 1)

A scenario is a human-readable [TOML](https://toml.io) file, usually named `*.scenario.toml`. It holds every model parameter plus a name, description and learning goal. Teachers share scenarios by sharing these files (UC2, M-16).

The app saves and loads them in its **Scenarios…** window (the `scenarios/` folder, or any typed path). `headless run --scenario <file>` runs them. Presets are files of the same format in `presets/`.

## Rules

- `format_version = 1` is **required**. Any other version is rejected with a message naming `format_version`.
- **Unknown fields are rejected** with their name and line, so typos never pass silently.
- **Missing fields and tables take their defaults** (the *Default* column below). The exception is a `[species.prey]` or `[species.predator]` table: if present, it must be complete (`start`, `start_energy_fraction`, `speed`, `size`, `sense`), so a partial table cannot silently mix in the other species' defaults. `max_age_seconds` may always be omitted.
- **Out-of-range values are rejected** with the field's path and its allowed range (for example `predation.size_ratio = 50 is outside the range 0.1 to 10`). Nothing is silently corrected.
- **Seeds** are TOML integers, so they are at most 9,223,372,036,854,775,807.
- **Save-then-load is lossless.** Floats are written in their shortest exact form (T-SCN-1).
- An invalid file never changes the running simulation (M-16).

## Top level

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `format_version` | File format version | must be 1 | (required) |
| `name` | Scenario name shown in the app | text | "Untitled scenario" |
| `description` | What the scenario contains | text | "" |
| `learning_goal` | Learning goal or experiment description | text | "" |
| `seed` | Suggested run seed; omit to let the app choose one | 0 – 2⁶³−1 | none |
| `ticks_per_second` | Fixed ticks per simulated second | 1 – 240 | 30 |
| `population_cap` | Safety cap: births stop at this total | 1 – 1,000,000 | 150,000 |

## `[world]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `width`, `height` | World size in world units | 16 – 4,096 | 512 |
| `cell_size` | Terrain and food cell side | 1 – 16 | 2 |
| `spatial_cell_size` | Spatial-grid cell side | 4 – 128 | 16 |

Cross-field rule: the grid may have at most 2,048 × 2,048 cells.

## `[terrain]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `seed` | Terrain seed; omit to derive it from the run seed | 0 – 2⁶³−1 | none |
| `water_level` | Share of the map under water (elevation rank) | 0 – 0.9 | 0.3 |
| `rock_level` | Elevation rank above which cells are rock | 0.1 – 1 | 0.85 |
| `feature_scale` | Typical size of lakes and hills, world units | 8 – 1,024 | 96 |

Cross-field rule: `rock_level − water_level ≥ 0.2`, so at least 20% of the map is passable.

## `[food]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `max_per_cell` | Food a fully fertile cell holds at abundance 1 | 0.01 – 100 | 1 |
| `regrowth_per_tick` | Food regrown per tick, up to the cap | 0 – 100 | 0.002 |
| `abundance` | Multiplier on every cell's cap | 0 – 10 | 1 |
| `initial_fraction` | Starting food as a fraction of each cap | 0 – 1 | 1 |
| `intake_per_tick` | Most food one prey eats per tick | 0 – 100 | 0.05 |
| `energy_per_unit` | Energy per unit of food | 0.01 – 100 | 1 |

## `[energy]` (per simulated second)

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `capacity` | Energy capacity per unit of size | 0.1 – 1,000 | 10 |
| `base` | Cost of living | 0 – 10 | 0.01 |
| `size` | Maintenance per unit of size | 0 – 10 | 0.02 |
| `move` | Movement: × effort × size × speed² | 0 – 10 | 0.01 |
| `sense` | Sensing per unit of sense range | 0 – 10 | 0.001 |

## `[mutation]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `probability` | Chance each trait of an offspring mutates | 0 – 1 | 0.1 |
| `magnitude` | σ: a mutated value is multiplied by 1 + N(0, σ) | 0 – 1 | 0.05 |

## `[reproduction]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `threshold` | Reproduce at this fraction of capacity | 0.1 – 1 | 0.8 |
| `offspring_energy_fraction` | Offspring energy, fraction of its capacity | 0.01 – 1 | 0.3 |
| `overhead` | Energy lost per birth | 0 – 100 | 0.5 |
| `spawn_radius` | Offspring placed within this distance | 0 – 32 | 2 |

## `[species.prey]` and `[species.predator]`

| Key | Meaning | Range | Default (prey / predator) |
| --- | --- | --- | --- |
| `start` | `{ count = N }` or `{ density = D }` (per 1,000 passable cells) | count 0 – 150,000; density 0 – 1,000 | density 40 / density 4 |
| `start_energy_fraction` | Starting energy, fraction of capacity | 0.01 – 1 | 0.5 / 0.5 |
| `speed` | `{ mean, spread }` | mean 0.1 – 5; spread 0 – 4.9 | 1.0, 0.1 / 1.2, 0.1 |
| `size` | `{ mean, spread }` | mean 0.2 – 5; spread 0 – 4.8 | 1.0, 0.1 / 1.5, 0.1 |
| `sense` | `{ mean, spread }` | mean 1 – 50; spread 0 – 49 | 10, 1 / 14, 1 |
| `max_age_seconds` | Death from old age (S-02); 0 = off | 0 – 100,000 | 0 |

Cross-field rule: a starting count may not exceed `population_cap`, and the resolved starting populations together may not exceed it either.

## `[predation]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `size_ratio` | Predator size must be ≥ ratio × prey size | 0.1 – 10 | 1.2 |
| `capture_radius` | Capture distance | 0.1 – 16 | 1 |
| `efficiency` | Share of the prey's energy content gained | 0 – 1 | 0.6 |
| `content` | Prey energy content per unit of size | 0 – 1,000 | 10 |

## `[behavior]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `decision_interval` | Ticks between goal decisions | 1 – 300 | 10 |
| `threat_interval` | Ticks between prey threat checks | 1 – 60 | 2 |
| `wander_speed_fraction` | Wandering speed, fraction of maximum | 0 – 1 | 0.5 |

## `[navigation]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `expansions_per_request` | Route-search steps per request | 16 – 65,536 | 512 |
| `budget_per_tick` | Route-search steps per tick, all requests | 16 – 1,000,000 | 4,000 |
| `window_margin` | Search window margin beyond sense range | 0 – 256 | 16 |
| `drift_cells` | A moving target this far from the route's end makes it stale | 0.5 – 64 | 2 |
| `hunt_replan_interval` | Fewest ticks between a chase's re-plans | 1 – 600 | 15 |
| `stuck_window` | Ticks without progress before re-planning | 1 – 3,600 | 60 |
| `stuck_min_progress` | Progress that counts, world units | 0 – 16 | 1 |
| `unreachable_ticks` | How long an abandoned target is avoided | 1 – 36,000 | 300 |

Cross-field rule: `budget_per_tick ≥ expansions_per_request`.

## `[metrics]`

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `interval` | Ticks between chart samples | 1 – 3,600 | 30 |

## `[famine]` (optional, S-01)

| Key | Meaning | Range | Default |
| --- | --- | --- | --- |
| `at_tick` | First tick of the famine | 0 – 100,000,000 | (required in the table) |
| `food_multiplier` | Multiplier on every cell's regrowth cap | 0 – 1 | (required in the table) |

## Example

The baseline classroom preset, [`presets/baseline.scenario.toml`](../presets/baseline.scenario.toml), is a complete, commented example. The smallest valid file is:

```toml
format_version = 1
name = "Defaults"
```
