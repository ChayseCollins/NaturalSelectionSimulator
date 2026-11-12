//! Asexual reproduction and inheritance with mutation (M-04, SCOPE 6.6).
//!
//! All of reproduction lives here, behind [`inherit`], so another reproduction system
//! could be added without touching movement or energy code (SCOPE 9).
//!
//! Randomness: every birth draws from the mutation domain only, always the same number of
//! values whatever the parameters: for each trait in [`TraitId::ALL`] order, one value
//! decides whether it mutates and two give the normal sample (three 64-bit draws per
//! trait); then two values place the offspring. Eleven 64-bit draws per birth in all.
//! Offspring placement is part of the reproduction event, so it shares the mutation
//! (inheritance) domain rather than adding a fifth domain (decision 2026-11-12).

use rand_chacha::ChaCha8Rng;

use crate::config::{MutationConfig, ReproductionConfig};
use crate::rng;
use crate::terrain::Terrain;
use crate::traits::{TraitId, Traits};

/// Offspring traits: per trait, with probability `p` the parent's value is multiplied by
/// `1 + N(0, σ)`, then clamped to the trait's range (T-INH-1, T-INH-2, T-INH-4). With
/// `p = 0` the child equals the parent exactly. Never NaN or negative.
pub fn inherit(parent: Traits, mutation: &MutationConfig, rng: &mut ChaCha8Rng) -> Traits {
    let mut child = parent;
    for id in TraitId::ALL {
        let roll = rng::unit_f32(rng);
        let normal = rng::standard_normal(rng);
        if roll < mutation.probability {
            let factor = 1.0 + f64::from(mutation.magnitude) * normal;
            let range = id.range();
            let value = (f64::from(parent.get(id)) * factor) as f32;
            let value = if value.is_finite() { value } else { range.min };
            child.set(id, value.clamp(range.min, range.max));
        }
    }
    child
}

/// Offspring position: a uniform point in the disk of `spawn_radius` around the parent
/// (two draws), clamped to the world. If that point is not passable, the offspring is
/// placed at the parent's own position, which is always passable.
pub(crate) fn offspring_position(
    parent: (f32, f32),
    settings: &ReproductionConfig,
    terrain: &Terrain,
    rng: &mut ChaCha8Rng,
) -> (f32, f32) {
    let angle = rng::unit_f32(rng) * core::f32::consts::TAU;
    let distance = settings.spawn_radius * libm::sqrtf(rng::unit_f32(rng));
    let g = terrain.geometry();
    let x = (parent.0 + distance * libm::cosf(angle)).clamp(0.0, g.world_width());
    let y = (parent.1 + distance * libm::sinf(angle)).clamp(0.0, g.world_height());
    if terrain.passable_at(x, y) {
        (x, y)
    } else {
        parent
    }
}

/// Energy a parent pays for one offspring: the offspring's starting energy plus the
/// overhead (T-REP-2).
pub fn birth_cost(offspring_energy: f32, settings: &ReproductionConfig) -> f32 {
    offspring_energy + settings.overhead
}

#[cfg(test)]
mod tests {
    use rand_core::SeedableRng;

    use super::*;
    use crate::config::WorldConfig;

    fn parent() -> Traits {
        Traits {
            speed: 1.0,
            size: 1.5,
            sense: 12.0,
        }
    }

    /// T-INH-1: mutation probability 0 gives child traits equal to the parent's, exactly.
    #[test]
    fn t_inh_1_no_mutation_copies_traits_exactly() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mutation = MutationConfig {
            probability: 0.0,
            magnitude: 0.5,
        };
        for _ in 0..1_000 {
            assert_eq!(inherit(parent(), &mutation, &mut rng), parent());
        }
    }

    /// T-INH-2: 10,000 seeded mutations with p = 1 and σ = 0.05: mean relative change
    /// within ±0.005, standard deviation within 10% of σ.
    #[test]
    fn t_inh_2_mutation_has_the_configured_distribution() {
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mutation = MutationConfig {
            probability: 1.0,
            magnitude: 0.05,
        };
        let n = 10_000;
        let changes: Vec<f64> = (0..n)
            .map(|_| {
                let child = inherit(parent(), &mutation, &mut rng);
                f64::from(child.speed) / f64::from(parent().speed) - 1.0
            })
            .collect();
        let mean = changes.iter().sum::<f64>() / n as f64;
        let variance = changes.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() <= 0.005, "mean relative change {mean}");
        assert!(
            (variance.sqrt() - 0.05).abs() <= 0.005,
            "sd {}",
            variance.sqrt()
        );
    }

    /// T-INH-3 (should): with p = 0.3 over 10,000 trials, the mutated fraction is within
    /// 0.3 ± 0.015.
    #[test]
    fn t_inh_3_mutation_probability_is_respected() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mutation = MutationConfig {
            probability: 0.3,
            magnitude: 0.05,
        };
        let mutated = (0..10_000)
            .filter(|_| inherit(parent(), &mutation, &mut rng).speed != parent().speed)
            .count();
        let fraction = mutated as f64 / 10_000.0;
        assert!((fraction - 0.3).abs() <= 0.015, "{fraction}");
    }

    /// T-INH-4: results are clamped to trait ranges and never NaN or negative, even with
    /// the largest magnitude at the range edges.
    #[test]
    fn t_inh_4_results_are_clamped_and_finite() {
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let mutation = MutationConfig {
            probability: 1.0,
            magnitude: 1.0,
        };
        let extremes = [
            Traits {
                speed: TraitId::Speed.range().max,
                size: TraitId::Size.range().min,
                sense: TraitId::Sense.range().max,
            },
            Traits {
                speed: TraitId::Speed.range().min,
                size: TraitId::Size.range().max,
                sense: TraitId::Sense.range().min,
            },
        ];
        let mut hit_min = false;
        let mut hit_max = false;
        for start in extremes {
            for _ in 0..5_000 {
                let child = inherit(start, &mutation, &mut rng);
                for id in TraitId::ALL {
                    let value = child.get(id);
                    assert!(value.is_finite() && value > 0.0, "{value}");
                    assert!(id.range().contains(value), "{} = {value}", id.name());
                    hit_min |= value == id.range().min;
                    hit_max |= value == id.range().max;
                }
            }
        }
        assert!(hit_min && hit_max, "clamping happened at both ends");
    }

    #[test]
    fn every_birth_draws_the_same_number_of_values() {
        let terrain = Terrain::open(&WorldConfig::default());
        for probability in [0.0, 0.5, 1.0] {
            let mut rng = ChaCha8Rng::seed_from_u64(5);
            let mutation = MutationConfig {
                probability,
                magnitude: 0.1,
            };
            inherit(parent(), &mutation, &mut rng);
            offspring_position(
                (10.0, 10.0),
                &ReproductionConfig::default(),
                &terrain,
                &mut rng,
            );
            assert_eq!(rng.get_word_pos(), 22, "eleven 64-bit draws");
        }
    }

    #[test]
    fn offspring_land_within_the_spawn_radius_on_passable_cells() {
        let terrain = Terrain::from_ascii("###\n#.#\n###\n", 2.0).expect("valid map");
        let settings = ReproductionConfig {
            spawn_radius: 3.0,
            ..ReproductionConfig::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(6);
        let mut moved = 0;
        for _ in 0..200 {
            let (x, y) = offspring_position((3.0, 3.0), &settings, &terrain, &mut rng);
            assert!(terrain.passable_at(x, y));
            assert!(((x - 3.0).powi(2) + (y - 3.0).powi(2)).sqrt() <= 3.0 + 1e-5);
            moved += usize::from((x, y) != (3.0, 3.0));
        }
        assert!(moved > 0, "some offspring land away from the parent");
    }
}
