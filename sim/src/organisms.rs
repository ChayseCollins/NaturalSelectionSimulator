//! Organism storage as structure of arrays (SCOPE 6.2, 7.2).
//!
//! One vector per column, all always the same length. A storage index is only a location
//! and changes when organisms are removed; the stable 64-bit ID is the identity, assigned
//! in creation order and never reused within a run. Removal is swap-remove applied to
//! every column at once, in descending index order (I-04), so the order of survivors is
//! deterministic.
//!
//! Every column is declared once, in the `organism_columns!` invocation below, which
//! generates creation, swap-removal, the alignment check and state hashing. A column
//! added there is therefore always removed, hashed and initialized (the [`OrganismRow`]
//! literal fails to compile until it is given a value).
//!
//! Energy capacity is not stored: it is derived from size (`energy::capacity`).

use crate::hash::Fnv1a64;
use crate::traits::{TraitId, Traits};

/// The two fixed species (SCOPE 7.2: a two-variant enum, behavior chosen with `match`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Species {
    Prey = 0,
    Predator = 1,
}

impl Species {
    pub const ALL: [Species; 2] = [Species::Prey, Species::Predator];

    pub fn name(self) -> &'static str {
        match self {
            Self::Prey => "prey",
            Self::Predator => "predator",
        }
    }

    /// Index for per-species arrays: prey 0, predator 1.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// What an organism is currently doing (SCOPE 6.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Goal {
    /// No target: moving along the stored heading at wandering speed.
    Wander = 0,
    /// Prey heading for, or feeding in, a food cell (`target_cell`).
    Seek = 1,
    /// Predator chasing a prey (`target_id`).
    Hunt = 2,
    /// Prey running from a predator (`target_id`).
    Flee = 3,
}

impl Goal {
    pub fn name(self) -> &'static str {
        match self {
            Self::Wander => "wandering",
            Self::Seek => "seeking food",
            Self::Hunt => "hunting",
            Self::Flee => "fleeing",
        }
    }
}

/// Writes one column into the state hash, element by element.
pub(crate) trait HashColumn {
    fn hash_column(&self, hasher: &mut Fnv1a64);
}

impl HashColumn for Vec<u64> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        hasher.write_u64_slice(self);
    }
}

impl HashColumn for Vec<u32> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        hasher.write_u32_slice(self);
    }
}

impl HashColumn for Vec<f32> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        hasher.write_f32_slice(self);
    }
}

impl HashColumn for Vec<bool> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        for &value in self {
            hasher.write_bytes(&[u8::from(value)]);
        }
    }
}

impl HashColumn for Vec<Species> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        for &value in self {
            hasher.write_bytes(&[value as u8]);
        }
    }
}

impl HashColumn for Vec<Goal> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        for &value in self {
            hasher.write_bytes(&[value as u8]);
        }
    }
}

impl HashColumn for Vec<Vec<u32>> {
    fn hash_column(&self, hasher: &mut Fnv1a64) {
        for route in self {
            hasher.write_u32(route.len() as u32);
            hasher.write_u32_slice(route);
        }
    }
}

macro_rules! organism_columns {
    ($( $(#[$meta:meta])* $name:ident : $ty:ty ),* $(,)?) => {
        /// Every living organism, one vector per column.
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct Organisms {
            $( $(#[$meta])* pub(crate) $name: Vec<$ty>, )*
        }

        /// One organism's values in every column, for appending.
        #[derive(Debug, Clone, PartialEq)]
        pub(crate) struct OrganismRow {
            $( pub(crate) $name: $ty, )*
        }

        impl Organisms {
            pub(crate) fn with_capacity(capacity: usize) -> Self {
                Self { $( $name: Vec::with_capacity(capacity), )* }
            }

            /// Appends one organism to every column.
            pub(crate) fn push_row(&mut self, row: OrganismRow) {
                $( self.$name.push(row.$name); )*
            }

            /// Removes the organism at `index` from every column, moving the last organism
            /// into its slot.
            pub(crate) fn swap_remove(&mut self, index: usize) {
                $( self.$name.swap_remove(index); )*
            }

            /// True when every column has the same length (checked every tick in debug).
            pub(crate) fn columns_aligned(&self) -> bool {
                let n = self.id.len();
                true $( && self.$name.len() == n )*
            }

            /// Hashes every column in declaration order, one column after another.
            pub(crate) fn hash_into(&self, hasher: &mut Fnv1a64) {
                $( HashColumn::hash_column(&self.$name, hasher); )*
            }
        }
    };
}

organism_columns! {
    id: u64,
    species: Species,
    x: f32,
    y: f32,
    energy: f32,
    /// Age in ticks.
    age: u32,
    /// Generation number; the starting population is generation 0.
    generation: u32,
    speed: f32,
    size: f32,
    sense: f32,
    /// Movement direction in radians, counterclockwise from +x. Wanderers move along it;
    /// for other goals it records the last movement direction.
    heading: f32,
    goal: Goal,
    /// Food cell of a `Seek` goal, else [`Organisms::NO_TARGET`].
    target_cell: u32,
    /// Organism ID of a `Hunt` or `Flee` goal, else [`Organisms::NO_ID`].
    target_id: u64,
    /// Waypoint cells of the current route, in order; empty when steering directly.
    route: Vec<u32>,
    /// Index of the next waypoint in `route`.
    route_next: u32,
    /// A route request for this organism is queued.
    route_pending: bool,
    /// Tick the current route was planned.
    planned_tick: u64,
    /// Last tick the distance to the goal improved by the minimum progress.
    progress_tick: u64,
    /// Best distance to the goal since the goal was set or progress last reset.
    best_distance: f32,
    /// Stuck recovery has already re-planned once for the current goal.
    replanned: bool,
    /// Target (cell index for prey, organism ID for predators) marked unreachable.
    avoid_target: u64,
    /// Tick until which `avoid_target` is skipped.
    avoid_until: u64,
    /// Consecutive ticks the organism's step was blocked by terrain while goal-directed.
    blocked_ticks: u32,
}

impl OrganismRow {
    /// A new organism at age 0 with no goal; it decides on its first tick.
    pub(crate) fn new(
        id: u64,
        species: Species,
        position: (f32, f32),
        energy: f32,
        generation: u32,
        traits: Traits,
    ) -> Self {
        Self {
            id,
            species,
            x: position.0,
            y: position.1,
            energy,
            age: 0,
            generation,
            speed: traits.speed,
            size: traits.size,
            sense: traits.sense,
            heading: 0.0,
            goal: Goal::Wander,
            target_cell: Organisms::NO_TARGET,
            target_id: Organisms::NO_ID,
            route: Vec::new(),
            route_next: 0,
            route_pending: false,
            planned_tick: 0,
            progress_tick: 0,
            best_distance: f32::INFINITY,
            replanned: false,
            avoid_target: Organisms::NO_ID,
            avoid_until: 0,
            blocked_ticks: 0,
        }
    }
}

impl Organisms {
    /// Target cell value meaning "no food target".
    pub const NO_TARGET: u32 = u32::MAX;
    /// Target ID value meaning "no organism target".
    pub const NO_ID: u64 = u64::MAX;

    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    /// Storage index of the organism with `id`, found by scanning (SCOPE 7.2: no hash map).
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.id.iter().position(|&candidate| candidate == id)
    }

    pub fn ids(&self) -> &[u64] {
        &self.id
    }

    pub fn species(&self) -> &[Species] {
        &self.species
    }

    pub fn x(&self) -> &[f32] {
        &self.x
    }

    pub fn y(&self) -> &[f32] {
        &self.y
    }

    pub fn energy(&self) -> &[f32] {
        &self.energy
    }

    /// Age in ticks.
    pub fn age(&self) -> &[u32] {
        &self.age
    }

    pub fn generation(&self) -> &[u32] {
        &self.generation
    }

    /// One trait column.
    pub fn trait_values(&self, id: TraitId) -> &[f32] {
        match id {
            TraitId::Speed => &self.speed,
            TraitId::Size => &self.size,
            TraitId::Sense => &self.sense,
        }
    }

    /// Movement headings in radians.
    pub fn headings(&self) -> &[f32] {
        &self.heading
    }

    pub fn goals(&self) -> &[Goal] {
        &self.goal
    }

    /// Food-cell targets; [`Organisms::NO_TARGET`] when not seeking food.
    pub fn target_cells(&self) -> &[u32] {
        &self.target_cell
    }

    /// Organism targets; [`Organisms::NO_ID`] when not hunting or fleeing.
    pub fn target_ids(&self) -> &[u64] {
        &self.target_id
    }

    /// Remaining waypoint cells of the organism at `index`.
    pub fn remaining_route(&self, index: usize) -> &[u32] {
        let route = &self.route[index];
        &route[(self.route_next[index] as usize).min(route.len())..]
    }

    /// Consecutive ticks the organism at `index` has been blocked by terrain.
    pub fn blocked_ticks(&self, index: usize) -> u32 {
        self.blocked_ticks[index]
    }

    /// The three trait values of the organism at `index`.
    pub fn traits(&self, index: usize) -> Traits {
        Traits {
            speed: self.speed[index],
            size: self.size[index],
            sense: self.sense[index],
        }
    }

    /// Number of living organisms of one species.
    pub fn count(&self, species: Species) -> usize {
        self.species.iter().filter(|&&s| s == species).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: u64) -> Organisms {
        let mut organisms = Organisms::with_capacity(n as usize);
        for id in 0..n {
            let v = id as f32;
            let traits = Traits {
                speed: 1.0 + v,
                size: 2.0 + v,
                sense: 3.0 + v,
            };
            let species = if id.is_multiple_of(2) {
                Species::Prey
            } else {
                Species::Predator
            };
            organisms.push_row(OrganismRow::new(
                id,
                species,
                (10.0 + v, 20.0 + v),
                30.0 + v,
                id as u32,
                traits,
            ));
        }
        organisms
    }

    /// Checks that the organism with `id` still has every value it was created with.
    fn assert_row_matches_id(organisms: &Organisms, id: u64) {
        let i = organisms.index_of(id).expect("organism present");
        let v = id as f32;
        assert_eq!(organisms.x()[i], 10.0 + v);
        assert_eq!(organisms.y()[i], 20.0 + v);
        assert_eq!(organisms.energy()[i], 30.0 + v);
        assert_eq!(organisms.generation()[i], id as u32);
        assert_eq!(
            organisms.species()[i] == Species::Prey,
            id.is_multiple_of(2)
        );
        assert_eq!(
            organisms.traits(i),
            Traits {
                speed: 1.0 + v,
                size: 2.0 + v,
                sense: 3.0 + v
            }
        );
    }

    #[test]
    fn push_keeps_columns_aligned() {
        let organisms = sample(5);
        assert!(organisms.columns_aligned());
        assert_eq!(organisms.len(), 5);
        assert_eq!(organisms.age(), [0; 5]);
        assert_eq!(organisms.count(Species::Prey), 3);
        assert_eq!(organisms.count(Species::Predator), 2);
    }

    #[test]
    fn descending_swap_remove_keeps_rows_together() {
        let mut organisms = sample(5);
        organisms.age[4] = 44;
        organisms.heading[4] = 1.25;
        organisms.target_cell[4] = 7;
        organisms.route[4] = vec![3, 4, 5];
        // Remove indices 1 and 3, in descending order as the tick pipeline does.
        organisms.swap_remove(3);
        organisms.swap_remove(1);
        assert!(organisms.columns_aligned());
        assert_eq!(organisms.ids(), [0, 4, 2]);
        for id in [0, 2, 4] {
            assert_row_matches_id(&organisms, id);
        }
        let moved = organisms.index_of(4).expect("present");
        assert_eq!(organisms.age()[moved], 44);
        assert_eq!(organisms.headings()[moved], 1.25);
        assert_eq!(organisms.target_cells()[moved], 7);
        assert_eq!(organisms.remaining_route(moved), [3, 4, 5]);
        assert_eq!(organisms.target_cells()[0], Organisms::NO_TARGET);
        assert_eq!(organisms.index_of(1), None);
        assert_eq!(organisms.index_of(3), None);
    }

    #[test]
    fn trait_columns_match_trait_ids() {
        let organisms = sample(3);
        assert_eq!(organisms.trait_values(TraitId::Speed), [1.0, 2.0, 3.0]);
        assert_eq!(organisms.trait_values(TraitId::Size), [2.0, 3.0, 4.0]);
        assert_eq!(organisms.trait_values(TraitId::Sense), [3.0, 4.0, 5.0]);
    }

    #[test]
    fn every_column_changes_the_hash() {
        let base = sample(3);
        let hash = |o: &Organisms| {
            let mut h = Fnv1a64::new();
            o.hash_into(&mut h);
            h.finish()
        };
        let base_hash = hash(&base);
        type Change = fn(&mut Organisms);
        let changes: [Change; 24] = [
            |o| o.id[1] = 99,
            |o| o.species[1] = Species::Prey,
            |o| o.x[1] += 1.0,
            |o| o.y[1] += 1.0,
            |o| o.energy[1] += 1.0,
            |o| o.age[1] += 1,
            |o| o.generation[1] += 1,
            |o| o.speed[1] += 1.0,
            |o| o.size[1] += 1.0,
            |o| o.sense[1] += 1.0,
            |o| o.heading[1] += 1.0,
            |o| o.goal[1] = Goal::Hunt,
            |o| o.target_cell[1] = 3,
            |o| o.target_id[1] = 3,
            |o| o.route[1].push(3),
            |o| o.route_next[1] = 1,
            |o| o.route_pending[1] = true,
            |o| o.planned_tick[1] = 5,
            |o| o.progress_tick[1] = 5,
            |o| o.best_distance[1] = 5.0,
            |o| o.replanned[1] = true,
            |o| o.avoid_target[1] = 5,
            |o| o.avoid_until[1] = 5,
            |o| o.blocked_ticks[1] = 5,
        ];
        for (k, change) in changes.iter().enumerate() {
            let mut changed = base.clone();
            change(&mut changed);
            assert_ne!(hash(&changed), base_hash, "column {k} is not hashed");
        }
    }
}
