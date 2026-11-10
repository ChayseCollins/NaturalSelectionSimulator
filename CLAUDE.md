# CLAUDE.md

Guidance for Claude Code sessions in this repository. The scope of record is `SCOPE.md` (version 2.2); do not edit it unless a scope change is explicitly requested.

## Working rules

- Every implementation prompt must cite the relevant feature IDs (M-xx, S-xx, C-xx) and test IDs (T-xx) from `SCOPE.md`. Implement only what those IDs cover.
- Use only the pinned versions in the root `Cargo.toml` and `rust-toolchain.toml`. When writing Bevy or egui code, follow the APIs of those exact versions.

## Architecture invariants (SCOPE.md Section 7.4)

- **I-01:** `sim` depends on no graphics, UI or windowing crate; checked with `cargo tree -p sim`.
- **I-02:** all randomness in `sim` comes from its four random domains (terrain, initial population, behavior, mutation), each used only for its own purpose; no thread-local RNG, OS entropy or wall clock.
- **I-03:** no iteration over `HashMap` or `HashSet` in `sim`; use vectors or `BTreeMap`.
- **I-04:** within a phase, organisms are processed in ascending index; births and deaths are applied at the end of the tick in a fixed order; new IDs follow birth order.
- **I-05:** `step()` takes no time argument; the tick length comes only from configuration.
- **I-06:** simulation rules exist only in `sim`; `app` and `headless` call its public API and never duplicate a rule.
- **I-07:** every scenario parameter has its minimum, maximum and default in the `limits` module, used by both `validate()` and the UI.
- **I-08:** `sim` performs no file I/O; it parses and produces strings, and the shells read and write files.
- **I-09:** every feature lands with its must-priority tests; `cargo test --workspace` passes before the next feature starts.
- **I-10:** dependency versions are pinned in `[workspace.dependencies]`; no upgrade without a recorded decision.
- **I-11:** prefer the simplest code that delivers working behavior for one prey and one predator species. Do not add generic registries, plugin layers or abstractions for features not in the current week's plan.
