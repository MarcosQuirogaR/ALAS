// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aircraft-specific landing lift inputs recovered from published speeds.
//!
//! These are effective gross-reference-area coefficients under the requested
//! `Vref = 1.23 VS1g` convention, not a measurement of the aircraft's trimmed
//! aerodynamic maximum. IAS/CAS, rounding, reference-area and configuration
//! differences remain in the source applicability record. In particular, a
//! source's maximum landing mass must not be replaced by a preset's different
//! weight variant when recovering its coefficient.

use super::AircraftPreset;

const STANDARD_GRAVITY_M_S2: f64 = alas_atmo::STANDARD_GRAVITY;
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const KNOT_M_S: f64 = 1852.0 / 3600.0;
const POUND_KG: f64 = 0.453_592_37;
const VREF_VS1G: f64 = 1.23;
const SQUARE_FOOT_M2: f64 = 0.304_8 * 0.304_8;
/// Reference speed over the minimum-speed stall of the FAR 25 basis before the 1-g
/// stall (14 CFR 25.125 before Amendment 25-108).
const FAR_VREF_OVER_VS: f64 = 1.3;
/// Wing area of the CN-235 and of the C-295, m^2 (EASA TCDS A.186, Issue 08).
const CN235_C295_WING_AREA_M2: f64 = 60.0;
/// Mass of the CN-235-300 flight-manual stall speeds below, kg (34,830 lb).
pub const CN235_300_STALL_MASS_KG: f64 = 34_830.0 * POUND_KG;
/// CN-235-300 stall speed in the landing configuration, flaps 23 deg, knots.
pub const CN235_300_LANDING_STALL_KT: f64 = 79.0;
/// CN-235-300 stall speed at the take-off flap, 10 deg, knots (the same table; read by the
/// validation test of the take-off share and of the class clean value).
#[cfg(test)]
pub const CN235_300_TAKEOFF_STALL_KT: f64 = 86.0;
/// CN-235-300 stall speed with flaps up, knots (the same table; read by the
/// validation test of the take-off share and of the class clean value).
#[cfg(test)]
pub const CN235_300_CLEAN_STALL_KT: f64 = 99.0;

/// Boeing 747-400 wing reference area, m^2: 5,650 ft^2, "Wing Reference
/// Area" of FAA DOT/FAA/AR-04/44, *Statistical Data for the Boeing-747-400
/// Aircraft in Commercial Operations* (Jan 2005), Table 1 (document p.2).
/// It is the reference area (524.90 m^2), the same definition the
/// registered reference wing, the maximum-wing-area requirement and the
/// landing-coefficient recovery use. The ACAP D6-58326-1 prints no wing area.
pub const B747_400_REFERENCE_AREA_M2: f64 = 5_650.0 * SQUARE_FOOT_M2;

/// A published aircraft landing-speed input and its exact recovery condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedLandingReference {
    /// Registered aircraft to which the effective coefficient is applied.
    pub preset_name: &'static str,
    /// Published landing mass associated with the speed recovery, kg.
    pub mass_kg: f64,
    /// Reference area used to express the lift coefficient, m^2.
    pub reference_area_m2: f64,
    /// Published reference/threshold approach speed, m/s.
    pub vref_m_s: f64,
    /// Density defining the sea-level equivalent stall-speed recovery, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed read/rounding uncertainty only, m/s; applicability gaps are separate.
    pub speed_uncertainty_m_s: f64,
    /// Revision-locked speed, mass and reference-area provenance.
    pub source: &'static str,
    /// Configuration, mass and airspeed-convention limitations.
    pub applicability: &'static str,
}

impl PublishedLandingReference {
    /// Recover the effective maximum lift coefficient from `Vref = 1.23 VS1g`.
    ///
    /// The records are finite, positive source inputs at ISA sea level. This
    /// coefficient is mass-independent when later used at the preset's MLW:
    /// the resulting speed scales with the square root of landing mass.
    pub fn cl_max_land(&self) -> f64 {
        let stall_speed_m_s = self.vref_m_s / VREF_VS1G;
        2.0 * self.mass_kg * STANDARD_GRAVITY_M_S2
            / (self.density_kg_m3 * self.reference_area_m2 * stall_speed_m_s.powi(2))
    }
}

/// Published landing-speed records; the synthetic AVE has no aircraft source.
pub fn published_landing_references() -> &'static [PublishedLandingReference] {
    &REFERENCES
}

/// Source input for one real-aircraft preset, when a published speed exists.
pub fn published_landing_reference(name: &str) -> Option<&'static PublishedLandingReference> {
    REFERENCES
        .iter()
        .find(|reference| reference.preset_name == name)
}

pub(super) fn apply(preset: &mut AircraftPreset) {
    if let Some(reference) = published_landing_reference(preset.name) {
        let performance = preset.performance.get_or_insert_with(Default::default);
        performance.cl_max_land = reference.cl_max_land();
        performance.cl_max_land_source = format!(
            "{}; CLmax_land recovered as 2*m*g/(rho*S*(Vref/1.23)^2), rho={} kg/m^3; {}",
            reference.source, reference.density_kg_m3, reference.applicability
        );
        performance.vapp_vstall_land_factor = VREF_VS1G;
    }
}

// old 9 -> 11 published landing references: A350-900 and Learjet 60 appended;
// 11 -> 12: E195-E2 from the FAA Aircraft Characteristics Database.
// 12 -> 13: C295 from the flight-manual stall speeds of the CN-235-300, the same wing
// and landing flap on the same type certificate (the sibling rule of the record).
const REFERENCES: [PublishedLandingReference; 13] = [
    PublishedLandingReference {
        preset_name: "A220-300",
        mass_kg: 129_500.0 * POUND_KG,
        reference_area_m2: 112.3,
        vref_m_s: 131.3 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Airbus A220 ACP Issue 013, 27 Nov 2025, BD500-A-J00-00-00-13AAB-030A-A, Fig.7 p.12 (6 Dec 2024): altitude 0 curve, Vref 131.3 kt at 129,500 lb MLW, chart-read uncertainty +-1 kt; BD500-A-J00-00-00-12AAB-030A-A Table 2: reference area 112.3 m^2; https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/A220-ACP-Issue013-00-27Nov2025.pdf",
        applicability: "A220-300; source speed uses the preset's legacy MLW, rather than the earlier 123,000 lb chart read; sea-level reference-speed convention treated as CAS",
    },
    PublishedLandingReference {
        preset_name: "A320-200",
        mass_kg: 66_000.0,
        reference_area_m2: 122.6,
        vref_m_s: 136.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Airbus A320 Aircraft Characteristics Rev 46, 1 Jul 2026, section 3-5-0 p.1: threshold IAS 136 kt at MLW 66,000 kg, maximum certified flap, standard atmosphere; https://mediaassets.airbus.com/pm_38_916_916266-iujedqawwy.pdf?fileName=aca32001-jul-2026-2.pdf; existing 122.6 m^2 gross-area input supported by secondary Flugzeuginfo A320 technical data, https://www.flugzeuginfo.net/acdata_php/acdata_a320_en.php",
        applicability: "WV017 mass matches; planning IAS treated as sea-level CAS, rounded whole-knot speed; Airbus marks the speed for information only",
    },
    PublishedLandingReference {
        preset_name: "A340-300",
        mass_kg: 192_000.0,
        reference_area_m2: 361.6,
        vref_m_s: 138.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Airbus A340-200/-300 Aircraft Characteristics, 1 Dec 2025, section 3-5-0 p.1: A340-300 threshold IAS 138 kt at MLW 192,000 kg, maximum certified flap, standard atmosphere; https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/AC_A340-200-300_20251201.pdf; existing declared gross reference area 361.6 m^2 retained",
        applicability: "Family high-MLW source, not WV029 MLW 188,000 kg; coefficient transferred at unchanged area/flap configuration, speed scales with sqrt(mass); planning IAS treated as sea-level CAS; the existing reference-area input lacks a primary area source",
    },
    PublishedLandingReference {
        preset_name: "A380-800",
        mass_kg: 395_000.0,
        reference_area_m2: 845.0,
        vref_m_s: 138.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Airbus A380 Aircraft Characteristics, 1 Dec 2025, section 3-5-0 p.1: threshold IAS 138 kt at MLW 395,000 kg, maximum certified flap, standard atmosphere; https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/AC_A380_20251201.pdf; Airbus A380 Facts and Figures, Feb 2022: gross reference area 845 m^2",
        applicability: "Family high-MLW source, not WV000 MLW 386,000 kg; coefficient transferred at unchanged area/flap configuration, speed scales with sqrt(mass); planning IAS treated as sea-level CAS",
    },
    PublishedLandingReference {
        preset_name: "ATR72-600",
        mass_kg: 22_350.0,
        reference_area_m2: 61.0,
        vref_m_s: 113.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "ATR 72-600 PW127M/N Factsheet, 2020, p.2: reference speed at landing 113 KIAS, basic MLW 22,350 kg, reference wing area 61 m^2; https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf",
        applicability: "Reference speed grouped with the basic-MLW sea-level landing performance; source does not explicitly repeat its speed mass or IAS correction, so basic MLW and IAS=CAS are declared conceptual assumptions",
    },
    PublishedLandingReference {
        preset_name: "B787-9",
        mass_kg: 425_000.0 * POUND_KG,
        reference_area_m2: 359.535,
        vref_m_s: 144.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "FAA Aircraft Characteristics Database, 2024-10 release (https://www.faa.gov/airports/engineering/aircraft_char_database/aircraft_data), ACD_Data row B789: Approach_Speed_knot 144 at MALW 425,000 lb, remark 'AAC validated by Flight Standardization Board (FSB) report' (minimum 140, maximum 144); it replaces the 153 kt of Boeing Airport Compatibility Engineering, FAA Reference Code and Approach Speeds for Boeing Aircraft (30 Mar 2016), which states the same landing weight: the FSB-validated speed is the one the operational Vref is certified to, and the rule is applied to every preset anchored on that Boeing list. Cross-check: 144 kt gives CLmax,1g 2.37 on this area, against the 2.40 flight-test CLmax,1g of the 777-200 at landing flap (Obert, Aerodynamic Design of Transport Aircraft, 2009, Fig. 26.16, printed p. 325); 153 kt would give 2.10. 359.535 m^2 (3,870 ft^2) is the preset's reference area, the Boeing (Wimpress) reference area of the PIANO Boeing 787-8 (Dreamliner) sample analysis (Lissys, 2005; https://www.lissys.uk/samp1/index.html), used for both this recovery and the model so Vref is reproduced (Boeing prints no wing area; the earlier 360.464 m^2 (3,880 ft^2) had no traceable source, and an untraceable 377 m^2 OpenAP/secondary value, 4.9 % larger, was used before 2026-10-07 and made the model Vref 156.5 kt against 153 kt published)",
        applicability: "Legacy 561,500 lb MTW / 425,000 lb MLW source matches the preset mass; area is a secondary-source Boeing (Wimpress) reference area (no Boeing, EASA or FAA document prints a 787 wing area); manufacturer planning speed treated as sea-level CAS; effective gross-area coefficient is distinct from the generic takeoff CLmax assumption",
    },
    PublishedLandingReference {
        preset_name: "DC-10",
        mass_kg: 421_000.0 * POUND_KG,
        reference_area_m2: 338.8,
        vref_m_s: 145.4 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Boeing DC-10 Airplane Characteristics for Airport Planning (ACAP, D6-58327) sec. 3.5 Approach Speed (Threshold), doc p.67: DC-10-30/30CF curve, 145.4 kt equivalent airspeed at maximum landing weight 421,000 lb (+-1 kt raster read, 1.3 Vs, flaps 50); https://www.boeing.com/content/dam/boeing/v2/airports/acaps/DC10.pdf; EASA IM.A.210 Issue 2, 2024: trapezoidal reference wing area 338.8 m^2",
        applicability: "ACAP value at the preset's own 421,000 lb MLW (the earlier 149 kt at 403,000 lb from the FAA approach-speed list is 3.6 kt higher than the ACAP curve at that weight, 142.7 kt); raster-scan read, +-1 kt; ACAP speed treated as sea-level CAS, 1.3 Vs read as the requested 1.23 VS1g equivalent",
    },
    PublishedLandingReference {
        preset_name: "B747-400",
        mass_kg: 630_000.0 * POUND_KG,
        reference_area_m2: B747_400_REFERENCE_AREA_M2,
        vref_m_s: 157.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "FAA Aircraft Characteristics Database, 2024-10 release (faa.gov/airports/engineering/aircraft_char_database), ACD_Data row B744: Approach_Speed_knot 157 at MALW 630,000 lb, remark 'AAC validated by Flight Standardization Board (FSB) report'; it replaces the 153 kt of Boeing Airport Compatibility Engineering, FAA Reference Code and Approach Speeds for Boeing Aircraft (30 Mar 2016), which states the same landing weight: the FSB-validated speed is the one the operational Vref is certified to, and the rule is applied to every preset anchored on that Boeing list. reference area 5,650 ft^2 (524.90 m^2) is the 'Wing Reference Area' of FAA DOT/FAA/AR-04/44 Table 1 (Jan 2005, https://www.govinfo.gov/content/pkg/GOVPUB-TD4-PURL-LPS64561/pdf/GOVPUB-TD4-PURL-LPS64561.pdf)",
        applicability: "630,000 lb is the preset's optional landing weight (Boeing 747-400 ACAP D6-58326-1 Rev F section 2.1.1); manufacturer planning speed treated as sea-level CAS; historical certification used 1.3 VS, so this is the requested 1.23 VS1g equivalent",
    },
    PublishedLandingReference {
        preset_name: "C919",
        mass_kg: 67_800.0,
        reference_area_m2: 129.15,
        vref_m_s: 141.7 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: 0.1 * KNOT_M_S,
        source: "COMAC C919 Airplane Characteristics for Airport Planning C919-SVV19-50009-00 R9, 17 Oct 2025, DM 03-05 Reference Speed - Performance Issue 004, PDF p131: 141.7 kt (262.4 km/h) indicated airspeed at 15 m threshold height, certificated maximum flap, maximum certificated landing weight 67,800 kg, standard atmosphere, approach category D, for information only; http://www.comac.cc/fujian/c919acap_en.pdf; reference area 129.15 m^2 per the ICAO AOP/SG/8 WP/26 aircraft data (area definition not stated)",
        applicability: "STD and ER share one value; planning IAS treated as sea-level CAS; reference-area definition unverified (family secondary value), so the coefficient is an effective gross-area one",
    },
    PublishedLandingReference {
        preset_name: "A350-900",
        mass_kg: 207_000.0,
        reference_area_m2: 445.0,
        vref_m_s: 140.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Airbus A350 Aircraft Characteristics, Airport and Maintenance Planning, Rev Jul 01/26, section 3-5-0 p.1: A350-900 final approach speed 140 kt at MLW 207,000 kg, indicated airspeed at threshold in the landing configuration at the certificated maximum landing weight, standard atmosphere; 445 m^2 is the preset's own planform area, used for both this recovery and the model so Vref is reproduced",
        applicability: "Airbus marks the speed for information only; planning IAS treated as sea-level CAS; reference-area definition not printed in the document, so the coefficient is an effective one on the preset's area",
    },
    PublishedLandingReference {
        preset_name: "LJ60",
        mass_kg: 19_500.0 * POUND_KG,
        reference_area_m2: 24.57,
        vref_m_s: 139.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Learjet 60 Airplane Flight Manual FM-123, Section V, Fig. 5-66 p. 5-86 (Approach Speeds, Normal Landings): VREF 139 KIAS, flaps 40 deg, gear down, 19,500 lb at 4,000 ft and below; the stall chart Fig. 5-22 p. 5-34 reads about 106 KCAS at that weight, so VREF = 1.3 VS; 24.57 m^2 is the preset's reference wing area, used for both this recovery and the model",
        applicability: "FAR 25 at the type's certification basis: 1.3 VS read as the requested 1.23 VS1g equivalent (AMC 25.103), IAS treated as CAS at this condition; maximum landing weight 19,500 lb",
    },
    PublishedLandingReference {
        preset_name: "E195-E2",
        mass_kg: 119_049.0 * POUND_KG,
        reference_area_m2: 103.0,
        vref_m_s: 136.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "FAA Aircraft Characteristics Database, October 2024 issue (xlsx served at https://www.faa.gov/airports/engineering/aircraft_char_database/aircraft_data, sha256 3e68848b6bb99f9700e25d33c080b993a80dc962d581a391f89aee94b6c8fcb3), sheet ACD_Data row E295 'Embraer 195-E2': Approach_Speed_knot 136, MALW_lb 119,049 (54,000 kg); Data_Dictionary: the highest approach speed at the maximum allowable landing weight over the landing flap configurations, from the Flight Standardization Board report or the manufacturer's documentation; 103.0 m^2 is the preset's reference wing area, used for both this recovery and the model",
        applicability: "Secondary compilation (no Embraer document prints an E195-E2 Vref with its mass); the database matches the manufacturer speeds of the A320-200, A350-900, A380-800 and ATR 72-600 exactly and the A220-300 and DC-10 within 1.2 % on the same mass; read as Vref = 1.23 VS1g at MALW, the maximum landing flap, IAS treated as sea-level CAS",
    },
    // Sibling rule: a derivative on the same type certificate with the same wing
    // reference area, the same flap system and the same landing flap deflection has the
    // sibling's landing coefficient, so its speed is the sibling's scaled by the square
    // root of the mass ratio. It applies only where no speed of the type itself is
    // published with its mass, and the record carries the sibling's own mass and speed.
    PublishedLandingReference {
        preset_name: "C295",
        mass_kg: CN235_300_STALL_MASS_KG,
        reference_area_m2: CN235_C295_WING_AREA_M2,
        vref_m_s: FAR_VREF_OVER_VS * CN235_300_LANDING_STALL_KT * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: FAR_VREF_OVER_VS * 0.5 * KNOT_M_S,
        source: "EADS CASA CN-235-300 Technical Review in BLM National Aviation Office, EADS CASA CN-235 Preliminary Aircraft Evaluation for SASEB, Flight Performance Data compiled from the Aircraft Flight Manual, PDF p. 59: stall speed in the landing configuration (flaps 23 deg) 79 kt at MTOW 34,830 lb and 75 kt at 31,350 lb, clean 99 and 94 kt, flaps 10 deg 86 and 82 kt; https://www.nifc.gov/sites/default/files/blm/aviation/BLMsmkj/CN235Report.pdf (sha256 91e73bcde744b1c6c249902fe8d534382556577f20b20ec141590300e4217103); EASA TCDS A.186 CN-235 / C-295, Issue 08: wing area 60 m^2 and flaps (inner and outer) landing 23 deg in the CN-235-300 section and in the C-295 section alike; Jane's All the World's Aircraft 2008-09, EADS CASA C-295 entry, printed p. 545: the C-295 has the CN-235 wing, reinforced",
        applicability: "Sibling rule: no C-295 stall or approach speed is published with its mass, so the coefficient is that of the CN-235-300 (same type certificate, wing area, single-slotted inner and outer flaps and 23 deg landing setting) and the speed scales with sqrt(mass); FAR 25 minimum-speed stall, reference speed 1.3 VS read as the requested 1.23 VS1g equivalent (AMC 25.103); the two weights give the same coefficient within 0.2 %; whole-knot stall speeds treated as sea-level CAS. Independent check: the FAA Aircraft Characteristics Database (2024-10) row CN35 gives 110 kt at 36,376 lb, 5 % above 1.3 VS at that mass; the class value of straight-wing turboprops read at 23 deg (high_lift_system) is 6 % above this coefficient",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_speeds_recover_the_one_g_lift_balance() {
        for reference in published_landing_references() {
            let stall_speed_m_s = (2.0 * reference.mass_kg * STANDARD_GRAVITY_M_S2
                / (reference.density_kg_m3
                    * reference.reference_area_m2
                    * reference.cl_max_land()))
            .sqrt();
            assert!(
                (VREF_VS1G * stall_speed_m_s - reference.vref_m_s).abs()
                    <= reference.speed_uncertainty_m_s,
                "{}",
                reference.preset_name
            );
            // The COMAC C919 ACAP is published over plain http only; every other
            // record cites an https source.
            let scheme = if reference.preset_name == "C919" {
                "http://"
            } else {
                "https://"
            };
            // The Airbus A350 AC revision and the Learjet 60 flight manual have no
            // stable public address: they are cited by document number and revision.
            let printed_only = matches!(reference.preset_name, "A350-900" | "LJ60");
            assert!(
                reference.source.contains(scheme)
                    || (printed_only
                        && (reference.source.contains("Rev ")
                            || reference.source.contains("FM-123"))),
                "{}",
                reference.preset_name
            );
            assert!(!reference.applicability.is_empty());
        }
    }

    #[test]
    fn lift_recovery_scales_with_weight_and_inverse_speed_squared() {
        let reference = REFERENCES[0];
        let heavier = PublishedLandingReference {
            mass_kg: 1.1 * reference.mass_kg,
            ..reference
        };
        let faster = PublishedLandingReference {
            vref_m_s: 1.1 * reference.vref_m_s,
            ..reference
        };
        assert!(heavier.cl_max_land() > reference.cl_max_land());
        assert!(faster.cl_max_land() < reference.cl_max_land());
        assert!(
            (faster.cl_max_land() * 1.1_f64.powi(2) / reference.cl_max_land() - 1.0).abs() < 1e-12
        );
    }

    #[test]
    fn source_mass_is_preserved_when_the_preset_weight_variant_differs() {
        let source = REFERENCES[3];
        let lighter_mass_kg = 386_000.0;
        let stall_speed_m_s = (2.0 * lighter_mass_kg * STANDARD_GRAVITY_M_S2
            / (source.density_kg_m3 * source.reference_area_m2 * source.cl_max_land()))
        .sqrt();
        assert!(VREF_VS1G * stall_speed_m_s < source.vref_m_s);
        assert!(
            (VREF_VS1G * stall_speed_m_s / source.vref_m_s
                - (lighter_mass_kg / source.mass_kg).sqrt())
            .abs()
                < 1e-12
        );
    }

    #[test]
    fn real_presets_carry_the_source_and_regulatory_speed_convention() {
        for preset in super::super::registry() {
            if let Some(reference) = published_landing_reference(preset.name) {
                let Some(performance) = &preset.performance else {
                    panic!("{} has no field inputs", preset.name);
                };
                assert!(performance.cl_max_land_source.contains(reference.source));
                assert_eq!(performance.vapp_vstall_land_factor, VREF_VS1G);
                assert!((performance.cl_max_land - reference.cl_max_land()).abs() < 1e-12);
            } else {
                // AVE is notional; the A400M has no validated landing speed with
                // its mass (the FAA database row, 130 kt at 271,166 lb, carries no
                // Flight Standardization Board remark and is 3 % below the class value).
                assert!(
                    matches!(preset.name, "AVE" | "A400M"),
                    "{} has no published landing reference",
                    preset.name
                );
            }
        }
    }
}
