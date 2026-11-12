//! Live charts (M-13, S-03): population per species and one selectable trait chart (mean
//! speed, size or sense range, or mean and maximum generation) per species, against
//! simulated time in minutes. Series come straight from the simulation's metric history;
//! a species with no organisms has a gap, never 0 or NaN (T-CHART-2).

use egui_plot::{Corner, Legend, Line, Plot, PlotPoints};
use sim::{MetricSample, Species, SpeciesStats};

use crate::session::Session;

use super::{TraitChart, UiState, species_color};

/// Points `(minutes, value)` for the samples where `value` exists. Consecutive samples
/// with values form one segment; a missing value ends the segment (a gap).
pub fn segments(
    samples: &[MetricSample],
    species: Species,
    value: impl Fn(&SpeciesStats) -> Option<f64>,
) -> Vec<Vec<[f64; 2]>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for sample in samples {
        match value(sample.species(species)) {
            Some(v) if v.is_finite() => current.push([sample.sim_seconds / 60.0, v]),
            _ => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn add_series(
    plot: &mut egui_plot::PlotUi<'_>,
    samples: &[MetricSample],
    species: Species,
    name: &str,
    dashed: bool,
    value: impl Fn(&SpeciesStats) -> Option<f64>,
) {
    let color = species_color(species);
    for segment in segments(samples, species, value) {
        let mut line = Line::new(name, PlotPoints::from(segment))
            .color(color)
            .width(2.0);
        if dashed {
            line = line.style(egui_plot::LineStyle::dashed_dense());
        }
        plot.line(line);
    }
}

/// The bottom panel.
pub fn panel(ui: &mut egui::Ui, session: &Session, state: &mut UiState) {
    let samples = session.simulation().metrics().samples();
    let height = (ui.available_height() - 28.0).max(80.0);
    ui.horizontal(|ui| {
        ui.label("Trait chart:");
        for chart in TraitChart::ALL {
            ui.selectable_value(&mut state.trait_chart, chart, chart.label());
        }
        ui.separator();
        ui.weak(format!(
            "{} samples, every {} ticks",
            samples.len(),
            session.simulation().metrics().spacing()
        ));
    });
    let width = ((ui.available_width() - 12.0) / 2.0).max(60.0);
    ui.horizontal(|ui| {
        Plot::new("population_plot")
            .width(width)
            .height(height)
            .legend(Legend::default().position(Corner::LeftTop))
            .x_axis_label("simulated minutes")
            .y_axis_label("population")
            .include_y(0.0)
            .allow_scroll(false)
            .show(ui, |plot| {
                add_series(plot, samples, Species::Prey, "Prey", false, |s| {
                    Some(s.population as f64)
                });
                add_series(plot, samples, Species::Predator, "Predators", false, |s| {
                    Some(s.population as f64)
                });
            });
        let chart = state.trait_chart;
        Plot::new(("trait_plot", chart.label()))
            .width(width)
            .height(height)
            .legend(Legend::default().position(Corner::LeftTop))
            .x_axis_label("simulated minutes")
            .y_axis_label(chart.label())
            .include_y(0.0)
            .allow_scroll(false)
            .show(ui, |plot| match chart {
                TraitChart::Speed => {
                    add_series(plot, samples, Species::Prey, "Prey speed", false, |s| {
                        s.mean_speed
                    });
                    add_series(
                        plot,
                        samples,
                        Species::Predator,
                        "Predator speed",
                        false,
                        |s| s.mean_speed,
                    );
                }
                TraitChart::Size => {
                    add_series(plot, samples, Species::Prey, "Prey size", false, |s| {
                        s.mean_size
                    });
                    add_series(
                        plot,
                        samples,
                        Species::Predator,
                        "Predator size",
                        false,
                        |s| s.mean_size,
                    );
                }
                TraitChart::Sense => {
                    add_series(plot, samples, Species::Prey, "Prey sense", false, |s| {
                        s.mean_sense
                    });
                    add_series(
                        plot,
                        samples,
                        Species::Predator,
                        "Predator sense",
                        false,
                        |s| s.mean_sense,
                    );
                }
                TraitChart::Generation => {
                    add_series(plot, samples, Species::Prey, "Prey mean", false, |s| {
                        s.mean_generation
                    });
                    add_series(plot, samples, Species::Prey, "Prey max", true, |s| {
                        s.max_generation.map(f64::from)
                    });
                    add_series(
                        plot,
                        samples,
                        Species::Predator,
                        "Predator mean",
                        false,
                        |s| s.mean_generation,
                    );
                    add_series(
                        plot,
                        samples,
                        Species::Predator,
                        "Predator max",
                        true,
                        |s| s.max_generation.map(f64::from),
                    );
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use sim::{ScenarioConfig, Simulation, StartPopulation};

    use super::*;

    /// T-CHART-1 (display part): chart points equal the recorded samples.
    #[test]
    fn chart_points_are_the_recorded_samples() {
        let mut config = ScenarioConfig::default();
        config.world.width = 96.0;
        config.world.height = 96.0;
        let mut s = Simulation::new(&config, 3).expect("valid");
        s.step_n(300);
        let samples = s.metrics().samples();
        let lines = segments(samples, Species::Prey, |st| Some(st.population as f64));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), samples.len());
        for (point, sample) in lines[0].iter().zip(samples) {
            assert_eq!(point[0], sample.sim_seconds / 60.0);
            assert_eq!(point[1], sample.prey.population as f64);
        }
    }

    /// T-CHART-2 (display part): a species with no organisms gives a gap, not zeros.
    #[test]
    fn missing_means_become_gaps() {
        let mut config = ScenarioConfig::default();
        config.world.width = 96.0;
        config.world.height = 96.0;
        config.species.predator.start = StartPopulation::Count(0);
        let mut s = Simulation::new(&config, 3).expect("valid");
        s.step_n(90);
        assert!(segments(s.metrics().samples(), Species::Predator, |st| st.mean_speed).is_empty());
        let populations = segments(s.metrics().samples(), Species::Predator, |st| {
            Some(st.population as f64)
        });
        assert!(
            populations[0].iter().all(|p| p[1] == 0.0),
            "population itself is a real zero"
        );
    }
}
