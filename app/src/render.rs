//! Pure helpers for drawing the simulation (M-10): terrain and food colors, the world
//! texture fill, organism appearance, and the procedural sprite shapes.
//!
//! Everything here only reads `sim` state. Visuals are a projection of the simulation and
//! never write back into it.

use sim::{FoodGrid, Species, Terrain, TerrainClass, TraitId};

/// Color of a terrain class with no food and with a full cell (food overlay), as RGB.
/// Impassable classes have one color.
pub fn terrain_colors(class: TerrainClass) -> ([u8; 3], [u8; 3]) {
    match class {
        TerrainClass::Water => ([38, 88, 148], [38, 88, 148]),
        TerrainClass::Sand => ([200, 184, 140], [180, 188, 124]),
        TerrainClass::Grass => ([156, 144, 92], [72, 150, 62]),
        TerrainClass::Forest => ([96, 92, 62], [34, 100, 50]),
        TerrainClass::Hills => ([146, 128, 102], [118, 134, 88]),
        TerrainClass::Rock => ([112, 108, 106], [112, 108, 106]),
    }
}

/// RGBA of one cell: the class's empty color blended toward its full color by the cell's
/// food fraction. Without the food overlay the class's middle color is used. Out-of-range
/// or non-finite food is clamped.
pub fn cell_color(class: TerrainClass, amount: f32, cap: f32, show_food: bool) -> [u8; 4] {
    let (empty, full) = terrain_colors(class);
    let fraction = if !show_food {
        0.6
    } else if cap > 0.0 && amount.is_finite() && cap.is_finite() {
        (amount / cap).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * fraction).round() as u8;
    [
        mix(empty[0], full[0]),
        mix(empty[1], full[1]),
        mix(empty[2], full[2]),
        255,
    ]
}

/// Bytes needed for an RGBA8 texture with one pixel per grid cell.
pub fn world_texture_len(food: &FoodGrid) -> usize {
    food.cell_count() * 4
}

/// Writes terrain and food into an RGBA8 pixel buffer, one pixel per cell. Image rows run
/// top to bottom while world y runs up, so pixel row `r` shows grid row `height - 1 - r`.
/// Does nothing if a size does not match.
pub fn write_world_pixels(terrain: &Terrain, food: &FoodGrid, show_food: bool, pixels: &mut [u8]) {
    let (width, height) = (food.width() as usize, food.height() as usize);
    let classes = terrain.classes();
    let amounts = food.amounts();
    if classes.len() != width * height || pixels.len() != amounts.len() * 4 {
        return;
    }
    for row in 0..height {
        let grid_row = height - 1 - row;
        for column in 0..width {
            let cell = grid_row * width + column;
            let pixel = (row * width + column) * 4;
            pixels[pixel..pixel + 4].copy_from_slice(&cell_color(
                classes[cell],
                amounts[cell],
                food.cap(cell),
                show_food,
            ));
        }
    }
}

/// How organisms are colored (C-05: color by a trait value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorBy {
    #[default]
    Species,
    Trait(TraitId),
}

impl ColorBy {
    pub const ALL: [ColorBy; 4] = [
        ColorBy::Species,
        ColorBy::Trait(TraitId::Speed),
        ColorBy::Trait(TraitId::Size),
        ColorBy::Trait(TraitId::Sense),
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColorBy::Species => "Species",
            ColorBy::Trait(TraitId::Speed) => "Speed",
            ColorBy::Trait(TraitId::Size) => "Size",
            ColorBy::Trait(TraitId::Sense) => "Sense range",
        }
    }
}

/// Species colors (sRGB 0–1): warm yellow prey, red predators.
pub const PREY_RGB: [f32; 3] = [1.0, 0.82, 0.22];
pub const PREDATOR_RGB: [f32; 3] = [0.92, 0.16, 0.14];

pub fn species_rgb(species: Species) -> [f32; 3] {
    match species {
        Species::Prey => PREY_RGB,
        Species::Predator => PREDATOR_RGB,
    }
}

/// Display range used when coloring by a trait: values at or below `low` get the first
/// ramp color, at or above `high` the last (P).
pub fn trait_display_range(id: TraitId) -> (f32, f32) {
    match id {
        TraitId::Speed => (0.5, 2.5),
        TraitId::Size => (0.5, 2.5),
        TraitId::Sense => (5.0, 25.0),
    }
}

/// A perceptually ordered ramp (dark purple, blue, teal, green, yellow) at `t` in [0, 1].
pub fn ramp(t: f32) -> [f32; 3] {
    const STOPS: [[f32; 3]; 5] = [
        [0.27, 0.00, 0.33],
        [0.23, 0.32, 0.55],
        [0.13, 0.57, 0.55],
        [0.37, 0.79, 0.38],
        [0.99, 0.91, 0.14],
    ];
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let scaled = t * (STOPS.len() - 1) as f32;
    let i = (scaled as usize).min(STOPS.len() - 2);
    let f = scaled - i as f32;
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

/// Color of one organism under a coloring mode.
pub fn organism_rgb(color_by: ColorBy, species: Species, trait_value: f32) -> [f32; 3] {
    match color_by {
        ColorBy::Species => species_rgb(species),
        ColorBy::Trait(id) => {
            let (low, high) = trait_display_range(id);
            ramp((trait_value - low) / (high - low))
        }
    }
}

/// Drawn diameter of an organism in world units: grows with its size trait.
pub fn marker_diameter(size: f32) -> f32 {
    1.2 + 1.2 * size
}

/// Side of the procedural sprite textures, in pixels.
pub const SHAPE_PIXELS: u32 = 64;

/// The three sprite shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Prey: a disc with a darker rim (tinted by the sprite color).
    Disc,
    /// Predators: a triangle pointing along +x (rotated to the heading).
    Triangle,
    /// The selection highlight: a white ring.
    Ring,
}

/// RGBA8 pixels of a white shape on transparency, `SHAPE_PIXELS` square, anti-aliased by
/// 4 × 4 supersampling. Rows run top to bottom.
pub fn shape_pixels(shape: Shape) -> Vec<u8> {
    let n = SHAPE_PIXELS as usize;
    let mut pixels = vec![0u8; n * n * 4];
    for row in 0..n {
        for column in 0..n {
            let (mut coverage, mut shade) = (0.0_f32, 0.0_f32);
            for sy in 0..4 {
                for sx in 0..4 {
                    // Coordinates in [-1, 1], y up.
                    let x = ((column as f32 + (sx as f32 + 0.5) / 4.0) / n as f32) * 2.0 - 1.0;
                    let y = 1.0 - ((row as f32 + (sy as f32 + 0.5) / 4.0) / n as f32) * 2.0;
                    if let Some(value) = shape_sample(shape, x, y) {
                        coverage += 1.0 / 16.0;
                        shade += value / 16.0;
                    }
                }
            }
            if coverage > 0.0 {
                let brightness = (shade / coverage * 255.0).round() as u8;
                let pixel = (row * n + column) * 4;
                pixels[pixel..pixel + 4].copy_from_slice(&[
                    brightness,
                    brightness,
                    brightness,
                    (coverage * 255.0).round() as u8,
                ]);
            }
        }
    }
    pixels
}

/// Brightness of a shape at `(x, y)` in [-1, 1]², or `None` outside it.
fn shape_sample(shape: Shape, x: f32, y: f32) -> Option<f32> {
    let r = (x * x + y * y).sqrt();
    match shape {
        Shape::Disc => (r <= 0.95).then_some(if r > 0.72 { 0.35 } else { 1.0 }),
        Shape::Ring => (0.78..=0.98).contains(&r).then_some(1.0),
        Shape::Triangle => {
            // Tip at (0.95, 0), base corners at (-0.75, ±0.75).
            let inside = x >= -0.75 && y.abs() <= (0.95 - x) * (0.75 / 1.7);
            inside.then(|| {
                let edge = ((0.95 - x) * (0.75 / 1.7) - y.abs()).min(x + 0.75);
                if edge < 0.16 { 0.35 } else { 1.0 }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use sim::{ScenarioConfig, Simulation, StartPopulation};

    use super::*;

    #[test]
    fn cell_color_spans_empty_to_full() {
        let (empty, full) = terrain_colors(TerrainClass::Grass);
        assert_eq!(cell_color(TerrainClass::Grass, 0.0, 1.0, true)[..3], empty);
        assert_eq!(cell_color(TerrainClass::Grass, 1.0, 1.0, true)[..3], full);
        assert_eq!(
            cell_color(TerrainClass::Grass, 2.0, 1.0, true),
            cell_color(TerrainClass::Grass, 1.0, 1.0, true)
        );
        assert_eq!(
            cell_color(TerrainClass::Grass, f32::NAN, 1.0, true),
            cell_color(TerrainClass::Grass, 0.0, 1.0, true)
        );
        assert_eq!(
            cell_color(TerrainClass::Water, 0.0, 0.0, true),
            cell_color(TerrainClass::Water, 0.0, 0.0, false)
        );
    }

    /// M-10: terrain classes are distinguishable, and M-07: the food overlay differs
    /// between high- and low-fertility terrain.
    #[test]
    fn every_class_has_a_distinct_color() {
        let colors: Vec<[u8; 4]> = TerrainClass::ALL
            .iter()
            .map(|&class| cell_color(class, 0.5, 1.0, true))
            .collect();
        for (i, a) in colors.iter().enumerate() {
            for b in &colors[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let fertile = cell_color(TerrainClass::Grass, 1.0, 1.0, true);
        let bare = cell_color(TerrainClass::Grass, 0.1, 1.0, true);
        let distance: i32 = (0..3)
            .map(|c| (i32::from(fertile[c]) - i32::from(bare[c])).abs())
            .sum();
        assert!(
            distance > 100,
            "grazed and full grass are clearly different: {distance}"
        );
        assert!(
            fertile[1] > fertile[0] && bare[0] > bare[1],
            "full is green, grazed is tan"
        );
    }

    #[test]
    fn world_pixels_flip_rows_so_world_y_points_up() {
        let terrain = Terrain::from_ascii("~.\n..\n", 2.0).expect("valid map");
        let mut config = ScenarioConfig::default();
        config.species.prey.start = StartPopulation::Count(0);
        config.species.predator.start = StartPopulation::Count(0);
        let mut ascii = String::new();
        for _ in 0..8 {
            ascii.push_str("~.......\n");
        }
        let terrain_big = Terrain::from_ascii(&ascii, 2.0).expect("valid map");
        let simulation = Simulation::with_terrain(&config, 1, terrain_big).expect("valid");
        let mut pixels = vec![0; world_texture_len(simulation.food())];
        write_world_pixels(simulation.terrain(), simulation.food(), true, &mut pixels);
        let water = cell_color(TerrainClass::Water, 0.0, 0.0, true);
        assert_eq!(&pixels[..4], &water, "top-left is water");
        let mut wrong = vec![0; 3];
        write_world_pixels(&terrain, simulation.food(), true, &mut wrong);
        assert_eq!(wrong, [0, 0, 0], "a wrong-sized buffer is left alone");
    }

    #[test]
    fn organism_colors_and_sizes() {
        assert_eq!(organism_rgb(ColorBy::Species, Species::Prey, 0.0), PREY_RGB);
        assert_eq!(
            organism_rgb(ColorBy::Species, Species::Predator, 0.0),
            PREDATOR_RGB
        );
        let slow = organism_rgb(ColorBy::Trait(TraitId::Speed), Species::Prey, 0.5);
        let fast = organism_rgb(ColorBy::Trait(TraitId::Speed), Species::Prey, 2.5);
        assert_eq!(slow, ramp(0.0));
        assert_eq!(fast, ramp(1.0));
        assert_ne!(slow, fast);
        assert!(marker_diameter(2.0) > marker_diameter(1.0));
        for c in ramp(f32::NAN) {
            assert!((0.0..=1.0).contains(&c));
        }
    }

    #[test]
    fn shapes_are_opaque_inside_and_clear_outside() {
        let n = SHAPE_PIXELS as usize;
        for shape in [Shape::Disc, Shape::Triangle, Shape::Ring] {
            let pixels = shape_pixels(shape);
            assert_eq!(pixels.len(), n * n * 4);
            assert_eq!(pixels[3], 0, "{shape:?}: the corner is transparent");
        }
        let disc = shape_pixels(Shape::Disc);
        let center = ((n / 2) * n + n / 2) * 4;
        assert_eq!(&disc[center..center + 4], &[255, 255, 255, 255]);
        let ring = shape_pixels(Shape::Ring);
        assert_eq!(ring[center + 3], 0, "the ring is hollow");
        let triangle = shape_pixels(Shape::Triangle);
        let tip_side = ((n / 2) * n + n - 6) * 4;
        let back_side = ((n / 2) * n + 10) * 4;
        assert!(
            triangle[tip_side + 3] > 0 && triangle[back_side + 3] > 0,
            "spans the width"
        );
    }
}
