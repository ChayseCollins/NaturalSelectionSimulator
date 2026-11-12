//! Bounded route search on the terrain grid (M-09, SCOPE 6.8 steps 3 to 5).
//!
//! A* with 8 neighbors over passable cells. A diagonal step is allowed only when both
//! cells beside it are passable, so routes never cut corners. Entering a cell costs its
//! step length (100 orthogonal, 141 diagonal) divided by the cell's speed multiplier, so
//! routes prefer fast terrain; the octile-distance heuristic stays admissible because no
//! multiplier exceeds 1. Costs are integers and the open set is ordered by
//! `(estimated total, cell)`, so equal-cost ties always break the same way.
//!
//! Each search is limited to a square window around the organism (sense range plus a
//! margin) and to a number of node expansions. Scratch buffers are reused across
//! searches, with generation stamps instead of clearing.
//!
//! Found routes are smoothed: a waypoint is dropped while the straight segment from the
//! previous kept point to the next waypoint is clear, so organisms walk straight lines
//! between corners instead of cell centers.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::terrain::Terrain;

const ORTHOGONAL: u32 = 100;
const DIAGONAL: u32 = 141;

/// Outcome of one route request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchOutcome {
    /// Waypoint cells after the start, ending at the goal cell (smoothed).
    Found(Vec<u32>),
    /// No route exists inside the window.
    NoRoute,
    /// The expansion cap was reached first.
    CapReached,
}

/// Reusable search buffers. Scratch only: they never carry information between searches,
/// so any two compare equal.
#[derive(Debug, Clone, Default)]
pub(crate) struct RouteSearch {
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    closed: Vec<u32>,
    stamp: u32,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
}

impl PartialEq for RouteSearch {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// A rectangle of cells, inclusive.
#[derive(Debug, Clone, Copy)]
struct Window {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Window {
    fn width(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    fn contains(&self, x: u32, y: u32) -> bool {
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }

    fn local(&self, x: u32, y: u32) -> usize {
        ((y - self.y0) * self.width() + (x - self.x0)) as usize
    }
}

impl RouteSearch {
    /// Searches from `start` toward `goal` (cell indices) within `half_window` cells of
    /// the start, expanding at most `max_expansions` nodes. Returns the outcome and the
    /// number of expansions used (never more than `max_expansions`).
    pub(crate) fn find(
        &mut self,
        terrain: &Terrain,
        start_point: (f32, f32),
        goal: usize,
        half_window: u32,
        max_expansions: u32,
    ) -> (SearchOutcome, u32) {
        let g = terrain.geometry();
        let start = g.cell_at(start_point.0, start_point.1);
        let (sx, sy) = g.coords(start);
        let (gx, gy) = g.coords(goal);
        let window = Window {
            x0: sx.saturating_sub(half_window),
            y0: sy.saturating_sub(half_window),
            x1: (sx + half_window).min(g.width() - 1),
            y1: (sy + half_window).min(g.height() - 1),
        };
        if !window.contains(gx, gy)
            || !terrain.passable_cell(start)
            || !terrain.passable_cell(goal)
            || terrain.region(start) != terrain.region(goal)
        {
            return (SearchOutcome::NoRoute, 0);
        }
        if start == goal {
            return (SearchOutcome::Found(vec![goal as u32]), 0);
        }

        let size = (window.width() * (window.y1 - window.y0 + 1)) as usize;
        if self.g.len() < size {
            self.g.resize(size, 0);
            self.parent.resize(size, 0);
            self.seen.resize(size, 0);
            self.closed.resize(size, 0);
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.iter_mut().for_each(|s| *s = 0);
            self.closed.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        let stamp = self.stamp;
        self.heap.clear();

        let heuristic = |x: u32, y: u32| {
            let (dx, dy) = (x.abs_diff(gx), y.abs_diff(gy));
            let (low, high) = (dx.min(dy), dx.max(dy));
            DIAGONAL * low + ORTHOGONAL * (high - low)
        };
        let start_local = window.local(sx, sy);
        self.g[start_local] = 0;
        self.seen[start_local] = stamp;
        self.parent[start_local] = start_local as u32;
        self.heap
            .push(Reverse((heuristic(sx, sy), start_local as u32)));

        let mut expansions = 0;
        while let Some(Reverse((_, local))) = self.heap.pop() {
            let local = local as usize;
            if self.closed[local] == stamp {
                continue;
            }
            if expansions >= max_expansions {
                return (SearchOutcome::CapReached, expansions);
            }
            self.closed[local] = stamp;
            expansions += 1;
            let x = window.x0 + (local as u32 % window.width());
            let y = window.y0 + (local as u32 / window.width());
            if (x, y) == (gx, gy) {
                let route = self.reconstruct(&window, terrain, local, start_point);
                return (SearchOutcome::Found(route), expansions);
            }
            let passable = |nx: i64, ny: i64| {
                nx >= i64::from(window.x0)
                    && ny >= i64::from(window.y0)
                    && nx <= i64::from(window.x1)
                    && ny <= i64::from(window.y1)
                    && terrain.passable_cell(g.index(nx as u32, ny as u32))
            };
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                if !passable(nx, ny) {
                    continue;
                }
                let diagonal = dx != 0 && dy != 0;
                if diagonal
                    && (!passable(i64::from(x) + dx, i64::from(y))
                        || !passable(i64::from(x), i64::from(y) + dy))
                {
                    continue;
                }
                let (nx, ny) = (nx as u32, ny as u32);
                let neighbor = window.local(nx, ny);
                if self.closed[neighbor] == stamp {
                    continue;
                }
                let base = if diagonal { DIAGONAL } else { ORTHOGONAL };
                let multiplier = terrain.class(g.index(nx, ny)).speed_multiplier();
                let cost = (base as f32 / multiplier).round() as u32;
                let tentative = self.g[local] + cost;
                if self.seen[neighbor] != stamp || tentative < self.g[neighbor] {
                    self.seen[neighbor] = stamp;
                    self.g[neighbor] = tentative;
                    self.parent[neighbor] = local as u32;
                    self.heap
                        .push(Reverse((tentative + heuristic(nx, ny), neighbor as u32)));
                }
            }
        }
        (SearchOutcome::NoRoute, expansions)
    }

    /// Cells from just after the start to the goal, then smoothed.
    fn reconstruct(
        &self,
        window: &Window,
        terrain: &Terrain,
        goal_local: usize,
        start_point: (f32, f32),
    ) -> Vec<u32> {
        let g = terrain.geometry();
        let mut cells = Vec::new();
        let mut local = goal_local;
        while self.parent[local] as usize != local {
            let x = window.x0 + (local as u32 % window.width());
            let y = window.y0 + (local as u32 / window.width());
            cells.push(g.index(x, y) as u32);
            local = self.parent[local] as usize;
        }
        cells.reverse();
        smooth(terrain, start_point, &cells)
    }
}

/// Drops waypoints that a straight segment can skip (greedy string pulling). A segment
/// may replace part of the route only when it is clear and no slower: its slowest cell is
/// at least as fast as the slowest cell of the route part it replaces, so smoothing never
/// undoes the search's preference for fast terrain.
pub(crate) fn smooth(terrain: &Terrain, start: (f32, f32), cells: &[u32]) -> Vec<u32> {
    let g = terrain.geometry();
    let multiplier = |cell: u32| terrain.class(cell as usize).speed_multiplier();
    let mut result = Vec::new();
    let mut anchor = start;
    let mut anchor_multiplier = terrain.speed_multiplier_at(start.0, start.1);
    let mut i = 0;
    while i < cells.len() {
        let mut j = i;
        let mut slowest = anchor_multiplier.min(multiplier(cells[i]));
        while j + 1 < cells.len() {
            let candidate_slowest = slowest.min(multiplier(cells[j + 1]));
            let (x, y) = g.cell_center(cells[j + 1] as usize);
            match terrain.segment_min_multiplier(anchor.0, anchor.1, x, y) {
                Some(lowest) if lowest >= candidate_slowest => {
                    j += 1;
                    slowest = candidate_slowest;
                }
                _ => break,
            }
        }
        result.push(cells[j]);
        anchor = g.cell_center(cells[j] as usize);
        anchor_multiplier = multiplier(cells[j]);
        i = j + 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(text: &str) -> Terrain {
        Terrain::from_ascii(text, 1.0).expect("valid map")
    }

    fn center(terrain: &Terrain, x: u32, y: u32) -> (f32, f32) {
        terrain
            .geometry()
            .cell_center(terrain.geometry().index(x, y))
    }

    /// Walks a route from `start` and checks every leg is a clear straight segment.
    fn assert_walkable(terrain: &Terrain, start: (f32, f32), route: &[u32]) {
        let mut at = start;
        for &cell in route {
            let next = terrain.geometry().cell_center(cell as usize);
            assert!(
                terrain.segment_clear(at.0, at.1, next.0, next.1),
                "leg {at:?} -> {next:?} is blocked"
            );
            at = next;
        }
    }

    #[test]
    fn finds_a_route_through_a_wall_opening() {
        let terrain = map("..........\n\
             ..........\n\
             #####.####\n\
             ..........\n\
             ..........\n");
        let start = center(&terrain, 1, 0);
        let goal = terrain.geometry().index(1, 4);
        let mut search = RouteSearch::default();
        let (outcome, expansions) = search.find(&terrain, start, goal, 20, 512);
        let SearchOutcome::Found(route) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(*route.last().expect("non-empty"), goal as u32);
        assert!(route.len() <= 4, "smoothed: {route:?}");
        assert!(expansions > 0 && expansions <= 512);
        assert_walkable(&terrain, start, &route);
    }

    #[test]
    fn identical_searches_give_identical_routes() {
        let terrain = map("......#...\n\
             ..##..#...\n\
             ..##......\n\
             ......##..\n");
        let start = center(&terrain, 0, 0);
        let goal = terrain.geometry().index(9, 3);
        let mut a = RouteSearch::default();
        let mut b = RouteSearch::default();
        let first = a.find(&terrain, start, goal, 20, 512);
        // Reusing scratch must not change the result.
        let _ = a.find(&terrain, center(&terrain, 9, 0), goal, 20, 512);
        assert_eq!(a.find(&terrain, start, goal, 20, 512), first);
        assert_eq!(b.find(&terrain, start, goal, 20, 512), first);
    }

    #[test]
    fn sealed_goals_and_goals_outside_the_window_have_no_route() {
        let terrain = map("...###\n\
             ...#.#\n\
             ...###\n");
        let mut search = RouteSearch::default();
        let sealed = terrain.geometry().index(4, 1);
        let (outcome, expansions) = search.find(&terrain, center(&terrain, 0, 1), sealed, 10, 512);
        assert_eq!(outcome, SearchOutcome::NoRoute);
        assert_eq!(
            expansions, 0,
            "different regions are rejected without search"
        );

        let open = map("..........\n");
        let far = open.geometry().index(9, 0);
        let (outcome, _) = search.find(&open, center(&open, 0, 0), far, 3, 512);
        assert_eq!(outcome, SearchOutcome::NoRoute, "outside the window");
    }

    #[test]
    fn expansion_cap_is_respected() {
        let terrain = map("....................\n\
             .##################.\n\
             ...................#\n\
             ##################..\n\
             ....................\n");
        let mut search = RouteSearch::default();
        let goal = terrain.geometry().index(0, 4);
        let (outcome, expansions) = search.find(&terrain, center(&terrain, 0, 0), goal, 30, 10);
        assert_eq!(outcome, SearchOutcome::CapReached);
        assert_eq!(expansions, 10);
        let (outcome, expansions) = search.find(&terrain, center(&terrain, 0, 0), goal, 30, 512);
        assert!(matches!(outcome, SearchOutcome::Found(_)), "{outcome:?}");
        assert!(expansions <= 512);
    }

    #[test]
    fn routes_never_cut_corners() {
        let terrain = map(".#\n\
             #.\n");
        let mut search = RouteSearch::default();
        let goal = terrain.geometry().index(1, 1);
        let (outcome, _) = search.find(&terrain, center(&terrain, 0, 0), goal, 5, 100);
        assert_eq!(outcome, SearchOutcome::NoRoute);
    }

    #[test]
    fn routes_prefer_fast_terrain() {
        // A straight line through hills versus a detour over grass.
        let terrain = map(".......\n\
             .^^^^^.\n\
             .^^^^^.\n\
             .......\n");
        let mut search = RouteSearch::default();
        let start = center(&terrain, 0, 1);
        let goal = terrain.geometry().index(6, 1);
        let (SearchOutcome::Found(route), _) = search.find(&terrain, start, goal, 10, 512) else {
            panic!("route expected");
        };
        // The straight line crosses five hill cells; the route walks around them over
        // grassland, touching hill cells at most at corners.
        assert!(route.len() >= 2, "not the straight line: {route:?}");
        for &cell in &route {
            assert_ne!(
                terrain.class(cell as usize),
                crate::terrain::TerrainClass::Hills,
                "{route:?}"
            );
        }
    }
}
