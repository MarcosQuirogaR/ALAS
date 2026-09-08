// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::flops_transport::structure::{wing_mass, FlopsWingBreakdown, FlopsWingInputs};
use crate::wingbox_feedback::{SecondaryWingMass, SizedWingboxMass, WingExtent};

/// Number of enumerated non-box items.
pub const ITEM_COUNT: usize = 5;

/// Maximum number of findings retained in a [`WingInventoryStatus`]: one per
/// required item plus the two plausibility gates, which is every finding this
/// module can raise at once.
pub const MAX_FINDINGS: usize = 6;

/// A movable surface's complete-wing planform area and centroid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovableSurface {
    /// Complete-wing planform area of the surface, m^2.
    pub area_m2: f64,
    /// Complete-wing area centroid `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
}

impl MovableSurface {
    /// A surface the configuration does not carry.
    pub const NONE: Self = Self {
        area_m2: 0.0,
        centroid_m: [0.0; 3],
    };
}

/// The four movable-surface groups of a conventional transport wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingMovableSurfaces {
    /// Trailing-edge high-lift devices.
    pub trailing_edge_flaps: MovableSurface,
    /// Leading-edge high-lift devices (slats or Krueger flaps).
    pub leading_edge_devices: MovableSurface,
    /// Ailerons.
    pub ailerons: MovableSurface,
    /// Spoilers and speedbrakes.
    pub spoilers: MovableSurface,
}

/// Wing planform that the structural box does not cover.
///
/// `chord_fraction_outside_box` is the local-chord fraction forward of the
/// front spar plus the fraction aft of the rear spar. It selects the share of
/// the FLOPS miscellaneous term attributed to fixed non-box structure so the
/// rib and box content that FLOPS also puts in that term is not counted twice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedNonBoxStructure {
    /// Local-chord fraction outside the front and rear spars, 0-1.
    pub chord_fraction_outside_box: f64,
    /// Area centroid of that planform `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
}

/// The Torenbeek App. C wing group evaluated on the candidate geometry.
///
/// `group_total_kg` is [`crate::torenbeek::mass_wing_with_control_surface_area`]
/// and the two movable terms are
/// [`crate::torenbeek::wing_secondary_mass_breakdown_with_control_surface_area`],
/// so the basic structure follows exactly as their difference and no private
/// coefficient of that module is duplicated here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TorenbeekWingGroup {
    /// Complete Torenbeek wing group, kg.
    pub group_total_kg: f64,
    /// Installed trailing-edge high-lift devices, kg.
    pub high_lift_devices_kg: f64,
    /// Installed spoilers and speedbrakes, kg.
    pub spoilers_and_speedbrakes_kg: f64,
}

impl TorenbeekWingGroup {
    /// Installed movable items, kg.
    pub fn movable_items_kg(&self) -> f64 {
        self.high_lift_devices_kg + self.spoilers_and_speedbrakes_kg
    }

    /// Torenbeek basic structure (spar box, skin and ribs), kg.
    pub fn basic_structure_kg(&self) -> f64 {
        self.group_total_kg - self.movable_items_kg()
    }
}

/// Everything the inventory is built from.
///
/// `flops.movable_surface_area_m2` is ignored: the movable areas come from
/// `surfaces`, so the FLOPS terms and the enumerated items cannot disagree
/// about how much movable surface the wing has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingInventoryInputs {
    /// FLOPS transport wing inputs (Eqs. 33-38) for this candidate.
    pub flops: FlopsWingInputs,
    /// Torenbeek App. C wing group for this candidate.
    pub torenbeek: TorenbeekWingGroup,
    /// The analytically sized primary box.
    pub sized_box: SizedWingboxMass,
    /// Movable-surface areas and centroids.
    pub surfaces: WingMovableSurfaces,
    /// Fixed structure outside the box.
    pub fixed_structure: FixedNonBoxStructure,
}

/// One enumerated non-box wing item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingInventoryEntry {
    /// Stable item name.
    pub name: &'static str,
    /// Complete-wing item mass, kg.
    pub mass_kg: f64,
    /// Complete-wing item centroid `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
    /// Literature or in-house source of the correlation.
    pub source: &'static str,
    /// What the item covers and where the correlation applies.
    pub applicability: &'static str,
    /// Declared relative uncertainty of the item mass (1 = 100 %).
    pub relative_uncertainty: f64,
}

impl WingInventoryEntry {
    /// The item's first moment `[Mx, My, Mz]`, kg m.
    pub fn first_moment_kg_m(&self) -> [f64; 3] {
        std::array::from_fn(|axis| self.mass_kg * self.centroid_m[axis])
    }
}

/// Why an inventory is not complete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WingInventoryFinding {
    /// A required item has no positive mass.
    MissingItem {
        /// The item that is absent.
        name: &'static str,
    },
    /// The sized box alone is not lighter than the empirical wing group.
    SizedBoxExceedsEmpiricalGroup {
        /// Complete-wing sized box, kg.
        sized_box_kg: f64,
        /// Torenbeek App. C wing group, kg.
        empirical_group_kg: f64,
    },
    /// The non-box share of the reconciled wing is outside the sourced band.
    NonBoxFractionOutOfBand {
        /// Non-box mass over reconciled total wing mass.
        fraction: f64,
        /// Lower bound: the Torenbeek movable-item share of its wing group.
        lower: f64,
        /// Upper bound: the FLOPS non-bending share `(W2 + W3) / W_total`.
        upper: f64,
    },
}

/// Whether the enumerated inventory may be presented as complete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WingInventoryStatus {
    /// Every required item is present and both plausibility gates pass.
    Complete,
    /// At least one finding blocks the inventory.
    Incomplete {
        /// Findings, in detection order, padded with `None`.
        findings: [Option<WingInventoryFinding>; MAX_FINDINGS],
    },
}

impl WingInventoryStatus {
    /// Whether the inventory may be presented as complete.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// The findings that block the inventory, in detection order.
    pub fn findings(&self) -> impl Iterator<Item = WingInventoryFinding> + '_ {
        let slots: &[Option<WingInventoryFinding>] = match self {
            Self::Complete => &[],
            Self::Incomplete { findings } => findings,
        };
        slots.iter().copied().flatten()
    }
}

/// Failure returned when an inventory cannot be built or cannot be defended.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum WingInventoryError {
    /// A named input was not finite.
    #[error("{field} must be finite")]
    NonFinite {
        /// Input name.
        field: &'static str,
    },
    /// A named input was negative where only nonnegative values are physical.
    #[error("{field} must not be negative")]
    Negative {
        /// Input name.
        field: &'static str,
    },
    /// A named input mass was not strictly positive.
    #[error("{field} must be positive")]
    NonPositive {
        /// Input name.
        field: &'static str,
    },
    /// A required enumerated item has no positive mass.
    #[error("non-box wing item {name} is missing")]
    MissingItem {
        /// The absent item.
        name: &'static str,
    },
    /// The sized box alone is not lighter than the empirical wing group.
    #[error(
        "sized wingbox ({sized_box_kg} kg) is not lighter than the Torenbeek wing group ({empirical_group_kg} kg)"
    )]
    SizedBoxExceedsEmpiricalGroup {
        /// Complete-wing sized box, kg.
        sized_box_kg: f64,
        /// Torenbeek App. C wing group, kg.
        empirical_group_kg: f64,
    },
    /// The non-box share is outside the sourced band.
    #[error("non-box wing share {fraction} is outside the sourced band [{lower}, {upper}]")]
    NonBoxFractionOutOfBand {
        /// Non-box mass over reconciled total wing mass.
        fraction: f64,
        /// Lower bound.
        lower: f64,
        /// Upper bound.
        upper: f64,
    },
}

/// Plausibility diagnostics; none of these change any mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingInventoryDiagnostics {
    /// Torenbeek App. C wing group, kg.
    pub torenbeek_group_total_kg: f64,
    /// Torenbeek basic structure, kg.
    pub torenbeek_basic_structure_kg: f64,
    /// FLOPS wing group `W1 + W2 + W3`, kg.
    pub flops_group_total_kg: f64,
    /// FLOPS bending material `W1`, kg.
    pub flops_bending_material_kg: f64,
    /// FLOPS shear material and control surfaces `W2`, kg.
    pub flops_shear_and_control_kg: f64,
    /// FLOPS miscellaneous `W3`, kg.
    pub flops_miscellaneous_kg: f64,
    /// Non-box mass over reconciled total wing mass.
    pub non_box_fraction: f64,
    /// `[lower, upper]` sourced band for `non_box_fraction`.
    pub non_box_fraction_band: [f64; 2],
    /// Sized box over Torenbeek basic structure.
    pub box_to_torenbeek_basic_ratio: f64,
    /// Sized box plus fixed non-box structure over Torenbeek basic structure.
    pub box_plus_fixed_to_torenbeek_basic_ratio: f64,
    /// Reconciled total wing over the Torenbeek wing group.
    pub total_to_torenbeek_group_ratio: f64,
    /// Reconciled total wing over the FLOPS wing group.
    pub total_to_flops_group_ratio: f64,
}

/// The enumerated non-box inventory and its diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingNonBoxInventory {
    /// The enumerated items.
    pub items: [WingInventoryEntry; ITEM_COUNT],
    /// Sum of the item masses, kg.
    pub total_kg: f64,
    /// Sum of the item first moments `[Mx, My, Mz]`, kg m.
    pub first_moment_kg_m: [f64; 3],
    /// Mass-weighted centroid of the items `[x, y, z]`, m.
    pub centroid_m: [f64; 3],
    /// Complete-wing sized box mass, kg.
    pub sized_box_mass_kg: f64,
    /// `sized_box_mass_kg + total_kg`, kg.
    pub complete_wing_mass_kg: f64,
    /// Plausibility diagnostics.
    pub diagnostics: WingInventoryDiagnostics,
    /// Whether the inventory may be presented as complete.
    pub status: WingInventoryStatus,
}

impl WingNonBoxInventory {
    /// Sum of the item masses, kg.
    pub fn total_kg(&self) -> f64 {
        self.total_kg
    }

    /// Sum of the item first moments `[Mx, My, Mz]`, kg m.
    pub fn first_moment_kg_m(&self) -> [f64; 3] {
        self.first_moment_kg_m
    }

    /// Whether the inventory may be presented as complete.
    pub fn status(&self) -> WingInventoryStatus {
        self.status
    }

    /// The inventory as a complete-wing secondary mass for
    /// [`crate::wingbox_feedback::reconcile_clean_sheet_wing`].
    pub fn secondary_wing_mass(&self) -> SecondaryWingMass {
        SecondaryWingMass {
            mass_kg: self.total_kg,
            centroid_m: self.centroid_m,
            extent: WingExtent::FullWing,
        }
    }

    /// The first blocking finding as a typed error, or `Ok(())`.
    pub fn require_complete(&self) -> Result<(), WingInventoryError> {
        match self.status.findings().next() {
            None => Ok(()),
            Some(WingInventoryFinding::MissingItem { name }) => {
                Err(WingInventoryError::MissingItem { name })
            }
            Some(WingInventoryFinding::SizedBoxExceedsEmpiricalGroup {
                sized_box_kg,
                empirical_group_kg,
            }) => Err(WingInventoryError::SizedBoxExceedsEmpiricalGroup {
                sized_box_kg,
                empirical_group_kg,
            }),
            Some(WingInventoryFinding::NonBoxFractionOutOfBand {
                fraction,
                lower,
                upper,
            }) => Err(WingInventoryError::NonBoxFractionOutOfBand {
                fraction,
                lower,
                upper,
            }),
        }
    }
}
