// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from neuralfoil/main.py (get_aero_from_kulfan_parameters and the
// _sigmoid / _squared_mahalanobis_distance helpers it calls).
// Upstream: NeuralFoil 0.3.x, MIT.
// Reference: alas @ rust-port-baseline.

//! The network itself: eighteen shape numbers and a flight condition in,
//! a lift curve and a boundary layer out.
//!
//! # Why it is evaluated twice
//!
//! An airfoil turned upside down at the opposite angle of attack has the
//! mirror image of the same flow. That is exact, and a network trained on
//! finite data does not obey it exactly -- so upstream builds it in at
//! evaluation time rather than hoping for it: the inputs are flipped (both
//! surfaces swapped and negated, the leading-edge mode and the
//! `sin(2*alpha)` feature negated, the two forced-transition stations
//! exchanged), the network is run again, the outputs are unflipped, and the
//! two are averaged. The training did the same, so this is not a correction
//! bolted on afterwards -- it is half of what the model is, and a port that
//! evaluated once would be systematically wrong rather than merely noisier.
//!
//! The unflipping permutation is the fiddly part, and it is where a port goes
//! wrong invisibly: lift and moment negate, the two transition locations
//! exchange, the upper and lower boundary layers exchange, and the two edge
//! velocity profiles exchange *and* negate. Get one of those wrong and the
//! lift curve still looks like a lift curve.
//!
//! # The confidence channel
//!
//! The first output is not an aerodynamic quantity. It is how far the query
//! is from the data the network was trained on, and it is built that way
//! explicitly: the squared Mahalanobis distance from the training
//! distribution is subtracted from the raw channel before the sigmoid, so a
//! shape unlike anything in the training set reports near zero confidence
//! however plausible the numbers beside it look. That subtraction happens on
//! both evaluations, before the average.
//!
//! # Scope
//!
//! Vectorization is not translated. Upstream accepts arrays for every input
//! and broadcasts them to a common length; both reached call sites pass
//! scalars (`airfoil_screening.py` sweeps alpha in a Python loop over
//! candidates, and `visualization.py` loops over its grid), and a
//! broadcasting rule is a lot of machinery to reproduce for a shape nothing
//! asks for.
//!
//! `nf.bl_x_points` -- the 32 chord stations the boundary layer is reported
//! at -- is not translated either. It is externally accessible upstream, and
//! the only thing in this program that touches it reads its *length*:
//! `kulfan_airfoil.py` writes `range(len(nf.bl_x_points))` where it means 32.

use super::parameters::{self, ModelSize, INPUTS, OUTPUTS};
use super::{Conditions, NeuralFoilError};
use crate::kulfan::KulfanAirfoil;

/// How many chord stations the boundary layer is reported at, per surface.
pub const BL_STATIONS: usize = 32;

/// The weights per side NeuralFoil is trained on, and the only count it
/// accepts.
const WEIGHTS_PER_SIDE: usize = 8;

/// One surface's boundary layer, at [`BL_STATIONS`] chord stations running
/// from the leading edge aft.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryLayer {
    /// Momentum thickness, as a fraction of chord.
    pub theta: [f64; BL_STATIONS],
    /// Shape factor `H` -- displacement thickness over momentum thickness.
    pub shape_factor: [f64; BL_STATIONS],
    /// Edge velocity over freestream velocity. Signed: the lower surface's
    /// is reported negative by the flip-and-average convention.
    pub ue_over_vinf: [f64; BL_STATIONS],
}

/// What the network reports, before any compressibility or post-stall
/// correction.
///
/// This is the whole of `neuralfoil.get_aero_from_kulfan_parameters`'s
/// output. `alas-aero::neuralfoil`'s other entry points wrap it.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkAero {
    /// How far inside its training data the query sat: 1 is confident, 0 is
    /// an extrapolation that should not be believed.
    pub analysis_confidence: f64,
    /// Section lift coefficient.
    pub cl: f64,
    /// Section drag coefficient.
    pub cd: f64,
    /// Section pitching-moment coefficient about the quarter chord.
    pub cm: f64,
    /// Upper-surface transition location, as a fraction of chord.
    pub top_xtr: f64,
    /// Lower-surface transition location, as a fraction of chord.
    pub bot_xtr: f64,
    /// The upper surface's boundary layer.
    pub upper: BoundaryLayer,
    /// The lower surface's boundary layer.
    pub lower: BoundaryLayer,
}

/// Evaluate the trained network on one airfoil at one flight condition.
///
/// # Errors
///
/// [`NeuralFoilError::WeightCount`] when the fit did not produce eight
/// weights per side, and [`NeuralFoilError::ClassExponents`] when the class
/// function is not the conventional `(0.5, 1.0)` -- the two things upstream
/// raises `NotImplementedError` for, and for the same reason: the network was
/// trained on that parameterization and nothing else, so an answer for
/// another one would be a confident guess.
pub fn evaluate(
    airfoil: &KulfanAirfoil,
    conditions: &Conditions,
    size: ModelSize,
) -> Result<NetworkAero, NeuralFoilError> {
    let input = features(airfoil, conditions)?;
    let network = parameters::network(size);
    let distribution = parameters::distribution();
    // Baked into training so the confidence asymptotes to zero far from the
    // data; applied to each evaluation separately, before the average.
    let penalty = 2.0 * distribution.inputs() as f64;

    let mut direct = network.evaluate(&input);
    direct[0] -= distribution.squared_mahalanobis_distance(&input) / penalty;

    let mirrored_input = mirror_input(&input);
    let mut mirrored = network.evaluate(&mirrored_input);
    mirrored[0] -= distribution.squared_mahalanobis_distance(&mirrored_input) / penalty;
    let restored = restore_mirrored(&mirrored);

    let mut fused: Vec<f64> = direct
        .iter()
        .zip(&restored)
        .map(|(one, other)| (one + other) / 2.0)
        .collect();

    fused[0] = confidence_sigmoid(fused[0]);
    fused[4] = fused[4].clamp(0.0, 1.0);
    fused[5] = fused[5].clamp(0.0, 1.0);

    Ok(unpack(&fused, conditions.reynolds))
}

/// The twenty-five features the network takes, in upstream's order.
fn features(
    airfoil: &KulfanAirfoil,
    conditions: &Conditions,
) -> Result<[f64; INPUTS], NeuralFoilError> {
    if airfoil.upper_weights.len() != WEIGHTS_PER_SIDE
        || airfoil.lower_weights.len() != WEIGHTS_PER_SIDE
    {
        return Err(NeuralFoilError::WeightCount {
            upper: airfoil.upper_weights.len(),
            lower: airfoil.lower_weights.len(),
        });
    }
    if airfoil.n1 != 0.5 || airfoil.n2 != 1.0 {
        return Err(NeuralFoilError::ClassExponents {
            n1: airfoil.n1,
            n2: airfoil.n2,
        });
    }

    let alpha = conditions.alpha_deg;
    let cos_alpha = alpha.to_radians().cos();

    let mut input = [0.0; INPUTS];
    input[..WEIGHTS_PER_SIDE].copy_from_slice(&airfoil.upper_weights);
    input[WEIGHTS_PER_SIDE..2 * WEIGHTS_PER_SIDE].copy_from_slice(&airfoil.lower_weights);
    input[16] = airfoil.leading_edge_weight;
    // The trailing-edge gap is scaled by 50 so that it spans a comparable
    // range to the shape weights; the network was trained on the scaled one.
    input[17] = airfoil.te_thickness * 50.0;
    input[18] = (2.0 * alpha).to_radians().sin();
    input[19] = cos_alpha;
    input[20] = 1.0 - cos_alpha * cos_alpha;
    input[21] = (conditions.reynolds.ln() - 12.5) / 3.5;
    // No Mach feature: the network is incompressible, and the compressibility
    // corrections sit outside it in `super::corrections`.
    input[22] = (conditions.n_crit - 9.0) / 4.5;
    input[23] = conditions.xtr_upper;
    input[24] = conditions.xtr_lower;
    Ok(input)
}

/// The same query with the airfoil turned over and the incidence reversed.
fn mirror_input(input: &[f64; INPUTS]) -> [f64; INPUTS] {
    let mut mirrored = *input;
    for index in 0..WEIGHTS_PER_SIDE {
        mirrored[index] = -input[WEIGHTS_PER_SIDE + index];
        mirrored[WEIGHTS_PER_SIDE + index] = -input[index];
    }
    mirrored[16] = -input[16];
    mirrored[18] = -input[18];
    mirrored[23] = input[24];
    mirrored[24] = input[23];
    mirrored
}

/// Put a mirrored evaluation back the right way up.
///
/// Everything the mirror exchanged is exchanged back, and everything it
/// negated is negated back. The confidence and the drag are invariant under
/// the mirror and pass through untouched, which is why neither appears here.
fn restore_mirrored(mirrored: &[f64]) -> Vec<f64> {
    const HEAD: usize = 6;
    const PAIR: usize = 2 * BL_STATIONS;

    let mut restored = mirrored.to_vec();
    restored[1] = -mirrored[1];
    restored[3] = -mirrored[3];
    restored[4] = mirrored[5];
    restored[5] = mirrored[4];

    // Momentum thickness and shape factor, which are the same quantity on
    // whichever surface: exchanged, not negated.
    for index in 0..PAIR {
        restored[HEAD + index] = mirrored[HEAD + 3 * BL_STATIONS + index];
        restored[HEAD + 3 * BL_STATIONS + index] = mirrored[HEAD + index];
    }
    // Edge velocity, which carries a sign convention that the mirror also
    // reverses: exchanged and negated.
    for index in 0..BL_STATIONS {
        restored[HEAD + 2 * BL_STATIONS + index] = -mirrored[HEAD + 5 * BL_STATIONS + index];
        restored[HEAD + 5 * BL_STATIONS + index] = -mirrored[HEAD + 2 * BL_STATIONS + index];
    }
    restored
}

/// The logistic curve the confidence channel is squashed through, clipped
/// first so that `exp` cannot overflow.
///
/// The clip bound is `ln(10 / f64::MAX)`, about -707.7 -- upstream computes
/// it the same way rather than writing it down, so that it follows the float
/// width rather than assuming one.
fn confidence_sigmoid(x: f64) -> f64 {
    let ln_eps = (10.0 / f64::MAX).ln();
    1.0 / (1.0 + (-x.clamp(ln_eps, -ln_eps)).exp())
}

/// Turn the 198 raw channels into named quantities.
fn unpack(fused: &[f64], reynolds: f64) -> NetworkAero {
    const HEAD: usize = 6;
    debug_assert_eq!(fused.len(), OUTPUTS);

    let upper = surface(fused, HEAD, reynolds);
    let lower = surface(fused, HEAD + 3 * BL_STATIONS, reynolds);

    NetworkAero {
        analysis_confidence: fused[0],
        cl: fused[1] / 2.0,
        // The network learns the logarithm, so that a drag coefficient two
        // orders apart at either end of the Reynolds range is one range of
        // outputs rather than two.
        cd: ((fused[2] - 2.0) * 2.0).exp(),
        cm: fused[3] / 20.0,
        top_xtr: fused[4],
        bot_xtr: fused[5],
        upper,
        lower,
    }
}

/// One surface's three boundary-layer channels, starting at `base`.
fn surface(fused: &[f64], base: usize, reynolds: f64) -> BoundaryLayer {
    let mut theta = [0.0; BL_STATIONS];
    let mut shape_factor = [0.0; BL_STATIONS];
    let mut ue_over_vinf = [0.0; BL_STATIONS];

    for station in 0..BL_STATIONS {
        ue_over_vinf[station] = fused[base + 2 * BL_STATIONS + station];
    }
    for station in 0..BL_STATIONS {
        // The learned channel is a momentum-thickness Reynolds number in
        // logarithmic form, offset by 0.1 so that zero is representable; the
        // thickness comes back out by dividing through by the local Reynolds
        // number the edge velocity implies.
        theta[station] =
            (10.0_f64.powf(fused[base + station]) - 0.1) / (ue_over_vinf[station].abs() * reynolds);
        shape_factor[station] = 2.6 * fused[base + BL_STATIONS + station].exp();
    }

    BoundaryLayer {
        theta,
        shape_factor,
        ue_over_vinf,
    }
}

// A test asserts on sections it constructed here directly, so a failed
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> KulfanAirfoil {
        // A plausible cambered section, close enough to the training data
        // that the network reports real numbers rather than an extrapolation.
        KulfanAirfoil {
            upper_weights: vec![0.18, 0.20, 0.19, 0.22, 0.20, 0.21, 0.17, 0.16],
            lower_weights: vec![-0.13, -0.14, -0.11, -0.10, -0.06, -0.02, 0.02, 0.04],
            leading_edge_weight: 0.06,
            te_thickness: 0.002,
            n1: 0.5,
            n2: 1.0,
        }
    }

    fn symmetric() -> KulfanAirfoil {
        KulfanAirfoil {
            upper_weights: vec![0.17, 0.16, 0.16, 0.15, 0.14, 0.13, 0.12, 0.10],
            lower_weights: vec![-0.17, -0.16, -0.16, -0.15, -0.14, -0.13, -0.12, -0.10],
            leading_edge_weight: 0.0,
            te_thickness: 0.0,
            n1: 0.5,
            n2: 1.0,
        }
    }

    #[test]
    fn a_symmetric_section_at_zero_incidence_carries_no_lift_and_no_moment() {
        // This is the property the flip-and-average exists to guarantee, and
        // it holds exactly rather than approximately: the mirrored query of a
        // symmetric section at zero alpha is the original query, so the two
        // evaluations cancel to the last bit.
        let aero = evaluate(&symmetric(), &Conditions::new(0.0, 1e6), ModelSize::Large)
            .expect("a conventional eight-weight section");
        assert_eq!(aero.cl, 0.0);
        assert_eq!(aero.cm, 0.0);
        assert_eq!(aero.top_xtr, aero.bot_xtr);
    }

    #[test]
    fn a_symmetric_section_is_antisymmetric_in_the_angle_of_attack() {
        let positive = evaluate(&symmetric(), &Conditions::new(5.0, 1e6), ModelSize::Large)
            .expect("a conventional eight-weight section");
        let negative = evaluate(&symmetric(), &Conditions::new(-5.0, 1e6), ModelSize::Large)
            .expect("a conventional eight-weight section");
        assert!((positive.cl + negative.cl).abs() < 1e-12);
        assert!((positive.cm + negative.cm).abs() < 1e-12);
        assert!((positive.cd - negative.cd).abs() < 1e-12);
        assert!((positive.top_xtr - negative.bot_xtr).abs() < 1e-12);
    }

    #[test]
    fn every_shipped_model_size_answers_the_same_question_to_within_its_accuracy() {
        // Not a parity check -- a check that all five decode and run, and
        // that none of them is wired up to the wrong blob. They disagree by
        // a few per cent, which is the accuracy-for-speed trade the sizes
        // exist to offer; a mis-wired one would disagree by far more.
        let conditions = Conditions::new(3.0, 1e6);
        let reference = evaluate(&section(), &conditions, ModelSize::Large)
            .expect("a conventional eight-weight section")
            .cl;
        for size in ModelSize::all() {
            let cl = evaluate(&section(), &conditions, size)
                .expect("a conventional eight-weight section")
                .cl;
            assert!(
                (cl - reference).abs() < 0.1,
                "{} reported CL {cl}, against {reference}",
                size.name()
            );
        }
    }

    #[test]
    fn the_transition_locations_stay_inside_the_chord() {
        for alpha in [-20.0, -5.0, 0.0, 5.0, 12.0, 20.0] {
            let aero = evaluate(&section(), &Conditions::new(alpha, 1e5), ModelSize::Large)
                .expect("a conventional eight-weight section");
            assert!((0.0..=1.0).contains(&aero.top_xtr), "alpha {alpha}");
            assert!((0.0..=1.0).contains(&aero.bot_xtr), "alpha {alpha}");
        }
    }

    #[test]
    fn confidence_falls_away_from_the_training_data() {
        // A section with weights an order of magnitude outside anything an
        // airfoil produces should not be believed, however plausible the
        // lift coefficient beside it looks.
        let mut wild = section();
        wild.upper_weights = wild.upper_weights.iter().map(|w| w * 20.0).collect();
        let aero = evaluate(&wild, &Conditions::new(3.0, 1e6), ModelSize::Large)
            .expect("a conventional eight-weight section");
        assert!(
            aero.analysis_confidence < 0.05,
            "{}",
            aero.analysis_confidence
        );

        let ordinary = evaluate(&section(), &Conditions::new(3.0, 1e6), ModelSize::Large)
            .expect("a conventional eight-weight section");
        assert!(ordinary.analysis_confidence > 0.8);
    }

    #[test]
    fn the_confidence_sigmoid_saturates_rather_than_overflowing() {
        assert_eq!(confidence_sigmoid(0.0), 0.5);
        assert_eq!(confidence_sigmoid(f64::INFINITY), 1.0);
        // The clip is what makes this finite: `exp(707.7)` is the largest
        // power of e a double holds, so the far tail lands at ten times the
        // smallest normal rather than at zero or at an overflow.
        assert!(confidence_sigmoid(f64::NEG_INFINITY) > 0.0);
        assert!(confidence_sigmoid(f64::NEG_INFINITY) < 1e-307);
        assert_eq!(
            confidence_sigmoid(f64::NEG_INFINITY),
            confidence_sigmoid(-1e300)
        );
    }

    #[test]
    fn a_section_with_the_wrong_weight_count_is_refused_rather_than_guessed_at() {
        let mut narrow = section();
        narrow.upper_weights.pop();
        assert_eq!(
            evaluate(&narrow, &Conditions::new(0.0, 1e6), ModelSize::Large),
            Err(NeuralFoilError::WeightCount { upper: 7, lower: 8 })
        );
    }

    #[test]
    fn a_section_with_unconventional_class_exponents_is_refused() {
        let mut blunt = section();
        blunt.n2 = 0.5;
        assert_eq!(
            evaluate(&blunt, &Conditions::new(0.0, 1e6), ModelSize::Large),
            Err(NeuralFoilError::ClassExponents { n1: 0.5, n2: 0.5 })
        );
    }

    #[test]
    fn mirroring_an_input_twice_returns_it_unchanged() {
        let input = features(&section(), &Conditions::new(4.0, 1e6)).expect("a valid section");
        assert_eq!(mirror_input(&mirror_input(&input)), input);
    }

    #[test]
    fn restoring_a_mirrored_output_twice_returns_it_unchanged() {
        let raw: Vec<f64> = (0..OUTPUTS).map(|i| f64::from(i as u32) / 7.0).collect();
        assert_eq!(restore_mirrored(&restore_mirrored(&raw)), raw);
    }
}
