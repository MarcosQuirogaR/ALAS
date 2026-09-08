// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! NASA FLOPS transport systems, equipment, and operating-item mass.
//!
//! The equations are the transport-aircraft relations in NASA/TM-2017-219627
//! Vol. I, sections 5.4-5.5, equations 97, 101-106, 108, 110, 113, and
//! 115-126. FLOPS publishes them in US customary units; this API accepts and
//! returns SI and brackets every equation with the exact legal conversions in
//! `alas-units`.
//!
//! [`evaluate_product`] resolves outer geometry from the built airplane and
//! reads every architecture datum from [`alas_config::FlopsTransportConfig`].
//! It returns [`FlopsTransportEvaluation::Unverified`] when any required datum
//! is absent or inconsistent. The reference-compatible MTOW fractions remain
//! a different, explicit method and are never a fallback from this module.

mod airframe;
mod airframe_geometry;
mod equations;
mod product;
pub mod propulsion;
pub mod structure;
pub mod wing_bending;

pub use airframe::{
    evaluate_airframe_product, FlopsAirframeBreakdown, FlopsAirframeEvaluation,
    FlopsAirframeRequest, FlopsAirframeSelection, FlopsAirframeSources,
};
pub use equations::estimate_flops_transport;
pub use product::evaluate_product;
pub use propulsion::{estimate_flops_propulsion, FlopsPropulsionBreakdown, FlopsPropulsionInputs};
pub use structure::{estimate_flops_structure, FlopsStructureBreakdown, FlopsStructureInputs};

/// Fully resolved SI inputs to the FLOPS transport subsystem equations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsTransportInputs {
    /// Maximum Mach number, FLOPS `VMAX`.
    pub maximum_mach: f64,
    /// Source-backed design range, nautical miles, FLOPS `DESRNG`.
    pub design_range_nmi: f64,
    /// Design gross mass, kilograms, converted to FLOPS `DG` weight in lb.
    pub design_gross_mass_kg: f64,
    /// Reference wing area, square metres, FLOPS `SW`.
    pub wing_area_m2: f64,
    /// Total movable surface planform area, square metres, FLOPS `SFLAP`.
    pub movable_surface_area_m2: f64,
    /// Wing span, metres, FLOPS `B`.
    pub wing_span_m: f64,
    /// Wing quarter-chord sweep, degrees, FLOPS `SWEEP`.
    pub quarter_chord_sweep_deg: f64,
    /// Representative total fuselage length, metres, FLOPS `XL`.
    pub fuselage_length_m: f64,
    /// Maximum fuselage width, metres, FLOPS `WF`.
    pub fuselage_width_m: f64,
    /// Maximum fuselage depth, metres, FLOPS `DF`.
    pub fuselage_depth_m: f64,
    /// Number of equivalent fuselages, FLOPS `NFUSE`.
    pub fuselage_count: usize,
    /// Passenger-compartment length, metres, FLOPS `XLP`.
    pub passenger_compartment_length_m: f64,
    /// First-class passenger count, FLOPS `NPF`.
    pub first_class_passenger_count: usize,
    /// Business-class passenger count, FLOPS `NPB`.
    pub business_class_passenger_count: usize,
    /// Tourist/economy passenger count, FLOPS `NPT`.
    pub tourist_class_passenger_count: usize,
    /// Flight-deck crew count, FLOPS `NFLCR`.
    pub flight_crew_count: usize,
    /// Cabin flight-attendant count, FLOPS `NSTU`.
    pub flight_attendant_count: usize,
    /// Galley crew count, FLOPS `NGALC`.
    pub galley_crew_count: usize,
    /// Number of wing-mounted engines, FLOPS `FNEW`.
    pub wing_mounted_engine_count: usize,
    /// Number of fuselage-mounted engines, FLOPS `FNEF`.
    pub fuselage_mounted_engine_count: usize,
    /// Total number of engines, FLOPS `FNENG`.
    pub engine_count: usize,
    /// Rated thrust per engine, newtons, converted to FLOPS `FTHRST` in lbf.
    pub rated_thrust_per_engine_n: f64,
    /// Average maximum nacelle diameter, metres, FLOPS `FNAC`.
    pub nacelle_diameter_m: f64,
    /// Hydraulic-system pressure, pascals, FLOPS `HYDPR`.
    pub hydraulic_pressure_pa: f64,
    /// Variable-sweep penalty from 0 (fixed) to 1 (full), FLOPS `VARSWP`.
    pub variable_sweep_penalty: f64,
    /// Maximum usable aircraft fuel capacity, kilograms, FLOPS `FMXTOT`.
    pub maximum_fuel_capacity_kg: f64,
    /// Number of fuel tanks, FLOPS `NTANK`.
    pub fuel_tank_count: usize,
    /// Cargo loaded into containers, kilograms, FLOPS `WCARGO`.
    pub containerized_cargo_kg: f64,
}

impl FlopsTransportInputs {
    /// Installed passenger count across FLOPS' three cabin classes.
    pub fn passenger_count(&self) -> usize {
        self.first_class_passenger_count
            + self.business_class_passenger_count
            + self.tourist_class_passenger_count
    }
}

/// One verified systems-and-equipment component buildup, in kilograms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsSystemsBreakdown {
    /// Surface controls, FLOPS `WSC`.
    pub surface_controls_kg: f64,
    /// Auxiliary power unit, FLOPS `WAPU`.
    pub apu_kg: f64,
    /// Instruments, FLOPS `WIN`.
    pub instruments_kg: f64,
    /// Hydraulics, FLOPS `WHYD`.
    pub hydraulics_kg: f64,
    /// Electrical system, FLOPS `WELEC`.
    pub electrical_kg: f64,
    /// Avionics, FLOPS `WAVONC`.
    pub avionics_kg: f64,
    /// Furnishings and equipment, FLOPS `WFURN`.
    pub furnishings_kg: f64,
    /// Air conditioning, FLOPS `WAC`.
    pub air_conditioning_kg: f64,
    /// Anti-icing, FLOPS `WAI`.
    pub anti_ice_kg: f64,
    /// Sum of the nine transport systems-and-equipment components.
    pub total_kg: f64,
}

/// One verified operating-items buildup, in kilograms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsOperatingItemsBreakdown {
    /// Flight attendants, galley crew, and their baggage, FLOPS `WSTUAB`.
    pub cabin_crew_and_baggage_kg: f64,
    /// Flight crew and baggage, FLOPS `WFLCRB`.
    pub flight_crew_and_baggage_kg: f64,
    /// Unusable fuel, FLOPS `WUF`.
    pub unusable_fuel_kg: f64,
    /// Engine oil, FLOPS `WOIL`.
    pub engine_oil_kg: f64,
    /// Passenger service, FLOPS `WSRV`.
    pub passenger_service_kg: f64,
    /// Cargo containers, FLOPS `WCON`.
    pub cargo_containers_kg: f64,
    /// Sum of all transport operating items above.
    pub total_kg: f64,
}

/// Verified FLOPS systems, equipment, and operating items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsTransportBreakdown {
    /// Systems and equipment from section 5.4.
    pub systems: FlopsSystemsBreakdown,
    /// Operating items from section 5.5.
    pub operating_items: FlopsOperatingItemsBreakdown,
}

/// A component-level projection when the complete method is not verifiable.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PartialFlopsTransportBreakdown {
    /// Surface-controls mass when its inputs are available.
    pub surface_controls_kg: Option<f64>,
    /// APU mass when its inputs are available.
    pub apu_kg: Option<f64>,
    /// Instruments mass when its inputs are available.
    pub instruments_kg: Option<f64>,
    /// Hydraulics mass when its inputs are available.
    pub hydraulics_kg: Option<f64>,
    /// Electrical-system mass when its inputs are available.
    pub electrical_kg: Option<f64>,
    /// Avionics mass when its inputs are available.
    pub avionics_kg: Option<f64>,
    /// Furnishings mass when its inputs are available.
    pub furnishings_kg: Option<f64>,
    /// Air-conditioning mass when its inputs are available.
    pub air_conditioning_kg: Option<f64>,
    /// Anti-ice mass when its inputs are available.
    pub anti_ice_kg: Option<f64>,
    /// Cabin-crew and baggage mass when its inputs are available.
    pub cabin_crew_and_baggage_kg: Option<f64>,
    /// Flight-crew and baggage mass when its inputs are available.
    pub flight_crew_and_baggage_kg: Option<f64>,
    /// Unusable-fuel mass when its inputs are available.
    pub unusable_fuel_kg: Option<f64>,
    /// Engine-oil mass when its inputs are available.
    pub engine_oil_kg: Option<f64>,
    /// Passenger-service mass when its inputs are available.
    pub passenger_service_kg: Option<f64>,
    /// Cargo-container mass when its inputs are available.
    pub cargo_containers_kg: Option<f64>,
}

/// Physical input that prevents a complete FLOPS transport evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlopsTransportUnverifiedReason {
    /// Maximum Mach number was absent or nonpositive.
    MaximumMach,
    /// Design range was absent or nonpositive.
    DesignRange,
    /// Mission-input provenance was absent or incomplete.
    MissionProvenance,
    /// The built airplane has no valid main wing.
    MainWingGeometry,
    /// The built airplane has no valid primary fuselage.
    FuselageGeometry,
    /// Movable-surface geometry could not be resolved.
    MovableSurfaceGeometry,
    /// Flight crew count was absent.
    FlightCrewCount,
    /// Cabin flight-attendant count was absent.
    FlightAttendantCount,
    /// Galley crew count was absent.
    GalleyCrewCount,
    /// FLOPS cabin-class counts were absent or inconsistent.
    PassengerClassCounts,
    /// Wing/fuselage engine mounting counts were absent or inconsistent.
    EngineMounting,
    /// Hydraulic pressure was absent or nonpositive.
    HydraulicPressure,
    /// Variable-sweep architecture was absent or outside 0-1.
    VariableSweepArchitecture,
    /// Fuel-tank count was absent or zero.
    FuelTankCount,
    /// Maximum fuel capacity was absent or nonpositive.
    MaximumFuelCapacity,
    /// Cabin-input provenance was absent or incomplete.
    CabinProvenance,
    /// Installed-architecture provenance was absent or incomplete.
    ArchitectureProvenance,
    /// The translated thrust-based FLOPS operating-item equations do not
    /// cover the selected propulsion technology.
    UnsupportedPropulsionTechnology,
    /// Containerized cargo mass was absent or negative.
    ContainerizedCargo,
    /// A resolved scalar was nonfinite, nonpositive, or internally inconsistent.
    InvalidResolvedInput,
    /// The built airplane has no horizontal or vertical stabilizer.
    TailGeometry,
    /// No nacelle body or profile with a positive diameter and length.
    NacelleGeometry,
    /// The main wing's lofted thickness ratio is not positive.
    WingThickness,
    /// A FLOPS technology factor or override is outside its fitted range.
    StructureConfiguration,
    /// The detailed wing method needs the FLOPS systems group for the pod
    /// inertia relief and the systems method is not FLOPS.
    DetailedWingRequiresFlopsSystems,
    /// The detailed bending integration found no load-carrying planform.
    DetailedWingIntegration,
}

impl FlopsTransportUnverifiedReason {
    /// Stable machine-readable name used by audits and saved evidence.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaximumMach => "maximum_mach",
            Self::DesignRange => "design_range",
            Self::MissionProvenance => "mission_provenance",
            Self::MainWingGeometry => "main_wing_geometry",
            Self::FuselageGeometry => "fuselage_geometry",
            Self::MovableSurfaceGeometry => "movable_surface_geometry",
            Self::FlightCrewCount => "flight_crew_count",
            Self::FlightAttendantCount => "flight_attendant_count",
            Self::GalleyCrewCount => "galley_crew_count",
            Self::PassengerClassCounts => "passenger_class_counts",
            Self::EngineMounting => "engine_mounting",
            Self::HydraulicPressure => "hydraulic_pressure",
            Self::VariableSweepArchitecture => "variable_sweep_architecture",
            Self::FuelTankCount => "fuel_tank_count",
            Self::MaximumFuelCapacity => "maximum_fuel_capacity",
            Self::CabinProvenance => "cabin_provenance",
            Self::ArchitectureProvenance => "architecture_provenance",
            Self::UnsupportedPropulsionTechnology => "unsupported_propulsion_technology",
            Self::ContainerizedCargo => "containerized_cargo",
            Self::InvalidResolvedInput => "invalid_resolved_input",
            Self::TailGeometry => "tail_geometry",
            Self::NacelleGeometry => "nacelle_geometry",
            Self::WingThickness => "wing_thickness",
            Self::StructureConfiguration => "structure_configuration",
            Self::DetailedWingRequiresFlopsSystems => "detailed_wing_requires_flops_systems",
            Self::DetailedWingIntegration => "detailed_wing_integration",
        }
    }
}

/// Product evaluation that never disguises an incomplete architecture.
#[derive(Debug, Clone, PartialEq)]
pub enum FlopsTransportEvaluation {
    /// Every source and physical input is present and the buildup is valid.
    Verified {
        /// Exact resolved input record.
        inputs: FlopsTransportInputs,
        /// Revision-locked evidence for every declared non-geometric input.
        provenance: Box<alas_config::FlopsTransportProvenance>,
        /// Complete FLOPS component buildup.
        breakdown: FlopsTransportBreakdown,
    },
    /// At least one required datum is missing or inconsistent.
    Unverified {
        /// Stable, deduplicated list of blockers.
        reasons: Vec<FlopsTransportUnverifiedReason>,
        /// Components whose own inputs were independently available.
        partial: PartialFlopsTransportBreakdown,
    },
}

/// Verification status that accompanies a FLOPS transport evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlopsTransportVerificationStatus {
    /// Required physical input or provenance remains missing or inconsistent.
    UnverifiedArchitecture,
    /// Every required input and its declared source family was evaluated.
    VerifiedArchitecture,
}

impl FlopsTransportVerificationStatus {
    /// Stable evidence label for an evaluated FLOPS selection.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnverifiedArchitecture => "unverified_architecture",
            Self::VerifiedArchitecture => "verified_architecture",
        }
    }
}

impl FlopsTransportEvaluation {
    /// Machine-readable status for an evaluated FLOPS selection.
    pub const fn verification_status(&self) -> FlopsTransportVerificationStatus {
        match self {
            Self::Verified { .. } => FlopsTransportVerificationStatus::VerifiedArchitecture,
            Self::Unverified { .. } => FlopsTransportVerificationStatus::UnverifiedArchitecture,
        }
    }
}

/// Invalid fully resolved input supplied directly to the equation API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid FLOPS transport input: {field}")]
pub struct FlopsTransportInputError {
    /// Stable field name that failed validation.
    pub field: &'static str,
}
