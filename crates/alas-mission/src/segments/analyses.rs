// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from external tools/mission_runner/mission_builder.py's `base_analysis`
// and the analysis objects it attaches: mission analysis model.Analyses.Aerodynamics.Fidelity_Zero,
// mission analysis model.Analyses.Energy.Energy over a Turbofan network,
// mission analysis model.Analyses.Atmospheric.US_Standard_1976 and mission analysis model.Analyses.Planets.Planet.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The analysis stack a mission segment evaluates against.
//!
//! `base_analysis` attaches six analyses to every configuration: weights,
//! aerodynamics, stability, energy, a planet and an atmosphere. Four of them
//! are reached from inside a segment iteration and are what this struct holds.
//! The stability analysis is not one of them -- `update_stability` runs, and
//! `mission analysis model.Analyses.Stability.Fidelity_Zero` reports nothing any residual, any
//! force or any exported column reads -- and the weights analysis is reached
//! for exactly one number, the takeoff mass the first segment starts at.
//!
//! # Why the vehicle arrives as data
//!
//! Every field here is read off the *built* mission analysis model vehicle after
//! `simple_sizing` and `finalize` have run, and the fixture records it. There
//! is no port of `vehicle_builder.py`: it is the reference's own external
//! runner rather than a module of the program under translation, and
//! reproducing it would put an untranslated, unchecked geometry assembly
//! underneath every mission number. This is the arrangement
//! `alas-mass::transport_weight`, `alas-aero::drag_buildup` and
//! `alas-aero::vorlax` already use, for the reason `drag_buildup`'s row
//! records at length: a parity test must be handed the inputs the reference
//! used rather than re-derive them through a second model.
//!
//! The lift surrogate arrives the same way, as the sampled tables rather than
//! as a panel method to re-run. `alas-aero::lift_surrogate` fits them here
//! ([`alas_aero::lift_surrogate::LiftSurrogate::from_training`]), which is the
//! half of the surrogate a mission's answers actually depend on; the sampling
//! half is `alas-aero::vorlax`'s own green row.

use alas_aero::drag_buildup::{
    DragBreakdown, DragSettings, DragVehicle, Freestream as DragFreestream, FuselageParams,
    NacelleParams, WingParams,
};
use alas_aero::lift_surrogate::{
    aircraft_lift_coefficient, LiftSolution, LiftSurrogate, SurrogateDomainError,
    SurrogateDomainStatus,
};
use alas_atmo::{us1976_compute_values, us1976_try_compute_values, Us1976Error, Us1976Values};
use alas_prop::mission_turbofan::{
    evaluate_thrust, freestream_from_atmosphere, ThrustOutput, TurbofanInputs, VehicleBuilderParams,
};

/// What one call to the aerodynamics analysis produces.
pub struct AeroSolution {
    /// The aircraft lift coefficient, after the fuselage correction.
    pub lift_coefficient: f64,
    /// The whole drag buildup; its `total` is the drag coefficient.
    pub drag: DragBreakdown,
    /// Each wing's lift coefficient, in the vehicle's wing order.
    pub wing_lift_coefficient: Vec<f64>,
    /// Each wing's inviscid induced drag coefficient, likewise.
    pub wing_induced_drag_coefficient: Vec<f64>,
    /// Whether the surrogate query was inside its trained rectangle.
    ///
    /// The translated mission path retains the reference edge clamp for
    /// parity, but carries this status so callers can reject or label
    /// out-of-domain trajectory points explicitly.
    pub surrogate_domain: SurrogateDomainStatus,
}

/// The analysis stack, resolved onto one aircraft.
pub struct MissionAnalyses {
    /// `geometry.reference_area`, m^2: what every coefficient is on.
    pub reference_area_m2: f64,
    /// `settings.maximum_lift_coefficient`. `None` is upstream's `np.inf`,
    /// which makes the clamp `update_aerodynamics` applies unreachable.
    pub maximum_lift_coefficient: Option<f64>,
    /// `weights.vehicle.mass_properties.takeoff`, kg: the mass the first
    /// segment of the mission begins at.
    pub takeoff_mass_kg: f64,
    /// Lowest physically reachable vehicle mass, kg, after all usable fuel is
    /// consumed. `None` preserves the translated mission analysis model behavior for parity
    /// fixtures that do not carry a fuel-loading contract.
    pub minimum_mass_kg: Option<f64>,
    /// `settings.fuselage_lift_correction`, the 1.14 the wings-only lift is
    /// multiplied by.
    pub fuselage_lift_correction: f64,
    /// Scale applied to per-wing VLM lift and induced-drag coefficients before
    /// the drag buildup. Product analyses may carry the fuselage correction
    /// here when their induced-drag policy follows corrected wing loads.
    /// Frozen SUAVE evidence leaves the VLM drag inputs unchanged and sets
    /// this to `1.0`.
    pub induced_drag_lift_correction: f64,
    /// Whether a solved throttle above the available `[0, 1]` envelope marks
    /// the segment as non-converged. Product mission runs enforce this
    /// physical availability check; the frozen SUAVE compatibility path keeps
    /// the reference solver's converged flag even where its historical engine
    /// sizing produces throttle above one.
    pub enforce_throttle_envelope: bool,
    /// The drag chain's settings.
    pub drag_settings: DragSettings,
    /// The wings. The two lift fields of each are placeholders: they are
    /// overwritten from the surrogate at every evaluation, because that is
    /// where `induced_drag_aircraft` and `compressibility_drag_wing` read
    /// them from upstream.
    pub wings: Vec<WingParams>,
    /// The fuselages.
    pub fuselages: Vec<FuselageParams>,
    /// The nacelles.
    pub nacelles: Vec<NacelleParams>,
    /// How many propulsion networks the vehicle carries.
    pub network_count: usize,
    /// The trained vortex-lattice surrogate.
    pub surrogate: LiftSurrogate,
    /// The engine's variable inputs.
    pub turbofan: TurbofanInputs,
    /// The engine's fixed component efficiencies and losses.
    pub turbofan_params: VehicleBuilderParams,
    /// The core-flow scale factor `turbofan_sizing` solved. The engine is
    /// already sized by the time a mission flies it.
    pub compressor_nondimensional_massflow: f64,
}

impl MissionAnalyses {
    /// `US_Standard_1976.compute_values` at one altitude.
    pub fn atmosphere(&self, altitude_m: f64, temperature_deviation_k: f64) -> Us1976Values {
        us1976_compute_values(altitude_m, temperature_deviation_k)
    }

    /// Checked US1976 atmosphere for callers that cannot accept the legacy
    /// edge-clamping behavior of [`Self::atmosphere`].
    pub fn atmosphere_checked(
        &self,
        altitude_m: f64,
        temperature_deviation_k: f64,
    ) -> Result<Us1976Values, Us1976Error> {
        us1976_try_compute_values(altitude_m, temperature_deviation_k)
    }

    /// `Fidelity_Zero`'s whole `compute` chain at one flight condition.
    ///
    /// Lift first, because the drag chain reads it. Aircraft lift and induced
    /// drag use separate policy inputs: frozen SUAVE evidence corrects the
    /// aircraft lift but consumes VLM induced drag unchanged, while product
    /// callers can opt into a corrected induced load.
    pub fn aerodynamics(
        &self,
        angle_of_attack_rad: f64,
        mach: f64,
        temperature_k: f64,
        reynolds_number_per_m: f64,
    ) -> AeroSolution {
        let lift = self.surrogate.evaluate(angle_of_attack_rad, mach);
        self.aerodynamics_from_lift(lift, mach, temperature_k, reynolds_number_per_m)
    }

    /// Checked variant of [`Self::aerodynamics`] that refuses to use the
    /// surrogate's edge-clamped value outside its trained rectangle.
    pub fn aerodynamics_checked(
        &self,
        angle_of_attack_rad: f64,
        mach: f64,
        temperature_k: f64,
        reynolds_number_per_m: f64,
    ) -> Result<AeroSolution, SurrogateDomainError> {
        let lift = self.surrogate.evaluate_checked(angle_of_attack_rad, mach)?;
        Ok(self.aerodynamics_from_lift(lift, mach, temperature_k, reynolds_number_per_m))
    }

    fn aerodynamics_from_lift(
        &self,
        lift: LiftSolution,
        mach: f64,
        temperature_k: f64,
        reynolds_number_per_m: f64,
    ) -> AeroSolution {
        let lift_scale = self.fuselage_lift_correction;
        let drag_lift_scale = self.induced_drag_lift_correction;
        let induced_drag_scale = drag_lift_scale * drag_lift_scale;
        let wings: Vec<WingParams> = self
            .wings
            .iter()
            .zip(&lift.wing_lift_coefficient)
            .zip(&lift.wing_induced_drag_coefficient)
            .map(|((wing, &wing_lift), &wing_drag)| WingParams {
                inviscid_lift_coefficient: wing_lift * drag_lift_scale,
                inviscid_induced_drag_coefficient: wing_drag * induced_drag_scale,
                ..*wing
            })
            .collect();

        let vehicle = DragVehicle {
            reference_area_m2: self.reference_area_m2,
            wings: &wings,
            fuselages: &self.fuselages,
            nacelles: &self.nacelles,
            network_count: self.network_count,
        };
        let freestream = DragFreestream {
            mach,
            temperature_k,
            reynolds_number_per_m,
        };
        let drag = alas_aero::drag_buildup::evaluate(&self.drag_settings, &freestream, &vehicle);

        AeroSolution {
            lift_coefficient: aircraft_lift_coefficient(lift.inviscid_lift_coefficient, lift_scale),
            drag,
            wing_lift_coefficient: lift
                .wing_lift_coefficient
                .into_iter()
                .map(|value| value * drag_lift_scale)
                .collect(),
            wing_induced_drag_coefficient: lift
                .wing_induced_drag_coefficient
                .into_iter()
                .map(|value| value * induced_drag_scale)
                .collect(),
            surrogate_domain: lift.domain,
        }
    }

    /// `Energy.evaluate_thrust` at one flight condition and throttle.
    ///
    /// The atmosphere is handed in rather than recomputed from the altitude:
    /// it is the one the segment already put on its conditions, and the
    /// velocity is the segment's own rather than `a * mach`, which is a
    /// different double.
    pub fn thrust(
        &self,
        atmosphere: &Us1976Values,
        altitude_m: f64,
        velocity_m_s: f64,
        mach: f64,
        gravity_m_s2: f64,
        throttle: f64,
    ) -> ThrustOutput {
        let freestream =
            freestream_from_atmosphere(atmosphere, altitude_m, velocity_m_s, mach, gravity_m_s2);
        evaluate_thrust(
            &freestream,
            &self.turbofan,
            &self.turbofan_params,
            self.compressor_nondimensional_massflow,
            throttle,
        )
    }
}
