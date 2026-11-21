// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`DragDivergenceLaw`]: which drag-divergence relation the main wing takes.
//!
//! Two relations give the drag-divergence Mach number `M_dd` (the Mach of
//! `dCD/dM = 0.10`) of a swept wing from its section thickness ratio `t/c`,
//! lift coefficient and sweep `L`:
//!
//! - Korn's relation on the swept wing,
//!   `M_dd = kappa/cos L - (t/c)/cos^2 L - CL/(10 cos^3 L)`, with the
//!   technology factor `kappa` 0.87 for NACA 6-series and 0.95 for
//!   supercritical sections (W. H. Mason, *Configuration Aerodynamics*,
//!   ch. 7, the Korn equation extended to swept wings; the section form is
//!   E. Torenbeek, *Advanced Aircraft Design*, 2013, Eq. 10.46, p. 301);
//! - Torenbeek's form for second-generation supercritical sections carried
//!   to the wing by simple sweep theory,
//!   `M_dd cos L + (t/c)/cos L + 0.10 (1.1 CL/cos^2 L)^1.5 = M*`,
//!   `M* = 0.935` (*Advanced Aircraft Design*, 2013, Eq. 10.48, p. 302; `M*`
//!   from Eq. 10.47, p. 301).
//!
//! The selection is dimensionless and applies to the product analyses; the
//! frozen comparison paths keep Korn's relation.

use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::AirfoilClass;
use super::WingConfig;
use crate::{Kind, Leaf};

/// The drag-divergence relation the main wing takes
/// (`geometry.wing.drag_divergence_law`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragDivergenceLaw {
    /// The Shevell-Kroo method with the crest-critical Mach increment of the
    /// wing's section generation; a wing with an in-range Korn factor
    /// override takes Korn's relation with that factor.
    #[default]
    Auto,
    /// The Shevell-Kroo crest-critical-Mach method and its compressibility
    /// drag curve, whatever else is declared. It has no Korn factor.
    Shevell,
    /// Korn's relation, with the class factor or the in-range override.
    Korn,
    /// Torenbeek's Eq. 10.48 with `M* = 0.935`, whatever the section class.
    /// It has no Korn factor, so an override is not used.
    Torenbeek1048,
}

/// The relation a [`DragDivergenceLaw`] resolves to on a given wing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceRelation {
    /// Korn's relation with [`WingConfig::korn_technology_factor`].
    Korn,
    /// Torenbeek's Eq. 10.48.
    Torenbeek1048,
    /// The Shevell-Kroo method with
    /// [`crate::AirfoilClass::crest_critical_mach_increment`].
    Shevell,
}

impl DragDivergenceLaw {
    /// Serialized names, in declaration order.
    pub const NAMES: [&'static str; 4] = ["auto", "shevell", "korn", "torenbeek1048"];

    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Shevell => "shevell",
            Self::Korn => "korn",
            Self::Torenbeek1048 => "torenbeek1048",
        }
    }

    /// Whether this is the default, which is not serialized.
    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

impl Leaf for DragDivergenceLaw {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

impl WingConfig {
    /// Whether a Korn factor override is set and inside its legal range.
    fn korn_override_in_range(&self) -> bool {
        self.korn_technology_factor_override.is_some() && self.korn_override_error().is_none()
    }

    /// The relation [`Self::drag_divergence_law`] resolves to on this wing.
    #[must_use]
    pub fn divergence_relation(&self) -> DivergenceRelation {
        match self.drag_divergence_law {
            DragDivergenceLaw::Korn => DivergenceRelation::Korn,
            DragDivergenceLaw::Torenbeek1048 => DivergenceRelation::Torenbeek1048,
            DragDivergenceLaw::Shevell => DivergenceRelation::Shevell,
            DragDivergenceLaw::Auto => {
                if self.korn_override_in_range() {
                    DivergenceRelation::Korn
                } else {
                    DivergenceRelation::Shevell
                }
            }
        }
    }

    /// What the selected relation does not use, or uses outside what its
    /// source states, one sentence each; empty when the selection and the
    /// declared inputs agree.
    ///
    /// - a Korn factor override outside its legal range is never used;
    /// - an in-range override has no effect under Eq. 10.48, which has no
    ///   Korn factor;
    /// - Eq. 10.48's `M* = 0.935` is stated for second-generation
    ///   supercritical sections only, so on a wing declared conventional it
    ///   is applied outside its source.
    #[must_use]
    pub fn divergence_law_notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        let relation = self.divergence_relation();
        if let Some(error) = self.korn_override_error() {
            notes.push(format!(
                "{error}: the override has no effect and the section-class value {} stands",
                self.airfoil_class.korn_technology_factor()
            ));
        } else if let (Some(value), DivergenceRelation::Torenbeek1048) =
            (self.korn_technology_factor_override, relation)
        {
            notes.push(format!(
                "the Korn technology factor override {value} has no effect: the selected \
                 drag-divergence law is Torenbeek's Eq. 10.48, which has no Korn factor"
            ));
        } else if let (Some(value), DivergenceRelation::Shevell) =
            (self.korn_technology_factor_override, relation)
        {
            notes.push(format!(
                "the Korn technology factor override {value} has no effect: the selected \
                 drag-divergence law is the Shevell-Kroo method, which has no Korn factor"
            ));
        }
        if relation == DivergenceRelation::Torenbeek1048 && !self.airfoil_class.is_supercritical() {
            notes.push(
                "Torenbeek's Eq. 10.48 is selected on a wing declared conventional: its M* = \
                 0.935 is stated for second-generation supercritical sections only"
                    .to_owned(),
            );
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wing(law: DragDivergenceLaw, class: AirfoilClass, korn_override: Option<f64>) -> WingConfig {
        WingConfig {
            drag_divergence_law: law,
            airfoil_class: class,
            korn_technology_factor_override: korn_override,
            ..WingConfig::default()
        }
    }

    #[test]
    fn auto_follows_the_section_class_and_an_in_range_override() {
        use AirfoilClass::{Conventional, Supercritical};
        use DragDivergenceLaw::Auto;
        let relation = |class, value| wing(Auto, class, value).divergence_relation();
        assert_eq!(relation(Supercritical, None), DivergenceRelation::Shevell);
        assert_eq!(relation(Conventional, None), DivergenceRelation::Shevell);
        assert_eq!(
            relation(Supercritical, Some(0.95)),
            DivergenceRelation::Korn
        );
        // An out-of-range override is never used, so it does not select Korn.
        assert_eq!(
            relation(Supercritical, Some(1.5)),
            DivergenceRelation::Shevell
        );
        assert_eq!(WingConfig::default().drag_divergence_law, Auto);
        assert!(WingConfig::default().divergence_law_notes().is_empty());
    }

    #[test]
    fn an_explicit_law_holds_on_either_class_and_says_what_it_ignores() {
        use AirfoilClass::{Conventional, Supercritical};
        use DragDivergenceLaw::{Korn, Torenbeek1048};
        for class in [Conventional, Supercritical] {
            assert_eq!(
                wing(Korn, class, None).divergence_relation(),
                DivergenceRelation::Korn
            );
            assert_eq!(
                wing(Torenbeek1048, class, Some(0.9)).divergence_relation(),
                DivergenceRelation::Torenbeek1048
            );
        }
        assert!(wing(Korn, Supercritical, Some(0.9))
            .divergence_law_notes()
            .is_empty());
        let unused = wing(Torenbeek1048, Supercritical, Some(0.9)).divergence_law_notes();
        assert_eq!(unused.len(), 1);
        assert!(
            unused[0].contains("override 0.9 has no effect"),
            "{unused:?}"
        );
        let outside = wing(Torenbeek1048, Conventional, None).divergence_law_notes();
        assert_eq!(outside.len(), 1);
        assert!(outside[0].contains("declared conventional"), "{outside:?}");
        let out_of_range = wing(Korn, Supercritical, Some(1.5)).divergence_law_notes();
        assert_eq!(out_of_range.len(), 1);
        assert!(
            out_of_range[0].contains("has no effect"),
            "{out_of_range:?}"
        );
    }

    #[test]
    fn the_names_are_the_serialized_values_and_auto_is_not_serialized() {
        for (law, name) in [
            (DragDivergenceLaw::Auto, "auto"),
            (DragDivergenceLaw::Shevell, "shevell"),
            (DragDivergenceLaw::Korn, "korn"),
            (DragDivergenceLaw::Torenbeek1048, "torenbeek1048"),
        ] {
            assert_eq!(law.as_str(), name);
            assert_eq!(
                serde_json::to_value(law).ok(),
                Some(serde_json::json!(name))
            );
        }
        assert_eq!(
            DragDivergenceLaw::NAMES,
            ["auto", "shevell", "korn", "torenbeek1048"]
        );
        let saved = serde_json::to_value(WingConfig::default()).unwrap_or_default();
        assert!(saved.get("drag_divergence_law").is_none());
    }
}
