//! Food stored per grid cell (M-07, SCOPE 6.1, 6.4).
//!
//! One contiguous `f32` per terrain cell, indexed `y * width + x`. Every cell's cap is
//! `fertility × max_per_cell × abundance` (T-TF-2), where fertility comes from the cell's
//! terrain class; impassable and zero-fertility cells have cap 0 and never hold food.
//! Food starts at `initial_fraction × cap` (no randomness), regrows linearly by
//! `regrowth_per_tick` once per tick without exceeding the cap, and never goes negative.
//!
//! Famine (S-01) multiplies the regrowth cap: from the famine tick on, cells regrow only
//! up to `multiplier × cap`. Food already above that level is not destroyed; it is eaten
//! down and then stays at the lower level.

use crate::config::FoodConfig;
use crate::terrain::{GridGeometry, Terrain};

#[derive(Debug, Clone, PartialEq)]
pub struct FoodGrid {
    geometry: GridGeometry,
    /// Cap of every cell (derived from terrain and configuration; not separate state).
    caps: Vec<f32>,
    pub(crate) food: Vec<f32>,
}

impl FoodGrid {
    /// Builds the grid for a terrain and validated food settings.
    pub(crate) fn new(terrain: &Terrain, food: &FoodConfig) -> Self {
        let caps: Vec<f32> = terrain
            .classes()
            .iter()
            .map(|class| class.fertility() * food.max_per_cell * food.abundance)
            .collect();
        let amounts = caps.iter().map(|cap| food.initial_fraction * cap).collect();
        Self {
            geometry: *terrain.geometry(),
            caps,
            food: amounts,
        }
    }

    pub fn geometry(&self) -> &GridGeometry {
        &self.geometry
    }

    /// Grid width in cells.
    pub fn width(&self) -> u32 {
        self.geometry.width()
    }

    /// Grid height in cells.
    pub fn height(&self) -> u32 {
        self.geometry.height()
    }

    pub fn cell_count(&self) -> usize {
        self.food.len()
    }

    pub fn cell_size(&self) -> f32 {
        self.geometry.cell_size()
    }

    /// Food cap of the cell at `index`.
    pub fn cap(&self, index: usize) -> f32 {
        self.caps[index]
    }

    /// Caps of every cell.
    pub fn caps(&self) -> &[f32] {
        &self.caps
    }

    /// Current food in every cell, indexed `y * width + x`.
    pub fn amounts(&self) -> &[f32] {
        &self.food
    }

    pub fn index(&self, cx: u32, cy: u32) -> usize {
        self.geometry.index(cx, cy)
    }

    pub fn coords(&self, index: usize) -> (u32, u32) {
        self.geometry.coords(index)
    }

    pub fn cell_coords_at(&self, x: f32, y: f32) -> (u32, u32) {
        self.geometry.cell_coords_at(x, y)
    }

    pub fn cell_at(&self, x: f32, y: f32) -> usize {
        self.geometry.cell_at(x, y)
    }

    pub fn cell_center(&self, index: usize) -> (f32, f32) {
        self.geometry.cell_center(index)
    }

    /// Sets one cell's food, clamped to `[0, cap]`. For constructed experiments and tests.
    pub(crate) fn set(&mut self, index: usize, amount: f32) {
        self.food[index] = amount.clamp(0.0, self.caps[index]);
    }

    /// Phase 10: every cell below `cap × cap_multiplier` gains `regrowth_per_tick`,
    /// stopping at that level. Cells at or above it are left unchanged.
    pub(crate) fn regrow(&mut self, regrowth_per_tick: f32, cap_multiplier: f32) {
        for (amount, &cap) in self.food.iter_mut().zip(&self.caps) {
            let limit = cap * cap_multiplier;
            if *amount < limit {
                *amount = (*amount + regrowth_per_tick).min(limit);
            }
        }
    }
}

/// Food one prey eats this tick: the least of the food in its cell, its intake limit, and
/// the food whose energy exactly fills its remaining capacity (`energy_room /
/// energy_per_unit`). Never negative.
pub(crate) fn amount_eaten(
    food_in_cell: f32,
    intake_per_tick: f32,
    energy_room: f32,
    energy_per_unit: f32,
) -> f32 {
    if energy_room <= 0.0 || food_in_cell <= 0.0 {
        return 0.0;
    }
    food_in_cell
        .min(intake_per_tick)
        .min(energy_room / energy_per_unit)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TerrainClass;

    fn grid(map: &str, food: &FoodConfig) -> FoodGrid {
        FoodGrid::new(&Terrain::from_ascii(map, 2.0).expect("valid map"), food)
    }

    /// T-TF-2: cell cap = fertility × max food × abundance; impassable and zero-fertility
    /// cells hold no food.
    #[test]
    fn t_tf_2_caps_follow_fertility_and_impassable_cells_stay_empty() {
        let config = FoodConfig {
            max_per_cell: 2.0,
            abundance: 1.5,
            regrowth_per_tick: 10.0,
            ..FoodConfig::default()
        };
        let mut food = grid(".f^:~#", &config);
        for (index, class) in [
            TerrainClass::Grass,
            TerrainClass::Forest,
            TerrainClass::Hills,
            TerrainClass::Sand,
            TerrainClass::Water,
            TerrainClass::Rock,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                food.cap(index),
                class.fertility() * 2.0 * 1.5,
                "{}",
                class.name()
            );
            assert_eq!(food.amounts()[index], food.cap(index), "starts full");
        }
        food.regrow(10.0, 1.0);
        assert_eq!(food.amounts()[4], 0.0, "water never holds food");
        assert_eq!(food.amounts()[5], 0.0, "rock never holds food");
        assert_eq!(food.amounts()[0], 3.0, "capped");
    }

    /// T-TF-3: an empty cell regrows to its cap in the expected number of ticks and never
    /// exceeds it.
    #[test]
    fn t_tf_3_regrowth_reaches_the_cap_in_the_expected_ticks() {
        let config = FoodConfig {
            initial_fraction: 0.0,
            ..FoodConfig::default()
        };
        let mut food = grid("..", &config);
        let mut ticks = 0;
        while food.amounts()[0] < food.cap(0) {
            food.regrow(0.25, 1.0);
            ticks += 1;
        }
        assert_eq!(ticks, 4);
        food.regrow(0.25, 1.0);
        assert_eq!(food.amounts()[0], 1.0);
    }

    #[test]
    fn famine_lowers_the_regrowth_cap_without_destroying_food() {
        let config = FoodConfig {
            initial_fraction: 0.0,
            ..FoodConfig::default()
        };
        let mut food = grid("..", &config);
        food.food[1] = 0.9;
        for _ in 0..100 {
            food.regrow(0.1, 0.4);
        }
        assert_eq!(food.amounts()[0], 0.4, "regrows only to 40% of the cap");
        assert_eq!(food.amounts()[1], 0.9, "existing food is kept");
    }

    #[test]
    fn amount_eaten_respects_food_intake_and_room() {
        assert_eq!(amount_eaten(1.0, 0.25, 10.0, 2.0), 0.25);
        assert_eq!(amount_eaten(0.1, 0.25, 10.0, 2.0), 0.1);
        assert_eq!(amount_eaten(1.0, 0.25, 0.25, 2.0), 0.125);
        assert_eq!(amount_eaten(1.0, 0.25, 0.0, 2.0), 0.0);
        assert_eq!(amount_eaten(0.0, 0.25, 1.0, 2.0), 0.0);
    }
}
