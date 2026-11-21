// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-hold extents declared by station for the registered bodies whose
//! airport-planning documents draw their cargo compartments to scale.
//!
//! Frame: metres aft of the nose tip. Each start station is read off the
//! printed compartment drawing at the scale of its own printed dimensions
//! (the drawing's overall fuselage length, cross-checked against the printed
//! compartment lengths), so a start carries about +-0.3 m of read
//! uncertainty. The compartment lengths are the printed ones. Volumes are not
//! declared: the enclosed volume of the built deck is computed over the
//! extent, so the total hold volume stays a prediction that can be compared
//! with the printed usable volumes. Every declared compartment carries the
//! maximum load its type certificate or planning document prints (A220-300,
//! A320, A340-300, A350-900, A380, 787-9); the DC-10 and C919 holds carry
//! none (their planning documents print no compartment limit).
//!
//! The B747-400 (passenger and freighter, one registered body) is declared
//! from the container rows its ACAP prints (see `declared_lower_holds`); it
//! carries no compartment load limit because no document in hand prints one.

use crate::{CargoDeckConfig, HoldCompartmentConfig, HoldDeck};

/// The A320 aft container hold, FR47 to FR59, m: the four containers the AC
/// prints for it at the pitch of the forward hold, whose 4.95 m the AC prints
/// for three.
const A320_AFT_CONTAINER_HOLD_M: f64 = 4.0 * 4.95 / 3.0;

/// Where the DC-10 centre compartment starts, m: four rows of the forward
/// pitch ahead of the standard compartment's drawn end at 39.06 m (see
/// `declared_lower_holds`).
const DC10_CENTRE_START_M: f64 = 39.06 - 4.0 * 1.639;

/// One B747-400 lower container hold, m: eight rows of LD1 at their 61.5 in
/// depth (ACAP D6-58326-1 Rev F section 2.6.2 p.2-33).
const B747_LD1_HOLD_M: f64 = 8.0 * 61.5 * 0.0254;

/// An A220 fuselage station, in, as metres aft of the nose tip: FS 0 lies
/// 168.0 in ahead of the nose (A220 Aircraft Recovery Publication
/// BD500-3AB48-10400-00, J06-20-01 p.14).
fn a220_fs_m(fs_in: f64) -> f64 {
    (fs_in - 168.0) * 0.0254
}

/// Seed a preset's cargo configuration with what its sources print: the
/// declared lower holds and the main-deck cargo door of its freighter.
pub(super) fn seed_cargo_sources(preset: &str, cargo: &mut CargoDeckConfig) {
    if let Some(holds) = declared_lower_holds(preset) {
        cargo.hold_compartments = holds;
    }
    cargo.main_deck_door_clear_height_m = main_deck_door_clear_height_m(preset);
    if let Some(doors) = main_deck_freighter_doors(preset) {
        cargo.main_deck_door_clear_width_m = Some(doors.side_width_m);
        cargo.main_deck_door_station_m = doors.side_station_m;
        if let Some((width_m, height_m)) = doors.nose_door_m {
            cargo.nose_cargo_door_width_m = Some(width_m);
            cargo.nose_cargo_door_height_m = Some(height_m);
        }
    }
    if let Some(pallet) = declared_main_deck_uld(preset) {
        cargo.main_deck_uld = pallet.to_owned();
    }
}

/// The cargo doors of a type's main-deck freighter, as its documents draw them.
struct FreighterDoors {
    /// Clear width of the main-deck side cargo door, m.
    side_width_m: f64,
    /// Door centreline, m aft of the nose, when the document dimensions it.
    side_station_m: Option<f64>,
    /// Width and height of the nose loading door at its break, m.
    nose_door_m: Option<(f64, f64)>,
}

/// The main-deck cargo doors the mass ledger places at their stations
/// (`alas-mass`, `flops_transport::main_deck_freight`).
fn main_deck_freighter_doors(preset: &str) -> Option<FreighterDoors> {
    const INCH: f64 = 0.0254;
    match preset {
        // EFW A320/A321P2F brochure p.2: main deck cargo door 142 in wide.
        // No document in hand dimensions its station.
        "A320-200" => Some(FreighterDoors {
            side_width_m: 142.0 * INCH,
            side_station_m: None,
            nose_door_m: None,
        }),
        // Boeing 747-400 ACAP D6-58326-1 Rev F. Section 2.7.9: main deck
        // side cargo door 134 in (3.40 m) wide, centreline 146 ft 8 in
        // (44.70 m) from the nose. Section 2.7.10: nose cargo door of the
        // 747-400 Freighter, 12 ft 6 in (3.81 m) wide and 11 ft 8 in (3.56 m)
        // high at its break.
        "B747-400" => Some(FreighterDoors {
            side_width_m: 134.0 * INCH,
            side_station_m: Some(44.70),
            nose_door_m: Some((3.81, 3.56)),
        }),
        // DC-10 ACAP section 2.7.2 (Series 10CF, 30CF and 40CF): main cargo
        // loading door 140 in wide, centreline at station 695, 38 ft 0 in
        // (11.58 m) from the nose.
        "DC-10" => Some(FreighterDoors {
            side_width_m: 140.0 * INCH,
            side_station_m: Some(11.58),
            nose_door_m: None,
        }),
        _ => None,
    }
}

/// Height a main-deck load of the type's freighter is built up to, m: the
/// clear height of its main-deck cargo door, which a built-up pallet has to
/// pass, or the lower load contour the source draws for that deck. Used only
/// when the main deck carries freight.
pub fn main_deck_door_clear_height_m(preset: &str) -> Option<f64> {
    match preset {
        // EFW A320/A321P2F brochure p.2: main deck cargo door, 142 in width and
        // 85 in height clearance.
        "A320-200" => Some(85.0 * 0.0254),
        // Boeing 747-400 ACAP D6-58326-1 Rev F section 2.7.9: main deck side
        // cargo door, 120 by 134 in (3.05 by 3.40 m) clear rectangular opening.
        "B747-400" => Some(120.0 * 0.0254),
        // DC-10 ACAP section 2.7.2, main cargo loading door at station 695
        // (Series 10CF, 30CF and 40CF): 140 in wide, 102 in (259.1 cm) high.
        // The main-deck load contour is lower: Boeing, McDonnell Douglas
        // DC-10-30 Freighter cross section TBC-CA-2067 (2007 Freighter
        // Reference Guide p.10) draws the main-deck loads 244 cm (96 in) high,
        // two abreast, with the upper outboard corner following the lining.
        "DC-10" => Some(96.0 * 0.0254),
        _ => None,
    }
}

/// Whether the type's main-deck freighter is a passenger-to-freighter
/// conversion rather than a production freighter.
pub(super) fn main_deck_freighter_is_conversion(preset: &str) -> bool {
    // A320-200: the A320P2F is a conversion of passenger aircraft (EFW
    // A320/A321P2F brochure; Jane's All the World's Aircraft 2008-2009, A320
    // family entry: reinforced centre-fuselage frames and floor). The
    // B747-400F (Jane's, 747-400 entry: built on the short-upper-deck
    // fuselage) and the DC-10-30F (DC-10 ACAP section 2.7.2, Series 30CF)
    // left the production line as freighters.
    preset == "A320-200"
}

fn rows_hold(name: &str, x_start_m: f64, length_m: f64, rows: u32) -> HoldCompartmentConfig {
    HoldCompartmentConfig {
        container_rows: Some(rows),
        ..hold(name, x_start_m, length_m)
    }
}

/// `compartment` with the maximum load its type certificate prints, kg.
fn limited(compartment: HoldCompartmentConfig, max_net_kg: f64) -> HoldCompartmentConfig {
    HoldCompartmentConfig {
        max_net_kg: Some(max_net_kg),
        ..compartment
    }
}

fn hold(name: &str, x_start_m: f64, length_m: f64) -> HoldCompartmentConfig {
    HoldCompartmentConfig {
        name: name.to_owned(),
        x_start_m,
        x_end_m: x_start_m + length_m,
        volume_m3: None,
        max_net_kg: None,
        deck: HoldDeck::Lower,
        container_rows: None,
        behind_seats: false,
        clear_width_m: None,
        narrow_forward_part_m: None,
    }
}

/// `compartment` with the clear width its drawing shows, m.
fn within(compartment: HoldCompartmentConfig, clear_width_m: f64) -> HoldCompartmentConfig {
    HoldCompartmentConfig {
        clear_width_m: Some(clear_width_m),
        ..compartment
    }
}

/// The declared lower holds of a registered preset, if its sources draw them.
pub fn declared_lower_holds(preset: &str) -> Option<Vec<HoldCompartmentConfig>> {
    match preset {
        // Boeing 747-400 Airport Planning ACAP D6-58326-1 Rev F: section 2.6.1
        // (passenger, p.2-32) and 2.6.3 (freighter, p.2-34) print, for the
        // forward and for the aft lower hold, "8 full-width or 16 half-width
        // (LD1)" containers: eight rows, two LD1 abreast. An LD1 is 61.5 in
        // (1.562 m) deep, so each hold is eight rows of that pitch
        // (`B747_LD1_HOLD_M`, 12.50 m; a hold scaled to 12.27 m from the
        // drawing cannot stand eight LD1). The holds are centred on the
        // positions scaled on the lower cargo door centrelines the drawings of
        // pp.2-40 and 2-41 dimension (13.34 m forward, 45.09 m aft): forward
        // centre 16.435 m, aft 41.935 m, each hold containing its door (about
        // +-0.3 m of read uncertainty; the freighter drawing puts the aft hold
        // about 0.6 m further forward, inside that uncertainty). The bulk
        // compartment follows the aft hold to 52.6 m; its printed volume is
        // 835 cu ft (23.6 m3) on the passenger and 520 cu ft (14.7 m3) on the
        // freighter, which one registered body cannot tell apart, so the
        // volume is left to the built deck. No document in hand prints
        // compartment load limits.
        "B747-400" => Some(vec![
            rows_hold(
                "Forward hold",
                16.435 - 0.5 * B747_LD1_HOLD_M,
                B747_LD1_HOLD_M,
                8,
            ),
            rows_hold(
                "Aft hold",
                41.935 - 0.5 * B747_LD1_HOLD_M,
                B747_LD1_HOLD_M,
                8,
            ),
            hold(
                "Bulk hold",
                41.935 + 0.5 * B747_LD1_HOLD_M,
                52.60 - (41.935 + 0.5 * B747_LD1_HOLD_M),
            ),
        ]),
        // Airbus A340-200/-300 Aircraft Characteristics, 1 Dec 2025, PDF p.61 (FIGURE-2-6-1-991-005-A01, A340-300
        // row): forward 15.02 m, aft 12.32 m, bulk 4.04 m; starts scaled on
        // the 63.66 m drawn fuselage. Door centres p.68: forward cargo door
        // 9.53 m, aft cargo door 44.36 m, bulk door 47.35 m, each inside its
        // compartment as drawn here. Maximum loads: EASA TCDS EASA.A.015
        // Issue 28 (A340-300) item 19, forward 22,861 kg, aft 18,507 kg, rear
        // (bulk) 3,468 kg.
        "A340-300" => Some(vec![
            limited(hold("Forward hold", 8.41, 15.02), 22_861.0),
            limited(hold("Aft hold", 34.44, 12.32), 18_507.0),
            limited(hold("Bulk hold", 46.90, 4.04), 3_468.0),
        ]),
        // Airbus A350 Aircraft Characteristics, FIGURE-2-6-0-991-002-C01
        // (A350-900, vector plan drawn nose down): forward 16.22 m (FR19 to
        // FR44), aft 13.58 m (FR61 to FR82), bulk 1.91 m (FR82 to FR85);
        // starts scaled on the printed forward and aft dimension lines (5.890
        // and 5.870 pt/m, mean 5.880; the 66.80 m overall length drawn in the
        // side view gives 5.897), nose tip at 564.69 pt: forward 518.52 pt ->
        // 7.85 m, aft 358.18 pt -> 35.12 m. Cross-checks: the drawn cargo
        // doors centre at 11.67 and 47.24 m, against 11.68 and 47.30 m printed
        // in FIGURE-2-7-0-991-001-A01; FR44 to FR61 at the 0.648 m frame pitch
        // of the printed lengths puts FR61 at 35.09 m. The loading combinations
        // (sheet 2) print 20 LD3 forward and 16 aft, two abreast. Maximum
        // loads: EASA TCDS EASA.A.151 Issue 33, section 1 (A350-900) III.2.6,
        // forward 22,000 kg (basic), aft 19,000 kg, rear (bulk) 3,468 kg.
        "A350-900" => Some(vec![
            limited(rows_hold("Forward hold", 7.85, 16.22, 10), 22_000.0),
            limited(rows_hold("Aft hold", 35.12, 13.58, 8), 19_000.0),
            limited(hold("Bulk hold", 35.12 + 13.58, 1.91), 3_468.0),
        ]),
        // Airbus A380 Aircraft Characteristics, 1 Dec 2025, PDF p.52 (FIGURE-2-6-0-991-001-A01):
        // forward 17.4 m (FR17 to FR44), forward-aft T-shaped 9.7 m and rear
        // aft 8.2 m (FR57 to FR86, 17.9 m together), bulk 2.8 m (FR86 to
        // FR90); starts scaled on the 72.72 m drawn fuselage. The 8 m between
        // FR44 and FR57 is the wing-box and gear zone. Maximum loads: EASA
        // TCDS EASA.A.110 Issue 17 III.3.10, forward 28,577 kg, aft 20,310 kg,
        // bulk 2,515 kg. Loading combinations, AC Dec 01/23 2-6-0 Page 3
        // (FIGURE-2-6-0-991-002-A01): standard 20 LD3 forward (22 optional),
        // 10 rows two abreast, and 16 LD3 aft: the T-shaped forward aft
        // compartment (9.7 m) takes its LD3 in single file, six of them, and
        // the rear aft compartment (8.2 m) five rows two abreast, 11 rows.
        // Clear widths, scaled across both vector plans on the 7.14 m
        // fuselage they draw: the compartments 4.20 m (2-6-0 p.2, 26.6 of
        // 45.2 pt) and 4.13 m (p.3, 31.3 of 54.1 pt), mean 4.15 m; the
        // T-shaped part 2.50 m (15.8 pt) and 2.44 m (18.5 pt), mean 2.47 m
        // (+-0.08 m, half a point of read). The bulk hold keeps the built
        // deck (its drawn outline tapers with the tailcone).
        "A380-800" => Some(vec![
            limited(
                within(rows_hold("Forward hold", 7.77, 17.4, 10), 4.15),
                28_577.0,
            ),
            limited(
                HoldCompartmentConfig {
                    narrow_forward_part_m: Some([9.7, 2.47]),
                    ..within(rows_hold("Aft hold", 33.60, 17.9, 11), 4.15)
                },
                20_310.0,
            ),
            limited(hold("Bulk hold", 51.50, 2.8), 2_515.0),
        ]),
        // Boeing 787 Airplane Characteristics for Airport Planning, 2025, PDF p.30 (section 2.6.2, 787-9): forward
        // 51 ft 6 in (15.72 m) with 20 LD-3, aft 49 ft 9 in (15.16 m) with 16 LD-3
        // and the 11.4 m3 bulk compartment at its end. Two LD-3 stand across, so the
        // forward hold is 10 rows at a pitch of 15.72 / 10 = 1.572 m and the 16 aft
        // containers are 8 rows at that pitch (12.576 m), the rest of the aft hold
        // (2.58 m) being the bulk compartment. Starts are scaled on the printed
        // dimension lines of the raster drawing (nose tip at 182.3 px, forward
        // compartment 274.8-471.9 px, aft 596.1-783.4 px, 12.54 and 12.36 px/m;
        // mean scale 12.45 px/m): forward 7.43 m, aft 33.23 m. Door stations p.32:
        // forward cargo door 11.00 m, aft cargo door 43.31 m. Maximum loads:
        // EASA TCDS IM.A.115 Issue 30, section 2 (787-9) III.21, forward
        // 32,005 kg, aft 25,655 kg, bulk 2,735 kg.
        "B787-9" => Some(vec![
            limited(rows_hold("Forward hold", 7.43, 15.72, 10), 32_005.0),
            limited(rows_hold("Aft hold", 33.23, 12.576, 8), 25_655.0),
            limited(hold("Bulk hold", 45.806, 2.584), 2_735.0),
        ]),
        // Boeing DC-10 ACAP section 2.6.3 (doc p.27, upper galley, standard
        // centre compartment, Series 10, 10CF, 30, 30CF, 40 and 40CF): 16 LD3
        // forward (8 rows, two abreast), 8 LD3 in the centre compartment (4
        // rows) and the bulk compartment behind it. The drawing prints no
        // length; it is scaled on the two cargo door stations of section 2.7.2
        // (forward door STA 949, 18.03 m aft of the nose, doc p.35; centre door
        // STA 1744.5, 38.24 m, doc p.37), which the drawn doors centre on
        // (0.02644 m per drawn unit; the drawn 104 in forward door then
        // measures 2.62 m). Forward compartment 8.84 to 21.96 m (pitch
        // 1.639 m); the standard centre compartment ends at 39.06 m where the
        // bulk compartment starts, and its four rows take the forward pitch
        // (the 6.13 m drawn would not hold four 61.5 in LD3 bases); the bulk
        // compartment ends at the drawn 44.40 m.
        // The preset is a Series 30, which has the extended centre
        // compartment of section 2.6.2 (doc p.26, Series 30, 30CF, 40 and
        // 40CF): 16 + 10 LD3, bulk 510 cu ft against the standard 805 cu ft,
        // the same compartments with one LD3 row moved from the bulk
        // compartment into the centre one. Section 2.1 (doc p.4) prints the
        // Series 30 lower-deck volume as 4,618 cu ft, the extended total of
        // 2.6.2 (the standard is 21 cu ft less), and the Boeing DC-10-30
        // Freighters brochure TBC-PS-2038 (2007) p.8 prints the -30F lower
        // hold as five pallets forward and five LD-6/-10 (ten LD3 positions)
        // aft with the 510 cu ft (14.4 m3) bulk compartment. So the centre
        // compartment takes five rows at the forward pitch, to 40.70 m, and
        // the bulk compartment the rest of the drawn length. The centre
        // compartment takes the aft-hold identifiers.
        "DC-10" => Some(vec![
            rows_hold("Forward hold", 8.84, 8.0 * 1.639, 8),
            rows_hold("Aft hold", DC10_CENTRE_START_M, 5.0 * 1.639, 5),
            hold(
                "Bulk hold",
                DC10_CENTRE_START_M + 5.0 * 1.639,
                44.40 - (DC10_CENTRE_START_M + 5.0 * 1.639),
            ),
        ]),
        // Airbus A220 Aircraft Characteristics Publication (ACP), issue 013
        // (BD500-3AB48-13800-00), DM BD500-A-J00-00-00-12AAB-030A-A
        // (A220-300), fig. 8 sheet 1 (PDF p.128): forward compartment
        // FS391.60 to FS668.50 (276.90 in), aft compartment FS995.47 to
        // FS1324.95 (329.48 in). FS 0 lies 168.0 in ahead of the nose tip
        // (A220 Aircraft Recovery Publication BD500-3AB48-10400-00, J06-20-01
        // p.14, as for the CG datum in a220.rs), so x = (FS - 168.0) x
        // 0.0254 m. Cross-check, fig. 11 (PDF p.138): the forward cargo door
        // 6.5 m and the aft cargo door 24.86 m from the nose, each inside its
        // compartment. Both are loose-bulk Class C compartments (DM
        // J26-00-00-00AAA, PDF p.642). Maximum loads, section 6 table 9
        // (PDF p.127): forward 2,446 kg, aft 2,606 kg (combined 5,052 kg).
        "A220-300" => Some(vec![
            limited(
                hold(
                    "Forward hold",
                    a220_fs_m(391.60),
                    a220_fs_m(668.50) - a220_fs_m(391.60),
                ),
                2_446.0,
            ),
            limited(
                hold(
                    "Aft hold",
                    a220_fs_m(995.47),
                    a220_fs_m(1324.95) - a220_fs_m(995.47),
                ),
                2_606.0,
            ),
        ]),
        // AC A320 FIGURE-2-6-0-991-003-A01 (vector plan drawn nose down):
        // forward 4.95 m (FR24A to FR34), aft and bulk together 9.8 m (FR47 to
        // FR65); starts scaled on the 37.57 m drawn fuselage (379.0 pt,
        // 10.09 pt/m), nose at 563.2 pt: forward compartment 488.1 -> 7.44 m,
        // aft 372.8 -> 18.87 m. The AC prints no frame-station table and no
        // length for the FR59 bulkhead between the aft container hold (FR47
        // to FR59) and the bulk hold (FR59 to FR65). The container hold holds
        // the four containers the AC prints for it (FIGURE-2-6-0-991-006-A01)
        // at the forward hold's pitch (4.95 m for three): 6.60 m, the bulk
        // hold the remaining 3.20 m of the 9.8 m. The FR59 leader line of the
        // schematic would leave 5.905 m, where four 1.56 m containers do not
        // fit. The 0.86 m printed on the same figure is the bulk cargo door,
        // not a compartment.
        // Maximum loads: EASA TCDS EASA.A.064 Issue 62 (A320) item 21,
        // forward 3,402 kg, aft 4,536 kg, rear (bulk) 1,497 kg.
        "A320-200" => Some(vec![
            limited(hold("Forward hold", 7.44, 4.95), 3_402.0),
            limited(hold("Aft hold", 18.87, A320_AFT_CONTAINER_HOLD_M), 4_536.0),
            limited(
                hold(
                    "Bulk hold",
                    18.87 + A320_AFT_CONTAINER_HOLD_M,
                    9.80 - A320_AFT_CONTAINER_HOLD_M,
                ),
                1_497.0,
            ),
        ]),
        // COMAC C919 ACAP C919-SVV19-50009-00 Issue 002 section 02-06 figure 1
        // (PDF p.69, vector side view): forward compartment FR24 STA11001 to
        // FR35 STA16831 (5.83 m printed), aft FR49 STA23884 (6.36 m printed)
        // and bulk 2.65 m printed to FR66 STA32894 (STA in mm). The drawn
        // fuselage, 346.94 pt nose to tail, is 38.9 m (8.919 pt/m; the FR24 to
        // FR66 lines give 8.915): FR24 lies 65.61 pt = 7.36 m aft of the nose,
        // so x = (STA - 3643) / 1000 m. Table 1 (PDF p.70) prints 18.1 m3
        // forward and 27.1 m3 aft with the bulk; it prints no compartment load
        // limit.
        "C919" => Some(vec![
            hold("Forward hold", 7.36, 5.83),
            hold("Aft hold", 7.36 + 12.883, 6.36),
            hold("Bulk hold", 7.36 + 12.883 + 6.36, 2.65),
        ]),
        _ => None,
    }
}

/// The lower-deck container a registered type's airport-planning document
/// prints, where it prints one. The loader would otherwise choose by capacity
/// among the catalogue, and a narrowbody-only unit could end up in a
/// widebody hold.
///
/// - B747-400: LD1 half-width containers, ACAP D6-58326-1 Rev F (Dec 2024)
///   section 2.6.1-2.6.2 p.2-32 to 2-33.
/// - A340-300: LD3, A340-200/-300 AC Dec 01/25 section 2-6-1 p.3.
/// - A380-800: LD3, A380 AC section 2-6 (20 + 16 LD3).
/// - B787-9: LD-3 at 158 cu ft, 787 ACAP D6-58333 Rev O p.2-14.
/// - A350-900: LD3, A350 AC Rev Feb 01/26 section 2-6-0 (PDF p.94),
///   "STANDARD: 20 LD3 CONTAINERS" forward and 16 aft; its usable volumes
///   (2-1-0, PDF p.40-42) are printed on the LD3 basis.
/// - DC-10: LD3, DC-10 ACAP sections 2.6.2 and 2.6.3 (doc pp.26-27), 158 cu
///   ft half-width containers.
///
/// Not declared: the C919 (ACAP C919-SVV19-50009-00 Issue 002 section 02-06
/// prints compartment capacities only, no ULD type) and AVE.
pub fn declared_lower_uld(preset: &str) -> Option<&'static str> {
    match preset {
        "B747-400" => Some("LD1"),
        "A340-300" | "A380-800" | "B787-9" | "A350-900" | "DC-10" => Some("LD3"),
        _ => None,
    }
}

/// The main-deck pallet a registered type's freighter source prints for its
/// headline arrangement, used when the main deck carries freight and the
/// configuration names none (`AUTO`).
///
/// - DC-10: 30 pallets of 88 by 108 in, Boeing "DC-10-30 Freighters"
///   TBC-PS-2038 (2007) p.8 performance summary, "Main deck, (30) 2.2 x 2.7-m
///   (88 x 108-in) pallet/container cargo capacity, 346.5 m3"; the
///   alternative 22 pallets of 88 by 125 in is drawn on TBC-CA-2066 p.11.
///
/// Not declared: the 747-400F and the A320P2F, whose automatic choice is
/// already the pallet their sources print.
pub fn declared_main_deck_uld(preset: &str) -> Option<&'static str> {
    match preset {
        "DC-10" => Some("P88x108"),
        _ => None,
    }
}

/// The lower-deck container a loose-bulk preset is offered with as an option,
/// where its airport-planning document prints one.
///
/// - A320-200: LD3-45W in three forward and four aft positions, Airbus A320
///   Aircraft Characteristics section 2-6-0 pp2-3 (the as-delivered hold of the
///   preset is loose-bulk).
///
/// Not declared, so the capability is zero: the A220-300, E195-E2, LJ60,
/// ATR72-600 and C295 (no container option read in a source document; the
/// preset's own loose-bulk loading is all there is). Containerized presets
/// need no entry: their installed positions are their capability.
pub fn optional_container_lower_uld(preset: &str) -> Option<&'static str> {
    match preset {
        "A320-200" => Some("LD3-45"),
        _ => None,
    }
}

/// Where the loadable interior of a preset stops keeping the cabin keel, m:
/// the aft end of its last declared container hold. A container hold is a
/// flat structural floor on the cargo loading system's rails (Airbus A320
/// Aircraft Characteristics section 2-6-0 pp2-3: the compartments are
/// stationed between frames and the 3 + 4 container positions stand in them),
/// so the outer belly line rising through the tail upsweep does not lift it
/// inside the compartment. The loft's belly rises from its own station and
/// crossed the A320's last container position, lifting the floor 49 mm under
/// it and leaving 1.123 m under a 1.14 m LD3-45. Only the container holds
/// count: the bulk compartment behind them follows the belly. `None` for a
/// preset that declares no container hold, and for one whose fuselage already
/// declares its own end.
pub(super) fn declared_hold_interior_end_x_m(preset: &str) -> Option<f64> {
    // A loose-bulk type with no container fit (the A220-300) has no
    // loading-system floor: its aft compartment narrows and shallows with the
    // belly towards its end (ACP fig. 8 sections A-A 2.24 by 1.05 m, B-B 2.00
    // by 0.74 m), so its holds follow the loft.
    if crate::preset_flops::declared_cargo_loading(preset) == crate::CargoHoldLoading::Bulk
        && optional_container_lower_uld(preset).is_none()
    {
        return None;
    }
    declared_lower_holds(preset)?
        .iter()
        .filter(|hold| !hold.name.to_ascii_lowercase().contains("bulk"))
        .map(|hold| hold.x_end_m)
        .reduce(f64::max)
}
