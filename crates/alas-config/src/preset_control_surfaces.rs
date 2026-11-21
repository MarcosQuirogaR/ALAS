// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The control-surface runs of each registered aircraft.
//!
//! Kept beside the presets, like [`crate::preset_fuel_tanks`], so the registry
//! does not grow a field for every discipline. [`config_for`] is the one seam
//! the configuration loader reads.
//!
//! A run is a chord fraction over a span band of the built surface. Stations
//! are fractions of the projected semi-span from the centreline (of the fin
//! height above its root for the rudder). The evidence grades are:
//!
//! - **S1**, a printed area: the chord fraction is that area over the chord
//!   integral of the band on the built surface (Jane's All the World's Aircraft
//!   2008-09, AREAS blocks). It is a derived mean: a run with a gap or a tab
//!   keeps the printed area, not the local chord.
//! - **S2**, a printed chord ratio `C_A/C`, `C_E/C_h`, `C_R/C_V` (Sadraey,
//!   Aircraft Design: A Systems Engineering Approach, Tables 12.17, 12.18,
//!   12.20), by one rule: the type's own row; else the nearest row of the same
//!   family: Airbus jets the A340-600 rows (aileron the mean of its 0.30 and
//!   0.25 rows), other wide bodies the 777-200 rows, other single-aisle
//!   airliners the Fokker 100 rows (Embraer's rudder its ERJ145 row), business
//!   jets the row nearest in take-off mass (Cessna Citation III for the Learjet
//!   60), military transports the C-130E row.
//! - **E**, where nothing is printed: the S1 chord of the class's printed
//!   aircraft, the A320 for single-aisle and business jets (slat 0.145, flap
//!   0.250, spoiler 0.110) and the 747-400 for wide bodies (slat 0.118, flap
//!   0.260, spoiler 0.104). None is tuned to a result.
//!
//! Stations come from the manufacturer's drawings:
//!
//! - Front-view ground clearance figures, whose arrows name each surface end,
//!   scaled from the fin centre and the wing tip: Airbus A320, A340-200/-300,
//!   A350 and A380 Aircraft Characteristics, section 2-3-0; COMAC C919 Aircraft
//!   Characteristics for Airport Planning, section 02-03 (perspective renders).
//! - Rudolph, High-Lift Systems on Commercial Subsonic Airliners, NASA CR-4746:
//!   Fig. 2.16, DC-10-30 printed butt lines. 747-400: Boeing 747-400 Airplane
//!   Characteristics for Airport Planning, D6-58326-1, Fig. 2.2.1 plan, scaled
//!   from the centreline and the tip, which Rudolph Fig. 2.7 matches.
//! - Plan views read by eye: Embraer E-Jets E2 Airport Planning Manual Fig. 2.2;
//!   Airbus A220 Airport and Maintenance Planning general dimensions; Jane's
//!   2008-09 A400M three-view.
//! - Estimated stations (no surface lines on the drawing on file): B787-9 from
//!   the track fairings of the Boeing 787 ACAP plan; ATR 72, C295 and Learjet 60
//!   from the entry text and the Jane's three-views.
//!
//! No leading-edge device: A400M (Jane's 2008-09, "No slats"); ATR 72 and C295
//! (none in the Jane's flying-controls entries or AREAS blocks); Learjet 60
//! (none among the surfaces of the Learjet 60 Pilot Training Manual, Fig. 15-1).
//! No spoiler: C295 (none in the flying-controls entry or the AREAS block). A
//! zero chord fraction states that.
//!
//! An inboard aileron (747, 787, DC-10) sits between an inboard and an outboard
//! flap: the flap run spans both flaps and the aileron band is cut out of it
//! (`ControlSurfacesConfig::flap_bands`).

use crate::ControlSurfacesConfig;

/// One run: chord fraction, span start, span end.
type Run = (f64, f64, f64);
/// A surface the aircraft does not have, stated by a zero chord fraction. The
/// stations are the default run's, as a span pair must stay ordered.
const NO_SLAT: Run = (0.0, 0.08, 0.95);
const NO_SPOILER: Run = (0.0, 0.10, 0.64);
const NO_RUN: Run = (0.0, 0.0, 0.0);

/// S1 chords from the printed totals on the built surfaces (see the module
/// notes): A320 spoilers 8.64 m2; 747-400 leading-edge flaps 43.85, trailing-edge
/// flaps 78.69, ailerons 20.90 and spoilers 28.24 m2; A400M elevators 11.49 m2.
const A320_SLAT: f64 = 0.1452;
const A320_FLAP: f64 = 0.2504;
const A320_SPOILER: f64 = 0.1096;
const B747_SLAT: f64 = 0.1177;
const B747_FLAP: f64 = 0.2602;
const B747_AILERON: f64 = 0.1945;
const B747_SPOILER: f64 = 0.1039;
const A400M_ELEVATOR: f64 = 0.1921;
/// DC-10 S1 chords from printed totals on the drawn bands: trailing-edge flaps 62.1 m2 (Jenkinson,
/// Simpkin and Rhodes, Civil Jet Aircraft Design, 1999, Data A, Table 6) and spoilers 12.7 m2
/// (secondary listing, grade C); the 51.25 and 9.84 m2 of the wide-body class chords were short
/// of both.
const DC10_FLAP: f64 = 0.3153;
const DC10_SPOILER: f64 = 0.1341;
/// E chords: the S1 chords of the class's printed aircraft.
const NARROW_BODY_SLAT: f64 = A320_SLAT;
const NARROW_BODY_FLAP: f64 = A320_FLAP;
const NARROW_BODY_SPOILER: f64 = A320_SPOILER;
const WIDE_BODY_SLAT: f64 = B747_SLAT;
const WIDE_BODY_FLAP: f64 = B747_FLAP;
const WIDE_BODY_SPOILER: f64 = B747_SPOILER;

/// The runs of one aircraft.
struct Surfaces {
    slat: Run,
    flap: Run,
    aileron: Run,
    inboard_aileron: Run,
    spoiler: Run,
    elevator: Run,
    rudder: Run,
}

/// The aircraft whose airframe `preset_name` shares, for the conversions.
fn airframe(preset_name: &str) -> &str {
    match preset_name {
        "A320P2F" => "A320-200",
        "B747-400F" => "B747-400",
        "DC-10-30F" => "DC-10",
        other => other,
    }
}

// Surface fractions are published chart data; a value near pi/8 is not that constant.
#[allow(clippy::approx_constant)]
fn surfaces(name: &str) -> Option<Surfaces> {
    Some(match name {
        // S1 slat 12.64, flaps 21.10, ailerons 2.74, spoilers 8.64 (Jane's p. 272). The
        // airbrakes 2.35 printed beside them are the inner six of the same ten panels
        // (p. 271), so they are not added. S2 elevator, A320 row; rudder, A340-600 row.
        "A320-200" => Surfaces {
            slat: (A320_SLAT, 0.169, 0.959),
            flap: (A320_FLAP, 0.115, 0.767),
            aileron: (0.2659, 0.810, 0.963),
            inboard_aileron: NO_RUN,
            spoiler: (A320_SPOILER, 0.154, 0.797),
            elevator: (0.32, 0.03, 0.97),
            rudder: (0.32, 0.05, 0.97),
        },
        // S2 aileron, elevator and rudder, A340-600 rows; E slat, flap and spoiler.
        // Flap, aileron and spoiler ends at the flap-aileron junction are the drawn
        // edges: the clearance arrows there stand off the corner they dimension.
        "A340-300" => Surfaces {
            slat: (WIDE_BODY_SLAT, 0.110, 0.952),
            flap: (WIDE_BODY_FLAP, 0.104, 0.647),
            aileron: (0.275, 0.660, 0.942),
            inboard_aileron: NO_RUN,
            spoiler: (WIDE_BODY_SPOILER, 0.134, 0.646),
            elevator: (0.31, 0.10, 0.87),
            rudder: (0.32, 0.03, 0.97),
        },
        // S2 A340-600 rows; E rest. The leading-edge run spans droop nose and slats.
        "A350-900" => Surfaces {
            slat: (WIDE_BODY_SLAT, 0.125, 0.909),
            flap: (WIDE_BODY_FLAP, 0.096, 0.624),
            aileron: (0.275, 0.625, 0.886),
            inboard_aileron: NO_RUN,
            spoiler: (WIDE_BODY_SPOILER, 0.162, 0.633),
            elevator: (0.31, 0.08, 0.93),
            rudder: (0.32, 0.05, 0.95),
        },
        // S2 A340-600 rows; E rest. The leading-edge run spans droop nose and slats.
        "A380-800" => Surfaces {
            slat: (WIDE_BODY_SLAT, 0.214, 0.980),
            flap: (WIDE_BODY_FLAP, 0.113, 0.640),
            aileron: (0.275, 0.653, 0.884),
            inboard_aileron: NO_RUN,
            spoiler: (WIDE_BODY_SPOILER, 0.106, 0.805),
            elevator: (0.31, 0.08, 0.93),
            rudder: (0.32, 0.05, 0.95),
        },
        // S1 leading-edge flaps 43.85, trailing-edge flaps 78.69, ailerons 20.90 in
        // total, spoilers 12.78 plus 15.46, elevators 30.38, rudder 21.37 (Jane's p. 685).
        // Stations off the D6-58326-1 plan: Krueger flaps 0.18-0.90, inboard aileron
        // 0.35-0.40 between the flaps, outboard aileron 0.66-0.87, spoilers 0.18-0.33
        // and 0.41-0.62 as one run whose printed area keeps the gap.
        "B747-400" => Surfaces {
            slat: (B747_SLAT, 0.178, 0.902),
            flap: (B747_FLAP, 0.116, 0.641),
            aileron: (B747_AILERON, 0.660, 0.870),
            inboard_aileron: (B747_AILERON, 0.346, 0.399),
            spoiler: (B747_SPOILER, 0.179, 0.622),
            elevator: (0.2407, 0.05, 0.95),
            rudder: (0.2839, 0.04, 0.96),
        },
        // E stations; S2 aileron, elevator and rudder chords, 777-200 rows; E rest.
        "B787-9" => Surfaces {
            slat: (WIDE_BODY_SLAT, 0.12, 0.93),
            flap: (WIDE_BODY_FLAP, 0.12, 0.73),
            aileron: (0.22, 0.75, 0.93),
            inboard_aileron: (0.22, 0.33, 0.40),
            spoiler: (WIDE_BODY_SPOILER, 0.20, 0.70),
            elevator: (0.32, 0.08, 0.93),
            rudder: (0.28, 0.05, 0.95),
        },
        // Printed butt lines of NASA CR-4746 Fig. 2.16 (tip BL 992.2): flaps 118.5-697.2
        // in with the inboard aileron 289.0-370.9 between them, outboard aileron
        // 712.0-895.0. The drawn spoiler panels run from about BL 400 (the oblique inboard
        // edge just outboard of the MAC line BL 397.9; BL 370.9 is the outboard-flap
        // corner) to 697.2. S2 elevator, DC-10-40 row; aileron, 777-200 row. Rudder: the
        // DC-10 row's C_R/C_V (printed "38"); the side view of the Boeing DC-10 Airport
        // Planning Fig. 2.2 draws the rudder at 0.35-0.38 of the fin chord over the full
        // height, which the row's S_R/S_V 0.145 contradicts. S1 flap and spoiler (above); E rest.
        "DC-10" => Surfaces {
            slat: (WIDE_BODY_SLAT, 0.119, 0.95),
            flap: (DC10_FLAP, 0.119, 0.703),
            aileron: (0.22, 0.718, 0.902),
            inboard_aileron: (0.22, 0.291, 0.374),
            spoiler: (DC10_SPOILER, 0.403, 0.703),
            elevator: (0.25, 0.05, 0.95),
            rudder: (0.38, 0.05, 0.95),
        },
        // S2 Fokker 100 rows; E rest.
        "A220-300" => Surfaces {
            slat: (NARROW_BODY_SLAT, 0.11, 0.92),
            flap: (NARROW_BODY_FLAP, 0.105, 0.70),
            aileron: (0.24, 0.70, 0.877),
            inboard_aileron: NO_RUN,
            spoiler: (NARROW_BODY_SPOILER, 0.14, 0.69),
            elevator: (0.32, 0.07, 0.95),
            rudder: (0.28, 0.05, 0.95),
        },
        // S2 Fokker 100 rows; E rest.
        "C919" => Surfaces {
            slat: (NARROW_BODY_SLAT, 0.145, 0.93),
            flap: (NARROW_BODY_FLAP, 0.119, 0.748),
            aileron: (0.24, 0.76, 0.92),
            inboard_aileron: NO_RUN,
            spoiler: (NARROW_BODY_SPOILER, 0.257, 0.729),
            elevator: (0.32, 0.03, 0.97),
            rudder: (0.28, 0.05, 0.95),
        },
        // S2 aileron and elevator, Fokker 100 rows; rudder, ERJ145 row; E rest.
        // Jane's p. 82 is the E1 wing.
        "E195-E2" => Surfaces {
            slat: (NARROW_BODY_SLAT, 0.13, 0.89),
            flap: (NARROW_BODY_FLAP, 0.11, 0.70),
            aileron: (0.24, 0.704, 0.884),
            inboard_aileron: NO_RUN,
            spoiler: (NARROW_BODY_SPOILER, 0.13, 0.69),
            elevator: (0.32, 0.07, 0.95),
            rudder: (0.31, 0.05, 0.95),
        },
        // S1 ailerons 3.75, flaps 12.28, spoilers 1.34 (Jane's ATR 72 entry); elevators
        // 3.92 and rudder 4.00, unchanged from the ATR 42 entry it refers to.
        "ATR72-600" => Surfaces {
            slat: NO_SLAT,
            flap: (0.3079, 0.106, 0.72),
            aileron: (0.3270, 0.72, 0.95),
            inboard_aileron: NO_RUN,
            spoiler: (0.0892, 0.45, 0.70),
            elevator: (0.3693, 0.03, 0.97),
            rudder: (0.3040, 0.06, 0.95),
        },
        // S1 ailerons 3.94 each, flaps 20.24 and spoilers 6.90 per wing, elevators 11.49
        // in total (Jane's p. 298). The printed total stands, although the same entry's
        // three-view draws about 0.29 of the local chord (some 17 m2): an open conflict
        // between two printed figures. S2 rudder, C-130E row.
        "A400M" => Surfaces {
            slat: NO_SLAT,
            flap: (0.2654, 0.10, 0.74),
            aileron: (0.2360, 0.74, 0.98),
            inboard_aileron: NO_RUN,
            spoiler: (0.1439, 0.29, 0.73),
            elevator: (A400M_ELEVATOR, 0.05, 0.95),
            rudder: (0.25, 0.05, 0.95),
        },
        // E stations; S2 aileron, elevator and rudder, Cessna Citation III rows (the
        // business jet nearest in take-off mass); E flap and spoiler.
        "LJ60" => Surfaces {
            slat: NO_SLAT,
            flap: (NARROW_BODY_FLAP, 0.12, 0.68),
            aileron: (0.30, 0.68, 0.94),
            inboard_aileron: NO_RUN,
            spoiler: (NARROW_BODY_SPOILER, 0.30, 0.66),
            elevator: (0.37, 0.06, 0.94),
            rudder: (0.27, 0.10, 0.90),
        },
        // S1 ailerons 3.14, flaps 10.87, elevators 6.17, rudder 4.20, all with tabs: the
        // CN-235 Srs 300 AREAS block, as the C-295 entry prints no areas of its own.
        "C295" => Surfaces {
            slat: NO_SLAT,
            flap: (0.2551, 0.10, 0.72),
            aileron: (0.3681, 0.72, 0.95),
            inboard_aileron: NO_RUN,
            spoiler: NO_SPOILER,
            elevator: (0.3304, 0.05, 0.95),
            rudder: (0.3927, 0.02, 0.98),
        },
        _ => return None,
    })
}

/// The control-surface runs of a registered aircraft; `None` keeps the class
/// default, which the study aircraft and a clean sheet use.
pub fn config_for(preset_name: &str) -> Option<ControlSurfacesConfig> {
    let runs = surfaces(airframe(preset_name))?;
    let defaults = ControlSurfacesConfig::default();
    Some(ControlSurfacesConfig {
        slat_chord_fraction: runs.slat.0,
        slat_span_start_frac: runs.slat.1,
        slat_span_end_frac: runs.slat.2,
        flap_chord_fraction: runs.flap.0,
        flap_span_start_frac: runs.flap.1,
        flap_span_end_frac: runs.flap.2,
        aileron_chord_fraction: runs.aileron.0,
        aileron_span_start_frac: runs.aileron.1,
        aileron_span_end_frac: runs.aileron.2,
        inboard_aileron_chord_fraction: runs.inboard_aileron.0,
        inboard_aileron_span_start_frac: runs.inboard_aileron.1,
        inboard_aileron_span_end_frac: runs.inboard_aileron.2,
        spoiler_chord_fraction: runs.spoiler.0,
        spoiler_span_start_frac: runs.spoiler.1,
        spoiler_span_end_frac: runs.spoiler.2,
        elevator_chord_fraction: runs.elevator.0,
        elevator_span_start_frac: runs.elevator.1,
        elevator_span_end_frac: runs.elevator.2,
        rudder_chord_fraction: runs.rudder.0,
        rudder_span_start_frac: runs.rudder.1,
        rudder_span_end_frac: runs.rudder.2,
        rudder_max_deflection_deg: rudder_travel_deg(airframe(preset_name))
            .unwrap_or(defaults.rudder_max_deflection_deg),
    })
}

/// The type's own airborne rudder travel toward the side that holds the
/// critical engine-out yaw, deg, where its type certificate prints one.
fn rudder_travel_deg(name: &str) -> Option<f64> {
    match name {
        // EASA TCDS EASA.A.186 Issue 08, Section 5 (C-295), Note 5.1, printed p. 83,
        // "Rudder Variables End System Movements" (all serial numbers from 0001): nominal
        // -21 deg / +12.5 deg, flight movement -23 deg / +16.5 deg, ground -25 deg / +18.8 deg.
        // The flight movement of the larger side is the airborne stop (the item 12.2 table,
        // p. 78, gives the earlier -19 deg, -22 deg in expander mode).
        "C295" => Some(23.0),
        _ => None,
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const AIRCRAFT: [&str; 14] = [
        "A320-200",
        "A340-300",
        "A380-800",
        "A350-900",
        "B747-400",
        "B787-9",
        "DC-10",
        "A220-300",
        "C919",
        "E195-E2",
        "ATR72-600",
        "A400M",
        "LJ60",
        "C295",
    ];

    #[test]
    fn every_registered_aircraft_declares_ordered_runs_on_its_surfaces() {
        for name in AIRCRAFT {
            let cs = config_for(name).unwrap();
            for (surface, chord, start, end) in [
                (
                    "slat",
                    cs.slat_chord_fraction,
                    cs.slat_span_start_frac,
                    cs.slat_span_end_frac,
                ),
                (
                    "flap",
                    cs.flap_chord_fraction,
                    cs.flap_span_start_frac,
                    cs.flap_span_end_frac,
                ),
                (
                    "aileron",
                    cs.aileron_chord_fraction,
                    cs.aileron_span_start_frac,
                    cs.aileron_span_end_frac,
                ),
                (
                    "spoiler",
                    cs.spoiler_chord_fraction,
                    cs.spoiler_span_start_frac,
                    cs.spoiler_span_end_frac,
                ),
                (
                    "elevator",
                    cs.elevator_chord_fraction,
                    cs.elevator_span_start_frac,
                    cs.elevator_span_end_frac,
                ),
                (
                    "rudder",
                    cs.rudder_chord_fraction,
                    cs.rudder_span_start_frac,
                    cs.rudder_span_end_frac,
                ),
            ] {
                assert!((0.0..1.0).contains(&chord), "{name} {surface} {chord}");
                assert!(
                    start < end && (0.0..=1.0).contains(&start) && end <= 1.0,
                    "{name} {surface} {start} {end}"
                );
            }
            assert!(
                cs.flap_span_end_frac <= cs.aileron_span_start_frac + 1e-9,
                "{name}"
            );
            assert!(cs.flap_chord_fraction > 0.0, "{name}");
            // No span carries both a flap and the inboard aileron.
            if cs.inboard_aileron_chord_fraction > 0.0 {
                let (a, b) = (
                    cs.inboard_aileron_span_start_frac,
                    cs.inboard_aileron_span_end_frac,
                );
                assert!(a < b && b <= cs.aileron_span_start_frac, "{name}");
                for (s, e) in cs.flap_bands() {
                    assert!(
                        e <= a + 1e-9 || s >= b - 1e-9,
                        "{name} flap {s}-{e} overlaps the inboard aileron {a}-{b}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_conversions_keep_their_parent_surfaces_and_other_aircraft_keep_the_default() {
        assert_eq!(config_for("A320P2F"), config_for("A320-200"));
        assert_eq!(config_for("B747-400F"), config_for("B747-400"));
        assert_eq!(config_for("DC-10-30F"), config_for("DC-10"));
        assert_eq!(config_for("AVE"), None);
    }

    #[test]
    fn slats_and_spoilers_exist_only_where_the_sources_say_so() {
        for name in ["A400M", "ATR72-600", "LJ60", "C295"] {
            assert_eq!(config_for(name).unwrap().slat_chord_fraction, 0.0, "{name}");
        }
        assert_eq!(config_for("C295").unwrap().spoiler_chord_fraction, 0.0);
        assert!(!config_for("C295").unwrap().has_spoilers());
        assert!(config_for("A320-200").unwrap().slat_chord_fraction > 0.0);
        assert!(
            config_for("B747-400")
                .unwrap()
                .inboard_aileron_chord_fraction
                > 0.0
        );
    }
}
