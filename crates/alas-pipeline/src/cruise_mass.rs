// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mid-cruise weight for the REPORTED cruise lift coefficient and lift-to-drag
//! ratio.
//!
//! `DesignRequirements::required_cruise_cl` sizes the lift coefficient at the
//! takeoff mass. That is the right bound for the stall margin, the trim
//! solution, the body-attitude window and the optimizer residuals, and the
//! trimmed design point (`trim_ih_deg`, `geometric_body_alpha_deg`) stays
//! there. An aircraft nonetheless cruises lighter, so the reported cruise CL
//! and L/D are moved to the mid-cruise mass along the analysed polar: the
//! mean of the takeoff mass and the takeoff mass less the cruise fuel.
//!
//! Units: masses kg, dynamic pressure Pa, area m^2, gravity m/s^2; lift and
//! drag coefficients refer to the report's reference area.

use std::collections::HashMap;

use alas_aero::analysis::PolarSweep;
use alas_mass::breakdown::{OEW_KEYS, PAYLOAD};

/// Level-flight lift coefficient `CL = m g / (q S)` at an arbitrary mass, for
/// gravity `gravity_m_s2` (m/s^2), dynamic pressure `q_pa` (Pa) and reference
/// area `wing_area_m2` (m^2).
///
/// The same expression `required_cruise_cl` evaluates at the takeoff mass.
pub fn cruise_cl_at_mass(mass_kg: f64, gravity_m_s2: f64, q_pa: f64, wing_area_m2: f64) -> f64 {
    mass_kg * gravity_m_s2 / (q_pa * wing_area_m2)
}

/// Mid-cruise mass, kg: the mean of the Breguet integral's own endpoints, the
/// takeoff mass and the takeoff mass less `fuel_burned_kg`.
///
/// The fuel is clamped to `[0, takeoff_mass_kg]`, so a non-positive or
/// non-finite fuel (for example a zero-fuel mass above the takeoff mass)
/// yields the takeoff mass, and the end mass never goes negative.
pub fn mid_cruise_mass_kg(takeoff_mass_kg: f64, fuel_burned_kg: f64) -> f64 {
    let fuel_kg = if fuel_burned_kg.is_finite() {
        fuel_burned_kg.clamp(0.0, takeoff_mass_kg.max(0.0))
    } else {
        0.0
    };
    takeoff_mass_kg - 0.5 * fuel_kg
}

/// A value read off the polar at a lift coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarLookup {
    /// The interpolated, or end-clamped, value.
    pub value: f64,
    /// Whether the lift coefficient lay outside the polar's range, so the
    /// value is the polar's end value rather than an interpolation.
    pub clamped: bool,
}

/// `values` at lift coefficient `cl`, linearly interpolated along the polar.
///
/// Only sweep points whose CL and value are both finite are used, in sweep
/// order. When more than one consecutive pair brackets `cl` (a polar that is
/// not monotonic in CL, for example past the lift peak), the bracket nearest
/// `anchor_cl` along the sweep is used: the anchor is placed at the first
/// sweep point nearest it in CL, and the bracket fewest sweep points away from
/// it wins, ties going to the earlier sweep point. Comparing CL intervals
/// instead cannot tell the attached and post-stall branches apart, since both
/// cover the same CL values. A `cl` outside the polar's CL range returns
/// the value at the extreme-CL point on that side and is flagged `clamped`.
/// `None` only for a non-finite `cl` or a polar without a finite point.
fn interpolate_at_cl(
    polar_cl: &[f64],
    values: &[f64],
    cl: f64,
    anchor_cl: f64,
) -> Option<PolarLookup> {
    if !cl.is_finite() {
        return None;
    }
    let anchor_cl = if anchor_cl.is_finite() { anchor_cl } else { cl };
    let points: Vec<(f64, f64)> = polar_cl
        .iter()
        .zip(values)
        .filter(|(c, v)| c.is_finite() && v.is_finite())
        .map(|(&c, &v)| (c, v))
        .collect();
    let (lowest, highest) = points.iter().fold((None, None), |(lo, hi), &point| {
        let lo = match lo {
            Some((c, _)) if c <= point.0 => lo,
            _ => Some(point),
        };
        let hi = match hi {
            Some((c, _)) if c >= point.0 => hi,
            _ => Some(point),
        };
        (lo, hi)
    });
    let ((cl_min, value_at_min), (cl_max, value_at_max)) = (lowest?, highest?);
    if cl < cl_min {
        return Some(PolarLookup {
            value: value_at_min,
            clamped: true,
        });
    }
    if cl > cl_max {
        return Some(PolarLookup {
            value: value_at_max,
            clamped: true,
        });
    }
    // The anchor's own place on the sweep: the first point nearest it in CL,
    // which on a sweep of increasing incidence is the attached-flow branch.
    let mut anchor_index = 0;
    for (index, &(point_cl, _)) in points.iter().enumerate() {
        if (point_cl - anchor_cl).abs() < (points[anchor_index].0 - anchor_cl).abs() {
            anchor_index = index;
        }
    }
    let mut best: Option<(usize, f64)> = None;
    for (index, pair) in points.windows(2).enumerate() {
        let ((cl_a, v_a), (cl_b, v_b)) = (pair[0], pair[1]);
        if cl < cl_a.min(cl_b) || cl > cl_a.max(cl_b) {
            continue;
        }
        // Sweep points between this bracket (points `index`, `index + 1`) and
        // the anchor point.
        let distance = if anchor_index < index {
            index - anchor_index
        } else {
            anchor_index.saturating_sub(index + 1)
        };
        if best.is_some_and(|(best_distance, _)| best_distance <= distance) {
            continue;
        }
        let value = if cl_b == cl_a {
            v_a
        } else {
            v_a + (v_b - v_a) * (cl - cl_a) / (cl_b - cl_a)
        };
        best = Some((distance, value));
    }
    // A single finite point has no pair; `cl` then equals its CL.
    let value = best.map_or(value_at_min, |(_, value)| value);
    Some(PolarLookup {
        value,
        clamped: false,
    })
}

/// The polar's L/D at lift coefficient `cl`; see [`interpolate_at_cl`] for the
/// bracket choice (nearest `anchor_cl`) and the end-value clamping.
pub fn polar_l_over_d_at_cl(polar: &PolarSweep, cl: f64, anchor_cl: f64) -> Option<PolarLookup> {
    interpolate_at_cl(&polar.cl, &polar.l_over_d, cl, anchor_cl)
}

/// The polar's total drag coefficient at lift coefficient `cl`; same bracket
/// and clamping rules as [`polar_l_over_d_at_cl`].
pub fn polar_cd_at_cl(polar: &PolarSweep, cl: f64, anchor_cl: f64) -> Option<PolarLookup> {
    interpolate_at_cl(&polar.cl, &polar.cd, cl, anchor_cl)
}

/// A cruise operating point moved along the polar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruisePoint {
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient, including the anchor's trim-drag increment.
    pub cd: f64,
    /// `cl / cd`.
    pub l_over_d: f64,
    /// Whether a polar lookup had to clamp to the polar's end values.
    pub clamped: bool,
}

/// The operating point at lift coefficient `cl`, anchored on a reported point
/// (`anchor_cl`, `anchor_cd`).
///
/// The anchor may carry drag the untrimmed sweep does not, chiefly trim drag.
/// That increment, `anchor_cd - CD_polar(anchor_cl)`, is carried unchanged to
/// `cl`: `CD = CD_polar(cl) + (anchor_cd - CD_polar(anchor_cl))`. The
/// increment does not depend on which point along the same polar is chosen as
/// the anchor, so moving a point and then moving it again gives the same
/// answer as one move. `None` when the polar cannot supply either drag value
/// or the result is not a positive finite drag.
pub fn cruise_point_at_cl(
    polar: &PolarSweep,
    cl: f64,
    anchor_cl: f64,
    anchor_cd: f64,
) -> Option<CruisePoint> {
    if !anchor_cd.is_finite() {
        return None;
    }
    let at_cl = polar_cd_at_cl(polar, cl, anchor_cl)?;
    let at_anchor = polar_cd_at_cl(polar, anchor_cl, anchor_cl)?;
    let cd = at_cl.value + (anchor_cd - at_anchor.value);
    (cd.is_finite() && cd > 0.0).then_some(CruisePoint {
        cl,
        cd,
        l_over_d: cl / cd,
        clamped: at_cl.clamped || at_anchor.clamped,
    })
}

/// Zero-fuel mass of a report's component map: the operating empty mass plus
/// the carried payload.
pub fn zero_fuel_mass_kg(component_masses: &HashMap<String, f64>) -> f64 {
    let mass = |key: &str| component_masses.get(key).copied().unwrap_or(0.0);
    OEW_KEYS.iter().map(|key| mass(key)).sum::<f64>() + mass(PAYLOAD)
}

#[cfg(test)]
#[path = "cruise_mass_tests.rs"]
mod tests;
