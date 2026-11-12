//! Parameter panel (M-14, S-12; SCOPE 7.3). Simple controls are always visible; the
//! essential Advanced set is collapsed by default; every other parameter is in a further
//! collapsed section. All ranges come from the `limits` module (I-07), so the UI cannot
//! enter an out-of-range value (T-VAL-3). Edits change the session's draft and apply on
//! Reset; while a run is going the panel is read-only.

use egui::RichText;
use sim::limits;
use sim::{FamineConfig, ScenarioConfig, SpeciesConfig, StartPopulation, TraitId};

use crate::session::Session;

use super::controls::join_errors;
use super::{ERROR, Notice, UiState, WARNING, float_slider, int_drag};

/// Starting population editor: a count, or a density shown with its estimated count.
fn start_editor(ui: &mut egui::Ui, label: &str, species: &mut SpeciesConfig, passable: usize) {
    let estimate = species.resolved_start_count(passable);
    let mut count = estimate.min(limits::START_COUNT.max) as u32;
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::DragValue::new(&mut count)
                .range(limits::START_COUNT.min as u32..=limits::START_COUNT.max as u32)
                .speed(5.0),
        );
        ui.label(label);
        if let StartPopulation::Density(density) = species.start {
            ui.weak(format!("({density} per 1,000 cells)"));
        }
        if response.changed() {
            species.start = StartPopulation::Count(count);
        }
    });
}

fn trait_grid(ui: &mut egui::Ui, config: &mut ScenarioConfig) {
    egui::Grid::new("trait_grid")
        .num_columns(5)
        .spacing([8.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Prey mean").small());
            ui.label(RichText::new("spread").small());
            ui.label(RichText::new("Predator mean").small());
            ui.label(RichText::new("spread").small());
            ui.end_row();
            for id in TraitId::ALL {
                ui.label(match id {
                    TraitId::Speed => "Speed",
                    TraitId::Size => "Size",
                    TraitId::Sense => "Sense",
                });
                for species in [&mut config.species.prey, &mut config.species.predator] {
                    let start = species.trait_start_mut(id);
                    let range = id.range();
                    let spread = id.spread_range();
                    ui.add(
                        egui::DragValue::new(&mut start.mean)
                            .range(range.min..=range.max)
                            .speed(f64::from(range.max - range.min) / 500.0)
                            .max_decimals(2),
                    );
                    ui.add(
                        egui::DragValue::new(&mut start.spread)
                            .range(spread.min..=spread.max)
                            .speed(f64::from(spread.max - spread.min) / 1000.0)
                            .max_decimals(2),
                    );
                }
                ui.end_row();
            }
        });
}

fn seed_editor(ui: &mut egui::Ui, label: &str, seed: &mut u64) {
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(seed)
                .range(0..=limits::SEED.max)
                .speed(1.0),
        );
        ui.label(label);
    });
}

/// Every parameter not in the simple or essential Advanced sets (S-12).
fn more_parameters(ui: &mut egui::Ui, c: &mut ScenarioConfig) {
    ui.label(RichText::new("Food").strong());
    float_slider(
        ui,
        "regrowth / tick",
        &mut c.food.regrowth_per_tick,
        limits::FOOD_REGROWTH_PER_TICK,
        "Food each cell regrows per tick, up to its cap",
    );
    float_slider(
        ui,
        "max per cell",
        &mut c.food.max_per_cell,
        limits::FOOD_MAX_PER_CELL,
        "Food a fully fertile cell holds at abundance 1",
    );
    float_slider(
        ui,
        "starting fraction",
        &mut c.food.initial_fraction,
        limits::FOOD_INITIAL_FRACTION,
        "Starting food as a fraction of each cell's cap",
    );
    float_slider(
        ui,
        "intake / tick",
        &mut c.food.intake_per_tick,
        limits::FOOD_INTAKE_PER_TICK,
        "Most food one prey eats per tick",
    );
    float_slider(
        ui,
        "energy / food",
        &mut c.food.energy_per_unit,
        limits::FOOD_ENERGY_PER_UNIT,
        "Energy gained per unit of food",
    );

    ui.label(RichText::new("Energy formula (per second)").strong());
    float_slider(
        ui,
        "capacity / size",
        &mut c.energy.capacity,
        limits::ENERGY_CAPACITY,
        "Energy capacity = this × size",
    );
    float_slider(
        ui,
        "base",
        &mut c.energy.base,
        limits::ENERGY_BASE,
        "Cost of living, whatever the traits",
    );
    float_slider(
        ui,
        "size",
        &mut c.energy.size,
        limits::ENERGY_SIZE,
        "Maintenance cost per unit of size",
    );
    float_slider(
        ui,
        "move",
        &mut c.energy.movement,
        limits::ENERGY_MOVEMENT,
        "Movement cost × effort × size × speed²",
    );
    float_slider(
        ui,
        "sense",
        &mut c.energy.sense,
        limits::ENERGY_SENSE,
        "Sensing cost per unit of sense range",
    );

    ui.label(RichText::new("Predation").strong());
    float_slider(
        ui,
        "capture radius",
        &mut c.predation.capture_radius,
        limits::CAPTURE_RADIUS,
        "Capture distance in world units",
    );
    float_slider(
        ui,
        "efficiency",
        &mut c.predation.efficiency,
        limits::EFFICIENCY,
        "Share of the prey's energy content the predator gains",
    );
    float_slider(
        ui,
        "content / size",
        &mut c.predation.content,
        limits::PREY_CONTENT,
        "Prey energy content per unit of size",
    );

    ui.label(RichText::new("Reproduction").strong());
    float_slider(
        ui,
        "threshold",
        &mut c.reproduction.threshold,
        limits::REPRODUCTION_THRESHOLD,
        "Reproduce at this fraction of capacity",
    );
    float_slider(
        ui,
        "offspring energy",
        &mut c.reproduction.offspring_energy_fraction,
        limits::OFFSPRING_ENERGY_FRACTION,
        "Offspring starting energy as a fraction of its capacity",
    );
    float_slider(
        ui,
        "overhead",
        &mut c.reproduction.overhead,
        limits::REPRODUCTION_OVERHEAD,
        "Energy lost per birth",
    );
    float_slider(
        ui,
        "spawn radius",
        &mut c.reproduction.spawn_radius,
        limits::SPAWN_RADIUS,
        "Offspring are placed within this distance",
    );

    ui.label(RichText::new("Terrain").strong());
    float_slider(
        ui,
        "rock level",
        &mut c.terrain.rock_level,
        limits::ROCK_LEVEL,
        "Elevation rank above which cells are rock",
    );
    float_slider(
        ui,
        "feature scale",
        &mut c.terrain.feature_scale,
        limits::FEATURE_SCALE,
        "Typical size of lakes and hills",
    );

    ui.label(RichText::new("Behavior and navigation").strong());
    int_drag(
        ui,
        "decision interval (ticks)",
        &mut c.behavior.decision_interval,
        limits::DECISION_INTERVAL,
        "Ticks between goal decisions",
    );
    int_drag(
        ui,
        "threat check interval (ticks)",
        &mut c.behavior.threat_interval,
        limits::THREAT_INTERVAL,
        "Ticks between prey threat checks",
    );
    float_slider(
        ui,
        "wander speed",
        &mut c.behavior.wander_speed_fraction,
        limits::WANDER_SPEED_FRACTION,
        "Wandering speed as a fraction of maximum",
    );
    int_drag(
        ui,
        "expansions / request",
        &mut c.navigation.expansions_per_request,
        limits::ROUTE_EXPANSIONS_PER_REQUEST,
        "Route search cap per request",
    );
    int_drag(
        ui,
        "expansions / tick",
        &mut c.navigation.budget_per_tick,
        limits::ROUTE_BUDGET_PER_TICK,
        "Route search budget per tick",
    );
    int_drag(
        ui,
        "stuck window (ticks)",
        &mut c.navigation.stuck_window,
        limits::STUCK_WINDOW,
        "Ticks without progress before re-planning",
    );
    int_drag(
        ui,
        "unreachable for (ticks)",
        &mut c.navigation.unreachable_ticks,
        limits::UNREACHABLE_TICKS,
        "How long an abandoned target is avoided",
    );

    ui.label(RichText::new("Lifespan and safety").strong());
    float_slider(
        ui,
        "prey max age (s, 0 = off)",
        &mut c.species.prey.max_age_seconds,
        limits::MAX_AGE_SECONDS,
        "Death from old age (S-02)",
    );
    float_slider(
        ui,
        "predator max age (s, 0 = off)",
        &mut c.species.predator.max_age_seconds,
        limits::MAX_AGE_SECONDS,
        "Death from old age (S-02)",
    );
    int_drag(
        ui,
        "population safety cap",
        &mut c.population_cap,
        limits::POPULATION_CAP,
        "Births stop at this total (a program safeguard)",
    );

    ui.label(RichText::new("Famine (S-01)").strong());
    let mut famine = c.famine.is_some();
    if ui.checkbox(&mut famine, "Scheduled famine").changed() {
        c.famine = famine.then(FamineConfig::default);
    }
    if let Some(f) = &mut c.famine {
        let tps = f64::from(c.ticks_per_second.max(1));
        let mut minutes = f.at_tick as f64 / tps / 60.0;
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::DragValue::new(&mut minutes)
                        .range(0.0..=10_000.0)
                        .speed(0.1)
                        .max_decimals(1),
                )
                .changed()
            {
                f.at_tick = ((minutes * 60.0 * tps).round() as u64).min(limits::FAMINE_AT_TICK.max);
            }
            ui.label("starts at (simulated minutes)");
        });
        float_slider(
            ui,
            "food cap multiplier",
            &mut f.food_multiplier,
            limits::FAMINE_FOOD_MULTIPLIER,
            "Regrowth cap multiplier during the famine",
        );
    }
}

/// The left panel.
pub fn panel(ui: &mut egui::Ui, session: &mut Session, state: &mut UiState) {
    let running = !session.driver().is_paused();
    let passable = session.simulation().terrain().passable_cells().len();
    // Fixing the terrain seed keeps the map currently shown.
    let current_terrain_seed = session.simulation().terrain_seed().min(limits::SEED.max);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading(&session.applied().name);
            let goal = &session.applied().learning_goal;
            if !goal.is_empty() {
                ui.label(RichText::new(goal).italics());
            }
            ui.separator();
            if running {
                ui.colored_label(
                    WARNING,
                    "Pause to change parameters. Changes apply on Reset.",
                );
            }
            ui.add_enabled_ui(!running, |ui| {
                let draft = &mut session.draft;
                ui.label(RichText::new("Starting conditions").strong());
                start_editor(ui, "prey", &mut draft.species.prey, passable);
                start_editor(ui, "predators", &mut draft.species.predator, passable);
                float_slider(
                    ui,
                    "food abundance",
                    &mut draft.food.abundance,
                    limits::FOOD_ABUNDANCE,
                    "Multiplies how much food every cell can hold",
                );
                ui.add_space(4.0);
                egui::CollapsingHeader::new("Advanced")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Starting traits").strong());
                        trait_grid(ui, draft);
                        ui.label(RichText::new("Mutation").strong());
                        float_slider(
                            ui,
                            "probability",
                            &mut draft.mutation.probability,
                            limits::MUTATION_PROBABILITY,
                            "Chance each trait of an offspring mutates",
                        );
                        float_slider(
                            ui,
                            "magnitude (σ)",
                            &mut draft.mutation.magnitude,
                            limits::MUTATION_MAGNITUDE,
                            "A mutated value is multiplied by 1 + N(0, σ)",
                        );
                        ui.label(RichText::new("Predators").strong());
                        float_slider(
                            ui,
                            "size ratio",
                            &mut draft.predation.size_ratio,
                            limits::SIZE_RATIO,
                            "A predator can eat a prey only if its size ≥ ratio × prey size",
                        );
                        ui.label(RichText::new("Terrain").strong());
                        let mut fixed = draft.terrain.seed.is_some();
                        if ui
                            .checkbox(&mut fixed, "Fixed terrain seed")
                            .on_hover_text("Off: the map changes with the run seed")
                            .changed()
                        {
                            draft.terrain.seed = fixed.then_some(current_terrain_seed);
                        }
                        if let Some(seed) = &mut draft.terrain.seed {
                            seed_editor(ui, "terrain seed", seed);
                        }
                        float_slider(
                            ui,
                            "water level",
                            &mut draft.terrain.water_level,
                            limits::WATER_LEVEL,
                            "Share of the map under water",
                        );
                    });
                egui::CollapsingHeader::new("More parameters")
                    .default_open(false)
                    .show(ui, |ui| more_parameters(ui, draft));
                ui.separator();
                seed_editor(ui, "run seed", &mut session.draft_seed);
            });

            let errors = session.draft_errors();
            for error in &errors {
                ui.colored_label(ERROR, error.to_string());
            }
            ui.horizontal(|ui| {
                let can_apply = !running && errors.is_empty() && session.has_pending_changes();
                if ui
                    .add_enabled(
                        can_apply,
                        egui::Button::new(RichText::new("Apply and reset").strong()),
                    )
                    .on_hover_text("Rebuild the run at tick 0 with these parameters")
                    .clicked()
                {
                    match session.reset() {
                        Ok(()) => {
                            state.params_notice = Some(Notice::ok("Applied."));
                            state.selected = None;
                        }
                        Err(errors) => {
                            state.params_notice = Some(Notice::error(join_errors(&errors)))
                        }
                    }
                }
                if ui
                    .add_enabled(session.has_pending_changes(), egui::Button::new("Revert"))
                    .clicked()
                {
                    session.revert_draft();
                    state.params_notice = None;
                }
            });
            if session.has_pending_changes() && errors.is_empty() {
                ui.colored_label(WARNING, "Changes are not applied yet.");
            }
            if let Some(notice) = &state.params_notice {
                notice.show(ui);
            }
        });
}
