// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Effective takeoff lift inputs recovered under an explicit model speed convention.
//!
//! Published V2 minima also depend on control, rotation and climb requirements.
//! Inverting a selected V2/VS1g ratio therefore gives a model-equivalent input;
//! it does not identify measured aerodynamic CLmax or a certified speed schedule.

use super::AircraftPreset;

const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;
const KNOT_M_S: f64 = 1852.0 / 3600.0;

/// Published minimum takeoff safety speed and the source condition supporting it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedTakeoffReference {
    /// Registered aircraft to which the effective coefficient is applied.
    pub preset_name: &'static str,
    /// Published MTOW option selected for the source-speed recovery, kg.
    pub mass_kg: f64,
    /// Reference area used to express the effective coefficient, m^2.
    pub reference_area_m2: f64,
    /// Source minimum takeoff safety speed at MTOW, m/s CAS.
    pub v2_min_m_s: f64,
    /// Sea-level equivalent-density basis for the recovery, kg/m^3.
    pub density_kg_m3: f64,
    /// Whole-knot publication/rounding allowance, m/s, not a model-validity bound.
    pub speed_uncertainty_m_s: f64,
    /// Primary source identifying the speed, mass and reference area.
    pub source: &'static str,
    /// Minimum-versus-selected speed, flap and certification limitations.
    pub applicability: &'static str,
}

impl PublishedTakeoffReference {
    /// Recover effective CLmax_TO with the caller's selected positive V2/VS1g ratio.
    ///
    /// The selected ratio is a model convention. CS/FAR 25.107(b) specifies
    /// minimum speed bounds; it does not make any single ratio an equality.
    pub fn cl_max_to(&self, selected_v2_over_vs1g: f64) -> f64 {
        let effective_stall_speed_m_s = self.v2_min_m_s / selected_v2_over_vs1g;
        2.0 * self.mass_kg * STANDARD_GRAVITY_M_S2
            / (self.density_kg_m3 * self.reference_area_m2 * effective_stall_speed_m_s.powi(2))
    }
}

/// Source-backed takeoff speed inputs; only the ATR has one registered here.
pub fn published_takeoff_references() -> &'static [PublishedTakeoffReference] {
    &REFERENCES
}

/// Source minimum speed for one real-aircraft preset, when the condition is known.
pub fn published_takeoff_reference(name: &str) -> Option<&'static PublishedTakeoffReference> {
    REFERENCES
        .iter()
        .find(|reference| reference.preset_name == name)
}

pub(super) fn apply(preset: &mut AircraftPreset) {
    if let Some(reference) = published_takeoff_reference(preset.name) {
        let performance = preset.performance.get_or_insert_with(Default::default);
        let ratio = performance.v2_vstall_factor;
        performance.cl_max_to = reference.cl_max_to(ratio);
        performance.cl_max_to_source = format!(
            "{}; effective model CLmax_TO recovered as 2*m*g/(rho*S*(V2_min/{ratio})^2), rho={} kg/m^3; {}",
            reference.source, reference.density_kg_m3, reference.applicability
        );
    }
}

const REFERENCES: [PublishedTakeoffReference; 1] = [PublishedTakeoffReference {
    preset_name: "ATR72-600",
    mass_kg: 23_000.0,
    reference_area_m2: 61.0,
    v2_min_m_s: 116.0 * KNOT_M_S,
    density_kg_m3: 1.225,
    speed_uncertainty_m_s: KNOT_M_S,
    source: "ATR 72-600 PW127M/N Factsheet, 2020, p.2: Take-off speed (V2 min @ MTOW) 116 KCAS; weight table includes option-1 MTOW 23,000 kg and wing area 61 m^2; https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf",
    applicability: "The single family-wide V2 minimum line is not column-paired with the 22,800 kg basic or 23,000 kg optional MTOW; assigning it to the preset's 23,000 kg option is a declared applicability assumption. The minimum is taken as the model-selected V2; retained V2/VS1g 1.20 is a model convention, rather than the twin-turboprop 25.107(b) stall floor 1.13. Flap configuration and whether V2 min is stall-, control- or climb-limited are unpublished. Effective lift recovery does not establish measured CLmax, VMC or a certified V2 schedule; speed reconstruction is an input check, not independent validation. Source CAS is interpreted on the ISA sea-level equivalent-density basis.",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_speed_recovers_the_declared_lift_balance() {
        for reference in published_takeoff_references() {
            for selected_ratio in [1.13, 1.20] {
                let coefficient = reference.cl_max_to(selected_ratio);
                assert!(coefficient.is_finite() && coefficient > 0.0);
                let stall_speed = (2.0 * reference.mass_kg * STANDARD_GRAVITY_M_S2
                    / (reference.density_kg_m3 * reference.reference_area_m2 * coefficient))
                    .sqrt();
                assert!(
                    (selected_ratio * stall_speed - reference.v2_min_m_s).abs()
                        <= reference.speed_uncertainty_m_s
                );
            }
        }
    }

    #[test]
    fn effective_lift_recovery_respects_mass_and_speed_scaling() {
        let reference = REFERENCES[0];
        let heavier = PublishedTakeoffReference {
            mass_kg: 1.1 * reference.mass_kg,
            ..reference
        };
        let faster = PublishedTakeoffReference {
            v2_min_m_s: 1.1 * reference.v2_min_m_s,
            ..reference
        };
        let coefficient = reference.cl_max_to(1.20);
        assert!((heavier.cl_max_to(1.20) / coefficient - 1.1).abs() < 1e-12);
        assert!((faster.cl_max_to(1.20) / coefficient - 1.0 / 1.1_f64.powi(2)).abs() < 1e-12);
    }

    #[test]
    fn the_registered_input_has_primary_provenance() -> Result<(), String> {
        let preset = super::super::get("ATR72-600").map_err(|error| error.to_string())?;
        let performance = preset
            .performance
            .as_ref()
            .ok_or("ATR performance is absent")?;
        let reference = published_takeoff_reference(preset.name).ok_or("ATR source is absent")?;
        assert!(
            (performance.cl_max_to / reference.cl_max_to(performance.v2_vstall_factor) - 1.0).abs()
                < 1e-12
        );
        assert!(performance.cl_max_to_source.contains("V2 min @ MTOW"));
        assert!(performance
            .cl_max_to_source
            .contains("https://www.atr-aircraft.com/"));
        Ok(())
    }
}
