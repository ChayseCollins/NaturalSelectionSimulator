//! The four random domains (SCOPE 6.10, I-02).
//!
//! All randomness in `sim` comes from these generators. Each is used only for its own
//! purpose, and drawing from one never changes another's sequence (T-RNG-1), because
//! each domain is a separate generator with its own key.
//!
//! Seed derivation (stable; changing it changes every result):
//! - Each domain has a fixed 64-bit constant: its name as 8 ASCII bytes, read little-endian.
//! - The domain's 32-byte ChaCha8 key is four successive SplitMix64 outputs, written
//!   little-endian, starting from state `base_seed XOR domain_constant`.
//! - `base_seed` is the resolved terrain seed for the terrain domain and the run seed for
//!   the initial-population, behavior and mutation domains.
//! - An omitted terrain seed is the first SplitMix64 output from state
//!   `run_seed XOR TERRAIN_SEED_FROM_RUN_SEED`.

use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

const TERRAIN_DOMAIN: u64 = u64::from_le_bytes(*b"terrain_");
const INITIAL_POPULATION_DOMAIN: u64 = u64::from_le_bytes(*b"init_pop");
const BEHAVIOR_DOMAIN: u64 = u64::from_le_bytes(*b"behavior");
const MUTATION_DOMAIN: u64 = u64::from_le_bytes(*b"mutation");
const TERRAIN_SEED_FROM_RUN_SEED: u64 = u64::from_le_bytes(*b"tseed_rs");

/// One generator per random domain. Crate-private: shells never draw random numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RandomDomains {
    /// Terrain generation (M-06). Seeded from the resolved terrain seed.
    pub(crate) terrain: ChaCha8Rng,
    /// Initial organism placement and starting traits (M-03). Seeded from the run seed.
    pub(crate) initial_population: ChaCha8Rng,
    /// Behavior decisions such as wandering (M-09). Seeded from the run seed.
    pub(crate) behavior: ChaCha8Rng,
    /// Inheritance mutation (M-04). Seeded from the run seed.
    pub(crate) mutation: ChaCha8Rng,
}

impl RandomDomains {
    pub(crate) fn new(terrain_seed: u64, run_seed: u64) -> Self {
        Self {
            terrain: domain_rng(terrain_seed, TERRAIN_DOMAIN),
            initial_population: domain_rng(run_seed, INITIAL_POPULATION_DOMAIN),
            behavior: domain_rng(run_seed, BEHAVIOR_DOMAIN),
            mutation: domain_rng(run_seed, MUTATION_DOMAIN),
        }
    }

    /// Stream position (in 32-bit words) of each generator, in the fixed order terrain,
    /// initial population, behavior, mutation. Used by the state hash.
    ///
    /// This is the only generator state the hash needs. Each ChaCha key is reconstructable
    /// from the stored run or terrain seed and the domain's stable constant (`domain_rng`),
    /// and the stream number is always 0, so seed plus word position identifies the
    /// generator state completely. Raw generator memory is never read or serialized.
    pub(crate) fn word_positions(&self) -> [u128; 4] {
        [
            self.terrain.get_word_pos(),
            self.initial_population.get_word_pos(),
            self.behavior.get_word_pos(),
            self.mutation.get_word_pos(),
        ]
    }
}

/// Terrain seed used when the configuration omits one (SCOPE 6.10).
pub(crate) fn derive_terrain_seed(run_seed: u64) -> u64 {
    let mut state = run_seed ^ TERRAIN_SEED_FROM_RUN_SEED;
    splitmix64_next(&mut state)
}

fn domain_rng(base_seed: u64, domain: u64) -> ChaCha8Rng {
    let mut state = base_seed ^ domain;
    let mut key = [0u8; 32];
    for chunk in key.as_chunks_mut::<8>().0 {
        *chunk = splitmix64_next(&mut state).to_le_bytes();
    }
    ChaCha8Rng::from_seed(key)
}

/// SplitMix64 (Steele, Lea and Flood, 2014): advances `state` and returns the next output.
fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform `f32` in `[0, 1)` from exactly one 64-bit draw: the top 24 bits scaled by
/// 2^-24, so every possible value is exactly representable.
pub(crate) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u64() >> 40) as f32 * (1.0 / 16_777_216.0)
}

/// Uniform integer in `[0, n)` from exactly one 64-bit draw, by multiply-and-shift
/// (bias below `n / 2^64`, negligible for any grid). Returns 0 when `n` is 0.
pub(crate) fn below(rng: &mut ChaCha8Rng, n: u64) -> u64 {
    ((u128::from(rng.next_u64()) * u128::from(n)) >> 64) as u64
}

/// Two uniform `f32` values in `[0, 1)` from exactly one 64-bit draw: the top 24 bits of
/// each 32-bit half.
pub(crate) fn two_unit_f32(rng: &mut ChaCha8Rng) -> (f32, f32) {
    let bits = rng.next_u64();
    let scale = 1.0 / 16_777_216.0;
    (
        (bits >> 40) as f32 * scale,
        ((bits >> 8) & 0xFF_FFFF) as f32 * scale,
    )
}

/// Standard normal sample by the Box–Muller transform, from exactly two 64-bit draws, with
/// `ln`, `sqrt` and `cos` from `libm` (SCOPE 6.10). The sine half is discarded so every
/// sample costs the same number of draws.
pub(crate) fn standard_normal(rng: &mut ChaCha8Rng) -> f64 {
    const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
    let u1 = ((rng.next_u64() >> 11) + 1) as f64 * SCALE; // (0, 1], so ln(u1) is finite
    let u2 = (rng.next_u64() >> 11) as f64 * SCALE; // [0, 1)
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(core::f64::consts::TAU * u2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_f32_stays_in_half_open_unit_interval() {
        let mut rng = domain_rng(1, INITIAL_POPULATION_DOMAIN);
        for _ in 0..10_000 {
            let u = unit_f32(&mut rng);
            assert!((0.0..1.0).contains(&u), "{u}");
        }
        assert_eq!(rng.get_word_pos(), 20_000, "one u64 per sample");
    }

    #[test]
    fn standard_normal_has_roughly_unit_moments() {
        let mut rng = domain_rng(1, INITIAL_POPULATION_DOMAIN);
        let n = 20_000;
        let samples: Vec<f64> = (0..n).map(|_| standard_normal(&mut rng)).collect();
        assert_eq!(rng.get_word_pos(), 4 * n as u128, "two u64 per sample");
        assert!(samples.iter().all(|z| z.is_finite()));
        let mean = samples.iter().sum::<f64>() / n as f64;
        let variance = samples.iter().map(|z| (z - mean) * (z - mean)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.03, "{mean}");
        assert!((variance.sqrt() - 1.0).abs() < 0.03, "{variance}");
    }

    fn name(index: usize) -> &'static str {
        ["terrain", "initial_population", "behavior", "mutation"][index]
    }

    fn domain(domains: &mut RandomDomains, index: usize) -> &mut ChaCha8Rng {
        match index {
            0 => &mut domains.terrain,
            1 => &mut domains.initial_population,
            2 => &mut domains.behavior,
            3 => &mut domains.mutation,
            _ => unreachable!("there are exactly four random domains"),
        }
    }

    fn next_values(rng: &mut ChaCha8Rng, count: usize) -> Vec<u64> {
        (0..count).map(|_| rng.next_u64()).collect()
    }

    /// T-RNG-1: drawing extra values from any one domain leaves the next values of the
    /// other three domains unchanged.
    #[test]
    fn t_rng_1_extra_draws_in_one_domain_do_not_affect_the_others() {
        for advanced in 0..4 {
            let mut control = RandomDomains::new(1001, 1);
            let mut treated = control.clone();

            let _ = next_values(domain(&mut treated, advanced), 1_000);

            for other in (0..4).filter(|&d| d != advanced) {
                assert_eq!(
                    next_values(domain(&mut treated, other), 32),
                    next_values(domain(&mut control, other), 32),
                    "drawing from {} changed {}",
                    name(advanced),
                    name(other),
                );
            }
        }
    }

    /// T-RNG-1: each domain's sequence is identical for the same terrain and run seeds.
    #[test]
    fn t_rng_1_identical_seeds_give_identical_sequences() {
        let mut a = RandomDomains::new(1001, 1);
        let mut b = RandomDomains::new(1001, 1);
        for index in 0..4 {
            assert_eq!(
                next_values(domain(&mut a, index), 64),
                next_values(domain(&mut b, index), 64),
                "{}",
                name(index),
            );
        }
    }

    #[test]
    fn domains_produce_distinct_sequences() {
        let mut domains = RandomDomains::new(5, 5);
        let firsts: Vec<Vec<u64>> = (0..4)
            .map(|i| next_values(domain(&mut domains, i), 4))
            .collect();
        for i in 0..4 {
            for j in (i + 1)..4 {
                assert_ne!(firsts[i], firsts[j], "{} vs {}", name(i), name(j));
            }
        }
    }

    #[test]
    fn terrain_domain_depends_only_on_terrain_seed() {
        let mut run_1 = RandomDomains::new(1001, 1);
        let mut run_2 = RandomDomains::new(1001, 2);
        assert_eq!(
            next_values(&mut run_1.terrain, 16),
            next_values(&mut run_2.terrain, 16)
        );
        for index in 1..4 {
            assert_ne!(
                next_values(domain(&mut run_1, index), 16),
                next_values(domain(&mut run_2, index), 16),
                "{}",
                name(index),
            );
        }
    }

    #[test]
    fn word_positions_track_draws() {
        let mut domains = RandomDomains::new(1, 1);
        assert_eq!(domains.word_positions(), [0; 4]);
        domains.behavior.next_u64();
        assert_eq!(domains.word_positions(), [0, 0, 2, 0]);
    }
}
