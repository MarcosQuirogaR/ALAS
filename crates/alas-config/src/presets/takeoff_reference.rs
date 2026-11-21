// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Effective takeoff lift inputs recovered under an explicit model speed convention.
//!
//! Published V2 minima also depend on control, rotation and climb requirements.
//! Inverting a selected V2/VS1g ratio therefore gives a model-equivalent input;
//! it does not identify measured aerodynamic CLmax or a certified speed schedule.

use super::AircraftPreset;

const STANDARD_GRAVITY_M_S2: f64 = alas_atmo::STANDARD_GRAVITY;
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

/// Highest `V2 / VS1g` an Airbus type's take-off performance is computed to:
/// Airbus, *Getting to Grips with Aircraft Performance* (January 2002),
/// Appendix 2, sec. 2.3.1.2 "V2/VS Range", printed p. 194: "a too high V2
/// speed requires long takeoff distances and leads to the reduction of climb
/// performance ... the V2/VS ratio is limited to a maximum value (V2/VS
/// maxi), which depends on aircraft type": 1.35 VS (A300/A310), 1.35 VS1g
/// (A320 family), 1.40 VS1g (A330), 1.50 VS1g (A340). A type without a
/// printed value takes the A320-family 1.35 VS1g, the lowest printed on the
/// 1-g basis the model's `VSR` uses, so an overspeed the manufacturer would
/// not schedule is reported as a climb limit rather than flown.
pub const DEFAULT_V2_MAX_OVER_VS1G: f64 = 1.35;

const V2_MAX_SOURCE: &str = "Airbus, Getting to Grips with Aircraft Performance (Jan 2002), App. 2 sec. 2.3.1.2, p. 194 (V2/VS maxi)";

/// The printed `V2/VS maxi` of a registered type on the 1-g stall basis.
const PUBLISHED_V2_MAX_OVER_VS1G: [(&str, f64); 2] = [("A320-200", 1.35), ("A340-300", 1.50)];

/// The take-off overspeed limit `V2max / VS1g` of `preset` and its basis:
/// the type's printed value, else [`DEFAULT_V2_MAX_OVER_VS1G`].
pub fn v2_max_over_vs1g(preset: &str) -> (f64, String) {
    match PUBLISHED_V2_MAX_OVER_VS1G
        .iter()
        .find(|(name, _)| *name == preset)
    {
        Some((name, limit)) => (*limit, format!("{name} V2/VS maxi, {V2_MAX_SOURCE}")),
        None => (
            DEFAULT_V2_MAX_OVER_VS1G,
            format!("no printed value for the type: the A320-family V2/VS maxi, the lowest printed on the 1-g basis, {V2_MAX_SOURCE}"),
        ),
    }
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
        // A published V2 minimum is the CS 25.107(b)(1) minimum, 1.13 VSR for a
        // two-engine aeroplane, not the 1.20 VSR class selection: inverting with
        // 1.20 overstated the coefficient by (1.20 / 1.13)^2 = 1.128 and gave the
        // ATR 72-600 a take-off CLmax of 2.44 at flaps 15 for the same V2.
        performance.v2_vstall_factor = performance.v2_min_vsr_factor;
        // The model flies the V2/VSR of the CS 25.107 rule: the greatest of the
        // selected factor, V2MIN from stall and 1.10 VMC. The control term depends on
        // the aircraft as built (its computed VMC replaces the declared fallback),
        // so the published minimum is inverted with the part that does not: the
        // aircraft whose control speed does not bind flies exactly the published V2
        // in the stage check, the gate and the reference; one whose control speed
        // binds flies above it (a minimum), and the declared fallback ratio
        // (1.10 x 1.13) does too. Inverting with the fallback instead made the built aircraft fly 3.5 % below the published V2.
        let ratio = performance.control_free_v2_over_vsr();
        performance.cl_max_to = reference.cl_max_to(ratio);
        performance.cl_max_to_source = format!(
            "{}; effective model CLmax_TO recovered as 2*m*g/(rho*S*(V2_min/{ratio:.4})^2), rho={} kg/m^3; {}",
            reference.source, reference.density_kg_m3, reference.applicability
        );
    }
}

const REFERENCES: [PublishedTakeoffReference; 2] = [
    PublishedTakeoffReference {
        preset_name: "ATR72-600",
        mass_kg: 23_000.0,
        reference_area_m2: 61.0,
        v2_min_m_s: 116.0 * KNOT_M_S,
        density_kg_m3: 1.225,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "ATR 72-600 PW127M/N Factsheet, 2020, p.2: Take-off speed (V2 min @ MTOW) 116 KCAS; weight table includes option-1 MTOW 23,000 kg and wing area 61 m^2; https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf",
        applicability: "The single family-wide V2 minimum line is not column-paired with the 22,800 kg basic or 23,000 kg optional MTOW; assigning it to the preset's 23,000 kg option is a declared applicability assumption. The minimum is read as the CS 25.107(b)(1) minimum of a two-engine turbopropeller aeroplane, 1.13 VSR, which is what the model selects; the built aircraft (computed VMC below the declared 1.13 VSR) flies the published V2, a larger control speed would raise V2 above it, and the declared fallback ratio (1.10 x 1.13 = 1.243) also flies above it. Flap configuration and whether V2 min is stall-, control- or climb-limited are unpublished. Effective lift recovery does not establish measured CLmax, VMC or a certified V2 schedule; speed reconstruction is an input check, not independent validation. Source CAS is interpreted on the ISA sea-level equivalent-density basis.",
    },
    PublishedTakeoffReference {
        preset_name: "LJ60",
        mass_kg: 23_500.0 * 0.453_592_37,
        reference_area_m2: 24.57,
        v2_min_m_s: 153.0 * KNOT_M_S,
        density_kg_m3: 1.225,
        speed_uncertainty_m_s: KNOT_M_S,
        source: "Learjet 60 Airplane Flight Manual FM-123, Section V, Fig. 5-42 p. 5-59 (Uncorrected Takeoff Speeds, flaps 8 deg): V2 153 KIAS, VR 145 KIAS at 23,500 lb, sea level; the stall chart Fig. 5-22 p. 5-34 reads about 126 KCAS flaps 8 at that weight, so V2 = 1.2 VS; 24.57 m^2 is the preset's reference wing area",
        applicability: "FAR 25 at the type's certification basis: 1.2 VS read as the requested 1.13 VS1g equivalent (AMC 25.103), IAS treated as CAS; VMCA flaps 8 is 120 KIAS (TCDS), so V2 is stall limited, not control limited.",
    },
];

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
    fn the_overspeed_limit_is_the_printed_type_value_else_the_a320_family_one() {
        assert_eq!(v2_max_over_vs1g("A320-200").0, 1.35);
        assert_eq!(v2_max_over_vs1g("A340-300").0, 1.50);
        let (limit, basis) = v2_max_over_vs1g("B787-9");
        assert_eq!(limit, DEFAULT_V2_MAX_OVER_VS1G);
        assert!(basis.contains("no printed value"));
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
            (performance.cl_max_to / reference.cl_max_to(performance.control_free_v2_over_vsr())
                - 1.0)
                .abs()
                < 1e-12
        );
        assert!(performance.cl_max_to_source.contains("V2 min @ MTOW"));
        assert!(performance
            .cl_max_to_source
            .contains("https://www.atr-aircraft.com/"));
        Ok(())
    }
}
