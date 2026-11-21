// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CG-limit mechanisms of the load-and-trim sheet, one line each.
//!
//! The model computes every mechanism's own boundary per loading state (see
//! `alas_opt::envelope::PhysicalCgLimits`). The sheet draws each as a line
//! over mass, and the operational envelope is, at each mass, the most
//! restrictive of the lines the gate applies there:
//!
//! - nose-gear load (forward), minimum nose load and tip-back (aft): ground
//!   mechanisms, applied to every state;
//! - rotation (forward) and the static-margin floor (aft): takeoff;
//! - clean climb trim (forward, CS 25.161(c)(1) with the flaps retracted):
//!   every airborne state, so from the lightest takeoff, flight or landing
//!   state upward;
//! - landing trim (forward): flight and landing states at or below the
//!   maximum landing mass. The gate applies it neither to the takeoff state
//!   nor to an en-route state above that mass (which cannot land), so the
//!   line stops at the heaviest state whose scope keeps it and the envelope
//!   above it follows the takeoff scope. A line is held at its end values
//!   below its first state, as a load sheet's allowed zone is.

use alas_opt::envelope::{ForwardLimitGovernance, ModelCgLoadingAssessment};

use super::{GatePhase, LimitVertex};

/// One CG-limit mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// Maximum nose-gear load (forward, ground).
    NoseGearLoad,
    /// Nose-wheel liftoff at rotation (forward, takeoff).
    Rotation,
    /// Clean climb trim (forward, every airborne state).
    CleanTrim,
    /// Landing trim and flare (forward, flight and landing).
    LandingTrim,
    /// Static-margin floor (aft, takeoff, flight and landing).
    StaticMargin,
    /// Minimum nose-gear load (aft, ground).
    MinNoseLoad,
    /// Tip-back on the main gear (aft, ground).
    TipBack,
}

impl Mechanism {
    /// Forward lines first, then aft, in the order the sheet labels them.
    pub const ALL: [Self; 7] = [
        Self::NoseGearLoad,
        Self::Rotation,
        Self::CleanTrim,
        Self::LandingTrim,
        Self::StaticMargin,
        Self::MinNoseLoad,
        Self::TipBack,
    ];

    /// Whether this is a forward limit.
    #[must_use]
    pub const fn is_forward(self) -> bool {
        matches!(
            self,
            Self::NoseGearLoad | Self::Rotation | Self::CleanTrim | Self::LandingTrim
        )
    }

    /// The label drawn at the line. Static strings so the desktop catalog
    /// can translate them.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoseGearLoad => "nose-gear load",
            Self::Rotation => "rotation",
            Self::CleanTrim => "clean trim",
            Self::LandingTrim => "landing trim",
            Self::StaticMargin => "static margin",
            Self::MinNoseLoad => "min nose load",
            Self::TipBack => "tip-back",
        }
    }

    /// The gate phase whose limit-set style the line keeps when faint.
    #[must_use]
    pub const fn phase(self) -> GatePhase {
        match self {
            Self::NoseGearLoad | Self::MinNoseLoad | Self::TipBack => GatePhase::Ground,
            Self::Rotation => GatePhase::Takeoff,
            Self::StaticMargin | Self::CleanTrim => GatePhase::Flight,
            Self::LandingTrim => GatePhase::Landing,
        }
    }

    /// The line of the mechanism the gate reports as `governance`. Exhaustive,
    /// so a new forward mechanism must be given its own line before it builds.
    #[must_use]
    pub const fn of_forward_governance(governance: ForwardLimitGovernance) -> Self {
        match governance {
            ForwardLimitGovernance::MaxNoseLoadHandling => Self::NoseGearLoad,
            ForwardLimitGovernance::RotationNoseWheelLiftoff
            | ForwardLimitGovernance::TakeoffTrimStabiliserStop => Self::Rotation,
            ForwardLimitGovernance::CleanClimbTrim => Self::CleanTrim,
            ForwardLimitGovernance::ScissorPlotEstimate
            | ForwardLimitGovernance::LandingTrimDesignRule
            | ForwardLimitGovernance::LandingTrimSourcedTravel => Self::LandingTrim,
        }
    }

    fn value(self, state: &ModelCgLoadingAssessment) -> f64 {
        let p = &state.physical_limits;
        match self {
            Self::NoseGearLoad => p.max_nose_load_fwd_pct_mac,
            Self::Rotation => p.rotation_fwd_pct_mac,
            Self::CleanTrim => p.clean_trim_fwd_pct_mac,
            Self::LandingTrim => p.landing_trim_fwd_pct_mac,
            Self::StaticMargin => p.aerodynamic_aft_pct_mac,
            Self::MinNoseLoad => p.ground_aft_pct_mac,
            Self::TipBack => p.tip_back_aft_pct_mac,
        }
    }
}

/// One mechanism's boundary over mass.
#[derive(Debug, Clone, PartialEq)]
pub struct MechanismLine {
    /// The mechanism.
    pub mechanism: Mechanism,
    /// (mass kg, %MAC) at each loading state, ascending mass; held at the
    /// end values beyond them.
    pub points: Vec<(f64, f64)>,
    /// Mass interval, kg, over which the gate applies the mechanism.
    pub applies_kg: (f64, f64),
}

impl MechanismLine {
    /// The boundary, %MAC, at `mass_kg`.
    #[must_use]
    pub fn pct_at(&self, mass_kg: f64) -> f64 {
        let vertices: Vec<LimitVertex> = self
            .points
            .iter()
            .map(|&(mass_kg, pct)| LimitVertex {
                mass_kg,
                fwd_pct_mac: pct,
                aft_pct_mac: pct,
            })
            .collect();
        super::limit_at(&vertices, mass_kg, true)
    }

    /// Whether the gate applies the mechanism at `mass_kg`.
    #[must_use]
    pub fn applies_at(&self, mass_kg: f64) -> bool {
        mass_kg >= self.applies_kg.0 - 1e-6 && mass_kg <= self.applies_kg.1 + 1e-6
    }
}

/// A published CG limit, one side, as (mass kg, %MAC) corners.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedLine {
    /// Source and edition, as the legend prints it.
    pub source: String,
    /// The full citation (document, section or table, page), exported with
    /// the plot data.
    pub reference: String,
    /// Whether this is the forward limit.
    pub forward: bool,
    /// Corners, ascending mass; the line is drawn between the first and last.
    pub points: Vec<(f64, f64)>,
}

/// The lines the sheet draws beyond the four limit sets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CgLines {
    /// The model's mechanism lines; empty for hand-built data, which draws
    /// its four limit sets instead.
    pub mechanisms: Vec<MechanismLine>,
    /// A sourced published envelope, drawn only as a comparison.
    pub published: Vec<PublishedLine>,
}

impl CgLines {
    /// The most restrictive applicable boundary of one side at `mass_kg`:
    /// the largest forward or the smallest aft value.
    #[must_use]
    pub fn envelope_at(&self, mass_kg: f64, forward: bool) -> f64 {
        let values = self
            .mechanisms
            .iter()
            .filter(|l| l.mechanism.is_forward() == forward && l.applies_at(mass_kg))
            .map(|l| l.pct_at(mass_kg))
            .filter(|v| v.is_finite());
        if forward {
            values.fold(f64::NEG_INFINITY, f64::max)
        } else {
            values.fold(f64::INFINITY, f64::min)
        }
    }

    /// Whether `line` is the operational boundary at `mass_kg`.
    #[must_use]
    pub fn governs(&self, line: &MechanismLine, mass_kg: f64) -> bool {
        line.applies_at(mass_kg)
            && (line.pct_at(mass_kg) - self.envelope_at(mass_kg, line.mechanism.is_forward())).abs()
                < 1e-6
    }

    /// Whether `line` is the operational boundary anywhere in
    /// `[lo_kg, hi_kg]`.
    #[must_use]
    pub fn governs_within(&self, line: &MechanismLine, lo_kg: f64, hi_kg: f64) -> bool {
        sample_masses(self, lo_kg, hi_kg)
            .into_iter()
            .any(|m| self.governs(line, m))
    }

    /// Masses where an applicability band ends, kg.
    pub(super) fn band_edges(&self) -> Vec<f64> {
        self.mechanisms
            .iter()
            .flat_map(|l| [l.applies_kg.0, l.applies_kg.1])
            .filter(|m| m.is_finite())
            .collect()
    }

    /// The operational envelope over `[lo_kg, hi_kg]`, ascending mass.
    #[must_use]
    pub fn operational(&self, lo_kg: f64, hi_kg: f64) -> Vec<LimitVertex> {
        sample_masses(self, lo_kg, hi_kg)
            .into_iter()
            .map(|mass_kg| LimitVertex {
                mass_kg,
                fwd_pct_mac: self.envelope_at(mass_kg, true),
                aft_pct_mac: self.envelope_at(mass_kg, false),
            })
            .filter(|v| v.fwd_pct_mac.is_finite() && v.aft_pct_mac.is_finite())
            .collect()
    }
}

/// Sample masses over `[lo_kg, hi_kg]`: an even grid, every state mass, and
/// each side of every band edge (so a step is kept as a step).
pub(super) fn sample_masses(lines: &CgLines, lo_kg: f64, hi_kg: f64) -> Vec<f64> {
    let n = 120;
    let mut masses: Vec<f64> = (0..=n)
        .map(|k| lo_kg + (hi_kg - lo_kg) * f64::from(k) / f64::from(n))
        .collect();
    for line in &lines.mechanisms {
        masses.extend(line.points.iter().map(|p| p.0));
    }
    for edge in lines.band_edges() {
        masses.extend([edge, edge + 0.5]);
    }
    masses.retain(|m| *m >= lo_kg && *m <= hi_kg);
    masses.sort_by(f64::total_cmp);
    masses.dedup_by(|a, b| (*a - *b).abs() < 0.25);
    masses
}

/// The mechanism lines of the gate's loading `states`. A mechanism the model
/// does not compute (no finite value at any state) has no line.
///
/// `takeoff_kg` ends the takeoff scope; `landing_kg` is the maximum landing
/// mass the landing-trim scope runs up to at least, and beyond it to the
/// heaviest state whose own scope keeps landing trim (an overweight landing).
pub(super) fn mechanism_lines(
    states: &[&ModelCgLoadingAssessment],
    takeoff_kg: f64,
    landing_kg: f64,
) -> Vec<MechanismLine> {
    let flight_top = states
        .iter()
        .filter(|s| s.scope.landing_trim)
        .map(|s| s.mass_kg)
        .fold(landing_kg, f64::max);
    let airborne_bottom = states
        .iter()
        .filter(|s| GatePhase::of(s.state) != GatePhase::Ground)
        .map(|s| s.mass_kg)
        .fold(f64::INFINITY, f64::min);
    Mechanism::ALL
        .into_iter()
        .filter_map(|mechanism| {
            let mut points: Vec<(f64, f64)> = states
                .iter()
                .map(|s| (s.mass_kg, mechanism.value(s)))
                .filter(|(m, v)| m.is_finite() && v.is_finite())
                .collect();
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            points.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);
            let applies_kg = match mechanism {
                Mechanism::Rotation => (f64::NEG_INFINITY, takeoff_kg),
                Mechanism::LandingTrim => (f64::NEG_INFINITY, flight_top),
                Mechanism::CleanTrim => (airborne_bottom, f64::INFINITY),
                _ => (f64::NEG_INFINITY, f64::INFINITY),
            };
            (!points.is_empty()).then_some(MechanismLine {
                mechanism,
                points,
                applies_kg,
            })
        })
        .collect()
}
