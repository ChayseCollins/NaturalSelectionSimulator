//! Procedural terrain (M-06) and the grid geometry it shares with food (SCOPE 6.1, 6.4).
//!
//! The world is covered by square cells of `cell_size` world units, indexed
//! `y * width + x` with y pointing up. When the world size is not a multiple of the cell
//! size, the last row and column are partial cells clipped to the world edge.
//!
//! Each cell has one terrain class from a small constant table: passable or not, a speed
//! multiplier and a food fertility (all P). Generation draws only from the terrain random
//! domain: two value-noise fields (elevation and moisture) of four octaves each. Elevation
//! is converted to a rank between 0 and 1 over the whole map, so `water_level = 0.3`
//! means the lowest 30% of the map is water and `rock_level = 0.85` means the highest
//! 15% is rock, whatever the noise looks like. Sand rims the water and hills ring the
//! rock; moisture splits the remaining land into grassland and forest.
//!
//! Connected regions of passable cells are labelled once by flood fill (SCOPE 6.8 step 1),
//! with 4-neighbor connectivity, matching route search, which never cuts corners.
//!
//! [`Terrain::from_ascii`] builds hand-drawn test maps (SCOPE 15.1).

use rand_chacha::ChaCha8Rng;

use crate::config::{TerrainConfig, WorldConfig};
use crate::hash::Fnv1a64;
use crate::rng;

/// Cell layout shared by the terrain and food grids.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridGeometry {
    width: u32,
    height: u32,
    cell_size: f32,
    world_width: f32,
    world_height: f32,
}

impl GridGeometry {
    pub(crate) fn new(world_width: f32, world_height: f32, cell_size: f32) -> Self {
        Self {
            width: ((world_width / cell_size).ceil() as u32).max(1),
            height: ((world_height / cell_size).ceil() as u32).max(1),
            cell_size,
            world_width,
            world_height,
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
        self.width as usize * self.height as usize
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    pub fn world_width(&self) -> f32 {
        self.world_width
    }

    pub fn world_height(&self) -> f32 {
        self.world_height
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
    /// (`x == width` or `y == height`) map to the last cell, never out of range; negative
    /// positions map to the first.
    pub fn cell_coords_at(&self, x: f32, y: f32) -> (u32, u32) {
        let cx = ((x / self.cell_size).max(0.0) as u32).min(self.width - 1);
        let cy = ((y / self.cell_size).max(0.0) as u32).min(self.height - 1);
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
}

/// One terrain class (SCOPE 9: a small terrain-class lookup table; Q12: named classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TerrainClass {
    Water = 0,
    Sand = 1,
    Grass = 2,
    Forest = 3,
    Hills = 4,
    Rock = 5,
}

impl TerrainClass {
    pub const ALL: [TerrainClass; 6] = [
        TerrainClass::Water,
        TerrainClass::Sand,
        TerrainClass::Grass,
        TerrainClass::Forest,
        TerrainClass::Hills,
        TerrainClass::Rock,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Water => "water",
            Self::Sand => "sand",
            Self::Grass => "grassland",
            Self::Forest => "forest",
            Self::Hills => "hills",
            Self::Rock => "rock",
        }
    }

    /// Whether organisms may enter the class.
    pub fn passable(self) -> bool {
        !matches!(self, Self::Water | Self::Rock)
    }

    /// Multiplier on the distance an organism covers per tick (P). Zero when impassable.
    pub fn speed_multiplier(self) -> f32 {
        match self {
            Self::Water | Self::Rock => 0.0,
            Self::Sand => 0.8,
            Self::Grass => 1.0,
            Self::Forest => 0.6,
            Self::Hills => 0.45,
        }
    }

    /// Food fertility (P): the share of `max_per_cell × abundance` the cell can hold.
    /// Impassable cells never hold food (SCOPE 6.4).
    pub fn fertility(self) -> f32 {
        match self {
            Self::Water | Self::Rock => 0.0,
            Self::Sand => 0.15,
            Self::Grass => 1.0,
            Self::Forest => 0.6,
            Self::Hills => 0.3,
        }
    }

    /// Character used by [`Terrain::from_ascii`] and [`Terrain::to_ascii`].
    pub fn symbol(self) -> char {
        match self {
            Self::Water => '~',
            Self::Sand => ':',
            Self::Grass => '.',
            Self::Forest => 'f',
            Self::Hills => '^',
            Self::Rock => '#',
        }
    }

    pub fn from_symbol(symbol: char) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.symbol() == symbol)
    }
}

/// Region label of an impassable cell.
pub const NO_REGION: u32 = u32::MAX;

/// The static terrain of one run: classes, reachability labels and the passable cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    geometry: GridGeometry,
    classes: Vec<TerrainClass>,
    /// Connected-region label per cell; [`NO_REGION`] for impassable cells.
    labels: Vec<u32>,
    region_count: u32,
    /// Indices of passable cells, ascending (used for spawning).
    passable: Vec<u32>,
    /// FNV-1a of the class grid, included in the state hash.
    checksum: u64,
}

/// Why an ASCII map could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsciiMapError {
    Empty,
    RaggedRow {
        row: usize,
    },
    UnknownSymbol {
        row: usize,
        column: usize,
        symbol: char,
    },
}

impl Terrain {
    fn from_classes(geometry: GridGeometry, classes: Vec<TerrainClass>) -> Self {
        let mut hasher = Fnv1a64::new();
        hasher.write_u32(geometry.width);
        hasher.write_u32(geometry.height);
        for &class in &classes {
            hasher.write_bytes(&[class as u8]);
        }
        let (labels, region_count) = label_regions(&geometry, &classes);
        let passable = (0..classes.len() as u32)
            .filter(|&i| classes[i as usize].passable())
            .collect();
        Self {
            geometry,
            classes,
            labels,
            region_count,
            passable,
            checksum: hasher.finish(),
        }
    }

    /// Open grassland everywhere: no barriers and uniform fertility. Used by tests and
    /// constructed experiments.
    pub fn open(world: &WorldConfig) -> Self {
        let geometry = GridGeometry::new(world.width, world.height, world.cell_size);
        Self::from_classes(geometry, vec![TerrainClass::Grass; geometry.cell_count()])
    }

    /// Builds a map from text, one character per cell (see [`TerrainClass::symbol`]): `.`
    /// grassland, `f` forest, `^` hills, `:` sand, `~` water, `#` rock. The first line is
    /// the top row of the world (largest y). Blank lines and surrounding whitespace on each
    /// line are ignored. The world is `columns × cell_size` by `rows × cell_size`.
    pub fn from_ascii(map: &str, cell_size: f32) -> Result<Self, AsciiMapError> {
        let rows: Vec<&str> = map
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let Some(first) = rows.first() else {
            return Err(AsciiMapError::Empty);
        };
        let width = first.chars().count();
        let height = rows.len();
        let mut classes = vec![TerrainClass::Grass; width * height];
        for (row, line) in rows.iter().enumerate() {
            if line.chars().count() != width {
                return Err(AsciiMapError::RaggedRow { row });
            }
            let y = height - 1 - row;
            for (column, symbol) in line.chars().enumerate() {
                let class =
                    TerrainClass::from_symbol(symbol).ok_or(AsciiMapError::UnknownSymbol {
                        row,
                        column,
                        symbol,
                    })?;
                classes[y * width + column] = class;
            }
        }
        let geometry = GridGeometry::new(
            width as f32 * cell_size,
            height as f32 * cell_size,
            cell_size,
        );
        Ok(Self::from_classes(geometry, classes))
    }

    /// The map as text in the [`Terrain::from_ascii`] format (top row first).
    pub fn to_ascii(&self) -> String {
        let (w, h) = (self.geometry.width, self.geometry.height);
        let mut text = String::with_capacity((w as usize + 1) * h as usize);
        for y in (0..h).rev() {
            for x in 0..w {
                text.push(self.classes[self.geometry.index(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    /// Generates terrain for `world` from the terrain random domain (M-06). The same
    /// terrain seed and settings always give the same grid (T-TF-1).
    pub(crate) fn generate(
        world: &WorldConfig,
        settings: &TerrainConfig,
        rng: &mut ChaCha8Rng,
    ) -> Self {
        let geometry = GridGeometry::new(world.width, world.height, world.cell_size);
        let elevation = fractal_noise(&geometry, settings.feature_scale, rng);
        let moisture = fractal_noise(&geometry, settings.feature_scale * 0.75, rng);
        let ranks = ranks(&elevation);
        let (moisture_low, moisture_high) = min_max(&moisture);
        let moisture_span = (moisture_high - moisture_low).max(f32::EPSILON);

        let water = settings.water_level;
        let rock = settings.rock_level;
        let sand_top = (water + SAND_BAND).min(rock);
        let hills_bottom = (rock - HILLS_BAND).max(sand_top);
        let classes = ranks
            .iter()
            .zip(&moisture)
            .map(|(&rank, &wetness)| {
                if rank < water {
                    TerrainClass::Water
                } else if rank >= rock {
                    TerrainClass::Rock
                } else if rank < sand_top {
                    TerrainClass::Sand
                } else if rank >= hills_bottom {
                    TerrainClass::Hills
                } else if (wetness - moisture_low) / moisture_span > FOREST_MOISTURE {
                    TerrainClass::Forest
                } else {
                    TerrainClass::Grass
                }
            })
            .collect();
        Self::from_classes(geometry, classes)
    }

    pub fn geometry(&self) -> &GridGeometry {
        &self.geometry
    }

    /// Class of every cell, indexed `y * width + x`.
    pub fn classes(&self) -> &[TerrainClass] {
        &self.classes
    }

    pub fn class(&self, index: usize) -> TerrainClass {
        self.classes[index]
    }

    pub fn class_at(&self, x: f32, y: f32) -> TerrainClass {
        self.classes[self.geometry.cell_at(x, y)]
    }

    pub fn passable_cell(&self, index: usize) -> bool {
        self.classes[index].passable()
    }

    /// Whether a world position is inside the world and on a passable cell.
    pub fn passable_at(&self, x: f32, y: f32) -> bool {
        (0.0..=self.geometry.world_width).contains(&x)
            && (0.0..=self.geometry.world_height).contains(&y)
            && self.class_at(x, y).passable()
    }

    /// Speed multiplier of the cell under a position.
    pub fn speed_multiplier_at(&self, x: f32, y: f32) -> f32 {
        self.class_at(x, y).speed_multiplier()
    }

    /// Connected-region label of a cell ([`NO_REGION`] when impassable).
    pub fn region(&self, index: usize) -> u32 {
        self.labels[index]
    }

    pub fn region_at(&self, x: f32, y: f32) -> u32 {
        self.labels[self.geometry.cell_at(x, y)]
    }

    pub fn region_count(&self) -> u32 {
        self.region_count
    }

    /// Passable cell indices in ascending order.
    pub fn passable_cells(&self) -> &[u32] {
        &self.passable
    }

    /// Share of cells that are passable.
    pub fn passable_share(&self) -> f32 {
        self.passable.len() as f32 / self.classes.len().max(1) as f32
    }

    pub(crate) fn checksum(&self) -> u64 {
        self.checksum
    }

    /// Blocked-path check (SCOPE 6.8 step 2): true when every cell the straight segment
    /// from `(x0, y0)` to `(x1, y1)` passes through is passable. Walks the grid cell by
    /// cell (Amanatides–Woo); a segment through a cell corner visits both side cells, so
    /// diagonal squeezes between two blocked cells count as blocked. Cost is proportional
    /// to the segment's length in cells.
    pub fn segment_clear(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
        self.walk_segment(x0, y0, x1, y1, |cell| self.classes[cell].passable())
    }

    /// Lowest speed multiplier along a clear segment, or `None` when it is blocked.
    pub fn segment_min_multiplier(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<f32> {
        let mut lowest = f32::INFINITY;
        let clear = self.walk_segment(x0, y0, x1, y1, |cell| {
            let class = self.classes[cell];
            lowest = lowest.min(class.speed_multiplier());
            class.passable()
        });
        clear.then_some(lowest)
    }

    /// Visits every cell the segment passes through, in order, stopping with `false` as
    /// soon as `visit` returns false.
    fn walk_segment(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        mut visit: impl FnMut(usize) -> bool,
    ) -> bool {
        let g = &self.geometry;
        let (mut cx, mut cy) = g.cell_coords_at(x0, y0);
        let (end_x, end_y) = g.cell_coords_at(x1, y1);
        if !visit(g.index(cx, cy)) {
            return false;
        }
        let (dx, dy) = (x1 - x0, y1 - y0);
        let step_x: i64 = if dx > 0.0 { 1 } else { -1 };
        let step_y: i64 = if dy > 0.0 { 1 } else { -1 };
        let size = g.cell_size;
        let boundary = |c: u32, step: i64| {
            if step > 0 {
                (c + 1) as f32 * size
            } else {
                c as f32 * size
            }
        };
        let mut t_max_x = if dx == 0.0 {
            f32::INFINITY
        } else {
            (boundary(cx, step_x) - x0) / dx
        };
        let mut t_max_y = if dy == 0.0 {
            f32::INFINITY
        } else {
            (boundary(cy, step_y) - y0) / dy
        };
        let t_delta_x = if dx == 0.0 {
            f32::INFINITY
        } else {
            size / dx.abs()
        };
        let t_delta_y = if dy == 0.0 {
            f32::INFINITY
        } else {
            size / dy.abs()
        };
        let clamp_x = |c: i64| c.clamp(0, i64::from(g.width) - 1) as u32;
        let clamp_y = |c: i64| c.clamp(0, i64::from(g.height) - 1) as u32;
        // At most the Manhattan cell distance plus slack for rounding at boundaries.
        let max_steps = (end_x.abs_diff(cx) + end_y.abs_diff(cy)) as usize + 2;
        for _ in 0..max_steps {
            if (cx, cy) == (end_x, end_y) {
                return true;
            }
            if t_max_x < t_max_y {
                cx = clamp_x(i64::from(cx) + step_x);
                t_max_x += t_delta_x;
            } else if t_max_y < t_max_x {
                cy = clamp_y(i64::from(cy) + step_y);
                t_max_y += t_delta_y;
            } else {
                // Exactly through a corner: both side cells must be acceptable.
                let nx = clamp_x(i64::from(cx) + step_x);
                let ny = clamp_y(i64::from(cy) + step_y);
                if !visit(g.index(nx, cy)) || !visit(g.index(cx, ny)) {
                    return false;
                }
                cx = nx;
                cy = ny;
                t_max_x += t_delta_x;
                t_max_y += t_delta_y;
            }
            if !visit(g.index(cx, cy)) {
                return false;
            }
        }
        (cx, cy) == (end_x, end_y)
    }
}

/// Elevation-rank width of the sand band above the water line (P).
const SAND_BAND: f32 = 0.04;
/// Elevation-rank width of the hills band below the rock line (P).
const HILLS_BAND: f32 = 0.12;
/// Normalized moisture above which land is forest (P).
const FOREST_MOISTURE: f32 = 0.6;
const OCTAVES: u32 = 4;

/// Fractal value noise at every cell center: four octaves, each a lattice of uniform
/// values from the terrain domain with smoothstep interpolation, frequency doubling and
/// amplitude halving per octave. Draws exactly one value per lattice point, in octave
/// then row then column order.
fn fractal_noise(geometry: &GridGeometry, feature_scale: f32, rng: &mut ChaCha8Rng) -> Vec<f32> {
    let mut field = vec![0.0_f32; geometry.cell_count()];
    let mut spacing = feature_scale;
    let mut amplitude = 1.0_f32;
    for _ in 0..OCTAVES {
        let lattice_w = (geometry.world_width / spacing).ceil() as usize + 2;
        let lattice_h = (geometry.world_height / spacing).ceil() as usize + 2;
        let lattice: Vec<f32> = (0..lattice_w * lattice_h)
            .map(|_| rng::unit_f32(rng))
            .collect();
        for cy in 0..geometry.height {
            for cx in 0..geometry.width {
                let index = geometry.index(cx, cy);
                let (x, y) = geometry.cell_center(index);
                let (fx, fy) = (x / spacing, y / spacing);
                let (ix, iy) = (fx as usize, fy as usize);
                let (tx, ty) = (smoothstep(fx - ix as f32), smoothstep(fy - iy as f32));
                let at = |gx: usize, gy: usize| {
                    lattice[gy.min(lattice_h - 1) * lattice_w + gx.min(lattice_w - 1)]
                };
                let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * tx;
                let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * tx;
                field[index] += amplitude * (top + (bottom - top) * ty);
            }
        }
        spacing = (spacing * 0.5).max(1.0);
        amplitude *= 0.5;
    }
    field
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn min_max(values: &[f32]) -> (f32, f32) {
    values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        })
}

/// Rank of each value in `[0, 1)`: its position in ascending order (ties broken by
/// index) divided by the count. Deterministic: a total order on finite values.
fn ranks(values: &[f32]) -> Vec<f32> {
    let mut order: Vec<u32> = (0..values.len() as u32).collect();
    order.sort_unstable_by(|&a, &b| {
        values[a as usize]
            .total_cmp(&values[b as usize])
            .then(a.cmp(&b))
    });
    let count = values.len().max(1) as f32;
    let mut ranks = vec![0.0; values.len()];
    for (position, &index) in order.iter().enumerate() {
        ranks[index as usize] = position as f32 / count;
    }
    ranks
}

/// Flood-fills connected passable regions with 4-neighbor connectivity, labelling regions
/// in ascending order of their lowest cell index.
fn label_regions(geometry: &GridGeometry, classes: &[TerrainClass]) -> (Vec<u32>, u32) {
    let mut labels = vec![NO_REGION; classes.len()];
    let mut next = 0;
    let mut stack = Vec::new();
    for start in 0..classes.len() {
        if labels[start] != NO_REGION || !classes[start].passable() {
            continue;
        }
        labels[start] = next;
        stack.push(start);
        while let Some(index) = stack.pop() {
            let (cx, cy) = geometry.coords(index);
            let mut visit = |nx: u32, ny: u32| {
                let n = geometry.index(nx, ny);
                if labels[n] == NO_REGION && classes[n].passable() {
                    labels[n] = next;
                    stack.push(n);
                }
            };
            if cx > 0 {
                visit(cx - 1, cy);
            }
            if cx + 1 < geometry.width {
                visit(cx + 1, cy);
            }
            if cy > 0 {
                visit(cx, cy - 1);
            }
            if cy + 1 < geometry.height {
                visit(cx, cy + 1);
            }
        }
        next += 1;
    }
    (labels, next)
}

#[cfg(test)]
mod tests {
    use rand_core::SeedableRng;

    use super::*;

    fn world(size: f32) -> WorldConfig {
        WorldConfig {
            width: size,
            height: size,
            ..WorldConfig::default()
        }
    }

    fn generate(seed: u64, settings: &TerrainConfig) -> Terrain {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        Terrain::generate(&world(256.0), settings, &mut rng)
    }

    /// T-TF-1: the same terrain seed gives the same terrain grid; another seed differs.
    #[test]
    fn t_tf_1_same_seed_same_terrain() {
        let settings = TerrainConfig::default();
        assert_eq!(generate(5, &settings), generate(5, &settings));
        assert_ne!(
            generate(5, &settings).classes(),
            generate(6, &settings).classes()
        );
    }

    #[test]
    fn levels_set_the_share_of_water_and_rock() {
        let settings = TerrainConfig {
            water_level: 0.3,
            rock_level: 0.85,
            ..TerrainConfig::default()
        };
        let terrain = generate(1, &settings);
        let share = |class: TerrainClass| {
            terrain.classes().iter().filter(|&&c| c == class).count() as f32
                / terrain.classes().len() as f32
        };
        assert!((share(TerrainClass::Water) - 0.3).abs() < 0.01);
        assert!((share(TerrainClass::Rock) - 0.15).abs() < 0.01);
        for class in TerrainClass::ALL {
            assert!(share(class) > 0.0, "{} present", class.name());
        }
        assert!((terrain.passable_share() - 0.55).abs() < 0.02);
    }

    #[test]
    fn ascii_maps_round_trip_and_put_the_first_line_on_top() {
        let map = "##.\n~f:\n.^.\n";
        let terrain = Terrain::from_ascii(map, 2.0).expect("valid map");
        assert_eq!(terrain.to_ascii(), map);
        assert_eq!(terrain.geometry().world_width(), 6.0);
        assert_eq!(terrain.class_at(1.0, 5.0), TerrainClass::Rock, "top-left");
        assert_eq!(
            terrain.class_at(1.0, 1.0),
            TerrainClass::Grass,
            "bottom-left"
        );
        assert_eq!(terrain.class_at(3.0, 1.0), TerrainClass::Hills);
        assert_eq!(
            Terrain::from_ascii("..\n.\n", 2.0),
            Err(AsciiMapError::RaggedRow { row: 1 })
        );
        assert!(matches!(
            Terrain::from_ascii("x", 2.0),
            Err(AsciiMapError::UnknownSymbol { symbol: 'x', .. })
        ));
        assert_eq!(Terrain::from_ascii("\n\n", 2.0), Err(AsciiMapError::Empty));
    }

    #[test]
    fn regions_are_labelled_by_four_neighbor_flood_fill() {
        let terrain = Terrain::from_ascii(
            "..#..\n\
             ..#..\n\
             ##.##\n\
             ..#..\n",
            1.0,
        )
        .expect("valid map");
        // Four corner rooms and the isolated center cell: diagonal contact does not join.
        assert_eq!(terrain.region_count(), 5);
        let at = |x: f32, y: f32| terrain.region_at(x, y);
        assert_eq!(at(0.5, 3.5), at(1.5, 2.5));
        assert_ne!(at(0.5, 3.5), at(3.5, 3.5));
        assert_ne!(at(2.5, 1.5), at(1.5, 0.5));
        assert_eq!(at(2.5, 3.5), NO_REGION);
    }

    #[test]
    fn segment_check_detects_walls_and_corner_squeezes() {
        let terrain = Terrain::from_ascii(
            ".....\n\
             ..#..\n\
             ..#..\n\
             .#...\n",
            1.0,
        )
        .expect("valid map");
        assert!(terrain.segment_clear(0.5, 3.5, 4.5, 3.5), "open top row");
        assert!(
            !terrain.segment_clear(0.5, 2.5, 4.5, 2.5),
            "through the wall"
        );
        assert!(terrain.segment_clear(3.5, 0.5, 3.5, 3.5), "open column");
        assert!(terrain.segment_clear(1.5, 2.5, 1.5, 2.5), "zero length");
        // The diagonal from (0.5, 0.5) to (2.5, 2.5) passes exactly through the corner at
        // (1, 1), whose side cell (1, 0) is rock: a corner squeeze counts as blocked.
        assert!(!terrain.segment_clear(0.5, 0.5, 2.5, 2.5));
        assert!(
            !terrain.segment_clear(2.5, 1.5, 2.5, 1.5),
            "start inside rock"
        );
    }

    #[test]
    fn class_table_is_consistent() {
        for class in TerrainClass::ALL {
            assert_eq!(class.passable(), class.speed_multiplier() > 0.0);
            if !class.passable() {
                assert_eq!(class.fertility(), 0.0, "impassable cells hold no food");
            }
            assert!((0.0..=1.0).contains(&class.fertility()));
            assert_eq!(TerrainClass::from_symbol(class.symbol()), Some(class));
        }
    }
}
