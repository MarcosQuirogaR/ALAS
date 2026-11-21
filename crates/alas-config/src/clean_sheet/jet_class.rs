// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS block of a clean-sheet jet brief.
//!
//! The shipped FLOPS block is the 350-seat long-haul study scenario of the
//! AVE reference twin (`VMAX` 0.90, `DESRNG` 7,600 nmi, `FMXTOT` 200,000 kg,
//! the long-haul operating-item relation, containerised holds, two wing
//! engines). Laid under a 100-seat single-aisle brief it prices about 4.5 t of
//! long-haul passenger service on a 72 t aircraft, and under a quad or a
//! trijet it describes engines that were not built. A jet brief therefore
//! takes these inputs from the registered transport jets, split by what the
//! brief's own cabin section is:
//!
//! - **Single aisle or twin aisle**: the seats abreast the brief's section
//!   admits under CS 25.817 ([`super::geometry::seats_abreast`]). Every
//!   registered twin-aisle jet is declared long-haul and every single-aisle
//!   jet short/medium-haul, and the source publishes the two operating-item
//!   relations with no range threshold, so the section selects the relation.
//! - `DESRNG`: the brief's design range, else the median declared range of
//!   the jets of its section. `FMXTOT`: the median declared maximum fuel mass
//!   over MTOW of those jets, times the brief's MTOW. `VMAX`: the fleet-median
//!   ratio of declared `VMAX` to cruise Mach, times the brief's cruise Mach.
//! - Hold loading: a twin-aisle section takes containers; a single-aisle
//!   section takes bulk, as most of the registered single-aisle jets declare.
//! - Engine mounting: from the built installation, an engine on the centreline
//!   or on the rear fuselage is not a wing engine
//!   ([`crate::EngineConfig::resolved_engine_mounting`]).
//! - The wing-area ceiling and wing-loading floor: the shipped 535 m^2 and
//!   485 kg/m^2 are the AVE's. A brief takes them from its own class wing at
//!   the low end of the lift-coefficient window, which is the largest wing the
//!   derived box already searches.
//! - The ultimate load factor: 14 CFR 25.337(b) at the brief's MTOW, as the
//!   turboprop class has it.
//! - The engine, when the brief names none: the shipped default is the AVE's
//!   467 kN GE9X, about 10 t installed per engine, whatever the brief's size.
//!   The brief takes the catalogue turbofan nearest in thrust ratio to the
//!   rated thrust per engine its own CS 25.121(b) second segment asks of that
//!   engine ([`super::oei_thrust`]): the climb gradient of its engine count on
//!   the `n - 1` remaining engines at `V2`, on the brief's take-off drag with
//!   that engine's nacelles, over that engine's take-off lapse. A quad of a
//!   twin's mass therefore needs about two thirds of the twin's installed
//!   thrust at the quad's 3.0 percent gradient, in engines of a third of the
//!   twin's each. The sizing then scales that engine, and its validity window
//!   still reports a brief no catalogue engine fits.
//!
//! None of this is validation. It moves which inputs a jet brief starts from;
//! no constraint or tolerance changes, and a key the brief names always wins.

use std::sync::OnceLock;

use serde_json::Value;

use super::fleet::median;
use super::{turboprop_class, LIFT_COEFFICIENT_WINDOW};
use crate::engines::PropulsionTechnology;
use crate::presets::{registry, AircraftClass};
use crate::{
    AlasConfig, CargoHoldLoading, FlopsInputEvidence, FlopsInputProvenance, OperatingHaulClass,
};

/// Passengers per cabin attendant (EASA ORO.CC.100).
const PASSENGERS_PER_ATTENDANT: usize = 50;

/// Medians over the registered transport jets of one cabin section.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SectionClass {
    /// Declared `DESRNG`, nmi.
    design_range_nmi: f64,
    /// Declared maximum usable fuel over MTOW.
    fuel_fraction: f64,
}

/// Medians over the registered transport jets.
#[derive(Debug, Clone, Copy, PartialEq)]
struct JetClass {
    single_aisle: SectionClass,
    twin_aisle: SectionClass,
    /// Declared `VMAX` over the requirement's cruise Mach.
    maximum_mach_over_cruise: f64,
}

fn measure() -> Option<JetClass> {
    let rows: Vec<_> = registry()
        .iter()
        // The transport fleet, as in the fleet statistics.
        .filter(|preset| {
            preset.geometry.engine.propulsion_technology == PropulsionTechnology::Turbofan
                && preset.class == AircraftClass::Transport
        })
        .filter_map(|preset| {
            let inputs = crate::preset_flops::inputs_for(preset.name)?;
            Some((preset, inputs.transport))
        })
        .collect();
    let section = |long_haul: bool| -> Option<SectionClass> {
        let of = || {
            rows.iter().filter(move |(_, t)| {
                (t.haul_class == Some(OperatingHaulClass::LongHaul)) == long_haul
            })
        };
        Some(SectionClass {
            design_range_nmi: median(of().filter_map(|(_, t)| t.design_range_nmi))?,
            fuel_fraction: median(of().filter_map(|(preset, t)| {
                Some(t.maximum_fuel_capacity_kg? / preset.requirements.mtow_kg)
            }))?,
        })
    };
    Some(JetClass {
        single_aisle: section(false)?,
        twin_aisle: section(true)?,
        maximum_mach_over_cruise: median(
            rows.iter()
                .filter_map(|(preset, t)| Some(t.maximum_mach? / preset.requirements.cruise_mach)),
        )?,
    })
}

/// The catalogue turbofan whose rating is nearest (in thrust ratio) to the
/// rated thrust per engine the brief's second segment asks of it
/// ([`super::oei_thrust::SecondSegment`]); `None` when the brief cannot be
/// sized.
pub(super) fn class_engine(brief: &AlasConfig, data: &Value) -> Option<&'static str> {
    let segment = super::oei_thrust::SecondSegment::of(brief, data)?;
    crate::engines::database()
        .iter()
        .filter(|engine| {
            engine.technology == PropulsionTechnology::Turbofan && engine.thrust_kn > 0.0
        })
        .filter_map(|engine| {
            let required_kn = segment.rated_thrust_per_engine_n(engine)? / 1_000.0;
            Some(((engine.thrust_kn / required_kn).ln().abs(), engine))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, engine)| engine.name.as_str())
}

fn jet_class() -> Option<&'static JetClass> {
    static CLASS: OnceLock<Option<JetClass>> = OnceLock::new();
    CLASS.get_or_init(measure).as_ref()
}

fn provenance(location: &str, uncertainty: &str) -> FlopsInputProvenance {
    FlopsInputProvenance {
        document: "ALAS clean-sheet jet class: medians of the registered transport jets' declared FLOPS inputs".to_owned(),
        revision: "ALAS 2026-11-06".to_owned(),
        location: location.to_owned(),
        applicability: "start of a clean-sheet jet brief; a configured run replaces it with aircraft evidence when available".to_owned(),
        evidence: FlopsInputEvidence::UserDeclared,
        uncertainty: uncertainty.to_owned(),
    }
}

/// Seed `instance` (the file is overlaid on it afterwards, so a key the brief
/// names wins) with the class inputs of a jet brief. `brief` is `instance`
/// with the file laid over it.
pub(super) fn seed(instance: &mut AlasConfig, brief: &AlasConfig, data: &Value) {
    let Some(class) = jet_class() else {
        return;
    };
    let r = &brief.requirements;
    let twin_aisle = super::geometry::seats_abreast(brief) > 6.0;
    let section = if twin_aisle {
        &class.twin_aisle
    } else {
        &class.single_aisle
    };
    if let Some(factor) = turboprop_class::ultimate_load_factor(r.mtow_kg) {
        instance.requirements.ultimate_load_factor = factor;
    }
    if data.pointer("/geometry/engine/engine_name").is_none() {
        if let Some(name) = class_engine(brief, data) {
            instance.geometry.engine.engine_name = name.to_owned();
            instance.geometry.engine.apply_engine_spec();
        }
    }
    if let Some(wing) = brief.class_wing() {
        let largest_m2 = wing.area_m2 / LIFT_COEFFICIENT_WINDOW.0;
        instance.requirements.max_wing_area_m2 = largest_m2;
        instance.requirements.min_wing_loading_kg_m2 = r.mtow_kg / largest_m2;
    }
    let design_range_nmi = brief.optimizer.objective.design_range_nmi;
    if data.pointer("/departure_airport").is_none() && data.pointer("/arrival_airport").is_none() {
        let stretch = brief.mission.max_airway_stretch;
        if let Some((departure, arrival)) =
            turboprop_class::pair_for_design_range(design_range_nmi, stretch)
        {
            instance.departure_airport = departure.to_owned();
            instance.arrival_airport = arrival.to_owned();
        }
    }
    let t = &mut instance.mass_model.flops_transport;
    t.maximum_mach = Some(class.maximum_mach_over_cruise * r.cruise_mach);
    t.design_range_nmi = Some(if design_range_nmi > 0.0 {
        design_range_nmi
    } else {
        section.design_range_nmi
    });
    let seats = usize::try_from(r.num_passengers.max(0)).unwrap_or(0);
    t.flight_attendant_count = Some(seats.div_ceil(PASSENGERS_PER_ATTENDANT));
    t.maximum_fuel_capacity_kg = Some(section.fuel_fraction * r.mtow_kg);
    let engine = &brief.geometry.engine;
    let fuselage_engines = engine.installed_fuselage_engine_count();
    t.fuselage_mounted_engine_count = Some(fuselage_engines);
    t.wing_mounted_engine_count = Some(engine.spanwise_positions_m.len() - fuselage_engines);
    if twin_aisle {
        t.cargo_loading = Some(CargoHoldLoading::Containerized);
        t.haul_class = Some(OperatingHaulClass::LongHaul);
    } else {
        t.cargo_loading = Some(CargoHoldLoading::Bulk);
        t.haul_class = Some(OperatingHaulClass::ShortMediumHaul);
    }
    t.provenance.mission = provenance(
        "VMAX = fleet median VMAX / cruise Mach x the brief's cruise Mach; DESRNG = the brief's design range, else the median of its cabin section",
        "class medians of declared inputs, not a certified speed or a charted range",
    );
    t.provenance.cabin = provenance(
        "one cabin attendant per 50 seats (EASA ORO.CC.100); all-economy until the layout seats the cabin",
        "operational minimum, not an airline crewing statement",
    );
    t.provenance.architecture = provenance(
        "engine mounting from the built installation; FMXTOT = the section's median maximum fuel / MTOW x the brief's MTOW; three tanks and 3,000 psi remain the study inputs",
        "class medians; the tank count and the hydraulic pressure are not derived from the brief",
    );
}
