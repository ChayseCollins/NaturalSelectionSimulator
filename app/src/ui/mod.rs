//! egui panels (M-11 to M-16, M-18; SCOPE 7.3 layout): control bar on top, parameters on
//! the left, inspector on the right, charts and the status bar at the bottom, and the
//! scenario browser, About panel and performance overlay in windows.
//!
//! These functions only read the simulation and call the session's public operations
//! (play, pause, step, reset, load); they never change simulation state directly.

pub mod charts;
pub mod controls;
pub mod inspector;
pub mod params;
pub mod scenarios;

use std::path::PathBuf;

use egui::Color32;
use sim::Species;

use crate::perf::PerfSummary;
use crate::presets::Preset;
use crate::render::{self, ColorBy};

/// Which trait chart is shown beside the population chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraitChart {
    #[default]
    Speed,
    Size,
    Sense,
    /// Mean and maximum generation (S-03).
    Generation,
}

impl TraitChart {
    pub const ALL: [TraitChart; 4] = [
        TraitChart::Speed,
        TraitChart::Size,
        TraitChart::Sense,
        TraitChart::Generation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TraitChart::Speed => "Mean speed",
            TraitChart::Size => "Mean size",
            TraitChart::Sense => "Mean sense range",
            TraitChart::Generation => "Generation",
        }
    }
}

/// A message shown to the user, with its kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub error: bool,
    pub text: String,
}

impl Notice {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            error: false,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            error: true,
            text: text.into(),
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let color = if self.error { ERROR } else { OK };
        ui.colored_label(color, &self.text);
    }
}

pub const ERROR: Color32 = Color32::from_rgb(255, 110, 100);
pub const OK: Color32 = Color32::from_rgb(120, 210, 120);
pub const WARNING: Color32 = Color32::from_rgb(255, 200, 80);

/// View and panel state that is not simulation state.
#[derive(Debug, Clone)]
pub struct UiState {
    pub presets: Vec<Preset>,
    /// Index into `presets` of the scenario shown in the picker, or `None` for a file.
    pub preset_index: Option<usize>,
    /// Selected organism (M-12).
    pub selected: Option<u64>,
    /// Keep the camera on the selected organism (S-08).
    pub follow_selected: bool,
    pub color_by: ColorBy,
    pub show_food: bool,
    pub show_overlay: bool,
    pub show_scenarios: bool,
    pub show_about: bool,
    pub trait_chart: TraitChart,
    /// Typed path in the scenario browser (M-16).
    pub scenario_path: String,
    pub scenario_files: Vec<PathBuf>,
    pub scenario_notice: Option<Notice>,
    /// Scenario text pasted into the browser (C-01).
    pub paste_text: String,
    pub params_notice: Option<Notice>,
    /// Set by the Fit button; the camera system consumes it.
    pub fit_requested: bool,
    /// Latest performance summary, for the overlay.
    pub perf: PerfSummary,
    /// Stall-capped frames so far (T-FT-2), for the overlay.
    pub stall_count: u64,
}

impl UiState {
    pub fn new(presets: Vec<Preset>) -> Self {
        Self {
            presets,
            preset_index: Some(0),
            selected: None,
            follow_selected: false,
            color_by: ColorBy::Species,
            show_food: true,
            show_overlay: false,
            show_scenarios: false,
            show_about: false,
            trait_chart: TraitChart::Speed,
            scenario_path: scenarios::default_save_path(),
            scenario_files: Vec::new(),
            scenario_notice: None,
            paste_text: String::new(),
            params_notice: None,
            fit_requested: false,
            perf: PerfSummary::default(),
            stall_count: 0,
        }
    }
}

/// egui color of a species.
pub fn species_color(species: Species) -> Color32 {
    rgb(render::species_rgb(species))
}

pub fn rgb(color: [f32; 3]) -> Color32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(c(color[0]), c(color[1]), c(color[2]))
}

/// "12 m 03.5 s" style simulated time.
pub fn format_sim_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "–".to_string();
    }
    let minutes = (seconds / 60.0).floor();
    let rest = seconds - minutes * 60.0;
    if minutes >= 1.0 {
        format!("{minutes:.0} m {rest:04.1} s")
    } else {
        format!("{rest:.1} s")
    }
}

/// Integer with thin thousands separators, for readouts.
pub fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A slider bound to a float parameter's `limits` range (I-07: ranges come from `limits`),
/// so the UI cannot enter an out-of-range value (T-VAL-3).
pub fn float_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    limits: sim::limits::FloatLimits,
    hover: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::Slider::new(value, limits.min..=limits.max)
                .clamping(egui::SliderClamping::Always)
                .smart_aim(true)
                .max_decimals(display_decimals(limits))
                .text(label),
        );
        response.on_hover_text(hover)
    })
    .inner
}

/// Decimals shown for a range: fewer for wide ranges.
fn display_decimals(limits: sim::limits::FloatLimits) -> usize {
    let span = limits.max - limits.min;
    if span >= 100.0 {
        1
    } else if span >= 10.0 {
        2
    } else if span >= 1.0 {
        3
    } else {
        4
    }
}

/// A drag value bound to an integer parameter's `limits` range.
pub fn int_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    limits: sim::limits::IntLimits,
    hover: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        let max = limits.max.min(u64::from(u32::MAX)) as u32;
        let response = ui.add(
            egui::DragValue::new(value)
                .range(limits.min as u32..=max)
                .speed(1.0),
        );
        ui.label(label);
        response.on_hover_text(hover)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_time_and_numbers_are_readable() {
        assert_eq!(format_sim_time(0.0), "0.0 s");
        assert_eq!(format_sim_time(59.94), "59.9 s");
        assert_eq!(format_sim_time(723.5), "12 m 03.5 s");
        assert_eq!(format_sim_time(f64::NAN), "–");
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
