// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ground plane of a body that declares no belly clearance: the documented
//! clearance samples of the aircraft classes and the ends a consumer judges at.

use alas_config::LandingGearConfig;

/// Fuselage-bottom height above the static ground line, as a fraction of the
/// fuselage diameter, used when a preset registers no clearance (a user
/// aircraft), for a jet. A turboprop uses the turboprop sample below.
///
/// Jet sample: the nine registered jets whose airport-planning documents give
/// the fuselage-bottom (or equivalent) clearance, over the preset fuselage
/// diameter: E195-E2 0.546, A220-300 0.475, A320-200 0.453, A340-300 0.378,
/// DC-10 0.375, C919 0.369, A380-800 0.333, B787-9 0.303, B747-400 0.298
/// (mean 0.392; the diameters of the double-deck A380 and the 747 are widths).
/// 0.35 D lies between the third and fourth lowest; six of the nine sit above
/// it, by up to 0.196 D.
///
/// Which way a wrong clearance errs depends on the row, and on the sign of what
/// the plane moves, so no single end of the sample is conservative for a
/// design. A consumer that judges a design with an undeclared clearance
/// evaluates both ends ([`GroundPlaneBasis::Low`], [`GroundPlaneBasis::High`])
/// and keeps the worse result:
/// - nacelle, wing tip and propeller gaps: a small clearance is conservative;
/// - tail gap: the scrape pitch about the main gear grows with the clearance,
///   which drops the aft surfaces, so a large clearance is conservative (the
///   AVE tail gap is 2.18 m at 0.35 D, 2.43 m at 0.298 D, 1.98 m at the declared 2.44 m);
/// - tail scrape angle against the rotation angle: a small clearance gives a
///   small angle, conservative;
/// - tip-back aft boundary, turnover angle and the scrape angle as the
///   tip-back requirement: a large clearance raises the CG and the
///   requirement, conservative (the E195-E2 boundary is 28.05 % MAC at 1.06 m,
///   23.84 % at 1.65 m);
/// - gear mass of a body whose leg is the clearance (sponson gear): a large
///   clearance gives a heavier gear (FLOPS mass goes with the leg length to
///   the power 0.43), conservative for OEW.
///
/// 0.35 D is the nominal value (reports, drawings); it judges nothing.
pub const FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION: f64 = 0.35;

/// The smallest documented jet clearance over diameter (B747-400, 0.298).
pub const FALLBACK_BELLY_CLEARANCE_LOW_FRACTION: f64 = 0.298;

/// The largest documented jet clearance over diameter (E195-E2, 0.546).
pub const FALLBACK_BELLY_CLEARANCE_HIGH_FRACTION: f64 = 0.546;

/// Turboprop sample: the two registered high-wing turboprops with sponson
/// main gear whose documents give the belly clearance, over fuselage
/// diameter: A400M 1.21/5.60 = 0.216 and ATR 72-600 0.64/2.865 = 0.223.
/// Two aircraft of one layout do not cover a turboprop of another layout (low
/// wing, nacelle gear): such a body takes the full documented sample, all
/// eleven jets and sponson turboprops, 0.216-0.546 D, at both ends and is
/// flagged ([`ClearanceSample::OtherTurboprop`]); a declared clearance replaces
/// it. The nominal value of the sponson sample is the mean.
pub const TURBOPROP_BELLY_CLEARANCE_NOMINAL_FRACTION: f64 = 0.22;
/// The smaller turboprop sample clearance over diameter (A400M).
pub const TURBOPROP_BELLY_CLEARANCE_LOW_FRACTION: f64 = 0.216;
/// The larger turboprop sample clearance over diameter (ATR 72-600).
pub const TURBOPROP_BELLY_CLEARANCE_HIGH_FRACTION: f64 = 0.223;

/// Which documented clearance sample an aircraft without a declared clearance
/// is judged on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearanceSample {
    /// A jet: the nine documented jets, 0.298-0.546 D.
    Jet,
    /// A turboprop in the layout of its sample (wing root above the crown, main
    /// wheels inboard of the engines): A400M and ATR 72-600, 0.216-0.223 D.
    SponsonTurboprop,
    /// A turboprop of another layout, which has no sample of its own: the full
    /// documented sample of the eleven jets and sponson turboprops, 0.216-0.546
    /// D, an assumption the caller should flag.
    OtherTurboprop,
}

/// Which end of the documented clearance sample of the aircraft class stands
/// in for an undeclared `fuselage_ground_clearance_m`. A declared clearance is
/// the same for every basis; only the fallback differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundPlaneBasis {
    /// The class nominal value: reports, drawings, anything that judges nothing.
    Nominal,
    /// The small end of the class sample (jets 0.298 D, turboprops 0.216 D).
    Low,
    /// The large end of the class sample (jets 0.546 D, turboprops 0.223 D).
    High,
}

impl GroundPlaneBasis {
    /// The two ends, for a consumer that keeps the worse of both.
    pub const ENDS: [Self; 2] = [Self::Low, Self::High];

    pub(super) fn fallback_fraction(self, sample: ClearanceSample) -> f64 {
        use ClearanceSample::{Jet, OtherTurboprop, SponsonTurboprop};
        match (self, sample) {
            (Self::Nominal, Jet | OtherTurboprop) => FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION,
            (Self::Nominal, SponsonTurboprop) => TURBOPROP_BELLY_CLEARANCE_NOMINAL_FRACTION,
            (Self::Low, Jet) => FALLBACK_BELLY_CLEARANCE_LOW_FRACTION,
            (Self::Low, SponsonTurboprop | OtherTurboprop) => {
                TURBOPROP_BELLY_CLEARANCE_LOW_FRACTION
            }
            (Self::High, Jet | OtherTurboprop) => FALLBACK_BELLY_CLEARANCE_HIGH_FRACTION,
            (Self::High, SponsonTurboprop) => TURBOPROP_BELLY_CLEARANCE_HIGH_FRACTION,
        }
    }
}

/// The bases a consumer that judges a design must evaluate: only
/// [`GroundPlaneBasis::Nominal`] (the declared plane) when the clearance is
/// declared, both ends of the class sample when it is not.
pub fn ground_plane_bases(landing_gear: &LandingGearConfig) -> &'static [GroundPlaneBasis] {
    if landing_gear
        .fuselage_ground_clearance_m
        .is_some_and(|v| v.is_finite() && v > 0.0)
    {
        &[GroundPlaneBasis::Nominal]
    } else {
        &GroundPlaneBasis::ENDS
    }
}
