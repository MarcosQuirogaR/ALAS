// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! SHA-256 (FIPS 180-4) for content-addressing external-tool inputs.
//!
//! The result cache keys a solver run by the exact bytes of its input deck, so
//! a collision would silently return another deck's answer. A 64-bit
//! non-cryptographic hash is not enough margin for that, and the dependency
//! policy asks for a justification per new crate; the algorithm is small and
//! fully specified, so it lives here with the FIPS 180-4 test vectors.

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// Streaming SHA-256 state.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length_bytes: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// A fresh hasher.
    pub fn new() -> Self {
        Self {
            state: INITIAL,
            block: [0; 64],
            filled: 0,
            length_bytes: 0,
        }
    }

    /// Absorb `data`.
    pub fn update(&mut self, mut data: &[u8]) {
        self.length_bytes = self.length_bytes.wrapping_add(data.len() as u64);
        if self.filled > 0 {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < 64 {
                return;
            }
            let block = self.block;
            compress(&mut self.state, &block);
            self.filled = 0;
        }
        let mut chunks = data.chunks_exact(64);
        for chunk in &mut chunks {
            let mut block = [0_u8; 64];
            block.copy_from_slice(chunk);
            compress(&mut self.state, &block);
        }
        let rest = chunks.remainder();
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// Absorb a length-prefixed field, so adjacent fields cannot be re-split
    /// into a different sequence with the same concatenation.
    pub fn update_field(&mut self, data: &[u8]) {
        self.update(&(data.len() as u64).to_le_bytes());
        self.update(data);
    }

    /// Finish and return the digest.
    pub fn finalize(mut self) -> [u8; 32] {
        let bit_length = self.length_bytes.wrapping_mul(8);
        let mut padding = vec![0x80_u8];
        let used = (self.filled + 1) % 64;
        let zeros = if used <= 56 { 56 - used } else { 120 - used };
        padding.extend(std::iter::repeat_n(0_u8, zeros));
        padding.extend_from_slice(&bit_length.to_be_bytes());
        let saved = self.length_bytes;
        self.update(&padding);
        self.length_bytes = saved;
        let mut digest = [0_u8; 32];
        for (slot, word) in digest.chunks_exact_mut(4).zip(self.state) {
            slot.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    /// Finish and return the digest as 64 lowercase hex digits.
    pub fn finalize_hex(self) -> String {
        hex(&self.finalize())
    }
}

/// Lowercase hex text of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0_u32; 64];
    for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize_hex()
    }

    #[test]
    fn matches_the_fips_180_4_vectors() {
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn a_million_a_characters_and_split_updates_agree() {
        let data = vec![b'a'; 1_000_000];
        assert_eq!(
            digest(&data),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let mut split = Sha256::new();
        for chunk in data.chunks(997) {
            split.update(chunk);
        }
        assert_eq!(split.finalize_hex(), digest(&data));
    }

    #[test]
    fn padding_boundaries_55_56_63_64_bytes_are_consistent() {
        for length in [55_usize, 56, 57, 63, 64, 65, 119, 120] {
            let data = vec![0x5a_u8; length];
            let mut byte_wise = Sha256::new();
            for byte in &data {
                byte_wise.update(std::slice::from_ref(byte));
            }
            assert_eq!(byte_wise.finalize_hex(), digest(&data), "length {length}");
        }
    }
}
