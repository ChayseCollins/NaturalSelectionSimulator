//! Desktop application for the Natural Selection Simulator.

use bevy::app::App;

fn main() {
    // TEMPORARY (M-01): proves the pinned Bevy links and runs one headless update
    // without opening a window. Replaced by the minimal world view (M-10, M-11).
    let mut app = App::new();
    app.update();
    println!("app: workspace placeholder (M-01). Bevy linked and ran one update; no window.");
}
