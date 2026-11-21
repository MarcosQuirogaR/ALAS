// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static deflection of the wing standing on the ground, from the sized box.
//!
//! Design rule: the lofted geometry stays the drawn, unloaded
//! shape for the aerodynamics and the overlays; the ground clearances of the
//! nacelles and the wing tip are that shape minus the wing's own static droop
//! on the ground, computed here. The droop is a cantilever from the side of
//! body (`alas_struct::ground_deflection`) with the wing's weight, its fuel and
//! its engines hanging on it and no aerodynamic lift.
//!
//! # Load state (all weights, standard gravity of the requirements)
//!
//! * Box structure: the strength- and stiffness-sized primary box
//!   ([`super::sized_primary_wing`]), its own running mass.
//! * Non-box wing structure (leading and trailing edges, flaps, slats, fixed
//!   systems): the authoritative complete-wing mass less the box, spread in
//!   proportion to the local chord. A chord-proportional share is an
//!   assumption; it is the same family of estimate the inventory uses for the
//!   non-box centroid.
//! * Fuel: the integral-tank fuel the loading envelope guarantees at the given
//!   gross mass (`min(C, max(0, W - MZFW))`, [`super::declared_wing_fuel_case`])
//!   with the declared spanwise shape, or an explicit override.
//! * Engines: [`alas_struct::loads::engine_point_loads_n`] installed masses at
//!   their spanwise stations (installation factor already includes the pylon).
//! * Not modelled: wing-mounted gear reaction, nacelle and pylon mass beyond
//!   the installation factor, payload in the wing, fuel sloshing, torsion.
//!
//! # Stiffness: a diagnostic EI
//!
//! `EI(y)` is the sized box with the 6 mm unstiffened covers credited at full
//! modulus. It is neither tuned nor calibrated here; whether it
//! is physical is a validation question reported with the results.

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_struct::ground_deflection::{
    box_bending_stiffness_nm2, static_deflection, GroundDeflection, PointForce,
};

use super::reconcile::primary_structure_geometry;
use super::sizing_cause::{material, StructuralMaterialRole as Role};
use super::{
    declared_wing_fuel_case, main_wing, sized_primary_wing, structural_design_mass_kg,
    StructuralSizingCause as Cause, WingReconciliationError,
};

/// The loading condition of the aircraft on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundCondition {
    /// Gross mass of the aircraft, kg (maximum ramp weight, maximum takeoff
    /// weight or a light maintenance weight).
    pub gross_mass_kg: f64,
    /// Total integral-wing fuel on board, both wings, kg. `None` takes the
    /// fuel the loading envelope guarantees in the wings at `gross_mass_kg`.
    pub wing_fuel_kg: Option<f64>,
    /// Vertical load factor on the wing weight: 1 at rest, above 1 for a
    /// taxi bump.
    pub load_factor: f64,
}

impl GroundCondition {
    /// A static condition (`n = 1`) at `gross_mass_kg` with the guaranteed
    /// wing fuel.
    pub fn at_rest(gross_mass_kg: f64) -> Self {
        Self {
            gross_mass_kg,
            wing_fuel_kg: None,
            load_factor: 1.0,
        }
    }
}

/// The ground response of one semi-wing and the load state behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct WingGroundResponse {
    /// Deflection curve, downward positive, relative to the unloaded shape.
    pub deflection: GroundDeflection,
    /// Box bending stiffness at the stations, N.m^2.
    pub ei_nm2: Vec<f64>,
    /// Clamp station (side of body), m from the centreline.
    pub support_y_m: f64,
    /// Semi-wing masses carried: box, non-box structure, fuel, engines, kg.
    pub box_kg: f64,
    /// Non-box wing structure of one semi-wing, kg.
    pub non_box_kg: f64,
    /// Fuel on one semi-wing, kg.
    pub fuel_kg: f64,
    /// Installed engine mass on one semi-wing, kg.
    pub engines_kg: f64,
    /// The sized box outweighs the complete wing, so no non-box mass was added.
    pub box_exceeds_complete_wing: bool,
    /// The fuel came from the guaranteed loading-envelope bound (`true`) or
    /// from an explicit override or a geometric estimate (`false`).
    pub fuel_from_declared_tanks: bool,
}

/// The authoritative complete-wing mass (both wings), kg, of `plane`: the same
/// mass buildup [`super::reconcile`] takes as the total wing.
///
/// # Errors
///
/// [`WingReconciliationError::MassCoordinates`] when the buildup fails.
pub fn complete_wing_mass_kg(
    config: &AlasConfig,
    plane: &Airplane,
) -> Result<f64, WingReconciliationError> {
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, _, _) = crate::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&model),
        None,
        crate::breakdown::MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| WingReconciliationError::MassCoordinates)?;
    Ok(masses.wing)
}

/// The wing-body junction the cantilever is clamped at, m from the centreline.
pub fn support_station_m(config: &AlasConfig, semi_span_m: f64) -> f64 {
    config
        .geometry
        .wing
        .side_of_body_span_fraction
        .map(|fraction| fraction * semi_span_m)
        .filter(|y| y.is_finite() && *y >= 0.0)
        .unwrap_or(0.5 * config.geometry.fuselage.diameter_m)
}

/// Static ground deflection of the main wing of `plane` for `condition`.
///
/// `complete_wing_kg` is the authoritative complete-wing mass (both wings), as
/// [`super::reconcile_with_wing_mass`] takes it.
///
/// # Errors
///
/// [`WingReconciliationError::StructuralSizing`] when the box cannot be sized
/// or the deflection inputs are not physical.
pub fn wing_ground_deflection(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    complete_wing_kg: f64,
    condition: GroundCondition,
) -> Result<WingGroundResponse, WingReconciliationError> {
    let wing = main_wing(plane).ok_or(Cause::MissingMainWing)?;
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = structural_design_mass_kg(config);
    let (_, sizing) = sized_primary_wing(config, design, plane, &requirements)?;
    let geometry = primary_structure_geometry(config, design, wing)?;
    let skin = material(&config.structures, Role::Skin)?;
    let web = material(&config.structures, Role::SparWeb)?;
    let cap = material(&config.structures, Role::SparCap)?;
    let y = sizing.y_stations.clone();
    let ei = box_bending_stiffness_nm2(&sizing, skin, web, cap);

    let (front, rear) = alas_struct::sizing::box_chord_band(&geometry);
    let box_running =
        alas_struct::sizing::box_running_mass_kg_m(&sizing, skin, web, cap, front, rear);
    let box_kg = trapezoid(&box_running, &y);

    // Non-box structure per semi-wing, proportional to the local chord.
    let semi_complete_kg = 0.5 * complete_wing_kg;
    let box_exceeds_complete_wing =
        semi_complete_kg.partial_cmp(&box_kg) != Some(std::cmp::Ordering::Greater);
    let non_box_kg = if box_exceeds_complete_wing {
        0.0
    } else {
        semi_complete_kg - box_kg
    };
    let chord_integral = trapezoid(&sizing.chord, &y);
    let non_box_running: Vec<f64> = sizing
        .chord
        .iter()
        .map(|c| {
            if chord_integral > 0.0 {
                non_box_kg * c / chord_integral
            } else {
                0.0
            }
        })
        .collect();

    // Fuel at the given gross mass.
    let mut at_gross = requirements.clone();
    at_gross.mtow_kg = condition.gross_mass_kg;
    let declared = declared_wing_fuel_case(config, design, &at_gross, &geometry, &y, front, rear);
    let (fuel_running, fuel_from_declared_tanks) = match (&declared, condition.wing_fuel_kg) {
        (Some(case), None) => (case.running_mass_kg_m.clone(), true),
        (Some(case), Some(total_kg)) => {
            (rescale(&case.running_mass_kg_m, &y, 0.5 * total_kg), false)
        }
        (None, total) => {
            let shape = alas_struct::tanks::integral_fuel_running_mass_kg_m(
                &geometry,
                &y,
                front,
                rear,
                crate::tanks::inventory_density_kg_m3(config),
            );
            // A geometric full-box estimate only when told how much fuel;
            // otherwise no fuel is invented.
            (rescale(&shape, &y, 0.5 * total.unwrap_or(0.0)), false)
        }
    };
    let fuel_kg = trapezoid(&fuel_running, &y);

    let point_masses = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    );
    let engines_kg: f64 = point_masses.iter().map(|&(_, m)| m).sum();

    let weight_factor = condition.load_factor * requirements.gravity_m_s2;
    let line_load: Vec<f64> = (0..y.len())
        .map(|k| weight_factor * (box_running[k] + non_box_running[k] + fuel_running[k]))
        .collect();
    let forces: Vec<PointForce> = point_masses
        .iter()
        .map(|&(station, mass)| PointForce {
            y_m: station,
            down_n: weight_factor * mass,
        })
        .collect();
    let support = support_station_m(config, geometry.semi_span);
    let deflection = static_deflection(&y, &ei, &line_load, &forces, support)
        .map_err(Cause::GroundDeflection)?;
    Ok(WingGroundResponse {
        deflection,
        ei_nm2: ei,
        support_y_m: support,
        box_kg,
        non_box_kg,
        fuel_kg,
        engines_kg,
        box_exceeds_complete_wing,
        fuel_from_declared_tanks,
    })
}

/// Tip rise of the sized box in 1 g level flight at the structural design
/// mass, m, relative to its unloaded shape: the analytical level-flight load
/// case (elliptic lift less the inertia relief of the box, the design-case wing
/// fuel and the engines) of [`alas_struct::analytical`].
///
/// It exists to check the stiffness the ground droop rests on against the
/// published take-off flex of a real wing (A380: "over 4 m", Airbus A380
/// Facts and Figures, 2022): the ground droop and this rise share one `EI(y)`.
///
/// # Errors
///
/// [`WingReconciliationError::StructuralSizing`] when the box cannot be sized.
pub fn one_g_tip_rise_m(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
) -> Result<f64, WingReconciliationError> {
    let wing = main_wing(plane).ok_or(Cause::MissingMainWing)?;
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = structural_design_mass_kg(config);
    let (_, sizing) = sized_primary_wing(config, design, plane, &requirements)?;
    let geometry = primary_structure_geometry(config, design, wing)?;
    let skin = material(&config.structures, Role::Skin)?;
    let web = material(&config.structures, Role::SparWeb)?;
    let cap = material(&config.structures, Role::SparCap)?;
    let (front, rear) = alas_struct::sizing::box_chord_band(&geometry);
    let fuel = declared_wing_fuel_case(
        config,
        design,
        &requirements,
        &geometry,
        &sizing.y_stations,
        front,
        rear,
    )
    .map(|case| case.running_mass_kg_m)
    .unwrap_or_else(|| {
        alas_struct::tanks::integral_fuel_running_mass_kg_m(
            &geometry,
            &sizing.y_stations,
            front,
            rear,
            crate::tanks::inventory_density_kg_m3(config),
        )
    });
    let engines = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    );
    let report = alas_struct::analytical::analyze_structure_with_wing_carried_mass(
        &geometry,
        &sizing,
        &config.structures,
        &requirements,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &fuel,
        &engines,
    );
    report
        .load_cases
        .iter()
        .find(|case| case.name == "level")
        .map(|case| case.tip_deflection_m)
        .filter(|rise| rise.is_finite())
        .ok_or(Cause::LevelTipDeflection.into())
}

/// Scale a running-mass shape so that it integrates to `target_kg`.
fn rescale(shape: &[f64], y: &[f64], target_kg: f64) -> Vec<f64> {
    let integral = trapezoid(shape, y);
    if integral > 0.0 && target_kg.is_finite() && target_kg >= 0.0 {
        shape.iter().map(|m| m * target_kg / integral).collect()
    } else {
        vec![0.0; shape.len()]
    }
}

fn trapezoid(values: &[f64], y: &[f64]) -> f64 {
    values
        .windows(2)
        .zip(y.windows(2))
        .map(|(v, s)| 0.5 * (s[1] - s[0]) * (v[0] + v[1]))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trapezoid_integrates_a_ramp_exactly() {
        let y = [0.0, 1.0, 3.0];
        let v = [0.0, 1.0, 3.0];
        assert!((trapezoid(&v, &y) - 4.5).abs() < 1e-12);
    }

    #[test]
    fn rescale_hits_its_target_and_rejects_nonsense() {
        let y = [0.0, 2.0, 4.0];
        let shape = [1.0, 3.0, 1.0];
        let r = rescale(&shape, &y, 100.0);
        assert!((trapezoid(&r, &y) - 100.0).abs() < 1e-12);
        assert!(rescale(&shape, &y, f64::NAN).iter().all(|m| *m == 0.0));
        assert!(rescale(&shape, &y, -1.0).iter().all(|m| *m == 0.0));
    }
}

/// Bending and torsional stiffness of the sized primary wing box, and the mass
/// it carries, for the static aeroelastic correction of the neutral point
/// (`alas-stab`).
#[derive(Debug, Clone, PartialEq)]
pub struct WingBoxStiffness {
    /// Stations from the box root to the tip, m.
    pub y_m: Vec<f64>,
    /// Spanwise-frame bending stiffness `EI_n cos^3 L`, N m^2.
    pub ei_nm2: Vec<f64>,
    /// Torsional rigidity `G J` of the section normal to the box axis, N m^2.
    pub gj_nm2: Vec<f64>,
    /// Chord fraction of the elastic axis: mean of the outermost spars.
    pub box_axis_fraction: f64,
    /// Running mass of one semi-wing carried on the elastic axis at `y_m`,
    /// kg/m: the sized box plus the integral-tank fuel of the design case.
    pub carried_kg_m: Vec<f64>,
    /// `(spanwise station m, kg)` wing-mounted engines of one semi-wing.
    pub point_masses_kg: Vec<(f64, f64)>,
    /// The aircraft mass the inertia relief is referred to, kg: the structural
    /// design mass, the mass the box is sized at.
    pub aircraft_mass_kg: f64,
}

/// The sized box's stiffness of `plane`'s main wing and the mass it carries at
/// the structural design mass, the same sizing [`wing_ground_deflection`]
/// reads. The carried mass is the sized box, the wing fuel the loading
/// envelope guarantees at that mass (none when the aircraft declares no wing
/// tank: no fuel is invented) and the wing-mounted engines. The non-box wing
/// structure (leading and trailing edges, flaps, fairings) is not included.
///
/// # Errors
///
/// [`WingReconciliationError::StructuralSizing`] when the box cannot be sized.
pub fn wing_box_stiffness(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
) -> Result<WingBoxStiffness, WingReconciliationError> {
    let wing = main_wing(plane).ok_or(Cause::MissingMainWing)?;
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = structural_design_mass_kg(config);
    let (_, sizing) = sized_primary_wing(config, design, plane, &requirements)?;
    let geometry = primary_structure_geometry(config, design, wing)?;
    let skin = material(&config.structures, Role::Skin)?;
    let web = material(&config.structures, Role::SparWeb)?;
    let cap = material(&config.structures, Role::SparCap)?;
    let (front, rear) = alas_struct::sizing::box_chord_band(&geometry);
    let y = sizing.y_stations.clone();
    let box_running =
        alas_struct::sizing::box_running_mass_kg_m(&sizing, skin, web, cap, front, rear);
    let fuel_running =
        declared_wing_fuel_case(config, design, &requirements, &geometry, &y, front, rear)
            .map_or_else(|| vec![0.0; y.len()], |case| case.running_mass_kg_m);
    let carried_kg_m = (0..y.len())
        .map(|k| box_running[k] + fuel_running.get(k).copied().unwrap_or(0.0))
        .collect();
    let point_masses_kg = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    );
    Ok(WingBoxStiffness {
        ei_nm2: box_bending_stiffness_nm2(&sizing, skin, web, cap),
        gj_nm2: alas_struct::ground_deflection::box_torsional_stiffness_nm2(&sizing, skin, web),
        y_m: y,
        box_axis_fraction: 0.5 * (front + rear),
        carried_kg_m,
        point_masses_kg,
        aircraft_mass_kg: requirements.mtow_kg,
    })
}
