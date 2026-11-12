//! The control bar (M-11, M-15), status bar (M-11, M-13), About panel (S-04) and
//! performance overlay (M-18).

use egui::RichText;
use sim::Species;

use crate::driver::Speed;
use crate::render::ColorBy;
use crate::session::Session;

use super::{Notice, UiState, WARNING, format_sim_time, group_digits};

fn speed_label(speed: Speed) -> &'static str {
    match speed {
        Speed::X1 => "1×",
        Speed::X2 => "2×",
        Speed::X5 => "5×",
        Speed::X10 => "10×",
        Speed::Max => "Max",
    }
}

/// A run seed for "New seed": readable, from the clock (the app may read the clock; the
/// seed is then shown and fully reproducible).
pub fn fresh_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (nanos % 999_999) as u64 + 1
}

/// Top control bar: preset picker, run controls, speeds, view options and windows.
pub fn top_bar(ui: &mut egui::Ui, session: &mut Session, state: &mut UiState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Natural Selection Simulator").strong());
        ui.separator();

        // Preset picker (M-15): selecting a preset loads it, paused at tick 0.
        let current = match state.preset_index {
            Some(index) => state.presets[index].config.name.clone(),
            None => format!("File: {}", session.applied().name),
        };
        let mut chosen = None;
        egui::ComboBox::from_id_salt("preset")
            .selected_text(current)
            .width(210.0)
            .show_ui(ui, |ui| {
                for (index, preset) in state.presets.iter().enumerate() {
                    if ui
                        .selectable_label(state.preset_index == Some(index), &preset.config.name)
                        .on_hover_text(&preset.config.description)
                        .clicked()
                    {
                        chosen = Some(index);
                    }
                }
            })
            .response
            .on_hover_text("Scenario preset");
        if let Some(index) = chosen {
            let preset = &state.presets[index];
            let seed = preset.config.seed.unwrap_or_else(fresh_seed);
            match session.load(preset.config.clone(), seed, preset.file) {
                Ok(()) => {
                    state.preset_index = Some(index);
                    state.selected = None;
                    state.fit_requested = true;
                    state.params_notice = None;
                }
                Err(errors) => {
                    state.params_notice = Some(Notice::error(join_errors(&errors)));
                }
            }
        }
        ui.separator();

        let paused = session.driver().is_paused();
        let play_label = if paused { "▶ Play" } else { "⏸ Pause" };
        if ui
            .add(
                egui::Button::new(RichText::new(play_label).strong())
                    .min_size(egui::vec2(72.0, 0.0)),
            )
            .on_hover_text("Space")
            .clicked()
        {
            session.toggle_play();
        }
        if ui
            .button("Step")
            .on_hover_text("Run exactly one tick (.)")
            .clicked()
        {
            session.step_once();
        }
        let reset_tip = if session.has_pending_changes() {
            "Back to tick 0, applying your parameter changes"
        } else {
            "Back to tick 0 with the same seed"
        };
        if ui.button("⟲ Reset").on_hover_text(reset_tip).clicked() {
            match session.reset() {
                Ok(()) => state.params_notice = None,
                Err(errors) => state.params_notice = Some(Notice::error(join_errors(&errors))),
            }
        }
        if ui
            .button("New seed")
            .on_hover_text("Reset with a different random seed")
            .clicked()
        {
            match session.new_seed(fresh_seed()) {
                Ok(()) => state.params_notice = None,
                Err(errors) => state.params_notice = Some(Notice::error(join_errors(&errors))),
            }
        }
        ui.separator();

        ui.label("Speed");
        for speed in Speed::ALL {
            let selected = session.driver().speed() == speed;
            if ui.selectable_label(selected, speed_label(speed)).clicked() && !selected {
                session.set_speed(speed);
            }
        }
        ui.separator();

        if ui
            .button("Fit map")
            .on_hover_text("Show the whole world (Home)")
            .clicked()
        {
            state.fit_requested = true;
        }
        egui::ComboBox::from_id_salt("color_by")
            .selected_text(format!("Color: {}", state.color_by.label()))
            .show_ui(ui, |ui| {
                for option in ColorBy::ALL {
                    ui.selectable_value(&mut state.color_by, option, option.label());
                }
            });
        if ui.checkbox(&mut state.show_food, "Food").changed() {
            session.request_rebuild();
        }
        ui.separator();
        if ui
            .button("Scenarios…")
            .on_hover_text("Load, save and share scenario files")
            .clicked()
        {
            state.show_scenarios = !state.show_scenarios;
            if state.show_scenarios {
                super::scenarios::refresh_files(state);
            }
        }
        ui.toggle_value(&mut state.show_about, "About");
        ui.toggle_value(&mut state.show_overlay, "Perf (F3)");
    });
}

/// Status bar: run state, requested and achieved speed, tick, simulated time, seed, hash
/// (while paused), backlog, extinction notices and the safety-cap warning.
pub fn status_bar(ui: &mut egui::Ui, session: &mut Session) {
    let paused = session.driver().is_paused();
    let speed = session.driver().speed();
    let achieved = session.driver().achieved_speed();
    let behind = session.driver().is_behind();
    let backlog = session.driver().backlog_sim_seconds();
    let hash = paused.then(|| session.state_hash());
    let s = session.simulation();
    ui.horizontal_wrapped(|ui| {
        if paused {
            ui.label(RichText::new("⏸ Paused").strong());
        } else {
            ui.label(RichText::new("▶ Running").strong());
        }
        let requested = speed_label(speed);
        match (paused, achieved) {
            (false, Some(achieved)) => {
                ui.label(format!("{requested} requested, {achieved:.1}× achieved"));
            }
            _ => {
                ui.label(format!("{requested} requested"));
            }
        }
        ui.separator();
        ui.label(format!("Tick {}", group_digits(s.tick())));
        ui.label(format!("Time {}", format_sim_time(s.sim_seconds())));
        ui.separator();
        ui.label(format!("Seed {}", s.run_seed()));
        if let Some(hash) = hash {
            ui.monospace(format!("Hash 0x{hash:016x}"))
                .on_hover_text("State hash: identical in headless for the same scenario, seed and tick");
        } else {
            ui.weak("Hash shown while paused");
        }
        if behind && !paused {
            ui.separator();
            ui.colored_label(WARNING, format!("Behind by {backlog:.1} s of simulated time"));
        }
        for species in Species::ALL {
            if let Some(tick) = s.extinct_at(species) {
                ui.separator();
                ui.colored_label(
                    WARNING,
                    format!(
                        "{} extinct at {}",
                        if species == Species::Prey { "Prey" } else { "Predators" },
                        format_sim_time(tick as f64 / f64::from(s.ticks_per_second()))
                    ),
                );
            }
        }
        if s.population_cap_reached() {
            ui.separator();
            ui.colored_label(
                super::ERROR,
                "⚠ Population safety cap reached: births are suppressed (a program safeguard, not biology)",
            );
        }
    });
}

/// The performance overlay (F3): rolling 60-second frame statistics (SCOPE 7.3).
pub fn overlay(ctx: &egui::Context, session: &Session, state: &mut UiState) {
    let perf = state.perf;
    let s = session.simulation();
    let tps = f64::from(s.ticks_per_second());
    egui::Window::new("Performance")
        .open(&mut state.show_overlay)
        .default_pos([1000.0, 80.0])
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("perf").num_columns(2).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, label: &str, value: String| {
                    ui.label(label);
                    ui.monospace(value);
                    ui.end_row();
                };
                row(
                    ui,
                    "Window",
                    format!("{:.0} s, {} frames", perf.window_seconds, perf.frames),
                );
                row(
                    ui,
                    "Median frame",
                    format!("{:.2} ms", perf.median_frame_ms),
                );
                row(ui, "p95 frame", format!("{:.2} ms", perf.p95_frame_ms));
                row(
                    ui,
                    "Frames > 50 ms",
                    format!("{:.2} %", perf.slow_frame_share * 100.0),
                );
                row(
                    ui,
                    "Simulation / frame",
                    format!("{:.2} ms", perf.sim_ms_per_frame),
                );
                row(
                    ui,
                    "Achieved speed",
                    format!("{:.2}×", perf.ticks_per_second / tps),
                );
                row(ui, "Organisms", group_digits(s.organisms().len() as u64));
                row(ui, "Stall-capped frames", state.stall_count.to_string());
                row(
                    ui,
                    "Route requests queued",
                    s.queued_route_requests().to_string(),
                );
            });
            ui.small("Reference-machine measurements only (SCOPE 10.3).");
        });
}

/// About this model (S-04): the rules and simplifications in plain language.
pub fn about(ctx: &egui::Context, state: &mut UiState) {
    egui::Window::new("About this model")
        .open(&mut state.show_about)
        .default_width(460.0)
        .default_pos([340.0, 90.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                ui.label(RichText::new("What happens").strong());
                ui.label(
                    "Prey eat plant food that grows on the terrain. Predators eat prey they touch, \
                     if they are big enough. Every organism spends energy each second; faster, \
                     bigger and longer-sighted organisms spend more. An organism whose energy \
                     reaches zero starves. When an organism's energy is high enough it splits off \
                     one offspring, which inherits its speed, size and sense range, sometimes \
                     with a small random mutation.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Why traits change").strong());
                ui.label(
                    "Nobody chooses the traits. Organisms whose traits help them eat, escape and \
                     reproduce leave more offspring, so their traits become more common. The \
                     charts show the averages shifting over generations. A trait that barely \
                     moves is not evidence that selection is absent: the starting traits may \
                     already suit the environment.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Simplifications").strong());
                for line in [
                    "Units of space, time and energy are abstract; results are qualitative, not predictions.",
                    "Reproduction is asexual and clonal; offspring are born adult-sized.",
                    "Behavior is fixed: only speed, size and sense range evolve.",
                    "Sensing sees through walls; organisms overlap without colliding.",
                    "One plant food, one prey species and one predator species; the terrain never changes.",
                    "The population safety cap is a program safeguard, not biology.",
                    "Extinction is a possible, valid outcome.",
                ] {
                    ui.label(format!("• {line}"));
                }
                ui.add_space(6.0);
                ui.small("Full description: docs/MODEL.md in the project folder.");
            });
        });
}

pub fn join_errors(errors: &[sim::ValidationError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_seeds_are_short_and_non_zero() {
        for _ in 0..10 {
            let seed = fresh_seed();
            assert!((1..=1_000_000).contains(&seed));
        }
    }
}
