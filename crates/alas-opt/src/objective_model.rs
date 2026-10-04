// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scalar objective function for aircraft design space optimization.
//!
//! Evaluates aerodynamic performance (L/D) alongside physical constraints
//! (stability, CG envelope, tank capacity, cabin sizing, geometry bounds).

use alas_config::cabin::annotate_flops_cabin_resolution;
use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_payload::build::{apply_cabin_preset, CabinPresetError};

use crate::history::OptimizationHistory;

/// Torenbeek geometric wing fuel-tank volume estimate in cubic meters.
pub fn wing_fuel_volume_m3(wing: &Wing, usable_fraction: f64) -> f64 {
    wing_fuel_volume_m3_with_references(
        wing,
        usable_fraction,
        wing.reference_area(),
        wing.reference_span(),
    )
}

/// Frozen translation/parity form of [`wing_fuel_volume_m3`].
///
/// The reference correlation consumes the wing's unfolded YZ area
/// and span.  Keep that choice behind an explicitly named seam so parity
/// fixtures cannot silently change the product tank-volume calculation.
pub fn wing_fuel_volume_m3_reference_compatibility(wing: &Wing, usable_fraction: f64) -> f64 {
    wing_fuel_volume_m3_with_references(
        wing,
        usable_fraction,
        wing.unfolded_area(),
        wing.unfolded_span(),
    )
}

fn wing_fuel_volume_m3_with_references(wing: &Wing, usable_fraction: f64, s: f64, b: f64) -> f64 {
    let taper = wing.taper_ratio();
    let sample = linspace(0.0, 1.0, 101);
    let t_over_c_root = if !wing.xsecs.is_empty() {
        wing.xsecs[0].airfoil.max_thickness(&sample)
    } else {
        0.12
    };
    let term_taper = (1.0 + taper + taper.powi(2)) / (1.0 + taper).powi(2);
    let v_geo = 0.54 * (s.powi(2) / b.max(1e-6)) * t_over_c_root * term_taper;
    v_geo * usable_fraction.clamp(0.0, 1.0)
}

/// [`apply_candidate_payload_load_case`] on a run that has already finished.
///
/// The winner was scored under its own load case, so the result stands if
/// restoring that case on the run's configuration fails; the configuration
/// then still describes the previous payload, which is reported rather than
/// dropped.
pub(crate) fn restore_winning_payload_load_case(
    config: &mut AlasConfig,
    design_vector: &DesignVector,
) {
    if let Err(error) = apply_candidate_payload_load_case(config, design_vector) {
        tracing::warn!(
            %error,
            "the winning design's payload load case could not be restored on the run configuration"
        );
    }
}

/// Resolve a geometry-driven payload only when the configured load case asks.
///
/// Passenger capacity is always dynamic: for every study, registered
/// aircraft or clean-sheet alike, it is whatever the configured cabin
/// class-mix percentages and the candidate's actual fuselage/cabin geometry
/// produce. There is no fixed/exact passenger-count target that overrides
/// that solve; a hard floor on the resolved count is instead a configurable
/// constraint (see [`alas_config::DesignRequirements::min_passenger_capacity`]
/// and the `passenger_shortfall` residual in `mdo::residuals_geometry`).
/// Cargo presets retain their existing capacity-derived behavior; cargo's
/// `cargo_payload_kg` load-case target is a separate mechanism, untouched
/// here.
pub(crate) fn apply_candidate_payload_load_case(
    config: &mut AlasConfig,
    design_vector: &DesignVector,
) -> Result<(), CabinPresetError> {
    if !config
        .requirements
        .resolves_payload_from_candidate_geometry()
    {
        return Ok(());
    }
    let target_cargo_kg = config.requirements.cargo_payload_kg;
    let passenger_mass_kg = config.requirements.passenger_mass_kg;
    if config.requirements.aircraft_type == "passenger" {
        // Resolve the Premium slot before deciding whether this is a
        // nonempty installed cabin; otherwise a Premium-only declaration
        // could be mistaken for an empty count cabin and rematerialized.
        config.cabin.passenger = config.cabin.passenger.canonicalized_for_product();
    }
    let explicit_count_cabin = config.requirements.aircraft_type == "passenger"
        && config.cabin.passenger.class_mix_mode == "count"
        && config.cabin.passenger.total_seats() > 0;
    if !explicit_count_cabin {
        apply_cabin_preset(config, Some(design_vector))?;
    }
    if config.requirements.aircraft_type == "passenger" {
        // Resolve one canonical three-class cabin for both payload and FLOPS.
        // A Premium slot is folded into Economy before the row packer
        // sees it; percent-mode stale count seeds are ignored unless they
        // already agree with the requirements total.
        let counts = config
            .cabin
            .passenger
            .resolved_flops_counts(config.requirements.num_passengers);
        if config.cabin.passenger.class_mix_mode == "count" && counts.is_nonempty() {
            config.requirements.num_passengers = counts.total();
        }
        let to_count = |count: i64| usize::try_from(count.max(0)).unwrap_or(usize::MAX);
        annotate_flops_cabin_resolution(&mut config.mass_model, counts);
        config
            .mass_model
            .flops_transport
            .first_class_passenger_count = Some(to_count(counts.first));
        config
            .mass_model
            .flops_transport
            .business_class_passenger_count = Some(to_count(counts.business));
        config
            .mass_model
            .flops_transport
            .tourist_class_passenger_count = Some(to_count(counts.tourist));
    }
    config.requirements.cargo_payload_kg = target_cargo_kg;
    config
        .cabin
        .passenger
        .set_passenger_mass_kg(passenger_mass_kg);
    Ok(())
}

/// Callable cost function for aircraft design space optimization.
#[derive(Debug, Clone)]
pub struct DesignObjective {
    pub(crate) cancellation: Option<crate::cancellation::EvaluationCancellation>,
    /// Active aircraft configuration.
    pub config: AlasConfig,
    /// Trajectory of evaluated candidates.
    pub history: OptimizationHistory,
    /// Hard floor on geometry-resolved passenger capacity
    /// ([`alas_config::DesignRequirements::min_passenger_capacity`]), or zero
    /// when no floor is configured. This does not force the cabin to hit an
    /// exact count (capacity is always resolved dynamically from the cabin
    /// class mix and the candidate's geometry) it only feeds the
    /// `passenger_shortfall` residual/penalty so a candidate whose resolved
    /// capacity falls short of the floor is scored accordingly.
    pub target_num_passengers: i64,
    /// The cargo payload mass a candidate's achieved payload is scored
    /// against, kg, captured before any candidate load case runs.
    ///
    /// [`alas_config::DesignRequirements::cargo_target_kg`]: the user's
    /// entered cargo objective when there is one, otherwise the configured
    /// cargo payload capacity. It is a target to match, not a floor: `mdo::residuals_geometry`
    /// turns the two-sided deviation from it into the soft
    /// `cargo_target_shortfall`/`cargo_target_excess` pair, and what rejects
    /// an overloaded aircraft stays in the mass, balance and volume
    /// residuals. Zero on a passenger aircraft, which has no such pair.
    pub target_cargo_payload_kg: f64,
    /// Nominal vector around which reference and baseline design envelopes
    /// are enforced. Clean-sheet runs use the configured preset when one is
    /// present, otherwise the canonical default vector.
    pub(crate) design_space_nominal: DesignVector,
    /// Keep a caller-pinned clean-sheet fuselage length literal.
    ///
    /// The ordinary product search derives this coordinate from the cabin
    /// load case.  A desktop/reference run may deliberately pin a literal
    /// vector, however; applying the derived solve again would make the
    /// report describe a different aircraft than the one the user supplied.
    pub(crate) preserve_explicit_fuselage_length: bool,
    /// Work budget and integration controls of every candidate's sizing
    /// closure; the default sizes exactly as the configuration says.
    pub(crate) sizing_controls: crate::mdo::SizingControls,
}

impl DesignObjective {
    /// Construct a new design objective initialized from `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self::with_nominal_policy(config, None, false)
    }

    /// Whether candidates keep a caller-pinned fuselage length literal, as
    /// [`crate::resolve_tail_sizing`] needs to build the same aircraft.
    pub fn preserves_explicit_fuselage_length(&self) -> bool {
        self.preserve_explicit_fuselage_length
    }

    /// Construct a product objective around an explicit nominal design. The
    /// optimizer uses this when a caller supplies a registered preset vector;
    /// direct assessment keeps the configuration's preset/default nominal.
    pub(crate) fn new_with_nominal(config: AlasConfig, nominal: DesignVector) -> Self {
        Self::with_nominal_policy(config, Some(nominal), false)
    }

    /// Construct a product objective for a caller that pins the fuselage
    /// coordinate explicitly in its design-space bounds.
    pub(crate) fn new_with_nominal_and_fuselage_policy(
        config: AlasConfig,
        nominal: DesignVector,
        preserve_explicit_fuselage_length: bool,
    ) -> Self {
        Self::with_nominal_policy(config, Some(nominal), preserve_explicit_fuselage_length)
    }

    fn with_nominal_policy(
        config: AlasConfig,
        nominal: Option<DesignVector>,
        preserve_explicit_fuselage_length: bool,
    ) -> Self {
        let target_num_passengers = config.requirements.min_passenger_capacity;
        let target_cargo_payload_kg = config.requirements.cargo_target_kg();
        let nominal = nominal.unwrap_or_else(|| config.configured_nominal_design());
        Self {
            config,
            history: OptimizationHistory::new(),
            cancellation: None,
            target_num_passengers,
            target_cargo_payload_kg,
            // Direct evaluator callers pass the configured preset/default
            // vector. Product optimizer callers use
            // `new_with_nominal` after the driver has materialized any
            // cabin-derived variables, so this field is always the nominal
            // vector for the boundary being evaluated.
            design_space_nominal: nominal,
            preserve_explicit_fuselage_length,
            sizing_controls: crate::mdo::SizingControls::default(),
        }
    }

    /// Check the configured mutable/fixed design boundary before building a
    /// candidate. This is deliberately at the evaluator boundary as well as
    /// in optimizer bounds, so GUI/CLI finalist assessment cannot bypass a
    /// reference or baseline sandbox by calling the production entry point
    /// directly.
    pub(crate) fn validate_design_space(&self, x: &[f64]) -> Result<(), String> {
        let design = DesignVector::from_array(x).map_err(|error| error.to_string())?;
        let space = &self.config.optimizer.design_space;
        space.validate()?;
        let envelopes = self.config.design_envelope(&self.design_space_nominal);
        for (index, (value, envelope)) in design.to_array().into_iter().zip(envelopes).enumerate() {
            // The clean-sheet passenger fuselage is a derived coordinate,
            // solved from the requested cabin load case in `build_geometry`.
            // It is fixed in optimizer bounds, but direct evaluator callers
            // may still submit the pre-sizing preset/default vector. The
            // builder replaces that coordinate deterministically before any
            // discipline sees it, so enforcing this fixed boundary here
            // would reject the public direct-assessment entry point without
            // adding a mutable degree of freedom.
            if space.sizes_fuselage_from_cabin() && envelope.name == "fuselage_length_m" {
                continue;
            }
            // A reference adaptation of a registered aircraft derives the
            // tail scale from the wing (`mdo::tail_sizing`), so a published
            // vector carries the derived value rather than the fixed nominal.
            if space.mode == DesignMode::ReferenceAdaptation
                && envelope.name == "tail_scale"
                && alas_config::presets::get(&self.config.preset).is_ok()
            {
                continue;
            }
            // A clean sheet's derived or explicit box steers its search; it
            // protects no reference aircraft. Direct assessment therefore
            // still admits every vector the global box admits, under the
            // same aerodrome span limit.
            let (lower, upper) = match SPECS.get(index) {
                Some(spec) if space.mode == DesignMode::CleanSheet => {
                    let mut upper = envelope.upper.max(spec.upper);
                    if envelope.name == "span_m" {
                        upper = upper.min(self.config.max_design_span_m().unwrap_or(upper));
                    }
                    (envelope.lower.min(spec.lower), upper)
                }
                _ => (envelope.lower, envelope.upper),
            };
            let tolerance = 1.0e-10 * lower.abs().max(upper.abs()).max(1.0);
            if value < lower - tolerance || value > upper + tolerance {
                return Err(format!(
                    "design variable {} ({}) lies outside the {:?} envelope [{}, {}]",
                    index, envelope.name, space.mode, lower, upper
                ));
            }
        }
        if space.mode == DesignMode::BaselineSandbox
            && design.to_array() != self.design_space_nominal.to_array()
        {
            return Err("baseline sandbox accepts only its nominal design vector".to_owned());
        }
        Ok(())
    }
}

impl DesignObjective {
    /// Evaluate the scalar cost for candidate design vector `x`.
    ///
    /// Each candidate is sized by the design mission and ranked feasibility
    /// first; see [`crate::mdo::evaluate_mission_sized`].
    pub fn evaluate(&mut self, x: &[f64]) -> f64 {
        crate::mdo::evaluate_mission_sized(self, x)
    }
}

#[cfg(test)]
#[path = "objective_tests.rs"]
mod objective_tests;
