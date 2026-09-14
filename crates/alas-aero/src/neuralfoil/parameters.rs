// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from neuralfoil/main.py (the module-level parameter loading).
// Upstream: NeuralFoil 0.3.x, MIT. Trained parameters (c) Peter Sharpe.
// Reference: alas @ rust-port-baseline.

//! The trained parameters: five networks and the statistics of the data they
//! were trained on.
//!
//! Everything else in this port is a formula that can be read and checked.
//! This is not: the weights *are* the model, and there is nothing to derive
//! them from. So they are shipped as data, exported from the installed
//! NeuralFoil package by `golden/generators/gen_aero_neuralfoil.py` -- which
//! also records a digest of each blob in `golden/aero/neuralfoil.json`, so
//! that a file here drifting from the package it came from is a failing test
//! and not a silently different aeroplane.
//!
//! # Which five, and why not eight
//!
//! NeuralFoil ships eight sizes. This program can ask for five:
//! `AirfoilSweepScreen.tsx`'s `MODEL_SIZES` offers `small` through `xxlarge`,
//! and every default in the reference -- `airfoil_screening.py`'s, the sweep
//! request's, and the one `neuralfoil.get_aero_from_coordinates` applies when
//! `visualization.py` passes none -- names `large`, which is inside that set.
//! `xxsmall`, `xsmall` and `xxxlarge` are unreachable from any input this
//! program accepts, and `xxxlarge` alone would add 5.7 MB. A documented scope
//! boundary, not a `deviation-candidate`.
//!
//! The five do not share a depth: `small` has four weight layers, `medium`
//! and `large` five, `xlarge` and `xxlarge` six. That is the point of
//! shipping more than one -- a reader that assumed a fixed architecture would
//! agree on `large` and be wrong on `small`.
//!
//! # The blob format, and why binary
//!
//! Every other embedded table in this port is text, because every other one
//! is reviewable as a table: a material's yield strength or an airfoil's
//! coordinates mean something to a reader. Half a million trained weights do
//! not, and a decimal encoding that round-tripped them exactly would be two
//! and a half times the size for no gain in reviewability. So each model is
//! one little-endian blob: an eight-byte magic, the layer count, one
//! `(rows, cols)` pair per layer, then each layer's weight matrix in
//! row-major order followed by its bias.
//!
//! # Width
//!
//! The stored values are `f32`, because that is what the `.npz` holds. The
//! *arithmetic* is not: NumPy promotes `f32 @ f64` to `f64` before
//! multiplying, so every product upstream evaluates is a double. The blobs
//! therefore keep the `f32` bit patterns and this module widens them once at
//! load, which reproduces both halves of that -- `f32`-precision values,
//! `f64` arithmetic -- and stores half of what a pre-widened blob would.

use std::sync::OnceLock;

use super::soft::swish;

/// Which trained network to evaluate.
///
/// Larger models are more accurate and slower. `Large` is what every default
/// in this program names; the variants either side exist because the sweep
/// interface offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelSize {
    /// Four weight layers of 64 units.
    Small,
    /// Five weight layers of 64 units.
    Medium,
    /// Five weight layers of 128 units. Every default in this program.
    Large,
    /// Six weight layers of 128 units.
    XLarge,
    /// Six weight layers of 256 units.
    XXLarge,
}

impl ModelSize {
    /// The upstream spelling, as a sweep request or a saved run carries it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::XLarge => "xlarge",
            Self::XXLarge => "xxlarge",
        }
    }

    /// The size `name` spells, or `None` if this port does not ship it.
    ///
    /// Returns `None` for NeuralFoil's other three sizes as well as for
    /// nonsense: see the module documentation for why only five are
    /// embedded. Upstream raises `ValueError` on an unrecognized name, which
    /// is the same refusal.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "xlarge" => Some(Self::XLarge),
            "xxlarge" => Some(Self::XXLarge),
            _ => None,
        }
    }

    /// Every size this port ships, smallest first.
    pub fn all() -> [Self; 5] {
        [
            Self::Small,
            Self::Medium,
            Self::Large,
            Self::XLarge,
            Self::XXLarge,
        ]
    }

    fn blob(self) -> &'static [u8] {
        match self {
            Self::Small => SMALL,
            Self::Medium => MEDIUM,
            Self::Large => LARGE,
            Self::XLarge => XLARGE,
            Self::XXLarge => XXLARGE,
        }
    }
}

macro_rules! blob {
    ($name:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/", $name))
    };
}

const SMALL: &[u8] = blob!("nn-small.bin");
const MEDIUM: &[u8] = blob!("nn-medium.bin");
const LARGE: &[u8] = blob!("nn-large.bin");
const XLARGE: &[u8] = blob!("nn-xlarge.bin");
const XXLARGE: &[u8] = blob!("nn-xxlarge.bin");
const DISTRIBUTION: &[u8] = blob!("nn-input-distribution.bin");

const NETWORK_MAGIC: &[u8; 8] = b"ALASNFW1";
const DISTRIBUTION_MAGIC: &[u8; 8] = b"ALASNFD1";

/// How many features every trained network takes: sixteen shape weights, the
/// leading-edge mode, the trailing-edge thickness, three angle-of-attack
/// features, the Reynolds number, the amplification factor, and two forced
/// transition stations.
pub(super) const INPUTS: usize = 25;

/// How many channels every trained network returns: six headline quantities
/// plus six boundary-layer channels sampled at 32 stations each.
pub(super) const OUTPUTS: usize = 198;

/// One affine layer: `rows` outputs from `cols` inputs, plus a bias.
struct Layer {
    rows: usize,
    cols: usize,
    /// Row-major, `rows * cols` long.
    weights: Vec<f64>,
    bias: Vec<f64>,
}

/// A trained network, as a stack of affine layers.
///
/// Only [`decode_network`] constructs one, and it refuses any blob whose
/// layers do not chain from [`INPUTS`] to [`OUTPUTS`] -- so [`Network::evaluate`]
/// is total rather than fallible, and the shape invariant is checked once at
/// load rather than at every call.
pub(super) struct Network {
    layers: Vec<Layer>,
}

impl Network {
    /// Evaluate the network on one input vector.
    ///
    /// Every layer but the last is followed by a [`swish`] activation, which
    /// is upstream's "don't apply the activation function on the last layer"
    /// written the other way around.
    pub(super) fn evaluate(&self, input: &[f64; INPUTS]) -> Vec<f64> {
        let mut activations = input.to_vec();
        let last = self.layers.len().saturating_sub(1);
        for (index, layer) in self.layers.iter().enumerate() {
            let mut next = Vec::with_capacity(layer.rows);
            for row in 0..layer.rows {
                let offset = row * layer.cols;
                let mut total = 0.0;
                for (column, activation) in activations.iter().enumerate() {
                    total += layer.weights[offset + column] * activation;
                }
                total += layer.bias[row];
                next.push(if index == last { total } else { swish(total) });
            }
            activations = next;
        }
        activations
    }

    /// Evaluate the network on a batch of inputs, one output vector per input.
    ///
    /// Bit-identical to calling [`Self::evaluate`] on each input: every
    /// output is accumulated over the columns in the same order and the bias
    /// and activation are applied at the same point. The batch only changes
    /// the loop nest -- activations are held feature-major so that the
    /// innermost loop runs across the batch with one weight, which the
    /// compiler vectorizes and which reads each weight once per layer rather
    /// than once per input. An airfoil sweep of a few dozen angles, each
    /// evaluated direct and mirrored, is the batch this exists for.
    pub(super) fn evaluate_batch(&self, inputs: &[[f64; INPUTS]]) -> Vec<Vec<f64>> {
        let batch = inputs.len();
        if batch == 0 {
            return Vec::new();
        }
        let mut width = INPUTS;
        let mut activations = vec![0.0; INPUTS * batch];
        for (b, input) in inputs.iter().enumerate() {
            for (feature, value) in input.iter().enumerate() {
                activations[feature * batch + b] = *value;
            }
        }
        let last = self.layers.len().saturating_sub(1);
        for (index, layer) in self.layers.iter().enumerate() {
            let mut next = vec![0.0; layer.rows * batch];
            for row in 0..layer.rows {
                let totals = &mut next[row * batch..(row + 1) * batch];
                let weights = &layer.weights[row * layer.cols..row * layer.cols + width];
                for (column, &weight) in weights.iter().enumerate() {
                    let feature = &activations[column * batch..(column + 1) * batch];
                    for (total, activation) in totals.iter_mut().zip(feature) {
                        *total += weight * activation;
                    }
                }
                let bias = layer.bias[row];
                for total in totals.iter_mut() {
                    *total += bias;
                    if index != last {
                        *total = swish(*total);
                    }
                }
            }
            activations = next;
            width = layer.rows;
        }
        (0..batch)
            .map(|b| (0..width).map(|row| activations[row * batch + b]).collect())
            .collect()
    }
}

/// Where the training data sat in the twenty-five-dimensional input space.
///
/// The network's first output is a confidence, and it is trained to fall away
/// from the data rather than to extrapolate confidently: the squared
/// Mahalanobis distance of a query from this distribution is subtracted from
/// that channel before the sigmoid. Only the mean and the *inverse*
/// covariance are read at inference time; the covariance itself is in the
/// upstream archive and is never used, so it is not exported.
pub(super) struct Distribution {
    mean: Vec<f64>,
    /// Row-major, `mean.len() * mean.len()` long.
    inverse_covariance: Vec<f64>,
}

impl Distribution {
    /// How many inputs the distribution describes -- the `N_inputs` upstream
    /// divides the penalty by.
    pub(super) fn inputs(&self) -> usize {
        self.mean.len()
    }

    /// `(x - mu)^T S^-1 (x - mu)`, evaluated in the order upstream evaluates
    /// it: the row vector against the matrix first, then elementwise against
    /// the offset again and summed.
    pub(super) fn squared_mahalanobis_distance(&self, x: &[f64]) -> f64 {
        let offsets: Vec<f64> = x
            .iter()
            .zip(&self.mean)
            .map(|(value, mean)| value - mean)
            .collect();
        let size = offsets.len();

        let mut total = 0.0;
        for (column, offset) in offsets.iter().enumerate() {
            let mut projected = 0.0;
            for (row, other) in offsets.iter().enumerate() {
                projected += other * self.inverse_covariance[row * size + column];
            }
            total += projected * offset;
        }
        total
    }
}

/// Reads little-endian scalars out of an embedded blob, refusing rather than
/// panicking when one runs short.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn magic(&mut self, expected: &[u8; 8]) -> Option<()> {
        let taken = self.bytes.get(self.at..self.at + 8)?;
        self.at += 8;
        (taken == expected).then_some(())
    }

    fn count(&mut self) -> Option<usize> {
        let taken: [u8; 4] = self.bytes.get(self.at..self.at + 4)?.try_into().ok()?;
        self.at += 4;
        Some(u32::from_le_bytes(taken) as usize)
    }

    /// `n` `f32` values, widened to `f64` as NumPy widens them before a
    /// multiply.
    fn widened(&mut self, n: usize) -> Option<Vec<f64>> {
        let taken = self.bytes.get(self.at..self.at + 4 * n)?;
        self.at += 4 * n;
        Some(
            taken
                .chunks_exact(4)
                .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok())
                .map(|bytes| f64::from(f32::from_le_bytes(bytes)))
                .collect(),
        )
    }

    fn doubles(&mut self, n: usize) -> Option<Vec<f64>> {
        let taken = self.bytes.get(self.at..self.at + 8 * n)?;
        self.at += 8 * n;
        Some(
            taken
                .chunks_exact(8)
                .filter_map(|chunk| <[u8; 8]>::try_from(chunk).ok())
                .map(f64::from_le_bytes)
                .collect(),
        )
    }

    fn is_exhausted(&self) -> bool {
        self.at == self.bytes.len()
    }
}

fn decode_network(bytes: &[u8]) -> Option<Network> {
    let mut reader = Reader::new(bytes);
    reader.magic(NETWORK_MAGIC)?;

    let count = reader.count()?;
    let mut shapes = Vec::with_capacity(count);
    for _ in 0..count {
        let rows = reader.count()?;
        let cols = reader.count()?;
        shapes.push((rows, cols));
    }

    // Refused here rather than indexed into later: a blob whose layers do
    // not chain would read a weight matrix against the wrong activation
    // vector, which is a panic at best and a plausible wrong answer at worst.
    let chains = shapes.first().is_some_and(|&(_, cols)| cols == INPUTS)
        && shapes.last().is_some_and(|&(rows, _)| rows == OUTPUTS)
        && shapes.windows(2).all(|pair| pair[0].0 == pair[1].1);
    if !chains {
        return None;
    }

    let mut layers = Vec::with_capacity(count);
    for (rows, cols) in shapes {
        let weights = reader.widened(rows * cols)?;
        let bias = reader.widened(rows)?;
        layers.push(Layer {
            rows,
            cols,
            weights,
            bias,
        });
    }
    reader.is_exhausted().then_some(Network { layers })
}

fn decode_distribution(bytes: &[u8]) -> Option<Distribution> {
    let mut reader = Reader::new(bytes);
    reader.magic(DISTRIBUTION_MAGIC)?;
    let size = reader.count()?;
    let mean = reader.widened(size)?;
    let inverse_covariance = reader.doubles(size * size)?;
    reader.is_exhausted().then_some(Distribution {
        mean,
        inverse_covariance,
    })
}

// The blobs are `include_bytes!` of files this repository ships, written by
// `gen_aero_neuralfoil.py` and digested by `golden/aero/neuralfoil.json`. A
// decode failure is therefore a malformed constant rather than anything a
// caller can cause, and `every_embedded_blob_decodes` asserts it directly
// rather than leaving the first caller to discover it. The same arrangement
// `alas-atmo::differentiable` uses for its two spline fits.
#[allow(clippy::expect_used)]
mod decoded {
    use super::{
        decode_distribution, decode_network, Distribution, ModelSize, Network, OnceLock,
        DISTRIBUTION,
    };

    pub(in crate::neuralfoil) fn network(size: ModelSize) -> &'static Network {
        static CACHE: OnceLock<Vec<(ModelSize, Network)>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| {
            ModelSize::all()
                .into_iter()
                .map(|size| {
                    let decoded = decode_network(size.blob())
                        .expect("an embedded network blob is well-formed");
                    (size, decoded)
                })
                .collect()
        });
        cache
            .iter()
            .find(|(candidate, _)| *candidate == size)
            .map(|(_, network)| network)
            .expect("every ModelSize is in the cache, which is built from ModelSize::all")
    }

    pub(in crate::neuralfoil) fn distribution() -> &'static Distribution {
        static CACHE: OnceLock<Distribution> = OnceLock::new();
        CACHE.get_or_init(|| {
            decode_distribution(DISTRIBUTION)
                .expect("the embedded input-distribution blob is well-formed")
        })
    }
}

pub(super) use decoded::{distribution, network};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_blob_decodes() {
        for size in ModelSize::all() {
            assert!(
                decode_network(size.blob()).is_some(),
                "{} did not decode",
                size.name()
            );
        }
        assert!(decode_distribution(DISTRIBUTION).is_some());
    }

    #[test]
    fn every_network_takes_twenty_five_inputs_and_returns_one_hundred_and_ninety_eight() {
        for size in ModelSize::all() {
            let produced = network(size).evaluate(&[0.0; INPUTS]);
            assert_eq!(produced.len(), OUTPUTS, "{}", size.name());
        }
    }

    /// A well-formed blob of the given layer shapes, filled with zeros.
    fn synthetic(shapes: &[(usize, usize)]) -> Vec<u8> {
        let mut bytes = NETWORK_MAGIC.to_vec();
        bytes.extend((shapes.len() as u32).to_le_bytes());
        for &(rows, cols) in shapes {
            bytes.extend((rows as u32).to_le_bytes());
            bytes.extend((cols as u32).to_le_bytes());
        }
        for &(rows, cols) in shapes {
            bytes.extend(std::iter::repeat_n(0u8, 4 * (rows * cols + rows)));
        }
        bytes
    }

    #[test]
    fn a_blob_whose_layers_do_not_chain_is_refused() {
        // Every byte count here is self-consistent, so nothing but the shape
        // chain itself distinguishes this from a valid network: layer one
        // produces four values and layer two expects three.
        assert!(decode_network(&synthetic(&[(4, INPUTS), (OUTPUTS, 3)])).is_none());
        assert!(decode_network(&synthetic(&[(4, INPUTS), (OUTPUTS, 4)])).is_some());
    }

    #[test]
    fn a_blob_with_the_wrong_input_or_output_width_is_refused() {
        assert!(decode_network(&synthetic(&[(4, INPUTS - 1), (OUTPUTS, 4)])).is_none());
        assert!(decode_network(&synthetic(&[(4, INPUTS), (OUTPUTS + 1, 4)])).is_none());
    }

    #[test]
    fn the_five_shipped_models_have_the_depths_upstream_trained() {
        let depths = [
            (ModelSize::Small, 4),
            (ModelSize::Medium, 5),
            (ModelSize::Large, 5),
            (ModelSize::XLarge, 6),
            (ModelSize::XXLarge, 6),
        ];
        for (size, depth) in depths {
            assert_eq!(network(size).layers.len(), depth, "{}", size.name());
        }
    }

    #[test]
    fn the_three_sizes_this_program_cannot_ask_for_are_not_shipped() {
        for name in ["xxsmall", "xsmall", "xxxlarge", "enormous", ""] {
            assert_eq!(ModelSize::from_name(name), None, "{name}");
        }
    }

    #[test]
    fn every_size_round_trips_through_its_upstream_name() {
        for size in ModelSize::all() {
            assert_eq!(ModelSize::from_name(size.name()), Some(size));
        }
    }

    #[test]
    fn the_inverse_covariance_is_symmetric_and_positive_on_its_diagonal() {
        // Not a translation check -- an assertion that the export landed the
        // 25x25 matrix in the right shape and orientation. It is symmetric to
        // about 1e-10 relative rather than exactly: upstream inverted a
        // symmetric matrix numerically and stored what came out, and the
        // entries run to 4e7, so a few ulps of asymmetry is what a `1e-9`
        // bound is framing here.
        let distribution = distribution();
        let size = distribution.inputs();
        assert_eq!(size, INPUTS);
        for row in 0..size {
            assert!(distribution.inverse_covariance[row * size + row] > 0.0);
            for column in 0..size {
                let upper = distribution.inverse_covariance[row * size + column];
                let lower = distribution.inverse_covariance[column * size + row];
                assert!(
                    (upper - lower).abs() <= 1e-9 * upper.abs().max(1.0),
                    "({row}, {column}): {upper} against {lower}"
                );
            }
        }
    }

    #[test]
    fn the_mahalanobis_distance_is_zero_at_the_mean_and_positive_away_from_it() {
        let distribution = distribution();
        let mean = distribution.mean.clone();
        assert!(distribution.squared_mahalanobis_distance(&mean).abs() < 1e-9);

        let mut moved = mean;
        moved[0] += 1.0;
        assert!(distribution.squared_mahalanobis_distance(&moved) > 0.0);
    }

    #[test]
    fn a_blob_with_the_wrong_magic_is_refused_rather_than_misread() {
        let mut corrupted = LARGE.to_vec();
        corrupted[0] = b'X';
        assert!(decode_network(&corrupted).is_none());
    }

    #[test]
    fn a_truncated_blob_is_refused_rather_than_misread() {
        assert!(decode_network(&LARGE[..LARGE.len() - 4]).is_none());
        assert!(decode_network(&LARGE[..8]).is_none());
    }

    #[test]
    fn a_blob_with_trailing_bytes_is_refused() {
        // Slack at the end means the reader and the writer disagree about the
        // layout, which is exactly the drift the digest fixture exists to
        // catch -- but this catches it at load rather than at test time.
        let mut padded = LARGE.to_vec();
        padded.push(0);
        assert!(decode_network(&padded).is_none());
    }
}
