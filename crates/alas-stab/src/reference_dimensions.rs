// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower bounds of the reference quantities the stability correlations
//! divide by, and the check that reports an aircraft below them.
//!
//! The correlations keep a denominator away from zero by holding each
//! reference quantity at a floor. For a transport the floors never bind. An
//! aircraft below one (or with a non-finite reference) is evaluated with the
//! floor value instead of its own geometry, so its moment coefficients are
//! rescaled: [reference_dimension_faults] names every such quantity so that
//! a caller can refuse the result or mark it instead of publishing it as
//! computed.

use alas_geom::aircraft::airplane::Airplane;

/// Floor of the reference area, m^2.
pub const REFERENCE_AREA_FLOOR_M2: f64 = 1.0;
/// Floor of the reference chord, m.
pub const REFERENCE_CHORD_FLOOR_M: f64 = 0.1;
/// Floor of the reference span, m.
pub const REFERENCE_SPAN_FLOOR_M: f64 = 1.0;
/// Floor of the largest fuselage width and height, m.
pub const FUSELAGE_SECTION_FLOOR_M: f64 = 0.1;

/// One reference quantity the correlations would replace by its floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferenceDimensionFault {
    /// Quantity name with its SI unit.
    pub quantity: &'static str,
    /// The aircraft's own value (possibly non-finite), SI.
    pub value: f64,
    /// The floor the correlations hold it at, SI.
    pub floor: f64,
}

impl std::fmt::Display for ReferenceDimensionFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} = {} is below the {} the stability correlations are evaluated with",
            self.quantity, self.value, self.floor
        )
    }
}

/// Every reference quantity of `airplane` that is non-finite or below its
/// floor; empty for an aircraft the correlations evaluate on its own
/// geometry.
pub fn reference_dimension_faults(airplane: &Airplane) -> Vec<ReferenceDimensionFault> {
    let mut checks = vec![
        (
            "reference area, m^2",
            airplane.s_ref,
            REFERENCE_AREA_FLOOR_M2,
        ),
        (
            "reference chord, m",
            airplane.c_ref,
            REFERENCE_CHORD_FLOOR_M,
        ),
        ("reference span, m", airplane.b_ref, REFERENCE_SPAN_FLOOR_M),
    ];
    if let Some(fuselage) = airplane.fuselages.first() {
        // A NaN section must surface, so the largest value is not taken with
        // `f64::max` (which drops NaN).
        let largest = |values: &mut dyn Iterator<Item = f64>| {
            values.fold(f64::NEG_INFINITY, |largest, value| {
                if value.is_nan() || largest.is_nan() {
                    f64::NAN
                } else {
                    largest.max(value)
                }
            })
        };
        if !fuselage.xsecs.is_empty() {
            checks.push((
                "largest fuselage width, m",
                largest(&mut fuselage.xsecs.iter().map(|section| section.width)),
                FUSELAGE_SECTION_FLOOR_M,
            ));
            checks.push((
                "largest fuselage height, m",
                largest(&mut fuselage.xsecs.iter().map(|section| section.height)),
                FUSELAGE_SECTION_FLOOR_M,
            ));
        }
    }
    checks
        .into_iter()
        .filter(|(_, value, floor)| !value.is_finite() || value < floor)
        .map(|(quantity, value, floor)| ReferenceDimensionFault {
            quantity,
            value,
            floor,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airplane(s_ref: f64, c_ref: f64, b_ref: f64) -> Airplane {
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: vec![],
            fuselages: vec![],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn a_transport_sized_reference_has_no_fault() {
        assert!(reference_dimension_faults(&airplane(122.4, 4.19, 34.1)).is_empty());
        // Exactly on the floors the correlations use the aircraft's values.
        assert!(reference_dimension_faults(&airplane(1.0, 0.1, 1.0)).is_empty());
    }

    #[test]
    fn a_sub_floor_or_non_finite_reference_is_named() {
        let faults = reference_dimension_faults(&airplane(0.6, f64::NAN, 34.1));
        assert_eq!(faults.len(), 2);
        assert_eq!(faults[0].quantity, "reference area, m^2");
        assert_eq!(faults[0].floor, REFERENCE_AREA_FLOOR_M2);
        assert_eq!(faults[1].quantity, "reference chord, m");
        assert!(faults[1].value.is_nan());
        assert!(faults[0].to_string().contains("reference area"));
    }
}
