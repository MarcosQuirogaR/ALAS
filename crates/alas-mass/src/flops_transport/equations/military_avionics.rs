// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Avionics group of a military transport.
//!
//! # Why the FLOPS equation is not used on a military transport
//!
//! The FLOPS avionics equation sizes the group on design range, flight crew
//! and fuselage planform: a civil airliner fit. Military transports carry
//! mission avionics an airliner of the same size does not (tactical navigation,
//! secure communications, IFF, station keeping, aerial-delivery guidance). The
//! itemised military transports of Beltramo, Trapp, Kimoto and Marsh,
//! *Parametric Study of Transport Aircraft Systems Cost and Weight*, NASA
//! CR-151970 (1977), Table 5.29, p. 5-114, carry 2,630-4,130 lb (C-130E 2,657,
//! C-133B 2,630, C-141A 2,938, C-5A 4,130 lb), on the report's "Category III
//! overwater or military" line, while the FLOPS equation prices a 141 t
//! four-turboprop military transport at about 2,170 lb, below the 1950s
//! C-130E.
//!
//! # Relation
//!
//! NASA CR-151970, p. 5-117 (Fig. 5.41, p. 5-116), military aircraft, "derived
//! by correlating the military avionics weight with body wetted area":
//!
//! `W16 = 0.10 Sb + 2,330`, lb, with `Sb` the body wetted area in ft^2.
//!
//! # Evidence and domain
//!
//! The relation is the report's, unchanged; nothing in it is per aircraft.
//! `Sb` is the wetted area of the built fuselage. It applies to an aircraft
//! that declares a military freight hold, within the report's military data:
//! from the C-130E (`Sb` 3,339 ft^2, Table 5.25, p. 5-100) upwards. A smaller
//! military transport keeps the FLOPS equation, because the 2,330 lb intercept
//! is the mission fit of a 70 t-class transport and would be extrapolated to a
//! body the report has no datum for. Every passenger transport and civil
//! freighter is bit-identical.
//!
//! The four itemised military transports of Table 5.29 against the line, with
//! their body wetted areas of Table 5.25: C-130E 2,657 lb on 3,339 ft^2 (line
//! 2,664, -0.3 %), C-141A 2,938 lb on 5,096 ft^2 (2,840, +3.5 %), C-133B
//! 2,630 lb on 6,900 ft^2 (3,020, -12.9 %) and C-5A 4,130 lb on 16,507 ft^2
//! (3,981, +3.7 %). The line is the report's, not refitted here; the C-133B
//! scatter is carried as its uncertainty.

use crate::flops_transport::units::{ft2, kg};
use crate::flops_transport::FlopsTransportInputs;

/// NASA CR-151970 p. 5-117, military avionics: lb per ft^2 of body wetted area.
pub(super) const MILITARY_LB_PER_FT2: f64 = 0.10;
/// NASA CR-151970 p. 5-117, military avionics: intercept, lb.
pub(super) const MILITARY_INTERCEPT_LB: f64 = 2_330.0;
/// Smallest body wetted area of the report's military data, ft^2 (C-130E,
/// Table 5.25, p. 5-100).
pub(super) const MIN_MILITARY_BODY_WETTED_FT2: f64 = 3_339.0;

/// The avionics group, kg: the NASA CR-151970 military relation for a
/// declared military freight hold whose body lies within the report's military
/// data, otherwise the FLOPS avionics equation.
pub(super) fn avionics_group_kg(
    inputs: &FlopsTransportInputs,
    fuselage_planform_area_m2: f64,
) -> f64 {
    match inputs.military_cargo {
        Some(hold)
            if hold.military
                && ft2(hold.body_wetted_area_m2) >= MIN_MILITARY_BODY_WETTED_FT2 =>
        {
            kg(MILITARY_LB_PER_FT2 * ft2(hold.body_wetted_area_m2) + MILITARY_INTERCEPT_LB)
        }
        _ => super::avionics_kg(
            inputs.design_range_nmi,
            inputs.flight_crew_count,
            fuselage_planform_area_m2,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relation against the four itemised military transports of Table
    /// 5.29 with their body wetted areas of Table 5.25: C-130E, C-141A and C-5A
    /// within 4 %, C-133B 12.9 % below the line.
    #[test]
    fn the_military_relation_reproduces_the_itemised_transports() {
        for (area_ft2, actual_lb, tolerance) in [
            (3_339.0, 2_657.0, 0.04),
            (5_096.0, 2_938.0, 0.04),
            (6_900.0, 2_630.0, 0.15),
            (16_507.0, 4_130.0, 0.04),
        ] {
            let line_lb = MILITARY_LB_PER_FT2 * area_ft2 + MILITARY_INTERCEPT_LB;
            assert!((actual_lb / line_lb - 1.0).abs() < tolerance, "{actual_lb} vs {line_lb}");
        }
    }
}
