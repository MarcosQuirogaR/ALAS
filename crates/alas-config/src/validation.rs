// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/validation.py
// Reference: alas @ rust-port-baseline.

//! The checks a configuration has to pass that no single field can make.
//!
//! A field's own bounds keep it sensible on its own. What they cannot see is a
//! combination: every value can sit well inside its own range and still add up
//! to an aircraft that cruises past its structural dive speed, or a tailplane
//! whose tip is wider than its root. Each rule here reads several groups at
//! once, which is why it needs a whole [`AlasConfig`] and cannot live on a
//! field.
//!
//! What a rule produces is a path and a sentence. The path is what the
//! interface scrolls to and highlights, so a rule that fires correctly and
//! names the wrong field sends the user to edit something that was never the
//! problem; the sentence has to say which two values disagree and by how much,
//! because "invalid configuration" is not actionable.
//!
//! Severity is not decoration. An [`Severity::Error`] blocks the run and a
//! [`Severity::Warning`] only highlights, and the V-n rule uses both: past the
//! dive speed is the diagram's red zone, and between the cruise and dive
//! speeds is its caution band, which is a real design and not a good one.
//!
//! # Two differences from the reference
//!
//! Upstream wraps each rule in a `try/except` that swallows anything a rule
//! raises, because it runs on every keystroke of a debounced live preview and
//! a field caught mid-edit could break a unit conversion. A rule here reads
//! typed fields off a constructed configuration and has nothing to raise, so
//! there is no equivalent and none is needed.
//!
//! The cruise rule reproduces the reference's own two lines rather than
//! calling the V-n diagram builder, for the reason upstream gives: that
//! builder additionally needs a fully constructed aeroplane for its stall
//! terms, which is far too expensive to build on every validation tick. It
//! evaluates the atmosphere through the closed-form ISA where upstream uses
//! AeroSandbox's fitted default; the two agree to about 1e-11 and every number
//! the rule prints is rounded to the nearest whole metre per second.

use serde::{Deserialize, Serialize};

use crate::AlasConfig;

/// Sea-level density the equivalent airspeed is referred to, in kg/m^3.
///
/// The certification V-speeds are equivalent airspeeds, so the cruise point
/// has to be converted to one before it can be compared with them. This is the
/// standard sea-level value the conversion is defined against, held here
/// rather than read from the atmosphere model because it is the definition of
/// the airspeed reference and not a property of today's air.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// The margin CS-25.335(b) requires between the design cruise and dive speeds.
const DIVE_TO_CRUISE_SPEED_RATIO: f64 = 1.25;

/// Whether an issue blocks the run or only marks the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The configuration cannot be run as it stands.
    Error,
    /// The configuration is runnable, and something about it deserves a look.
    Warning,
}

/// One thing wrong with a configuration, and where to look for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Dotted path from the configuration root down to the offending field,
    /// such as `geometry.empennage.hstab_tip_chord_m`. The first segment names
    /// one of [`AlasConfig`]'s groups and the rest is a chain of field names.
    pub field_path: String,
    /// What is wrong, naming both values and their units.
    pub message: String,
    /// Whether this blocks the run.
    pub severity: Severity,
}

/// Every issue every rule finds, in the order the rules are registered.
///
/// All rules run: the first failure does not stop the rest, because a user
/// fixing one problem at a time and re-running is a worse experience than a
/// list, and because two issues are often one cause.
pub fn validate(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = cruise_point_inside_the_flight_envelope(config);
    issues.extend(empennage_tapers_toward_its_tips(config));
    issues
}

/// The cruise design point must sit inside the V-n envelope.
///
/// The design dive speed is an equivalent airspeed, as every CS-25 V-speed is,
/// so the cruise point is converted to one through the square root of the
/// density ratio before the two are compared. Comparing a true airspeed
/// against an equivalent one instead rejects designs that are perfectly fine,
/// by a factor that grows with altitude.
fn cruise_point_inside_the_flight_envelope(config: &AlasConfig) -> Vec<ValidationIssue> {
    let requirements = &config.requirements;
    let atmosphere = alas_atmo::Atmosphere::new(requirements.cruise_altitude_m);
    let cruise_eas_m_s = requirements.cruise_mach
        * atmosphere.speed_of_sound()
        * (atmosphere.density() / SEA_LEVEL_DENSITY_KG_M3).sqrt();

    let dive_speed = requirements.dive_speed_m_s;
    let design_cruise_speed = dive_speed / DIVE_TO_CRUISE_SPEED_RATIO;

    if cruise_eas_m_s > dive_speed {
        return vec![ValidationIssue {
            field_path: "requirements.dive_speed_m_s".to_owned(),
            message: format!(
                "Cruise design point ({cruise_eas_m_s:.0} m/s EAS at Mach \
                 {:.2} / {} m) exceeds the design dive speed VD \
                 ({dive_speed:.0} m/s EAS) -- the aircraft would cruise \
                 outside its own structural flight envelope.",
                requirements.cruise_mach,
                grouped(requirements.cruise_altitude_m),
            ),
            severity: Severity::Error,
        }];
    }
    if cruise_eas_m_s > design_cruise_speed {
        return vec![ValidationIssue {
            field_path: "requirements.dive_speed_m_s".to_owned(),
            message: format!(
                "Cruise design point ({cruise_eas_m_s:.0} m/s EAS) is above VC \
                 ({design_cruise_speed:.0} m/s EAS = VD/1.25) -- the aircraft \
                 cruises in the V-n diagram's caution band, not normal \
                 operation."
            ),
            severity: Severity::Warning,
        }];
    }
    Vec::new()
}

/// Both stabilizers must be narrower at the tip than at the root.
///
/// The canonical "is this a shape at all" check. An inverted taper is almost
/// always a transposed pair of numbers, and it produces a surface the geometry
/// builder will happily loft and every downstream area and volume coefficient
/// will then be computed from.
fn empennage_tapers_toward_its_tips(config: &AlasConfig) -> Vec<ValidationIssue> {
    let empennage = &config.geometry.empennage;
    let mut issues = Vec::new();

    if empennage.hstab_tip_chord_m >= empennage.hstab_root_chord_m {
        issues.push(ValidationIssue {
            field_path: "geometry.empennage.hstab_tip_chord_m".to_owned(),
            message: format!(
                "H-stab tip chord ({:.2} m) must be smaller than its root \
                 chord ({:.2} m).",
                empennage.hstab_tip_chord_m, empennage.hstab_root_chord_m
            ),
            severity: Severity::Error,
        });
    }
    if empennage.vstab_tip_chord_m >= empennage.vstab_root_chord_m {
        issues.push(ValidationIssue {
            field_path: "geometry.empennage.vstab_tip_chord_m".to_owned(),
            message: format!(
                "V-stab tip chord ({:.2} m) must be smaller than its root \
                 chord ({:.2} m).",
                empennage.vstab_tip_chord_m, empennage.vstab_root_chord_m
            ),
            severity: Severity::Error,
        });
    }
    issues
}

/// A whole number with thousands separators, as Python's `,.0f` renders it.
///
/// An altitude in metres is five digits, and five unbroken digits in the
/// middle of a sentence is the kind of number a reader mis-reads by a factor
/// of ten. The grouping is part of the message the reference emits, so it is
/// reproduced rather than left to the formatter.
fn grouped(value: f64) -> String {
    let rounded = format!("{value:.0}");
    let (sign, digits) = match rounded.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", rounded.as_str()),
    };

    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped}")
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_configuration_is_one_nothing_objects_to() {
        assert!(validate(&AlasConfig::default()).is_empty());
    }

    #[test]
    fn every_registered_aircraft_passes_its_own_validation() {
        // A preset that does not validate is a starting point the interface
        // offers and then refuses to run.
        for preset in crate::presets::registry() {
            let config = AlasConfig {
                geometry: preset.geometry.clone(),
                requirements: preset.requirements.clone(),
                ..Default::default()
            };
            assert_eq!(validate(&config), Vec::new(), "{}", preset.name);
        }
    }

    #[test]
    fn cruising_past_the_dive_speed_blocks_the_run() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 100.0;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].field_path, "requirements.dive_speed_m_s");
    }

    #[test]
    fn cruising_in_the_caution_band_warns_without_blocking() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 130.0;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warning);
    }

    #[test]
    fn the_cruise_point_is_compared_as_an_equivalent_airspeed() {
        // The true airspeed at the default cruise point is about twice the
        // equivalent one, so a rule comparing the wrong reference would reject
        // the shipped configuration outright.
        let config = AlasConfig::default();
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let true_airspeed = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        assert!(true_airspeed > config.requirements.dive_speed_m_s);
        assert!(validate(&config).is_empty());
    }

    #[test]
    fn an_untapered_surface_is_rejected_as_well_as_an_inverted_one() {
        // The comparison is >= and not >, so equal chords fail too.
        let mut config = AlasConfig::default();
        config.geometry.empennage.hstab_tip_chord_m = config.geometry.empennage.hstab_root_chord_m;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_path, "geometry.empennage.hstab_tip_chord_m");
    }

    #[test]
    fn every_rule_runs_rather_than_the_first_failure_stopping_the_rest() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 100.0;
        config.geometry.empennage.vstab_tip_chord_m = 10.0;
        assert_eq!(validate(&config).len(), 2);
    }

    #[test]
    fn a_severity_is_written_the_way_the_interface_reads_it() {
        assert_eq!(
            serde_json::to_value(Severity::Warning).unwrap(),
            serde_json::json!("warning")
        );
    }

    #[test]
    fn a_five_digit_altitude_is_grouped_the_way_the_message_expects() {
        assert_eq!(grouped(11_887.2), "11,887");
        assert_eq!(grouped(0.0), "0");
        assert_eq!(grouped(999.0), "999");
        assert_eq!(grouped(1_000.0), "1,000");
        assert_eq!(grouped(1_234_567.0), "1,234,567");
        assert_eq!(grouped(-2_500.0), "-2,500");
    }
}
