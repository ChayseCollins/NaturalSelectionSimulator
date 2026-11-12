//! Embeds every `presets/*.toml` file at build time (M-15): adding a preset file and
//! rebuilding makes it appear in the picker, with no change to `sim` or the app's code.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("set by Cargo");
    let dir = Path::new(&manifest).join("..").join("presets");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut code = String::from(
        "/// Embedded preset files as `(file name, contents)`, sorted by file name.\n\
         pub const PRESET_FILES: &[(&str, &str)] = &[\n",
    );
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().expect("file").to_string_lossy();
        let absolute = fs::canonicalize(path).expect("readable preset");
        code.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            absolute.display().to_string()
        ));
    }
    code.push_str("];\n");
    let out = Path::new(&env::var("OUT_DIR").expect("set by Cargo")).join("presets.rs");
    fs::write(out, code).expect("write generated presets");
}
