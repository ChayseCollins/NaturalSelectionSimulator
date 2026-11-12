//! Goal choice and steering helpers (M-09, SCOPE 6.8).
//!
//! Prey pick the richest useful food cell within sense range and move toward it; with no
//! useful food in range they wander on a stored heading. Candidate cells are filtered by
//! the caller (same reachability region, not marked unreachable).
//!
//! Target reconsideration: at every decision a prey keeps its current food target only
//! while that target is still worthwhile, meaning it holds at least one useful bite
//! ([`MIN_USEFUL_TARGET_BITES`] × intake per tick, or the cell's whole cap when a bite is
//! larger than the cap) and at least [`TARGET_SWITCH_FRACTION`] of the best useful cell
//! in sense range. Otherwise it switches to that best cell, or wanders if there is none.
//! Reconsideration reads food only and draws no random numbers.
//!
//! Food-region scoring (SCOPE 6.8: score regions by food ÷ (1 + distance), then pick the
//! richest cell in the best region) is replaced by this direct cell ranking, recorded as a
//! provisional decision in README.md: it gives the same "richest nearby food" behavior,
//! scans the same number of cells, and is already covered by tests.
//!
//! Movement energy uses realized effort: the distance actually moved this tick divided by
//! the distance available (`max_distance_per_tick × terrain multiplier`), clamped to
//! `[0, 1]`. Standing still costs no movement energy; full-speed movement costs effort 1.

use rand_chacha::ChaCha8Rng;

use crate::food::FoodGrid;
use crate::rng;
use crate::terrain::Terrain;
use crate::traits::within_sense_range;

/// Fewest bites of food that make a cell worth targeting (P). One bite is one tick's
/// intake (`food.intake_per_tick`). A fixed provisional constant, not a scenario
/// parameter.
pub const MIN_USEFUL_TARGET_BITES: f32 = 1.0;

/// A current target stays acceptable while it holds at least this fraction of the food in
/// the best useful cell in sense range (P). Smaller differences never cause a switch.
pub const TARGET_SWITCH_FRACTION: f32 = 0.75;

/// Least food a cell must hold to be a target: one useful bite.
pub(crate) fn min_useful_food(intake_per_tick: f32) -> f32 {
    MIN_USEFUL_TARGET_BITES * intake_per_tick
}

/// A cell is useful when it holds food and at least `min_useful`, or is full when its cap
/// is below `min_useful` (so a large bite never makes all food invisible). Empty cells
/// never count, even when intake is zero.
fn is_useful(amount: f32, min_useful: f32, cap: f32) -> bool {
    amount > 0.0 && amount >= min_useful.min(cap)
}

/// Food target after one decision: the current target while it is still worthwhile,
/// otherwise the best useful accepted cell in sense range ([`select_food_target`]),
/// otherwise `None` (wander).
///
/// The current target is worthwhile when it is accepted, useful, and holds at least
/// [`TARGET_SWITCH_FRACTION`] of the best candidate's food. Reads food only.
pub(crate) fn reconsider_target(
    food: &FoodGrid,
    x: f32,
    y: f32,
    sense: f32,
    current: Option<usize>,
    min_useful: f32,
    accept: impl Fn(usize) -> bool,
) -> Option<usize> {
    let best = select_food_target(food, x, y, sense, min_useful, &accept);
    if let Some(current) = current {
        let amount = food.amounts()[current];
        let keep = accept(current)
            && is_useful(amount, min_useful, food.cap(current))
            && best.is_none_or(|best| amount >= TARGET_SWITCH_FRACTION * food.amounts()[best]);
        if keep {
            return Some(current);
        }
    }
    best
}

/// Richest useful accepted food cell whose center is within `sense` of `(x, y)`.
///
/// Ranking: more food first, then smaller squared distance to the cell center, then lower
/// cell index. Only the cells inside the sensing circle's bounding box are examined.
pub(crate) fn select_food_target(
    food: &FoodGrid,
    x: f32,
    y: f32,
    sense: f32,
    min_useful: f32,
    accept: impl Fn(usize) -> bool,
) -> Option<usize> {
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
            if !is_useful(amount, min_useful, food.cap(index)) {
                continue;
            }
            if let Some((_, best_amount, _)) = best
                && amount < best_amount
            {
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
            if better && accept(index) {
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

/// Heading of the vector `(dx, dy)` in `[0, 2π)`, or `fallback` for a zero vector.
pub(crate) fn heading_of(dx: f32, dy: f32, fallback: f32) -> f32 {
    if dx == 0.0 && dy == 0.0 {
        return fallback;
    }
    let angle = libm::atan2f(dy, dx);
    let wrapped = if angle < 0.0 {
        angle + core::f32::consts::TAU
    } else {
        angle
    };
    if wrapped >= core::f32::consts::TAU {
        0.0
    } else {
        wrapped
    }
}

/// Result of one movement step against terrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Step {
    pub x: f32,
    pub y: f32,
    /// Distance actually moved.
    pub moved: f32,
    /// Terrain or the world edge stopped part of the step.
    pub obstructed: bool,
}

/// Moves from `(x, y)` to `(to_x, to_y)` (a step of length `length`) through passable
/// terrain with wall sliding (SCOPE 6.8 step 7). The step is split into pieces of at most
/// half a cell, so it can never jump over a cell. For each piece: if the destination is
/// passable it is taken; otherwise the piece's x part alone, then its y part alone, is
/// tried, so the organism slides along the blocking edge; if neither is possible the rest
/// of the step is dropped. Positions stay inside the world (solid edges).
pub(crate) fn step_through_terrain(
    terrain: &Terrain,
    x: f32,
    y: f32,
    to_x: f32,
    to_y: f32,
    length: f32,
) -> Step {
    let g = terrain.geometry();
    let (raw_x, raw_y) = (to_x, to_y);
    let (to_x, to_y) = clamp_to_world(to_x, to_y, g.world_width(), g.world_height());
    let edge_cut = (to_x, to_y) != (raw_x, raw_y);
    let (total_dx, total_dy) = (to_x - x, to_y - y);
    if total_dx == 0.0 && total_dy == 0.0 {
        return Step {
            x,
            y,
            moved: 0.0,
            obstructed: length > 0.0,
        };
    }
    let pieces = ((length / (0.5 * g.cell_size())).ceil() as u32).max(1);
    let (piece_dx, piece_dy) = (total_dx / pieces as f32, total_dy / pieces as f32);
    let (mut cx, mut cy) = (x, y);
    let mut obstructed = false;
    for piece in 0..pieces {
        // The last piece lands exactly on the target, avoiding accumulated rounding.
        let (nx, ny) = if piece + 1 == pieces && !obstructed {
            (to_x, to_y)
        } else {
            (cx + piece_dx, cy + piece_dy)
        };
        if terrain.passable_at(nx, ny) {
            (cx, cy) = (nx, ny);
        } else if piece_dx != 0.0 && terrain.passable_at(nx, cy) {
            (cx, cy) = (nx, cy);
            obstructed = true;
        } else if piece_dy != 0.0 && terrain.passable_at(cx, ny) {
            (cx, cy) = (cx, ny);
            obstructed = true;
        } else {
            obstructed = true;
            break;
        }
    }
    // The executed step length, unless terrain or an edge cut it short; measuring every
    // step from f32 positions would add rounding noise of up to one ulp per axis.
    let moved = if obstructed || edge_cut {
        distance(x, y, cx, cy)
    } else {
        length
    };
    Step {
        x: cx,
        y: cy,
        moved,
        obstructed: obstructed || edge_cut,
    }
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

    /// A 20 × 20 open world of 2-unit cells (10 × 10 grid) with no food.
    fn empty_grid() -> FoodGrid {
        FoodGrid::new(
            &Terrain::open(&WorldConfig {
                width: 20.0,
                height: 20.0,
                cell_size: 2.0,
                ..WorldConfig::default()
            }),
            &FoodConfig {
                initial_fraction: 0.0,
                ..FoodConfig::default()
            },
        )
    }

    fn any(_: usize) -> bool {
        true
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
            select_food_target(&grid, 1.0, 1.0, 10.0, 0.0, any),
            Some(grid.index(5, 0))
        );
        assert_eq!(select_food_target(&grid, 1.0, 1.0, 9.99, 0.0, any), None);
    }

    #[test]
    fn richest_cell_wins_then_nearest_then_lowest_index() {
        let grid = with_food(&[((2, 0), 0.5), ((4, 0), 0.75)]);
        assert_eq!(
            select_food_target(&grid, 1.0, 1.0, 20.0, 0.0, any),
            Some(grid.index(4, 0)),
            "more food beats distance"
        );

        let grid = with_food(&[((2, 0), 0.5), ((4, 0), 0.5)]);
        assert_eq!(
            select_food_target(&grid, 1.0, 1.0, 20.0, 0.0, any),
            Some(grid.index(2, 0)),
            "equal food: nearer wins"
        );

        // (2, 2) is equidistant from the centers (1, 3) of cell (0, 1) and (3, 1) of
        // cell (1, 0), which hold equal food.
        let grid = with_food(&[((0, 1), 0.5), ((1, 0), 0.5)]);
        assert_eq!(
            select_food_target(&grid, 2.0, 2.0, 5.0, 0.0, any),
            Some(grid.index(1, 0)),
            "full tie: lower index (1, 0) = 1 beats (0, 1) = 10"
        );
    }

    #[test]
    fn empty_cells_are_never_selected() {
        let grid = with_food(&[((9, 9), 0.01)]);
        // Many empty cells are nearer, but only the non-empty one counts.
        assert_eq!(
            select_food_target(&grid, 10.0, 10.0, 15.0, 0.0, any),
            Some(grid.index(9, 9))
        );
        assert_eq!(
            select_food_target(&empty_grid(), 10.0, 10.0, 50.0, 0.0, any),
            None
        );
    }

    #[test]
    fn search_is_clipped_at_world_edges() {
        let grid = with_food(&[((0, 0), 0.5), ((9, 9), 0.5)]);
        assert_eq!(select_food_target(&grid, 0.0, 0.0, 3.0, 0.0, any), Some(0));
        assert_eq!(
            select_food_target(&grid, 20.0, 20.0, 3.0, 0.0, any),
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

    // ----- Target reconsideration -----

    /// One bite at the default intake.
    const BITE: f32 = 0.05;

    /// Prey standing at the center (5, 5) of cell (2, 2), sense 10.
    fn reconsider(grid: &FoodGrid, current: Option<usize>) -> Option<usize> {
        reconsider_target(grid, 5.0, 5.0, 10.0, current, BITE, any)
    }

    #[test]
    fn useful_food_needs_at_least_one_bite() {
        assert_eq!(min_useful_food(0.05), 0.05 * MIN_USEFUL_TARGET_BITES);
        let grid = with_food(&[((3, 2), 0.049), ((6, 2), 0.05)]);
        assert_eq!(
            select_food_target(&grid, 5.0, 5.0, 10.0, BITE, any),
            Some(grid.index(6, 2)),
            "less than a bite is ignored even when nearer"
        );
        let grid = with_food(&[((3, 2), 0.049)]);
        assert_eq!(select_food_target(&grid, 5.0, 5.0, 10.0, BITE, any), None);
        assert!(
            !is_useful(0.0, 0.0, 1.0),
            "empty never counts, even at zero intake"
        );
        assert!(
            is_useful(0.5, 2.0, 0.5),
            "a full cell counts even when a bite is larger"
        );
        assert!(!is_useful(0.4, 2.0, 0.5));
    }

    #[test]
    fn worthwhile_current_target_is_kept() {
        // Current (2, 2) holds 0.8; the best other cell holds 1.0 (0.8 ≥ 0.75 × 1.0).
        let grid = with_food(&[((2, 2), 0.8), ((4, 2), 1.0)]);
        let current = grid.index(2, 2);
        assert_eq!(reconsider(&grid, Some(current)), Some(current));
        // It is also kept when it is itself the best candidate.
        let grid = with_food(&[((2, 2), 1.0), ((4, 2), 1.0)]);
        assert_eq!(reconsider(&grid, Some(current)), Some(current));
        // Exactly at the switch fraction still counts as worthwhile.
        let grid = with_food(&[((2, 2), 0.75), ((4, 2), 1.0)]);
        assert_eq!(reconsider(&grid, Some(current)), Some(current));
    }

    #[test]
    fn current_target_below_one_bite_is_abandoned_for_a_useful_one() {
        let grid = with_food(&[((2, 2), 0.04), ((4, 2), 0.06)]);
        assert_eq!(
            reconsider(&grid, Some(grid.index(2, 2))),
            Some(grid.index(4, 2))
        );
        // With no useful alternative the target is dropped: the prey wanders.
        let grid = with_food(&[((2, 2), 0.04), ((4, 2), 0.03)]);
        assert_eq!(reconsider(&grid, Some(grid.index(2, 2))), None);
        // Even a below-a-bite target is dropped when it is all there is.
        let grid = with_food(&[((2, 2), 0.04)]);
        assert_eq!(reconsider(&grid, Some(grid.index(2, 2))), None);
    }

    #[test]
    fn materially_richer_cell_replaces_the_current_target() {
        // 0.74 < 0.75 × 1.0: switch.
        let grid = with_food(&[((2, 2), 0.74), ((4, 2), 1.0)]);
        assert_eq!(
            reconsider(&grid, Some(grid.index(2, 2))),
            Some(grid.index(4, 2))
        );
    }

    #[test]
    fn small_differences_never_cause_a_switch() {
        let current_index = |grid: &FoodGrid| grid.index(2, 2);
        for current in [0.76, 0.9, 0.99, 0.999_9] {
            let grid = with_food(&[((2, 2), current), ((3, 2), 1.0), ((4, 4), 1.0)]);
            assert_eq!(
                reconsider(&grid, Some(current_index(&grid))),
                Some(current_index(&grid)),
                "current {current} vs best 1.0"
            );
        }
    }

    #[test]
    fn switching_uses_the_deterministic_ranking() {
        let current = |grid: &FoodGrid| Some(grid.index(2, 2));
        // More food wins over distance.
        let grid = with_food(&[((2, 2), 0.01), ((3, 2), 0.5), ((5, 2), 0.6)]);
        assert_eq!(reconsider(&grid, current(&grid)), Some(grid.index(5, 2)));
        // Equal food: the nearer center wins.
        let grid = with_food(&[((2, 2), 0.01), ((3, 2), 0.5), ((5, 2), 0.5)]);
        assert_eq!(reconsider(&grid, current(&grid)), Some(grid.index(3, 2)));
        // Equal food and distance: the lower index wins. (3, 2) = 23 beats (2, 3) = 32,
        // both one cell from (2, 2).
        let grid = with_food(&[((2, 2), 0.01), ((2, 3), 0.5), ((3, 2), 0.5)]);
        assert_eq!(reconsider(&grid, current(&grid)), Some(grid.index(3, 2)));
        // Same input, same answer.
        assert_eq!(
            reconsider(&grid, current(&grid)),
            reconsider(&grid.clone(), current(&grid))
        );
    }

    #[test]
    fn without_a_current_target_the_best_useful_cell_is_chosen() {
        let grid = with_food(&[((3, 2), 0.04), ((4, 2), 0.3)]);
        assert_eq!(reconsider(&grid, None), Some(grid.index(4, 2)));
        assert_eq!(reconsider(&empty_grid(), None), None);
    }

    #[test]
    fn rejected_cells_are_never_targets() {
        let grid = with_food(&[((3, 2), 0.9), ((4, 2), 0.5)]);
        let rejected = grid.index(3, 2);
        assert_eq!(
            select_food_target(&grid, 5.0, 5.0, 10.0, BITE, |cell| cell != rejected),
            Some(grid.index(4, 2))
        );
        assert_eq!(
            reconsider_target(&grid, 5.0, 5.0, 10.0, Some(rejected), BITE, |cell| cell
                != rejected),
            Some(grid.index(4, 2)),
            "a current target that became unacceptable is dropped"
        );
    }

    #[test]
    fn walls_stop_movement_and_steps_slide_along_them() {
        let terrain = Terrain::from_ascii(
            "...#\n\
             ...#\n\
             ...#\n",
            2.0,
        )
        .expect("valid map");
        // Straight into the wall: stops short of it, obstructed.
        let step = step_through_terrain(&terrain, 5.0, 3.0, 7.0, 3.0, 2.0);
        assert!(step.x < 6.0 && step.obstructed, "{step:?}");
        assert!(terrain.passable_at(step.x, step.y));
        // Diagonally into the wall: slides up along it.
        let step = step_through_terrain(&terrain, 5.0, 1.0, 7.0, 3.0, 8.0_f32.sqrt());
        assert!(step.y > 2.5 && step.x < 6.0 && step.obstructed, "{step:?}");
        // Open ground: exact arrival, unobstructed.
        let step = step_through_terrain(&terrain, 1.0, 1.0, 2.0, 2.0, 2.0_f32.sqrt());
        assert_eq!((step.x, step.y), (2.0, 2.0));
        assert!(!step.obstructed);
        // The world edge is solid.
        let step = step_through_terrain(&terrain, 0.5, 1.0, -1.0, 1.0, 1.5);
        assert_eq!((step.x, step.y), (0.0, 1.0));
        assert!(step.obstructed);
    }

    #[test]
    fn long_steps_cannot_jump_over_thin_walls() {
        let terrain = Terrain::from_ascii(".#.\n", 2.0).expect("valid map");
        let step = step_through_terrain(&terrain, 1.0, 1.0, 5.0, 1.0, 4.0);
        assert!(step.x < 2.0, "{step:?}");
    }

    #[test]
    fn heading_of_points_along_the_vector() {
        assert_eq!(heading_of(1.0, 0.0, 9.0), 0.0);
        assert!((heading_of(0.0, 1.0, 9.0) - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((heading_of(-1.0, 0.0, 9.0) - core::f32::consts::PI).abs() < 1e-6);
        assert!(heading_of(0.0, -1.0, 9.0) > core::f32::consts::PI);
        assert_eq!(heading_of(0.0, 0.0, 1.5), 1.5);
    }
}
