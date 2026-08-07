// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/schema.py (`_field_kind`).
// Reference: alas @ rust-port-baseline.

//! Which editor a value's type asks for.
//!
//! The reference decides this at run time by looking at the value, because
//! Python's dataclass fields carry no usable type at that point. Here the
//! decision is a trait implemented once per value type, which gets the same
//! answers with the compiler checking that every configuration field's type
//! has one at all -- a field of some type nobody thought about is a build
//! error rather than a form row reading "unsupported".
//!
//! One classification depends on the value and not only on the type, and is
//! reproduced as such: a positive real whose field name ends in one of the
//! weight suffixes is offered as a slider rather than as a number, because it
//! is a relative weight in an objective function and not a physical quantity.

use serde::Serialize;

use crate::Kind;

/// Field-name endings that mark a real number as a relative weight.
///
/// These are the objective function's knobs -- a term's scale, a per-metre
/// cost, a penalty floor -- and the interface offers them as sliders because
/// only their ratio to each other means anything.
const WEIGHT_SUFFIXES: &[&str] = &["_scale", "_per_m", "_weight", "_floor", "_cost", "_floor_m"];

/// A value a configuration field can hold.
pub trait Leaf: Serialize {
    /// Which editor this value asks for. `name` is the field's identifier,
    /// which the weight-slider rule reads.
    fn kind(&self, name: &str) -> Kind;

    /// The value, as the interface receives it.
    ///
    /// A value that cannot be represented as JSON -- a non-finite float --
    /// becomes null rather than an error: the schema describes a form, and a
    /// form with one unrepresentable default is still worth rendering.
    fn value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl Leaf for bool {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Bool
    }
}

impl Leaf for i64 {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Int
    }
}

impl Leaf for u32 {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Int
    }
}

impl Leaf for usize {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Int
    }
}

impl Leaf for f64 {
    fn kind(&self, name: &str) -> Kind {
        let is_weight = WEIGHT_SUFFIXES.iter().any(|suffix| name.ends_with(suffix));
        if is_weight && *self > 0.0 {
            Kind::WeightSlider
        } else {
            Kind::Float
        }
    }
}

impl Leaf for String {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

impl<T: Leaf> Leaf for Option<T> {
    /// An unset value has no type to classify, so it reports itself unset and
    /// the interface decides what to offer. A set one is classified as
    /// whatever it holds, which is what the reference does by looking at the
    /// value rather than at the declaration.
    fn kind(&self, name: &str) -> Kind {
        match self {
            None => Kind::Optional,
            Some(value) => value.kind(name),
        }
    }
}

impl Leaf for Vec<f64> {
    /// An empty list reports itself unsupported. The reference tests the
    /// list's contents to decide it holds numbers, and an empty list has no
    /// contents to test, so it falls through to the same answer a list of
    /// something unrecognized gets. `deviation-candidate`: an empty list of
    /// numbers is a list of numbers, and a form row reading "unsupported" is
    /// not what a user emptying a table should be shown.
    fn kind(&self, _name: &str) -> Kind {
        if self.is_empty() {
            Kind::Unsupported
        } else {
            Kind::NumberList
        }
    }
}

impl Leaf for Vec<(f64, f64)> {
    /// Empty behaves as it does for a list of numbers, and for the same
    /// upstream reason.
    fn kind(&self, _name: &str) -> Kind {
        if self.is_empty() {
            Kind::Unsupported
        } else {
            Kind::TupleList
        }
    }
}

impl Leaf for (f64, f64) {
    fn kind(&self, _name: &str) -> Kind {
        Kind::NumberList
    }
}

impl Leaf for (f64, f64, f64) {
    fn kind(&self, _name: &str) -> Kind {
        Kind::NumberList
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_positive_real_named_as_a_weight_is_offered_as_a_slider() {
        assert_eq!(2.5_f64.kind("range_weight"), Kind::WeightSlider);
        assert_eq!(0.3_f64.kind("fuel_cost"), Kind::WeightSlider);
    }

    #[test]
    fn a_weight_that_is_switched_off_is_offered_as_a_number() {
        // Zero means the term is disabled, and a slider pinned at its floor
        // reads as a live control that happens to be at the bottom.
        assert_eq!(0.0_f64.kind("range_weight"), Kind::Float);
        assert_eq!((-1.0_f64).kind("range_weight"), Kind::Float);
    }

    #[test]
    fn a_real_that_is_not_named_as_a_weight_stays_a_number() {
        assert_eq!(2.5_f64.kind("wing_area_m2"), Kind::Float);
    }

    #[test]
    fn an_unset_optional_reports_itself_unset_and_a_set_one_reports_its_value() {
        let unset: Option<f64> = None;
        assert_eq!(unset.kind("cruise_altitude_m"), Kind::Optional);
        assert_eq!(Some(2.0_f64).kind("cruise_altitude_m"), Kind::Float);
        assert_eq!(Some(2.0_f64).kind("range_weight"), Kind::WeightSlider);
    }

    #[test]
    fn lists_are_classified_by_what_they_hold() {
        assert_eq!(vec![1.0, 2.0].kind("stations_m"), Kind::NumberList);
        assert_eq!(vec![(1.0, 2.0)].kind("profile"), Kind::TupleList);
        assert_eq!((1.0, 2.0).kind("engine_y_positions_m"), Kind::NumberList);
    }

    #[test]
    fn an_empty_list_reports_itself_unsupported_as_upstream_does() {
        let empty: Vec<f64> = Vec::new();
        assert_eq!(empty.kind("stations_m"), Kind::Unsupported);
    }

    #[test]
    fn a_value_serializes_to_what_the_interface_receives() {
        assert_eq!(true.value(), serde_json::json!(true));
        assert_eq!(3_i64.value(), serde_json::json!(3));
        assert_eq!(vec![(1.0, 2.0)].value(), serde_json::json!([[1.0, 2.0]]));
        let unset: Option<f64> = None;
        assert_eq!(unset.value(), serde_json::Value::Null);
    }

    #[test]
    fn a_value_that_cannot_be_represented_becomes_null_rather_than_an_error() {
        assert_eq!(f64::NAN.value(), serde_json::Value::Null);
    }
}
