// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The seeded pseudo-random stream every search stage draws from.
//!
//! The generator is xoshiro256** (D. Blackman and S. Vigna, "Scrambled
//! Linear Pseudorandom Number Generators," ACM Transactions on Mathematical
//! Software 47(4), article 36, 2021, DOI 10.1145/3460772), seeded by
//! expanding the 64-bit run seed through SplitMix64 (G. L. Steele, D. Lea
//! and C. H. Flood, "Fast Splittable Pseudorandom Number Generators," OOPSLA
//! 2014, DOI 10.1145/2660193.2660195), which is the seeding the xoshiro
//! authors recommend. Both are transcribed from the authors' public-domain
//! reference C sources; the unit tests pin their published output.
//!
//! The stream is not cryptographic and does not need to be: it only has to
//! be reproducible from the seed on every platform, which integer-only state
//! updates guarantee.

/// xoshiro256** state.
#[derive(Debug, Clone)]
pub(crate) struct SearchRng {
    state: [u64; 4],
}

/// One SplitMix64 step: advances `state` and returns the mixed output.
fn split_mix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

impl SearchRng {
    /// The stream for `seed`. Four consecutive SplitMix64 outputs are never
    /// all zero, so the xoshiro state is always valid.
    pub(crate) fn seed(seed: u64) -> Self {
        let mut expander = seed;
        Self {
            state: std::array::from_fn(|_| split_mix64(&mut expander)),
        }
    }

    /// An independent stream derived from this run's seed for a named
    /// purpose, so one stage's draw count cannot shift another stage's
    /// sample.
    pub(crate) fn stream(seed: u64, stream: u64) -> Self {
        let mut mixed = seed ^ stream.wrapping_mul(0xd1b5_4a32_d192_ed03);
        Self::seed(split_mix64(&mut mixed))
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= shifted;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }

    /// A uniform double in `[0, 1)` from the top 53 bits.
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// A uniform double in `[low, high)`.
    pub(crate) fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// A uniform integer in `0..bound` without modulo bias, by Lemire's
    /// multiply-and-reject method (D. Lemire, "Fast Random Integer Generation
    /// in an Interval," ACM Transactions on Modeling and Computer Simulation
    /// 29(1), article 3, 2019, DOI 10.1145/3230636). `bound == 0` returns 0.
    pub(crate) fn below(&mut self, bound: usize) -> usize {
        let range = bound as u64;
        if range == 0 {
            return 0;
        }
        let threshold = range.wrapping_neg() % range;
        loop {
            let product = u128::from(self.next_u64()) * u128::from(range);
            if (product as u64) >= threshold {
                return (product >> 64) as usize;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_mix64_reproduces_the_reference_sequence() {
        // Output of the reference `splitmix64.c` for seed 1234567.
        let mut state = 1_234_567_u64;
        let outputs: Vec<u64> = (0..5).map(|_| split_mix64(&mut state)).collect();
        assert_eq!(
            outputs,
            [
                6_457_827_717_110_365_317,
                3_203_168_211_198_807_973,
                9_817_491_932_198_370_423,
                4_593_380_528_125_082_431,
                16_408_922_859_458_223_821,
            ]
        );
    }

    #[test]
    fn xoshiro256_star_star_reproduces_the_reference_sequence() {
        // Output of the reference `xoshiro256starstar.c` from state {1, 2, 3, 4}.
        let mut rng = SearchRng {
            state: [1, 2, 3, 4],
        };
        let outputs: Vec<u64> = (0..6).map(|_| rng.next_u64()).collect();
        assert_eq!(
            outputs,
            [
                11_520,
                0,
                1_509_978_240,
                1_215_971_899_390_074_240,
                1_216_172_134_540_287_360,
                607_988_272_756_665_600,
            ]
        );
    }

    #[test]
    fn bounded_draws_cover_the_range_and_never_leave_it() {
        let mut rng = SearchRng::seed(7);
        let mut seen = [0usize; 7];
        for _ in 0..7_000 {
            seen[rng.below(7)] += 1;
        }
        assert!(
            seen.iter().all(|&count| (800..1_200).contains(&count)),
            "{seen:?}"
        );
        assert_eq!(rng.below(0), 0);
        assert!((0..1_000).all(|_| (0.0..1.0).contains(&rng.unit())));
    }

    #[test]
    fn named_streams_are_distinct_and_reproducible() {
        let first = SearchRng::stream(42, 1).next_u64();
        assert_eq!(first, SearchRng::stream(42, 1).next_u64());
        assert_ne!(first, SearchRng::stream(42, 2).next_u64());
        assert_ne!(first, SearchRng::stream(43, 1).next_u64());
    }
}
