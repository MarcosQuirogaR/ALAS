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

const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const KNOT_M_S: f64 = 1852.0 / 3600.0;
const POUND_KG: f64 = 0.453_592_37;
const VREF_VS1G: f64 = 1.23;

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

// old 7 -> 8 published landing references: Boeing 747-400 added.
const REFERENCES: [PublishedLandingReference; 8] = [
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
        reference_area_m2: 377.0,
        vref_m_s: 153.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Boeing Airport Compatibility Engineering, FAA Reference Code and Approach Speeds for Boeing Aircraft, 30 Mar 2016, p.3: 787-9 approach speed 153 kt, MLW 425,000 lb; https://www.boeing.com/content/dam/boeing/v2/airports/faq/arcandapproachspeeds.pdf; 377 m^2 gross reference area from OpenAP data release (Sun et al., Aerospace 2020, doi:10.3390/aerospace7080104), https://github.com/junzis/openap/blob/master/openap/data/aircraft/b789.yml",
        applicability: "Legacy 561,500 lb MTW / 425,000 lb MLW source matches the preset mass; manufacturer planning speed treated as sea-level CAS; effective gross-area coefficient is distinct from the generic takeoff CLmax assumption",
    },
    PublishedLandingReference {
        preset_name: "DC-10",
        mass_kg: 403_000.0 * POUND_KG,
        reference_area_m2: 338.8,
        vref_m_s: 149.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Boeing Airport Compatibility Engineering, FAA Reference Code and Approach Speeds for Boeing Aircraft, 30 Mar 2016, p.3: DC-10-30 approach speed 149 kt, MLW 403,000 lb; https://www.boeing.com/content/dam/boeing/v2/airports/faq/arcandapproachspeeds.pdf; EASA IM.A.210 Issue 2, 2024: trapezoidal reference wing area 338.8 m^2",
        applicability: "Lower-MLW DC-10-30 source, not the preset's 421,000 lb option; coefficient transferred at unchanged reference area/flap configuration with sqrt(mass) speed scaling; manufacturer planning speed treated as sea-level CAS; historical certification used 1.3 VS, so this is the requested 1.23 VS1g equivalent",
    },
    PublishedLandingReference {
        preset_name: "B747-400",
        mass_kg: 630_000.0 * POUND_KG,
        reference_area_m2: 525.0,
        vref_m_s: 153.0 * KNOT_M_S,
        density_kg_m3: ISA_SEA_LEVEL_DENSITY_KG_M3,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Boeing Airport Compatibility Engineering, FAA Reference Code and Approach Speeds for Boeing Aircraft, 30 Mar 2016, p.2: 747-400 approach speed 153 kt, MTW 877,000 lb, MLW 630,000 lb, wingspan 213.00 ft; https://www.boeing.com/content/dam/boeing/v2/airports/faq/arcandapproachspeeds.pdf; 525 m^2 reference area is Boeing's published 5,650 ft^2 (recalled, not retrieved in this session: estimate)",
        applicability: "630,000 lb is the preset's optional landing weight (Boeing 747-400 ACAP D6-58326-1 Rev F section 2.1.1); manufacturer planning speed treated as sea-level CAS; historical certification used 1.3 VS, so this is the requested 1.23 VS1g equivalent",
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
            assert!(reference.source.contains("https://"));
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
                // AVE is notional; no landing-speed source was found for the
                // E195-E2 (Embraer prints field lengths, not a Vref) or the
                // C919 (no airport-planning manual is public); the A400M
                // publishes no landing speed.
                assert!(
                    matches!(preset.name, "AVE" | "E195-E2" | "C919" | "A400M"),
                    "{} has no published landing reference",
                    preset.name
                );
            }
        }
    }
}
