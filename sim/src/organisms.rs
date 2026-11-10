//! Organism storage as structure of arrays (SCOPE 6.2, 7.2).
//!
//! One vector per column, all always the same length. A storage index is only a location
//! and changes when organisms are removed; the stable 64-bit ID is the identity, assigned
//! in creation order and never reused within a run. Removal is swap-remove applied to
//! every column at once, in descending index order (I-04), so the order of survivors is
//! deterministic. M-03 stores prey only; M-05 adds a species column.
//!
//! Energy capacity is not stored: it is derived from size (`energy::capacity`). Movement
//! effort is not stored either: it is recomputed every tick from the distance moved.

use crate::traits::{TraitId, Traits};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Organisms {
    pub(crate) id: Vec<u64>,
    pub(crate) x: Vec<f32>,
    pub(crate) y: Vec<f32>,
    pub(crate) energy: Vec<f32>,
    /// Age in ticks.
    pub(crate) age: Vec<u32>,
    /// Generation number; the starting population is generation 0.
    pub(crate) generation: Vec<u32>,
    pub(crate) speed: Vec<f32>,
    pub(crate) size: Vec<f32>,
    pub(crate) sense: Vec<f32>,
    /// Wandering direction in radians, counterclockwise from +x; used only while the
    /// organism has no food target.
    pub(crate) heading: Vec<f32>,
    /// Food cell the organism is heading for, or [`Organisms::NO_TARGET`].
    pub(crate) target: Vec<u32>,
}

impl Organisms {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            id: Vec::with_capacity(capacity),
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            energy: Vec::with_capacity(capacity),
            age: Vec::with_capacity(capacity),
            generation: Vec::with_capacity(capacity),
            speed: Vec::with_capacity(capacity),
            size: Vec::with_capacity(capacity),
            sense: Vec::with_capacity(capacity),
            heading: Vec::with_capacity(capacity),
            target: Vec::with_capacity(capacity),
        }
    }

    /// Target value meaning "no food target": the organism wanders.
    pub const NO_TARGET: u32 = u32::MAX;

    /// Appends one organism at age 0, heading 0 and with no target; it chooses a goal on
    /// its first tick.
    pub(crate) fn push(
        &mut self,
        id: u64,
        position: (f32, f32),
        energy: f32,
        generation: u32,
        traits: Traits,
    ) {
        self.id.push(id);
        self.x.push(position.0);
        self.y.push(position.1);
        self.energy.push(energy);
        self.age.push(0);
        self.generation.push(generation);
        self.speed.push(traits.speed);
        self.size.push(traits.size);
        self.sense.push(traits.sense);
        self.heading.push(0.0);
        self.target.push(Self::NO_TARGET);
    }

    /// Removes the organism at `index` from every column, moving the last organism into
    /// its slot.
    pub(crate) fn swap_remove(&mut self, index: usize) {
        self.id.swap_remove(index);
        self.x.swap_remove(index);
        self.y.swap_remove(index);
        self.energy.swap_remove(index);
        self.age.swap_remove(index);
        self.generation.swap_remove(index);
        self.speed.swap_remove(index);
        self.size.swap_remove(index);
        self.sense.swap_remove(index);
        self.heading.swap_remove(index);
        self.target.swap_remove(index);
    }

    /// True when every column has the same length (cheap; checked every tick in debug).
    pub(crate) fn columns_aligned(&self) -> bool {
        let n = self.id.len();
        [
            self.x.len(),
            self.y.len(),
            self.energy.len(),
            self.age.len(),
            self.generation.len(),
            self.speed.len(),
            self.size.len(),
            self.sense.len(),
            self.heading.len(),
            self.target.len(),
        ]
        .iter()
        .all(|&len| len == n)
    }

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

    /// Wandering headings in radians.
    pub fn headings(&self) -> &[f32] {
        &self.heading
    }

    /// Food-cell targets; [`Organisms::NO_TARGET`] where the organism is wandering.
    pub fn targets(&self) -> &[u32] {
        &self.target
    }

    /// The three trait values of the organism at `index`.
    pub fn traits(&self, index: usize) -> Traits {
        Traits {
            speed: self.speed[index],
            size: self.size[index],
            sense: self.sense[index],
        }
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
            organisms.push(id, (10.0 + v, 20.0 + v), 30.0 + v, id as u32, traits);
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
    }

    #[test]
    fn descending_swap_remove_keeps_rows_together() {
        let mut organisms = sample(5);
        organisms.age[4] = 44;
        organisms.heading[4] = 1.25;
        organisms.target[4] = 7;
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
        assert_eq!(organisms.targets()[moved], 7);
        assert_eq!(organisms.targets()[0], Organisms::NO_TARGET);
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
}
