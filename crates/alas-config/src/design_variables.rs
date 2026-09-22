// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/design_variables.py
// Reference: alas @ rust-port-baseline.

//! The design space the optimizer searches.
//!
//! Sixteen degrees of freedom. In the scripts this program grew out of they
//! were a bare list addressed by index (`x[10]`, `x[4]`) which made every
//! call site a place to get the ordering wrong silently. Here they are named,
//! and the flat vector the optimizer works in is derived from the names
//! rather than the other way round.
//!
//! # One table, two views
//!
//! The same sixteen variables have to appear as an ordered list of bounds for
//! the optimizer and as named fields for the geometry builder, and those two
//! views drifting apart would mean the optimizer perturbing one variable
//! while the builder read another. Upstream keeps them in two places and
//! asserts at import that they agree. Here one table generates both, so the
//! failure it guards against cannot be written down: the assertion still
//! exists as a test, but it is now checking a property the construction
//! already guarantees rather than one a maintainer has to preserve.
//!
//! This is not part of the settings form, nothing here carries form
//! metadata upstream either. The design space is presented as its own table,
//! which is what [`SPECS`] is for.

use serde::{Deserialize, Serialize};

/// Metadata describing one design degree of freedom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DesignVariableSpec {
    /// The variable's name, which is also its field name on [`DesignVector`].
    pub name: &'static str,
    /// Its nominal value.
    pub default: f64,
    /// The optimizer's lower bound.
    pub lower: f64,
    /// The optimizer's upper bound.
    pub upper: f64,
    /// Lower cross-preset guardrail used when a registered aircraft is
    /// selected as the centre of a local redesign study.
    #[serde(skip)]
    pub preset_lower: f64,
    /// Upper cross-preset guardrail used when a registered aircraft is
    /// selected as the centre of a local redesign study.
    #[serde(skip)]
    pub preset_upper: f64,
    /// Absolute fallback scale for a symmetric local sweep around a zero
    /// nominal value, in the variable's native units.
    #[serde(skip)]
    pub preset_local_scale: f64,
    /// Its physical unit, or `-` when dimensionless.
    pub unit: &'static str,
    /// What it means, in English.
    pub description: &'static str,
    /// Decimal places to show, for display only.
    ///
    /// These are full-precision values internally (an optimizer result, or
    /// a preset's exact vector) and rendering that precision raw reads as
    /// noise rather than information. The count is chosen per variable's
    /// working resolution, which is why the surface bumps get more of them
    /// than the spans do: their entire useful range spans about `0.005`, so
    /// two decimals would round every value to the same handful of steps.
    pub decimals: i64,
}

/// Why a flat vector could not be read as a design vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected {expected} design variables, got {actual}")]
pub struct DesignVectorError {
    /// How many the design space has.
    pub expected: usize,
    /// How many were supplied.
    pub actual: usize,
}

/// Declare the design space once, and derive from it both the named vector
/// and the ordered spec table.
///
/// The order the variables are written here is the order of the flat vector
/// the optimizer operates on, and there is no second place that order is
/// recorded.
macro_rules! design_space {
    (
        $(
            $name:ident: $default:expr, $lower:expr, $upper:expr,
            $preset_lower:expr, $preset_upper:expr, $preset_local_scale:expr,
            $unit:literal, $decimals:literal, $description:literal;
        )*
    ) => {
        /// One candidate design, named rather than indexed.
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct DesignVector {
            $(
                #[doc = $description]
                pub $name: f64,
            )*
        }

        /// Every design variable, in the order of the flat optimizer vector.
        pub const SPECS: &[DesignVariableSpec] = &[
            $(
                DesignVariableSpec {
                    name: stringify!($name),
                    default: $default,
                    lower: $lower,
                    upper: $upper,
                    preset_lower: $preset_lower,
                    preset_upper: $preset_upper,
                    preset_local_scale: $preset_local_scale,
                    unit: $unit,
                    description: $description,
                    decimals: $decimals,
                },
            )*
        ];

        impl Default for DesignVector {
            fn default() -> Self {
                Self { $( $name: $default, )* }
            }
        }

        impl DesignVector {
            /// Flatten to the ordered vector the optimizer operates on.
            pub fn to_array(&self) -> Vec<f64> {
                vec![ $( self.$name, )* ]
            }

            /// Rebuild a named vector from a flat one.
            ///
            /// # Errors
            ///
            /// [`DesignVectorError`] when the length does not match the
            /// design space. Upstream raises for the same input.
            pub fn from_array(values: &[f64]) -> Result<Self, DesignVectorError> {
                if values.len() != SPECS.len() {
                    return Err(DesignVectorError {
                        expected: SPECS.len(),
                        actual: values.len(),
                    });
                }
                // Struct field initializers evaluate in the order written,
                // which is the order this macro was invoked in, which is the
                // order `to_array` writes and `SPECS` lists.
                let mut index = 0;
                let mut next = || {
                    let value = values[index];
                    index += 1;
                    value
                };
                Ok(Self { $( $name: next(), )* })
            }
        }
    };
}

design_space! {
    span_m: 71.75, 60.0, 80.0, 20.0, 90.0, 1.0, "m", 2, "Full projected wingspan (tip to tip)";
    root_chord_m: 16.50, 12.0, 19.0, 3.0, 26.0, 1.0, "m", 2, "Chord at the wing root";
    break_chord_m: 7.80, 6.0, 10.0, 2.0, 14.0, 1.0, "m", 2, "Chord at the trailing-edge break (yehudi)";
    tip_chord_m: 1.60, 1.0, 3.0, 0.5, 4.0, 1.0, "m", 2, "Chord at the wingtip";
    sweep_deg: 34.00, 25.0, 45.0, 0.0, 45.0, 1.0, "deg", 2, "Inboard leading-edge sweep angle";
    tip_twist_deg: 0.00, -5.0, 1.0, -5.0, 2.0, 1.0, "deg", 2, "Geometric washout at tip (negative = washout)";
    wing_x_shift_m: 0.00, -5.0, 8.0, -10.0, 5.0, 1.0, "m", 2, "Longitudinal shift of the wing root for CG balance";
    tail_scale: 1.00, 0.75, 1.25, 0.5, 1.5, 1.0, "-", 3, "Uniform scale factor on the empennage";
    fuselage_length_m: 76.72, 65.0, 85.0, 20.0, 90.0, 1.0, "m", 2, "Overall fuselage length";
    tail_x_shift_m: 0.00, -2.0, 3.0, -5.0, 5.0, 1.0, "m", 2, "Longitudinal shift of the empennage";
    airfoil_thickness_scale: 1.00, 0.80, 1.30, 0.5, 1.5, 1.0, "-", 3, "Multiplier on root/break airfoil thickness";
    airfoil_camber_scale: 1.00, 0.7, 1.4, 0.5, 1.5, 1.0, "-", 3, "Multiplier on root/break airfoil camber";
    bump_upper_front: 0.00, -0.005, 0.002, -0.01, 0.005, 0.005, "-", 4,
        "Hicks-Henne bump, upper surface ~25% chord (suction)";
    bump_upper_rear: 0.00, -0.005, 0.002, -0.01, 0.005, 0.005, "-", 4,
        "Hicks-Henne bump, upper surface ~75% chord (shock/recovery)";
    bump_lower_mid: 0.00, -0.005, 0.003, -0.01, 0.006, 0.005, "-", 4,
        "Hicks-Henne bump, lower surface ~40% chord (belly volume)";
    bump_lower_rear: 0.00, -0.005, 0.003, -0.01, 0.006, 0.005, "-", 4,
        "Hicks-Henne bump, lower surface ~85% chord (rear loading)";
}

impl DesignVector {
    /// The `(lower, upper)` bounds, in vector order.
    pub fn bounds() -> Vec<(f64, f64)> {
        SPECS.iter().map(|spec| (spec.lower, spec.upper)).collect()
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_vector_and_the_spec_table_describe_the_same_space() {
        // Upstream asserts this at import because its two views are written
        // twice. Here they come from one table, so this checks a property the
        // construction already guarantees, which is the point: the failure
        // it used to guard against can no longer be expressed.
        let vector = DesignVector::default();
        assert_eq!(vector.to_array().len(), SPECS.len());
        for (value, spec) in vector.to_array().iter().zip(SPECS) {
            assert_eq!(*value, spec.default, "{}", spec.name);
        }
    }

    #[test]
    fn the_design_space_has_sixteen_degrees_of_freedom() {
        assert_eq!(SPECS.len(), 16);
    }

    #[test]
    fn a_vector_survives_a_round_trip_through_the_flat_array() {
        let original = DesignVector {
            span_m: 65.0,
            sweep_deg: 30.0,
            bump_lower_rear: -0.001,
            ..Default::default()
        };
        let rebuilt = DesignVector::from_array(&original.to_array()).unwrap();
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn the_flat_array_is_in_the_order_the_spec_table_lists() {
        // The optimizer perturbs by index and the geometry builder reads by
        // name; if these two orders disagreed, it would move one variable and
        // the builder would see another, with no error anywhere.
        let mut values = vec![0.0; SPECS.len()];
        let sweep = SPECS.iter().position(|s| s.name == "sweep_deg").unwrap();
        values[sweep] = 42.0;
        assert_eq!(DesignVector::from_array(&values).unwrap().sweep_deg, 42.0);
    }

    #[test]
    fn a_vector_of_the_wrong_length_is_an_error_not_a_panic() {
        assert_eq!(
            DesignVector::from_array(&[1.0, 2.0]).unwrap_err(),
            DesignVectorError {
                expected: 16,
                actual: 2
            }
        );
    }

    #[test]
    fn every_default_lies_inside_its_own_bounds() {
        // A default outside its bounds means the nominal design is not a
        // candidate the search could ever return, which reads as the
        // optimizer refusing to reproduce its own starting point.
        for spec in SPECS {
            assert!(spec.lower < spec.upper, "{}: empty range", spec.name);
            assert!(
                spec.default >= spec.lower && spec.default <= spec.upper,
                "{}: default {} is outside [{}, {}]",
                spec.name,
                spec.default,
                spec.lower,
                spec.upper
            );
        }
    }

    #[test]
    fn the_bounds_list_is_what_the_optimizer_is_handed() {
        let bounds = DesignVector::bounds();
        assert_eq!(bounds.len(), SPECS.len());
        assert_eq!(bounds[0], (60.0, 80.0));
    }

    #[test]
    fn a_variable_is_shown_to_the_precision_it_actually_resolves() {
        // The surface bumps span about 0.005 end to end, so displaying them
        // at the spans' two decimals would round every value to the same few
        // steps and hide the whole variable.
        let bump = SPECS.iter().find(|s| s.name == "bump_upper_front").unwrap();
        let span = SPECS.iter().find(|s| s.name == "span_m").unwrap();
        assert!(bump.decimals > span.decimals);
        assert!(bump.upper - bump.lower < 0.01);
    }
}
