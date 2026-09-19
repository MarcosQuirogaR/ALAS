// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::{FlopsStructureConfig, FlopsTurbopropConfig, PropellerConstruction, PylonMassMethod};

/// Structure and propulsion overrides for registered aircraft.
///
/// NASA/TM-2017-219627 Vol. I defines the engine term `WENGB` as a user
/// input, the weight of the baseline engine, and falls back to the transport
/// correlation `THRSO / 5.5` (equation 76) only when none is input. The
/// pinned Aviary implementation likewise takes the engine mass from the
/// engine deck. A certified dry weight from the engine type-certificate data
/// sheet is that input when its stated scope is the FLOPS engine term: the
/// basic engine with its accessories, with the nacelle, thrust reverser and
/// installation outside it (those are FLOPS equations 69, 86, 87 and 89).
/// [`certified_dry_engine_mass_kg`] declares it only for the engines whose
/// data sheet states that scope; every other preset keeps the equation 76
/// correlation and records the reason. No legacy fraction is inserted here.
pub(super) fn declared_structure(name: &str) -> FlopsStructureConfig {
    let mut config = FlopsStructureConfig::default();
    // No registered aircraft has a source-backed FLOPS FCOMP coefficient. A
    // literal composite percentage would be a different quantity, so retain
    // the published metallic baseline until a calibrated FLOPS coefficient is
    // obtained. This branch keeps the aircraft distinction visible for future
    // evidence without silently pretending it is known today.
    if matches!(name, "A220-300" | "B787-9") {
        config.composite_utilization = 0.0;
    }
    config.baseline_engine_mass_kg = certified_dry_engine_mass_kg(name);
    config.paint_area_density_kg_m2 = declared_paint_area_density_kg_m2(name);
    config.pylon_mass_method = declared_pylon_mass_method(name);
    config
}

/// Exterior paint, FLOPS `WPAINT`, kg per square metre of wetted area.
///
/// FLOPS equation 68 is `WTPNT = WPAINT x SWTWG`, and the published equation
/// set gives no default: zero means an aircraft delivered in bare metal or
/// bare composite, which none of these is. NASA's own validated decks declare
/// it — `large_single_aisle_1_FLOPS.csv` carries
/// `aircraft:paint:mass_per_unit_area, 0.037, lbm/ft**2` and
/// `large_single_aisle_2` carries `0.07` — so the quantity has a source even
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
/// * **AVE, A320-200, A220-300, A340-300, A380-800, B787-9, DC-10-30** —
///   podded wing installations, all above the LTH relation's stated 40 t
///   MTOM floor, so the box-beam relation is evaluated on each aircraft's own
///   sea-level static thrust. The DC-10's tail-mounted centre engine is
///   carried by fin and fuselage structure that the relation was not fitted
///   on, and the relation reads the wing-mounted count, so it is charged two
///   pylons rather than three.
/// * **ATR 72-600 — none.** Its nacelles are faired into the wing rather than
///   pylon-mounted, which is an architecture statement and the same one its
///   shaft-power group already makes with a zero GASP pylon coefficient. It
///   is also a 23 t aircraft, below the LTH relation's own domain.
fn declared_pylon_mass_method(name: &str) -> PylonMassMethod {
    match name {
        "ATR72-600" => PylonMassMethod::None,
        _ => PylonMassMethod::LthBoxBeamV1,
    }
}

/// The certified dry engine mass declared as FLOPS `WENGB`, kg, where the
/// type-certificate data sheet states a scope that matches the FLOPS engine
/// term; `None` keeps the equation 76 correlation.
///
/// Declared:
///
/// * A320-200, CFM56-5B4/3 SAC: 2,454.8 kg, EASA TCDS E.003 Issue 06
///   (2023-01-09) p.11, "including basic engine, its accessories and
///   optional accessories, as well as engine condition monitoring
///   equipment"; the starter is engine type design (inside); the CFM56-5B
///   thrust reverser is aircraft equipment (only the -5C parts list carries
///   a reverser, p.17 note 9).
/// * A220-300, PW1521G-3: 2,177 kg, EASA TCDS IM.E.090 Issue 10
///   (2025-08-14) p.7, "applies to the basic engine and includes standard
///   equipment"; the thrust reverser "is not engine type design" (p.14
///   note 4).
/// * A380-800, Trent 970-84: 6,246 kg, EASA TCDS E.012 Issue 12
///   (2026-03-16) p.9, "Not including fluids and Nacelle EBU".
///
/// Kept on equation 76:
///
/// * A340-300, CFM56-5C3/F: the 2,644.4 kg dry weight (E.003 p.11) includes
///   an adapter kit with mixer, exhaust plug and thrust reverser, which
///   FLOPS prices separately (equation 86); the split is not published.
/// * B787-9, GEnx-1B74/75/P2: 6,147.1 kg (EASA TCDS for the GEnx, p.8) is
///   the basic engine with accessories, but the same data sheet lists the
///   787 fan reversers under the engine type design without stating whether
///   they are inside the dry weight; the scope is unresolved.
/// * DC-10, CF6-50C: no certified dry weight was retained.
/// * AVE (notional) and ATR72-600 (unsupported propulsion technology).
///
/// Known scope residuals of the declared values, recorded rather than
/// adjusted: the engine starter is inside the data-sheet mass and FLOPS
/// equation 89 prices a starting system separately (order 20 kg per engine
/// of overlap); the exhaust nozzle and plug are outside the data-sheet mass
/// and are not a separate FLOPS term on this branch (order 50 kg per engine
/// of omission). Both are far below the correlation's departure from the
/// certified value.
fn certified_dry_engine_mass_kg(name: &str) -> Option<f64> {
    match name {
        "A320-200" => Some(2_454.8),
        "A220-300" => Some(2_177.0),
        "A380-800" => Some(6_246.0),
        _ => None,
    }
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
/// * **Activity factor 130.** *Declared, not sourced.* No public document
///   retrieved states the 568F-1 blade activity factor; the FAA propeller
///   type-certificate data sheet was not reachable. 130 sits mid-band for a
///   modern high-power turboprop blade. The propeller mass scales as
///   `A.F.^0.75`, so the whole 120-160 plausible band moves it by about
///   -7 to +9 percent.
/// * **Nacelle area density 16.96 kg/m^2.** *Calibrated, not sourced.* There
///   is no published shaft-power turboprop nacelle relation at all
///   (GASP equation V.1.6 takes `UW_NAC` as a user input). The value carries
///   the "Engine Section" line of the NASA ATR 42-600 group weight statement
///   (NTRS 20230006542 Table 2: 916 lbf = 415.5 kg for two nacelles) onto
///   this aircraft's resolved nacelle wetted area, 2 x pi x 1.3 m x 3.0 m =
///   24.50 m^2, which is legitimate only because the ATR 42-600 and the
///   ATR 72-600 share the same PW127-series nacelle installation. That NASA
///   statement is a GASP model output calibrated to published top-level data,
///   **not** a measured manufacturer weight statement, and the calibration is
///   to a component line rather than to any total.
/// * **Pylon coefficient zero.** The ATR's nacelles are faired into the wing
///   rather than pylon-mounted, so the GASP pylon relation does not apply.
///   This is an architecture statement, not an omission.
/// * **Engine installation 308 kg for both engines.** *Calibrated, not
///   sourced.* Engine controls, starters, mounts and fire protection, from
///   the "Engine Installation" line of the same NASA ATR 42-600 statement
///   (679 lbf). FLOPS equations 87 and 89 cover the same scope from rated
///   thrust and have no shaft-power form.
/// * **Engine oil 26 kg, propeller accessories 0 kg.** Declared study values;
///   no retrieved source gives the PW127M oil charge or the 568F-1 spinner,
///   de-icing and governor masses. The accessory mass stays at zero so the
///   gap is visible in the ledger rather than filled with a guess.
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
            nacelle_area_density_kg_m2: 16.956,
            pylon_coefficient: 0.0,
            engine_installation_mass_kg: 308.0,
            engine_oil_mass_kg: 26.0,
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
    fn certified_dry_engine_masses_are_declared_only_where_the_scope_is_stated() {
        assert_eq!(
            declared_structure("A320-200").baseline_engine_mass_kg,
            Some(2_454.8)
        );
        assert_eq!(
            declared_structure("A220-300").baseline_engine_mass_kg,
            Some(2_177.0)
        );
        assert_eq!(
            declared_structure("A380-800").baseline_engine_mass_kg,
            Some(6_246.0)
        );
        for name in ["A340-300", "B787-9", "DC-10", "AVE", "ATR72-600"] {
            assert_eq!(
                declared_structure(name).baseline_engine_mass_kg,
                None,
                "{name} keeps the equation 76 correlation"
            );
        }
        // A declared engine mass without a separate inlet or nozzle is the
        // equation 80 branch and validates as such.
        assert!(declared_structure("A320-200").validate().is_ok());
    }
}
