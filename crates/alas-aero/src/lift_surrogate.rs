// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Aerodynamics/Vortex_Lattice.py's
// sample_training, build_surrogate and evaluate_surrogate, and the
// fuselage_correction/aircraft_total tail of Fidelity_Zero's lift chain.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The lift and induced-drag surrogate the mission actually flies on.
//!
//! `mission analysis model.Analyses.Aerodynamics.Vortex_Lattice` does not run a vortex lattice
//! per flight condition. It runs [`crate::vorlax`] **once**, on a fixed grid
//! of ten angles of attack against eight Mach numbers, fits a bicubic spline
//! through the result, and every lift and induced-drag number a mission
//! segment then sees is an evaluation of that spline. So this is not an
//! optimization of the solver: it is the model. A knot placed differently is
//! a different aeroplane, which is why [`alas_math::BicubicSpline`] exists as
//! its own green row rather than as "a bicubic through the same points".
//!
//! This row is also what closes `alas-aero::drag_buildup`'s open input. That
//! module takes each wing's lift coefficient and inviscid induced drag
//! coefficient as data, because with `span_efficiency` at its `None` default
//! the inviscid induced drag *is* the vortex lattice's and there is no
//! closed-form fallback on the path this program takes. [`LiftSolution`] is
//! what supplies them.
//!
//! # Scope
//!
//! `Vortex_Lattice` carries three regimes -- subsonic, supersonic, and a
//! transonic interpolant blended between them by a `Cubic_Spline_Blender`
//! over two Mach bands. Only the first is reached.
//! `Vortex_Lattice.__defaults__` trains on sixteen Mach numbers of which
//! eight are supersonic, and `Fidelity_Zero.__defaults__` then **overrides
//! that grid** with `[0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]`. With the
//! supersonic training block empty, `build_surrogate` builds neither the
//! supersonic nor the transonic surface, and `evaluate_surrogate` takes its
//! `CL_surrogate_sup == None` branch -- one spline evaluation, no blending.
//! `gen_aero_lift_surrogate.py` refuses to write a fixture in which either
//! absent surrogate has become a surface.
//!
//! Above Mach 0.9 the answer is therefore the value at Mach 0.9, because
//! FITPACK clamps its argument to the boundary knot rather than continuing
//! the edge polynomial. That is not an approximation this port introduces; it
//! is what the reference computes, and the difference between a clamp and a
//! cubic extrapolation grows without bound. The fixture evaluates outside the
//! training rectangle on all four sides for exactly that reason.
//!
//! `evaluate_no_surrogate`, which runs the vortex lattice per condition and
//! reports sectional loads and pressures, is not translated:
//! `Fidelity_Zero.__defaults__` sets `use_surrogate = True` and the mission
//! runner overrides nothing, so `initialize` never binds it.

use std::collections::BTreeMap;

use alas_math::{BicubicSpline, BicubicSplineError};

use crate::vorlax::{self, VlmError, VlmGeometry, VlmSettings};

/// The angles of attack `Vortex_Lattice.__defaults__` trains at, in degrees.
///
/// The last two are far past any attached-flow regime a panel method can
/// speak about. They are upstream's, and they are load-bearing: they are what
/// stop the spline's outermost interval from being extrapolated into by a
/// mission that reaches a high angle of attack on rotation.
const TRAINING_ANGLES_DEG: [f64; 10] = [-5.0, -2.0, 0.0, 2.0, 5.0, 8.0, 10.0, 12.0, 45.0, 75.0];

/// The Mach numbers `Fidelity_Zero.__defaults__` overrides
/// `Vortex_Lattice`'s sixteen with. Subsonic throughout, which is what makes
/// the supersonic and transonic branches unreachable.
const TRAINING_MACH: [f64; 8] = [0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9];

/// `Fidelity_Zero.__defaults__`'s allowance for the lift a fuselage carries
/// that a wings-only panel method cannot see.
///
/// Source: adg.stanford.edu, the Stanford AA241 course notes, by way of
/// upstream's own citation.
pub const FUSELAGE_LIFT_CORRECTION: f64 = 1.14;

/// The grid the surrogate is fitted through.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingGrid {
    /// Angles of attack, in radians and increasing.
    pub angle_of_attack_rad: Vec<f64>,
    /// Mach numbers, increasing and all below one.
    pub mach: Vec<f64>,
}

impl Default for TrainingGrid {
    fn default() -> Self {
        Self {
            angle_of_attack_rad: TRAINING_ANGLES_DEG
                .iter()
                .map(|degrees| degrees.to_radians())
                .collect(),
            mach: TRAINING_MACH.to_vec(),
        }
    }
}

/// What the surrogate answers at one flight condition.
#[derive(Debug, Clone, PartialEq)]
pub struct LiftSolution {
    /// The whole aircraft's inviscid lift coefficient, before the fuselage
    /// allowance.
    pub inviscid_lift_coefficient: f64,
    /// The whole aircraft's inviscid induced drag coefficient.
    pub inviscid_induced_drag_coefficient: f64,
    /// Each wing's lift coefficient, on that wing's own reference area, in
    /// the order the geometry holds its wings.
    pub wing_lift_coefficient: Vec<f64>,
    /// Each wing's inviscid induced drag coefficient, likewise.
    pub wing_induced_drag_coefficient: Vec<f64>,
}

/// Why a surrogate could not be trained.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SurrogateError {
    /// The vortex lattice could not solve the training grid.
    #[error("the training grid could not be solved: {0}")]
    Solve(#[from] VlmError),
    /// A training table could not be fitted.
    #[error("the {quantity} surface could not be fitted: {source}")]
    Fit {
        /// Which table failed.
        quantity: String,
        /// What the fit reported.
        #[source]
        source: BicubicSplineError,
    },
    /// The grid reaches Mach 1 or beyond, where the supersonic and transonic
    /// branches this row does not translate would be selected.
    #[error("the training grid reaches Mach {mach}; only the subsonic branch is translated")]
    Supersonic {
        /// The offending Mach number.
        mach: f64,
    },
    /// [`LiftSurrogate::from_training`] was given a wing tag its tables do not
    /// carry. Unreachable from [`LiftSurrogate::train`], which builds both
    /// from the same geometry.
    #[error("no {quantity} table for wing {tag}")]
    MissingWingTable {
        /// The wing whose table was absent.
        tag: String,
        /// Which of the two tables was missing.
        quantity: String,
    },
}

/// The trained surrogate: two aircraft surfaces and two per wing.
#[derive(Debug, Clone)]
pub struct LiftSurrogate {
    grid: TrainingGrid,
    wing_tags: Vec<String>,
    training: TrainingTables,
    lift: BicubicSpline,
    drag: BicubicSpline,
    wing_lift: Vec<BicubicSpline>,
    wing_drag: Vec<BicubicSpline>,
}

/// The tables the surfaces were fitted through, kept so a caller can compare
/// the fit against the samples it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingTables {
    /// Aircraft lift coefficient, `[angle of attack][Mach]`.
    pub lift_coefficient: Vec<Vec<f64>>,
    /// Aircraft inviscid induced drag coefficient, likewise.
    pub drag_coefficient: Vec<Vec<f64>>,
    /// Per-wing lift coefficient, by wing tag.
    pub wing_lift_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
    /// Per-wing inviscid induced drag coefficient, by wing tag.
    pub wing_drag_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
}

impl LiftSurrogate {
    /// Run the vortex lattice over the grid and fit the surfaces.
    ///
    /// This is `sample_training` followed by `build_surrogate`, and it is one
    /// call because upstream's `initialize` makes it one: nothing ever
    /// samples without fitting.
    ///
    /// # Errors
    ///
    /// See [`SurrogateError`].
    pub fn train(
        geometry: &VlmGeometry,
        settings: &VlmSettings,
        grid: &TrainingGrid,
    ) -> Result<Self, SurrogateError> {
        if let Some(&mach) = grid.mach.iter().find(|&&mach| mach >= 1.0) {
            return Err(SurrogateError::Supersonic { mach });
        }

        let n_alpha = grid.angle_of_attack_rad.len();
        let n_mach = grid.mach.len();

        // The grid is flattened Mach-major: `sample_training` tiles the angle
        // vector once per Mach number and the Mach vector once per angle, so
        // the solve sees every angle at Mach 0 before it sees any at Mach
        // 0.1. The reshape below undoes it, and undoing it the other way
        // round produces a surface that is smooth, plausible and transposed.
        //
        // The velocity is left at zero, which is what upstream's conditions
        // carry: `VLM` substitutes 1e-6 for it under `use_surrogate`, and the
        // rotation rates are all zero, so nothing depends on the number.
        let conditions: Vec<vorlax::VlmCondition> = grid
            .mach
            .iter()
            .flat_map(|&mach| {
                grid.angle_of_attack_rad
                    .iter()
                    .map(move |&angle_of_attack_rad| vorlax::VlmCondition {
                        angle_of_attack_rad,
                        mach,
                        side_slip_angle_rad: 0.0,
                        pitch_rate_rad_s: 0.0,
                        roll_rate_rad_s: 0.0,
                        yaw_rate_rad_s: 0.0,
                        velocity_m_s: 0.0,
                    })
            })
            .collect();

        let results = vorlax::run(geometry, settings, &conditions)?;
        let areas = &results.distribution.wing_areas_m2;

        let reshape = |flat: &dyn Fn(usize) -> f64| -> Vec<Vec<f64>> {
            (0..n_alpha)
                .map(|i| (0..n_mach).map(|j| flat(j * n_alpha + i)).collect())
                .collect()
        };

        let lift_table = reshape(&|k| results.cases[k].cl);
        let drag_table = reshape(&|k| results.cases[k].cdi);

        // `calculate_VLM`'s regrouping: a symmetric wing occupies two columns
        // of the per-surface arrays. They are dimensionalized on the surfaces'
        // own areas, summed, and divided by the wing's reference area, which
        // is what makes a wing's coefficient comparable across the two.
        let mut wing_lift_tables = BTreeMap::new();
        let mut wing_drag_tables = BTreeMap::new();
        let mut wing_tags = Vec::with_capacity(geometry.wings.len());
        let mut surface = 0usize;
        for wing in &geometry.wings {
            let count = if wing.symmetric { 2 } else { 1 };
            let sides = surface..surface + count;
            let lift = reshape(&|k| {
                sides
                    .clone()
                    .map(|s| results.cases[k].cl_wing[s] * f64::from(areas[s]))
                    .sum::<f64>()
                    / wing.area_reference_m2
            });
            let drag = reshape(&|k| {
                sides
                    .clone()
                    .map(|s| results.cases[k].cdi_wing[s] * f64::from(areas[s]))
                    .sum::<f64>()
                    / wing.area_reference_m2
            });
            wing_lift_tables.insert(wing.tag.clone(), lift);
            wing_drag_tables.insert(wing.tag.clone(), drag);
            wing_tags.push(wing.tag.clone());
            surface += count;
        }

        Self::from_training(
            grid,
            &wing_tags,
            &TrainingTables {
                lift_coefficient: lift_table,
                drag_coefficient: drag_table,
                wing_lift_coefficient: wing_lift_tables,
                wing_drag_coefficient: wing_drag_tables,
            },
        )
    }

    /// Fit the surfaces through tables that have already been sampled.
    ///
    /// This is upstream's `build_surrogate` on its own: the half of
    /// [`Self::train`] that turns the sampled grid into splines, without the
    /// vortex-lattice sweep that produced it. `initialize` never calls one
    /// without the other, so this is not a second way to reach a surrogate --
    /// it is the seam that lets a caller *supply* the samples instead of
    /// solving for them.
    ///
    /// `alas-mission`'s segment solver is why it exists. A mission run has to
    /// be handed the tables the reference trained on rather than resampling
    /// them, for the reason `alas-aero::drag_buildup`'s row records: a parity
    /// test that re-derives an input through a second model reports that
    /// model's disagreement as its own. The vortex lattice's agreement is
    /// [`Self::train`]'s claim and `alas-aero::vorlax`'s, checked there.
    ///
    /// # Errors
    ///
    /// [`SurrogateError::Supersonic`] if the grid reaches Mach 1, and
    /// [`SurrogateError::Fit`] naming the table whose fit failed. A table
    /// whose shape does not match the grid fails as a [`SurrogateError::Fit`]
    /// on that table, since that is what the fit reports.
    pub fn from_training(
        grid: &TrainingGrid,
        wing_tags: &[String],
        training: &TrainingTables,
    ) -> Result<Self, SurrogateError> {
        if let Some(&mach) = grid.mach.iter().find(|&&mach| mach >= 1.0) {
            return Err(SurrogateError::Supersonic { mach });
        }

        let fit = |quantity: &str, table: &[Vec<f64>]| {
            BicubicSpline::interpolate(&grid.angle_of_attack_rad, &grid.mach, table).map_err(
                |source| SurrogateError::Fit {
                    quantity: quantity.to_owned(),
                    source,
                },
            )
        };

        let missing = |tag: &str, quantity: &str| SurrogateError::MissingWingTable {
            tag: tag.to_owned(),
            quantity: quantity.to_owned(),
        };

        let lift = fit("lift_coefficient", &training.lift_coefficient)?;
        let drag = fit("drag_coefficient", &training.drag_coefficient)?;
        let mut wing_lift = Vec::with_capacity(wing_tags.len());
        let mut wing_drag = Vec::with_capacity(wing_tags.len());
        for tag in wing_tags {
            let lift_table = training
                .wing_lift_coefficient
                .get(tag)
                .ok_or_else(|| missing(tag, "lift_coefficient"))?;
            let drag_table = training
                .wing_drag_coefficient
                .get(tag)
                .ok_or_else(|| missing(tag, "drag_coefficient"))?;
            wing_lift.push(fit(&format!("{tag} lift_coefficient"), lift_table)?);
            wing_drag.push(fit(&format!("{tag} drag_coefficient"), drag_table)?);
        }

        Ok(Self {
            grid: grid.clone(),
            wing_tags: wing_tags.to_vec(),
            training: training.clone(),
            lift,
            drag,
            wing_lift,
            wing_drag,
        })
    }

    /// Evaluate the surrogate at one flight condition.
    ///
    /// Outside the training rectangle the answer is the value on the nearest
    /// edge, not an extrapolation of it. The module doc says why.
    pub fn evaluate(&self, angle_of_attack_rad: f64, mach: f64) -> LiftSolution {
        LiftSolution {
            inviscid_lift_coefficient: self.lift.evaluate(angle_of_attack_rad, mach),
            inviscid_induced_drag_coefficient: self.drag.evaluate(angle_of_attack_rad, mach),
            wing_lift_coefficient: self
                .wing_lift
                .iter()
                .map(|spline| spline.evaluate(angle_of_attack_rad, mach))
                .collect(),
            wing_induced_drag_coefficient: self
                .wing_drag
                .iter()
                .map(|spline| spline.evaluate(angle_of_attack_rad, mach))
                .collect(),
        }
    }

    /// The grid this surrogate was fitted through.
    pub fn grid(&self) -> &TrainingGrid {
        &self.grid
    }

    /// The wing tags, in the order [`LiftSolution`]'s per-wing vectors use.
    pub fn wing_tags(&self) -> &[String] {
        &self.wing_tags
    }

    /// The sampled tables the surfaces were fitted through.
    pub fn training(&self) -> &TrainingTables {
        &self.training
    }

    /// The aircraft lift surface, for a caller that wants its knots.
    pub fn lift_surface(&self) -> &BicubicSpline {
        &self.lift
    }

    /// The aircraft induced-drag surface.
    pub fn drag_surface(&self) -> &BicubicSpline {
        &self.drag
    }

    /// One wing's lift surface, by position in [`Self::wing_tags`].
    pub fn wing_lift_surface(&self, wing: usize) -> &BicubicSpline {
        &self.wing_lift[wing]
    }

    /// One wing's induced-drag surface.
    pub fn wing_drag_surface(&self, wing: usize) -> &BicubicSpline {
        &self.wing_drag[wing]
    }
}

/// The rest of `Fidelity_Zero`'s lift chain, which is one multiplication.
///
/// `compute.lift.vortex` is `mission analysis model.Methods.skip`, `compute.lift.fuselage` is
/// `fuselage_correction` -- the whole of which is
/// `CL * settings.fuselage_lift_correction`, overwriting the same field --
/// and `compute.lift.total` is `aircraft_total`, which returns what it was
/// handed. They are here rather than in a module of their own because
/// together they are three lines, and because the correction factor is the
/// one number in them a reader would want to find.
pub fn aircraft_lift_coefficient(inviscid_lift_coefficient: f64, fuselage_correction: f64) -> f64 {
    inviscid_lift_coefficient * fuselage_correction
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vorlax::VlmWing;

    fn probe_geometry() -> VlmGeometry {
        let wing = |tag: &str, symmetric: bool, vertical: bool, origin: [f64; 3]| VlmWing {
            tag: tag.to_string(),
            symmetric,
            vertical,
            vortex_lift: false,
            span_projected_m: 10.0,
            chord_root_m: 2.0,
            chord_tip_m: 1.0,
            taper: 0.5,
            aspect_ratio: 100.0 / 7.5,
            sweep_quarter_chord_rad: 0.2,
            sweep_leading_edge_rad: None,
            twist_root_rad: 0.02,
            twist_tip_rad: -0.01,
            dihedral_rad: 0.0,
            area_reference_m2: 7.5,
            origin_m: origin,
        };
        VlmGeometry {
            reference_area_m2: 7.5,
            center_of_gravity_m: [0.0, 0.0, 0.0],
            mean_aerodynamic_chord_m: 1.6,
            reference_span_m: 10.0,
            moment_reference_m: [0.5, 0.0],
            wings: vec![
                wing("main_wing", true, false, [0.0, 0.0, 0.0]),
                wing("fin", false, true, [8.0, 0.0, 0.0]),
            ],
        }
    }

    /// A coarser grid than the production one, so the unit tests stay cheap.
    /// Four points is the fewest a cubic can be fitted through.
    fn coarse_grid() -> TrainingGrid {
        TrainingGrid {
            angle_of_attack_rad: vec![
                (-4.0f64).to_radians(),
                0.0,
                4.0f64.to_radians(),
                8.0f64.to_radians(),
                12.0f64.to_radians(),
            ],
            mach: vec![0.0, 0.2, 0.5, 0.8],
        }
    }

    fn trained() -> LiftSurrogate {
        LiftSurrogate::train(
            &probe_geometry(),
            &VlmSettings {
                number_spanwise_vortices: 4,
                number_chordwise_vortices: 2,
                ..Default::default()
            },
            &coarse_grid(),
        )
        .expect("the probe geometry trains")
    }

    #[test]
    fn the_surface_passes_through_every_point_it_was_fitted_through() {
        let surrogate = trained();
        let grid = coarse_grid();
        for (i, &alpha) in grid.angle_of_attack_rad.iter().enumerate() {
            for (j, &mach) in grid.mach.iter().enumerate() {
                let solution = surrogate.evaluate(alpha, mach);
                let sampled = surrogate.training().lift_coefficient[i][j];
                assert!(
                    (solution.inviscid_lift_coefficient - sampled).abs() < 1e-9,
                    "interpolation, not approximation: the fit is at zero smoothing"
                );
            }
        }
    }

    #[test]
    fn outside_the_training_rectangle_the_answer_is_the_edge_value() {
        let surrogate = trained();
        let grid = coarse_grid();
        let (lowest, highest) = (grid.mach[0], grid.mach[grid.mach.len() - 1]);
        let alpha = 4.0f64.to_radians();

        let at_edge = surrogate.evaluate(alpha, highest).inviscid_lift_coefficient;
        let beyond = surrogate.evaluate(alpha, 3.0).inviscid_lift_coefficient;
        assert!(
            (beyond - at_edge).abs() < 1e-12,
            "clamped, not extrapolated"
        );

        let at_floor = surrogate.evaluate(alpha, lowest).inviscid_lift_coefficient;
        let below = surrogate.evaluate(alpha, -1.0).inviscid_lift_coefficient;
        assert!((below - at_floor).abs() < 1e-12);
    }

    #[test]
    fn the_training_grid_is_flattened_mach_major_and_reshaped_back() {
        // A transposed reshape would still produce a smooth surface, so the
        // check is that lift grows with angle of attack at fixed Mach and
        // barely moves with Mach at fixed angle -- which is the wrong way
        // round for a transposed table.
        let surrogate = trained();
        let table = &surrogate.training().lift_coefficient;
        for row in table.windows(2) {
            assert!(
                row[1][0] > row[0][0],
                "lift grows down the angle-of-attack axis"
            );
        }
        let span_across_mach = table[0][table[0].len() - 1] - table[0][0];
        let span_across_alpha = table[table.len() - 1][0] - table[0][0];
        assert!(span_across_alpha.abs() > span_across_mach.abs() * 5.0);
    }

    #[test]
    fn a_supersonic_training_grid_is_refused_rather_than_silently_mis_evaluated() {
        let mut grid = coarse_grid();
        grid.mach.push(1.5);
        let error =
            LiftSurrogate::train(&probe_geometry(), &VlmSettings::default(), &grid).unwrap_err();
        assert_eq!(error, SurrogateError::Supersonic { mach: 1.5 });
    }

    #[test]
    fn the_default_grid_is_the_one_fidelity_zero_overrides_vortex_lattice_with() {
        let grid = TrainingGrid::default();
        assert_eq!(grid.mach, vec![0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]);
        assert_eq!(grid.angle_of_attack_rad.len(), 10);
        assert!(
            grid.mach.iter().all(|&mach| mach < 1.0),
            "the whole reason the supersonic branch is unreachable"
        );
    }

    #[test]
    fn the_fuselage_allowance_is_a_scale_on_the_wings_only_lift() {
        assert_eq!(
            aircraft_lift_coefficient(0.5, FUSELAGE_LIFT_CORRECTION),
            0.5 * 1.14
        );
    }

    #[test]
    fn each_wing_reports_a_coefficient_on_its_own_reference_area() {
        let surrogate = trained();
        assert_eq!(surrogate.wing_tags(), ["main_wing", "fin"]);
        let solution = surrogate.evaluate(4.0f64.to_radians(), 0.3);
        assert_eq!(solution.wing_lift_coefficient.len(), 2);
        // The fin is vertical, so it carries no lift in the aircraft's own
        // sense; the main wing carries essentially all of it.
        assert!(solution.wing_lift_coefficient[0] > 0.1);
        assert!(solution.wing_lift_coefficient[1].abs() < 1e-3);
    }
}
