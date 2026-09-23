// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from neuralfoil/main.py (get_aero_from_kulfan_parameters,
// get_aero_from_airfoil, get_aero_from_coordinates) and
// native aerodynamic model/geometry/airfoil/{airfoil,kulfan_airfoil}.py
// (get_aero_from_neuralfoil).
// Upstream: NeuralFoil 0.3.x and native aerodynamic model 4.2.8, both MIT.
// Reference: alas @ rust-port-baseline.

//! The airfoil surrogate: a trained network that answers in microseconds the
//! question XFoil answers in seconds.
//!
//! This program screens the whole 1,665-entry UIUC coordinate database
//! against a design point, at every angle of attack in a sweep. That is a few
//! hundred thousand two-dimensional solves, and no panel method with a
//! coupled boundary layer will do it while someone waits. NeuralFoil is what
//! makes the sweep exist: `alas/analysis/airfoil_screening.py` uses it as its
//! first-stage proxy and only sends the survivors to MSES.
//!
//! # The two ways in
//!
//! They are not the same function with different arguments, and the
//! difference is worth stating because it is invisible at low speed.
//!
//! [`aero_from_airfoil`] is what `airfoil_screening.py:251` reaches, through
//! `Airfoil.get_aero_from_neuralfoil`. It normalizes the section, fits Kulfan
//! weights, runs the network, and then applies everything the network cannot
//! see: the post-stall blend and the whole compressibility schedule in
//! [`corrections`]. It takes a Mach number.
//!
//! [`aero_from_coordinates`] is what `visualization.py:2013` reaches, through
//! `neuralfoil.get_aero_from_coordinates`. It normalizes and fits the same
//! way, and then calls the *raw network*, with no Mach number and no
//! post-stall blending anywhere. A port that routed the two through one
//! implementation would agree at Mach zero and low incidence and disagree
//! everywhere else.
//!
//! Both apply the same moment correction on the way out, which is what makes
//! them look alike: the network answers about the normalized section, and its
//! quarter chord is not where the original section's was.
//!
//! # Scope
//!
//! Control surfaces are not translated. `get_aero_from_neuralfoil` takes a
//! list of them and folds each into an effective incidence and a drag
//! multiplier; both reached call sites leave it at `None`, and
//! `alas-geom::aircraft::wing`'s `WingXSec` carries no control-surface field at
//! all (its own module doc records why), so there is nothing in this port
//! that could supply one.
//!
//! `include_360_deg_effects` is not translated as a switch either. It
//! defaults to true, neither call site overrides it, and the false path is
//! simply the true path with the post-stall blend removed, so the blend is
//! unconditional here rather than guarded by a flag no caller sets.
//!
//! Vectorization, `nf.bl_x_points`, the three unshipped model sizes, and
//! `get_aero_from_dat_file` are each out of scope for reasons the submodules
//! record.
//!
//! # A finding, recorded rather than reproduced as a fixture case
//!
//! The database this surrogate is swept over contains multi-element high-lift
//! decks (`30p-30n` is one). An eight-weight-per-side Kulfan fit cannot
//! represent one: the weights diverge (1.4e7 on the last lower-surface mode)
//! and the network overflows to infinity rather than reporting low
//! confidence. That is faithful upstream behaviour and this port reproduces
//! it, but it is not a fixture case: every expected value would be an
//! infinity, and infinities compare equal whatever produced them. Whether the
//! sweep should reject such a section before fitting it is `alas-screen`'s
//! question, not this row's.

mod corrections;
mod network;
mod parameters;
mod soft;

pub use corrections::Aero;
pub use network::{BoundaryLayer, NetworkAero, BL_STATIONS};
pub use parameters::ModelSize;

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::spacing::linspace;

use crate::kulfan::{KulfanAirfoil, KulfanError};

/// The weights per side the network is trained on. Both reached call sites
/// fit with this count and no other.
const WEIGHTS_PER_SIDE: usize = 8;

/// Upstream's default sample count for `max_thickness`, `np.linspace(0, 1, 101)`.
const THICKNESS_SAMPLE_POINTS: usize = 101;

/// Why an airfoil could not be put to the surrogate.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum NeuralFoilError {
    /// The section was not parameterized with eight weights per side.
    /// Upstream raises `NotImplementedError` for the same input: the network
    /// was trained on that count and an answer for another would be a
    /// confident guess.
    #[error("NeuralFoil takes 8 Kulfan weights per side, not {upper} upper and {lower} lower")]
    WeightCount {
        /// Upper-surface weights supplied.
        upper: usize,
        /// Lower-surface weights supplied.
        lower: usize,
    },
    /// The class function was not the conventional round-nosed,
    /// closed-tailed `(0.5, 1.0)`. Upstream raises for this too.
    #[error("NeuralFoil takes N1 = 0.5 and N2 = 1.0, not N1 = {n1} and N2 = {n2}")]
    ClassExponents {
        /// Leading-edge exponent supplied.
        n1: f64,
        /// Trailing-edge exponent supplied.
        n2: f64,
    },
    /// The section's coordinates could not be reduced to Kulfan weights.
    #[error("the airfoil could not be fitted for NeuralFoil: {0}")]
    Fit(#[from] KulfanError),
}

/// One flight condition, as the surrogate takes it.
///
/// Mach is deliberately not here: the network is incompressible and the two
/// entry points that reach it directly have no Mach number to give it. The
/// compressible entry points take one alongside this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conditions {
    /// Angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Chord Reynolds number.
    pub reynolds: f64,
    /// Critical amplification factor for natural transition. Nine is the
    /// standard `e^9` method and an average wind tunnel; a clean tunnel is
    /// 10-12 and a dirty one 4-8.
    pub n_crit: f64,
    /// Forced transition on the upper surface, as a fraction of chord. One
    /// allows fully natural transition.
    pub xtr_upper: f64,
    /// Forced transition on the lower surface, as a fraction of chord.
    pub xtr_lower: f64,
}

impl Conditions {
    /// A condition at upstream's defaults for everything but incidence and
    /// Reynolds number: `n_crit = 9`, natural transition on both surfaces.
    ///
    /// Nothing in this program overrides those three, so this is the
    /// constructor every caller here wants; the fields stay public for a
    /// caller that does.
    pub fn new(alpha_deg: f64, reynolds: f64) -> Self {
        Self {
            alpha_deg,
            reynolds,
            n_crit: 9.0,
            xtr_upper: 1.0,
            xtr_lower: 1.0,
        }
    }

    /// The same condition at a different incidence.
    fn at_alpha(self, alpha_deg: f64) -> Self {
        Self { alpha_deg, ..self }
    }
}

/// The raw network on a section already reduced to Kulfan weights:
/// `neuralfoil.get_aero_from_kulfan_parameters`.
///
/// No compressibility, no post-stall blending: this is the incompressible,
/// attached answer the network was trained to give.
///
/// # Errors
///
/// [`NeuralFoilError`] when the section is not the eight-weight, `(0.5, 1.0)`
/// parameterization the network was trained on.
pub fn aero_from_kulfan_parameters(
    airfoil: &KulfanAirfoil,
    conditions: &Conditions,
    size: ModelSize,
) -> Result<NetworkAero, NeuralFoilError> {
    network::evaluate(airfoil, conditions, size)
}

/// The surrogate on a section already reduced to Kulfan weights, with
/// compressibility and post-stall behaviour applied:
/// `KulfanAirfoil.get_aero_from_neuralfoil`.
///
/// # Errors
///
/// [`NeuralFoilError`], as [`aero_from_kulfan_parameters`].
pub fn aero_from_kulfan_airfoil(
    airfoil: &KulfanAirfoil,
    conditions: &Conditions,
    mach: f64,
    size: ModelSize,
) -> Result<Aero, NeuralFoilError> {
    // Enforced before the network sees it, because the `sin(2 alpha)` feature
    // is not periodic in 360 degrees on its own.
    let alpha_deg = wrap_alpha(conditions.alpha_deg);
    let raw = network::evaluate(airfoil, &conditions.at_alpha(alpha_deg), size)?;
    let t_over_c = airfoil.max_thickness(&linspace(0.0, 1.0, THICKNESS_SAMPLE_POINTS));
    Ok(corrections::apply(
        raw,
        t_over_c,
        alpha_deg,
        conditions.reynolds,
        mach,
    ))
}

/// [`aero_from_kulfan_airfoil`] over a schedule of conditions, with the
/// network run once over the whole batch. The results are, in order, what
/// the single-condition entry point returns for each condition.
///
/// # Errors
///
/// See [`aero_from_kulfan_airfoil`].
pub fn aero_from_kulfan_airfoil_sweep(
    airfoil: &KulfanAirfoil,
    conditions: &[Conditions],
    mach: f64,
    size: ModelSize,
) -> Result<Vec<Aero>, NeuralFoilError> {
    let wrapped: Vec<Conditions> = conditions
        .iter()
        .map(|conditions| conditions.at_alpha(wrap_alpha(conditions.alpha_deg)))
        .collect();
    let raw = network::evaluate_sweep(airfoil, &wrapped, size)?;
    let t_over_c = airfoil.max_thickness(&linspace(0.0, 1.0, THICKNESS_SAMPLE_POINTS));
    Ok(raw
        .into_iter()
        .zip(&wrapped)
        .map(|(raw, conditions)| {
            corrections::apply(
                raw,
                t_over_c,
                conditions.alpha_deg,
                conditions.reynolds,
                mach,
            )
        })
        .collect())
}

/// The surrogate on a section given as coordinates, with compressibility and
/// post-stall behaviour applied: `Airfoil.get_aero_from_neuralfoil`, which
/// is what `alas/analysis/airfoil_screening.py` calls.
///
/// # Errors
///
/// [`NeuralFoilError::Fit`] when the coordinates cannot be reduced to Kulfan
/// weights, and the other variants as [`aero_from_kulfan_parameters`].
pub fn aero_from_airfoil(
    airfoil: &Airfoil,
    conditions: &Conditions,
    mach: f64,
    size: ModelSize,
) -> Result<Aero, NeuralFoilError> {
    PreparedAirfoil::prepare(airfoil)?.aero(conditions, mach, size)
}

/// An airfoil normalized and fitted once, ready to be evaluated at many
/// flight conditions.
///
/// [`aero_from_airfoil`] normalizes the section and fits its Kulfan weights
/// on every call, and that fit (a least-squares solve over a few hundred
/// vertices) costs more than the network it feeds (about 210 us against
/// 140 us per call on a desktop core). A caller sweeping angle of attack over one
/// section, as the airfoil screening does, prepares once and evaluates per
/// angle; the result is identical to calling [`aero_from_airfoil`] each time.
pub struct PreparedAirfoil {
    framed: Framed,
}

impl PreparedAirfoil {
    /// Normalize and fit `airfoil`.
    ///
    /// # Errors
    ///
    /// The same as [`aero_from_airfoil`]: the fit refuses a section with too
    /// few vertices or one it cannot fit at full rank.
    pub fn prepare(airfoil: &Airfoil) -> Result<Self, NeuralFoilError> {
        Ok(Self {
            framed: Framed::of(airfoil)?,
        })
    }

    /// The section's fitted Kulfan parameters, in the normalized frame.
    pub fn kulfan(&self) -> &KulfanAirfoil {
        &self.framed.airfoil
    }

    /// Evaluate at `conditions`, exactly what [`aero_from_airfoil`] returns
    /// for the same section and conditions.
    ///
    /// # Errors
    ///
    /// See [`aero_from_kulfan_airfoil`].
    pub fn aero(
        &self,
        conditions: &Conditions,
        mach: f64,
        size: ModelSize,
    ) -> Result<Aero, NeuralFoilError> {
        let framed = &self.framed;
        let mut aero =
            aero_from_kulfan_airfoil(&framed.airfoil, &framed.conditions(conditions), mach, size)?;
        aero.cm += framed.moment_correction(aero.cl, aero.cd);
        Ok(aero)
    }

    /// Evaluate at every condition of a schedule with one batched network
    /// pass, in order, exactly what [`Self::aero`] returns for each.
    ///
    /// # Errors
    ///
    /// See [`aero_from_kulfan_airfoil`].
    pub fn aero_sweep(
        &self,
        conditions: &[Conditions],
        mach: f64,
        size: ModelSize,
    ) -> Result<Vec<Aero>, NeuralFoilError> {
        let framed = &self.framed;
        let framed_conditions: Vec<Conditions> = conditions
            .iter()
            .map(|conditions| framed.conditions(conditions))
            .collect();
        let mut aeros =
            aero_from_kulfan_airfoil_sweep(&framed.airfoil, &framed_conditions, mach, size)?;
        for aero in &mut aeros {
            aero.cm += framed.moment_correction(aero.cl, aero.cd);
        }
        Ok(aeros)
    }
}

/// The raw network on a section given as coordinates:
/// `neuralfoil.get_aero_from_coordinates`, which is what
/// `alas/reporting/visualization.py` calls.
///
/// Note what this does *not* do: there is no Mach number and no post-stall
/// blending on this path, however far the incidence is pushed. See the module
/// documentation.
///
/// # Errors
///
/// [`NeuralFoilError`], as [`aero_from_airfoil`].
pub fn aero_from_coordinates(
    coordinates: &[(f64, f64)],
    conditions: &Conditions,
    size: ModelSize,
) -> Result<NetworkAero, NeuralFoilError> {
    let airfoil = Airfoil::from_coordinates("Untitled", coordinates.to_vec());
    let framed = Framed::of(&airfoil)?;
    let mut aero = network::evaluate(&framed.airfoil, &framed.conditions(conditions), size)?;
    aero.cm += framed.moment_correction(aero.cl, aero.cd);
    Ok(aero)
}

/// A section moved into the frame the network was trained in, together with
/// what has to be undone afterwards.
///
/// Both coordinate-taking entry points build one of these and neither can
/// avoid it: the network expects a unit-chord section at zero incidence, and
/// an arbitrary set of coordinates is neither. The incidence the section
/// carried becomes part of the angle of attack, its chord divides the
/// Reynolds number, and its quarter-chord point, which is where the moment
/// is reported about, moves.
struct Framed {
    airfoil: KulfanAirfoil,
    alpha_offset_deg: f64,
    reynolds_scale: f64,
    x_quarter_chord: f64,
    y_quarter_chord: f64,
}

impl Framed {
    fn of(airfoil: &Airfoil) -> Result<Self, NeuralFoilError> {
        let normalization = airfoil.normalize();
        let fitted = KulfanAirfoil::fit(
            &normalization.airfoil.coordinates,
            WEIGHTS_PER_SIDE,
            0.5,
            1.0,
        )?;
        let rotation = normalization.rotation_angle_deg;
        let scale = normalization.scale_factor;

        Ok(Self {
            airfoil: fitted,
            alpha_offset_deg: rotation,
            reynolds_scale: scale,
            x_quarter_chord: -normalization.x_translation
                + 0.25 * (1.0 / scale * rotation.to_radians().cos())
                - 0.25,
            y_quarter_chord: -normalization.y_translation
                + 0.25 * (1.0 / scale * (-rotation).to_radians().sin()),
        })
    }

    /// The condition as the normalized section sees it.
    fn conditions(&self, conditions: &Conditions) -> Conditions {
        Conditions {
            alpha_deg: conditions.alpha_deg + self.alpha_offset_deg,
            reynolds: conditions.reynolds / self.reynolds_scale,
            ..*conditions
        }
    }

    /// The moment the translation adds, as the force vectors are carried back
    /// to the original section's quarter chord.
    fn moment_correction(&self, cl: f64, cd: f64) -> f64 {
        -cl * self.x_quarter_chord + cd * self.y_quarter_chord
    }
}

/// Fold an angle of attack into `[-180, 180)`.
///
/// Upstream's `np.mod(alpha + 180, 360) - 180`. Python's modulo is floored,
/// so a negative input comes back positive; Rust's `%` truncates, and
/// `rem_euclid` is the floored one.
fn wrap_alpha(alpha_deg: f64) -> f64 {
    (alpha_deg + 180.0).rem_euclid(360.0) - 180.0
}

// A test asserts on sections it constructed here directly, so a failed
// unwrap or expect is the assertion failing, not a library invariant being
// broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).unwrap_or_else(|| panic!("{name} is a 4-digit NACA name"))
    }

    #[test]
    fn an_angle_of_attack_folds_into_the_half_open_range() {
        assert_eq!(wrap_alpha(0.0), 0.0);
        assert_eq!(wrap_alpha(5.0), 5.0);
        assert_eq!(wrap_alpha(-5.0), -5.0);
        assert_eq!(wrap_alpha(200.0), -160.0);
        assert_eq!(wrap_alpha(-200.0), 160.0);
        assert_eq!(wrap_alpha(360.0), 0.0);
        assert_eq!(wrap_alpha(180.0), -180.0);
    }

    #[test]
    fn an_exactly_framed_section_needs_no_correction() {
        // A *symmetric* 4-digit section is exactly framed, unlike the
        // cambered one below: its two trailing-edge points straddle y = 0 and
        // its leading edge is on the origin, so the chord is exactly one.
        let framed = Framed::of(&naca("naca0012")).expect("a fittable section");
        assert_eq!(framed.alpha_offset_deg, 0.0);
        assert_eq!(framed.reynolds_scale, 1.0);
        assert!(framed.moment_correction(1.0, 0.01).abs() < 1e-15);
    }

    #[test]
    fn a_generated_naca_section_is_not_quite_in_the_standard_frame() {
        // Worth stating, because the opposite is the natural assumption: the
        // 4-digit formula lays the thickness distribution perpendicular to a
        // camber line, so a cambered section's two trailing-edge points do
        // not straddle y = 0 and its chord is not exactly one. `naca2412`
        // comes out 0.08 degrees nose-down and 78 parts per million short,
        // which is small and is not zero, so the surrogate's frame
        // correction is doing something on every section this program builds.
        let framed = Framed::of(&naca("naca2412")).expect("a fittable section");
        assert!((framed.alpha_offset_deg - 0.080_247_414_566).abs() < 1e-9);
        assert!((framed.reynolds_scale - 0.999_921_906_121).abs() < 1e-9);
        assert!(framed.moment_correction(1.0, 0.01).abs() > 1e-6);
    }

    #[test]
    fn the_two_entry_points_differ_by_the_corrections_and_not_by_the_network() {
        // At Mach zero and low incidence the compressible path's corrections
        // are almost inert, so the two agree; this is the property that makes
        // the difference between them easy to miss.
        let section = naca("naca2412");
        let conditions = Conditions::new(3.0, 1e6);
        let compressible =
            aero_from_airfoil(&section, &conditions, 0.0, ModelSize::Large).expect("a fit");
        let raw = aero_from_coordinates(&section.coordinates, &conditions, ModelSize::Large)
            .expect("a fit");
        // Within the half per cent the softened Prandtl-Glauert factor costs
        // even at Mach zero, see `corrections`' own test for that.
        assert!((compressible.cl - raw.cl).abs() < 0.01 * raw.cl.abs().max(0.1));
        assert!((compressible.cd - raw.cd).abs() < 1e-4);
    }

    #[test]
    fn only_the_compressible_entry_point_reports_wave_drag() {
        let section = naca("naca2412");
        let conditions = Conditions::new(1.0, 2e7);
        let transonic =
            aero_from_airfoil(&section, &conditions, 0.85, ModelSize::Large).expect("a fit");
        let raw = aero_from_coordinates(&section.coordinates, &conditions, ModelSize::Large)
            .expect("a fit");
        assert!(
            transonic.cd > 10.0 * raw.cd,
            "transonic {} against incompressible {}",
            transonic.cd,
            raw.cd
        );
    }

    #[test]
    fn a_tilted_section_reports_the_lift_the_untilted_one_does_at_the_shifted_incidence() {
        // The frame is undone, not ignored: a section built with two degrees
        // of incidence baked into its coordinates, queried at one degree,
        // should report what the plain section reports at three.
        let section = naca("naca2412");
        let tilted = Airfoil::from_coordinates(
            "tilted",
            section
                .coordinates
                .iter()
                .map(|&(x, y)| {
                    let angle: f64 = -2.0_f64.to_radians();
                    (
                        angle.cos() * x - angle.sin() * y,
                        angle.sin() * x + angle.cos() * y,
                    )
                })
                .collect(),
        );

        let plain = aero_from_airfoil(&section, &Conditions::new(3.0, 1e6), 0.0, ModelSize::Large)
            .expect("a fit");
        let rotated = aero_from_airfoil(&tilted, &Conditions::new(1.0, 1e6), 0.0, ModelSize::Large)
            .expect("a fit");
        assert!(
            (plain.cl - rotated.cl).abs() < 5e-3,
            "plain {} against tilted {}",
            plain.cl,
            rotated.cl
        );
    }

    #[test]
    fn a_section_too_short_to_fit_is_an_error_rather_than_a_panic() {
        let stub = [(1.0, 0.0), (0.0, 0.0), (1.0, -0.0)];
        assert!(matches!(
            aero_from_coordinates(&stub, &Conditions::new(0.0, 1e6), ModelSize::Large),
            Err(NeuralFoilError::Fit(_))
        ));
    }

    #[test]
    fn the_default_conditions_are_the_ones_this_program_never_overrides() {
        let conditions = Conditions::new(2.0, 1e6);
        assert_eq!(conditions.n_crit, 9.0);
        assert_eq!(conditions.xtr_upper, 1.0);
        assert_eq!(conditions.xtr_lower, 1.0);
    }
}
