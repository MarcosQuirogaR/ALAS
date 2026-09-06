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
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_payload::build::{apply_cabin_preset, CabinPresetError};

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
/// A fixed passenger target must survive every candidate evaluation. Cargo
/// presets retain their existing capacity-derived behavior, while passenger
/// capacity sizing is an explicit opt-in on [`alas_config::DesignRequirements`].
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
    apply_cabin_preset(config, Some(design_vector))
}

/// Callable cost function for aircraft design space optimization.
#[derive(Debug, Clone)]
pub struct DesignObjective {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    /// Trajectory of evaluated candidates.
    pub history: OptimizationHistory,
    /// Original passenger count target before cabin preset scaling.
    pub target_num_passengers: i64,
    /// Original cargo payload target in kg.
    pub target_cargo_payload_kg: f64,
    reference_mass_coordinates: bool,
    body_alpha_mesh_correction_deg: Option<f64>,
}

impl DesignObjective {
    /// Construct a new design objective initialized from `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self::with_mass_coordinate_compatibility(config, false)
    }

    /// Construct an objective that reproduces the frozen Python mass point.
    ///
    /// This is only for reference-parity replay. Product optimization uses
    /// [`Self::new`], which evaluates the configured structural-wingbox
    /// centroid and refuses an invalid structural configuration.
    pub fn new_reference_compatibility(config: AlasConfig) -> Self {
        Self::with_mass_coordinate_compatibility(config, true)
    }

    /// Whether this objective replays the frozen Python model (the parity
    /// fixtures) rather than the mission-sized product objective.
    pub(crate) fn is_reference_replay(&self) -> bool {
        self.reference_mass_coordinates
    }

    fn with_mass_coordinate_compatibility(
        mut config: AlasConfig,
        reference_mass_coordinates: bool,
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
        }
        if reference_mass_coordinates {
            config.geometry.engine.apply_engine_spec();
        }
        let target_num_passengers = config.requirements.num_passengers;
        let target_cargo_payload_kg = config.requirements.cargo_payload_kg;
        Self {
            config,
            history: OptimizationHistory::new(),
            target_num_passengers,
            target_cargo_payload_kg,
            reference_mass_coordinates,
            body_alpha_mesh_correction_deg: None,
        }
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
