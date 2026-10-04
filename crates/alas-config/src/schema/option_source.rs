// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where a string field's accepted values come from.

use serde::Serialize;

/// Where a string field's accepted values come from.
///
/// Most of these lists are owned by crates that sit above this one (the
/// airfoil library, the engine deck, the material database) so this names
/// the list and something that can see both resolves it. The lists that
/// depend on nothing are resolved by [`OptionSource::options`] here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionSource {
    /// The airfoil library, which also accepts names it does not list: any
    /// NACA 4-digit code resolves without being in it.
    Airfoil,
    /// The engine registry.
    Engine,
    /// The material database.
    Material,
    /// The landing-gear strut materials, which are their own list and not
    /// the general material database.
    StrutMaterial,
    /// The tire database.
    TireClass,
    /// Which trailing-edge ribs the structural mesh generates.
    TeRibMode,
    /// The optimization method.
    OptimizerMethod,
    /// Whether the aircraft carries passengers or freight.
    AircraftType,
    /// The cabin layout presets, which differ by aircraft type.
    CabinPreset,
    /// The one method that owns every production mass group.
    MassArchitecture,
    /// What kind of knowledge a family of declared FLOPS inputs rests on.
    FlopsInputEvidence,
    /// Versioned systems-and-equipment mass method.
    SystemsMassMethod,
    /// Versioned structural-group mass method.
    StructuralMassMethod,
    /// Versioned propulsion-group mass method.
    PropulsionMassMethod,
    /// Which method prices the engine pylons FLOPS itself omits.
    PylonMassMethod,
    /// Which method prices the cabin equipment and operating items.
    CabinEquipmentMethod,
    /// Which LTH operating-item relation an aircraft takes.
    OperatingHaulClass,
    /// Which FLOPS wing bending-material factor is evaluated.
    FlopsWingBendingMethod,
    /// Whether the FLOPS engine starter is inside the declared baseline mass.
    FlopsStarterScope,
    /// Whether the FLOPS engine nozzle is inside the declared baseline mass.
    FlopsNozzleScope,
    /// Blade material and pitch-change hardware of a turboprop propeller.
    PropellerConstruction,
    /// Whether the cargo compartments are loose-loaded or take unit load
    /// devices.
    CargoHoldLoading,
    /// The operating rule a design mission's reserves are sized under.
    FuelScheme,
    /// The container or pallet loaded on the main cargo deck.
    MainDeckUld,
    /// The lower-hold container format, including the physical auto-selector.
    LowerDeckUld,
    /// How the cargo loader distributes payload between available positions.
    CargoLoadingStrategy,
    /// How checked baggage is divided between hold compartments.
    BaggagePolicy,
    /// The scalar the mission-sized design search minimises.
    ObjectiveKind,
    /// Whether the takeoff mass is a fixed input, closed by the mission up
    /// to it, or closed by the mission with it used only to seed the first
    /// pass.
    MtowSizing,
    /// How the optimizer treats the aircraft geometry it starts from.
    DesignMode,
    /// The ICAO Annex 14 aerodrome reference code letter that caps the
    /// wingspan of a clean-sheet design.
    AerodromeReferenceCode,
}

impl OptionSource {
    /// The accepted values, when this crate can name them.
    ///
    /// `None` means the list is owned elsewhere and has to be resolved by a
    /// crate that can see its owner.
    pub fn options(self) -> Option<&'static [&'static str]> {
        match self {
            Self::TeRibMode => Some(&["all", "none", "alternate", "inboard", "outboard"]),
            Self::OptimizerMethod => Some(&["differential_evolution"]),
            Self::AircraftType => Some(&["passenger", "cargo"]),
            Self::MassArchitecture => Some(&[
                "pure_flops_transport_v1",
                "legacy_reference_compatible_comparison",
            ]),
            Self::FlopsInputEvidence => Some(&[
                "source_backed",
                "user_declared",
                "published_flops_default",
                "uncertain_engineering_estimate",
            ]),
            Self::SystemsMassMethod => {
                Some(&["reference_compatible_fractions", "flops_transport_v1"])
            }
            Self::StructuralMassMethod | Self::PropulsionMassMethod => {
                Some(&["reference_compatible", "flops_transport_v1"])
            }
            Self::FlopsWingBendingMethod => Some(&["simplified", "detailed"]),
            Self::PylonMassMethod => Some(&["none", "lth_box_beam_v1"]),
            Self::FlopsStarterScope => Some(&[
                "separate_equation_89",
                "included_in_baseline",
                "hardware_included_system_unresolved",
                "unknown_conservative_separate",
            ]),
            Self::FlopsNozzleScope => Some(&[
                "included_in_baseline",
                "separate_equation_78",
                "outside_unmodelled",
                "unknown",
            ]),
            Self::CabinEquipmentMethod => Some(crate::CabinEquipmentMethod::OPTIONS),
            Self::OperatingHaulClass => Some(&["short_medium_haul", "long_haul"]),
            Self::PropellerConstruction => Some(&[
                "aluminium_double_acting",
                "aluminium_single_acting",
                "composite",
            ]),
            Self::CargoHoldLoading => Some(&["bulk", "containerized", "mixed"]),
            Self::FuelScheme => Some(&[
                "easa_basic",
                "faa_domestic",
                "faa_flag_supplemental",
                "study_convention",
                "trip_fuel_only",
            ]),
            Self::CargoLoadingStrategy => {
                Some(&["target_cg", "min_pallets", "door_proximity", "uniform"])
            }
            Self::BaggagePolicy => Some(&["target_cg", "volume_proportional"]),
            Self::ObjectiveKind => Some(&[
                "block_fuel",
                "takeoff_mass",
                "operating_empty_mass",
                "fuel_per_seat_kilometre",
            ]),
            Self::MtowSizing => Some(&crate::optimizer::MtowSizing::NAMES),
            Self::DesignMode => Some(&["clean_sheet", "reference_adaptation", "baseline_sandbox"]),
            Self::AerodromeReferenceCode => Some(&crate::optimizer::AerodromeReferenceCode::NAMES),
            _ => None,
        }
    }

    /// Whether a value outside the list is still accepted.
    ///
    /// Only the airfoil field is: the geometry layer resolves names the
    /// library does not carry, so a strict list would reject valid input.
    /// Everywhere else the list is the valid set, and free text could only
    /// produce a lookup failure later.
    pub fn editable(self) -> bool {
        self == Self::Airfoil
    }
}
