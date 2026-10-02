// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A cruise polar supplied by an external aerodynamic solver and the
//! condition identity it may be flown at.

/// How closely an [`ExternalPolar`] must have been evaluated at a
/// candidate's own cruise condition to be flown by it.
///
/// The defaults are numerical-identity tolerances, not modelling slack: the
/// external run is expected to have been commanded at exactly the condition
/// being sized, so anything larger than solver round-tripping noise means a
/// different operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarConditionTolerance {
    /// Absolute Mach tolerance, dimensionless. `1e-9` admits only the
    /// text round-trip of a commanded Mach through a solver input deck.
    pub mach: f64,
    /// Absolute altitude tolerance, m. One metre changes ISA density by
    /// about `1e-4` relative at cruise, already below the drag model's
    /// fidelity, and no solver deck carries sub-metre altitude.
    pub altitude_m: f64,
    /// Relative reference-area tolerance, dimensionless. The geometry
    /// builder's own rounding moves the built area by about `7e-6` relative
    /// at the default design (see `types::NUMERICAL_SLACK`), so `1e-5` accepts a
    /// rebuild of the same design and rejects a different wing.
    pub relative_area: f64,
}

impl Default for PolarConditionTolerance {
    fn default() -> Self {
        Self {
            mach: 1.0e-9,
            altitude_m: 1.0,
            relative_area: 1.0e-5,
        }
    }
}

/// A cruise drag polar supplied by an external aerodynamic solver, so the
/// sizing loop can close a candidate around aerodynamics it did not trim
/// itself. `induced_factor_k` is the parabolic-polar factor
/// `(cd - cd0) / cl^2` at the cruise lift coefficient.
///
/// The coefficients are meaningless without the state they were solved at,
/// so the evaluation condition (`mach`, `altitude_m`, `reference_area_m2`,
/// `target_cl`) and the solver identity travel with them: a polar solved for
/// one wing area cannot be non-dimensionally reused on another.
///
/// Units: areas m^2, altitudes m (ISA geometric), angles degrees, stations m
/// in geometry axes (x positive aft); Mach and every coefficient are
/// dimensionless.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExternalPolar {
    /// Zero-lift drag coefficient at the cruise point.
    pub cd0: f64,
    /// Induced-drag factor `k`, positive.
    pub induced_factor_k: f64,
    /// Wave-drag coefficient at the external solver's design Mach and lift.
    /// It remains separate from `induced_factor_k` so the shared mission
    /// model can omit transonic wave drag during takeoff, climb, descent and
    /// landing phases.
    pub wave_drag_cd: f64,
    /// Lift-to-drag ratio at the required cruise lift.
    pub lift_to_drag: f64,
    /// Angle of attack at that point, degrees, for the history.
    pub alpha_deg: f64,
    /// Stabilizer incidence the point was evaluated at, degrees.
    pub incidence_deg: f64,
    /// Neutral-point station in geometry axes, m, for the balance family.
    pub x_np: f64,
    /// Free-stream Mach the external solver evaluated the point at.
    /// Subsonic: the linear panel methods this admits are invalid at `M >= 1`.
    pub mach: f64,
    /// ISA geometric altitude the point is referred to, m. A panel solver
    /// carries no atmosphere itself, so this is the altitude the required
    /// lift coefficient and the parasite build-up were computed at.
    pub altitude_m: f64,
    /// Wing reference area the coefficients are non-dimensionalized by, m^2.
    pub reference_area_m2: f64,
    /// Lift coefficient the polar was evaluated/interpolated at, positive.
    pub target_cl: f64,
    /// Identity of the solver that produced the point, e.g. `"avl"`.
    pub source: &'static str,
    /// Whether `target_cl` fell inside the solved polar's lift range, so the
    /// point is an interpolation rather than an extrapolation.
    pub bracketed: bool,
}

impl ExternalPolar {
    /// Whether every term is finite, physically usable, and carries a
    /// complete, non-extrapolated evaluation identity.
    pub fn is_valid(&self) -> bool {
        self.cd0.is_finite()
            && self.cd0 > 0.0
            && self.induced_factor_k.is_finite()
            && self.induced_factor_k > 0.0
            && self.wave_drag_cd.is_finite()
            && self.wave_drag_cd >= 0.0
            && self.lift_to_drag.is_finite()
            && self.lift_to_drag > 0.0
            && self.alpha_deg.is_finite()
            && self.incidence_deg.is_finite()
            && self.x_np.is_finite()
            && self.mach.is_finite()
            && self.mach > 0.0
            && self.mach < 1.0
            && self.altitude_m.is_finite()
            && self.reference_area_m2.is_finite()
            && self.reference_area_m2 > 0.0
            && self.target_cl.is_finite()
            && self.target_cl > 0.0
            && !self.source.is_empty()
            && self.bracketed
    }

    /// Whether this polar was evaluated at the condition a candidate is being
    /// sized at.
    ///
    /// # Errors
    ///
    /// A human-readable description naming the mismatched quantity, both
    /// values and the tolerance, when the Mach, altitude or reference area
    /// differs by more than `tolerance`. A polar reused across conditions
    /// silently rescales every coefficient, so this is a rejection rather
    /// than a warning.
    pub fn matches_condition(
        &self,
        mach: f64,
        altitude_m: f64,
        reference_area_m2: f64,
        tolerance: PolarConditionTolerance,
    ) -> Result<(), String> {
        if !mach.is_finite() || !altitude_m.is_finite() || !reference_area_m2.is_finite() {
            return Err(format!(
                "candidate condition is not finite: mach={mach}, altitude_m={altitude_m}, reference_area_m2={reference_area_m2}"
            ));
        }
        if (self.mach - mach).abs() > tolerance.mach {
            return Err(format!(
                "polar mach {} does not match candidate mach {mach} within {}",
                self.mach, tolerance.mach
            ));
        }
        if (self.altitude_m - altitude_m).abs() > tolerance.altitude_m {
            return Err(format!(
                "polar altitude {} m does not match candidate altitude {altitude_m} m within {} m",
                self.altitude_m, tolerance.altitude_m
            ));
        }
        let area_scale = reference_area_m2.abs().max(1e-9);
        if (self.reference_area_m2 - reference_area_m2).abs() / area_scale > tolerance.relative_area
        {
            return Err(format!(
                "polar reference area {} m2 does not match candidate area {reference_area_m2} m2 within {} relative",
                self.reference_area_m2, tolerance.relative_area
            ));
        }
        Ok(())
    }
}
