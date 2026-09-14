// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one selector that decides which method owns every production mass group.
//!
//! Before this module the buildup carried three independent selectors --
//! systems, structure and propulsion -- each defaulting to the frozen
//! Torenbeek/fraction method. Eight combinations were reachable and six of
//! them were hybrids: a FLOPS wing paired with a fraction systems group, a
//! FLOPS propulsion group whose nacelles were owned by the *structural*
//! selector and therefore silently dropped, and a ledger that labelled rows
//! "FLOPS" from a selection it had not been given the buildup for.
//!
//! A mass model is not a menu of interchangeable parts. FLOPS distributes
//! items between groups on its own conventions -- nacelles are structure but
//! ride with the engines, furnishings sit inside the systems group, operating
//! items sit above empty mass -- and a correlation from a different method
//! set does not honour any of them. Mixing the two produces a number that
//! belongs to no published method and cannot be audited against either.
//!
//! So there is one selection, [`MassArchitecture`], and it is
//! [`MassArchitecture::PureFlopsTransportV1`] by default. The legacy buildup
//! remains reachable, but only as
//! [`MassArchitecture::LegacyReferenceCompatibleComparison`], whose name says
//! what it is for: comparison and regression evidence. It is never a fallback
//! -- a FLOPS group that cannot be evaluated returns its blockers and the
//! analysis fails, rather than quietly reverting to a fraction of takeoff
//! mass.

use serde::{Deserialize, Serialize};

use crate::{Kind, Leaf, PropulsionMassMethod, StructuralMassMethod, SystemsMassMethod};

/// Current [`crate::MassModelConfig`] schema version.
///
/// * **1** -- three independent group selectors, all defaulting to the frozen
///   reference-compatible methods. Hybrid selections were representable.
/// * **2** -- one [`MassArchitecture`]. The three group selectors are derived
///   from it and are no longer written to saved files.
pub const MASS_MODEL_SCHEMA_VERSION: u32 = 2;

/// Schema version assumed for a saved file that does not state one.
pub const fn legacy_schema_version() -> u32 {
    1
}

/// `serde` default for [`crate::MassModelConfig::schema_version`].
pub(crate) const fn legacy_mass_model_schema_version() -> u32 {
    legacy_schema_version()
}

/// Which method owns every production mass group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MassArchitecture {
    /// The NASA FLOPS conventional-transport weight equations own the wing,
    /// both tails, the fuselage, the landing gear, the nacelles, the paint,
    /// the installed propulsion group, the systems-and-equipment group, the
    /// furnishings and the operating items. This is the production model.
    #[default]
    PureFlopsTransportV1,
    /// The frozen Torenbeek structural relations with fractions of takeoff
    /// mass for the gear, systems and furnishings.
    ///
    /// **Comparison and regression evidence only.** Nothing in the product
    /// falls back to this: it has to be asked for by name. It reproduces the
    /// translated Python baseline bit for bit, which is what makes it useful
    /// as a control, and it is uncalibrated in exactly the same way FLOPS is.
    LegacyReferenceCompatibleComparison,
}

impl MassArchitecture {
    /// Stable serialized name, used by saved files, exports and evidence.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PureFlopsTransportV1 => "pure_flops_transport_v1",
            Self::LegacyReferenceCompatibleComparison => "legacy_reference_compatible_comparison",
        }
    }

    /// Whether this architecture may own a production analysis.
    ///
    /// The legacy buildup is retained for comparison and regression evidence
    /// and answers `false`, so a caller that must not publish a legacy mass
    /// can say so in one place instead of re-deriving the rule.
    pub const fn is_production(self) -> bool {
        matches!(self, Self::PureFlopsTransportV1)
    }

    /// Whether every group in this architecture is a FLOPS equation.
    pub const fn is_pure_flops(self) -> bool {
        matches!(self, Self::PureFlopsTransportV1)
    }

    /// The systems-and-operating-items method this architecture implies.
    pub const fn systems_method(self) -> SystemsMassMethod {
        match self {
            Self::PureFlopsTransportV1 => SystemsMassMethod::FlopsTransportV1,
            Self::LegacyReferenceCompatibleComparison => {
                SystemsMassMethod::ReferenceCompatibleFractions
            }
        }
    }

    /// The structural-group method this architecture implies.
    pub const fn structural_method(self) -> StructuralMassMethod {
        match self {
            Self::PureFlopsTransportV1 => StructuralMassMethod::FlopsTransportV1,
            Self::LegacyReferenceCompatibleComparison => StructuralMassMethod::ReferenceCompatible,
        }
    }

    /// The propulsion-group method this architecture implies.
    pub const fn propulsion_method(self) -> PropulsionMassMethod {
        match self {
            Self::PureFlopsTransportV1 => PropulsionMassMethod::FlopsTransportV1,
            Self::LegacyReferenceCompatibleComparison => PropulsionMassMethod::ReferenceCompatible,
        }
    }

    /// The architecture a pre-version-2 file's three group selectors describe,
    /// or `None` when they describe a hybrid that no architecture represents.
    pub fn from_group_selection(
        systems: SystemsMassMethod,
        structure: StructuralMassMethod,
        propulsion: PropulsionMassMethod,
    ) -> Option<Self> {
        match (
            systems.is_reference_compatible(),
            structure.is_reference_compatible(),
            propulsion.is_reference_compatible(),
        ) {
            (false, false, false) => Some(Self::PureFlopsTransportV1),
            (true, true, true) => Some(Self::LegacyReferenceCompatibleComparison),
            _ => None,
        }
    }
}

impl Leaf for MassArchitecture {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// What loading a saved configuration did to its mass-method selection.
///
/// Recorded rather than logged so the graphical front end, the exported
/// design database and the evidence report can all say the same thing about
/// the same run. A silent change of mass method is a silent change of
/// operating empty weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MassArchitectureMigration {
    /// The file already stated a version-2 architecture, or carried no mass
    /// model at all and took the default.
    #[default]
    None,
    /// A pre-version-2 file selected FLOPS for all three groups. That is
    /// exactly the pure architecture, so nothing about the run changed.
    LegacyPureFlopsPreserved,
    /// A pre-version-2 file kept the frozen reference-compatible methods on
    /// all three groups, the pre-version-2 default. It now runs pure
    /// FLOPS. **The operating empty mass this configuration produces will
    /// differ**, and the legacy buildup has to be re-selected by name to get
    /// the old numbers back.
    LegacyDefaultsMovedToPureFlops,
    /// A pre-version-2 file selected a hybrid of FLOPS and reference-compatible
    /// groups. No architecture represents that, so it was migrated to pure
    /// FLOPS rather than reconstructed.
    LegacyHybridMigratedToPureFlops {
        /// Whether the file had selected the FLOPS systems group.
        systems_was_flops: bool,
        /// Whether the file had selected the FLOPS structural group.
        structure_was_flops: bool,
        /// Whether the file had selected the FLOPS propulsion group.
        propulsion_was_flops: bool,
    },
}

impl MassArchitectureMigration {
    /// Whether loading changed the mass method the configuration will run.
    pub const fn changed_the_mass_method(self) -> bool {
        !matches!(self, Self::None | Self::LegacyPureFlopsPreserved)
    }

    /// Stable machine-readable name for exports and evidence artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LegacyPureFlopsPreserved => "legacy_pure_flops_preserved",
            Self::LegacyDefaultsMovedToPureFlops => "legacy_defaults_moved_to_pure_flops",
            Self::LegacyHybridMigratedToPureFlops { .. } => "legacy_hybrid_migrated_to_pure_flops",
        }
    }

    /// One sentence a user interface can show without inventing wording.
    pub fn message(self) -> Option<String> {
        match self {
            Self::None | Self::LegacyPureFlopsPreserved => None,
            Self::LegacyDefaultsMovedToPureFlops => Some(
                "This configuration was saved with the legacy Torenbeek/fraction mass \
                 model, which is no longer the product method. It now runs the pure \
                 NASA FLOPS transport buildup, so its operating empty mass will differ. \
                 Select the legacy comparison architecture by name to reproduce the old \
                 numbers."
                    .to_owned(),
            ),
            Self::LegacyHybridMigratedToPureFlops {
                systems_was_flops,
                structure_was_flops,
                propulsion_was_flops,
            } => Some(format!(
                "This configuration mixed mass methods (systems: {}, structure: {}, \
                 propulsion: {}). A mixed selection belongs to no published method -- \
                 FLOPS assigns nacelles, furnishings and operating items on its own \
                 conventions -- so it has been migrated to the pure NASA FLOPS transport \
                 buildup rather than reconstructed. Its masses will differ from the \
                 saved run.",
                method_label(systems_was_flops),
                method_label(structure_was_flops),
                method_label(propulsion_was_flops),
            )),
        }
    }
}

/// "FLOPS" or "reference-compatible", for the migration message above.
const fn method_label(was_flops: bool) -> &'static str {
    if was_flops {
        "FLOPS"
    } else {
        "reference-compatible"
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_architecture_is_pure_flops() {
        assert_eq!(
            MassArchitecture::default(),
            MassArchitecture::PureFlopsTransportV1
        );
        assert!(MassArchitecture::default().is_production());
        assert!(MassArchitecture::default().is_pure_flops());
    }

    #[test]
    fn the_legacy_architecture_is_not_a_production_method() {
        let legacy = MassArchitecture::LegacyReferenceCompatibleComparison;
        assert!(!legacy.is_production());
        assert!(!legacy.is_pure_flops());
    }

    #[test]
    fn each_architecture_selects_the_same_method_for_all_three_groups() {
        let pure = MassArchitecture::PureFlopsTransportV1;
        assert!(!pure.systems_method().is_reference_compatible());
        assert!(!pure.structural_method().is_reference_compatible());
        assert!(!pure.propulsion_method().is_reference_compatible());

        let legacy = MassArchitecture::LegacyReferenceCompatibleComparison;
        assert!(legacy.systems_method().is_reference_compatible());
        assert!(legacy.structural_method().is_reference_compatible());
        assert!(legacy.propulsion_method().is_reference_compatible());
    }

    #[test]
    fn a_group_selection_round_trips_through_the_architecture() {
        for architecture in [
            MassArchitecture::PureFlopsTransportV1,
            MassArchitecture::LegacyReferenceCompatibleComparison,
        ] {
            assert_eq!(
                MassArchitecture::from_group_selection(
                    architecture.systems_method(),
                    architecture.structural_method(),
                    architecture.propulsion_method(),
                ),
                Some(architecture)
            );
        }
    }

    #[test]
    fn a_hybrid_group_selection_names_no_architecture() {
        assert_eq!(
            MassArchitecture::from_group_selection(
                SystemsMassMethod::FlopsTransportV1,
                StructuralMassMethod::ReferenceCompatible,
                PropulsionMassMethod::ReferenceCompatible,
            ),
            None
        );
        assert_eq!(
            MassArchitecture::from_group_selection(
                SystemsMassMethod::ReferenceCompatibleFractions,
                StructuralMassMethod::FlopsTransportV1,
                PropulsionMassMethod::FlopsTransportV1,
            ),
            None
        );
    }

    #[test]
    fn the_serialized_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&MassArchitecture::PureFlopsTransportV1).unwrap(),
            "\"pure_flops_transport_v1\""
        );
        assert_eq!(
            serde_json::to_string(&MassArchitecture::LegacyReferenceCompatibleComparison).unwrap(),
            "\"legacy_reference_compatible_comparison\""
        );
        assert_eq!(
            MassArchitecture::PureFlopsTransportV1.as_str(),
            "pure_flops_transport_v1"
        );
        assert_eq!(
            MassArchitecture::LegacyReferenceCompatibleComparison.as_str(),
            "legacy_reference_compatible_comparison"
        );
    }

    #[test]
    fn only_a_real_method_change_is_reported_to_the_user() {
        assert!(!MassArchitectureMigration::None.changed_the_mass_method());
        assert!(!MassArchitectureMigration::LegacyPureFlopsPreserved.changed_the_mass_method());
        assert!(MassArchitectureMigration::LegacyDefaultsMovedToPureFlops.changed_the_mass_method());
        assert!(MassArchitectureMigration::LegacyHybridMigratedToPureFlops {
            systems_was_flops: true,
            structure_was_flops: false,
            propulsion_was_flops: false,
        }
        .changed_the_mass_method());
        assert!(MassArchitectureMigration::None.message().is_none());
        assert!(MassArchitectureMigration::LegacyDefaultsMovedToPureFlops
            .message()
            .is_some());
    }

    #[test]
    fn the_hybrid_message_names_every_group_it_migrated() {
        let message = MassArchitectureMigration::LegacyHybridMigratedToPureFlops {
            systems_was_flops: true,
            structure_was_flops: false,
            propulsion_was_flops: true,
        }
        .message()
        .expect("a hybrid migration always has a message");
        assert!(message.contains("systems: FLOPS"));
        assert!(message.contains("structure: reference-compatible"));
        assert!(message.contains("propulsion: FLOPS"));
    }
}
