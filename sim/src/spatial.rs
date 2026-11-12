//! Uniform spatial grid for neighbor queries (M-08, SCOPE 6.1, 7.2).
//!
//! Rebuilt every tick by counting sort: organisms are bucketed by grid cell into one
//! index array with per-cell offsets. Within a cell, organisms appear in ascending storage
//! index, and queries visit cells row by row, so query order is deterministic. Buffers are
//! reused, so rebuilding allocates nothing after warm-up.
//!
//! Sensing, predation and click picking all go through this grid (M-08 acceptance).

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialGrid {
    cell_size: f32,
    columns: u32,
    rows: u32,
    /// `starts[c]..starts[c + 1]` is cell `c`'s range in `entries`.
    starts: Vec<u32>,
    /// Storage indices, grouped by cell.
    entries: Vec<u32>,
    /// Scratch: each entry's cell, insertion cursors and the unsorted indices.
    cells: Vec<u32>,
    cursor: Vec<u32>,
    unsorted: Vec<u32>,
}

impl SpatialGrid {
    pub fn new(world_width: f32, world_height: f32, cell_size: f32) -> Self {
        let columns = ((world_width / cell_size).ceil() as u32).max(1);
        let rows = ((world_height / cell_size).ceil() as u32).max(1);
        Self {
            cell_size,
            columns,
            rows,
            starts: vec![0; (columns * rows) as usize + 1],
            entries: Vec::new(),
            cells: Vec::new(),
            cursor: Vec::new(),
            unsorted: Vec::new(),
        }
    }

    fn cell_coords(&self, x: f32, y: f32) -> (u32, u32) {
        let cx = ((x / self.cell_size).max(0.0) as u32).min(self.columns - 1);
        let cy = ((y / self.cell_size).max(0.0) as u32).min(self.rows - 1);
        (cx, cy)
    }

    /// Rebuilds the grid from positions, including only the storage indices for which
    /// `include` returns true.
    pub fn rebuild(&mut self, xs: &[f32], ys: &[f32], include: impl Fn(usize) -> bool) {
        let cell_count = (self.columns * self.rows) as usize;
        self.starts.clear();
        self.starts.resize(cell_count + 1, 0);
        self.cells.clear();
        self.unsorted.clear();
        for i in 0..xs.len() {
            if !include(i) {
                continue;
            }
            let (cx, cy) = self.cell_coords(xs[i], ys[i]);
            let cell = cy * self.columns + cx;
            self.cells.push(cell);
            self.unsorted.push(i as u32);
            self.starts[cell as usize + 1] += 1;
        }
        for c in 0..cell_count {
            self.starts[c + 1] += self.starts[c];
        }
        // Stable placement: indices were pushed in ascending order.
        self.cursor.clear();
        self.cursor.extend_from_slice(&self.starts[..cell_count]);
        self.entries.clear();
        self.entries.resize(self.unsorted.len(), 0);
        for (&cell, &index) in self.cells.iter().zip(&self.unsorted) {
            let slot = &mut self.cursor[cell as usize];
            self.entries[*slot as usize] = index;
            *slot += 1;
        }
    }

    /// Visits every indexed organism in the grid cells overlapping the square of half-side
    /// `radius` around `(x, y)`, row by row and in ascending index within a cell. The
    /// caller filters by exact distance.
    pub fn for_each_candidate(&self, x: f32, y: f32, radius: f32, mut visit: impl FnMut(usize)) {
        let (x0, y0) = self.cell_coords(x - radius, y - radius);
        let (x1, y1) = self.cell_coords(x + radius, y + radius);
        for cy in y0..=y1 {
            let row = cy * self.columns;
            let first = self.starts[(row + x0) as usize] as usize;
            let last = self.starts[(row + x1) as usize + 1] as usize;
            for &index in &self.entries[first..last] {
                visit(index as usize);
            }
        }
    }

    /// Storage indices within `radius` (inclusive) of `(x, y)`, in ascending order.
    pub fn within_radius(&self, xs: &[f32], ys: &[f32], x: f32, y: f32, radius: f32) -> Vec<usize> {
        let mut found = Vec::new();
        let r2 = radius * radius;
        self.for_each_candidate(x, y, radius, |i| {
            let (dx, dy) = (xs[i] - x, ys[i] - y);
            if dx * dx + dy * dy <= r2 {
                found.push(i);
            }
        });
        found.sort_unstable();
        found
    }

    /// Number of indexed organisms.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    use super::*;
    use crate::rng::unit_f32;

    /// T-GRID-1: radius queries equal brute force for 1,000 seeded random positions and
    /// 100 queries, including edges, corners, cell boundaries and radii larger than a cell.
    #[test]
    fn t_grid_1_radius_queries_match_brute_force() {
        let (width, height) = (200.0, 150.0);
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut xs: Vec<f32> = (0..1_000).map(|_| unit_f32(&mut rng) * width).collect();
        let mut ys: Vec<f32> = (0..1_000).map(|_| unit_f32(&mut rng) * height).collect();
        // Exact edges, corners and cell boundaries.
        for (x, y) in [
            (0.0, 0.0),
            (width, height),
            (width, 0.0),
            (16.0, 32.0),
            (48.0, 0.0),
        ] {
            xs.push(x);
            ys.push(y);
        }
        let mut grid = SpatialGrid::new(width, height, 16.0);
        grid.rebuild(&xs, &ys, |_| true);
        assert_eq!(grid.len(), xs.len());

        let mut queries: Vec<(f32, f32, f32)> = (0..90)
            .map(|k| {
                (
                    unit_f32(&mut rng) * width,
                    unit_f32(&mut rng) * height,
                    [0.5, 4.0, 16.0, 40.0, 100.0][k % 5],
                )
            })
            .collect();
        queries.extend([
            (0.0, 0.0, 10.0),
            (width, height, 30.0),
            (16.0, 32.0, 16.0),
            (48.0, 0.0, 0.0),
            (100.0, 75.0, 500.0),
            (-5.0, 50.0, 8.0),
            (width + 5.0, 50.0, 8.0),
            (32.0, 32.0, 16.0),
            (0.0, height, 1.0),
            (width * 0.5, 0.0, 17.0),
        ]);
        assert_eq!(queries.len(), 100);
        for (x, y, r) in queries {
            let brute: Vec<usize> = (0..xs.len())
                .filter(|&i| {
                    let (dx, dy) = (xs[i] - x, ys[i] - y);
                    dx * dx + dy * dy <= r * r
                })
                .collect();
            assert_eq!(
                grid.within_radius(&xs, &ys, x, y, r),
                brute,
                "({x}, {y}, {r})"
            );
        }
    }

    #[test]
    fn excluded_organisms_are_not_indexed_and_order_is_ascending_per_cell() {
        let xs = [1.0, 2.0, 3.0, 40.0];
        let ys = [1.0, 1.0, 1.0, 40.0];
        let mut grid = SpatialGrid::new(64.0, 64.0, 16.0);
        grid.rebuild(&xs, &ys, |i| i != 1);
        let mut seen = Vec::new();
        grid.for_each_candidate(2.0, 2.0, 1.0, |i| seen.push(i));
        assert_eq!(seen, [0, 2]);
        assert_eq!(grid.len(), 3);
    }
}
