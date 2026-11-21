// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The validity domain of the compressible polar (A1F).
//!
//! The report polar is a linear Goethert/Prandtl-Glauert solve on the
//! stretched lattice. USAF DATCOM 4.1.3.2 Method 1 (the swept-wing lift-curve
//! slope the lattice reproduces) lists M < 0.8 in its methods summary, and
//! 4.1.5.2 (compressibility of the lift-dependent drag terms) is substantiated
//! for 0.13 < M < 0.81. Linear theory has no shock or critical-Mach physics,
//! so a cruise above the smaller limit is flagged rather than silently
//! reported. (Section numbers; the DATCOM 1978 page numbers are not cited
//! here.)

use alas_opt::mdo::drag_table::MACH_ABOVE_CRUISE;

use super::{warning, FindingCode, PhysicalFinding};

// Needs data: the transonic lift-slope and induced-drag correction
// above M 0.81 (cruise of B747, A380, B787, A350, B777) is not on disk. Raymer
// and Torenbeek give the Korn/Lock wave-drag rise only (already in analysis::wave),
// not a lift-slope or induced correction; DATCOM 4.1.3.2 and its compressibility
// tables must be read on the page before a correction is coded. Until then the
// polar is flagged, not corrected.
/// Highest Mach number the DATCOM compressible-lift correlations are listed
/// for (4.1.3.2 Method 1: M < 0.8; 4.1.5.2 reaches 0.81): the limit the
/// closed-form lift-curve-slope rule declares.
pub(super) const SUBSTANTIATED_MACH_MAX: f64 =
    alas_aero::analysis::lift_slope::SUBSTANTIATED_MACH_MAX;

/// The warning for a polar solved on the stretched lattice at `mach`, when
/// that Mach is above [`SUBSTANTIATED_MACH_MAX`]; `None` for the
/// incompressible polar (`None`) or one inside the range.
pub(super) fn finding(compressible_mach: Option<f64>) -> Option<PhysicalFinding> {
    let mach =
        compressible_mach.filter(|mach| mach.is_finite() && *mach > SUBSTANTIATED_MACH_MAX)?;
    Some(warning(
        FindingCode::AeroLinearModelDomain,
        format!(
            "the reported polar is a linear Prandtl-Glauert solve at Mach {mach:.2}, above the \
             range DATCOM substantiates for compressible lift and induced drag (M <= \
             {SUBSTANTIATED_MACH_MAX}); lift slope, induced drag and trim are not validated there"
        ),
        Some(mach),
        Some(SUBSTANTIATED_MACH_MAX),
        "Mach",
    ))
}

/// A warning for each thing the wing's selected drag-divergence law
/// (`geometry.wing.drag_divergence_law`) does not use or applies outside its
/// source (`alas_config::WingConfig::divergence_law_notes`): a Korn factor
/// override that has no effect, or Torenbeek's Eq. 10.48 on a wing declared
/// conventional.
pub(super) fn divergence_law_findings(wing: &alas_config::WingConfig) -> Vec<PhysicalFinding> {
    wing.divergence_law_notes()
        .into_iter()
        .map(|note| {
            warning(
                FindingCode::DragDivergenceLawSelection,
                note,
                wing.korn_technology_factor_override,
                None,
                "",
            )
        })
        .collect()
}

/// The compressible stretch `1/beta - 1` the mission drag table scales its
/// trimmed induced drag with (`alas_opt::mdo::drag_table`, the Mach blend),
/// the radicand floored as there.
fn stretch_excess(mach: f64) -> f64 {
    1.0 / (1.0 - mach * mach).max(1.0e-6).sqrt() - 1.0
}

/// The warning for the mission drag table of a polar solved on the stretched
/// lattice at `compressible_mach`, when the table's top Mach node
/// (`cruise + MACH_ABOVE_CRUISE`) is above [`SUBSTANTIATED_MACH_MAX`]; `None`
/// for the incompressible polar or a table that stays inside the range.
///
/// The table does not solve the lattice above the cruise Mach: it carries the
/// compressible excess of the cruise-Mach induced drag upward in proportion
/// to `1/beta - 1`. The finding quotes that weight at the top node, the
/// factor by which the last solved excess is extrapolated.
pub(super) fn mission_table_finding(compressible_mach: Option<f64>) -> Option<PhysicalFinding> {
    let cruise = compressible_mach.filter(|mach| mach.is_finite() && *mach > 0.0)?;
    let top = cruise + MACH_ABOVE_CRUISE;
    if top <= SUBSTANTIATED_MACH_MAX {
        return None;
    }
    let weight = stretch_excess(top) / stretch_excess(cruise);
    Some(warning(
        FindingCode::AeroLinearModelDomain,
        format!(
            "the mission drag table reaches Mach {top:.2} (cruise Mach {cruise:.2} plus \
             {MACH_ABOVE_CRUISE}), above the range DATCOM substantiates for compressible lift and \
             induced drag (M <= {SUBSTANTIATED_MACH_MAX}): above the cruise Mach the compressible \
             part of the trimmed induced drag is not solved but extrapolated, at the top by a \
             factor of {weight:.2} on the cruise-Mach value; mission fuel flown above Mach \
             {SUBSTANTIATED_MACH_MAX} is not validated"
        ),
        Some(top),
        Some(SUBSTANTIATED_MACH_MAX),
        "Mach",
    ))
}

/// The line the report summary carries when the cruise Mach of the
/// compressible polar is above [`SUBSTANTIATED_MACH_MAX`]: the cruise
/// lift-to-drag ratio and the mission fuel are then extrapolated quantities.
pub(crate) fn summary_note(compressible_mach: Option<f64>) -> Option<String> {
    let mach =
        compressible_mach.filter(|mach| mach.is_finite() && *mach > SUBSTANTIATED_MACH_MAX)?;
    Some(format!(
        "cruise L/D and mission fuel are extrapolated above M {SUBSTANTIATED_MACH_MAX:.2}: \
         linear compressible model at Mach {mach:.2}, not validated there"
    ))
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mission_table_is_flagged_when_its_top_mach_leaves_the_range() {
        assert!(mission_table_finding(None).is_none());
        // 0.44 + 0.06 = 0.50 and 0.74 + 0.06 = 0.80: inside.
        assert!(mission_table_finding(Some(0.44)).is_none());
        assert!(mission_table_finding(Some(0.74)).is_none());
        // Cruise inside the range, table top outside it.
        let single_aisle = mission_table_finding(Some(0.78)).unwrap();
        assert_eq!(single_aisle.code, FindingCode::AeroLinearModelDomain);
        assert!((single_aisle.actual.unwrap() - 0.84).abs() < 1e-12);
        // Cruise 0.85: 1/beta = 1.898 at 0.85 and 2.412 at 0.91, weight 1.57.
        let wide_body = mission_table_finding(Some(0.85)).unwrap();
        assert!((wide_body.actual.unwrap() - 0.91).abs() < 1e-12);
        assert!(
            wide_body.message.contains("factor of 1.57"),
            "{}",
            wide_body.message
        );
        assert_eq!(wide_body.limit, Some(SUBSTANTIATED_MACH_MAX));
    }

    #[test]
    fn the_summary_note_follows_the_cruise_mach() {
        assert!(summary_note(None).is_none());
        assert!(summary_note(Some(0.78)).is_none());
        let note = summary_note(Some(0.85)).unwrap();
        assert!(note.contains("extrapolated above M 0.80"), "{note}");
    }

    #[test]
    fn only_a_compressible_polar_above_the_substantiated_mach_is_flagged() {
        assert!(finding(None).is_none());
        assert!(finding(Some(0.44)).is_none());
        assert!(finding(Some(SUBSTANTIATED_MACH_MAX)).is_none());
        assert!(finding(Some(f64::NAN)).is_none());
        let flagged = finding(Some(0.85)).unwrap();
        assert_eq!(flagged.code, FindingCode::AeroLinearModelDomain);
        assert_eq!(flagged.actual, Some(0.85));
        assert_eq!(flagged.limit, Some(SUBSTANTIATED_MACH_MAX));
    }

    /// The default selection raises nothing; an override the selected law
    /// does not use, and Eq. 10.48 on a wing declared conventional, are each
    /// one warning.
    #[test]
    fn a_law_that_ignores_a_declared_input_is_reported() {
        use alas_config::{AirfoilClass, DragDivergenceLaw, WingConfig};
        assert!(divergence_law_findings(&WingConfig::default()).is_empty());
        let wing = WingConfig {
            drag_divergence_law: DragDivergenceLaw::Torenbeek1048,
            airfoil_class: AirfoilClass::Conventional,
            korn_technology_factor_override: Some(0.9),
            ..WingConfig::default()
        };
        let findings = divergence_law_findings(&wing);
        assert_eq!(findings.len(), 2);
        for finding in &findings {
            assert_eq!(finding.code, FindingCode::DragDivergenceLawSelection);
            assert_eq!(finding.severity, super::super::FindingSeverity::Warning);
        }
        assert!(findings[0].message.contains("has no effect"));
        assert!(findings[1].message.contains("declared conventional"));
    }
}
