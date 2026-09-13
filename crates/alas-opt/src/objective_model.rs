// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/objective.py
// Reference: alas @ rust-port-baseline.

//! Scalar objective function for aircraft design space optimization.
//!
//! Evaluates aerodynamic performance (L/D) alongside physical constraints
//! (stability, CG envelope, tank capacity, cabin sizing, geometry bounds).

use std::f64::consts::PI;

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_payload::build::{
    apply_cabin_preset, apply_cabin_preset_reference_compatibility, CabinPresetError,
};

use crate::history::OptimizationHistory;

/// Reproduce the frozen parasite-drag buildup used by the parity fixture.
///
/// Product aero now accounts for each surface's own thickness, sweep, and
/// reference convention. The historical objective fixture instead applied the
/// main-wing section and design sweep to every wing and summed each wing's
/// unfolded area. Keep that numerical seam local to the explicit objective
/// compatibility path; product evaluations continue to use
/// [`AeroAnalysis::parasite_drag`].
fn parasite_drag_reference_compatibility(
    aero: &AeroAnalysis<'_>,
    mach: f64,
    altitude_m: f64,
) -> f64 {
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let density = atmosphere.density();
    let viscosity = atmosphere.dynamic_viscosity();
    let s_ref = aero.plane.s_ref;
    let thickness = aero.section_thickness();
    let x_over_c = aero.drag.max_thickness_chordwise_loc;
    let sweep = aero.sweep_deg.to_radians();
    let turbulent_cf = |reynolds: f64| {
        0.455 / (reynolds.log10().powf(2.58) * (1.0 + 0.144 * mach * mach).powf(0.65))
    };

    let mut cd0 = 0.0;
    for wing in &aero.plane.wings {
        let mac = wing.mean_aerodynamic_chord();
        let reynolds = density * velocity * mac / viscosity;
        let cf = turbulent_cf(reynolds);
        let form_factor = (1.0 + 0.6 / x_over_c * thickness + 100.0 * thickness.powf(4.0))
            * (1.34 * mach.powf(0.18) * sweep.cos().powf(0.28));
        let wetted = wing.unfolded_area() * aero.geometry.wing_wetted_area_factor;
        cd0 += cf * form_factor * aero.drag.interference_factor_wing * (wetted / s_ref);
    }

    if let Some(fuselage) = aero.plane.fuselages.first() {
        let length = match (fuselage.xsecs.first(), fuselage.xsecs.last()) {
            (Some(first), Some(last)) => last.xyz_c[0] - first.xyz_c[0],
            _ => 0.0,
        };
        let diameter = aero.geometry.fuselage.diameter_m;
        let wetted = PI * diameter * length * aero.geometry.fuselage_wetted_factor;
        let reynolds = density * velocity * length / viscosity;
        cd0 += turbulent_cf(reynolds) * aero.drag.interference_factor_fuselage * (wetted / s_ref);
    }

    for nacelle in aero.plane.fuselages.iter().skip(1) {
        let length = match (nacelle.xsecs.first(), nacelle.xsecs.last()) {
            (Some(first), Some(last)) => last.xyz_c[0] - first.xyz_c[0],
            _ => 0.0,
        };
        let diameter = 2.0 * aero.geometry.engine.radius_scale_m;
        let wetted = PI * diameter * length;
        let reynolds = density * velocity * length / viscosity;
        cd0 += turbulent_cf(reynolds) * aero.drag.interference_factor_nacelle * (wetted / s_ref);
    }

    cd0 * aero.drag.viscous_margin
}

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
/// The historical Python correlation consumed the wing's unfolded YZ area
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
    apply_cabin_preset(config, Some(design_vector))?;
    if config.requirements.aircraft_type == "passenger" {
        // The FLOPS transport mass model declares its own per-class
        // passenger counts independently of the cabin/requirements model.
        // Keep them derived from the same geometry-resolved counts
        // `apply_cabin_preset` just wrote, so the FLOPS buildup's own
        // completeness check (`first + business + tourist ==
        // requested_passengers`, in `alas_mass::flops_transport::product`)
        // can never fail from a stale copy, whatever the resolved total is.
        let to_count = |count: i64| usize::try_from(count.max(0)).unwrap_or(0);
        config
            .mass_model
            .flops_transport
            .first_class_passenger_count = Some(to_count(config.cabin.passenger.first.count));
        config
            .mass_model
            .flops_transport
            .business_class_passenger_count = Some(to_count(config.cabin.passenger.business.count));
        config
            .mass_model
            .flops_transport
            .tourist_class_passenger_count = Some(to_count(config.cabin.passenger.economy.count));
    }
    config.requirements.cargo_payload_kg = target_cargo_kg;
    config
        .cabin
        .passenger
        .set_passenger_mass_kg(passenger_mass_kg);
    Ok(())
}

/// Apply the candidate cabin load case using the frozen Python semantics.
///
/// The reference objective predates the product requirements-first cabin
/// contract.  It must therefore materialise the historical class counts and
/// payload geometry through the explicitly frozen preset helper; routing it
/// through [`apply_candidate_payload_load_case`] would silently replace those
/// counts with the product load case before parity mass coordinates are
/// evaluated.
pub(crate) fn apply_candidate_payload_load_case_reference_compatibility(
    config: &mut AlasConfig,
    design_vector: &DesignVector,
) -> Result<(), CabinPresetError> {
    if !config
        .requirements
        .resolves_payload_from_candidate_geometry()
    {
        return Ok(());
    }
    apply_cabin_preset_reference_compatibility(config, Some(design_vector))
}

/// Callable cost function for aircraft design space optimization.
#[derive(Debug, Clone)]
pub struct DesignObjective {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    /// Trajectory of evaluated candidates.
    pub history: OptimizationHistory,
    /// Hard floor on geometry-resolved passenger capacity
    /// ([`alas_config::DesignRequirements::min_passenger_capacity`]), or zero
    /// when no floor is configured. This does not force the cabin to hit an
    /// exact count -- capacity is always resolved dynamically from the cabin
    /// class mix and the candidate's geometry -- it only feeds the
    /// `passenger_shortfall` residual/penalty so a candidate whose resolved
    /// capacity falls short of the floor is scored accordingly.
    pub target_num_passengers: i64,
    /// Original cargo payload target in kg.
    pub target_cargo_payload_kg: f64,
    /// Nominal vector around which reference and baseline design envelopes
    /// are enforced. Clean-sheet runs use the configured preset when one is
    /// present, otherwise the canonical default vector.
    pub(crate) design_space_nominal: DesignVector,
    reference_mass_coordinates: bool,
    /// Keep a caller-pinned clean-sheet fuselage length literal.
    ///
    /// The ordinary product search derives this coordinate from the cabin
    /// load case.  A desktop/reference run may deliberately pin a literal
    /// vector, however; applying the derived solve again would make the
    /// report describe a different aircraft than the one the user supplied.
    pub(crate) preserve_explicit_fuselage_length: bool,
    body_alpha_mesh_correction_deg: Option<f64>,
}

impl DesignObjective {
    /// Construct a new design objective initialized from `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self::with_mass_coordinate_compatibility(config, false, None, false)
    }

    /// Construct a product objective around an explicit nominal design. The
    /// optimizer uses this when a caller supplies a registered preset vector;
    /// direct assessment keeps the configuration's preset/default nominal.
    pub(crate) fn new_with_nominal(config: AlasConfig, nominal: DesignVector) -> Self {
        Self::with_mass_coordinate_compatibility(config, false, Some(nominal), false)
    }

    /// Construct a product objective for a caller that pins the fuselage
    /// coordinate explicitly in its design-space bounds.
    pub(crate) fn new_with_nominal_and_fuselage_policy(
        config: AlasConfig,
        nominal: DesignVector,
        preserve_explicit_fuselage_length: bool,
    ) -> Self {
        Self::with_mass_coordinate_compatibility(
            config,
            false,
            Some(nominal),
            preserve_explicit_fuselage_length,
        )
    }

    /// Construct an objective that reproduces the frozen Python mass point.
    ///
    /// This is only for reference-parity replay. Product optimization uses
    /// [`Self::new`], which evaluates the configured structural-wingbox
    /// centroid and refuses an invalid structural configuration.
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        // The comparison constructor is the explicit opt-in to the frozen
        // mass model.  Pin the authoritative architecture here so a caller
        // starting from the pure product default cannot accidentally run the
        // legacy coordinate/aerodynamic replay with a FLOPS mass buildup.
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        config.mass_model.apply_architecture();
        Self::with_mass_coordinate_compatibility(config, true, None, false)
    }

    /// Whether this objective replays the frozen Python model (the parity
    /// fixtures) rather than the mission-sized product objective.
    pub(crate) fn is_reference_replay(&self) -> bool {
        self.reference_mass_coordinates
    }

    fn with_mass_coordinate_compatibility(
        mut config: AlasConfig,
        reference_mass_coordinates: bool,
        nominal: Option<DesignVector>,
        preserve_explicit_fuselage_length: bool,
    ) -> Self {
        if reference_mass_coordinates {
            // The parity fixture predates the native transport-planform
            // defaults. Restore its three-station planform before geometry,
            // mass and fuel-volume calculations so the frozen cost remains
            // attributable to the translated reference model.
            config.geometry.wing.side_of_body_span_fraction = None;
            config.geometry.wing.side_of_body_chord_ratio = None;
            config.geometry.wing.kink_span_fraction = None;
            config.geometry.wing.outboard_le_sweep_deg = None;
            // And its vortex-lattice mesh, for the same reason. The
            // geometry's own spanwise subdivision is not restored here: it
            // changed meaning rather than value, and only a builder can
            // interpret it, so `AircraftBuilder::new_reference_compatibility`
            // owns that -- which is the builder this path uses below.
            config.analysis.restore_reference_mesh();
        }
        if reference_mass_coordinates {
            config.geometry.engine.apply_engine_spec();
        }
        let target_num_passengers = config.requirements.min_passenger_capacity;
        let target_cargo_payload_kg = config.requirements.cargo_payload_kg;
        let nominal = nominal.unwrap_or_else(|| {
            if config.preset.is_empty() {
                DesignVector::default()
            } else {
                alas_config::presets::get(&config.preset)
                    .map(|preset| preset.design_vector)
                    .unwrap_or_default()
            }
        });
        Self {
            config,
            history: OptimizationHistory::new(),
            target_num_passengers,
            target_cargo_payload_kg,
            // Direct evaluator callers historically pass the configured
            // preset/default vector. Product optimizer callers use
            // `new_with_nominal` after the driver has materialized any
            // cabin-derived variables, so this field is always the nominal
            // vector for the boundary being evaluated.
            design_space_nominal: nominal,
            reference_mass_coordinates,
            preserve_explicit_fuselage_length,
            body_alpha_mesh_correction_deg: None,
        }
    }

    /// Check the configured mutable/fixed design boundary before building a
    /// candidate. This is deliberately at the evaluator boundary as well as
    /// in optimizer bounds, so GUI/CLI finalist assessment cannot bypass a
    /// reference or baseline sandbox by calling the production entry point
    /// directly.
    pub(crate) fn validate_design_space(&self, x: &[f64]) -> Result<(), String> {
        if self.reference_mass_coordinates {
            return Ok(());
        }
        let design = DesignVector::from_array(x).map_err(|error| error.to_string())?;
        let space = &self.config.optimizer.design_space;
        space.validate()?;
        let envelopes = space.envelope(&self.design_space_nominal);
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
            let tolerance = 1.0e-10 * envelope.lower.abs().max(envelope.upper.abs()).max(1.0);
            if value < envelope.lower - tolerance || value > envelope.upper + tolerance {
                return Err(format!(
                    "design variable {} ({}) lies outside the {:?} envelope [{}, {}]",
                    index, envelope.name, space.mode, envelope.lower, envelope.upper
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

// Keep evaluation as a child module: it extends `DesignObjective` while
// retaining access to the model's private compatibility state. The evaluator
// imports that state explicitly so this boundary remains stable as the split
// evolves.
#[path = "objective_evaluate.rs"]
mod evaluate;

#[path = "objective_history.rs"]
mod objective_history;
