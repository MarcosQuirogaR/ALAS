// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drag of the take-off configuration in the climb-out at `V2`.
//!
//! Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), Sec. 5.3.3 "Drag
//! in takeoff and landing", Eq. 5-29, p. 154, the generalised locus of the
//! operating points at `V2` across flap settings:
//!
//! ```text
//! C_D,V2 = C0 + C_L,V2^2 / (pi A E)
//!
//! C0 = 0.018, E = 0.70   slats extended
//! C0 = 0.005, E = 0.61   no slats or slats retracted
//! ```
//!
//! `C_D,V2` and `C_L,V2` are the drag and lift coefficients at the take-off
//! safety speed on the reference wing area, `A` the wing aspect ratio. `C0` is
//! the intercept of the locus, not an increment over the clean zero-lift drag,
//! and `E` divides the whole of `C_L^2`: the two are selected together by the
//! slat state. The locus contains the take-off flaps, so no flap increment is
//! added to it. For an engine failure `E` "may be reduced by approximately 4 %
//! for wing-mounted engines and 2 % for engines on either side of the rear
//! fuselage" ("to include drag due to engine failure at low thrust/weight
//! ratios").
//!
//! Interpretation, not printed: the two classes differ by the yawing arm of
//! the failed engine, so the reduction is read as the drag of flying with
//! asymmetric thrust (the trim drag of the rudder, the fin side force and the
//! bank), and the drag of the dead engine itself (windmilling, spillage, or a
//! feathered propeller) is kept as a separate increment for every engine
//! position. The position that sets the class is the critical engine's, the
//! one with the longest yawing arm: an engine on the centreline has no
//! asymmetry and is never critical. A configuration that states its own
//! asymmetric-trim drag coefficient keeps `E` unreduced and adds the stated
//! drag instead.
//!
//! Validity domain, as the text states it: "this equation does not represent
//! an actual aircraft polar; it refers to the initial climb-out after
//! takeoff", undercarriage retracted, take-off flaps, at `V2`, correlated on
//! civil transports. It is therefore read for the CS 25.121(b) second segment
//! and the improved-climb `V2` selection only: not for the ground roll, not
//! for the gear-down first segment, and not above the take-off maximum lift
//! coefficient, where [`V2ClimbOutPolar::cd`] returns an error instead of a
//! value.

use alas_config::presets::{slat_declaration, SlatDeclaration};
use alas_config::AlasConfig;
use alas_perf::performance::{assess_oei_climb, OeiClimbAssessment, OeiV2Condition};

/// Airplane efficiency factor `E` of Eq. 5-29 with no slats or slats retracted.
pub(super) const TAKEOFF_AIRPLANE_OSWALD_FACTOR: f64 = 0.61;
/// `E` of Eq. 5-29 with the slats extended.
pub(super) const TAKEOFF_AIRPLANE_OSWALD_FACTOR_SLATS: f64 = 0.70;
/// Intercept `C0` of Eq. 5-29 with no slats or slats retracted.
const V2_LOCUS_INTERCEPT: f64 = 0.005;
/// `C0` of Eq. 5-29 with the slats extended.
const V2_LOCUS_INTERCEPT_SLATS: f64 = 0.018;
/// Factor on `E` for a failed wing-mounted engine (a reduction of 4 %).
const ENGINE_FAILURE_E_FACTOR_WING: f64 = 0.96;
/// Factor on `E` for a failed engine on the side of the rear fuselage (2 %).
const ENGINE_FAILURE_E_FACTOR_REAR_FUSELAGE: f64 = 0.98;

/// Position class of the critical engine (the failed engine with the longest
/// yawing arm), which selects the engine-failure reduction of `E`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineFailureClass {
    /// The critical engine hangs from the wing.
    WingMounted,
    /// The critical engine is on a side of the rear fuselage.
    RearFuselage,
    /// No engine off the centreline, or a rear mount that is not valid: the
    /// source gives no reduction.
    Unclassified,
    /// The configuration states the asymmetric-trim drag coefficient itself:
    /// that drag is added and `E` is not reduced.
    StatedTrimDrag,
}

impl EngineFailureClass {
    /// The class of the built installation of `config`.
    ///
    /// A centreline engine (a centre installation or a zero spanwise
    /// position) is passed over: its failure leaves no yawing moment, so the
    /// critical engine is an off-centreline one. `aft_fuselage` puts every
    /// off-centreline engine on the rear fuselage; without it they hang from
    /// the wing.
    pub fn of(config: &AlasConfig) -> Self {
        let engine = &config.geometry.engine;
        let positions = &engine.spanwise_positions_m;
        if config.performance.oei_asymmetric_trim_cd.is_some() {
            Self::StatedTrimDrag
        } else if positions.len() < 2 || positions.iter().all(|y| *y == 0.0) {
            Self::Unclassified
        } else if engine.aft_fuselage.is_some() {
            if engine.rear_fuselage_mount().is_some() {
                Self::RearFuselage
            } else {
                Self::Unclassified
            }
        } else {
            Self::WingMounted
        }
    }
}

/// The `V2` climb-out locus of one configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V2ClimbOutPolar {
    /// Slat state the pair `(C0, E)` is selected by.
    pub slats: SlatDeclaration,
    /// Engine position class the engine-failure reduction is selected by.
    pub engines: EngineFailureClass,
    /// Wing aspect ratio `b_ref^2 / s_ref`.
    pub aspect_ratio: f64,
}

/// Aspect ratio `A` of the Eq. 5-29 locus: the overall projected span
/// squared over the reference area.
///
/// `span_m` is the built wing-proper span (`Airplane::b_ref`), which ends at
/// the wing-tip chord; a wingtip device reaches a further
/// [`alas_config::geometry::WingtipDevice::lateral_reach_m`] outboard on each
/// side. The locus correlates `A = b^2 / S` with `b` the overall span of the
/// lifting system, and the induced drag of a planar lifting system is set by
/// its projected span (Munk's stagger theorem; Raymer, *Aircraft Design: A
/// Conceptual Approach*, 6th ed., Sec. 12.6), so the device's in-plane reach
/// is counted and its vertical extent is not (a lower bound of its benefit).
/// Without a device this is `b_ref^2 / S`.
pub fn climb_out_aspect_ratio(config: &AlasConfig, span_m: f64, area_m2: f64) -> f64 {
    let reach = config
        .geometry
        .wing
        .wingtip_device
        .as_ref()
        .map_or(0.0, |device| device.lateral_reach_m().max(0.0));
    let span = 2.0f64.mul_add(reach, span_m);
    span * span / area_m2
}

impl V2ClimbOutPolar {
    /// The locus of `config` on a wing of `aspect_ratio`.
    pub fn of(config: &AlasConfig, aspect_ratio: f64) -> Self {
        Self {
            slats: slat_declaration(
                &config.preset,
                &config.performance.high_lift_chord_extension,
            ),
            engines: EngineFailureClass::of(config),
            aspect_ratio,
        }
    }

    /// The `(C0, E)` pairs that apply: one for a stated slat state, both for
    /// an undeclared one ([`Self::cd`] then takes the higher drag).
    fn pairs(&self) -> &'static [(f64, f64)] {
        const SLATS: (f64, f64) = (
            V2_LOCUS_INTERCEPT_SLATS,
            TAKEOFF_AIRPLANE_OSWALD_FACTOR_SLATS,
        );
        const NO_SLATS: (f64, f64) = (V2_LOCUS_INTERCEPT, TAKEOFF_AIRPLANE_OSWALD_FACTOR);
        match self.slats {
            SlatDeclaration::Slats => &[SLATS],
            SlatDeclaration::NoSlats | SlatDeclaration::KruegerOnly => &[NO_SLATS],
            SlatDeclaration::Undeclared => &[SLATS, NO_SLATS],
        }
    }

    /// Factor on `E` for an engine failure, where the source states one for
    /// the position of the critical engine.
    pub fn engine_failure_e_factor(&self) -> Option<f64> {
        match self.engines {
            EngineFailureClass::WingMounted => Some(ENGINE_FAILURE_E_FACTOR_WING),
            EngineFailureClass::RearFuselage => Some(ENGINE_FAILURE_E_FACTOR_REAR_FUSELAGE),
            EngineFailureClass::Unclassified | EngineFailureClass::StatedTrimDrag => None,
        }
    }

    /// `C_D,V2` at the lift coefficient `cl` at `V2`, gear retracted, take-off
    /// flaps included; with `engine_failed`, `E` carries the engine-failure
    /// reduction of the engine position (none for an unclassified position).
    /// Dimensionless, on the reference wing area.
    ///
    /// # Errors
    ///
    /// Outside the domain (`cl` above the take-off maximum lift coefficient
    /// `cl_max_to`, or not positive and finite) and without a positive finite
    /// aspect ratio: the reason, never an extrapolated value.
    pub fn cd(&self, cl: f64, cl_max_to: f64, engine_failed: bool) -> Result<f64, String> {
        if !(self.aspect_ratio.is_finite() && self.aspect_ratio > 0.0) {
            return Err(
                "the V2 climb-out drag needs a positive finite wing aspect ratio".to_owned(),
            );
        }
        if !(cl.is_finite() && cl > 0.0) {
            return Err(
                "the V2 climb-out drag needs a positive finite lift coefficient".to_owned(),
            );
        }
        if !(cl_max_to.is_finite() && cl <= cl_max_to) {
            return Err(format!(
                "lift coefficient {cl:.3} is above the take-off maximum lift coefficient \
                 {cl_max_to:.3}, outside the domain of the V2 climb-out locus (Torenbeek 1982, \
                 Eq. 5-29); the locus is not extrapolated"
            ));
        }
        let factor = if engine_failed {
            self.engine_failure_e_factor().unwrap_or(1.0)
        } else {
            1.0
        };
        Ok(self
            .pairs()
            .iter()
            .map(|(c0, e)| c0 + cl * cl / (std::f64::consts::PI * self.aspect_ratio * e * factor))
            .fold(f64::NEG_INFINITY, f64::max))
    }

    /// The engine-out condition the assessor reads with this locus: where `E`
    /// carries the engine failure, the separate asymmetric-trim increment is
    /// zero (it would count the asymmetry twice); otherwise it stands as the
    /// engine-out model derived it or as the configuration states it. The
    /// drag of the failed engine itself (`windmilling_cd`) always stands.
    pub fn engine_out_condition(&self, condition: OeiV2Condition) -> OeiV2Condition {
        if self.engine_failure_e_factor().is_none() {
            return condition;
        }
        OeiV2Condition {
            asymmetric_trim_cd: Some(0.0),
            ..condition
        }
    }

    /// The CS 25.121(b) second-segment assessment at `cl` on this locus. A
    /// lift coefficient outside the domain leaves the drag undefined, which
    /// the assessor reports as an evidence gap.
    pub fn assess(
        &self,
        n_engines: i64,
        gradient: f64,
        cl: f64,
        cl_max_to: f64,
        condition: Option<OeiV2Condition>,
    ) -> OeiClimbAssessment {
        assess_oei_climb(
            self.cd(cl, cl_max_to, true).unwrap_or(f64::NAN),
            0.0,
            n_engines,
            gradient,
            cl,
            0.0,
            cl_max_to,
            condition.map(|condition| self.engine_out_condition(condition)),
        )
    }

    /// What the report must state about the inputs of the locus: an undeclared
    /// slat state, a Krueger-only leading edge, an engine position without a
    /// stated reduction. Empty when the source covers the configuration.
    pub fn notes(&self) -> Vec<String> {
        const SOURCE: &str = "V2 climb-out drag locus (Torenbeek 1982, Eq. 5-29)";
        let mut notes = Vec::new();
        match self.slats {
            SlatDeclaration::Slats | SlatDeclaration::NoSlats => {}
            SlatDeclaration::KruegerOnly => notes.push(format!(
                "the leading edge carries Krueger flaps only, which the {SOURCE} does not name: \
                 its pair for no slats (C0 {V2_LOCUS_INTERCEPT}, E \
                 {TAKEOFF_AIRPLANE_OSWALD_FACTOR}) is applied as printed"
            )),
            SlatDeclaration::Undeclared => notes.push(format!(
                "no leading-edge device is stated (the high-lift chord extension is the class \
                 default and the preset declares no high-lift system): the {SOURCE} is taken as \
                 the higher of its slats-extended pair (C0 {V2_LOCUS_INTERCEPT_SLATS}, E \
                 {TAKEOFF_AIRPLANE_OSWALD_FACTOR_SLATS:.2}) and its no-slat pair (C0 \
                 {V2_LOCUS_INTERCEPT}, E {TAKEOFF_AIRPLANE_OSWALD_FACTOR}) at the flown lift \
                 coefficient; state the slat chord extension (zero without slats) to select one"
            )),
        }
        if self.engines == EngineFailureClass::Unclassified {
            notes.push(format!(
                "the {SOURCE} states the engine-failure reduction of E for wing-mounted engines \
                 (4 %) and for engines on either side of the rear fuselage (2 %) only; the \
                 critical engine of this installation is in neither class: no reduction is \
                 applied, and the asymmetric-trim drag of the engine-out model is added instead"
            ));
        }
        notes
    }
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn polar(slats: SlatDeclaration, engines: EngineFailureClass) -> V2ClimbOutPolar {
        V2ClimbOutPolar {
            slats,
            engines,
            aspect_ratio: 9.0,
        }
    }

    /// Eq. 5-29 by hand at A = 9, CL = 1.5 (CL^2 = 2.25, pi A = 28.274334):
    /// slats 0.018 + 2.25 / (28.274334 x 0.70) = 0.018 + 0.113682 = 0.131682;
    /// no slats 0.005 + 2.25 / (28.274334 x 0.61) = 0.005 + 0.130455 = 0.135455.
    #[test]
    fn the_locus_is_equation_5_29_for_both_slat_states() {
        let wing = EngineFailureClass::WingMounted;
        let slats = polar(SlatDeclaration::Slats, wing)
            .cd(1.5, 2.2, false)
            .unwrap();
        let none = polar(SlatDeclaration::NoSlats, wing)
            .cd(1.5, 2.2, false)
            .unwrap();
        assert!((slats - 0.131_682).abs() < 5.0e-7, "{slats}");
        assert!((none - 0.135_455).abs() < 5.0e-7, "{none}");
        // A Krueger-only leading edge reads the no-slat pair as printed; an
        // undeclared one the higher of the two at the lift coefficient.
        let krueger = polar(SlatDeclaration::KruegerOnly, wing);
        assert_eq!(
            krueger.cd(1.5, 2.2, false).unwrap().to_bits(),
            none.to_bits()
        );
        let undeclared = polar(SlatDeclaration::Undeclared, wing);
        assert_eq!(
            undeclared.cd(1.5, 2.2, false).unwrap().to_bits(),
            none.to_bits()
        );
        // Below CL^2 = 0.013 pi A / (1/0.61 - 1/0.70) the slat pair is the higher.
        let low_slats = polar(SlatDeclaration::Slats, wing)
            .cd(0.9, 2.2, false)
            .unwrap();
        assert_eq!(
            undeclared.cd(0.9, 2.2, false).unwrap().to_bits(),
            low_slats.to_bits()
        );
        assert_eq!(undeclared.notes().len(), 1);
        assert_eq!(krueger.notes().len(), 1);
        assert!(polar(SlatDeclaration::Slats, wing).notes().is_empty());
    }

    /// Engine failure, same point: wing-mounted E = 0.70 x 0.96 = 0.672,
    /// 0.018 + 2.25 / (28.274334 x 0.672) = 0.018 + 0.118419 = 0.136419;
    /// rear fuselage, no slats, E = 0.61 x 0.98 = 0.5978,
    /// 0.005 + 2.25 / (28.274334 x 0.5978) = 0.005 + 0.133117 = 0.138117.
    #[test]
    fn an_engine_failure_lowers_e_by_the_engine_position() {
        let wing = polar(SlatDeclaration::Slats, EngineFailureClass::WingMounted);
        let rear = polar(SlatDeclaration::NoSlats, EngineFailureClass::RearFuselage);
        let other = polar(SlatDeclaration::Slats, EngineFailureClass::Unclassified);
        assert!((wing.cd(1.5, 2.2, true).unwrap() - 0.136_419).abs() < 5.0e-7);
        assert!((rear.cd(1.5, 2.2, true).unwrap() - 0.138_117).abs() < 5.0e-7);
        // No class, no reduction: the all-engine locus, and a note saying so.
        assert_eq!(
            other.cd(1.5, 2.2, true).unwrap().to_bits(),
            other.cd(1.5, 2.2, false).unwrap().to_bits()
        );
        assert_eq!(other.notes().len(), 1);
        assert!(wing.notes().is_empty() && rear.notes().is_empty());

        // The separate trim increment is zero exactly where E carries the
        // failure; the drag of the dead engine stands everywhere.
        let condition = OeiV2Condition {
            departure_elevation_m: 0.0,
            departure_isa_deviation_c: 0.0,
            v2_over_vstall: 1.13,
            condition_to_sls_thrust_ratio: Some(0.8),
            asymmetric_trim_cd: Some(0.004),
            windmilling_cd: Some(0.003),
        };
        let in_e = wing.engine_out_condition(condition);
        assert_eq!(in_e.asymmetric_trim_cd, Some(0.0));
        assert_eq!(in_e.windmilling_cd, Some(0.003));
        assert_eq!(in_e.condition_to_sls_thrust_ratio, Some(0.8));
        assert_eq!(other.engine_out_condition(condition), condition);

        // Twin, 2.4 % gradient, CL_V2 = 2.2 / 1.13^2: the requirement is
        // 2 (gradient + (CD + windmilling) / CL) over the thrust ratio.
        let cl = 2.2 / (1.13 * 1.13);
        let cd = wing.cd(cl, 2.2, true).unwrap();
        let assessment = wing.assess(2, 0.024, cl, 2.2, Some(condition));
        let required = assessment.required_sls_tw.unwrap();
        assert!((required - 2.0 * (0.024 + (cd + 0.003) / cl) / 0.8).abs() < 1.0e-9);
    }

    /// Above the take-off maximum lift the locus has no value, and the
    /// assessment built on it is an evidence gap, not a number.
    #[test]
    fn the_locus_is_not_read_outside_its_domain() {
        let wing = polar(SlatDeclaration::Slats, EngineFailureClass::WingMounted);
        assert!(wing.cd(2.2, 2.2, true).is_ok());
        assert!(wing.cd(2.3, 2.2, true).is_err());
        assert!(wing.cd(f64::NAN, 2.2, true).is_err());
        assert!(wing.cd(-0.1, 2.2, true).is_err());
        let flat = V2ClimbOutPolar {
            aspect_ratio: 0.0,
            ..wing
        };
        assert!(flat.cd(1.5, 2.2, true).is_err());
        let gap = wing.assess(2, 0.024, 2.3, 2.2, None);
        assert!(gap.required_inflight_tw.is_none() && gap.required_sls_tw.is_none());
    }

    /// The locus span is the overall projected span: a wingtip device adds
    /// its lateral reach on both sides, a plain wing keeps `b^2 / S`.
    #[test]
    fn the_locus_aspect_ratio_counts_the_wingtip_reach() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A350-900"}))
            .unwrap_or_else(|error| panic!("A350 config: {error}"));
        let device = config
            .geometry
            .wing
            .wingtip_device
            .as_ref()
            .unwrap_or_else(|| panic!("A350 wingtip device"));
        let reach = device.lateral_reach_m();
        assert!(reach > 0.5, "A350 reach {reach}");
        let (span, area) = (62.93, 445.0);
        let overall = span + 2.0 * reach;
        let ar = climb_out_aspect_ratio(&config, span, area);
        assert!((ar - overall * overall / area).abs() < 1e-12);
        let mut plain = config.clone();
        plain.geometry.wing.wingtip_device = None;
        assert!((climb_out_aspect_ratio(&plain, span, area) - span * span / area).abs() < 1e-12);
    }
}
