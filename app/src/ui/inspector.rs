//! Right panel: the organism inspector (M-12), population numbers (M-13, S-05) and the
//! color legend (M-10).

use egui::RichText;
use sim::{Species, TerrainClass, TraitId};

use crate::render::{self, ColorBy};
use crate::session::Session;

use super::{UiState, format_sim_time, group_digits, rgb, species_color};

fn species_name(species: Species) -> &'static str {
    match species {
        Species::Prey => "Prey",
        Species::Predator => "Predator",
    }
}

fn trait_label(id: TraitId) -> &'static str {
    match id {
        TraitId::Speed => "Speed",
        TraitId::Size => "Size",
        TraitId::Sense => "Sense range",
    }
}

/// The inspector: ID, species, generation, age, energy and capacity, the three traits
/// beside the species' current mean, goal and terrain (M-12). A dead selection says so;
/// Esc clears it (T-INSP-3).
fn inspector(ui: &mut egui::Ui, session: &Session, state: &mut UiState) {
    ui.heading("Inspector");
    let Some(id) = state.selected else {
        ui.weak("Click an organism in the world to inspect it.");
        return;
    };
    let s = session.simulation();
    let Some(o) = s.organism_snapshot(id) else {
        ui.label(format!("Organism #{id} is no longer alive."));
        if ui.button("Clear (Esc)").clicked() {
            state.selected = None;
            state.follow_selected = false;
        }
        return;
    };
    let stats = s.species_stats(o.species);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(species_name(o.species))
                .strong()
                .color(species_color(o.species)),
        );
        ui.label(format!("#{id}"));
    });
    egui::Grid::new("inspector")
        .num_columns(2)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label("Generation");
            ui.label(o.generation.to_string());
            ui.end_row();
            ui.label("Age");
            ui.label(format_sim_time(o.age_seconds));
            ui.end_row();
            ui.label("Energy");
            ui.add(
                egui::ProgressBar::new((o.energy / o.capacity).clamp(0.0, 1.0))
                    .desired_width(120.0)
                    .text(format!("{:.1} / {:.1}", o.energy, o.capacity)),
            );
            ui.end_row();
            ui.label("Doing");
            ui.label(o.goal.name());
            ui.end_row();
            ui.label("Standing on");
            ui.label(o.terrain.name());
            ui.end_row();
        });
    ui.add_space(4.0);
    egui::Grid::new("inspector_traits")
        .num_columns(3)
        .striped(true)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Trait").small());
            ui.label(RichText::new("This one").small());
            ui.label(RichText::new(format!("{} mean", species_name(o.species))).small());
            ui.end_row();
            for id in TraitId::ALL {
                let mean = match id {
                    TraitId::Speed => stats.mean_speed,
                    TraitId::Size => stats.mean_size,
                    TraitId::Sense => stats.mean_sense,
                };
                let value = o.traits.get(id);
                ui.label(trait_label(id));
                ui.label(format!("{value:.2}"));
                match mean {
                    Some(mean) => {
                        let diff = (f64::from(value) / mean - 1.0) * 100.0;
                        ui.label(format!("{mean:.2} ({diff:+.0}%)"));
                    }
                    None => {
                        ui.label("–");
                    }
                }
                ui.end_row();
            }
        });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.follow_selected, "Follow")
            .on_hover_text("Keep the camera on this organism");
        if ui.button("Clear (Esc)").clicked() {
            state.selected = None;
            state.follow_selected = false;
        }
    });
}

/// Population numbers (M-13): population, mean and maximum generation per species;
/// cumulative births, deaths by cause (S-05) and successful predations; elapsed time.
fn populations(ui: &mut egui::Ui, session: &Session) {
    let s = session.simulation();
    ui.heading("Populations");
    ui.label(format!("Elapsed {}", format_sim_time(s.sim_seconds())));
    let counters = *s.counters();
    egui::Grid::new("populations")
        .num_columns(3)
        .striped(true)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label("");
            for species in Species::ALL {
                ui.label(
                    RichText::new(species_name(species))
                        .strong()
                        .color(species_color(species)),
                );
            }
            ui.end_row();
            let stats = [
                s.species_stats(Species::Prey),
                s.species_stats(Species::Predator),
            ];
            let row = |ui: &mut egui::Ui, label: &str, values: [String; 2]| {
                ui.label(label);
                for value in values {
                    ui.label(value);
                }
                ui.end_row();
            };
            row(
                ui,
                "Alive",
                stats.map(|st| group_digits(st.population as u64)),
            );
            row(
                ui,
                "Mean generation",
                stats.map(|st| st.mean_generation.map_or("–".into(), |g| format!("{g:.1}"))),
            );
            row(
                ui,
                "Max generation",
                stats.map(|st| st.max_generation.map_or("–".into(), |g| g.to_string())),
            );
            let per = |f: fn(&sim::SpeciesCounters) -> u64| {
                [
                    group_digits(f(&counters.prey)),
                    group_digits(f(&counters.predator)),
                ]
            };
            row(ui, "Births", per(|c| c.births));
            row(ui, "Deaths", per(|c| c.deaths()));
            row(ui, "  starved", per(|c| c.starvation_deaths));
            row(ui, "  eaten", per(|c| c.predation_deaths));
            row(ui, "  old age", per(|c| c.age_deaths));
        });
    ui.label(format!(
        "Successful predations: {}",
        group_digits(counters.predations())
    ));
}

fn swatch(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 3.0, color);
        ui.label(label);
    });
}

/// The legend (M-10): organism colors and terrain classes.
fn legend(ui: &mut egui::Ui, state: &UiState) {
    ui.heading("Legend");
    match state.color_by {
        ColorBy::Species => {
            swatch(ui, species_color(Species::Prey), "Prey (circles)");
            swatch(
                ui,
                species_color(Species::Predator),
                "Predators (triangles)",
            );
        }
        ColorBy::Trait(id) => {
            let (low, high) = render::trait_display_range(id);
            ui.label(format!(
                "{} (circles: prey, triangles: predators)",
                trait_label(id)
            ));
            ui.horizontal(|ui| {
                ui.label(format!("{low}"));
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
                let steps = 24;
                for k in 0..steps {
                    let t = k as f32 / (steps - 1) as f32;
                    let x0 = rect.left() + rect.width() * k as f32 / steps as f32;
                    let x1 = rect.left() + rect.width() * (k + 1) as f32 / steps as f32;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(x0, rect.top()),
                            egui::pos2(x1, rect.bottom()),
                        ),
                        0.0,
                        rgb(render::ramp(t)),
                    );
                }
                ui.label(format!("{high}"));
            });
        }
    }
    ui.add_space(2.0);
    for class in TerrainClass::ALL {
        let color = render::cell_color(class, 1.0, 1.0, true);
        let label = match class {
            TerrainClass::Water | TerrainClass::Rock => format!("{} (impassable)", class.name()),
            _ => format!(
                "{} (speed ×{}, food ×{})",
                class.name(),
                class.speed_multiplier(),
                class.fertility()
            ),
        };
        swatch(
            ui,
            egui::Color32::from_rgb(color[0], color[1], color[2]),
            &label,
        );
    }
    if state.show_food {
        ui.small("Brighter green = more food; brown = grazed.");
    }
}

/// The right panel.
pub fn panel(ui: &mut egui::Ui, session: &Session, state: &mut UiState) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        inspector(ui, session, state);
        ui.separator();
        populations(ui, session);
        ui.separator();
        legend(ui, state);
        ui.separator();
        ui.small("Left-click: inspect · Right/middle-drag or WASD: pan · Wheel: zoom · Home: fit · Space: play/pause · period: step · F3: performance");
    });
}
