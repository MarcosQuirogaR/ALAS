// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The class start of a clean-sheet turboprop brief.
//!
//! The shipped defaults and the turbofan fleet statistics describe a
//! transonic jet: a 535 m^2 wing at aspect ratio 9.5 with a tapered inboard
//! panel, a 485 kg/m^2 wing-loading floor, a supercritical section, a 350-seat
//! long-haul FLOPS block (`FMXTOT` 200,000 kg, seven attendants, an APU, three
//! tanks, `VMAX` 0.90), the London-Dubai route and the jet high-lift level. A
//! brief whose engine is a turboprop starts from none of that. Its class is
//! the regional turboprop transport (40-80 seats, a straight wing, no
//! slats, integral wing tanks, propeller brake instead of an APU), and every
//! relation below is taken from the published records of that class.
//!
//! The records are the four regional turboprop transports of *Jane's All the
//! World's Aircraft 2008-2009* whose entries state the quantities (printed
//! page numbers; the PDF page is the printed page + 44): ATR 42-500 (pp.
//! 300-301), ATR 72-500 (p. 302), Dash 8 Series 300 (pp. 69-70) and Dash 8
//! Q400 (pp. 70-71). Each statistic is the
//! median over the records that carry it, as the turbofan statistics are the
//! median over the registered turbofans ([`super::fleet`]). Raymer's
//! twin-turboprop class values (aspect ratio 9.2, wing loading 40 lb/ft^2 =
//! 195 kg/m^2, *Aircraft Design: A Conceptual Approach*, 6th ed., Tables 4.1
//! and 5.5, pp. 78 and 124) pool 9-seat commuters with these transports and
//! are not used: every record here is above them.
//!
//! | quantity | ATR 42-500 | ATR 72-500 | Dash 8-300 | Dash 8 Q400 | median |
//! |---|---|---|---|---|---|
//! | aspect ratio `b^2/S` | 11.08 | 12.0 | 13.39 | 12.8 | 12.40 |
//! | MTOW / S, kg/m^2 | 341.3 | 368.9 | 331.7 | 443.7 | 355.1 |
//! | max fuel / MTOW | 0.242 | 0.222 | 0.249 | - | 0.242 |
//! | tip / root chord | 0.549 | 0.619 | - | - | 0.584 |
//! | engine station / semispan | 0.330 | 0.299 | - | 0.317 | 0.317 |
//!
//! Wing planform: "constant-chord centre section and tapered outer panels"
//! (ATR 42 entry, p. 300), so the trailing-edge break chord equals the root
//! chord. Section: a NACA 43-series derivative, 18 percent thick at the root
//! and 13 percent at the tip, root incidence 2 deg (same entry); the library
//! sections NACA 23018 and 23012 are the nearest. Flight Mach 0.44-0.56 is
//! below any wave-drag onset, so the section class is conventional.
//!
//! Aircraft systems of the class: pressurised to a certified maximum altitude
//! of 7,620 m (25,000 ft) (Dash 8 entry, "Max certified altitude: all", p.
//! 69; ATR 72 "Service ceiling", p. 302); "Instead of APU, starboard
//! propeller braked" (ATR 42 entry, p. 300) and "APU optional" (Dash 8, p.
//! 69); "Fuel in two integral tanks in spar box" (ATR 42, p. 300).
//!
//! Operating rules: one cabin attendant per 50 seats rounded up (EASA
//! ORO.CC.100, the rule `cabin_sync` already applies); `VMAX` the cruise
//! Mach times the CS 25.335(b) `V_D/V_C` = 1.25 envelope ratio (an
//! engineering selection: no record states an MMO); high-lift level the
//! product's `conservative_simple_flaps` (single/double-slotted flaps, no
//! slats; Mattingly Table 2.1 citing Torenbeek, `CLmax_land` 2.2).
//!
//! None of this is validation. It moves where a turboprop brief starts and
//! which floors and block values it carries; no constraint or tolerance is
//! changed, and a key the brief names always wins.

use std::sync::OnceLock;

use serde_json::Value;

use super::fleet::median;
use super::ClassWing;
use crate::engines::PropulsionTechnology;
use crate::{
    AlasConfig, CargoHoldLoading, FlopsInputEvidence, FlopsInputProvenance, OperatingHaulClass,
};

/// One published regional turboprop record of the 2008-2009 Jane's.
struct Record {
    /// Wing span over area, or the aspect ratio the entry states.
    aspect_ratio: f64,
    /// Maximum take-off weight over gross wing area, kg/m^2 (the entry's
    /// "Max wing loading" line, standard MTOW).
    wing_loading_kg_m2: f64,
    /// Maximum fuel mass over MTOW, where the entry states a fuel mass.
    fuel_fraction: Option<f64>,
    /// Outer-panel tip chord over centreline root chord.
    tip_root_chord_ratio: Option<f64>,
    /// Propeller-axis station over semispan.
    engine_semispan_fraction: Option<f64>,
}

const RECORDS: [Record; 4] = [
    // pp. 300-301: span 24.57 m, S 54.50 m^2, MTOW 18,600 kg (341.3 kg/m^2),
    // max fuel 4,500 kg, chords 2.57 m root and 1.41 m tip, propeller
    // centres 8.10 m apart.
    Record {
        aspect_ratio: 24.57 * 24.57 / 54.50,
        wing_loading_kg_m2: 341.3,
        fuel_fraction: Some(4_500.0 / 18_600.0),
        tip_root_chord_ratio: Some(1.41 / 2.57),
        engine_semispan_fraction: Some(0.5 * 8.10 / (0.5 * 24.57)),
    },
    // p. 302: span 27.05 m, aspect ratio 12.0, MTOW 22,500 kg (368.9
    // kg/m^2), max fuel 5,000 kg, tip chord 1.59 m (root as the ATR 42).
    Record {
        aspect_ratio: 12.0,
        wing_loading_kg_m2: 368.9,
        fuel_fraction: Some(5_000.0 / 22_500.0),
        tip_root_chord_ratio: Some(1.59 / 2.57),
        engine_semispan_fraction: Some(0.5 * 8.10 / (0.5 * 27.05)),
    },
    // pp. 69-70: span 27.43 m, S 56.21 m^2, MTOW 18,642 kg (331.7 kg/m^2),
    // optional fuel 4,646 kg.
    Record {
        aspect_ratio: 27.43 * 27.43 / 56.21,
        wing_loading_kg_m2: 331.7,
        fuel_fraction: Some(4_646.0 / 18_642.0),
        tip_root_chord_ratio: None,
        engine_semispan_fraction: None,
    },
    // pp. 70-71: span 28.42 m, aspect ratio 12.8, S 63.08 m^2, basic MTOW
    // 27,986 kg (443.7 kg/m^2); propeller 4.11 m with 1.10 m fuselage
    // clearance on a 2.69 m body.
    Record {
        aspect_ratio: 12.8,
        wing_loading_kg_m2: 443.7,
        fuel_fraction: None,
        tip_root_chord_ratio: None,
        engine_semispan_fraction: Some((0.5 * 2.69 + 1.10 + 0.5 * 4.11) / (0.5 * 28.42)),
    },
];

/// Median class relations of the regional turboprop transports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TurbopropClass {
    /// Projected aspect ratio `b^2/S`.
    pub aspect_ratio: f64,
    /// Take-off wing loading `MTOW/S`, kg/m^2.
    pub wing_loading_kg_m2: f64,
    /// Lowest published wing loading of the class, kg/m^2: the floor.
    pub minimum_wing_loading_kg_m2: f64,
    /// Maximum fuel mass over MTOW.
    pub fuel_fraction: f64,
    /// Outer-panel tip chord over centreline root chord.
    pub tip_root_chord_ratio: f64,
    /// Propeller-axis station over semispan.
    pub engine_semispan_fraction: f64,
}

/// Trailing-edge break chord over root chord: the class wing has a
/// constant-chord centre section (ATR 42 entry, p. 300).
pub(crate) const BREAK_ROOT_CHORD_RATIO: f64 = 1.0;
/// Certified maximum operating altitude of the class, m (25,000 ft).
const MAXIMUM_OPERATING_ALTITUDE_M: f64 = 7_620.0;
/// CS 25.335(b) `V_D/V_C` envelope ratio applied to the cruise Mach.
const MAXIMUM_MACH_OVER_CRUISE: f64 = 1.25;
/// Passengers per cabin attendant (EASA ORO.CC.100).
const PASSENGERS_PER_ATTENDANT: usize = 50;
/// Integral wing tanks of the class (one per half-wing).
const FUEL_TANK_COUNT: usize = 2;
/// Wing incidence of the centre section, deg (ATR 42 entry, p. 300).
const ROOT_INCIDENCE_DEG: f64 = 2.0;
/// The performance level of a flapped wing without slats; its take-off and
/// landing lift limits are replaced by the class values below.
const HIGH_LIFT_LEVEL: &str = "conservative_simple_flaps";
/// Take-off and landing `CLmax` of a twin turboprop with double-slotted
/// flaps and no slats: Sadraey, *Aircraft Design: A Systems Engineering
/// Approach*, Table 5.14 (p. 232) lists 2.07 and 2.71 for the Short Skyvan 3,
/// whose flaps Table 5.16 (p. 237) gives as double-slotted (18 deg take-off,
/// 45 deg landing). The ATR 42 flaps are "two-segment double-slotted" (Jane's
/// p. 300). One published record; cross-check only, the take-off to landing
/// ratio 0.76 agrees with Raymer's "about 80 percent" (6th ed., p. 127).
const CL_MAX_TAKEOFF: f64 = 2.07;
const CL_MAX_LANDING: f64 = 2.71;
/// Section pair nearest the entry's 18 percent root / 13 percent tip.
const ROOT_AIRFOIL: &str = "naca23018";
const TIP_AIRFOIL: &str = "naca23012";
/// Pounds per kilogram (the rule below states the weight in pounds).
const LB_PER_KG: f64 = 2.204_622_621_848_775_7;
/// CS 25.303 / 14 CFR 25.303 factor of safety between limit and ultimate.
const ULTIMATE_FACTOR_OF_SAFETY: f64 = 1.5;
/// Earth mean radius for the route selection, m.
const EARTH_RADIUS_M: f64 = 6_371_000.0;
/// Field elevation above which an aerodrome is not an ordinary regional
/// sector end, m.
const ORDINARY_AERODROME_MAX_ELEVATION_M: f64 = 1_000.0;
/// Shortest landing distance of an ordinary aerodrome, m.
const ORDINARY_AERODROME_MIN_LDA_M: f64 = 2_500.0;

/// The class relations, measured once.
pub(crate) fn turboprop_class() -> &'static TurbopropClass {
    static CLASS: OnceLock<TurbopropClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let stat = |f: &dyn Fn(&Record) -> Option<f64>| {
            median(RECORDS.iter().filter_map(f)).unwrap_or(f64::NAN)
        };
        TurbopropClass {
            aspect_ratio: stat(&|r| Some(r.aspect_ratio)),
            wing_loading_kg_m2: stat(&|r| Some(r.wing_loading_kg_m2)),
            minimum_wing_loading_kg_m2: RECORDS
                .iter()
                .map(|r| r.wing_loading_kg_m2)
                .fold(f64::INFINITY, f64::min),
            fuel_fraction: stat(&|r| r.fuel_fraction),
            tip_root_chord_ratio: stat(&|r| r.tip_root_chord_ratio),
            engine_semispan_fraction: stat(&|r| r.engine_semispan_fraction),
        }
    })
}

impl AlasConfig {
    /// Whether the brief's engines are a twin turboprop pair (the regional
    /// turboprop class start applies; a four-engine airlifter is not in it).
    pub(crate) fn is_turboprop_brief(&self) -> bool {
        self.geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop
            && self.geometry.engine.spanwise_positions_m.len() == 2
    }
}

/// The class wing of a turboprop brief: area from the class wing loading,
/// span from the class aspect ratio (held under the aerodrome code's span
/// limit), straight (the class cruises far below the wave-drag onset).
pub(super) fn class_wing(config: &AlasConfig) -> Option<ClassWing> {
    let class = turboprop_class();
    let area_m2 = config.requirements.mtow_kg / class.wing_loading_kg_m2;
    let mut span_m = (class.aspect_ratio * area_m2).sqrt();
    if let Some(limit) = config.max_design_span_m() {
        span_m = span_m.min(limit);
    }
    let sweep_deg = super::fleet_statistics().map_or(0.0, |stats| {
        super::sweep_for_mach_deg(config.requirements.cruise_mach, stats.normal_mach)
    });
    (area_m2.is_finite() && area_m2 > 0.0 && span_m.is_finite() && span_m > 0.0).then_some(
        ClassWing {
            area_m2,
            span_m,
            sweep_deg,
        },
    )
}

/// Great-circle distance, m.
pub(super) fn great_circle_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat_a, lat_b) = (a.0.to_radians(), b.0.to_radians());
    let half_dlat = 0.5 * (lat_b - lat_a);
    let half_dlon = 0.5 * (b.1 - a.1).to_radians();
    let h = half_dlat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_dlon.sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}

/// The pair of ordinary aerodromes (standard day, below 1,000 m, landing
/// distance of 2,500 m or more) of the curated table whose longest route the
/// planner can return, the great circle times `max_route_stretch` (the
/// configured generated-airway detour limit, at least 1), is nearest below
/// `range_nmi`; when no pair is that short, the shortest pair. The first such
/// pair in table order on a tie.
///
/// The aircraft is sized on the design range and then flies the planned route
/// at the design payload (`offdesign_tow`): a route longer than the design
/// range would need more takeoff mass than the closed aircraft has. Choosing
/// the pair by its longest possible route makes that impossible for a
/// generated route (a dispatched plan is exempt from the stretch limit).
pub(super) fn pair_for_design_range(
    range_nmi: f64,
    max_route_stretch: f64,
) -> Option<(&'static str, &'static str)> {
    if !(range_nmi.is_finite() && range_nmi > 0.0) {
        return None;
    }
    let stretch = if max_route_stretch.is_finite() {
        max_route_stretch.max(1.0)
    } else {
        1.0
    };
    let ordinary: Vec<_> = crate::airports::database()
        .iter()
        .filter(|a| {
            a.isa_deviation_c == 0.0
                && a.elevation_m <= ORDINARY_AERODROME_MAX_ELEVATION_M
                && a.lda_m >= ORDINARY_AERODROME_MIN_LDA_M
        })
        .collect();
    let target_m = range_nmi * 1_852.0;
    // Cost: the shortfall of the longest route below the range, or, for a
    // pair too long to fit, a large offset plus its excess.
    let mut best: Option<(f64, usize, usize)> = None;
    for (i, a) in ordinary.iter().enumerate() {
        for (j, b) in ordinary.iter().enumerate().skip(i + 1) {
            let longest_m = stretch
                * great_circle_m(
                    (a.latitude_deg, a.longitude_deg),
                    (b.latitude_deg, b.longitude_deg),
                );
            let cost = if longest_m <= target_m {
                target_m - longest_m
            } else {
                EARTH_RADIUS_M + longest_m - target_m
            };
            if best.is_none_or(|(m, _, _)| cost < m) {
                best = Some((cost, i, j));
            }
        }
    }
    best.map(|(_, i, j)| (ordinary[i].name.as_str(), ordinary[j].name.as_str()))
}

/// Ultimate load factor of a brief of `mtow_kg`: 1.5 times the positive limit
/// maneuvering load factor of 14 CFR 25.337(b) / CS 25.337(b),
/// `n = 2.1 + 24,000 / (W + 10,000)` with `W` the design maximum take-off
/// weight in pounds, not below 2.5 and not required above 3.8 (Torenbeek,
/// *Advanced Aircraft Design*, 2013, Eqs. 11.5-11.6, p. 324). The shipped
/// 3.75 is `n = 2.5`, which is the rule above 50,000 lb (22.7 t); a class
/// brief below that mass would otherwise fail the envelope check
/// (`alas_perf::performance::far25_positive_limit_load_factor_min`).
///
/// Limitation: the brief states a ceiling, the rule reads the take-off weight
/// the mission closes at, and the envelope check evaluates it at the closed
/// mass. A design that closes below a ceiling under 50,000 lb is still flagged
/// by that check when its own minimum exceeds this value (a 20 t ceiling that
/// closes at 19.7 t needs n = 2.549 against 2.544 here). Tying the factor to
/// the closed mass belongs to the sizing loop, not to the start.
pub(super) fn ultimate_load_factor(mtow_kg: f64) -> Option<f64> {
    let weight_lb = mtow_kg * LB_PER_KG;
    (weight_lb.is_finite() && weight_lb > 0.0).then(|| {
        ULTIMATE_FACTOR_OF_SAFETY * (2.1 + 24_000.0 / (weight_lb + 10_000.0)).clamp(2.5, 3.8)
    })
}

fn provenance(location: &str, applicability: &str, uncertainty: &str) -> FlopsInputProvenance {
    FlopsInputProvenance {
        document: "Jane's All the World's Aircraft 2008-2009 (regional turboprop transports: ATR 42/72, Dash 8)".to_owned(),
        revision: "2008-2009 edition, class medians; ALAS clean-sheet turboprop class 2026-10-15".to_owned(),
        location: location.to_owned(),
        applicability: applicability.to_owned(),
        evidence: FlopsInputEvidence::UserDeclared,
        uncertainty: uncertainty.to_owned(),
    }
}

/// Seed `instance` (the file is overlaid on it afterwards, so a key the
/// brief names wins) with the regional turboprop class values the shipped
/// jet defaults get wrong. `brief` is `instance` with the file laid over it.
pub(super) fn seed(instance: &mut AlasConfig, brief: &AlasConfig, data: &Value) {
    let class = turboprop_class();
    let r = &brief.requirements;
    instance.requirements.min_wing_loading_kg_m2 = class.minimum_wing_loading_kg_m2;
    if let Some(factor) = ultimate_load_factor(r.mtow_kg) {
        instance.requirements.ultimate_load_factor = factor;
    }
    if let Ok(level) = crate::performance_presets::get(HIGH_LIFT_LEVEL) {
        instance.performance = level.settings.clone();
        let perf = &mut instance.performance;
        perf.cl_max_to = CL_MAX_TAKEOFF;
        perf.cl_max_to_source = "Sadraey, Aircraft Design: A Systems Engineering Approach, Table 5.14 p. 232 and Table 5.16 p. 237: twin turboprop, double-slotted flaps, no slats (Short Skyvan 3, CLmax take-off 2.07); a class value, one published record, not an aircraft coefficient".to_owned();
        perf.cl_max_land = CL_MAX_LANDING;
        perf.cl_max_land_source = "Sadraey, Aircraft Design: A Systems Engineering Approach, Table 5.14 p. 232 and Table 5.16 p. 237: twin turboprop, double-slotted flaps, no slats (Short Skyvan 3, CLmax landing 2.71); a class value, one published record, not an aircraft coefficient".to_owned();
    }
    let design_range_nmi = brief.optimizer.objective.design_range_nmi;
    if data.pointer("/departure_airport").is_none() && data.pointer("/arrival_airport").is_none() {
        let stretch = brief.mission.max_airway_stretch;
        if let Some((departure, arrival)) = pair_for_design_range(design_range_nmi, stretch) {
            instance.departure_airport = departure.to_owned();
            instance.arrival_airport = arrival.to_owned();
        }
    }
    let t = &mut instance.mass_model.flops_transport;
    t.maximum_mach = Some(MAXIMUM_MACH_OVER_CRUISE * r.cruise_mach);
    if design_range_nmi > 0.0 {
        t.design_range_nmi = Some(design_range_nmi);
    }
    let seats = usize::try_from(r.num_passengers.max(0)).unwrap_or(0);
    t.flight_attendant_count = Some(seats.div_ceil(PASSENGERS_PER_ATTENDANT));
    t.fuel_tank_count = Some(FUEL_TANK_COUNT);
    t.maximum_fuel_capacity_kg = Some(class.fuel_fraction * r.mtow_kg);
    t.apu_installed = false;
    t.cargo_loading = Some(CargoHoldLoading::Bulk);
    t.haul_class = Some(OperatingHaulClass::ShortMediumHaul);
    t.provenance.mission = provenance(
        "VMAX = 1.25 x cruise Mach (CS 25.335(b)); DESRNG = the brief's design range",
        "regional turboprop transport class start of a clean-sheet brief",
        "no record states an MMO; the factor is an engineering selection",
    );
    t.provenance.cabin = provenance(
        "one cabin attendant per 50 seats (EASA ORO.CC.100)",
        "regional turboprop transport class start of a clean-sheet brief",
        "operational minimum, not an airline crewing statement",
    );
    t.provenance.architecture = provenance(
        "pp. 300-302 (ATR 42/72) and 69-71 (Dash 8): integral wing tanks, no APU, bulk holds; max fuel / MTOW class median",
        "twin wing-mounted turboprops with integral wing tanks",
        "four records; the fuel fraction is the median of three",
    );
    instance
        .mass_model
        .flops_structure
        .maximum_operating_altitude_m = Some(MAXIMUM_OPERATING_ALTITUDE_M);
}

/// Wing sections, incidence and engine station of the class, for the
/// geometry the brief does not name (called from [`super::geometry`]).
pub(super) fn shape_geometry(
    geometry: &mut crate::GeometryConfig,
    data: &Value,
    class_span_m: f64,
) {
    let set = |pointer: &str| data.pointer(pointer).is_some();
    let class = turboprop_class();
    let wing = &mut geometry.wing;
    if !set("/geometry/wing/root_airfoil") {
        wing.root_airfoil = ROOT_AIRFOIL.to_owned();
    }
    if !set("/geometry/wing/tip_airfoil") {
        wing.tip_airfoil = TIP_AIRFOIL.to_owned();
    }
    if !set("/geometry/wing/airfoil_class") {
        wing.airfoil_class = crate::AirfoilClass::Conventional;
    }
    if !set("/geometry/wing/root_twist_deg") {
        wing.root_twist_deg = ROOT_INCIDENCE_DEG;
    }
    if !set("/geometry/wing/break_twist_deg") {
        wing.break_twist_deg = ROOT_INCIDENCE_DEG;
    }
    let engine = &mut geometry.engine;
    let twin = engine.spanwise_positions_m.len() == 2;
    if twin && !set("/geometry/engine/spanwise_positions_m") {
        let y = class.engine_semispan_fraction * 0.5 * class_span_m;
        engine.spanwise_positions_m = vec![y, -y];
    }
}
