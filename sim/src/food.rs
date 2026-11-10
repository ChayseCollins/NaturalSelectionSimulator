//! Food stored per grid cell (M-07, SCOPE 6.1, 6.4).
//!
//! One contiguous `f32` per cell, indexed `y * width + x`. The grid covers the world with
//! square cells of `cell_size` world units; when the world size is not a multiple of the
//! cell size, the last row and column are partial cells clipped to the world edge.
//!
//! Every cell's cap is `fertility × max_per_cell × abundance`. Terrain does not exist yet,
//! so every cell has fertility 1 and the same cap; terrain (M-06) replaces the uniform cap
//! with a per-cell cap and zero-fertility or impassable cells. Food starts uniform at
//! `initial_fraction × cap` (no randomness), regrows linearly by `regrowth_per_tick` once
//! per tick without exceeding the cap, and never goes negative.

use crate::config::{FoodConfig, WorldConfig};

/// Fertility of every cell until terrain classes exist (M-06).
const UNIFORM_FERTILITY: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct FoodGrid {
    width: u32,
    height: u32,
    cell_size: f32,
    world_width: f32,
    world_height: f32,
    /// Cap shared by every cell (derived from configuration; not separate state).
    cap: f32,
    pub(crate) food: Vec<f32>,
}

impl FoodGrid {
    /// Builds the grid for a validated configuration, with uniform starting food.
    pub(crate) fn new(world: &WorldConfig, food: &FoodConfig) -> Self {
        let width = (world.width / world.cell_size).ceil() as u32;
        let height = (world.height / world.cell_size).ceil() as u32;
        let cap = UNIFORM_FERTILITY * food.max_per_cell * food.abundance;
        let cells = width as usize * height as usize;
        Self {
            width,
            height,
            cell_size: world.cell_size,
            world_width: world.width,
            world_height: world.height,
            cap,
            food: vec![food.initial_fraction * cap; cells],
        }
    }

    /// Grid width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Grid height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn cell_count(&self) -> usize {
        self.food.len()
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Food cap of the cell at `index`. Uniform until terrain exists.
    pub fn cap(&self, _index: usize) -> f32 {
        self.cap
    }

    /// Current food in every cell, indexed `y * width + x`.
    pub fn amounts(&self) -> &[f32] {
        &self.food
    }

    /// Flat index of cell `(cx, cy)`. The caller keeps the coordinates inside the grid.
    pub fn index(&self, cx: u32, cy: u32) -> usize {
        debug_assert!(cx < self.width && cy < self.height);
        cy as usize * self.width as usize + cx as usize
    }

    /// Cell coordinates of a flat index.
    pub fn coords(&self, index: usize) -> (u32, u32) {
        let width = self.width as usize;
        ((index % width) as u32, (index / width) as u32)
    }

    /// Cell coordinates containing a world position. Positions on the far world edge
    /// (`x == width` or `y == height`) map to the last cell, never out of range.
    pub fn cell_coords_at(&self, x: f32, y: f32) -> (u32, u32) {
        let cx = ((x / self.cell_size) as u32).min(self.width - 1);
        let cy = ((y / self.cell_size) as u32).min(self.height - 1);
        (cx, cy)
    }

    /// Flat index of the cell containing a world position.
    pub fn cell_at(&self, x: f32, y: f32) -> usize {
        let (cx, cy) = self.cell_coords_at(x, y);
        self.index(cx, cy)
    }

    /// Center of the in-world part of a cell (clipped cells on the far edges have their
    /// center inside the world).
    pub fn cell_center(&self, index: usize) -> (f32, f32) {
        let (cx, cy) = self.coords(index);
        let center = |c: u32, world: f32| {
            let low = c as f32 * self.cell_size;
            let high = ((c + 1) as f32 * self.cell_size).min(world);
            (low + high) * 0.5
        };
        (center(cx, self.world_width), center(cy, self.world_height))
    }

    /// Phase 10: every cell gains `regrowth_per_tick`, stopping at its cap.
    pub(crate) fn regrow(&mut self, regrowth_per_tick: f32) {
        let cap = self.cap;
        for amount in &mut self.food {
            *amount = (*amount + regrowth_per_tick).min(cap);
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

    fn grid(width: f32, height: f32, cell_size: f32, food: FoodConfig) -> FoodGrid {
        FoodGrid::new(
            &WorldConfig {
                width,
                height,
                cell_size,
            },
            &food,
        )
    }

    fn default_grid(width: f32, height: f32, cell_size: f32) -> FoodGrid {
        grid(width, height, cell_size, FoodConfig::default())
    }

    #[test]
    fn dimensions_follow_world_and_cell_size() {
        let g = default_grid(512.0, 512.0, 2.0);
        assert_eq!(
            (g.width(), g.height()),
            (256, 256),
            "tier C grid (SCOPE 10.4)"
        );
        assert_eq!(g.cell_count(), 65_536);
        let g = default_grid(17.0, 16.0, 2.0);
        assert_eq!((g.width(), g.height()), (9, 8), "partial last column");
    }

    #[test]
    fn index_and_coords_round_trip() {
        let g = default_grid(20.0, 12.0, 2.0);
        assert_eq!((g.width(), g.height()), (10, 6));
        assert_eq!(g.index(0, 0), 0);
        assert_eq!(g.index(9, 0), 9);
        assert_eq!(g.index(0, 1), 10);
        assert_eq!(g.index(9, 5), 59);
        for index in 0..g.cell_count() {
            let (cx, cy) = g.coords(index);
            assert_eq!(g.index(cx, cy), index);
        }
    }

    #[test]
    fn world_positions_map_to_cells_including_edges() {
        let g = default_grid(20.0, 12.0, 2.0);
        assert_eq!(g.cell_coords_at(0.0, 0.0), (0, 0));
        assert_eq!(g.cell_coords_at(1.999, 0.0), (0, 0));
        assert_eq!(g.cell_coords_at(2.0, 0.0), (1, 0));
        assert_eq!(g.cell_coords_at(5.0, 7.5), (2, 3));
        assert_eq!(
            g.cell_coords_at(20.0, 12.0),
            (9, 5),
            "far edge maps to last cell"
        );
        assert_eq!(g.cell_at(20.0, 12.0), g.cell_count() - 1);
    }

    #[test]
    fn cell_centers_stay_inside_the_world() {
        let g = default_grid(20.0, 12.0, 2.0);
        assert_eq!(g.cell_center(g.index(0, 0)), (1.0, 1.0));
        assert_eq!(g.cell_center(g.index(9, 5)), (19.0, 11.0));
        let clipped = default_grid(17.0, 16.0, 2.0);
        assert_eq!(clipped.cell_center(clipped.index(8, 0)), (16.5, 1.0));
        for index in 0..clipped.cell_count() {
            let (x, y) = clipped.cell_center(index);
            assert_eq!(clipped.cell_at(x, y), index, "center lies in its own cell");
        }
    }

    #[test]
    fn initial_food_is_uniform_fraction_of_cap() {
        let food = FoodConfig {
            max_per_cell: 2.0,
            abundance: 1.5,
            initial_fraction: 0.5,
            ..FoodConfig::default()
        };
        let g = grid(16.0, 16.0, 2.0, food.clone());
        assert_eq!(g.cap(0), 3.0);
        assert!(g.amounts().iter().all(|&a| a == 1.5));
        assert_eq!(g, grid(16.0, 16.0, 2.0, food), "deterministic");
    }

    #[test]
    fn zero_abundance_means_no_food_and_no_regrowth() {
        let mut g = grid(
            16.0,
            16.0,
            2.0,
            FoodConfig {
                abundance: 0.0,
                ..FoodConfig::default()
            },
        );
        g.regrow(0.5);
        assert!(g.amounts().iter().all(|&a| a == 0.0));
    }

    #[test]
    fn regrowth_is_linear_and_stops_at_cap() {
        let mut g = grid(
            4.0,
            2.0,
            2.0,
            FoodConfig {
                max_per_cell: 1.0,
                initial_fraction: 0.0,
                ..FoodConfig::default()
            },
        );
        g.food[1] = 0.75;
        g.regrow(0.125);
        assert_eq!(g.amounts(), [0.125, 0.875]);
        g.regrow(0.125);
        assert_eq!(g.amounts(), [0.25, 1.0]);
        for _ in 0..10 {
            g.regrow(0.125);
        }
        assert_eq!(g.amounts(), [1.0, 1.0], "never above cap");
    }

    #[test]
    fn amount_eaten_respects_every_limit() {
        // Limited by intake.
        assert_eq!(amount_eaten(1.0, 0.25, 5.0, 2.0), 0.25);
        // Limited by the food in the cell.
        assert_eq!(amount_eaten(0.125, 0.25, 5.0, 2.0), 0.125);
        // Limited by remaining capacity: 0.25 energy at 2 per unit needs 0.125 food.
        assert_eq!(amount_eaten(1.0, 0.25, 0.25, 2.0), 0.125);
        // Full organism or empty cell: nothing.
        assert_eq!(amount_eaten(1.0, 0.25, 0.0, 2.0), 0.0);
        assert_eq!(amount_eaten(0.0, 0.25, 5.0, 2.0), 0.0);
        assert_eq!(amount_eaten(1.0, 0.0, 5.0, 2.0), 0.0, "zero intake");
    }

    #[test]
    fn zero_regrowth_leaves_food_unchanged() {
        let mut g = default_grid(8.0, 8.0, 2.0);
        g.food[3] = 0.25;
        let before = g.clone();
        g.regrow(0.0);
        assert_eq!(g, before);
    }
}
