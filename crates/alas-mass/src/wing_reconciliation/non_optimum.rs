// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The box-side non-optimum items of the clean-sheet wing inventory.
//!
//! [`alas_struct::non_optimum`] holds the equations (Torenbeek 2013, section
//! 11.5); this module reads their drivers off the configuration and the built
//! wing and turns the result into inventory entries. Reference-adaptation and
//! sandbox modes never reach it: their empirical remainder already contains a
//! real wing's non-optimum weight.

use alas_config::AlasConfig;
use alas_geom::aircraft::wing::Wing;
use alas_struct::non_optimum::{self, Construction, NonOptimumInputs};

use crate::wing_inventory::{WingInventoryEntry, NON_OPTIMUM_ITEM_COUNT};
use crate::wingbox_feedback::SizedWingboxMass;

/// ISA sea-level density that defines equivalent airspeed, kg/m^3.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Share of the main-gear load carried by legs attached to the wing, 0-1.
///
/// Torenbeek's Eq. 11.59 charges the landing load absorbed by wing-mounted
/// landing gears and is omitted when the legs attach to the fuselage. The
/// built geometry decides: a gear whose wing root sits above the fuselage crown
/// and whose wheels are not under a wing nacelle is fuselage-mounted
/// ([`crate::stations::fuselage_mounted_main_gear_leg_m`], the datum the FLOPS
/// leg length already uses) and carries no wing share. Otherwise the wing pair
/// carries `2 / n` of the load with equal leg loads (`n` = configured legs, or
/// the auto-sized 2 / 4 by the body-gear mass threshold, the rule
/// `alas_perf::landing_gear` applies); one configured leg is centreline gear.
pub(super) fn main_gear_wing_fraction(
    config: &AlasConfig,
    plane: &alas_geom::aircraft::airplane::Airplane,
    wing: &Wing,
    requirements: &alas_config::DesignRequirements,
) -> f64 {
    let gear = &config.landing_gear;
    let outboard_engine_y = config
        .geometry
        .engine
        .spanwise_positions_m
        .iter()
        .map(|y| y.abs())
        .fold(None, |m: Option<f64>, y| Some(m.map_or(y, |v| v.max(y))));
    let fuselage_mounted =
        crate::flops_transport::product::primary_fuselage(plane).is_some_and(|f| {
            crate::stations::fuselage_mounted_main_gear_leg_m(
                wing,
                f,
                &config.geometry,
                gear,
                outboard_engine_y,
            )
            .is_some()
        });
    if fuselage_mounted {
        return 0.0;
    }
    let legs = if gear.n_mlg_struts != 0 {
        gear.n_mlg_struts
    } else if requirements.mtow_kg >= gear.mlg_body_gear_mtow_kg {
        4
    } else {
        2
    };
    if legs < 2 {
        0.0
    } else {
        (2.0 / legs as f64).min(1.0)
    }
}

/// Thickness ratio of the wing chord at the fraction `eta` of the semispan,
/// linear between the cross-sections.
fn thickness_ratio_at(wing: &Wing, eta: f64) -> f64 {
    let sample: Vec<f64> = (0..101).map(|index| index as f64 / 100.0).collect();
    let stations: Vec<(f64, f64)> = wing
        .xsecs
        .iter()
        .map(|xsec| (xsec.xyz_le[1], xsec.airfoil.max_thickness(&sample)))
        .collect();
    let (Some(&(first, _)), Some(&(last, _))) = (stations.first(), stations.last()) else {
        return 0.0;
    };
    let y = first + eta.clamp(0.0, 1.0) * (last - first);
    for pair in stations.windows(2) {
        let ((y0, t0), (y1, t1)) = (pair[0], pair[1]);
        if y <= y1 && y1 > y0 {
            return t0 + (t1 - t0) * (y - y0).max(0.0) / (y1 - y0);
        }
    }
    stations.last().map_or(0.0, |&(_, t)| t)
}

/// The non-optimum inputs of a sized box, or `None` when the box did not come
/// from the sizer (no [`SizedWingboxMass::box_quantities`]).
///
/// `movable_secondary_kg` is the sum of the high-lift, aileron and spoiler
/// items of the non-box inventory.
pub(super) fn inputs(
    config: &AlasConfig,
    wing: &Wing,
    sized_box: &SizedWingboxMass,
    movable_secondary_kg: f64,
) -> Option<NonOptimumInputs> {
    let box_quantities = sized_box.box_quantities?;
    let structures = &config.structures;
    let skin = alas_config::materials::get(&structures.skin_material).ok()?;
    let cap = alas_config::materials::get(&structures.spar_cap_material).ok()?;
    let composite = skin.category == "composite" || cap.category == "composite";
    let design_gross_mass_kg = super::design_gross_mass_kg(config);
    let requirements = super::support::design_requirements(config);
    // Wing-mounted engines of the whole aircraft: the loader returns the
    // positive-y semi-wing only.
    // Eq. 11.58 is the attachment of engines on the wing: an engine within
    // the fuselage half-width (rear-fuselage or tail mounted) is not charged.
    let half_width_m = config.geometry.fuselage.diameter_m / 2.0;
    let engines: Vec<(f64, f64)> = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    )
    .into_iter()
    .filter(|&(y_m, _)| y_m > half_width_m)
    .collect();
    let dive_eas = config.requirements.dive_speed_m_s;
    let aileron_start = config.control_surfaces.aileron_span_start_frac;
    Some(NonOptimumInputs {
        box_quantities,
        cover_density_kg_m3: skin.rho_kg_m3,
        // Torenbeek 2013, Eq. 11.55: built-up unless bonded or machined. A
        // composite cover is declared bonded; a metallic one built-up (the
        // book's typical value, the heavier of the three).
        construction: if composite {
            Construction::Bonded
        } else {
            Construction::BuiltUp
        },
        composite,
        wing_area_m2: wing.reference_area(),
        span_m: wing.reference_span(),
        ultimate_load_factor: config.requirements.ultimate_load_factor,
        design_gross_mass_kg,
        landing_mass_kg: config.landing_mass_limit_kg(design_gross_mass_kg),
        wing_mounted_powerplant_kg: 2.0 * engines.iter().map(|&(_, mass)| mass).sum::<f64>(),
        wing_mounted_engines: 2 * engines.len(),
        wing_gear_load_fraction: sized_box.main_gear_wing_fraction.unwrap_or(0.0),
        dive_dynamic_pressure_pa: 0.5 * SEA_LEVEL_DENSITY_KG_M3 * dive_eas * dive_eas,
        // AMC 25.335(b): M_D = M_C + 0.07 (PDF p. 217 of CS-25 Amdt 27).
        dive_mach: config.requirements.cruise_mach + alas_struct::gust::MACH_MARGIN_C_TO_D,
        le_sweep_rad: wing.mean_sweep_angle(0.0).to_radians(),
        mid_chord_sweep_rad: wing.mean_sweep_angle(0.5).to_radians(),
        aileron_root_thickness_ratio: thickness_ratio_at(wing, aileron_start),
        secondary_mass_kg: movable_secondary_kg,
    })
}

/// Turn the evaluated items into inventory entries placed at the box
/// centroid (a declared placement: the penalties are smeared over the box or
/// concentrated at its root and pylons, all within a few percent of the box
/// first moment, and the complete-wing centroid has `y = 0` by symmetry).
pub(super) fn entries(
    inputs: &NonOptimumInputs,
    sized_box: &SizedWingboxMass,
) -> [WingInventoryEntry; NON_OPTIMUM_ITEM_COUNT] {
    let centroid = [sized_box.centroid_m[0], 0.0, sized_box.centroid_m[2]];
    non_optimum::evaluate(inputs).map(|item| WingInventoryEntry {
        name: item.name,
        mass_kg: item.mass_kg,
        centroid_m: centroid,
        source: item.source,
        applicability: item.applicability,
        relative_uncertainty: item.relative_uncertainty,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        clean_sheet_secondary, main_wing, sized_primary_wing, structural_design_mass_kg,
    };
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    fn inventory(name: &str) -> crate::wing_inventory::WingNonBoxInventory {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = presets::get(name).unwrap().design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let mut requirements = config.requirements.clone();
        requirements.mtow_kg = structural_design_mass_kg(&config);
        let (primary, _) = sized_primary_wing(&config, &design, &plane, &requirements).unwrap();
        let wing = main_wing(&plane).unwrap();
        clean_sheet_secondary(&config, wing, primary).unwrap()
    }

    #[test]
    fn the_clean_sheet_inventory_carries_every_non_optimum_item_and_keeps_its_ledger() {
        let inv = inventory("A320-200");
        let items = inv.non_optimum.expect("a sized box carries its quantities");
        assert_eq!(items.len(), crate::wing_inventory::NON_OPTIMUM_ITEM_COUNT);
        assert!(items
            .iter()
            .all(|i| i.mass_kg > 0.0 && !i.source.is_empty()));
        let added: f64 = items.iter().map(|i| i.mass_kg).sum();
        assert!((inv.total_kg() - inv.non_box_total_kg() - added).abs() < 1e-9);
        assert!((inv.complete_wing_mass_kg - inv.sized_box_mass_kg - inv.total_kg()).abs() < 1e-9);
        assert!((inv.diagnostics.non_optimum_total_kg - added).abs() < 1e-9);
        // The plausibility gates describe the ideal box plus the non-box
        // items and are not re-evaluated against the penalties.
        assert!(inv.status().is_complete());
        // Torenbeek (2013) p. 340: joints and splices are 5-10 % of the box
        // weight; the item must be of that order.
        let joints = items
            .iter()
            .find(|i| i.name == "joints_splices_fasteners")
            .unwrap();
        let fraction = joints.mass_kg / inv.sized_box_mass_kg;
        assert!((0.03..0.15).contains(&fraction), "{fraction}");
    }

    #[test]
    fn a_composite_box_has_no_stiffness_penalty() {
        let inv = inventory("B787-9");
        let items = inv.non_optimum.unwrap();
        let find = |n: &str| items.iter().find(|i| i.name == n).unwrap().mass_kg;
        assert_eq!(find("torsional_stiffness"), 0.0);
        assert!(find("joints_splices_fasteners") > 0.0);
    }

    fn gear_fraction(name: &str) -> f64 {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = presets::get(name).unwrap().design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let mut requirements = config.requirements.clone();
        requirements.mtow_kg = structural_design_mass_kg(&config);
        let (primary, _) = sized_primary_wing(&config, &design, &plane, &requirements).unwrap();
        primary
            .main_gear_wing_fraction
            .expect("resolved from the built geometry")
    }

    #[test]
    fn only_the_gear_legs_attached_to_the_wing_are_charged() {
        // Torenbeek Eq. 11.59 is the provision for wing-mounted gear. Low-wing
        // two-leg gear is all on the wing; the body legs of a multi-leg gear
        // (A340 centreline, A380 two body legs) are not; the ATR's sponson
        // gear under a high wing is fuselage-mounted.
        assert_eq!(gear_fraction("A320-200"), 1.0);
        assert!((gear_fraction("A340-300") - 2.0 / 3.0).abs() < 1e-12);
        assert!((gear_fraction("A380-800") - 0.5).abs() < 1e-12);
        assert_eq!(gear_fraction("ATR72-600"), 0.0);
    }
}
