# The model and its limitations

This document describes, in plain language, every rule the Natural Selection Simulator uses, the simplifications it makes, and which numbers are provisional. It serves M-20 and the educational criteria E5 and X5 of [SCOPE.md](../SCOPE.md).

**Read this first.** Units of space, time and energy are abstract. No real species is modelled, and results show qualitative mechanisms, not predictions. Extinction of either species is a possible and valid outcome. Performance numbers apply only to the machine they were measured on.

Values marked **(P)** are provisional: starting points for balancing that may change without a scope change. Values marked **(tuned)** were chosen by balancing runs and are recorded in the README's decision log.

## 1. World, grids and time

- **The world** is a rectangle of continuous 2D positions with solid edges. The baseline preset uses 512 × 512 world units.
- **The terrain and food grid** divides the world into square cells of 2 × 2 units (P). Each cell has one terrain class and one food amount, so the baseline grid has 256 × 256 cells.
- **The spatial grid** uses 16-unit cells (P). It is rebuilt every tick and makes "who is near me?" questions fast (M-08).
- **Time** advances in fixed ticks; 30 ticks are one simulated second (P). The speed buttons (1×, 2×, 5×, 10×, Max) change how many ticks run per real second, never what a tick does. The app never skips a tick: if the computer cannot keep up, the status bar shows the lower achieved speed and the backlog.

## 2. Terrain (M-06)

Terrain is generated from a **terrain seed** using smooth random noise:

1. An elevation map is generated.
2. Each cell is ranked by elevation. `water_level = 0.25` makes the lowest 25% of cells water, and `rock_level = 0.88` makes cells above the 88th percentile rock.
3. A narrow sand band rims the water, and a band of hills rings the rock.
4. A second "moisture" noise divides the remaining land into grassland and forest.

| Class | Passable | Speed multiplier (P) | Food fertility (P) |
| --- | --- | --- | --- |
| Water | no | – | 0 |
| Sand | yes | 0.8 | 0.15 |
| Grassland | yes | 1.0 | 1.0 |
| Forest | yes | 0.6 | 0.6 |
| Hills | yes | 0.45 | 0.3 |
| Rock | no | – | 0 |

- **Movement:** an organism moving across a cell covers `multiplier × normal` distance per tick. Organisms cannot enter water or rock. A step that would enter one slides along the edge instead of stopping ("wall sliding").
- **Movement cost:** slow terrain does **not** reduce the energy spent per tick (SCOPE 6.3's terrain rule), so crossing hills costs more energy per unit of distance.
- **Same map across runs:** the same terrain seed always gives the same map. If a scenario does not fix a terrain seed, one is derived from the run seed, so a new run seed also gives a new map.
- **Reachability:** connected areas of land are labelled once. Organisms never try to reach food or prey in an area they cannot walk to.

## 3. Food (M-07)

- Each cell holds plant food up to a **cap** = fertility × `max_per_cell` × `abundance`. Water and rock never hold food.
- Food **regrows** by a fixed amount per tick (`regrowth_per_tick`, tuned 0.001) until it reaches the cap.
- **Prey eat** from the cell they stand in: at most `intake_per_tick` (0.05) food per tick, converted to energy at `energy_per_unit` (1), never beyond their capacity.
  - When several prey share a cell, they eat in storage order: a small, deterministic advantage for older prey.
- **Predators never eat plants.**
- **Famine (optional, S-01):** from a chosen tick, every cell regrows only up to a fraction of its cap. Food already above that level is eaten down, not destroyed.

## 4. Organisms and traits (M-03)

Every organism has a stable ID, a species (prey or predator), a position, energy, an age, a generation number and three heritable traits:

| Trait | Benefit | Cost | Valid range (P) |
| --- | --- | --- | --- |
| Speed | Covers more ground per tick: reaches food first, chases or escapes better | Movement cost grows with speed² | 0.1 – 5 |
| Size | Holds more energy; a predator can only eat prey sufficiently smaller than itself | Maintenance and movement cost grow with size | 0.2 – 5 |
| Sense range | Detects food, predators and prey farther away | Sensing cost grows with range | 1 – 50 |

Starting values for each species are drawn from a normal distribution with the scenario's mean and spread, then clipped to the valid range.

## 5. Energy (SCOPE 6.3)

There is one energy formula, shared by both species. Coefficients are per simulated second and set in the scenario's `[energy]` block (tuned):

```text
capacity          = capacity_coefficient × size                    (10 × size)
cost per second   = base                                           (0.03)
                  + size_coefficient  × size                       (0.04 × size)
                  + move_coefficient  × effort × size × speed²     (0.04 × …)
                  + sense_coefficient × sense range                (0.003 × sense)
```

- **Effort** is how much of its possible movement an organism actually used this tick:
  - 0 when standing still;
  - 1 at full speed (fleeing, hunting, walking to food);
  - 0.5 when wandering (P).
- This refines SCOPE's "effort level of the current behavior" (decision 2026-11-10): energy is charged for movement that actually happens.
- Speed carries the steepest cost (speed²), so faster organisms pay substantially more.

## 6. Predation (M-05, SCOPE 6.5)

- A predator can eat a prey only if **predator size ≥ size ratio × prey size** (`size_ratio` 1.2).
- **Capture** happens when an eligible predator is within the capture radius (1 unit) of the prey. Speed matters only through whether contact is made.
- The predator gains **efficiency × content × prey size** energy (0.8 × 15 × size, tuned), never beyond its capacity.
- Predators are handled in order of their IDs. Each captures at most one prey per tick, and each prey is captured at most once. With two predators touching one prey, the older (lower-ID) one wins.

## 7. Reproduction and inheritance (M-04, SCOPE 6.6)

- When an organism's energy reaches **threshold × capacity** (0.8, P), it produces exactly one offspring that tick.
  - The parent pays the offspring's starting energy (`offspring_energy_fraction` 0.3 of the **offspring's** capacity) plus a fixed overhead (0.5).
  - A parent that could not pay and keep some energy has no offspring.
- **The offspring:**
  - is the same species, at full size (no growth stage), age 0;
  - has generation = parent's generation + 1;
  - is placed at a random point within 2 units of the parent. If that point is not passable, it is placed on the parent's own spot.
- **Inheritance:** each trait is copied from the parent. With probability `probability` (0.1) it mutates: it is multiplied by `1 + N(0, magnitude)` (magnitude 0.05), then clipped to the valid range. This is clonal, asexual inheritance.
- **Birth order:** births are processed in order of parent ID, and new IDs follow birth order.
- **Population safety cap:** the total may not exceed `population_cap` (150,000). At the cap, births are suppressed and the app shows a warning. This is a program safeguard, not biology.

## 8. Death (SCOPE 6.7)

- **Starvation:** energy reaches 0 after the tick's energy charge.
- **Predation:** captured this tick.
- **Old age (optional, S-02):** older than the species' `max_age_seconds`. It is off by default (0), so starvation and predation are the causes of death in the baseline.

Each death is counted once, by cause; a prey that is captured in the same tick it would have starved counts as eaten. When a species reaches zero, the app announces the extinction with its simulated time and the run continues.

## 9. Behavior and navigation (M-09, SCOPE 6.8)

Behavior rules are fixed; only the three traits evolve.

| Species | Goal priority |
| --- | --- |
| Prey | 1. Flee the nearest predator within sense range. 2. Go to the richest worthwhile food cell within sense range. 3. Wander. |
| Predator | 1. Hunt the nearest prey it can eat within sense range. 2. Wander. |

### Decisions

- Each organism re-chooses its goal every 10 ticks (P), staggered by ID so not everyone decides at once.
- Prey check for predators every 2 ticks (P).
- **Food choice (P).** A prey keeps its current food cell while that cell:
  - still holds at least one bite, and
  - holds at least 75% of the best alternative in range.

  Otherwise it switches to the richest cell, preferring nearer cells, then lower cell numbers, on ties.
- This ranks single cells directly instead of SCOPE 6.8's "best 16-unit region, then its richest cell". The outcome is the same kind of choice, and it is already tested (decision 2026-11-12).

### Movement toward a goal

1. When a goal is set, a straight line to it is checked against the terrain. If the line is clear, the organism walks straight there.
2. If the line is blocked, a **route** is requested.
   - The route search runs on the terrain grid (A* with eight directions, never cutting corners). It prefers fast terrain.
   - It searches only within sense range plus a 16-unit margin, and at most 512 steps per request.
   - All requests share a budget of 4,000 search steps per tick. Requests wait in a fixed queue ordered by request time, then ID.
3. A route is reused until the goal changes or the organism drifts off it. A predator chasing a moving prey re-plans at most every 15 ticks.
4. **Stuck recovery.** Progress is measured along the path still to walk (through the remaining route, then to the goal), so walking around a wall counts as progress. An organism whose remaining path has not shortened by 1 unit in 60 ticks (P) re-plans once. If it is still stuck, it marks that target unreachable for 300 ticks (10 s) and picks another, or wanders. Unreachable targets found by the route search are marked the same way.
5. **Fleeing** is straight away from the predator, with wall sliding. Fleeing prey can be cornered against water, rock or the world edge.
6. A blocked wanderer turns by a fixed angle (no randomness), so wanderers do not press against walls.
7. **Senses ignore walls.** An organism can see food or prey across a lake (no line of sight, W-10).

## 10. Order of events in one tick (SCOPE 6.9)

1. Decide goals: threat checks, food or prey choice, blocked-path checks.
2. Serve queued route requests within the budget.
3. Move.
4. Feed: prey eat plants; then predators capture.
5. Charge energy.
6. Age by one tick; mark starved or old organisms.
7. Collect births, in order of parent ID.
8. Remove the dead, then add the newborns with new IDs.
9. Regrow food (with the famine factor, if any).
10. Rebuild the spatial grid.
11. Record chart samples every 30 ticks (P).

## 11. Determinism and reproducibility (SCOPE 6.10)

- All randomness comes from four separate random number generators, called domains:

  | Domain | Used for |
  | --- | --- |
  | terrain | the terrain map |
  | initial population | starting positions and traits |
  | behavior | wandering directions |
  | mutation | mutations and offspring placement |

  Drawing from one domain never changes another's sequence.
- No wall-clock time, operating-system randomness or hash-map ordering affects results.
- The same scenario, seed and build on the same machine give bit-identical results, checked by a 64-bit state hash that covers every organism, the food grid, the counters, the route queue and the generator positions.
- The app and `headless` print the same hash at the same tick for the same scenario and seed.
- Identical results across Windows and Linux builds are likely but not guaranteed (W-12).

## 12. Simplifying assumptions (SCOPE 6.11)

| Assumption | Reality it simplifies | Why it is acceptable here |
| --- | --- | --- |
| Clonal asexual reproduction with mutation | Most animals reproduce sexually with recombination | Selection on heritable variation is the teaching target |
| Phenotype equals genotype; offspring born at adult size | Development, growth, plasticity | Keeps inheritance traceable trait by trait |
| One energy scalar, one plant-food resource | Multiple nutrients, water, temperature | Enough to create trade-offs between the three traits |
| Simple linear and quadratic energy costs, tuned for visible trade-offs | Measured metabolic scaling | Units are abstract; a scaling-law refinement is future work (W-17) |
| Fixed rule-based behavior that does not evolve | Behavior is itself heritable and learned | Behavior evolution is out of scope (W-03) |
| Perfect omnidirectional sensing within range, ignoring barriers | Sensory limits, occlusion | Line of sight is a future extension (W-10) |
| Deterministic capture on contact given a size ratio | Probabilistic hunts, handling time, satiation | Easy to explain and test in class |
| Exactly one prey and one predator species | Food webs | Enough to show predation as a selection pressure |
| Organisms overlap; no body collisions | Physical crowding | Large performance saving; crowding is not a required pressure |
| Static terrain; only food changes | Seasons, erosion, weather | Weather is deferred (SCOPE 9) |
| Discrete time steps | Continuous time | Standard in agent-based models; required for determinism |
| Population safety cap | No such cap in nature | Protects the program; triggers a visible warning |

## 13. What to expect from the baseline preset

The baseline preset is described here as measured with `headless`, seeds 1 to 5, 180,000 ticks (100 simulated minutes, which is 10 real minutes at 10×). The exact numbers are in the README's balancing record.

- **Early on:** prey grow from 2,000 to roughly 3,000 and predators from 150 to several hundred, with thousands of successful predations.
- **While predators are present:** prey speed rises (faster prey escape), and predator speed rises too: an arms race. In four of the five measured seeds, predators eventually pay so much for their speed that they die out, after about 45 to 63 simulated minutes; in the fifth they persist through all 100 minutes.
- **After the predators are gone:** selection relaxes. Prey sense range falls because sensing costs energy and there is nothing left to flee from, prey shrink because small bodies are cheaper, and the prey population grows.
- Prey pass roughly 100 generations in 100 simulated minutes.

Different seeds give different details. A trait that barely moves is not evidence that selection is absent: the starting traits may already suit the environment, which is why selection is verified directly by the mechanism tests (SCOPE 15.3).

## 14. Performance

Performance figures are measured on the machine named in [BENCHMARKS.md](BENCHMARKS.md) and apply only to it. Nothing is claimed about classroom hardware (SCOPE 10.3).
