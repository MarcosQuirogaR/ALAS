// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Uncertainty-aware verdict of the cruise-thrust and top-of-climb checks
//! (owner decision 2026-10-13).
//!
//! An engine whose cruise or climb thrust has a published datum (the deck is
//! scaled onto it, `EmpiricalTurbofanModel::engine_scale`) keeps the hard
//! check. An engine whose altitude thrust rests on the Jenkinson chart
//! correlation alone is a hard failure only when the thrust shortfall exceeds
//! the correlation's documented scatter
//! ([`alas_prop::empirical_turbofan::max_climb::CHART_THRUST_SCATTER`], derived from the
//! spread of the charts themselves); a smaller shortfall is a flagged warning
//! that every report carries with the required value, the available value and
//! the scatter. It is never hidden.
//!
//! The take-off thrust checks (CS 25.121(b) second segment, 25.121(d) approach
//! climb, 25.119 landing climb: the take-off/go-around rating) carry the
//! take-off band instead ([`takeoff_thrust_scatter`], owner decision
//! 2026-10-14): the half-spread between the two take-off thrust sources, the
//! Jenkinson charts the deck uses and the Bartel-Young/OpenAP correlation it
//! replaced. The cruise datums of the engine catalogue are cruise or climb
//! points and say nothing about the thrust at V2, so the band applies to every
//! turbofan, datum engine or not; a turboprop (unit model) has none.

use alas_config::{ActiveEngineModel, AlasConfig};

use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;
use alas_prop::empirical_turbofan::max_climb::CHART_THRUST_SCATTER;
use alas_prop::empirical_turbofan::takeoff_chart::TAKEOFF_THRUST_SCATTER;

/// Numerical slack on the relative shortfall.
const SLACK: f64 = 1.0e-5;

/// Outcome of a thrust check against its scatter band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustVerdict {
    /// The available thrust meets the requirement.
    Pass,
    /// Short of the requirement by no more than the scatter: flagged.
    Warning,
    /// Short by more than the scatter (or any shortfall with a datum).
    Hard,
}

/// What the scatter band of an assessment stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScatterBasis {
    /// The spread of the Jenkinson altitude charts (cruise, top of climb).
    AltitudeChart,
    /// The spread of the two take-off thrust sources (take-off/go-around
    /// rating checks).
    TakeoffSources,
}

/// The parts of a thrust-check sentence: formatted numbers and the English
/// basis sentence (a stable catalogue key), for presentation-side translation.
#[derive(Debug, Clone, PartialEq)]
pub struct ThrustMessage {
    /// What was checked, as composed by the caller.
    pub what: String,
    /// Required value, formatted.
    pub required: String,
    /// Available value, formatted.
    pub available: String,
    /// Unit of both.
    pub unit: String,
    /// Shortfall, percent, formatted.
    pub short_pct: String,
    /// The English sentence naming the scatter source.
    pub basis: &'static str,
    /// Scatter, percent, formatted.
    pub scatter_pct: String,
    /// The finished English sentence (what `describe` returns).
    pub english: String,
}

/// Required and available thrust (any consistent basis) with the scatter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrustAssessment {
    /// Required thrust (or thrust-to-weight).
    pub required: f64,
    /// Available thrust, same basis.
    pub available: f64,
    /// Documented scatter of the available value, fraction (0 with a datum).
    pub scatter: f64,
    /// What the scatter stands for.
    pub basis: ScatterBasis,
}

impl ThrustAssessment {
    /// Bind the three numbers.
    pub fn new(required: f64, available: f64, scatter: f64) -> Self {
        Self {
            required,
            available,
            scatter,
            basis: ScatterBasis::AltitudeChart,
        }
    }

    /// An assessment of a take-off/go-around rating check against the
    /// take-off band.
    pub fn takeoff(required: f64, available: f64, band: f64) -> Self {
        Self {
            basis: ScatterBasis::TakeoffSources,
            ..Self::new(required, available, band)
        }
    }

    /// Thrust the scatter band leaves as margin, `available - required (1 - scatter)`
    /// (same basis as the inputs): non-negative exactly when the level or
    /// point is flyable under the band rule.
    pub fn band_margin(&self) -> f64 {
        self.available - self.required * (1.0 - self.scatter)
    }

    /// Whether the point is flyable under the band rule: any verdict but
    /// [`ThrustVerdict::Hard`]. The mission planner's cruise ceiling and the
    /// `cruise_thrust` check share this one rule.
    pub fn flyable(&self) -> bool {
        self.verdict() != ThrustVerdict::Hard
    }

    /// Shortfall as a fraction of the requirement, `1 - available / required`
    /// (negative when there is margin). Not finite for unusable inputs.
    pub fn shortfall(&self) -> f64 {
        1.0 - self.available / self.required
    }

    /// Shortfall beyond the scatter band, zero inside it (the normalized
    /// violation of the hard constraint).
    pub fn violation(&self) -> f64 {
        let shortfall = self.shortfall();
        if shortfall.is_finite() {
            (shortfall - self.scatter - SLACK).max(0.0)
        } else {
            f64::INFINITY
        }
    }

    /// The verdict. Unusable numbers are hard.
    pub fn verdict(&self) -> ThrustVerdict {
        let shortfall = self.shortfall();
        if !shortfall.is_finite() {
            ThrustVerdict::Hard
        } else if shortfall <= SLACK {
            ThrustVerdict::Pass
        } else if shortfall <= self.scatter + SLACK {
            ThrustVerdict::Warning
        } else {
            ThrustVerdict::Hard
        }
    }

    /// One sentence with the required value, the available value and the
    /// scatter, for warnings.
    pub fn describe(&self, what: &str, unit: &str) -> String {
        let basis = match self.basis {
            ScatterBasis::AltitudeChart => {
                "the engine's altitude thrust rests on the Jenkinson chart correlation alone, whose documented scatter is"
            }
            ScatterBasis::TakeoffSources => {
                "the thrust at the take-off rating rests on two sources that differ (Jenkinson's take-off charts, used by the deck, and the Bartel-Young/OpenAP correlation); their half-spread over the fleet's bypass ratios is"
            }
        };
        format!(
            "{what}: required {:.4} {unit}, available {:.4} {unit} (short by {:.1} %); {basis} +-{:.1} %, so a shortfall inside that band is a warning, not a failure",
            self.required,
            self.available,
            100.0 * self.shortfall(),
            100.0 * self.scatter
        )
    }

    /// The same sentence as [`Self::describe`] as English templates over
    /// numbers, so a presentation layer can look the templates up in its
    /// catalogue: `(template, basis sentence, values)` where the template
    /// carries `{what} {required} {available} {unit} {short} {basis} {scatter}`
    /// placeholders and `values` are the formatted substitutions in that order.
    pub fn describe_template(&self, what: &str, unit: &str) -> ThrustMessage {
        let text = self.describe(what, unit);
        let (basis, _) = self.basis_and_scatter();
        ThrustMessage {
            what: what.to_owned(),
            required: format!("{:.4}", self.required),
            available: format!("{:.4}", self.available),
            unit: unit.to_owned(),
            short_pct: format!("{:.1}", 100.0 * self.shortfall()),
            basis,
            scatter_pct: format!("{:.1}", 100.0 * self.scatter),
            english: text,
        }
    }

    fn basis_and_scatter(&self) -> (&'static str, f64) {
        let basis = match self.basis {
            ScatterBasis::AltitudeChart => {
                "the engine's altitude thrust rests on the Jenkinson chart correlation alone, whose documented scatter is"
            }
            ScatterBasis::TakeoffSources => {
                "the thrust at the take-off rating rests on two sources that differ (Jenkinson's take-off charts, used by the deck, and the Bartel-Young/OpenAP correlation); their half-spread over the fleet's bypass ratios is"
            }
        };
        (basis, self.scatter)
    }
}

/// The scatter of the altitude thrust of the configured engine: zero for a
/// turboprop (a different model, not covered) and for a turbofan with a
/// published cruise or climb datum, the chart scatter otherwise.
pub fn thrust_scatter(config: &AlasConfig) -> f64 {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(payload)) if !payload.off_design.is_engine_datum() => {
            CHART_THRUST_SCATTER
        }
        _ => 0.0,
    }
}

/// The take-off thrust band of the configured engine: the half-spread of the
/// two take-off thrust sources ([`TAKEOFF_THRUST_SCATTER`]) for every
/// turbofan (no engine has a take-off or V2 datum: the catalogue datums are
/// cruise and climb points), zero for a turboprop (unit model, not covered).
pub fn takeoff_thrust_scatter(config: &AlasConfig) -> f64 {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(_)) => TAKEOFF_THRUST_SCATTER,
        _ => 0.0,
    }
}

/// The residual rows of a take-off/go-around rating check on the installed
/// static thrust-to-weight axis (`available`, `required`), judged against
/// `band`.
///
/// Without a band (a turboprop, or a check that is not a take-off rating) it
/// is the plain scaled constraint. With a band the constraint row carries
/// only the shortfall beyond it, and a shortfall inside the band adds a
/// Diagnostic `warning_id` row with the required value, the available value
/// and the band (never hidden).
pub(super) fn takeoff_banded_rows(
    id: &'static str,
    warning_id: &'static str,
    what: &str,
    available: f64,
    required: f64,
    band: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if band <= 0.0 {
        return vec![ConstraintResidual::scaled(
            id,
            Performance,
            available,
            required,
            "T/W",
            required - available,
            role,
        )];
    }
    let assessment = ThrustAssessment::takeoff(required, available, band);
    let mut rows = vec![ConstraintResidual::direct(
        id,
        Performance,
        available,
        required,
        "T/W",
        required - available,
        assessment.violation(),
        role,
    )];
    if assessment.verdict() == ThrustVerdict::Warning {
        rows.push(
            ConstraintResidual::direct(
                warning_id,
                Performance,
                available,
                required,
                "T/W",
                required - available,
                0.0,
                ResidualRole::Diagnostic,
            )
            .with_detail(assessment.describe(what, "T/W")),
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datum_engine_fails_on_any_shortfall() {
        let a = ThrustAssessment::new(0.30, 0.29, 0.0);
        assert_eq!(a.verdict(), ThrustVerdict::Hard);
        assert!(a.violation() > 0.0);
        assert_eq!(
            ThrustAssessment::new(0.30, 0.30, 0.0).verdict(),
            ThrustVerdict::Pass
        );
    }

    #[test]
    fn a_chart_engine_warns_inside_the_band_and_fails_beyond_it() {
        let s = CHART_THRUST_SCATTER;
        let inside = ThrustAssessment::new(1.0, 1.0 - 0.5 * s, s);
        assert_eq!(inside.verdict(), ThrustVerdict::Warning);
        assert_eq!(inside.violation(), 0.0);
        let beyond = ThrustAssessment::new(1.0, 1.0 - 1.5 * s, s);
        assert_eq!(beyond.verdict(), ThrustVerdict::Hard);
        assert!((beyond.violation() - (0.5 * s - SLACK)).abs() < 1e-12);
        assert_eq!(
            ThrustAssessment::new(1.0, 1.2, s).verdict(),
            ThrustVerdict::Pass
        );
        assert_eq!(
            ThrustAssessment::new(1.0, f64::NAN, s).verdict(),
            ThrustVerdict::Hard
        );
        assert!(inside.describe("x", "T/W").contains("5.1 %"));
    }

    #[test]
    fn the_take_off_band_applies_to_every_turbofan_and_describes_its_basis() {
        let s = TAKEOFF_THRUST_SCATTER;
        let inside = ThrustAssessment::takeoff(1.0, 1.0 - 0.5 * s, s);
        assert_eq!(inside.verdict(), ThrustVerdict::Warning);
        assert_eq!(inside.violation(), 0.0);
        let beyond = ThrustAssessment::takeoff(1.0, 1.0 - 1.5 * s, s);
        assert_eq!(beyond.verdict(), ThrustVerdict::Hard);
        let text = inside.describe("x", "T/W");
        assert!(
            text.contains("Bartel-Young") && text.contains("8.2 %"),
            "{text}"
        );
        // Datum engines (A320: CFM56-5B4) and chart-only engines (AVE: GE9X)
        // alike; a turboprop has no band.
        for (preset, expected) in [
            ("A320-200", s),
            ("AVE", s),
            ("DC-10", s),
            ("ATR72-600", 0.0),
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({"preset": preset})).unwrap();
            assert_eq!(takeoff_thrust_scatter(&config), expected, "{preset}");
        }
    }

    #[test]
    fn take_off_rows_carry_only_the_shortfall_beyond_the_band_and_flag_the_rest() {
        let band = TAKEOFF_THRUST_SCATTER;
        let rows = |available: f64, band: f64| {
            takeoff_banded_rows(
                "id",
                "id_within_takeoff_band",
                "check",
                available,
                1.0,
                band,
                ResidualRole::Constraint,
            )
        };
        // Met: one row, no violation.
        let met = rows(1.01, band);
        assert_eq!(met.len(), 1);
        assert_eq!(met[0].normalized_violation, 0.0);
        // Inside the band: satisfied constraint plus a Diagnostic warning row.
        let inside = rows(1.0 - 0.5 * band, band);
        assert_eq!(inside.len(), 2);
        assert_eq!(inside[0].normalized_violation, 0.0);
        assert_eq!(inside[1].id, "id_within_takeoff_band");
        assert_eq!(inside[1].role, ResidualRole::Diagnostic);
        assert!(inside[1].detail.as_deref().unwrap().contains("8.2 %"));
        // Beyond: the violation is the excess over the band.
        let beyond = rows(1.0 - 1.5 * band, band);
        assert_eq!(beyond.len(), 1);
        assert!((beyond[0].normalized_violation - (0.5 * band - SLACK)).abs() < 1e-12);
        // No band (turboprop): the plain scaled constraint, no warning row.
        let plain = rows(1.0 - 0.5 * band, 0.0);
        assert_eq!(plain.len(), 1);
        assert!((plain[0].normalized_violation - (0.5 * band - 1.0e-5)).abs() < 1e-12);
    }

    #[test]
    fn only_engines_without_a_datum_carry_the_scatter() {
        for (engine, datum) in [
            ("CFM56-5B4/3", true),
            ("CFM56-5C3/F", true),
            ("CF6-50", true),
            ("CF6-80C2", true),
            ("LEAP-1A", true),
            ("PW1500G", false),
            ("PW1900G", false),
            ("GEnx-1B", false),
            ("GE9X", false),
            ("LEAP-1C", false),
            ("Trent 900", false),
            ("Trent 970-84", false),
        ] {
            let spec = alas_config::engines::get(engine)
                .unwrap()
                .turbofan_spec()
                .unwrap();
            assert_eq!(spec.off_design.is_engine_datum(), datum, "{engine}");
        }
    }
}
