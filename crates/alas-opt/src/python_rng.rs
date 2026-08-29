// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from NumPy 2.5.1 `numpy.random.bit_generator` and `pcg64`.
// Upstream: NumPy 2.5.1, BSD-3-Clause and MIT.
// Reference: alas @ rust-port-baseline.

//! The two NumPy random streams used by the reference optimizer.
//!
//! The initial cluster is drawn by `numpy.random.default_rng`, whose seeded
//! bit generator is PCG64. SciPy's differential-evolution solver receives the
//! same integer seed separately and constructs a legacy RandomState, whose
//! stream is MT19937. Keeping these streams distinct is part of seeded parity.

const PCG64_MULTIPLIER: u128 = 0x2360_ed05_1fc6_5da4_4385_df64_9fcc_f645;

const INIT_A: u32 = 0x43b0_d7e5;
const MULT_A: u32 = 0x931e_8875;
const INIT_B: u32 = 0x8b51_f9dd;
const MULT_B: u32 = 0x58f3_8ded;
const MIX_MULT_L: u32 = 0xca01_f9dd;
const MIX_MULT_R: u32 = 0x4973_f715;

/// NumPy's `default_rng(seed)` PCG64 stream.
#[derive(Debug, Clone)]
pub(crate) struct Pcg64 {
    state: u128,
    increment: u128,
}

impl Pcg64 {
    /// Construct the stream seeded by a non-negative Python integer.
    pub(crate) fn seed(seed: u64) -> Self {
        let seed_words = seed_sequence_state(seed);
        let init_state = (u128::from(seed_words[0]) << 64) | u128::from(seed_words[1]);
        let init_sequence = (u128::from(seed_words[2]) << 64) | u128::from(seed_words[3]);
        let increment = init_sequence.wrapping_shl(1) | 1;
        let state = increment
            .wrapping_add(init_state)
            .wrapping_mul(PCG64_MULTIPLIER)
            .wrapping_add(increment);

        Self { state, increment }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(PCG64_MULTIPLIER)
            .wrapping_add(self.increment);
        let state_high = (self.state >> 64) as u64;
        let state_low = self.state as u64;
        (state_high ^ state_low).rotate_right((self.state >> 122) as u32)
    }

    /// Draw a half-open uniform value in `[low, high)`.
    pub(crate) fn uniform(&mut self, low: f64, high: f64) -> f64 {
        let fraction = (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
        low + (high - low) * fraction
    }
}

/// SciPy's integer-seeded `numpy.random.RandomState` MT19937 stream.
#[derive(Debug, Clone)]
pub(crate) struct RandomState {
    state: [u32; 624],
    position: usize,
}

impl RandomState {
    /// Construct the legacy stream seeded by a 32-bit Python integer.
    pub(crate) fn seed(seed: u64) -> Self {
        let mut state = [0_u32; 624];
        state[0] = seed as u32;
        for index in 1..state.len() {
            state[index] = 1_812_433_253_u32
                .wrapping_mul(state[index - 1] ^ (state[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        let position = state.len();
        Self { state, position }
    }

    fn next_u32(&mut self) -> u32 {
        if self.position >= self.state.len() {
            for index in 0..self.state.len() {
                let value = (self.state[index] & 0x8000_0000)
                    | (self.state[(index + 1) % self.state.len()] & 0x7fff_ffff);
                self.state[index] = self.state[(index + 397) % self.state.len()]
                    ^ (value >> 1)
                    ^ if value & 1 != 0 { 0x9908_b0df } else { 0 };
            }
            self.position = 0;
        }

        let mut value = self.state[self.position];
        self.position += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    /// Draw a half-open uniform value in `[low, high)`.
    pub(crate) fn uniform(&mut self, low: f64, high: f64) -> f64 {
        let high_word = u64::from(self.next_u32() >> 5);
        let low_word = u64::from(self.next_u32() >> 6);
        let fraction = ((high_word << 26) + low_word) as f64 * (1.0 / 9_007_199_254_740_992.0);
        low + (high - low) * fraction
    }

    /// Draw a legacy masked-rejection integer in `[0, high)`.
    pub(crate) fn randint(&mut self, high: usize) -> usize {
        let maximum = (high - 1) as u32;
        let mut mask = maximum;
        mask |= mask >> 1;
        mask |= mask >> 2;
        mask |= mask >> 4;
        mask |= mask >> 8;
        mask |= mask >> 16;
        loop {
            let value = self.next_u32() & mask;
            if value <= maximum {
                return value as usize;
            }
        }
    }
}

fn seed_sequence_state(seed: u64) -> [u64; 4] {
    let mut entropy = Vec::new();
    let mut remaining = seed;
    entropy.push(remaining as u32);
    remaining >>= 32;
    while remaining != 0 {
        entropy.push(remaining as u32);
        remaining >>= 32;
    }

    let mut pool = [0_u32; 4];
    let mut hash_constant = INIT_A;
    for (index, value) in pool.iter_mut().enumerate() {
        *value = hash(
            entropy.get(index).copied().unwrap_or(0),
            &mut hash_constant,
            MULT_A,
        );
    }
    for source in 0..pool.len() {
        for destination in 0..pool.len() {
            if source != destination {
                let hashed = hash(pool[source], &mut hash_constant, MULT_A);
                pool[destination] = mix(pool[destination], hashed);
            }
        }
    }
    for source in entropy.iter().skip(pool.len()) {
        for destination in &mut pool {
            let hashed = hash(*source, &mut hash_constant, MULT_A);
            *destination = mix(*destination, hashed);
        }
    }

    let mut words = [0_u32; 8];
    let mut hash_constant = INIT_B;
    for (index, word) in words.iter_mut().enumerate() {
        *word = hash(pool[index % pool.len()], &mut hash_constant, MULT_B);
    }

    [
        u64::from(words[0]) | (u64::from(words[1]) << 32),
        u64::from(words[2]) | (u64::from(words[3]) << 32),
        u64::from(words[4]) | (u64::from(words[5]) << 32),
        u64::from(words[6]) | (u64::from(words[7]) << 32),
    ]
}

fn hash(mut value: u32, hash_constant: &mut u32, multiplier: u32) -> u32 {
    value ^= *hash_constant;
    *hash_constant = (*hash_constant).wrapping_mul(multiplier);
    value = value.wrapping_mul(*hash_constant);
    value ^ (value >> 16)
}

fn mix(x: u32, y: u32) -> u32 {
    let value = MIX_MULT_L
        .wrapping_mul(x)
        .wrapping_sub(MIX_MULT_R.wrapping_mul(y));
    value ^ (value >> 16)
}

#[cfg(test)]
mod tests {
    use super::{Pcg64, RandomState};

    #[test]
    fn pcg64_matches_numpy_default_rng_seed_42() {
        let mut rng = Pcg64::seed(42);
        let values: Vec<u64> = (0..3).map(|_| rng.uniform(0.0, 1.0).to_bits()).collect();

        assert_eq!(
            values,
            [
                0x3fe8_c43f_79a2_db24,
                0x3fdc_1695_9869_e47e,
                0x3feb_79a2_584d_db42,
            ]
        );
    }

    #[test]
    fn random_state_matches_numpy_random_state_seed_42() {
        let mut rng = RandomState::seed(42);
        let values: Vec<u64> = (0..3).map(|_| rng.uniform(0.0, 1.0).to_bits()).collect();

        assert_eq!(
            values,
            [
                0x3fd7_f877_1e5f_51ec,
                0x3fee_6c40_68bb_d654,
                0x3fe7_6c7e_8f1e_6751,
            ]
        );
    }
}
