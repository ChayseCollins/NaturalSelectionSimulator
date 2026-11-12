//! Scenario presets embedded at build time (M-15, T-SCN-4).
//!
//! `build.rs` embeds every file in the repository's `presets/` folder. The baseline
//! classroom preset (`baseline.scenario.toml`) is listed first and is what the app opens.

use sim::ScenarioConfig;

include!(concat!(env!("OUT_DIR"), "/presets.rs"));

/// File name of the baseline classroom preset.
pub const BASELINE_FILE: &str = "baseline.scenario.toml";

/// One parsed preset.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub file: &'static str,
    pub config: ScenarioConfig,
}

/// Every embedded preset that parses and validates, baseline first, then by file name.
/// A preset that fails to parse is reported in the returned error list instead (T-SCN-4
/// checks that there are none).
pub fn all() -> (Vec<Preset>, Vec<String>) {
    let mut presets = Vec::new();
    let mut errors = Vec::new();
    for &(file, text) in PRESET_FILES {
        match ScenarioConfig::from_toml_str(text) {
            Ok(config) => presets.push(Preset { file, config }),
            Err(error) => errors.push(format!("{file}: {error}")),
        }
    }
    presets.sort_by_key(|preset| (preset.file != BASELINE_FILE, preset.file));
    (presets, errors)
}

/// The baseline classroom preset.
///
/// # Panics
/// If the embedded baseline is missing or invalid, which T-SCN-4 rules out.
pub fn baseline() -> Preset {
    all()
        .0
        .into_iter()
        .find(|preset| preset.file == BASELINE_FILE)
        .expect("the embedded baseline preset is valid (T-SCN-4)")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-SCN-4: every embedded preset parses and validates; the baseline comes first.
    #[test]
    fn t_scn_4_every_embedded_preset_is_valid() {
        let (presets, errors) = all();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(presets.len(), PRESET_FILES.len());
        assert_eq!(presets[0].file, BASELINE_FILE);
        for preset in &presets {
            assert!(!preset.config.name.is_empty());
            assert!(!preset.config.learning_goal.is_empty());
        }
    }

    /// The app and `headless` default to the same scenario (X2).
    #[test]
    fn baseline_matches_the_headless_default() {
        assert_eq!(
            baseline().config,
            headless::run::load_scenario(None).expect("valid")
        );
    }
}
