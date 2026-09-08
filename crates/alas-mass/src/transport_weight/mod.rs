// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Analyses.Weights.Weights_Transport and
// mission reference.Methods.Weights.Correlations.Common.weight_transport.empty_weight.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! mission reference's transport-category empty-weight buildup --
//! `Weights_Transport.evaluate()`, which unpacks a built `Vehicle` and calls
//! `empty_weight(vehicle, settings, method_type="New mission reference")`. `"New mission reference"`
//! is the only `method_type` `evaluate()` ever passes (its own default
//! argument), and the only place in the whole reference that constructs a
//! `Weights_Transport` is the mission runner's
//! `external tools/mission_runner/mission_builder.py`'s `base_analysis`, whose
//! `weights.vehicle = vehicle` is always a sized `Config` --
//! `mission_builder.simple_sizing`'s output, not `vehicle_builder.build_vehicle`'s
//! raw one. That ordering matters for two fields this module takes as plain
//! data rather than recomputing: see `TransportVehicle::max_zero_fuel_kg` and
//! `WingPlanform`'s `area_exposed_m2`/`area_wetted_m2` below.
//!
//! # Scope: `method_type="New mission reference"` only
//!
//! `empty_weight` branches on `method_type` at every step (FLOPS Simple/
//! Complex, Raymer, mission reference, New mission reference). Since `Weights_Transport.evaluate()`
//! never passes anything but `"New mission reference"`, every other branch is dead from
//! this program's own inputs and is not translated: `wing_weight_FLOPS`,
//! `wing_main_raymer`, `tail_horizontal_FLOPS`/`_Raymer`,
//! `tail_vertical_FLOPS`/`_Raymer`, `fuselage_weight_FLOPS`/`_Raymer`,
//! `landing_gear_FLOPS`/`_Raymer`, `systems_FLOPS`/`_Raymer`,
//! `operating_items_FLOPS`, `payload_FLOPS`, `total_prop_flops`,
//! `total_prop_Raymer`. Reached instead: [`wing::wing_main`] (the mission reference/
//! Common branch), [`wing::tail_horizontal`], [`wing::tail_vertical`],
//! [`fuselage::tube`], [`systems::systems`], [`systems::operating_items`],
//! [`payload::payload`], [`landing_gear::landing_gear`], and
//! [`propulsion::engine_jet`]/[`propulsion::integrated_propulsion`] (the
//! branch taken when a network has no `total_weight` override, which none of
//! this program's vehicles ever set).
//!
//! `empty_weight`'s `settings.weight_reduction_factors` (`main_wing`,
//! `empennage`, `fuselage`, `structural`, `systems`) are `Weights_Transport`'s
//! own `__defaults__` -- all zero -- and no reachable caller ever constructs
//! one with anything else (a grep of the whole reference finds no assignment
//! to `weights.settings`). Every `wt_x * (1 - wt_factors.y)` in the original
//! is therefore multiplication by exactly `1.0`, and this port omits the
//! factors rather than carrying parameters that can never move a number, the
//! same choice [`super::torenbeek`] made for `mass_wing`'s unexposed `k_f1`/
//! `k_f2`.
//!
//! `WPOD` (the FLOPS-Complex nacelle-pod term) and every `wt_prop_data` read
//! are unreached: `wt_prop_data` stays `None` on the `New mission reference` path, so
//! upstream's own `if wt_prop_data is None` branches make
//! `output.structures.nacelle`, `.propulsion_breakdown.engines`,
//! `.thrust_reversers`, `.miscellaneous` and `.fuel_system` all exactly
//! `0.0` -- reproduced as hardcoded zeros in [`WeightBreakdown`] rather than
//! as unused fields threaded through every call.
//!
//! # A naming quirk, reproduced rather than fixed
//!
//! `Weights_Transport.evaluate()` sets
//! `vehicle.mass_properties.operating_empty = results.empty` -- the
//! structures + propulsion + systems subtotal, *not* `results.operating_empty`
//! (which additionally carries flight crew, attendants and the other
//! operating items). The same substitution appears in the base
//! `mission reference.Analyses.Weights.Weights.evaluate()`, so it is an established
//! naming choice upstream and not a one-off slip. [`WeightBreakdown::empty_kg`]
//! is therefore the field that corresponds to what a caller reading
//! `vehicle.mass_properties.operating_empty` after `evaluate()` would see --
//! not [`WeightBreakdown::operating_empty_kg`], despite the name.
//!
//! # `wing.Segments` is always empty
//!
//! `wing_main`'s `computation_type='segmented'` branch is gated on
//! `len(wing.Segments) > 0 *and* computation_type == 'segmented'`.
//! `external tools/mission_runner/vehicle_builder.py` never calls
//! `wing.append_segment`, so every wing this program's mission reference bridge builds
//! has an empty `Segments` container and the traditional (`else`) formula is
//! the only one ever reached -- confirmed against every case in
//! `golden/mass/transport_weight.json`. The segmented branch and its
//! `big_integral` helper (a closed-form bending integral over a
//! piecewise-linear thickness distribution, evaluated through complex
//! logarithms) are not translated.

mod fuselage;
mod landing_gear;
mod payload;
mod propulsion;
mod systems;
mod wing;

pub use fuselage::Fuselage;
pub use systems::{
    operating_items as estimate_operating_items, systems as estimate_systems, AccessoriesType,
    ControlSystemType,
};
pub use wing::{HorizontalTail, MainWing, VerticalTail};

/// One `Main_Wing`, `Horizontal_Tail` or `Vertical_Tail` reached by
/// [`empty_weight`], and the fuselage, engines and vehicle-level mass/
/// envelope figures the correlations read off it -- narrowed to the fields
/// they actually touch, the same scoping [`super::torenbeek`] uses for its
/// own inputs. Not the whole `mission reference.Vehicle`.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportVehicle {
    /// `vehicle.mass_properties.max_takeoff`, kg.
    pub mtow_kg: f64,
    /// `vehicle.mass_properties.max_zero_fuel`, kg -- taken as given rather
    /// than recomputed, because the value `Weights_Transport` actually reads
    /// is set by `mission_builder.simple_sizing` (`0.73 * max_takeoff`)
    /// *after* `vehicle_builder.build_vehicle` has already computed its own
    /// `operating_empty + payload` figure and been overwritten. A caller
    /// supplies whichever of the two its own pipeline reaches.
    pub max_zero_fuel_kg: f64,
    /// `vehicle.mass_properties.cargo`, kg.
    pub cargo_kg: f64,
    /// `vehicle.passengers`.
    pub passenger_count: u32,
    /// `vehicle.reference_area`, m^2 -- the main wing's own `areas.reference`
    /// in every vehicle this program's mission reference bridge builds, but read
    /// separately since `systems`/`tail_vertical` read it off the vehicle,
    /// not off a wing.
    pub reference_area_m2: f64,
    /// `vehicle.envelope.ultimate_load`.
    pub ultimate_load_factor: f64,
    /// `vehicle.envelope.limit_load`.
    pub limit_load_factor: f64,
    /// `vehicle.systems.control`.
    pub control_type: ControlSystemType,
    /// `vehicle.systems.accessories`.
    pub accessories_type: AccessoriesType,
    /// `sum(prop.number_of_engines for prop in vehicle.networks)` --
    /// narrowed to the one `Turbofan` network every vehicle this program's
    /// bridge builds carries.
    pub engine_count: u32,
    /// `turbofan.sealevel_static_thrust`, N, per engine -- a
    /// `turbofan_sizing()` output (`alas-prop::mission_turbofan`'s own row,
    /// not yet translated), taken here as plain data exactly as
    /// [`super::torenbeek`]'s `mass_wing` takes `design_mass_togw`.
    pub sealevel_static_thrust_per_engine_n: f64,
    /// Technology-specific basis for propulsion mass. The turbofan variant
    /// preserves the translated reference correlation; turboprops use rated
    /// shaft power and never reinterpret the compatibility thrust field.
    pub propulsion_mass_basis: TransportPropulsionMassBasis,
    /// The `Main_Wing`.
    pub main_wing: MainWing,
    /// The `Horizontal_Tail`.
    pub horizontal_tail: HorizontalTail,
    /// The `Vertical_Tail`.
    pub vertical_tail: VerticalTail,
    /// The single fuselage.
    pub fuselage: Fuselage,
}

/// Physical rating used to estimate installed propulsion mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportPropulsionMassBasis {
    /// Historical jet-engine sea-level-static thrust correlation.
    TurbofanThrust,
    /// Turboprop takeoff shaft power per engine, kW.
    TurbopropShaftPower {
        /// Rated takeoff shaft power per engine, kW.
        takeoff_power_kw: f64,
    },
}

/// `output.structures` -- the airframe's structural mass, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuresBreakdown {
    /// Main wing structural mass.
    pub wing_kg: f64,
    /// Horizontal tail structural mass.
    pub horizontal_tail_kg: f64,
    /// Vertical tail structural mass.
    pub vertical_tail_kg: f64,
    /// Fuselage structural mass.
    pub fuselage_kg: f64,
    /// Main landing gear mass.
    pub main_landing_gear_kg: f64,
    /// Nose landing gear mass.
    pub nose_landing_gear_kg: f64,
    /// Nacelle structural mass -- always `0.0` on the `New mission reference` path; see
    /// the module doc.
    pub nacelle_kg: f64,
    /// Paint mass -- always `0.0` for any `method_type` other than FLOPS.
    pub paint_kg: f64,
    /// Sum of all the structural masses above.
    pub total_kg: f64,
}

/// `output.propulsion_breakdown`, kg. `engines_kg`, `thrust_reversers_kg`,
/// `miscellaneous_kg` and `fuel_system_kg` are always `0.0` on the
/// `New mission reference` path; see the module doc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropulsionBreakdown {
    /// Dry engine mass -- always `0.0`; see the struct doc.
    pub engines_kg: f64,
    /// Thrust-reverser mass -- always `0.0`; see the struct doc.
    pub thrust_reversers_kg: f64,
    /// Miscellaneous propulsion mass -- always `0.0`; see the struct doc.
    pub miscellaneous_kg: f64,
    /// Fuel-system mass -- always `0.0`; see the struct doc.
    pub fuel_system_kg: f64,
    /// The whole integrated propulsion system's mass.
    pub total_kg: f64,
}

/// `output.systems_breakdown`, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemsBreakdown {
    /// Flight control system mass.
    pub control_systems_kg: f64,
    /// Auxiliary power unit mass.
    pub apu_kg: f64,
    /// Electrical system mass.
    pub electrical_kg: f64,
    /// Avionics mass.
    pub avionics_kg: f64,
    /// Hydraulics and pneumatics mass.
    pub hydraulics_kg: f64,
    /// Furnishings mass.
    pub furnish_kg: f64,
    /// Air conditioning (with anti-ice folded in) mass.
    pub air_conditioner_kg: f64,
    /// Instruments and navigation equipment mass.
    pub instruments_kg: f64,
    /// Sum of all the system masses above.
    pub total_kg: f64,
}

/// `output.payload_breakdown`, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadBreakdown {
    /// Passenger body mass.
    pub passengers_kg: f64,
    /// Passenger baggage mass.
    pub baggage_kg: f64,
    /// Paid cargo mass.
    pub cargo_kg: f64,
    /// Passengers plus baggage plus cargo.
    pub total_kg: f64,
}

/// `output.operational_items`, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperationalItems {
    /// Unusable fuel, engine oil, passenger service and cargo containers.
    pub operating_items_less_crew_kg: f64,
    /// Flight crew mass (bodies plus baggage allowance).
    pub flight_crew_kg: f64,
    /// Cabin attendant mass (bodies plus baggage allowance).
    pub flight_attendants_kg: f64,
    /// Sum of the operating items and crew above.
    pub total_kg: f64,
}

/// `empty_weight`'s return value -- the whole weight breakdown `output`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightBreakdown {
    /// The airframe structural masses.
    pub structures: StructuresBreakdown,
    /// The propulsion system masses.
    pub propulsion: PropulsionBreakdown,
    /// The on-board systems masses.
    pub systems: SystemsBreakdown,
    /// The payload masses.
    pub payload: PayloadBreakdown,
    /// The operating-item and crew masses.
    pub operational_items: OperationalItems,
    /// `output.empty` -- structures + propulsion + systems. This, not
    /// [`Self::operating_empty_kg`], is what
    /// `vehicle.mass_properties.operating_empty` is set to after
    /// `Weights_Transport.evaluate()`; see the module doc.
    pub empty_kg: f64,
    /// `output.operating_empty` -- `empty_kg` plus the operational items.
    pub operating_empty_kg: f64,
    /// `output.zero_fuel_weight` -- `operating_empty_kg` plus the payload.
    pub zero_fuel_weight_kg: f64,
    /// `output.fuel` -- `max_takeoff_kg` minus the zero-fuel weight.
    pub fuel_kg: f64,
    /// `output.max_takeoff` -- the vehicle's `max_takeoff` mass, echoed back.
    pub max_takeoff_kg: f64,
}

/// `mission reference.Methods.Weights.Correlations.Common.empty_weight`, scoped to
/// `method_type="New mission reference"`; see the module doc.
pub fn empty_weight(vehicle: &TransportVehicle) -> WeightBreakdown {
    let turboprop_estimate = match vehicle.propulsion_mass_basis {
        TransportPropulsionMassBasis::TurbofanThrust => None,
        TransportPropulsionMassBasis::TurbopropShaftPower { takeoff_power_kw } => {
            crate::propulsion_mass::turboprop_installed_mass_from_power(
                takeoff_power_kw,
                vehicle.engine_count as usize,
            )
        }
    };
    // Preserve the upstream jet operation order and its zero-engine guard.
    let wt_prop_total = match vehicle.propulsion_mass_basis {
        TransportPropulsionMassBasis::TurbofanThrust if vehicle.engine_count == 0 => 0.0,
        TransportPropulsionMassBasis::TurbofanThrust => propulsion::integrated_propulsion(
            propulsion::engine_jet(vehicle.sealevel_static_thrust_per_engine_n),
            f64::from(vehicle.engine_count),
        ),
        TransportPropulsionMassBasis::TurbopropShaftPower { .. } => {
            turboprop_estimate.map_or(0.0, |estimate| estimate.total_kg)
        }
    };

    let payload = payload::payload(f64::from(vehicle.passenger_count), vehicle.cargo_kg);
    let operational_items =
        systems::operating_items(vehicle.passenger_count, vehicle.accessories_type);
    let systems = systems::systems(
        vehicle.passenger_count,
        vehicle.control_type,
        vehicle.accessories_type,
        vehicle.reference_area_m2,
        vehicle.horizontal_tail.area_m2 + vehicle.vertical_tail.area_m2,
        vehicle.main_wing.area_m2,
    );

    let mut wt_main_wing = wing::wing_main(
        &vehicle.main_wing,
        vehicle.ultimate_load_factor,
        vehicle.mtow_kg,
        vehicle.max_zero_fuel_kg,
    );
    // Upstream's `if np.isnan(wt_wing): wt_wing = 0.` guard, applied only to
    // the main wing (the tail correlations carry no such guard upstream).
    if wt_main_wing.is_nan() {
        wt_main_wing = 0.0;
    }
    let wt_tail_horizontal = wing::tail_horizontal(
        &vehicle.horizontal_tail,
        &vehicle.main_wing,
        vehicle.ultimate_load_factor,
        vehicle.mtow_kg,
    );
    let wt_tail_vertical = wing::tail_vertical(
        &vehicle.vertical_tail,
        vehicle.ultimate_load_factor,
        vehicle.mtow_kg,
        vehicle.reference_area_m2,
    );

    let wt_fuselage = fuselage::tube(
        &vehicle.fuselage,
        vehicle.main_wing.root_chord_m,
        vehicle.limit_load_factor,
        vehicle.max_zero_fuel_kg,
        wt_main_wing,
        wt_prop_total,
    );

    let gear = landing_gear::landing_gear(vehicle.mtow_kg);

    let structures = StructuresBreakdown {
        wing_kg: wt_main_wing,
        horizontal_tail_kg: wt_tail_horizontal,
        vertical_tail_kg: wt_tail_vertical,
        fuselage_kg: wt_fuselage,
        main_landing_gear_kg: gear.main_kg,
        nose_landing_gear_kg: gear.nose_kg,
        nacelle_kg: 0.0,
        paint_kg: 0.0,
        total_kg: wt_main_wing
            + wt_tail_horizontal
            + wt_tail_vertical
            + wt_fuselage
            + gear.main_kg
            + gear.nose_kg,
    };

    let propulsion = PropulsionBreakdown {
        engines_kg: turboprop_estimate.map_or(0.0, |estimate| estimate.dry_engines_kg),
        thrust_reversers_kg: 0.0,
        miscellaneous_kg: turboprop_estimate.map_or(0.0, |estimate| {
            estimate.propellers_kg + estimate.installation_kg
        }),
        fuel_system_kg: 0.0,
        total_kg: wt_prop_total,
    };

    let empty_kg = structures.total_kg + propulsion.total_kg + systems.total_kg;
    let operating_empty_kg = empty_kg + operational_items.total_kg;
    let zero_fuel_weight_kg = operating_empty_kg + payload.total_kg;
    let fuel_kg = vehicle.mtow_kg - zero_fuel_weight_kg;

    WeightBreakdown {
        structures,
        propulsion,
        systems,
        payload,
        operational_items,
        empty_kg,
        operating_empty_kg,
        zero_fuel_weight_kg,
        fuel_kg,
        max_takeoff_kg: vehicle.mtow_kg,
    }
}
