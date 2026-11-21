// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Anti-icing group of a transport with wing-mounted turboprops.
//!
//! # Why the FLOPS equation is not used on a turboprop
//!
//! The FLOPS anti-icing equation (NASA/TM-2017-219627 Vol. I, eq. 115) charges
//! the span, the nacelle diameters and the fuselage width: hot-air ducting of a
//! jet transport. It has no term for propeller ice protection, and evaluated on
//! a propeller transport it lands far below every itemised turboprop of
//! Beltramo, Trapp, Kimoto and Marsh, *Parametric Study of Transport Aircraft
//! Systems Cost and Weight*, NASA CR-151970 (1977), Table 5.23, p. 5-94
//! (military) : C-130A 932 lb, C-130E 785 lb, C-133B 1,575 lb, where the jets of
//! the same table carry 233-598 lb. The report states the reason (p. 5-96):
//! "the anti-icing weights for aircraft with turbo-prop engines reflect the
//! additional components required for the propeller anti-icing systems".
//!
//! # Relation
//!
//! NASA CR-151970, Fig. 5.35, p. 5-95, and p. 5-96, wing-mounted turboprop
//! engines with tail anti-icing, an average over the turboprop configuration:
//!
//! `W12 = 0.520 Sw`, lb, with `Sw` the wing area in ft^2.
//!
//! # Evidence and domain
//!
//! The line is the report's, unchanged; nothing in it is per aircraft. Its
//! population is the three four-engine turboprops above (wing areas
//! 1,745-2,673 ft^2), all with wing, tail, propeller, intake and windshield
//! protection. It applies to every transport whose engines are all
//! wing-mounted shaft-power engines; any other installation keeps equation 115.
//! Below 1,745 ft^2 (the regional turboprops) the line passes through the
//! origin and is extrapolated; on the regional-turboprop cabin method the
//! systems group is closed by the Torenbeek relation, so there it only moves
//! mass from the furnishings remainder into this group and leaves the empty
//! mass unchanged. The tail share of the C-130E (ducting, controls and
//! supports, 145 of 785 lb, 18 %) is carried as the uncertainty of an aircraft
//! whose tail protection is not declared; no turboprop line without tail
//! anti-icing is published.
//!
//! Itemised, the turboprop excess over the jets sits mostly in the wing and
//! intake ducting and the windshield items of Table 5.23; the propeller row of
//! the anti-icing group is 5-43 lb. Blade heater mats integral to a propeller
//! assembly mass taken from a type certificate overlap the line by at most
//! that row, so nothing is subtracted.

use crate::flops_transport::units::{ft2, kg};
use crate::flops_transport::{FlopsTransportInputs, PropulsionSizing};

/// NASA CR-151970 p. 5-96 coefficient, lb per ft^2 of wing area, wing-mounted
/// turboprop engines with tail anti-icing.
pub(super) const TURBOPROP_LB_PER_FT2: f64 = 0.520;

/// Whether every engine is a wing-mounted shaft-power engine.
pub(super) fn is_wing_mounted_turboprop(inputs: &FlopsTransportInputs) -> bool {
    matches!(inputs.propulsion_sizing, PropulsionSizing::ShaftPower { .. })
        && inputs.engine_count > 0
        && inputs.wing_mounted_engine_count == inputs.engine_count
}

/// The anti-icing group, kg: the NASA CR-151970 turboprop line for a
/// wing-mounted turboprop installation, otherwise FLOPS equation 115 with the
/// scaled nacelle diameter `FNAC` and engine count `FNENG`.
pub(super) fn anti_ice_group_kg(
    inputs: &FlopsTransportInputs,
    scaled_nacelle_diameter_m: f64,
    scaled_engines: f64,
) -> f64 {
    if is_wing_mounted_turboprop(inputs) {
        kg(TURBOPROP_LB_PER_FT2 * ft2(inputs.wing_area_m2))
    } else {
        super::anti_ice_kg(
            inputs.wing_span_m,
            inputs.quarter_chord_sweep_deg,
            scaled_nacelle_diameter_m,
            scaled_engines,
            inputs.fuselage_width_m,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line against the itemised turboprops of Table 5.23: C-130E 785 lb
    /// and C-130A 932 lb on 1,745 ft^2 (907 lb on the line), C-133B 1,575 lb
    /// on 2,673 ft^2 (1,390 lb). The line is their average, so each sits
    /// within 15 % of it.
    #[test]
    fn the_turboprop_line_reproduces_the_itemised_turboprops() {
        for (area_ft2, actual_lb) in [(1_745.0, 785.0), (1_745.0, 932.0), (2_673.0, 1_575.0)] {
            let line_lb = TURBOPROP_LB_PER_FT2 * area_ft2;
            assert!((actual_lb / line_lb - 1.0).abs() < 0.15, "{actual_lb} vs {line_lb}");
        }
    }
}
