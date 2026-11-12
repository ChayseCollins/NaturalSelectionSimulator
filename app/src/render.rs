//! Pure helpers for drawing the simulation (M-10): the food color scale, the food texture
//! fill, and the map from stable organism IDs to visual handles.
//!
//! Everything here only reads `sim` state. Visuals are a projection of the simulation and
//! never write back into it.

use std::collections::BTreeMap;

use sim::FoodGrid;

/// Color of an empty cell (dark soil).
pub const FOOD_EMPTY_RGB: [u8; 3] = [28, 24, 20];
/// Color of a cell at its cap (green).
pub const FOOD_FULL_RGB: [u8; 3] = [46, 150, 64];

/// RGBA color for a cell holding `amount` of `cap` food: a linear blend from
/// [`FOOD_EMPTY_RGB`] to [`FOOD_FULL_RGB`]. Out-of-range or non-finite input is clamped,
/// so the result is always valid; a zero cap shows as empty.
pub fn food_color(amount: f32, cap: f32) -> [u8; 4] {
    let fraction = if cap > 0.0 && amount.is_finite() && cap.is_finite() {
        (amount / cap).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |empty: u8, full: u8| {
        let value = f32::from(empty) + (f32::from(full) - f32::from(empty)) * fraction;
        value.round() as u8
    };
    [
        mix(FOOD_EMPTY_RGB[0], FOOD_FULL_RGB[0]),
        mix(FOOD_EMPTY_RGB[1], FOOD_FULL_RGB[1]),
        mix(FOOD_EMPTY_RGB[2], FOOD_FULL_RGB[2]),
        255,
    ]
}

/// Bytes needed for an RGBA8 texture of the food grid.
pub fn food_texture_len(food: &FoodGrid) -> usize {
    food.cell_count() * 4
}

/// Writes the food grid into an RGBA8 pixel buffer, one pixel per cell. Image rows run
/// top to bottom while world y runs up, so pixel row `r` shows grid row
/// `height - 1 - r`. Does nothing if `pixels` has the wrong length.
pub fn write_food_pixels(food: &FoodGrid, pixels: &mut [u8]) {
    write_cells(
        food.width() as usize,
        food.height() as usize,
        food.amounts(),
        |cell| food.cap(cell),
        pixels,
    );
}

fn write_cells(
    width: usize,
    height: usize,
    amounts: &[f32],
    cap: impl Fn(usize) -> f32,
    pixels: &mut [u8],
) {
    if amounts.len() != width * height || pixels.len() != amounts.len() * 4 {
        return;
    }
    for row in 0..height {
        let grid_row = height - 1 - row;
        for column in 0..width {
            let cell = grid_row * width + column;
            let pixel = (row * width + column) * 4;
            pixels[pixel..pixel + 4].copy_from_slice(&food_color(amounts[cell], cap(cell)));
        }
    }
}

/// What to do to bring the visuals in line with the living organisms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan<E> {
    /// Visuals whose organism no longer exists; the caller removes them.
    pub removed: Vec<E>,
    /// Storage indices of organisms with no visual yet; the caller creates one and
    /// registers it with [`VisualMap::insert`].
    pub missing: Vec<usize>,
    /// `(storage index, visual)` for organisms that already have a visual; the caller
    /// updates its position from that index.
    pub existing: Vec<(usize, E)>,
}

/// Map from stable organism ID to a visual handle (a Bevy `Entity` in the app).
///
/// Keyed by ID, never by storage index, because swap-removal moves organisms between
/// indices. A `BTreeMap` keeps iteration order deterministic, though visuals never feed
/// back into the simulation anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualMap<E> {
    visuals: BTreeMap<u64, E>,
}

impl<E> Default for VisualMap<E> {
    fn default() -> Self {
        Self {
            visuals: BTreeMap::new(),
        }
    }
}

impl<E: Copy> VisualMap<E> {
    /// Compares the map with the living organisms' IDs (in storage order). Removes entries
    /// for IDs that are gone and returns them, with the indices that still need a visual
    /// and the visuals to update.
    pub fn plan(&mut self, ids: &[u64]) -> SyncPlan<E> {
        let mut alive = ids.to_vec();
        alive.sort_unstable();
        let gone: Vec<u64> = self
            .visuals
            .keys()
            .copied()
            .filter(|id| alive.binary_search(id).is_err())
            .collect();
        let removed = gone
            .iter()
            .filter_map(|id| self.visuals.remove(id))
            .collect();

        let mut missing = Vec::new();
        let mut existing = Vec::with_capacity(ids.len());
        for (index, id) in ids.iter().enumerate() {
            match self.visuals.get(id) {
                Some(&visual) => existing.push((index, visual)),
                None => missing.push(index),
            }
        }
        SyncPlan {
            removed,
            missing,
            existing,
        }
    }

    /// Registers the visual created for organism `id`.
    pub fn insert(&mut self, id: u64, visual: E) {
        self.visuals.insert(id, visual);
    }

    /// Removes every entry and returns the visuals, for a full rebuild.
    pub fn drain(&mut self) -> Vec<E> {
        std::mem::take(&mut self.visuals).into_values().collect()
    }

    pub fn get(&self, id: u64) -> Option<E> {
        self.visuals.get(&id).copied()
    }

    pub fn len(&self) -> usize {
        self.visuals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visuals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use sim::{ScenarioConfig, Simulation};

    use super::*;

    #[test]
    fn food_color_spans_empty_to_full() {
        assert_eq!(food_color(0.0, 1.0), [28, 24, 20, 255]);
        assert_eq!(food_color(1.0, 1.0), [46, 150, 64, 255]);
        let half = food_color(0.5, 1.0);
        for channel in 0..3 {
            let (low, high) = (FOOD_EMPTY_RGB[channel], FOOD_FULL_RGB[channel]);
            assert!((low.min(high)..=low.max(high)).contains(&half[channel]));
        }
        assert!(half[1] > food_color(0.25, 1.0)[1], "monotonic in green");
        assert_eq!(food_color(2.0, 1.0), food_color(1.0, 1.0), "clamped above");
        assert_eq!(food_color(f32::NAN, 1.0), food_color(0.0, 1.0));
        assert_eq!(
            food_color(0.5, 0.0),
            food_color(0.0, 1.0),
            "zero cap shows empty"
        );
    }

    #[test]
    fn food_pixels_flip_rows_so_world_y_points_up() {
        let mut config = ScenarioConfig::default();
        config.world.width = 16.0;
        config.world.height = 16.0;
        config.food.initial_fraction = 0.0;
        config.prey.start_count = 0;
        let simulation = Simulation::new(&config, 1).expect("valid");
        let food = simulation.food();
        assert_eq!((food.width(), food.height()), (8, 8));

        let mut pixels = vec![0; food_texture_len(food)];
        write_food_pixels(food, &mut pixels);
        assert!(pixels.chunks(4).all(|p| p == food_color(0.0, 1.0)));

        let mut wrong = vec![0; 3];
        write_food_pixels(food, &mut wrong);
        assert_eq!(wrong, [0, 0, 0], "wrong-sized buffer is left alone");
    }

    #[test]
    fn bottom_left_cell_is_drawn_in_the_last_image_row() {
        // A 3 × 2 grid where only cell (0, 0), the bottom-left of the world, is full.
        let amounts = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut pixels = vec![0; 6 * 4];
        write_cells(3, 2, &amounts, |_| 1.0, &mut pixels);
        let full = food_color(1.0, 1.0);
        let empty = food_color(0.0, 1.0);
        let pixel = |row: usize, column: usize| &pixels[(row * 3 + column) * 4..][..4];
        assert_eq!(pixel(1, 0), full, "bottom image row, first column");
        for (row, column) in [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2)] {
            assert_eq!(pixel(row, column), empty, "({row}, {column})");
        }
    }

    #[test]
    fn food_pixels_follow_simulation_food() {
        let mut config = ScenarioConfig::default();
        config.world.width = 32.0;
        config.world.height = 32.0;
        config.prey.start_count = 20;
        config.food.regrowth_per_tick = 0.0;
        let mut simulation = Simulation::new(&config, 1).expect("valid");
        let mut before = vec![0; food_texture_len(simulation.food())];
        write_food_pixels(simulation.food(), &mut before);
        simulation.step_n(60);
        let mut after = vec![0; food_texture_len(simulation.food())];
        write_food_pixels(simulation.food(), &mut after);
        assert_ne!(before, after, "eaten cells change color");
    }

    #[test]
    fn visual_map_follows_ids_through_swap_removal() {
        let mut map = VisualMap::default();
        let mut next_visual = 100_u32;
        let mut apply = |map: &mut VisualMap<u32>, ids: &[u64]| {
            let plan = map.plan(ids);
            for &index in &plan.missing {
                map.insert(ids[index], next_visual);
                next_visual += 1;
            }
            plan
        };

        let plan = apply(&mut map, &[0, 1, 2, 3, 4]);
        assert_eq!(plan.missing, [0, 1, 2, 3, 4]);
        assert!(plan.removed.is_empty() && plan.existing.is_empty());
        let visual_of = |map: &VisualMap<u32>, id: u64| map.get(id).expect("mapped");
        let id4 = visual_of(&map, 4);
        let id2 = visual_of(&map, 2);

        // IDs 1 and 3 starve; swap-removal leaves storage order [0, 4, 2].
        let plan = apply(&mut map, &[0, 4, 2]);
        assert_eq!(plan.removed.len(), 2);
        assert!(plan.missing.is_empty());
        assert_eq!(plan.existing, [(0, 100), (1, id4), (2, id2)]);
        assert_eq!(map.get(1), None);
        assert_eq!(map.get(3), None);
        assert_eq!(map.len(), 3);

        // A new ID gets a new visual; survivors keep theirs.
        let plan = apply(&mut map, &[0, 4, 2, 7]);
        assert_eq!(plan.missing, [3]);
        assert_eq!(visual_of(&map, 4), id4);

        assert_eq!(map.drain().len(), 4);
        assert!(map.is_empty());
    }
}
