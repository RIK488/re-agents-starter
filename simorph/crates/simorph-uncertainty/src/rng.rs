//! Deterministic random number generation.

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Seeded RNG type used throughout SIMORPH.
pub type Rng = ChaCha20Rng;

/// Create a reproducible RNG from a 64-bit seed.
pub fn seeded(seed: u64) -> Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

/// Derive an independent child RNG for sub-task `index` (stream splitting).
pub fn child(seed: u64, index: u64) -> Rng {
    // Mix with a large odd constant so that neighbouring (seed, index) pairs
    // produce unrelated streams.
    seeded(seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng as _;

    #[test]
    fn same_seed_same_stream() {
        let mut a = seeded(42);
        let mut b = seeded(42);
        let xa: Vec<f64> = (0..10).map(|_| a.gen()).collect();
        let xb: Vec<f64> = (0..10).map(|_| b.gen()).collect();
        assert_eq!(xa, xb);
        let mut c = child(42, 1);
        let xc: f64 = c.gen();
        assert_ne!(xa[0], xc);
    }
}
