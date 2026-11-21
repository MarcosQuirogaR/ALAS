// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::composite::declared_composite_utilization;
use crate::{
    FlopsNozzleScope, FlopsStarterScope, FlopsStructureConfig, FlopsTurbopropConfig,
    PropellerConstruction, PylonMassMethod,
};

/// Structure and propulsion overrides for registered aircraft.
///
/// NASA/TM-2017-219627 Vol. I defines the engine term `WENGB` as a user
/// input, the weight of the baseline engine, and falls back to the transport
/// correlation `THRSO / 5.5` (equation 76) only when none is input. The
/// pinned Aviary implementation likewise takes the engine mass from the
/// engine deck. [`engine_sheet`] holds that input and the one rule that decides
/// the installation terms next to it. No legacy fraction is inserted here.
pub(super) fn declared_structure(name: &str) -> FlopsStructureConfig {
    let mut config = FlopsStructureConfig {
        composite_utilization: declared_composite_utilization(name),
        ..FlopsStructureConfig::default()
    };
    if let Some(sheet) = engine_sheet(name) {
        config.baseline_engine_mass_kg = Some(sheet.dry_mass_kg);
        config.thrust_reversers_installed = !sheet.weight_line_names_reverser;
        config.thrust_reverser_engine_count = sheet.reversing_engines;
        config.starter_scope = sheet.starter;
        config.nozzle_scope = sheet.nozzle;
    } else {
        config.nozzle_scope = FlopsNozzleScope::IncludedInBaseline;
    }
    config.paint_area_density_kg_m2 = declared_paint_area_density_kg_m2(name);
    config.pylon_mass_method = declared_pylon_mass_method(name);
    config.maximum_operating_altitude_m = certified_maximum_operating_altitude_m(name);
    if let Some((main_m, nose_m)) = printed_gear_leg_lengths_m(name) {
        config.main_gear_oleo_length_m = Some(main_m);
        config.nose_gear_oleo_length_m = Some(nose_m);
    }
    if name == "C295" {
        // S: Jane's 2008-2009 p.545 ramp and freight hold of the C-295M: FLOPS CARGF 1.0.
        config.military_cargo_floor = 1.0;
    }
    config
}

/// The extended main- and nose-gear leg lengths, m, a primary document
/// prints for a registered aircraft: FLOPS `XMLG` and `XNLG`.
///
/// FLOPS reads both as user inputs, the extended oleo length (NASA/TM-2017-
/// 219627 Vol. I, nomenclature), and NASA's own FLOPS decks declare them
/// (NASA Aviary `large_single_aisle_1_FLOPS_data.py` 102/67 in and
/// `large_single_aisle_2_FLOPS_data.py` 84/58.8 in).
/// Equation 66 estimates the main leg only where no length is input; it
/// prices the leg that keeps the outboard wing nacelle clear of the ground,
/// which on a four-engine wing is far longer than the gear actually fitted.
/// The leg is the trunnion pivot to axle centreline, extended; a document
/// that prints only ground clearances is not used, because the pivot height
/// and the static stroke it would need are not printed.
///
/// * **A320-200:** Airbus A320 Aircraft Characteristics (Jun
///   01/24) 2-9-0 p. 5, FIGURE-2-9-0-991-011-A01, main gear pivot to axle
///   extended 2,868.70 mm; p. 9, FIGURE-2-9-0-991-015-A01, nose gear
///   extended 1,957.03 mm.
/// * **B747-400:** S. Chai and W. Mason, NASA CR-205551 (1996),
///   747 sample input pp. 170-174, piston plus cylinder trunnion-to-axle
///   vectors: wing gear 50 + 112 = 162 in, body gear 50 + 64 = 114 in, nose
///   gear 34 + 78 = 112 in. FLOPS has one main leg for the whole main gear,
///   so the main leg is the mean over the four struts, which carry equal
///   wheel counts: (2 x 162 + 2 x 114) / 4 = 138 in.
///
/// Every other aircraft prints no leg and keeps equation 66 (or the
/// fuselage ground datum of a fuselage-mounted gear). The freighter variants
/// built on these presets keep the airliner gear.
pub(super) fn printed_gear_leg_lengths_m(name: &str) -> Option<(f64, f64)> {
    const INCH_M: f64 = 0.0254;
    match name {
        "A320-200" => Some((2.868_70, 1.957_03)),
        "B747-400" => Some((138.0 * INCH_M, 112.0 * INCH_M)),
        _ => None,
    }
}

/// The certified maximum operating altitude of a registered aircraft, m.
///
/// A certification limit (CS 25.1527), so a datum and not a computed
/// quantity. The pressurized fuselage method of the regional turboprop class
/// reads it for its design cabin differential; every aircraft reports it as
/// its datasheet value. Feet are converted at the exact 0.3048 m/ft.
///
/// * **ATR 72-600: 25,000 ft.** JCAB TCDS No. 75 (ATR 42/72), Revision 3,
///   model ATR72-212A, item (18), p. 9.
/// * **C295: 25,000 ft.** EASA TCDS A.186 Issue 08, Section 5 III.11.
/// * **A220-300: 41,000 ft.** EASA TCDS IM.A.570 Issue 25, Section 2 item 11.
/// * **A320-200: 39,100 ft.** EASA TCDS A.064 Issue 62 (the A320 freighter
///   conversion inherits the base limit; none is stated for it).
/// * **A340-300: 41,450 ft** (maximum flight level). EASA TCDS A.015 Issue 28,
///   Section 2 item 10.
/// * **A350-900: 43,100 ft.** EASA TCDS A.151 Issue 32, Section 1 item 2.7.
/// * **A400M: 40,000 ft.** Airbus A400M brochure (military standard), p. 13;
///   the civil type design of EASA TCDS A.169 states 35,000 ft for a different
///   weight variant and is not mixed in (as for the masses).
/// * **B747-400 (and the -400F): 45,100 ft (13,750 m).** EASA TCDS IM.A.196
///   Issue 19, item 10.
/// * **B787-9: 43,100 ft.** EASA TCDS IM.A.115 Issue 30, 787-9 section III.11.
/// * **C919: 39,800 ft (12,131 m).** COMAC data in ICAO working paper
///   ICAO/WP26, aircraft characteristics table (no type certificate data
///   sheet is held for this type).
/// * **E195-E2: 41,000 ft.** EASA TCDS IM.A.071 Issue 29, item 11.
/// * **LJ60: 51,000 ft.** EASA TCDS IM.A.212 Issue 04, p. 6 (special
///   condition "High Operation Altitude 51K").
///
/// * **A380-800: 43,000 ft.** Explanatory Note to EASA TCDS A.110 (Airbus
///   A380) Issue 04, special condition D-31 "High altitude operation",
///   p. 179 (the data sheet itself, Issue 17 item 3.5, refers to the flight
///   manual).
/// * **DC-10 (DC-10-30 and the -30F): 42,000 ft.** FAA TCDS A22WE
///   Revision 13, Model DC-10-30 p. 12 and Model DC-10-30F p. 15.
///
/// No datum is declared for the AVE: it is the app's own design, not a
/// certified aircraft, and keeps its cruise altitude requirement.
fn certified_maximum_operating_altitude_m(name: &str) -> Option<f64> {
    /// Meters per foot, exact (international foot).
    const METERS_PER_FOOT: f64 = 0.3048;
    let feet = match name {
        "ATR72-600" | "C295" => 25_000.0,
        "A220-300" | "E195-E2" => 41_000.0,
        "A320-200" => 39_100.0,
        "A340-300" => 41_450.0,
        "A380-800" => 43_000.0,
        "DC-10" => 42_000.0,
        "A350-900" | "B787-9" => 43_100.0,
        "A400M" => 40_000.0,
        "B747-400" => 45_100.0,
        "C919" => 39_800.0,
        "LJ60" => 51_000.0,
        _ => return None,
    };
    Some(feet * METERS_PER_FOOT)
}

/// Exterior paint, FLOPS `WPAINT`, kg per square metre of wetted area.
///
/// FLOPS equation 68 is `WTPNT = WPAINT x SWTWG`, and the published equation
/// set gives no default: zero means an aircraft delivered in bare metal or
/// bare composite, which none of these is. NASA's own validated decks declare
/// it - `large_single_aisle_1_FLOPS.csv` carries
/// `aircraft:paint:mass_per_unit_area, 0.037, lbm/ft**2` and
/// `large_single_aisle_2` carries `0.07` - so the quantity has a source even
/// though the equation has no default.
///
/// The lower of the two published values is declared here, for every
/// registered aircraft, because a coating system's areal density is a
/// property of the primer/basecoat/clearcoat stack rather than of the
/// airframe it is sprayed on: 0.037 lbm/ft^2 = 0.180 7 kg/m^2. The
/// **uncertainty is the published spread itself**, 0.037 to 0.07 lbm/ft^2,
/// i.e. -0 %/+89 % on every paint mass below; an independent coating-stack
/// estimate lands at 0.13-0.15 kg/m^2, which is inside that band and below
/// the declared value.
///
/// It is applied uniformly and deliberately: no aircraft-specific livery mass
/// was retrieved for any preset, so varying it per aircraft would be fitting.
/// At the declared density the resolved wetted areas give roughly 545 kg
/// (AVE), 745 kg (A380-800), 396 kg (B787-9), 387 kg (A340-300), 374 kg
/// (DC-10), 150 kg (A320-200), 137 kg (A220-300) and 75 kg (ATR 72-600).
fn declared_paint_area_density_kg_m2(_name: &str) -> f64 {
    // 0.037 lbm/ft^2 in SI: 0.037 x 0.453592 37 / 0.092 903 04.
    0.037 * 0.453_592_37 / (0.3048 * 0.3048)
}

/// Which method prices the engine pylons of each registered aircraft.
///
/// The published FLOPS transport equations contain no pylon term, so a podded
/// installation is missing the structure that carries its engines to the
/// wing; see [`crate::PylonMassMethod`] for the source, the derived actual
/// pylon masses and the validity domain.
///
/// * **AVE, A320-200, A220-300, A340-300, A380-800, B787-9, DC-10-30** -
///   podded wing installations, all above the LTH relation's stated 40 t
///   MTOM floor, so the box-beam relation is evaluated on each aircraft's own
///   sea-level static thrust. The DC-10's tail-mounted centre engine is
///   carried by fin and fuselage structure that the relation was not fitted
///   on, and the relation reads the wing-mounted count, so it is charged two
///   pylons rather than three.
/// * **ATR 72-600 - none.** Its nacelles are faired into the wing rather than
///   pylon-mounted, which is an architecture statement and the same one its
///   shaft-power group already makes with a zero GASP pylon coefficient. It
///   is also a 23 t aircraft, below the LTH relation's own domain.
fn declared_pylon_mass_method(name: &str) -> PylonMassMethod {
    match name {
        // C295: nacelles faired into the wing, 23 t, as the ATR 72-600.
        "ATR72-600" | "C295" => PylonMassMethod::None,
        _ => PylonMassMethod::LthBoxBeamV1,
    }
}

/// What the published mass of a preset's engine covers.
struct EngineSheet {
    /// FLOPS `WENGB`, kg.
    dry_mass_kg: f64,
    /// The weight line itself says the reverser is in the figure.
    weight_line_names_reverser: bool,
    /// The engines the record fits a reverser to, where it names the positions;
    /// `None` is every nacelle.
    reversing_engines: Option<usize>,
    /// What the sheet says of the starter.
    starter: FlopsStarterScope,
    /// What the sheet's envelope says of the exhaust.
    nozzle: FlopsNozzleScope,
}

/// The engine record of a preset, decided for every preset by one rule.
///
/// 1. `WENGB` is the data sheet's dry weight of the preset's engine model
///    wherever a sheet is held; failing that, the "engines" line of a weight
///    statement of the same aircraft and engine. Only without either does the
///    equation 76 correlation stand.
/// 2. An installation equation leaves the build-up only when the **weight line
///    names its hardware**. A type-design list, a parts list or a dimension
///    envelope says what is certified together, not what the figure weighs.
/// 3. The sentence "the engine starter is part of the engine type design" is not
///    a weight line, and equation 89 prices the wider starter *system* (TM,
///    "Weight of the engine starter system"), so equation 89 stays; the status
///    records what the sheet says. Removing all of it would be a bound of 0.8 %
///    of OEW on these aircraft, not a result.
/// 4. Equation 86 is priced on the engines the record fits a reverser to. Where
///    a sheet names the engine positions that carry one, that count; otherwise
///    every nacelle. Only the A380 sheet names them: E.012 Issue 12 p.9 fits the
///    Thrust Reverser Unit "at the inboard engine positions" and a Fixed Fan
///    Duct at the outboard ones, so two engines reverse.
///
/// Consequences that read oddly until the rule is applied:
///
/// * A340-300: E.003 p.11 says the CFM56-5C "contain an adapter kit including
///   mixer, exhaust plug, thrust reverser", but its dry-weight note names only
///   the basic engine and accessories, and its length stops at the LP turbine
///   flange. The 189.6 kg above the -5B cannot hold a reverser that equation 86
///   prices at about 500 kg. So 2,644.4 kg stands with equation 86.
/// * C919: E.110 p.8 prints "Without fluids (oil, fuel), including thrust
///   reverser", so equation 86 leaves. The sheet's envelope starts at the fan
///   cowl hinge beam and the LEAP-1A prints about 921 kg less, more than the 451
///   kg of equation 86, so part of the 3,929 kg may be nacelle. The weight line
///   does not say so, and equation 69 stays; removing it all would take OEW
///   from -2.5 % to -4.8 %.
/// * B787-9: IM.E.102 lists the 787 fan reversers in its note 10, but its dry
///   weight (p.8) is the basic engine, accessories and optional equipment.
///
/// Sources, dry weight and its scope:
///
/// * A320-200, CFM56-5B SAC: 2,454.8 kg, EASA TCDS E.003 Issue 06 p.11.
/// * A340-300, CFM56-5C: 2,644.4 kg, the same page.
/// * A220-300 and E195-E2, PW1500G and PW1900G: 2,177 kg, EASA TCDS IM.E.090
///   p.7; the reverser "is not engine of type design" (p.14 note 4).
/// * A380-800, Trent 970: 6,246 kg, EASA TCDS E.012 Issue 12 p.9, "Not including
///   fluids and Nacelle EBU"; TRU and FFD outside the type design.
/// * A350-900, Trent XWB-84: 7,277 kg, EASA TCDS E.111 Issue 20 p.8; the reverser
///   unit is not part of the type design (p.7).
/// * B747-400, CF6-80C2B1F: 4,440.7 kg, EASA TCDS IM.E.240 Issue 02 p.10; basic
///   engine accessories and optional equipment (p.19 note 1).
/// * B787-9, GEnx-1B: 6,147.1 kg, EASA TCDS IM.E.102 Issue 12 p.8.
/// * C919, LEAP-1C G02 and up: 3,929 kg, EASA.E.110 Issue 14 p.8.
/// * LJ60, PW305A: 452.5 kg, EASA TCDS IM.E.051 Issue 04 p.6, excluding fluids and
///   buyer furnished equipment; no reverser in the engine definition (note 5).
/// * DC-10, CF6-50C: no sheet retrieved. 8,721 lb per engine, Roskam, Airplane
///   Design Part V (1985), Table A7.1a p.150, the DC-10-30 statement, which
///   lists exhaust and reverser on their own line.
///
/// The turboprops carry their engines in [`declared_turboprop`] and the A400M
/// record; AVE is notional and keeps the correlation.
fn engine_sheet(name: &str) -> Option<EngineSheet> {
    use FlopsNozzleScope::{IncludedInBaseline, OutsideUnmodelled, Unknown};
    use FlopsStarterScope::{HardwareIncludedSystemUnresolved, UnknownConservativeSeparate};
    let (dry_mass_kg, starter, nozzle) = match name {
        "A320-200" => (2_454.8, HardwareIncludedSystemUnresolved, OutsideUnmodelled),
        "A340-300" => (2_644.4, HardwareIncludedSystemUnresolved, OutsideUnmodelled),
        "A220-300" | "E195-E2" => (2_177.0, UnknownConservativeSeparate, OutsideUnmodelled),
        "A380-800" => (6_246.0, UnknownConservativeSeparate, OutsideUnmodelled),
        "A350-900" => (7_277.0, HardwareIncludedSystemUnresolved, Unknown),
        "B747-400" => (4_440.7, UnknownConservativeSeparate, OutsideUnmodelled),
        "B787-9" => (6_147.1, HardwareIncludedSystemUnresolved, Unknown),
        "C919" => (
            3_929.0,
            HardwareIncludedSystemUnresolved,
            IncludedInBaseline,
        ),
        "LJ60" => (452.5, UnknownConservativeSeparate, Unknown),
        "DC-10" => (
            8_721.0 * 0.453_592_37,
            FlopsStarterScope::SeparateEquation89,
            OutsideUnmodelled,
        ),
        _ => return None,
    };
    Some(EngineSheet {
        dry_mass_kg,
        weight_line_names_reverser: name == "C919",
        reversing_engines: (name == "A380-800").then_some(2),
        starter,
        nozzle,
    })
}

/// Declared shaft-power propulsion-group inputs for registered turboprops.
///
/// FLOPS has no propeller, gearbox or shaft-power mass equation (see
/// [`crate::turboprop_mass`]), so a turboprop's propulsion group is declared
/// here instead of being estimated from a thrust it does not have. Every jet
/// gets the undeclared default, which nothing reads.
///
/// ## The ATR 72-600 record, input by input
///
/// * **Engine dry mass 481.7 kg at 2,475 shp.** EASA TCDS IM.E.041,
///   *Pratt & Whitney Canada PW100 series*, the PW127M dry specification
///   weight. Section III.2 of the same data sheet states that the PW100 type
///   design comprises the three-spool turbomachine **and the reduction
///   gearbox**, which is why `gearbox_inside_engine_mass` is true: charging
///   a separate gearbox would count roughly 200 kg per engine twice.
///   Certification evidence.
/// * **Six blades, composite construction.** The Collins/Hamilton Standard
///   568F-1 is a six-blade composite propeller. Manufacturer data.
/// * **`K_w` 170.** NASA TM-83458 p. 5 states verbatim that "values of `K_w`
///   ranging between 160 to 180 may be assumed for advanced technology
///   fiberglass or composite propellers". 170 is the midpoint of that
///   published band, not a value fitted to any total.
/// * **Activity factor 130 and `K_w = 170`.** *Declared fallback values, not
///   aircraft-specific measurements.* No public document retrieved here
///   states the 568F-1 blade activity factor. The generic regression remains
///   available for configurations without a source-backed propeller assembly
///   mass. The registered ATR uses the JCAB type-certificate sheet's
///   approximate 568F-1 propeller weight instead, so these fallback values do
///   not silently override the aircraft-specific statement.
/// * **Propeller mass 360.9 lb (163.70 kg) each.** *Source-backed
///   approximate value.* JCAB ATR 42/72 TCDS Revision 3, PDF pp. 7-8, names
///   the Hamilton Standard 568F-1 and lists its approximate weight. The input
///   is carried as a source-declared propeller mass override; the separate
///   spinner/de-icing/governor accessory field remains zero because the
///   retained sheet provides neither a separable accessory mass nor an
///   inclusion list. No second allowance is invented.
/// * **Nacelle area coefficient.** *Calibrated, not sourced as a material
///   density.* There is no published shaft-power turboprop nacelle relation
///   (GASP equation V.1.6 takes `UW_NAC` as a user input). The source anchor
///   is explicit: the NASA ATR 42-600 group statement (NTRS 20230006542
///   Table 2, "Engine Section", 916 lbf = 415.5 kg for two nacelles). The
///   runtime reference area is the built ATR nacelle profile's per-nacelle
///   wetted area, 10.543172340348409 m^2, and the reference component mass is
///   207.75 kg. This preserves the source component anchor while applying the
///   coefficient to the same built profile used by the evaluator; it does
///   not claim a measured manufacturer nacelle density or total-aircraft
///   calibration. The original 24.50 m^2 cylinder is source context, not the
///   runtime geometry area.
/// * **Pylon coefficient zero.** The ATR's nacelles are faired into the wing
///   rather than pylon-mounted, so the GASP pylon relation does not apply.
///   This is an architecture statement, not an omission.
/// * **Engine installation 308 kg for both engines.** *Calibrated, not
///   sourced.* Engine controls, starters, mounts and fire protection, from
///   the "Engine Installation" line of the same NASA ATR 42-600 statement
///   (679 lbf). FLOPS equations 87 and 89 cover the same scope from rated
///   thrust and have no shaft-power form.
/// * **Engine oil 46.220 kg for both engines.** *Source-backed installed
///   capacity.* JCAB ATR 42/72 TCDS Revision 3 p. 10 lists 23.110 kg per
///   PW127M engine (23.703 L each), and the p. 12 full-oil note corroborates
///   the aircraft-level convention, so the two-engine input is 46.220 kg.
///   **Propeller accessories remain 0 kg:** the retained source gives no
///   separable spinner, de-icing or governor mass, and the assembly override
///   is not augmented with an invented allowance.
pub(super) fn declared_turboprop(name: &str) -> FlopsTurbopropConfig {
    match name {
        "ATR72-600" => FlopsTurbopropConfig {
            engine_dry_mass_kg: Some(481.7),
            // 2,475 shp expressed in kW with the exact mechanical horsepower.
            baseline_shaft_power_kw: Some(2_475.0 * 745.699_872 / 1_000.0),
            // The NASA GASP turboprop specific weight is linear in power, so
            // an exponent of one is the same relation continued off the
            // certificated point rather than a second, different law.
            engine_mass_scaling_exponent: 1.0,
            gearbox_inside_engine_mass: true,
            propeller_blade_count: 6,
            propeller_activity_factor: 130.0,
            propeller_construction: PropellerConstruction::Composite,
            propeller_weight_coefficient: Some(170.0),
            propeller_accessory_mass_kg: 0.0,
            propeller_assembly_mass_kg: Some(360.9 * 0.453_592_37),
            propeller_assembly_accessories_included: None,
            nacelle_area_density_kg_m2: 415.5 / (2.0 * 10.543_172_340_348_41),
            nacelle_reference_mass_kg: Some(415.5 / 2.0),
            // Per-nacelle area from the configured ATR profile through the
            // same Fuselage::area_wetted integration used by the evaluator.
            nacelle_reference_area_m2: Some(10.543_172_340_348_41),
            pylon_coefficient: 0.0,
            engine_installation_mass_kg: 308.0,
            engine_oil_mass_kg: 46.220,
        },
        "C295" => FlopsTurbopropConfig {
            engine_dry_mass_kg: Some(484.4), // S: EASA IM.E.041 Issue 07 p.9, PW127G
            baseline_shaft_power_kw: Some(1_973.0), // S: same data sheet p.10, normal take-off
            engine_mass_scaling_exponent: 1.0,
            gearbox_inside_engine_mass: true, // S: same data sheet p.9, reduction gearbox in the engine
            propeller_blade_count: 6,         // S: EASA.A.186 Section 5 III.7
            propeller_activity_factor: 130.0, // E: ATR value, unused with an assembly mass
            propeller_construction: PropellerConstruction::Composite, // S: Jane's 2008-2009 p.545
            propeller_weight_coefficient: Some(170.0), // E: ATR value
            propeller_accessory_mass_kg: 0.0,
            // E: ATR 568F-1 assembly mass for the same-family 568F-5; no 568F-5 mass retrieved.
            propeller_assembly_mass_kg: Some(360.9 * 0.453_592_37),
            propeller_assembly_accessories_included: None,
            // E: ATR nacelle area density; no C295 nacelle mass retrieved.
            nacelle_area_density_kg_m2: 415.5 / (2.0 * 10.543_172_340_348_41),
            nacelle_reference_mass_kg: None,
            nacelle_reference_area_m2: None,
            pylon_coefficient: 0.0,
            // E: the ATR ratio, 0.32 x engine dry mass for two engines.
            engine_installation_mass_kg: 0.32 * 2.0 * 484.4,
            // I: EASA.A.186 Section 5 III.8 23.02 L per engine tank at the ATR 0.975 kg/L.
            engine_oil_mass_kg: 2.0 * 23.02 * 0.975,
        },
        _ => FlopsTurbopropConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_atr_turboprop_record_keeps_the_gearbox_inside_the_certificated_mass() {
        let atr = declared_turboprop("ATR72-600");
        assert_eq!(atr.engine_dry_mass_kg, Some(481.7));
        // EASA TCDS IM.E.041 section III.2: the PW100 type design is the
        // turbomachine and the reduction gearbox together.
        assert!(atr.gearbox_inside_engine_mass);
        // NASA TM-83458 p. 5 sanctions 160 to 180 for composite blades.
        assert_eq!(atr.propeller_construction, PropellerConstruction::Composite);
        let Some(coefficient) = atr.resolved_weight_coefficient() else {
            panic!("a composite propeller must carry a declared coefficient");
        };
        assert!((160.0..=180.0).contains(&coefficient), "{coefficient}");
        assert_eq!(atr.validate(), Ok(()));
        // A pylon coefficient of zero is the ATR's wing-faired nacelle
        // architecture, and the jets must not pick the record up at all.
        assert_eq!(atr.pylon_coefficient, 0.0);
        for jet in [
            "A320-200", "A220-300", "A340-300", "A380-800", "B787-9", "DC-10", "AVE",
        ] {
            assert!(
                declared_turboprop(jet).is_default(),
                "{jet} must not carry a shaft-power propulsion record"
            );
        }
    }

    #[test]
    fn one_rule_decides_every_engine_record() {
        for (name, mass_kg) in [
            ("A320-200", 2_454.8),
            ("A340-300", 2_644.4),
            ("A220-300", 2_177.0),
            ("E195-E2", 2_177.0),
            ("A380-800", 6_246.0),
            ("A350-900", 7_277.0),
            ("B747-400", 4_440.7),
            ("B787-9", 6_147.1),
            ("C919", 3_929.0),
            ("LJ60", 452.5),
        ] {
            let structure = declared_structure(name);
            assert_eq!(structure.baseline_engine_mass_kg, Some(mass_kg), "{name}");
            // Only the weight line that names the reverser switches equation 86.
            assert_eq!(
                structure.thrust_reversers_installed,
                name != "C919",
                "{name}"
            );
            // Only the sheet that names the reversing positions limits them.
            let reversing = (name == "A380-800").then_some(2);
            assert_eq!(structure.thrust_reverser_engine_count, reversing, "{name}");
            assert!(structure.validate().is_ok(), "{name}");
        }
        // The starter sentence labels the record for every sheet that prints it
        // and removes equation 89 from none.
        for name in ["A320-200", "A340-300", "A350-900", "B787-9", "C919"] {
            let scope = declared_structure(name).starter_scope;
            assert_eq!(
                scope,
                FlopsStarterScope::HardwareIncludedSystemUnresolved,
                "{name}"
            );
            assert!(scope.includes_equation_89(), "{name}");
        }
        let dc10 = declared_structure("DC-10")
            .baseline_engine_mass_kg
            .unwrap_or(0.0);
        assert!((dc10 - 3_955.8).abs() < 0.1, "{dc10}");
        for name in ["AVE", "ATR72-600"] {
            assert_eq!(
                declared_structure(name).baseline_engine_mass_kg,
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn printed_gear_legs_are_declared_only_where_a_document_prints_them() {
        let a320 = declared_structure("A320-200");
        assert_eq!(a320.main_gear_oleo_length_m, Some(2.868_70));
        assert_eq!(a320.nose_gear_oleo_length_m, Some(1.957_03));
        let b747 = declared_structure("B747-400");
        let main_m = b747.main_gear_oleo_length_m.unwrap_or(0.0);
        assert!((main_m - 3.505_2).abs() < 1e-9, "{main_m}");
        let nose_m = b747.nose_gear_oleo_length_m.unwrap_or(0.0);
        assert!((nose_m - 2.844_8).abs() < 1e-9, "{nose_m}");
        for name in ["A340-300", "A380-800", "DC-10", "B787-9", "AVE", "A400M"] {
            let structure = declared_structure(name);
            assert_eq!(structure.main_gear_oleo_length_m, None, "{name}");
            assert_eq!(structure.nose_gear_oleo_length_m, None, "{name}");
        }
    }

    #[test]
    fn registered_composite_selection_follows_the_construction_table() {
        for (name, fcomp) in [
            ("AVE", 0.125 / 0.30),
            ("A220-300", 0.125 / 0.30),
            ("B787-9", 0.125 / 0.30),
            ("A380-800", 0.25 * 0.125 / 0.30),
            ("A320-200", 0.0),
            ("A340-300", 0.0),
            ("ATR72-600", 0.0),
            ("DC-10", 0.0),
        ] {
            let structure = declared_structure(name);
            assert!(
                (structure.composite_utilization - fcomp).abs() < 1e-12,
                "{name}"
            );
            assert!(structure.validate().is_ok(), "{name}");
        }
    }
}
