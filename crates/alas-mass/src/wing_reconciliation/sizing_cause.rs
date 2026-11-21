// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why a wing could not be strength-sized or reconciled: the typed cause
//! carried by [`super::WingReconciliationError::StructuralSizing`].
//!
//! Each variant names the check that failed and, where that check reads a
//! sized quantity, the spanwise station and the offending value in SI units,
//! so a rejected candidate is traced to one invariant rather than one bucket.
//! Masses of the sized box are per semi-wing, as the sizer reports them.

use std::fmt;

use alas_config::materials::MaterialSpec;
use alas_config::StructuresConfig;
use alas_geom::aircraft::wing::Wing;
use alas_geom::wing_structure::WingStructureError;
use alas_struct::ground_deflection::GroundDeflectionError;
use alas_struct::sizing::{margin_is_structurally_non_negative, ControllingMargin, WingboxSizing};

use crate::wing_centroid::WingCentroidError;
use crate::wing_inventory::WingInventoryError;
use crate::wingbox_feedback::WingboxFeedbackError;

use super::WingReconciliationError;

/// Which configured structural material a lookup was for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralMaterialRole {
    /// `structures.skin_material`.
    Skin,
    /// `structures.spar_web_material`.
    SparWeb,
    /// `structures.spar_cap_material`.
    SparCap,
    /// `structures.rib_material`.
    Rib,
}

impl StructuralMaterialRole {
    /// The configuration field that names this material.
    pub fn config_field(self) -> &'static str {
        match self {
            Self::Skin => "structures.skin_material",
            Self::SparWeb => "structures.spar_web_material",
            Self::SparCap => "structures.spar_cap_material",
            Self::Rib => "structures.rib_material",
        }
    }

    fn configured(self, structures: &StructuresConfig) -> &str {
        match self {
            Self::Skin => &structures.skin_material,
            Self::SparWeb => &structures.spar_web_material,
            Self::SparCap => &structures.spar_cap_material,
            Self::Rib => &structures.rib_material,
        }
    }
}

/// One family of the sized box's semi-wing mass breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxMassComponent {
    /// Spar caps.
    SparCaps,
    /// Spar webs.
    SparWebs,
    /// Upper and lower covers.
    Skin,
    /// Ribs.
    Ribs,
}

impl BoxMassComponent {
    fn label(self) -> &'static str {
        match self {
            Self::SparCaps => "spar-cap",
            Self::SparWebs => "spar-web",
            Self::Skin => "cover-skin",
            Self::Ribs => "rib",
        }
    }
}

/// Which wing mass a reconciliation was closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationBasis {
    /// The sized box plus the enumerated clean-sheet non-box inventory.
    CleanSheet,
    /// The sized box against the frozen reference aircraft's wing.
    Reference,
}

/// The check that refused a wing in
/// [`super::WingReconciliationError::StructuralSizing`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StructuralSizingCause {
    /// The aircraft has no main wing to size.
    MissingMainWing,
    /// The main wing is not mirrored; the sizer sizes one symmetric semispan.
    AsymmetricWing,
    /// The main wing has fewer than a root and a tip cross-section.
    TooFewCrossSections {
        /// Cross-sections present.
        count: usize,
    },
    /// `structures.spanwise_stations` is below the two a sizing grid needs.
    TooFewSpanwiseStations {
        /// Configured station count.
        count: i64,
    },
    /// The structural-box geometry (spar layout on the planform) failed.
    BoxGeometry(WingStructureError),
    /// A configured structural material is not in the material table.
    UnknownMaterial(StructuralMaterialRole),
    /// The sized semi-wing box mass is non-finite or not positive.
    NonPositiveBoxMass {
        /// Sized semi-wing box mass, kg.
        mass_kg: f64,
    },
    /// The controlling spar strength margin is negative (beyond the
    /// arithmetic zero band) or NaN; the station and spar are located.
    NegativeSparMargin(ControllingMargin),
    /// The installed rib pitch exceeds the panel-buckling allowable, or
    /// either pitch is not finite.
    RibSpacing {
        /// Installed spanwise rib pitch, m.
        installed_m: f64,
        /// Maximum pitch the panel-buckling calculation allows, m.
        allowable_m: f64,
        /// Installed rib count, root and tip ribs included.
        ribs: i64,
    },
    /// One family of the sized box mass is negative or non-finite.
    InvalidBoxMassComponent {
        /// The offending family.
        component: BoxMassComponent,
        /// Its semi-wing mass, kg.
        mass_kg: f64,
    },
    /// The sized box's first moment could not be integrated.
    Centroid {
        /// What the centroid integration rejected.
        reason: &'static str,
    },
    /// The Torenbeek secondary-structure breakdown is not a physical split.
    SecondaryBreakdown {
        /// High-lift devices, kg.
        high_lift_devices_kg: f64,
        /// Spoilers and speed brakes, kg.
        spoilers_and_speedbrakes_kg: f64,
        /// Their declared total, kg.
        total_kg: f64,
    },
    /// The clean-sheet non-box inventory is not physically evaluable.
    Inventory(WingInventoryError),
    /// The box and its non-box remainder did not close into one wing.
    Reconciliation {
        /// Which wing mass was being closed.
        basis: ReconciliationBasis,
        /// The input or result that broke the closure, when one is named.
        field: Option<&'static str>,
        /// What was wrong with it.
        problem: &'static str,
    },
    /// The registered reference preset named by the configuration is unknown.
    ReferencePreset,
    /// The frozen reference aircraft could not be built.
    ReferenceAircraftBuild,
    /// The reference aircraft's empirical wing mass or centroid is not
    /// physical.
    InvalidReferenceWing {
        /// Complete-wing empirical mass, kg.
        mass_kg: f64,
        /// Its centroid, aircraft axes, m.
        centroid_m: [f64; 3],
    },
    /// The static ground deflection of the sized wing did not resolve.
    GroundDeflection(GroundDeflectionError),
    /// The 1 g ("level") load case reports no finite tip deflection.
    LevelTipDeflection,
    /// The fuselage or horizontal-tail stiffness could not be described.
    TailFlexibility {
        /// The missing or degenerate input.
        reason: &'static str,
    },
}

impl fmt::Display for StructuralSizingCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMainWing => f.write_str("the aircraft has no main wing"),
            Self::AsymmetricWing => {
                f.write_str("the main wing is not symmetric; the box is sized on one semispan")
            }
            Self::TooFewCrossSections { count } => write!(
                f,
                "the main wing has {count} cross-section(s); a root and a tip are required"
            ),
            Self::TooFewSpanwiseStations { count } => write!(
                f,
                "structures.spanwise_stations = {count}; at least 2 are required"
            ),
            Self::BoxGeometry(error) => write!(f, "the wing-box geometry failed: {error}"),
            Self::UnknownMaterial(role) => write!(
                f,
                "{} does not name a registered material",
                role.config_field()
            ),
            Self::NonPositiveBoxMass { mass_kg } => write!(
                f,
                "sized semi-wing box mass is {mass_kg} kg; a finite positive mass is required"
            ),
            Self::NegativeSparMargin(at) => write!(
                f,
                "spar {} (x/c = {:.3}) margin of safety {} (dimensionless) is negative or NaN \
                 at station {} (y = {:.3} m, eta = {:.3})",
                at.spar_index, at.chord_fraction, at.margin, at.station_index, at.y_m, at.eta
            ),
            Self::RibSpacing {
                installed_m,
                allowable_m,
                ribs,
            } => write!(
                f,
                "installed rib pitch {installed_m:.4} m ({ribs} ribs) exceeds the \
                 panel-buckling allowable {allowable_m:.4} m or is not finite"
            ),
            Self::InvalidBoxMassComponent { component, mass_kg } => write!(
                f,
                "sized semi-wing {} mass is {mass_kg} kg; it must be finite and non-negative",
                component.label()
            ),
            Self::Centroid { reason } => {
                write!(f, "the sized box centroid did not resolve: {reason}")
            }
            Self::SecondaryBreakdown {
                high_lift_devices_kg,
                spoilers_and_speedbrakes_kg,
                total_kg,
            } => write!(
                f,
                "Torenbeek secondary breakdown is not physical: high-lift \
                 {high_lift_devices_kg} kg + spoilers {spoilers_and_speedbrakes_kg} kg \
                 against a total of {total_kg} kg"
            ),
            Self::Inventory(error) => write!(f, "the non-box wing inventory failed: {error}"),
            Self::Reconciliation {
                basis,
                field,
                problem,
            } => {
                let basis = match basis {
                    ReconciliationBasis::CleanSheet => "clean-sheet",
                    ReconciliationBasis::Reference => "reference",
                };
                match field {
                    Some(field) => write!(f, "{basis} wing closure: {field} {problem}"),
                    None => write!(f, "{basis} wing closure: {problem}"),
                }
            }
            Self::ReferencePreset => f.write_str("the configured reference preset is unknown"),
            Self::ReferenceAircraftBuild => {
                f.write_str("the frozen reference aircraft could not be built")
            }
            Self::InvalidReferenceWing {
                mass_kg,
                centroid_m,
            } => write!(
                f,
                "reference wing mass {mass_kg} kg at {centroid_m:?} m is not finite and positive"
            ),
            Self::GroundDeflection(error) => write!(f, "ground deflection failed: {error}"),
            Self::LevelTipDeflection => {
                f.write_str("the level (1 g) load case has no finite tip deflection")
            }
            Self::TailFlexibility { reason } => write!(f, "tail flexibility: {reason}"),
        }
    }
}

impl std::error::Error for StructuralSizingCause {}

impl From<StructuralSizingCause> for WingReconciliationError {
    fn from(cause: StructuralSizingCause) -> Self {
        Self::StructuralSizing(cause)
    }
}

/// Look up one configured structural material.
pub(super) fn material(
    structures: &StructuresConfig,
    role: StructuralMaterialRole,
) -> Result<&'static MaterialSpec, WingReconciliationError> {
    alas_config::materials::get(role.configured(structures))
        .map_err(|_| StructuralSizingCause::UnknownMaterial(role).into())
}

/// The wing and sizing-grid checks that precede the strength sizing, in
/// order: symmetry, root and tip sections, spanwise stations.
pub(super) fn sizable(wing: &Wing, spanwise_stations: i64) -> Result<(), StructuralSizingCause> {
    let count = wing.xsecs.len();
    if !wing.symmetric {
        Err(StructuralSizingCause::AsymmetricWing)
    } else if count < 2 {
        Err(StructuralSizingCause::TooFewCrossSections { count })
    } else if spanwise_stations < 2 {
        Err(StructuralSizingCause::TooFewSpanwiseStations {
            count: spanwise_stations,
        })
    } else {
        Ok(())
    }
}

/// The frozen reference aircraft's empirical complete-wing mass (kg) and
/// centroid (aircraft axes, m) must be finite, and the mass positive.
pub(super) fn reference_wing(
    mass_kg: f64,
    centroid_m: [f64; 3],
) -> Result<(), StructuralSizingCause> {
    if mass_kg.is_finite() && mass_kg > 0.0 && centroid_m.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(StructuralSizingCause::InvalidReferenceWing {
            mass_kg,
            centroid_m,
        })
    }
}

/// The first admissibility check a sized box fails, in the gate's order:
/// total mass, spar strength margins, rib pitch, mass families.
pub(super) fn sized_box_check(sizing: &WingboxSizing) -> Result<(), StructuralSizingCause> {
    let mass_kg = sizing.total_mass_kg;
    if !mass_kg.is_finite() || mass_kg <= 0.0 {
        return Err(StructuralSizingCause::NonPositiveBoxMass { mass_kg });
    }
    // The controlling margin is NaN first, else the minimum, so it fails the
    // shared zero-band predicate exactly when any sized margin does.
    if let Some(at) = sizing
        .controlling_margin()
        .filter(|at| !margin_is_structurally_non_negative(at.margin))
    {
        return Err(StructuralSizingCause::NegativeSparMargin(at));
    }
    if !sizing.rib_spacing_pass() {
        return Err(StructuralSizingCause::RibSpacing {
            installed_m: sizing.installed_rib_spacing_m(),
            allowable_m: sizing.rib_spacing_m,
            ribs: sizing.num_ribs,
        });
    }
    let breakdown = &sizing.mass_breakdown_kg;
    [
        (BoxMassComponent::SparCaps, breakdown.spar_caps),
        (BoxMassComponent::SparWebs, breakdown.spar_webs),
        (BoxMassComponent::Skin, breakdown.skin),
        (BoxMassComponent::Ribs, breakdown.ribs),
    ]
    .into_iter()
    .find(|(_, mass_kg)| !mass_kg.is_finite() || *mass_kg < 0.0)
    .map_or(Ok(()), |(component, mass_kg)| {
        Err(StructuralSizingCause::InvalidBoxMassComponent { component, mass_kg })
    })
}

/// Name a centroid-integration failure.
pub(super) fn centroid(error: &WingCentroidError) -> StructuralSizingCause {
    StructuralSizingCause::Centroid {
        reason: match error {
            WingCentroidError::InsufficientSections => "fewer than two wing sections",
            WingCentroidError::DegenerateSpan => "the semispan is not positive",
            WingCentroidError::InvalidSparLayout => "fewer than two valid full-span spars",
            WingCentroidError::UnknownMaterial(_) => "a structural material is not registered",
            WingCentroidError::DegenerateMass => "the integrated box mass is not positive",
        },
    }
}

/// Name a failed box/non-box mass closure.
pub(super) fn reconciliation(
    basis: ReconciliationBasis,
    error: &WingboxFeedbackError,
) -> StructuralSizingCause {
    let (field, problem) = match error {
        WingboxFeedbackError::NonFinite { field } => (Some(*field), "is not finite"),
        WingboxFeedbackError::NonPositive { field } => (Some(*field), "is not positive"),
        WingboxFeedbackError::InvalidConvertedMass { field } => (
            Some(*field),
            "is invalid after the semi-wing to complete-wing conversion",
        ),
        WingboxFeedbackError::ReferenceHasNoPositiveSecondary { .. } => {
            (None, "the reference wing does not exceed the sized box")
        }
        WingboxFeedbackError::InvalidReferenceResidual => {
            (None, "the reference non-box first moment is not finite")
        }
        WingboxFeedbackError::InvalidReconciledResult => {
            (None, "the reconciled mass or first moment is not finite")
        }
    };
    StructuralSizingCause::Reconciliation {
        basis,
        field,
        problem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_material_names_its_configuration_field() {
        let mut structures = alas_config::AlasConfig::default().structures;
        structures.rib_material = "not-a-registered-material".into();
        let error = material(&structures, StructuralMaterialRole::Rib).err();
        assert_eq!(
            error,
            Some(WingReconciliationError::StructuralSizing(
                StructuralSizingCause::UnknownMaterial(StructuralMaterialRole::Rib)
            ))
        );
        let text = error.map(|error| error.to_string()).unwrap_or_default();
        assert!(text.contains("structures.rib_material"), "{text}");
    }

    #[test]
    fn a_negative_margin_is_located_with_its_unrounded_value() {
        let cause = StructuralSizingCause::NegativeSparMargin(ControllingMargin {
            margin: -1.0e-9,
            spar_index: 1,
            chord_fraction: 0.65,
            station_index: 3,
            y_m: 4.25,
            eta: 0.125,
        });
        let text = cause.to_string();
        assert!(text.contains("margin of safety -0.000000001 "), "{text}");
        assert!(text.contains("station 3 (y = 4.250 m"), "{text}");
        assert!(text.contains("spar 1 (x/c = 0.650)"), "{text}");
    }

    #[test]
    fn a_feedback_failure_keeps_the_field_it_names() {
        let cause = reconciliation(
            ReconciliationBasis::CleanSheet,
            &WingboxFeedbackError::NonFinite {
                field: "secondary mass",
            },
        );
        assert_eq!(
            cause.to_string(),
            "clean-sheet wing closure: secondary mass is not finite"
        );
    }
}
