//! Basic goal-directed behavior in an unobstructed world (the Week 1 part of M-09).
//!
//! Prey pick the richest food cell within sense range and move straight toward it;
//! with no food in range they wander on a stored heading. There are no barriers, routes,
//! predators or spatial-grid regions yet (SCOPE 6.8's region scoring needs M-08).
//!
//! Movement energy uses realized effort: the distance actually moved this tick divided by
//! the distance available (`max_distance_per_tick × terrain multiplier`), clamped to
//! `[0, 1]`. Standing still costs no movement energy; full-speed movement costs effort 1.

use rand_chacha::ChaCha8Rng;

use crate::food::FoodGrid;
use crate::rng;
use crate::traits::within_sense_range;

/// Richest food cell whose center is within `sense` of `(x, y)`.
///
/// Ranking: more food first, then smaller squared distance to the cell center, then lower
/// cell index. Only cells holding food count; `None` means nothing useful is in range.
/// Only the cells inside the sensing circle's bounding box are examined.
pub(crate) fn select_food_target(food: &FoodGrid, x: f32, y: f32, sense: f32) -> Option<usize> {
    let cell_size = food.cell_size();
    let bounds = |center: f32, cells: u32| {
        let first = ((center - sense) / cell_size).max(0.0) as u32;
        let last = (((center + sense) / cell_size).max(0.0) as u32).min(cells - 1);
        (first.min(cells - 1), last)
    };
    let (x_first, x_last) = bounds(x, food.width());
    let (y_first, y_last) = bounds(y, food.height());

    let mut best: Option<(usize, f32, f32)> = None;
    for cy in y_first..=y_last {
        for cx in x_first..=x_last {
            let index = food.index(cx, cy);
            let amount = food.amounts()[index];
            if amount <= 0.0 {
                continue;
            }
            let (center_x, center_y) = food.cell_center(index);
            let (dx, dy) = (center_x - x, center_y - y);
            if !within_sense_range(dx, dy, sense) {
                continue;
            }
            let distance_squared = dx * dx + dy * dy;
            let better = match best {
                None => true,
                Some((_, best_amount, best_distance)) => {
                    amount > best_amount
                        || (amount == best_amount && distance_squared < best_distance)
                }
            };
            if better {
                best = Some((index, amount, distance_squared));
            }
        }
    }
    best.map(|(index, _, _)| index)
}

/// Straight-line step from `(x, y)` toward `(target_x, target_y)` of at most
/// `max_distance`, returning the new position and the step length. Lands exactly on the
/// target when it is within reach, so it never overshoots.
pub(crate) fn move_toward(
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    max_distance: f32,
) -> (f32, f32, f32) {
    let (dx, dy) = (target_x - x, target_y - y);
    let distance = libm::sqrtf(dx * dx + dy * dy);
    if distance <= max_distance {
        return (target_x, target_y, distance);
    }
    let scale = max_distance / distance;
    (x + dx * scale, y + dy * scale, max_distance)
}

/// Step of `distance` along `heading` (radians, counterclockwise from +x).
pub(crate) fn move_along(x: f32, y: f32, heading: f32, distance: f32) -> (f32, f32) {
    (
        x + distance * libm::cosf(heading),
        y + distance * libm::sinf(heading),
    )
}

/// Keeps a position inside `[0, width] × [0, height]` (solid world edges).
pub(crate) fn clamp_to_world(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    (x.clamp(0.0, width), y.clamp(0.0, height))
}

pub(crate) fn distance(x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let (dx, dy) = (x1 - x0, y1 - y0);
    libm::sqrtf(dx * dx + dy * dy)
}

/// Terrain speed multiplier at a position. Always 1.0 until generated terrain exists;
/// M-06 replaces this with a lookup of the cell's terrain class.
pub(crate) fn terrain_speed_multiplier(_x: f32, _y: f32) -> f32 {
    1.0
}

/// Realized movement effort: `actual / available`, clamped to `[0, 1]`.
pub(crate) fn realized_effort(actual_distance: f32, available_distance: f32) -> f32 {
    if available_distance <= 0.0 {
        return 0.0;
    }
    (actual_distance / available_distance).clamp(0.0, 1.0)
}

/// New wandering heading in `[0, 2π)` from exactly one behavior-domain draw.
pub(crate) fn wander_heading(behavior: &mut ChaCha8Rng) -> f32 {
    rng::unit_f32(behavior) * core::f32::consts::TAU
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FoodConfig, WorldConfig};

    /// A 20 × 20 world of 2-unit cells (10 × 10 grid) with no food.
    fn empty_grid() -> FoodGrid {
        FoodGrid::new(
            &WorldConfig {
                width: 20.0,
                height: 20.0,
                cell_size: 2.0,
            },
            &FoodConfig {
                initial_fraction: 0.0,
                ..FoodConfig::default()
            },
        )
    }

    fn with_food(cells: &[((u32, u32), f32)]) -> FoodGrid {
        let mut grid = empty_grid();
        for &((cx, cy), amount) in cells {
            let index = grid.index(cx, cy);
            grid.food[index] = amount;
        }
        grid
    }

    #[test]
    fn food_inside_sense_range_is_detected_and_outside_is_not() {
        // Cell (5, 0) has its center at (11, 1); the prey stands at (1, 1).
        let grid = with_food(&[((5, 0), 0.5)]);
        assert_eq!(
            select_food_target(&grid, 1.0, 1.0, 10.0),
            Some(grid.index(5, 0))
        );
        assert_eq!(select_food_target(&grid, 1.0, 1.0, 9.99), None);
    }

    #[test]
    fn richest_cell_wins_then_nearest_then_lowest_index() {
        let grid = with_food(&[((2, 0), 0.5), ((4, 0), 0.75)]);
        assert_eq!(
            select_food_target(&grid, 1.0, 1.0, 20.0),
            Some(grid.index(4, 0)),
            "more food beats distance"
        );

        let grid = with_food(&[((2, 0), 0.5), ((4, 0), 0.5)]);
        assert_eq!(
            select_food_target(&grid, 1.0, 1.0, 20.0),
            Some(grid.index(2, 0)),
            "equal food: nearer wins"
        );

        // (2, 2) is equidistant from the centers (1, 3) of cell (0, 1) and (3, 1) of
        // cell (1, 0), which hold equal food.
        let grid = with_food(&[((0, 1), 0.5), ((1, 0), 0.5)]);
        assert_eq!(
            select_food_target(&grid, 2.0, 2.0, 5.0),
            Some(grid.index(1, 0)),
            "full tie: lower index (1, 0) = 1 beats (0, 1) = 10"
        );
    }

    #[test]
    fn empty_cells_are_never_selected() {
        let grid = with_food(&[((9, 9), 0.01)]);
        // Many empty cells are nearer, but only the non-empty one counts.
        assert_eq!(
            select_food_target(&grid, 10.0, 10.0, 15.0),
            Some(grid.index(9, 9))
        );
        assert_eq!(select_food_target(&empty_grid(), 10.0, 10.0, 50.0), None);
    }

    #[test]
    fn search_is_clipped_at_world_edges() {
        let grid = with_food(&[((0, 0), 0.5), ((9, 9), 0.5)]);
        assert_eq!(select_food_target(&grid, 0.0, 0.0, 3.0), Some(0));
        assert_eq!(
            select_food_target(&grid, 20.0, 20.0, 3.0),
            Some(grid.cell_count() - 1)
        );
    }

    #[test]
    fn move_toward_is_capped_and_never_overshoots() {
        let (x, y, step) = move_toward(0.0, 0.0, 3.0, 4.0, 1.0);
        assert_eq!(step, 1.0);
        assert!(
            (distance(0.0, 0.0, x, y) - 1.0).abs() < 1e-6,
            "capped at max distance"
        );
        assert!(
            (x - 0.6).abs() < 1e-6 && (y - 0.8).abs() < 1e-6,
            "toward the target"
        );
        assert_eq!(
            move_toward(0.0, 0.0, 3.0, 4.0, 5.0),
            (3.0, 4.0, 5.0),
            "exact arrival"
        );
        assert_eq!(
            move_toward(0.0, 0.0, 3.0, 4.0, 50.0),
            (3.0, 4.0, 5.0),
            "no overshoot"
        );
        assert_eq!(move_toward(2.0, 2.0, 2.0, 2.0, 1.0), (2.0, 2.0, 0.0));
    }

    #[test]
    fn positions_are_clamped_to_the_world() {
        assert_eq!(clamp_to_world(-1.0, 25.0, 20.0, 20.0), (0.0, 20.0));
        assert_eq!(clamp_to_world(5.0, 5.0, 20.0, 20.0), (5.0, 5.0));
        let (x, y) = move_along(19.5, 10.0, 0.0, 2.0);
        assert_eq!(clamp_to_world(x, y, 20.0, 20.0), (20.0, 10.0));
    }

    #[test]
    fn realized_effort_is_actual_over_available() {
        assert_eq!(realized_effort(0.0, 0.5), 0.0, "no movement, no effort");
        assert_eq!(realized_effort(0.25, 0.5), 0.5);
        assert_eq!(realized_effort(0.5, 0.5), 1.0);
        assert_eq!(realized_effort(0.5000001, 0.5), 1.0, "clamped at 1");
        assert_eq!(realized_effort(0.0, 0.0), 0.0);
    }

    #[test]
    fn wander_heading_uses_one_draw_and_stays_in_range() {
        let mut behavior = crate::rng::RandomDomains::new(1, 1).behavior;
        for _ in 0..1_000 {
            let heading = wander_heading(&mut behavior);
            assert!((0.0..core::f32::consts::TAU).contains(&heading));
        }
        assert_eq!(
            behavior.get_word_pos(),
            2_000,
            "one 64-bit draw per heading"
        );
    }
}
