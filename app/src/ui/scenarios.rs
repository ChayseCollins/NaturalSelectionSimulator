//! Scenario browser (M-16, UC2): edit the scenario's name, description and learning goal;
//! list the `.toml` files in the `scenarios/` folder; load one or a typed path; save the
//! current parameters as versioned, human-readable TOML; or share them as copy-and-paste
//! text (C-01).
//!
//! File I/O lives here in the app, never in `sim` (I-08). Loading an invalid file never
//! changes the current simulation, and the error names the field.

use std::fs;
use std::path::{Path, PathBuf};

use egui::RichText;
use sim::ScenarioConfig;

use crate::session::Session;

use super::controls::join_errors;
use super::{Notice, UiState};

/// Folder the browser lists and saves into, relative to the working directory.
pub const SCENARIO_DIR: &str = "scenarios";

pub fn default_save_path() -> String {
    format!("{SCENARIO_DIR}/my-scenario.scenario.toml")
}

/// Re-reads the scenario folder: `.toml` files, sorted by name.
pub fn refresh_files(state: &mut UiState) {
    state.scenario_files = list_scenarios(Path::new(SCENARIO_DIR));
}

pub fn list_scenarios(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// Reads and validates a scenario file without touching any simulation.
pub fn read_scenario(path: &Path) -> Result<ScenarioConfig, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    ScenarioConfig::from_toml_str(&text)
        .map_err(|e| format!("{} is not a valid scenario:\n{e}", path.display()))
}

/// Writes `config` as TOML to `path`, creating the parent folder. Validates first, so an
/// invalid configuration is never saved.
pub fn write_scenario(path: &Path, config: &ScenarioConfig) -> Result<(), String> {
    config.validate().map_err(|errors| join_errors(&errors))?;
    let text = config.to_toml_string()?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

fn load(session: &mut Session, state: &mut UiState, path: &Path) {
    match read_scenario(path) {
        Err(message) => state.scenario_notice = Some(Notice::error(message)),
        Ok(config) => {
            let seed = config.seed.unwrap_or_else(super::controls::fresh_seed);
            let name = config.name.clone();
            match session.load(config, seed, path.display().to_string()) {
                Ok(()) => {
                    state.preset_index = None;
                    state.selected = None;
                    state.fit_requested = true;
                    state.scenario_notice =
                        Some(Notice::ok(format!("Loaded \"{name}\" with seed {seed}.")));
                }
                Err(errors) => state.scenario_notice = Some(Notice::error(join_errors(&errors))),
            }
        }
    }
}

/// The browser window.
pub fn window(ctx: &egui::Context, session: &mut Session, state: &mut UiState) {
    let mut open = state.show_scenarios;
    egui::Window::new("Scenarios")
        .open(&mut open)
        .default_width(440.0)
        .default_pos([360.0, 90.0])
        .show(ctx, |ui| {
            ui.label(RichText::new("Describe this scenario").strong());
            let editable = session.driver().is_paused();
            ui.add_enabled_ui(editable, |ui| {
                egui::Grid::new("scenario_text").num_columns(2).show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut session.draft.name);
                    ui.end_row();
                    ui.label("Description");
                    ui.add(egui::TextEdit::multiline(&mut session.draft.description).desired_rows(2));
                    ui.end_row();
                    ui.label("Learning goal");
                    ui.add(egui::TextEdit::multiline(&mut session.draft.learning_goal).desired_rows(2));
                    ui.end_row();
                });
                let mut remember = session.draft.seed.is_some();
                if ui
                    .checkbox(&mut remember, format!("Save the run seed ({})", session.draft_seed))
                    .on_hover_text("Others who load the file get exactly the same run")
                    .changed()
                {
                    session.draft.seed = remember.then_some(session.draft_seed);
                }
                if let Some(seed) = &mut session.draft.seed {
                    *seed = session.draft_seed;
                }
            });
            if !editable {
                ui.small("Pause to edit.");
            }
            ui.separator();

            ui.label(RichText::new(format!("Files in {SCENARIO_DIR}/")).strong());
            let mut clicked = None;
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                if state.scenario_files.is_empty() {
                    ui.weak("No scenario files yet. Save one below.");
                }
                for file in &state.scenario_files {
                    let label = file.file_name().map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into_owned());
                    if ui.selectable_label(state.scenario_path == file.display().to_string(), label).clicked() {
                        clicked = Some(file.clone());
                    }
                }
            });
            if let Some(file) = clicked {
                state.scenario_path = file.display().to_string();
            }
            if ui.small_button("Refresh list").clicked() {
                refresh_files(state);
            }
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Path");
                ui.add(egui::TextEdit::singleline(&mut state.scenario_path).desired_width(300.0));
            });
            ui.horizontal(|ui| {
                let path = PathBuf::from(state.scenario_path.trim());
                if ui.button("Load").on_hover_text("Open this file; the current run is replaced only if it is valid").clicked() {
                    load(session, state, &path);
                }
                let save_enabled = session.draft_errors().is_empty();
                if ui
                    .add_enabled(save_enabled, egui::Button::new("Save current parameters"))
                    .on_hover_text("Writes the parameters shown in the left panel, including unapplied changes")
                    .clicked()
                {
                    state.scenario_notice = Some(match write_scenario(&path, &session.draft) {
                        Ok(()) => {
                            refresh_files(state);
                            Notice::ok(format!("Saved {}. Share this file to share the scenario.", path.display()))
                        }
                        Err(message) => Notice::error(message),
                    });
                }
            });
            egui::CollapsingHeader::new("Share as text (copy and paste)")
                .default_open(false)
                .show(ui, |ui| {
                    if ui
                        .button("Copy current parameters")
                        .on_hover_text("Copies the scenario as TOML text, for email or chat")
                        .clicked()
                    {
                        state.scenario_notice = Some(match session.draft.to_toml_string() {
                            Ok(text) => {
                                ui.ctx().copy_text(text);
                                Notice::ok("Copied the scenario text to the clipboard.")
                            }
                            Err(message) => Notice::error(message),
                        });
                    }
                    ui.add(
                        egui::TextEdit::multiline(&mut state.paste_text)
                            .desired_rows(4)
                            .hint_text("Paste scenario text here"),
                    );
                    if ui.button("Load pasted text").clicked() {
                        match ScenarioConfig::from_toml_str(&state.paste_text) {
                            Err(error) => {
                                state.scenario_notice =
                                    Some(Notice::error(format!("Not a valid scenario:\n{error}")));
                            }
                            Ok(config) => {
                                let seed = config.seed.unwrap_or_else(super::controls::fresh_seed);
                                let name = config.name.clone();
                                state.scenario_notice = Some(match session.load(config, seed, "pasted text") {
                                    Ok(()) => {
                                        state.preset_index = None;
                                        state.selected = None;
                                        state.fit_requested = true;
                                        Notice::ok(format!("Loaded \"{name}\" with seed {seed}."))
                                    }
                                    Err(errors) => Notice::error(join_errors(&errors)),
                                });
                            }
                        }
                    }
                });
            if let Some(notice) = &state.scenario_notice {
                notice.show(ui);
            }
        });
    state.show_scenarios = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nss-app-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// T-SHARE-1 (automated part) and M-16: a saved file lists, loads with identical
    /// parameters, and runs to the same hash.
    #[test]
    fn saved_scenarios_list_and_reload_identically() {
        let dir = temp_dir("roundtrip");
        let mut config = presets::baseline().config;
        config.name = "Shared".to_string();
        config.mutation.magnitude = 0.0731;
        let path = dir.join("shared.scenario.toml");
        write_scenario(&path, &config).expect("saved");
        assert_eq!(list_scenarios(&dir), std::slice::from_ref(&path));
        let back = read_scenario(&path).expect("loads");
        assert_eq!(back, config);
        let text = fs::read_to_string(&path).expect("text");
        assert!(text.contains("format_version = 1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_files_are_reported_naming_the_field() {
        let dir = temp_dir("invalid");
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("bad.toml");
        fs::write(&path, "format_version = 1\n[mutation]\nprobability = 3.0\n").expect("write");
        let message = read_scenario(&path).expect_err("invalid");
        assert!(message.contains("mutation.probability"), "{message}");
        assert!(read_scenario(&dir.join("missing.toml")).is_err());
        let mut config = presets::baseline().config;
        config.world.width = -5.0;
        assert!(write_scenario(&dir.join("never.toml"), &config).is_err());
        assert!(
            !dir.join("never.toml").exists(),
            "invalid scenarios are never saved"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
