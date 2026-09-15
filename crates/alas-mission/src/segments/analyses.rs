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
//! The stability analysis is not one of them: `update_stability` runs, and
//! `mission analysis model.Analyses.Stability.Fidelity_Zero` reports nothing any residual, any
//! force or any exported column reads, and the weights analysis is reached
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
use alas_prop::system::{
    FailureState, OperatingMode, PropulsionDemand, PropulsionError, PropulsionLoads,
    PropulsionOrchestrator, PropulsionRating, PropulsionRequest, PropulsionResult, PropulsionState,
    ResourceKind, TechnologyTrace,
};

use crate::operating::ThrustRating;

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

/// Inputs retained solely for the frozen turbofan solver's rejected-iterate
/// compatibility path. Non-turbofan systems never carry this state.
pub struct LegacyTurbofanCompatibility {
    /// Legacy network inputs.
    pub inputs: TurbofanInputs,
    /// Legacy component and part-power parameters.
    pub params: VehicleBuilderParams,
    /// Sized core-flow scale.
    pub compressor_nondimensional_massflow: f64,
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
    /// the drag buildup. The product currently leaves this at `1.0`: the
    /// SUAVE Fidelity-Zero fuselage correction belongs to the aircraft lift
    /// balance, and must not be squared into VLM induced drag without a
    /// separately calibrated load model. Frozen compatibility also sets this
    /// to `1.0`.
    pub induced_drag_lift_correction: f64,
    /// Whether the product solver uses the signed longitudinal force residual
    /// for cruise. The frozen compatibility path retains SUAVE's historical
    /// horizontal-force magnitude residual so its golden fixture remains
    /// reproducible; product missions must preserve the sign so a thrust
    /// deficit cannot look identical to a thrust surplus to the root finder.
    pub signed_cruise_force_residual: bool,
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
    /// Rejected-iterate fallback state carried only by legacy turbofans.
    /// Turboprops and future technologies leave this absent.
    pub legacy_turbofan: Option<LegacyTurbofanCompatibility>,
    /// Immutable technology-neutral propulsion boundary used by every mission
    /// operating-point evaluation. The optional compatibility state is not a
    /// second product model; it only preserves historical nonlinear-solver
    /// behavior when the strict boundary rejects a turbofan probe.
    pub propulsion: PropulsionOrchestrator,
}

impl MissionAnalyses {
    /// Whether named ratings still need the historical scalar schedule.
    pub(crate) fn uses_legacy_propulsion_schedule(&self) -> bool {
        self.legacy_turbofan.is_some()
    }

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
        let request = PropulsionRequest {
            flight: (&freestream).into(),
            demand: PropulsionDemand::NormalizedForce(throttle),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        };
        let result = match self.propulsion.evaluate(&request) {
            Ok(result) => result,
            Err(PropulsionError::InvalidInput { .. } | PropulsionError::NonFiniteOutput(_))
                if self.legacy_turbofan.is_some() =>
            {
                // MINPACK can probe non-finite iterates before returning to
                // the physical solution. The neutral boundary intentionally
                // rejects those points; preserve the frozen solver's NaN
                // propagation only for such rejected compatibility probes.
                let Some(legacy) = self.legacy_turbofan.as_ref() else {
                    unreachable!("guarded by the compatibility-state check");
                };
                return evaluate_thrust(
                    &freestream,
                    &legacy.inputs,
                    &legacy.params,
                    legacy.compressor_nondimensional_massflow,
                    throttle,
                );
            }
            Err(PropulsionError::InvalidInput { .. } | PropulsionError::NonFiniteOutput(_)) => {
                // Nonlinear root solvers may probe outside a typed model's
                // validity domain. Return a rejected numerical point, never
                // a result from another propulsion technology.
                return ThrustOutput {
                    thrust_n: f64::NAN,
                    thrust_specific_fuel_consumption: f64::NAN,
                    non_dimensional_thrust: f64::NAN,
                    core_mass_flow_rate_kg_s: f64::NAN,
                    fuel_flow_rate_kg_s: f64::NAN,
                    power_w: f64::NAN,
                    specific_impulse_s: f64::NAN,
                };
            }
            Err(error) => {
                // A typed model-domain rejection can occur at a nonlinear
                // solver trial point. The mission API predates the neutral
                // Result boundary, so encode the rejected point as a
                // non-finite residual input and let the root solver report a
                // controlled non-convergence instead of panicking the GUI.
                tracing::warn!(
                    error = %error,
                    "mission propulsion rejected a trial operating point"
                );
                return ThrustOutput {
                    thrust_n: f64::NAN,
                    thrust_specific_fuel_consumption: f64::NAN,
                    non_dimensional_thrust: f64::NAN,
                    core_mass_flow_rate_kg_s: f64::NAN,
                    fuel_flow_rate_kg_s: f64::NAN,
                    power_w: f64::NAN,
                    specific_impulse_s: f64::NAN,
                };
            }
        };

        self.project_propulsion_result(result, velocity_m_s, gravity_m_s2)
            .unwrap_or_else(|error| {
                tracing::warn!(error = %error, "mission propulsion projection rejected a trial point");
                rejected_thrust_output()
            })
    }

    /// Evaluate a phase rating through the technology-neutral model.
    ///
    /// Legacy turbofans have no named schedules, so their established rating
    /// fraction is supplied explicitly by the mission configuration. Typed
    /// technologies receive the named rating without reinterpretation.
    #[allow(clippy::too_many_arguments)] // mirrors the existing mission thrust boundary plus rating
    pub fn thrust_for_rating(
        &self,
        atmosphere: &Us1976Values,
        altitude_m: f64,
        velocity_m_s: f64,
        mach: f64,
        gravity_m_s2: f64,
        rating: ThrustRating,
        legacy_rating_fraction: f64,
    ) -> Result<ThrustOutput, PropulsionError> {
        let freestream =
            freestream_from_atmosphere(atmosphere, altitude_m, velocity_m_s, mach, gravity_m_s2);
        let demand = if self.uses_legacy_propulsion_schedule() {
            PropulsionDemand::NormalizedForce(legacy_rating_fraction)
        } else {
            PropulsionDemand::RatedFraction {
                rating: match rating {
                    ThrustRating::TakeoffGoAround => PropulsionRating::TakeoffGoAround,
                    ThrustRating::MaximumClimb => PropulsionRating::MaximumClimb,
                    ThrustRating::MaximumContinuous => PropulsionRating::MaximumContinuous,
                    ThrustRating::FlightIdle => PropulsionRating::FlightIdle,
                    ThrustRating::Cruise => PropulsionRating::Cruise,
                },
                fraction: legacy_rating_fraction,
            }
        };
        let result = self.propulsion.evaluate(&PropulsionRequest {
            flight: (&freestream).into(),
            demand,
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        })?;
        self.project_propulsion_result(result, velocity_m_s, gravity_m_s2)
    }

    fn project_propulsion_result(
        &self,
        result: PropulsionResult,
        velocity_m_s: f64,
        gravity_m_s2: f64,
    ) -> Result<ThrustOutput, PropulsionError> {
        let fuel_flow_rate_kg_s = jet_a_mass_flow(&result.resource_flows)?;
        match result.trace {
            Some(TechnologyTrace::LegacyTurbofan(mut output)) => {
                output.thrust_n = result.body_force_n[0];
                output.fuel_flow_rate_kg_s = fuel_flow_rate_kg_s;
                Ok(output)
            }
            None => {
                let thrust_n = result.body_force_n[0];
                Ok(ThrustOutput {
                    thrust_n,
                    thrust_specific_fuel_consumption: if thrust_n > 0.0 {
                        fuel_flow_rate_kg_s * gravity_m_s2 * 3_600.0 / thrust_n
                    } else {
                        0.0
                    },
                    non_dimensional_thrust: 0.0,
                    core_mass_flow_rate_kg_s: 0.0,
                    fuel_flow_rate_kg_s,
                    power_w: thrust_n * velocity_m_s,
                    specific_impulse_s: if fuel_flow_rate_kg_s > 0.0 {
                        thrust_n / (fuel_flow_rate_kg_s * gravity_m_s2)
                    } else {
                        0.0
                    },
                })
            }
        }
    }
}

// The historical scalar mission API signals rejected solver iterates through
// non-finite residuals. The rated Result API retains the explicit error.
fn rejected_thrust_output() -> ThrustOutput {
    ThrustOutput {
        thrust_n: f64::NAN,
        thrust_specific_fuel_consumption: f64::NAN,
        non_dimensional_thrust: f64::NAN,
        core_mass_flow_rate_kg_s: f64::NAN,
        fuel_flow_rate_kg_s: f64::NAN,
        power_w: f64::NAN,
        specific_impulse_s: f64::NAN,
    }
}

fn jet_a_mass_flow(flows: &[alas_prop::system::ResourceFlow]) -> Result<f64, PropulsionError> {
    flows
        .iter()
        .find(|flow| flow.resource == ResourceKind::JetA)
        .and_then(|flow| flow.mass_flow_kg_s)
        .ok_or(PropulsionError::UnsupportedDemand(
            "mission requires a Jet-A mass-flow result",
        ))
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use alas_prop::system::ResourceFlow;

    #[test]
    fn unsupported_resource_flow_is_an_error_and_rejected_iterate() {
        for resource in [
            ResourceKind::Hydrogen,
            ResourceKind::ElectricalEnergy,
            ResourceKind::Custom("other".into()),
            ResourceKind::JetA,
        ] {
            let flows = [ResourceFlow {
                resource,
                mass_flow_kg_s: None,
                power_w: Some(1.0),
            }];
            assert!(matches!(
                jet_a_mass_flow(&flows),
                Err(PropulsionError::UnsupportedDemand(_))
            ));
        }
        assert!(jet_a_mass_flow(&[]).is_err());
        let rejected = rejected_thrust_output();
        assert!(rejected.thrust_n.is_nan());
        assert!(rejected.fuel_flow_rate_kg_s.is_nan());
    }

    #[test]
    fn jet_a_projection_preserves_positive_consumption_including_zero() {
        for value in [0.0, 0.75] {
            let flows = [ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(value),
                power_w: None,
            }];
            assert!(matches!(jet_a_mass_flow(&flows), Ok(got) if got == value));
        }
    }
}
