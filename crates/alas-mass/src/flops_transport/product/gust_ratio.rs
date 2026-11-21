// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing loads FLOPS does not see, priced relative to the registered
//! aircraft's: the discrete-gust load and the aeroelastic stiffness weight.
//!
//! # Discrete gust
//!
//! The FLOPS wing equations take the ultimate load factor as an input, and the
//! requirements carry only the CS 25.337 manoeuvre value (`n_ult = 1.5 n_man`).
//! The CS 25.341 discrete gust (Pratt formula, Torenbeek, *Advanced Aircraft
//! Design*, 2013, Eqs. 11.7-11.8, pp. 325-326; CS-25 Amdt 27, pp. 218-219)
//! rises with the lift-curve slope, so a wing that has been stretched to a
//! higher aspect ratio carries a larger gust load than the registered wing
//! the FLOPS fit was calibrated on. The design load factor the wing is priced
//! at is therefore
//!
//! `n_ult_wing = n_ult * max(n_man, n_gust) / max(n_man, n_gust_registered)`,
//!
//! with `n_man = n_ult / 1.5` and `n_gust` the larger of the V_C and V_D
//! limit load factors of the wing planform (area, span, half-chord sweep) at
//! the design gross mass, the same speeds and altitude. Only the *ratio* to
//! the registered aircraft enters, so the registered aircraft is priced
//! exactly as before (ratio 1, bit-identical) and the term is a physical
//! trade (gust load against span), not a new calibration. It applies wherever
//! the configuration names a registered aircraft
//! ([`FlopsStructureConfig::gust_reference_preset`]); a configuration without
//! one is priced at the manoeuvre load alone.
//!
//! # Aeroelastic stiffness (flutter, aileron reversal)
//!
//! Torenbeek 2013, Eq. 11.62 (book p. 344) gives the stiffness weight of an
//! Al-alloy wing, [`alas_struct::non_optimum::torsional_stiffness_mass_kg`]:
//! `W_ref/g (q_D/q_ref)(b cos L0/b_ref)^3 (t/c)^-2 (1 - sin L0.5)[1 - (M_D cos L0.5)^2]^-1/2`
//! with the book's typical `W_ref = 200 N` (used when no in-house data exist,
//! as in `non_optimum`), `q_ref = 30 kPa`, `b_ref = 50 m`. `q_D` is the
//! sea-level equivalent dive pressure `0.5 rho_0 V_D^2` and `M_D = M_C + 0.07`
//! (AMC 25.335(b)), as in `wing_reconciliation::non_optimum`; `t/c` is the
//! FLOPS chord-weighted mean thickness. The registered aircraft's FLOPS wing
//! already contains the stiffness material its own planform needed, so only
//! the excess `max(0, W_st - W_st_registered)` is added, to the wing bending
//! material and the structural group. Because it is a difference, the absolute
//! `W_ref` matters linearly: 200 N is the book's stated default, not a
//! calibration to a wing. It is zero at the registered aircraft (bit-identical
//! mass), without a reference, and for a composite wing (`FCOMP > 0`), for
//! which the book gives no stiffness penalty.
//!
//! No double count: `wing_reconciliation::non_optimum` carries the same
//! equation but only places mass inside the wing (centroid) in the clean-sheet
//! inventory, and is never reached in reference adaptation or sandbox mode;
//! the FLOPS total built here is the authoritative wing mass in every mode.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use alas_config::{DesignRequirements, FlopsStructureConfig};
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_struct::gust::{critical_discrete_gusts, GustSpeeds, GustWing};
use alas_struct::non_optimum::torsional_stiffness_mass_kg;
use alas_struct::sizing::envelope::design_cruise_eas_m_s;

use super::super::airframe_geometry::average_thickness;
use super::super::structure::{
    estimate_flops_structure, FlopsStructureBreakdown, FlopsStructureInputs,
};

/// Factor of safety on the limit load, `n_ult = 1.5 n_limit` (CS 25.303).
const FACTOR_OF_SAFETY: f64 = 1.5;
/// Largest departure from 1 treated as floating-point noise of two builds of
/// the same planform.
const SAME_PLANFORM: f64 = 1.0e-9;

/// Sea-level air density defining equivalent airspeed, kg/m^3.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// The planform the gust formula reads, from the carried-box main wing.
fn gust_wing(wing: &Wing) -> GustWing {
    GustWing {
        area_m2: wing.reference_area(),
        span_m: wing.reference_span(),
        tan_half_chord_sweep: wing.mean_sweep_angle(0.5).to_radians().tan(),
    }
}

/// The critical (larger of V_C, V_D) discrete-gust limit load factor.
fn critical_gust_load_factor(
    wing: &Wing,
    requirements: &DesignRequirements,
    mass_kg: f64,
) -> Option<f64> {
    let speeds = GustSpeeds {
        cruise_eas_m_s: design_cruise_eas_m_s(requirements),
        dive_eas_m_s: requirements.dive_speed_m_s,
        cruise_mach: requirements.design_cruise_mach_limit(),
    };
    let gusts = critical_discrete_gusts(
        &gust_wing(wing),
        mass_kg,
        requirements.gravity_m_s2,
        &speeds,
        requirements.cruise_altitude_m,
    )?;
    let n = gusts[0]
        .limit_load_factor()
        .max(gusts[1].limit_load_factor());
    n.is_finite().then_some(n)
}

/// What the candidate is compared with, built once per process.
#[derive(Clone, Copy)]
struct Registered {
    gust_load_factor: Option<f64>,
    stiffness_kg: f64,
}

fn registered(preset_name: &str) -> Registered {
    static CACHE: OnceLock<Mutex<HashMap<String, Registered>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache
        .lock()
        .ok()
        .and_then(|held| held.get(preset_name).copied())
    {
        return hit;
    }
    let value = compute_registered(preset_name).unwrap_or(Registered {
        gust_load_factor: None,
        stiffness_kg: 0.0,
    });
    if let Ok(mut held) = cache.lock() {
        held.insert(preset_name.to_owned(), value);
    }
    value
}

fn compute_registered(preset_name: &str) -> Option<Registered> {
    let preset = alas_config::presets::get(preset_name).ok()?;
    let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), false)
        .ok()?;
    let wing = super::main_wing(&plane)?.carried_box();
    let design_gross_kg = alas_config::preset_flops::inputs_for(preset_name)
        .and_then(|inputs| inputs.structure.design_gross_mass_kg)
        .unwrap_or(preset.requirements.mtow_kg);
    Some(Registered {
        gust_load_factor: critical_gust_load_factor(&wing, &preset.requirements, design_gross_kg),
        stiffness_kg: stiffness_kg(&wing, &preset.requirements),
    })
}

/// Eq. 11.62 of the wing, kg.
fn stiffness_kg(wing: &Wing, requirements: &DesignRequirements) -> f64 {
    let v_d = requirements.dive_speed_m_s;
    torsional_stiffness_mass_kg(
        wing.reference_span(),
        wing.mean_sweep_angle(0.0).to_radians(),
        wing.mean_sweep_angle(0.5).to_radians(),
        0.5 * SEA_LEVEL_DENSITY_KG_M3 * v_d * v_d,
        requirements.design_dive_mach(),
        average_thickness(wing),
    )
}

/// The stiffness weight above the registered aircraft's, kg; zero without a
/// reference, for a composite wing, or when not larger.
fn stiffness_increment_kg(
    requirements: &DesignRequirements,
    technology: &FlopsStructureConfig,
    wing: &Wing,
) -> f64 {
    let Some(name) = technology.gust_reference_preset.as_deref() else {
        return 0.0;
    };
    if technology.composite_utilization > 0.0 {
        return 0.0;
    }
    let excess = stiffness_kg(wing, requirements) - registered(name).stiffness_kg;
    if excess.is_finite() && excess > 0.0 {
        excess
    } else {
        0.0
    }
}

/// [`estimate_flops_structure`] with the stiffness excess of the wing added.
pub fn estimate_priced_structure(
    inputs: &FlopsStructureInputs,
    requirements: &DesignRequirements,
    technology: &FlopsStructureConfig,
    wing: &Wing,
) -> FlopsStructureBreakdown {
    let mut structure = estimate_flops_structure(inputs);
    let excess = stiffness_increment_kg(requirements, technology, wing);
    if excess > 0.0 {
        structure.wing.bending_material_kg += excess;
        structure.wing.total_kg += excess;
        structure.total_kg += excess;
    }
    structure
}

/// Ratio `max(n_man, n_gust) / max(n_man, n_gust_registered)` of the candidate
/// wing; exactly 1.0 without a registered reference or for the registered wing.
pub fn gust_load_ratio(
    requirements: &DesignRequirements,
    technology: &FlopsStructureConfig,
    wing: &Wing,
    design_gross_mass_kg: f64,
) -> f64 {
    let Some(name) = technology.gust_reference_preset.as_deref() else {
        return 1.0;
    };
    let (Some(reference), Some(candidate)) = (
        registered(name).gust_load_factor,
        critical_gust_load_factor(wing, requirements, design_gross_mass_kg),
    ) else {
        return 1.0;
    };
    let n_man = requirements.design_ultimate_load_factor() / FACTOR_OF_SAFETY;
    let ratio = n_man.max(candidate) / n_man.max(reference);
    if ratio.is_finite() && (ratio - 1.0).abs() > SAME_PLANFORM {
        ratio
    } else {
        1.0
    }
}

/// The ultimate load factor the FLOPS wing equations are evaluated at.
pub fn wing_ultimate_load_factor(
    requirements: &DesignRequirements,
    technology: &FlopsStructureConfig,
    wing: &Wing,
    design_gross_mass_kg: f64,
) -> f64 {
    requirements.design_ultimate_load_factor()
        * gust_load_ratio(requirements, technology, wing, design_gross_mass_kg)
}
