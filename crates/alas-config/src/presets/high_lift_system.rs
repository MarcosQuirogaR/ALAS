// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off CLmax of a registered jet from its declared high-lift system.
//!
//! The class defaults of `performance_presets` give every modern jet the same
//! `CLmax_TO` regardless of its flap type, flapped span or sweep. Here the
//! coefficient is built from the declared system instead, by the handbook
//! method of Raymer, *Aircraft Design: A Conceptual Approach*, 5th ed., Sec.
//! 12.4 (printed p. 415, Eq. 12.21 and Table 12.2; PDF p. 449 of the library
//! copy), with the same expression in Jenkinson, *Civil Jet Aircraft Design*,
//! Ch. 8 (PDF pp. 179-183):
//!
//! ```text
//! CLmax_TO = CLmax_clean + f_TO dCL_flap(landing) + dCL_LE
//! dCL_flap = 0.9 dcl_max (S_flapped / S_ref) cos(Lambda_hinge)
//! dcl_max  = k_device c'/c      k = 1.3 Fowler, 1.6 double slotted, 1.9 triple
//!                               slotted
//! dCL_LE   = CLmax(slats out, flaps up) - CLmax_clean
//! ```
//!
//! The leading-edge term is the measured slats-out, flaps-up level of the same wing
//! less its measured clean level, else the pooled flight-test laws of
//! [`super::clean_lift`], `(1.97 - 1.43) cos(Lambda_c/4)`: Obert's Figs. 26.12-26.16
//! (printed pp. 324-325) give the slats-out less slats-in increments 0.47, 0.42, 0.42,
//! 0.62, 0.31, 0.68 and 0.43 for the 737-300, 757, 767, A320, A330, 737-800 and 777,
//! mean 0.48, with the slats-out points plotted at flap 1 taken to flap 0. Roskam,
//! *Airplane Design Part VI*, Sec. 8.1.4.3 B, Eq. 8.30, printed p. 262,
//! `7.11 (c_f/c) (b_lef/b_e)^2 cos^2(Lambda_c/4)` for "slats and/or Kruegers", gives
//! 0.48 to 0.69 on the declared runs (span ratios near 0.9, not the 0.8 of its worked
//! A320), mean 0.59, 24 % above that mean; on the two wings measured by Obert it
//! gives A320 0.674 against 0.62 and A340 (the A330 wing) 0.685 against 0.31. Added to
//! a measured clean value it counts twice what a wing with a high clean level does not
//! gain, so it is not used for the level. Its geometry form stays the law by which a
//! candidate wing scales the leading-edge part (`alas_perf::performance::clmax`). It
//! covers Kruegers, so no Krueger factor is applied.
//!
//! Constants, none fitted to an aircraft or a field length:
//! - `f_TO`, the share of the landing flap increment at the take-off flap
//!   setting. Where the take-off and landing flap deflections are declared
//!   (`takeoff_flap`) it is read from the pooled measured `CLmax,1g`-against-
//!   flap-angle curve of [`super::flap_effectiveness::increment_share`]
//!   (Obert, Figs. 26.13-26.16). Otherwise it is `0.70`, the midpoint of "lift
//!   increments of about 60-80% of these values should be used" (Raymer p.
//!   415; Jenkinson PDF p. 183 "For take-off values use 60 to 80%"), which is
//!   about the pooled curve at half the landing deflection. The slat is kept
//!   at its full value, as in Raymer's worked A320 (2.29 take-off).
//! - The landing flap increment is the Raymer handbook value, unless the
//!   preset has a published approach speed (`landing_reference`): then it is
//!   that speed's recovered coefficient minus the slats-out, flaps-up
//!   coefficient `clean + dCL_LE`. The handbook landing value misses the
//!   Vref-recovered coefficient by -5 to +34 % (clmax report) and the
//!   measured 1g landing coefficients of the same plots are 2.15-2.6.
//! - `c'/c = 1.275` Fowler chord extension, the midpoint of "typical values are
//!   1.25-1.30" (Jenkinson PDF p. 183).
//! - Flap and slat spans and chord fractions are the declared
//!   [`ControlSurfacesConfig`] values the vortex-lattice geometry and the
//!   `CLmax` geometry scaling already read; the flap hinge line sits at
//!   `(1 - c_f/c)` of the local chord (Jenkinson p. 183).
//! - The clean value is the configuration's own `cl_max_clean`, set by
//!   [`super::clean_lift`] from measured slats-in values.
//!
//! This is a conceptual-design estimate (Raymer: "if at all possible, it
//! should be calibrated with test data on an actual airplane with similar flap
//! geometry"). It is not a measured CLmax. Landing coefficients of the same
//! method are compared with the Vref-recovered `cl_max_land` of each preset in
//! the tests of `alas-perf` and in the fix report.

use crate::control_surfaces::ControlSurfacesConfig;
use crate::PerformanceConfig;
use crate::TransportPlanform;

use super::{published_takeoff_reference, AircraftPreset};

/// Trailing-edge device row of Raymer Table 12.2 (per unit extended chord ratio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailingEdgeDevice {
    /// Fowler flap, single slot: `1.3 c'/c`.
    FowlerSingleSlotted,
    /// Double-slotted flap (Fowler action): `1.6 c'/c`.
    DoubleSlotted,
    /// Triple-slotted flap (Fowler action): `1.9 c'/c`.
    TripleSlotted,
}

impl TrailingEdgeDevice {
    /// Raymer Table 12.2 section increment per unit `c'/c`, landing setting.
    pub const fn section_increment_per_chord_ratio(self) -> f64 {
        match self {
            Self::FowlerSingleSlotted => 1.3,
            Self::DoubleSlotted => 1.6,
            Self::TripleSlotted => 1.9,
        }
    }
}

/// CS 25.125: Vref not less than 1.23 VSR0.
const VREF_OVER_VS1G: f64 = 1.23;
/// Take-off flap increment as a fraction of the landing increment when the
/// deflection is not declared (Raymer p. 415, 60-80 %).
pub const TAKEOFF_FLAP_FRACTION: f64 = 0.70;
/// Fowler chord extension `c'/c` (Jenkinson PDF p. 183, 1.25-1.30).
pub const FOWLER_CHORD_RATIO: f64 = 1.275;
/// Slat chord extension `c'/c` (Jenkinson printed pp. 167-168, PDF pp. 182-183, 1.05-1.10).
pub const SLAT_CHORD_RATIO: f64 = 1.075;

/// Take-off flap deflection of the reference field length, with the landing
/// deflection that the landing coefficient belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffFlap {
    /// Trailing-edge flap deflection of the take-off setting, degrees.
    pub takeoff_deg: f64,
    /// Largest certified landing flap deflection, degrees.
    pub landing_deg: f64,
    /// Where each deflection is stated.
    pub basis: &'static str,
    /// The other sourced take-off deflections of the flight manual, degrees,
    /// when the reference field length names no setting. ICAO Annex 14 defines
    /// the aeroplane reference field length as the minimum field length at
    /// maximum take-off mass, sea level, ISA, still air; the flight manual
    /// offers a setting only where it meets the CS 25.121(b) second-segment
    /// gradient, so the ISA sea-level reference takes the shortest field over
    /// `takeoff_deg` and these among the settings that meet it. Empty when the
    /// reference states its setting.
    pub other_takeoff_deg: &'static [f64],
}

impl TakeoffFlap {
    /// Share of the landing flap increment at the take-off deflection.
    pub fn increment_share(&self) -> Option<f64> {
        super::flap_effectiveness::increment_share(self.takeoff_deg / self.landing_deg)
    }
}

/// A declared high-lift system with its document basis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeclaredHighLift {
    /// Registered aircraft.
    pub preset_name: &'static str,
    /// Trailing-edge device.
    pub trailing_edge: TrailingEdgeDevice,
    /// Share of the leading-edge device span that is Krueger flap, not slat.
    pub krueger_share: f64,
    /// Take-off and landing flap deflections when the reference states them.
    pub takeoff_flap: Option<TakeoffFlap>,
    /// Document, page and the facts taken from it.
    pub source: &'static str,
    /// Evidence gaps and assumptions.
    pub applicability: &'static str,
}

/// The wing quantities the method reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighLiftGeometry {
    /// Flapped wing area (full chord over the flap span, both sides) over `S_ref`.
    pub flapped_area_ratio: f64,
    /// Flap hinge-line sweep, degrees.
    pub hinge_sweep_deg: f64,
    /// Leading-edge device chord over wing chord, `c_f/c` of Roskam VI Fig. 8.56.
    pub slat_chord_ratio: f64,
    /// Leading-edge device span over the exposed span, `b_lef/b_e` of Roskam VI Fig. 8.57.
    pub slat_span_ratio: f64,
    /// Quarter-chord sweep of the outboard wing, degrees.
    pub sweep_c4_deg: f64,
}

/// The coefficients the method returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighLiftBreakdown {
    /// Clean wing.
    pub clean: f64,
    /// Trailing-edge increment at the landing setting.
    pub flap_landing: f64,
    /// Leading-edge increment: slats out, flaps up, less clean.
    pub leading_edge: f64,
    /// Landing coefficient the flap increment is anchored to: the published-speed one,
    /// else the pooled slatted-landing law of [`super::clean_lift`].
    pub landing_anchor: Option<f64>,
    /// Share of the landing flap increment at the take-off setting.
    pub takeoff_share: f64,
}

impl HighLiftBreakdown {
    /// Flap increment at the landing setting used for the take-off value: the
    /// published-speed coefficient less the slats-out, flaps-up coefficient,
    /// else the handbook increment.
    pub fn flap_increment(&self) -> f64 {
        self.landing_anchor.map_or(self.flap_landing, |anchor| {
            (anchor - self.clean - self.leading_edge).max(0.0)
        })
    }

    /// `CLmax` at the take-off flap setting.
    pub fn takeoff(&self) -> f64 {
        self.clean + self.takeoff_share * self.flap_increment() + self.leading_edge
    }

    /// Handbook `CLmax` at the landing flap setting (a cross-check against Vref).
    pub fn landing(&self) -> f64 {
        self.clean + self.flap_landing + self.leading_edge
    }
}

// Measured effect of the trailing-edge breaks on the take-off CLmax parts (diagnostic
// `te_break_highlift_probe`, release, break on and off): A340-300 CLmax_TO 2.3491 ->
// 2.3488 (-0.0003), flap increment 0.7769 -> 0.7782, slat 0.2750 -> 0.2737; A380-800 2.1000 -> 2.1001
// (+0.0001). S_drawn (the reference area measure() divides by) moves A340-300 383.553 -> 374.236 m^2
// (-2.4 %), A380-800 845.000 -> 847.295 m^2. The CLmax change is small; the A340 area loss is not, and
// the take-off field length was not re-run (no run in this sprint). Open: the A340 break removes 9.3 m^2
// from the drawn planform against the published 383.6 m^2.
fn station_at(planform: &TransportPlanform, y_m: f64) -> Option<(f64, f64)> {
    let stations = planform.stations_with_rake();
    stations.windows(2).find_map(|pair| {
        let (y0, y1) = (pair[0].y_m, pair[1].y_m);
        (y1 > y0 && y_m >= y0 - 1e-9 && y_m <= y1 + 1e-9).then(|| {
            let t = ((y_m - y0) / (y1 - y0)).clamp(0.0, 1.0);
            (
                pair[0].leading_edge_x_m
                    + t * (pair[1].leading_edge_x_m - pair[0].leading_edge_x_m),
                pair[0].chord_m + t * (pair[1].chord_m - pair[0].chord_m),
            )
        })
    })
}

/// Full-chord wing area between `y_in` and `y_out` on both semispans, m^2.
fn chord_area(planform: &TransportPlanform, y_in: f64, y_out: f64) -> f64 {
    let stations = planform.stations_with_rake();
    let mut area = 0.0;
    for pair in stations.windows(2) {
        let (y0, y1) = (pair[0].y_m, pair[1].y_m);
        let (lo, hi) = (y0.max(y_in), y1.min(y_out));
        if y1 > y0 && hi > lo {
            let chord = |y: f64| {
                pair[0].chord_m + (y - y0) / (y1 - y0) * (pair[1].chord_m - pair[0].chord_m)
            };
            area += (chord(lo) + chord(hi)) * (hi - lo);
        }
    }
    area
}

/// Measure the declared flap and slat runs on a planform, without its blends: no
/// flap or slat sits in a fillet. The area ratios are over the reference area of that
/// same clean planform, which the blend does not move, not over the aircraft's quoted
/// `S_ref`.
pub fn measure(
    planform: &TransportPlanform,
    controls: &ControlSurfacesConfig,
) -> Option<HighLiftGeometry> {
    let planform = &planform.without_blends();
    let semi_span = planform.tip.y_m;
    let s_ref = planform.reference_area_m2();
    if !(semi_span > 0.0 && s_ref > 0.0) {
        return None;
    }
    let (f_in, f_out) = (
        controls.flap_span_start_frac * semi_span,
        controls.flap_span_end_frac * semi_span,
    );
    let (s_in, s_out) = (
        controls.slat_span_start_frac * semi_span,
        controls.slat_span_end_frac * semi_span,
    );
    if !(f_out > f_in && s_out > s_in) {
        return None;
    }
    let hinge = |y: f64| {
        station_at(planform, y).map(|(x_le, c)| x_le + (1.0 - controls.flap_chord_fraction) * c)
    };
    let hinge_sweep_deg = (hinge(f_out)? - hinge(f_in)?)
        .atan2(f_out - f_in)
        .to_degrees();
    // Raymer's flapped area is the wing area behind the flaps: an inboard aileron
    // between two flaps is not part of it.
    let flapped_area: f64 = controls
        .flap_bands()
        .iter()
        .map(|(start, end)| chord_area(planform, start * semi_span, end * semi_span))
        .sum();
    // No slat sits in the body: the span runs from the side of body, the root of the
    // exposed span, when the planform declares one.
    let side_of_body = planform.side_of_body.map_or(0.0, |station| station.y_m);
    let exposed = semi_span - side_of_body;
    let slat_span = s_out - s_in.max(side_of_body);
    if !(exposed > 0.0 && slat_span > 0.0) {
        return None;
    }
    Some(HighLiftGeometry {
        flapped_area_ratio: flapped_area / s_ref,
        hinge_sweep_deg,
        slat_chord_ratio: controls.slat_chord_fraction,
        slat_span_ratio: slat_span / exposed,
        sweep_c4_deg: outboard_quarter_chord_sweep_deg(planform),
    })
}

/// Quarter-chord sweep of the kink-to-tip panel, degrees: the same mean line the
/// candidate scaling of `CLmax` and the Korn relation read.
fn outboard_quarter_chord_sweep_deg(planform: &TransportPlanform) -> f64 {
    let quarter =
        |station: &crate::MainWingStation| station.leading_edge_x_m + 0.25 * station.chord_m;
    (quarter(&planform.tip) - quarter(&planform.kink))
        .atan2(planform.tip.y_m - planform.kink.y_m)
        .to_degrees()
}

/// Quarter-chord sweep of a registered preset's wing, degrees.
pub(super) fn quarter_chord_sweep_deg(preset: &AircraftPreset) -> Option<f64> {
    preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .ok()
        .map(|planform| outboard_quarter_chord_sweep_deg(&planform))
}

/// Raymer Eq. 12.21 for the declared flap on a measured wing, with the measured or
/// pooled slats-out, flaps-up level `slats_out`.
pub fn breakdown(
    clean: f64,
    slats_out: f64,
    system: &DeclaredHighLift,
    geometry: &HighLiftGeometry,
    landing_anchor: Option<f64>,
) -> HighLiftBreakdown {
    let flap_section =
        system.trailing_edge.section_increment_per_chord_ratio() * FOWLER_CHORD_RATIO;
    HighLiftBreakdown {
        clean,
        landing_anchor,
        takeoff_share: system
            .takeoff_flap
            .and_then(|flap| flap.increment_share())
            .unwrap_or(TAKEOFF_FLAP_FRACTION),
        flap_landing: 0.9
            * flap_section
            * geometry.flapped_area_ratio
            * geometry.hinge_sweep_deg.to_radians().cos(),
        leading_edge: (slats_out - clean).max(0.0),
    }
}

/// The declared systems; presets not listed keep their class default.
pub fn declared_systems() -> &'static [DeclaredHighLift] {
    &SYSTEMS
}

/// The declared system of one preset.
pub fn declared_system(name: &str) -> Option<&'static DeclaredHighLift> {
    SYSTEMS.iter().find(|system| system.preset_name == name)
}

/// Breakdown of a registered preset on its own planform, `None` when it has no
/// declared system or the planform does not resolve.
pub fn preset_breakdown(preset: &AircraftPreset) -> Option<HighLiftBreakdown> {
    let system = declared_system(preset.name)?;
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .ok()?;
    let controls = crate::preset_control_surfaces::config_for(preset.name).unwrap_or_default();
    let geometry = measure(&planform, &controls)?;
    let clean = preset.performance.as_ref().map_or_else(
        || PerformanceConfig::default().cl_max_clean,
        |p| p.cl_max_clean,
    );
    let slats_out = super::clean_lift::slats_out_flaps_up(preset)?;
    let anchor = super::landing_reference::published_landing_reference(preset.name)
        .map(|reference| reference.cl_max_land())
        .or_else(|| super::clean_lift::pooled_slatted_landing(preset));
    Some(breakdown(clean, slats_out, system, &geometry, anchor))
}

/// A FAR stall coefficient on the `Vref = 1.23 VS1g` basis: the same Vref is 1.3 VS, so
/// the coefficient scales with `(1.23 / 1.3)^2`.
pub const FAR_TO_ONE_G_LIFT: f64 = (1.23 / 1.3) * (1.23 / 1.3);

/// Landing-flap `CLmax` at the minimum speed of the stall manoeuvre of the two
/// straight-wing turboprops without leading-edge devices of Obert, *Aerodynamic Design
/// of Transport Aircraft*, Fig. 26.9, printed p. 322 (Canadian Aeronautics and Space
/// Journal, Sept. 1984), read at the end of each curve: DHC-8 3.19 at 35 deg and DHC-7
/// 3.30 at 45 deg. The text gives "around 3.0" on the 1-g basis; the axis is labelled
/// `CLmax,Vmin`, so the points are taken on that basis and converted with
/// [`FAR_TO_ONE_G_LIFT`].
pub const DHC_LANDING_CLMAX_VMIN: [f64; 2] = [3.19, 3.30];

/// Landing `CLmax,1g` of a straight-wing turboprop without leading-edge devices: the
/// mean of the class measurements, the two DHC aircraft of [`DHC_LANDING_CLMAX_VMIN`]
/// and the ATR 72-600 recovered from its published Vref (`landing_reference`).
pub fn slatless_turboprop_landing_cl_max() -> Option<f64> {
    let atr = super::landing_reference::published_landing_reference("ATR72-600")?.cl_max_land();
    let dhc: f64 = DHC_LANDING_CLMAX_VMIN.iter().sum::<f64>() * FAR_TO_ONE_G_LIFT;
    Some((atr + dhc) / (1.0 + DHC_LANDING_CLMAX_VMIN.len() as f64))
}

/// The two curves of Obert's Fig. 26.9 (printed p. 322), `(flap deflection in degrees,
/// CLmax at Vmin)`, read from the plot at the grid lines (about +-0.02): DHC-8, then
/// DHC-7. Their last points are [`DHC_LANDING_CLMAX_VMIN`].
const DHC8_CLMAX_VMIN: [(f64, f64); 8] = [
    (0.0, 1.93),
    (5.0, 2.34),
    (10.0, 2.61),
    (15.0, 2.77),
    (20.0, 2.91),
    (25.0, 3.03),
    (30.0, 3.13),
    (35.0, 3.19),
];
const DHC7_CLMAX_VMIN: [(f64, f64); 6] = [
    (0.0, 1.50),
    (10.0, 2.05),
    (20.0, 2.54),
    (30.0, 3.01),
    (40.0, 3.25),
    (45.0, 3.30),
];

/// Smallest landing flap deflection among the measurements behind
/// [`slatless_turboprop_landing_cl_max`], degrees: ATR 72-600 30 (`FLAP_ONLY`), DHC-8
/// 35 and DHC-7 45 (Fig. 26.9). The class value is a landing value of 30 to 45 deg.
pub const SLATLESS_CLASS_MIN_LANDING_DEG: f64 = 30.0;

fn curve_at(points: &[(f64, f64)], deg: f64) -> f64 {
    let Some(upper) = points.iter().position(|&(node, _)| node >= deg) else {
        return points[points.len() - 1].1;
    };
    if upper == 0 {
        return points[0].1;
    }
    let ((x0, y0), (x1, y1)) = (points[upper - 1], points[upper]);
    y0 + (y1 - y0) * (deg - x0) / (x1 - x0)
}

/// The class landing value at a declared landing flap deflection, degrees.
///
/// The class value is measured at 30 to 45 deg of flap. A wing whose landing setting
/// is smaller does not reach it: the value is taken down the two measured curves of
/// Fig. 26.9, by the mean of their `CLmax(deflection) / CLmax(30 deg)`; from 30 deg
/// up, and with no declared deflection, it is the class value itself. At the 23 deg
/// of the CN-235 this gives 0.92 of the class value, 2.59, against the 2.28 of that
/// aircraft's flight-manual stall speed (`landing_reference`, C295 record): 13 %
/// high, where the class value itself is 23 % high. A wing with a published speed
/// never reads this.
pub fn slatless_turboprop_landing_cl_max_at(landing_deg: Option<f64>) -> Option<f64> {
    let class = slatless_turboprop_landing_cl_max()?;
    let Some(deg) = landing_deg.filter(|deg| *deg < SLATLESS_CLASS_MIN_LANDING_DEG) else {
        return Some(class);
    };
    let ratio = |points: &[(f64, f64)]| {
        curve_at(points, deg.max(0.0)) / curve_at(points, SLATLESS_CLASS_MIN_LANDING_DEG)
    };
    Some(class * 0.5 * (ratio(&DHC8_CLMAX_VMIN) + ratio(&DHC7_CLMAX_VMIN)))
}

/// Landing `CLmax,1g` of a slatless turboprop wing: the coefficient of its published
/// speed record when it has one, else the class value at its landing deflection.
fn slatless_landing_cl_max(wing: &SlatlessWing) -> Option<f64> {
    if let Some(reference) = super::landing_reference::published_landing_reference(wing.preset_name)
    {
        return Some(reference.cl_max_land());
    }
    slatless_turboprop_landing_cl_max_at(wing.takeoff_flap.map(|flap| flap.landing_deg))
}

/// A slatless turboprop: its flap record, and the class landing value when no stall
/// or approach speed is published for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlatlessWing {
    /// Registered aircraft.
    pub preset_name: &'static str,
    /// Take-off and landing flap deflections when a document states them.
    pub takeoff_flap: Option<TakeoffFlap>,
    /// Where the absence of slats and the flap are stated.
    pub source: &'static str,
}

const SLATLESS: [SlatlessWing; 2] = [
    SlatlessWing {
        preset_name: "A400M",
        takeoff_flap: None,
        source: "Jane's All the World's Aircraft 2008-09, Airbus A400M entry, PDF p. 340: five spoilers and two-section fixed-vane flaps on each wing, no slats",
    },
    SlatlessWing {
        preset_name: "C295",
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 10.0,
            landing_deg: 23.0,
            basis: "EASA TCDS A.186 CN-235 / C-295, Issue 08, Section 5 Model C-295, PDF p. 78: flaps take off 10 deg, approach 15 deg, landing 23 deg",
            other_takeoff_deg: &[],
        }),
        source: "EASA TCDS A.186 CN-235 / C-295, Issue 08, Section 5, PDF p. 78: inner and outer flaps, no leading-edge device among the control movements; Jane's All the World's Aircraft 2008-09, CN-235 entry, PDF p. 336: single-slotted flaps",
    },
];

/// The slatless turboprop record of one preset.
pub fn slatless_wing(name: &str) -> Option<&'static SlatlessWing> {
    SLATLESS.iter().find(|wing| wing.preset_name == name)
}

/// The landing and take-off coefficients of a slatless turboprop wing: the landing
/// value of its published speed record, else the class value at its landing
/// deflection; the take-off value at the share of the flap increment.
///
/// The share is the pooled jet curve of `flap_effectiveness`; Obert's Fig. 26.9 gives
/// 0.63 (DHC-8, 15 of 35 deg) and 0.57 (DHC-7, 20 of 45 deg) at the C295 deflection
/// ratio 10/23, against the 0.63 of that
/// curve at the same ratio.
fn apply_slatless(preset: &mut AircraftPreset, wing: &SlatlessWing) {
    let Some(landing) = slatless_landing_cl_max(wing) else {
        return;
    };
    let published = super::landing_reference::published_landing_reference(preset.name).is_some();
    let share = wing
        .takeoff_flap
        .and_then(|flap| flap.increment_share())
        .unwrap_or(TAKEOFF_FLAP_FRACTION);
    let performance = preset.performance.get_or_insert_with(Default::default);
    let takeoff = performance.cl_max_clean + share * (landing - performance.cl_max_clean);
    // A published speed record (`landing_reference`) has already set the landing
    // coefficient and its source.
    if !published {
        performance.cl_max_land = landing;
        performance.cl_max_land_source = format!(
            "Class mean of straight-wing turboprops without leading-edge devices: DHC-8 3.19 and DHC-7 3.30 CLmax,Vmin at the landing flap (Obert, Aerodynamic Design of Transport Aircraft, Fig. 26.9, printed p. 322) times (1.23/1.3)^2 for the 1-g basis, and the ATR 72-600 Vref recovery (landing_reference), landing flaps of 30 to 45 deg; {}; {}. Idle-power stall (CS 25.103), no propeller-wash lift; a class value, not a measurement of this aircraft",
            match wing.takeoff_flap.map(|flap| flap.landing_deg) {
                Some(deg) if deg < SLATLESS_CLASS_MIN_LANDING_DEG => format!(
                    "taken down the two Fig. 26.9 curves from 30 deg to the declared {deg} deg landing flap"
                ),
                Some(deg) => format!("declared landing flap {deg} deg, inside that range"),
                None => "landing flap deflection not stated, the class value as measured".to_owned(),
            },
            wing.source
        );
    }
    performance.cl_max_to = takeoff;
    performance.cl_max_to_source = format!(
        "clean {:.3} + share {share:.3} x (landing {landing:.3} - clean); {}",
        performance.cl_max_clean,
        wing.takeoff_flap.map_or(
            "take-off share 0.70, Raymer 60-80 % midpoint, flap deflection not stated".to_owned(),
            |flap| flap.basis.to_owned()
        ),
    );
    performance.vapp_vstall_land_factor = VREF_OVER_VS1G;
    performance.v2_vstall_factor = performance.v2_min_vsr_factor;
}

/// Raymer Eq. 12.61 flap factor for slotted flaps (plain flaps 0.0144), 5th ed.,
/// Sec. 12.6.5, printed p. 452. Fowler, double- and triple-slotted flaps are slotted.
pub const SLOTTED_FLAP_DRAG_FACTOR: f64 = 0.0074;
/// Raymer Eq. 12.61 deflection below which the flap adds no profile drag, degrees.
pub const FLAP_DRAG_ONSET_DEG: f64 = 10.0;
/// Take-off flap deflection when the reference does not state it, degrees: the
/// lower end of Raymer's "about 20-40 deg for takeoff" (5th ed., Sec. 12.6.5, p. 452)
/// and the largest take-off setting of the transports of Obert, Aerodynamic Design of
/// Transport Aircraft (2009), Fig. 27.34, printed p. 361 (A320 flaps 1+F/2/3, 777
/// flaps 5/15/20). Conservative within both ranges.
pub const DEFAULT_TAKEOFF_FLAP_DEG: f64 = 20.0;

/// Flap profile drag increment of the take-off setting on the reference area,
/// Raymer Eq. 12.61: `F_flap (c_f/c) (S_flapped/S_ref) (delta - 10 deg)`.
pub fn takeoff_flap_profile_cd(
    chord_fraction: f64,
    flapped_area_ratio: f64,
    deflection_deg: f64,
) -> f64 {
    SLOTTED_FLAP_DRAG_FACTOR
        * chord_fraction
        * flapped_area_ratio
        * (deflection_deg - FLAP_DRAG_ONSET_DEG).max(0.0)
}

/// Replace the class-default take-off flap drag increment of a registered
/// aircraft by the Eq. 12.61 value of its declared flap run and take-off setting.
/// The flap type is slotted for every registered aircraft (declared Fowler or
/// multi-slotted, or assumed for turboprops that declare none). The induced-drag
/// change of the flap span load is not added here: the lattice polar already
/// carries a clean-wing e of 0.71-0.85 at the take-off lift (Obert, Figs.
/// 27.36-27.39, printed pp. 362-363, measure e rising 0.0026 per degree of flap on
/// wing-engined transports), while Raymer Eq. 12.62 would add a further induced
/// term that those measured polars do not show.
/// Flap chord fraction and flapped-area ratio of a registered aircraft, the
/// geometry of Raymer Eq. 12.61; `None` when the planform or flap does not resolve.
fn flap_drag_geometry(preset: &AircraftPreset) -> Option<(f64, f64)> {
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .ok()?;
    let controls = crate::preset_control_surfaces::config_for(preset.name).unwrap_or_default();
    let planform_clean = planform.without_blends();
    let semi_span = planform_clean.tip.y_m;
    let s_ref = planform_clean.reference_area_m2();
    if !(semi_span > 0.0 && s_ref > 0.0 && controls.flap_chord_fraction > 0.0) {
        return None;
    }
    let flapped_area: f64 = controls
        .flap_bands()
        .iter()
        .map(|(start, end)| chord_area(&planform_clean, start * semi_span, end * semi_span))
        .sum();
    Some((controls.flap_chord_fraction, flapped_area / s_ref))
}

fn apply_takeoff_flap_drag(
    preset: &mut AircraftPreset,
    declared: Option<TakeoffFlap>,
    declared_type: bool,
) {
    let Some((flap_chord_fraction, flapped_ratio)) = flap_drag_geometry(preset) else {
        return;
    };
    let (deflection, deflection_basis) = match declared {
        Some(flap) => (flap.takeoff_deg, format!("declared {}", flap.basis)),
        None => (
            DEFAULT_TAKEOFF_FLAP_DEG,
            "default from Raymer 5th ed. Sec. 12.6.5 p. 452 (20-40 deg) and Obert 2009 Fig. 27.34 p. 361 (largest take-off setting 20 deg) for slotted flaps".to_owned(),
        ),
    };
    let increment = takeoff_flap_profile_cd(flap_chord_fraction, flapped_ratio, deflection);
    let (landing_deflection, landing_basis) = match declared {
        Some(flap) => (flap.landing_deg, "declared"),
        None => (
            DEFAULT_LANDING_FLAP_DEG,
            "default from Jane's All the World's Aircraft 2008-09, A320 entry (flaps to 35 deg) and Obert 2009 Fig. 27.34 (30-40 deg landing flaps)",
        ),
    };
    let landing_increment =
        takeoff_flap_profile_cd(flap_chord_fraction, flapped_ratio, landing_deflection);
    let performance = preset.performance.get_or_insert_with(Default::default);
    // Only a deflection the preset documents is declared; the others stay unset and the
    // labelled defaults apply wherever the deflection is read.
    performance.takeoff_flap_deflection_deg = declared.map(|flap| flap.takeoff_deg);
    performance.landing_flap_deflection_deg = declared.map(|flap| flap.landing_deg);
    performance.oei_climb_delta_cd = increment;
    performance.landing_flap_delta_cd = landing_increment;
    performance.cl_max_land_source.push_str(&format!(
        "; landing flap profile drag {landing_increment:.4} = Raymer Eq. 12.61 (5th ed., p. 452) at {landing_deflection:.0} deg, {landing_basis}"
    ));
    performance.cl_max_to_source.push_str(&format!(
        "; take-off flap profile drag {increment:.4} = Raymer Eq. 12.61 (5th ed., p. 452) 0.0074 x c_f/c {:.3} x S_flapped/S {flapped_ratio:.3} x ({deflection:.0} - 10) deg, {deflection_basis}; flap type {}",
        flap_chord_fraction,
        if declared_type { "declared slotted" } else { "assumed slotted" }
    ));
}

/// Take-off flap of an aircraft with no declared high-lift system whose take-off
/// coefficient is fixed elsewhere (a published `V2`, `takeoff_reference`, or the
/// preset's class value): the deflection sets the flap drag and the zero-alpha
/// flap lift, and `other_takeoff_deg` the alternative manual settings.
const FLAP_ONLY: [(&str, TakeoffFlap); 2] = [
    (
    "ATR72-600",
    TakeoffFlap {
        takeoff_deg: 15.0,
        landing_deg: 30.0,
        basis: "flap positions 0, 15 and 30 deg (Jane's All the World's Aircraft 2008-09, ATR 72 entry); position 1 is 15 deg and FULL the landing setting (BEA2022-0150, ATR 72-212A F-ORVS, p. 4); 15 deg is the only setting between flaps up and landing",
        other_takeoff_deg: &[],
    },
    ),
    (
    "LJ60",
    TakeoffFlap {
        takeoff_deg: 8.0,
        landing_deg: 40.0,
        basis: "Learjet 60 Airplane Flight Manual FM-123, Section V, Take-off Field Length charts: field lengths 'presented for 8 deg or 20 deg flap settings' (Fig. 5-38 flaps 8 deg, V1/VR/V2 136/145/153 KIAS at 23,500 lb, Fig. 5-42; flaps 20 deg chart and speeds 132/140/147 KIAS follow); landing flaps 40 deg (Fig. 5-66 p. 5-86). The preset's take-off coefficient flies the flaps-8 speeds, so 8 deg is the declared setting; the reference names no setting, so the ISA sea-level reference takes the shortest field of the two that meet CS 25.121(b)",
        other_takeoff_deg: &[20.0],
    },
    ),
];

/// The declared take-off flap of a registered aircraft, `None` when no document
/// states its deflections.
pub fn declared_takeoff_flap(name: &str) -> Option<TakeoffFlap> {
    declared_system(name)
        .and_then(|system| system.takeoff_flap)
        .or_else(|| slatless_wing(name).and_then(|wing| wing.takeoff_flap))
        .or_else(|| {
            FLAP_ONLY
                .iter()
                .find(|(preset, _)| *preset == name)
                .map(|(_, flap)| *flap)
        })
}

/// One sourced take-off flap setting with the take-off values it sets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffFlapSetting {
    /// Trailing-edge flap deflection, degrees.
    pub deflection_deg: f64,
    /// `CLmax_TO` at this setting.
    pub cl_max_to: f64,
    /// Take-off flap profile drag increment at this setting (Raymer Eq. 12.61).
    pub oei_climb_delta_cd: f64,
}

/// The sourced take-off settings of registered aircraft `name` flown from
/// `performance`, the declared setting first, when the flight manual offers more
/// than one and the reference names none (`TakeoffFlap::other_takeoff_deg`).
///
/// Each setting moves only what the deflection moves: `CLmax_TO` by the ratio
/// of the handbook values at the two deflections (the share of the landing
/// flap increment on the pooled curve of `flap_effectiveness`), so a
/// geometry-scaled coefficient keeps its scaling, and the flap profile drag by
/// the Raymer Eq. 12.61 difference. Empty when `performance` does not fly the
/// declared setting (an input chose another).
pub fn takeoff_flap_settings(
    name: &str,
    performance: &PerformanceConfig,
) -> Vec<TakeoffFlapSetting> {
    let Some(flap) = declared_takeoff_flap(name) else {
        return Vec::new();
    };
    if flap.other_takeoff_deg.is_empty()
        || performance.takeoff_flap_deflection_deg != Some(flap.takeoff_deg)
    {
        return Vec::new();
    }
    let Ok(preset) = super::get(name) else {
        return Vec::new();
    };
    let Some((chord_fraction, flapped_ratio)) = flap_drag_geometry(preset) else {
        return Vec::new();
    };
    let share = |deg: f64| super::flap_effectiveness::increment_share(deg / flap.landing_deg);
    let coefficient = |deg: f64| -> Option<f64> {
        let share = share(deg)?;
        if slatless_wing(name).is_some() {
            let clean = preset.performance.as_ref()?.cl_max_clean;
            let landing = slatless_wing(name).and_then(slatless_landing_cl_max)?;
            return Some(clean + share * (landing - clean));
        }
        if let Some(parts) = preset_breakdown(preset) {
            return Some(parts.clean + share * parts.flap_increment() + parts.leading_edge);
        }
        // A wing without leading-edge devices and with no declared system: the
        // same share of the increment between the clean coefficient and the
        // landing coefficient recovered from the manual's own approach speed.
        let clean = preset.performance.as_ref()?.cl_max_clean;
        let landing = super::landing_reference::published_landing_reference(name)?.cl_max_land();
        Some(clean + share * (landing - clean))
    };
    let drag = |deg: f64| takeoff_flap_profile_cd(chord_fraction, flapped_ratio, deg);
    let Some(declared_cl) = coefficient(flap.takeoff_deg).filter(|cl| *cl > 0.0) else {
        return Vec::new();
    };
    let scale = performance.cl_max_to / declared_cl;
    std::iter::once(flap.takeoff_deg)
        .chain(flap.other_takeoff_deg.iter().copied())
        .filter_map(|deg| {
            Some(TakeoffFlapSetting {
                deflection_deg: deg,
                cl_max_to: scale * coefficient(deg)?,
                oei_climb_delta_cd: performance.oei_climb_delta_cd + drag(deg)
                    - drag(flap.takeoff_deg),
            })
        })
        .collect()
}

pub(super) fn apply(preset: &mut AircraftPreset) {
    let declared = declared_takeoff_flap(preset.name);
    let declared_type = declared_system(preset.name).is_some();
    apply_after_flap(preset);
    apply_takeoff_flap_drag(preset, declared, declared_type);
}

fn apply_after_flap(preset: &mut AircraftPreset) {
    if let Some(wing) = slatless_wing(preset.name) {
        apply_slatless(preset, wing);
        return;
    }
    // A published V2 minimum (ATR) already fixes the effective coefficient.
    if published_takeoff_reference(preset.name).is_some() {
        return;
    }
    let (Some(system), Some(parts)) = (declared_system(preset.name), preset_breakdown(preset))
    else {
        return;
    };
    let performance = preset.performance.get_or_insert_with(Default::default);
    performance.cl_max_to = parts.takeoff();
    performance.cl_max_le_increment = parts.leading_edge;
    if super::landing_reference::published_landing_reference(preset.name).is_none() {
        if let Some(landing) = parts.landing_anchor {
            performance.cl_max_land = landing;
            performance.cl_max_land_source = format!(
                "Obert, Aerodynamic Design of Transport Aircraft, Figs. 26.12-26.16, printed pp. 324-325: pooled flight-test landing CLmax,1g = {} cos(sweep), 7.4 % r.m.s. over seven slatted transports (presets clean_lift); no published approach speed with its mass for this aircraft; a fleet law, not a measurement of it",
                super::clean_lift::SLATTED_LANDING_OVER_COS_SWEEP
            );
            performance.vapp_vstall_land_factor = VREF_OVER_VS1G;
        }
    }
    // CS 25.107(b)-(c): the safety speed is the larger of 1.13 VSR, 1.10 VMC and
    // VR, raised further by the speed that meets the CS 25.121(b) second-
    // segment gradient (the improved-climb selection). The class default of
    // 1.20 VSR is an operational choice, not a rule; a registered jet flies the
    // rule and the climb requirement decides any increase.
    performance.v2_vstall_factor = performance.v2_min_vsr_factor;
    performance.cl_max_to_source = format!(
        "Raymer, Aircraft Design: A Conceptual Approach, 5th ed., Eq. 12.21 and Table 12.2 (printed p. 415): CLmax_TO = clean {:.3} + {:.3} x flap {:.3} + leading edge {:.3} (slats out less clean, Obert Figs. 26.12-26.16) (handbook landing flap increment {:.3} = 0.9 dcl_max Sflapped/Sref cos(hinge sweep), c'/c {FOWLER_CHORD_RATIO} flap, declared control-surface spans; landing increment {}; take-off share {}); {} {}",
        parts.clean,
        parts.takeoff_share,
        parts.flap_increment(),
        parts.leading_edge,
        parts.flap_landing,
        if parts.landing_anchor.is_some() {
            if super::landing_reference::published_landing_reference(preset.name).is_some() {
                "anchored to the published-speed landing coefficient"
            } else {
                "anchored to the pooled slatted-landing law"
            }
        } else {
            "handbook"
        },
        match system.takeoff_flap {
            Some(flap) => format!(
                "Obert pooled CLmax,1g-vs-flap curve at {} of {} deg ({})",
                flap.takeoff_deg, flap.landing_deg, flap.basis
            ),
            None => "Raymer 60-80 % midpoint, take-off flap deflection not stated by the reference"
                .to_owned(),
        },
        system.source,
        system.applicability
    );
}

const COMMON_GAP: &str = "Conceptual handbook estimate, not a measured CLmax: the 60-80 % take-off share is taken at its 70 % midpoint, flap and slat spans and chords are the preset's declared control-surface runs, and the clean coefficient is the measured or pooled slats-in value of the clean-lift table.";

const SYSTEMS: [DeclaredHighLift; 11] = [
    DeclaredHighLift {
        preset_name: "A350-900",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "Airbus A350 Aircraft Characteristics, Rev Jul 01/26, section 2-3-0, Ground Clearances, library PDF p. 58: droop nose inboard, six leading-edge slats, flaps. Flap slot count is not printed: Fowler (single-slot row) adopted, an assumption.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A320-200",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 15.0,
            landing_deg: 35.0,
            basis: "take-off configurations 1+F, 2 and 3 (Obert, Aerodynamic Design of Transport Aircraft, 2009, Fig. 27.34, printed p. 361); CONF 1+F is slats 18 / flaps 10 deg (AAIB Bulletin 9/2017, A319 G-EZEW, p. 1); Jane's 2008-09 PDF p. 315 gives flaps 10/15/20/35 deg, so CONF 2 and 3 are flaps 15 and 20 and the landing flap is 35 deg. The A320 ACAP take-off chart names no configuration, so the ISA sea-level reference takes the shortest field among the three that meet CS 25.121(b)",
            other_takeoff_deg: &[10.0, 20.0],
        }),
        source: "Jane's All the World's Aircraft 2008-09, Airbus A320 entry, PDF p. 315: five-segment leading-edge slats per wing, two-segment Fowler trailing-edge flaps, slats 18/22/27 deg and flaps 10/15/20/35 deg; PDF p. 316 flap area 21.10 m2 (cross-check: 21.10/0.30 chord = 0.58 Sref flapped).",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A340-300",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 22.0,
            landing_deg: 32.0,
            basis: "Airbus A340-200/-300 Aircraft Characteristics, 1 Dec 2025, Fig. 3-3-1-991-006-A01 (library PDF p. 177) draws FLAPS 1, 2 and 3 branches and the matched 2,844 m is the flaps-3 branch at sea level; Jane's 2008-09 PDF p. 323 gives flaps 32 deg; the 22 deg of flaps 3 is the A330-200 detent plotted in Obert Fig. 26.14 (points 8, 14, 22 and 32 deg), the A330/A340 shared flap system, not an A340 document",
            other_takeoff_deg: &[],
        }),
        source: "Jane's All the World's Aircraft 2008-09, Airbus A330/A340 entry, PDF p. 323: 'High-lift devices consist of full-span slats, flaps and aileron droop', flaps 32 deg, slats 21-24 deg. Flap slot count is not printed in the library: Fowler (single-slot row) adopted.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A380-800",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "Jane's All the World's Aircraft 2008-09, Airbus A380 entry, PDF p. 330: 'Three-section, single-slotted flaps incorporate droop-nose device', slats in two sections inboard of engines, three between engines and three outboard.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "B787-9",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.25,
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 15.0,
            landing_deg: 30.0,
            basis: "787 take-off flaps 5, 15 and 20 of the flap positions 0, 1, 5, 15, 20, 25 and 30 (ATSB AO-2017-051, 787-8 flap event, quoted from the atsb.gov.au search extract; the host timed out); flaps 5 the normal take-off setting (AAIB India AI171 preliminary report, 2025, p. 10); landing flaps 30, the largest of the Boeing D6-58333 Rev Q landing charts 3.4 (flaps 30 and 25). Boeing flap positions are taken as degrees, as in Obert Fig. 26.16 (777-200 points at 5, 15, 20 and 30 deg). The 787-9 ACAP take-off chart names no setting, so the ISA sea-level reference takes the shortest field among the three that meet CS 25.121(b)",
            other_takeoff_deg: &[5.0, 20.0],
        }),
        source: "EASA TCDS IM.A.115 (library easa_2025_b787tcds) lists Krueger flaps (D-05-9); Jane's All the World's Aircraft 2008-09, Boeing 787 entry, PDF p. 742: inboard and outboard flaps with spoilers ahead of each, variable-camber trailing edge. Flap slot count and the Krueger span share (one inboard Krueger in four leading-edge sections) are assumptions.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "B747-400",
        trailing_edge: TrailingEdgeDevice::TripleSlotted,
        krueger_share: 1.0,
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 20.0,
            landing_deg: 30.0,
            basis: "Boeing 747-400 ACAP D6-58326-1 Rev F Fig. 3.3.1 (library PDF p. 74) is drawn for FLAPS 20 (read on the page); landing charts 3.4.1-3.4.4 are for flaps 25 and 30, 30 taken as the largest certified landing flap",
            other_takeoff_deg: &[],
        }),
        source: "Jane's All the World's Aircraft 2008-09, Boeing 747-400 entry, PDF p. 726: 'Three-section Krueger flaps inboard of engines; variable camber slats between (five-section) and outboard (six-section)'; two flap assemblies per wing of fore, mid and aft flap forming three slots; PDF p. 729 flaps 78.69 m2, leading-edge flaps 43.85 m2, quarter-chord sweep 37.5 deg. Sadraey, Aircraft Design: A Systems Engineering Approach, PDF p. 259: 747 triple-slotted Fowler. Leading-edge devices are all Krueger (share 1.0): Rudolph, High-Lift Systems on Commercial Subsonic Airliners, NASA CR-4746 (1996), PDF p. 61 (printed p. 51), 2.1.4: 'It has Krueger flaps as leading-edge devices', three bull-nose Krueger flaps inboard of the inboard engine and 5 + 6 variable-camber (VC) Krueger flaps outboard, 14 panels per side on the -400 (Jane's 'variable camber slats' are these VC Krueger flaps).",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "DC-10",
        trailing_edge: TrailingEdgeDevice::DoubleSlotted,
        krueger_share: 0.0,
        takeoff_flap: Some(TakeoffFlap {
            takeoff_deg: 15.0,
            landing_deg: 50.0,
            basis: "AAIB Bulletin 7/97, EW/C97/2/1 (DC-10-30 PP-VMD, CF6-50), p. 2: take-off at its maximum certificated weight of 256,000 kg, near sea level, maximum thrust, 15 deg flap; landing 50 deg is the Boeing DC-10 ACAP landing chart basis ('flaps 50'). The ACAP take-off chart itself prints no flap; this is the operational setting at maximum weight on the same series and engine family",
            other_takeoff_deg: &[],
        }),
        source: "No DC-10 high-lift description is in the library; the Boeing DC-10 ACAP chart basis is 'flaps 50' (landing), a deflection reached by double- and triple-slotted systems (Torenbeek, Advanced Aircraft Design, Sec. 10.9.1). Double slotted adopted as a documented assumption.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A220-300",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "Jane's All the World's Aircraft 2008-09, Bombardier CSeries entry, PDF p. 111: 'Fowler flaps; three-section leading-edge slats' per wing. Flap slot count is not printed in the library: Fowler (single-slot row) adopted.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "E195-E2",
        trailing_edge: TrailingEdgeDevice::DoubleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "Jane's All the World's Aircraft 2008-09, Embraer 170/190 entry, PDF p. 82: flaps deployed in six stages with main and aft elements, four-section leading-edge slats (E1 wing). The E2 wing is new and no E2 flap description is in the library: the E1 double-slotted system is adopted as an assumption.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "C919",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "No C919 high-lift description is in the library (ACAP lists only take-off and landing flap configurations by name): single-slot Fowler adopted as the Raymer baseline row, an assumption.",
        applicability: COMMON_GAP,
    },
    // The program's own design, not a published type: its preset states slats and
    // slotted Fowler flaps by its high-lift class, and the coefficients follow by
    // the rule of every other slatted jet instead of the class placeholders.
    DeclaredHighLift {
        preset_name: "AVE",
        trailing_edge: TrailingEdgeDevice::DoubleSlotted,
        krueger_share: 0.0,
        takeoff_flap: None,
        source: "AVE-v1 design statement (no aircraft document): leading-edge slats on the whole device run and tracked double-slotted flaps, the devices of the class-default chord extension (Jenkinson, Civil Jet Aircraft Design, Figs. 8.10 and 8.11, printed pp. 168-169) the preset already carried. The landing coefficient is the pooled slatted-landing law, so the slot count sets only the chord extension and the flap moment row.",
        applicability: COMMON_GAP,
    },
];

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> HighLiftGeometry {
        HighLiftGeometry {
            flapped_area_ratio: 0.60,
            hinge_sweep_deg: 20.0,
            slat_chord_ratio: 0.15,
            slat_span_ratio: 0.80,
            sweep_c4_deg: 25.0,
        }
    }

    fn system(trailing_edge: TrailingEdgeDevice, krueger_share: f64) -> DeclaredHighLift {
        DeclaredHighLift {
            preset_name: "test",
            trailing_edge,
            krueger_share,
            takeoff_flap: None,
            source: "",
            applicability: "",
        }
    }

    #[test]
    fn the_flap_follows_raymer_12_21_and_the_slat_is_slats_out_less_clean() {
        let parts = breakdown(
            1.5,
            2.06,
            &system(TrailingEdgeDevice::FowlerSingleSlotted, 0.0),
            &geometry(),
            None,
        );
        let flap = 0.9 * 1.3 * FOWLER_CHORD_RATIO * 0.60 * 20.0_f64.to_radians().cos();
        let slat = 2.06 - 1.5;
        assert!((parts.flap_landing - flap).abs() < 1e-12);
        assert!((parts.leading_edge - slat).abs() < 1e-12);
        assert!((parts.takeoff() - (1.5 + 0.7 * flap + slat)).abs() < 1e-12);
        assert!(parts.takeoff() < parts.landing());
    }

    #[test]
    fn more_slots_raise_the_coefficient_and_the_device_kind_does_not_scale_the_slat() {
        let take_off = |device, krueger| {
            breakdown(1.5, 2.06, &system(device, krueger), &geometry(), None).takeoff()
        };
        let single = take_off(TrailingEdgeDevice::FowlerSingleSlotted, 0.0);
        let double = take_off(TrailingEdgeDevice::DoubleSlotted, 0.0);
        let triple = take_off(TrailingEdgeDevice::TripleSlotted, 0.0);
        assert!(single < double && double < triple);
        // Eq. 8.30 is for "slats and/or Kruegers"; the measured level includes both.
        assert_eq!(take_off(TrailingEdgeDevice::DoubleSlotted, 1.0), double);
    }

    #[test]
    fn registered_jets_take_their_coefficient_from_the_declared_system() {
        for system in declared_systems() {
            let preset = super::super::get(system.preset_name).unwrap();
            let parts = preset_breakdown(preset).unwrap();
            let stored = preset.performance.as_ref().unwrap().cl_max_to;
            assert_eq!(stored, parts.takeoff(), "{}", system.preset_name);
            let landing = parts.landing_anchor.unwrap_or_else(|| parts.landing());
            assert!(
                (1.8..3.0).contains(&stored) && stored < landing,
                "{} {stored}",
                system.preset_name
            );
            assert!(preset
                .performance
                .as_ref()
                .unwrap()
                .cl_max_to_source
                .contains("Eq. 12.21"));
        }
    }

    #[test]
    fn the_stated_take_off_flaps_read_the_measured_share_and_the_rest_the_raymer_midpoint() {
        let mut stated = Vec::new();
        for system in declared_systems() {
            let preset = super::super::get(system.preset_name).unwrap();
            let parts = preset_breakdown(preset).unwrap();
            match system.takeoff_flap {
                Some(flap) => {
                    assert_eq!(parts.takeoff_share, flap.increment_share().unwrap());
                    assert!(
                        flap.takeoff_deg < flap.landing_deg,
                        "{}",
                        system.preset_name
                    );
                    stated.push(system.preset_name);
                }
                None => assert_eq!(parts.takeoff_share, TAKEOFF_FLAP_FRACTION),
            }
        }
        // Only the aircraft whose take-off flap a document states declare one.
        assert_eq!(
            stated,
            ["A320-200", "A340-300", "B787-9", "B747-400", "DC-10"]
        );
    }

    #[test]
    fn the_sourced_take_off_settings_move_only_what_the_deflection_moves() {
        let a320 = super::super::get("A320-200").unwrap();
        let performance = a320.performance.as_ref().unwrap();
        let settings = takeoff_flap_settings("A320-200", performance);
        let degrees: Vec<f64> = settings.iter().map(|s| s.deflection_deg).collect();
        assert_eq!(degrees, [15.0, 10.0, 20.0]);
        // The declared setting reproduces the stored values.
        assert!((settings[0].cl_max_to - performance.cl_max_to).abs() < 1e-12);
        assert!((settings[0].oei_climb_delta_cd - performance.oei_climb_delta_cd).abs() < 1e-12);
        // More flap, more lift and more drag; no profile drag at 10 deg (Raymer Eq. 12.61 onset).
        assert!(settings[1].cl_max_to < settings[0].cl_max_to);
        assert!(settings[2].cl_max_to > settings[0].cl_max_to);
        assert!(settings[2].oei_climb_delta_cd > settings[0].oei_climb_delta_cd);
        let at_ten = performance.oei_climb_delta_cd
            - (settings[0].oei_climb_delta_cd - settings[1].oei_climb_delta_cd);
        assert!((settings[1].oei_climb_delta_cd - at_ten).abs() < 1e-12);
        // A setting chosen by an input is not re-selected.
        let mut other = performance.clone();
        other.takeoff_flap_deflection_deg = Some(20.0);
        assert!(takeoff_flap_settings("A320-200", &other).is_empty());
        // A reference that states its setting offers no alternatives.
        let b747 = super::super::get("B747-400").unwrap();
        assert!(takeoff_flap_settings("B747-400", b747.performance.as_ref().unwrap()).is_empty());
    }

    /// A wing with no declared system and no leading-edge device (LJ60) shares the
    /// increment between its clean value and the coefficient of the manual's own
    /// approach speed. Validation only: the flight manual's V2 at 23,500 lb, 153
    /// KIAS at flaps 8 and 147 KIAS at flaps 20 (Learjet 60 AFM FM-123, Section V),
    /// gives a coefficient ratio (153/147)^2 = 1.083 at one V2/VSR; the pooled
    /// share curve scatters by about +-0.08 in `CLmax_TO`.
    #[test]
    fn an_unslatted_jet_offers_its_manual_settings_from_the_landing_anchor() {
        let lj60 = super::super::get("LJ60").unwrap();
        let performance = lj60.performance.as_ref().unwrap();
        assert_eq!(performance.takeoff_flap_deflection_deg, Some(8.0));
        let settings = takeoff_flap_settings("LJ60", performance);
        let degrees: Vec<f64> = settings.iter().map(|s| s.deflection_deg).collect();
        assert_eq!(degrees, [8.0, 20.0]);
        assert!((settings[0].cl_max_to - performance.cl_max_to).abs() < 1e-12);
        let ratio = settings[1].cl_max_to / settings[0].cl_max_to;
        assert!(
            (ratio - (153.0_f64 / 147.0).powi(2)).abs() * settings[0].cl_max_to <= 0.08,
            "ratio {ratio:.3}"
        );
        assert!(settings[1].oei_climb_delta_cd > settings[0].oei_climb_delta_cd);
    }

    /// Validation only (not an input): Obert, Aerodynamic Design of Transport
    /// Aircraft (2009), Fig. 26.13, printed p. 324, A320-200 flight-test
    /// `CLmax,1g` 2.36 at flap 15 gear down and 2.42 at flap 20, read +-0.02;
    /// the pooled share curve scatters by about +-0.08 in `CLmax_TO`.
    #[test]
    fn the_a320_take_off_coefficients_sit_within_the_pooled_scatter_of_the_flight_test() {
        let a320 = super::super::get("A320-200").unwrap();
        let settings = takeoff_flap_settings("A320-200", a320.performance.as_ref().unwrap());
        for (deg, flight_test) in [(15.0, 2.36), (20.0, 2.42)] {
            let setting = settings.iter().find(|s| s.deflection_deg == deg).unwrap();
            assert!(
                (setting.cl_max_to - flight_test).abs() <= 0.10,
                "flap {deg}: model {:.3} vs Obert {flight_test}",
                setting.cl_max_to
            );
        }
    }

    #[test]
    fn a_published_approach_speed_anchors_the_landing_increment_and_bounds_the_take_off_value() {
        for system in declared_systems() {
            let preset = super::super::get(system.preset_name).unwrap();
            let parts = preset_breakdown(preset).unwrap();
            let published = super::super::published_landing_reference(system.preset_name);
            // Every slatted jet is anchored: the published speed, else the pooled law.
            let stored = preset.performance.as_ref().unwrap().cl_max_land;
            assert_eq!(parts.landing_anchor, Some(stored), "{}", system.preset_name);
            if published.is_none() {
                assert!(preset
                    .performance
                    .as_ref()
                    .unwrap()
                    .cl_max_land_source
                    .contains("pooled flight-test landing"));
            }
            {
                let anchor = stored;
                assert_eq!(parts.landing_anchor, Some(anchor));
                // Slats-out, flaps-up coefficient plus the whole increment is the
                // landing coefficient, and take-off sits strictly below it.
                let increment = parts.flap_increment();
                assert!(
                    (parts.clean + parts.leading_edge + increment - anchor).abs() < 1e-12,
                    "{}",
                    system.preset_name
                );
                assert!(parts.takeoff() < anchor, "{}", system.preset_name);
            }
        }
    }

    #[test]
    fn presets_without_a_declared_system_keep_their_source_backed_or_class_value() {
        let atr = super::super::get("ATR72-600").unwrap();
        assert!(atr
            .performance
            .as_ref()
            .unwrap()
            .cl_max_to_source
            .contains("V2 min @ MTOW"));
        assert!(declared_system("ATR72-600").is_none());
        let name = "A400M";
        assert!(declared_system(name).is_none());
        let preset = super::super::get(name).unwrap();
        assert!(!preset
            .performance
            .as_ref()
            .unwrap()
            .cl_max_to_source
            .contains("Eq. 12.21"));
    }

    #[test]
    fn the_programs_own_design_takes_the_pooled_laws_of_the_slatted_jets() {
        // AVE has no published speed: clean, slats-out and landing levels are the
        // pooled flight-test laws at its own quarter-chord sweep, the take-off value
        // the handbook build between them, as for every declared jet without one.
        let preset = super::super::get("AVE").unwrap();
        let performance = preset.performance.as_ref().unwrap();
        let cos = quarter_chord_sweep_deg(preset).unwrap().to_radians().cos();
        let clean = super::super::clean_lift::SLATS_IN_CLEAN_OVER_COS_SWEEP * cos;
        let slats_out = super::super::clean_lift::SLATS_OUT_FLAPS_UP_OVER_COS_SWEEP * cos;
        let landing = super::super::clean_lift::SLATTED_LANDING_OVER_COS_SWEEP * cos;
        assert!((performance.cl_max_clean - clean).abs() < 1e-12);
        assert!((performance.cl_max_land - landing).abs() < 1e-12);
        let takeoff = slats_out + TAKEOFF_FLAP_FRACTION * (landing - slats_out);
        assert!(
            (performance.cl_max_to - takeoff).abs() < 1e-12,
            "{}",
            performance.cl_max_to
        );
        assert!(performance.cl_max_to_source.contains("Eq. 12.21"));
        assert!(performance
            .cl_max_land_source
            .contains("pooled flight-test landing"));
        // Inside the measured landing range of the slatted transports (2.16-3.00).
        assert!((2.16..=3.0).contains(&performance.cl_max_land));
    }

    #[test]
    fn straight_wing_slatless_turboprops_take_the_class_landing_value() {
        let atr = super::super::published_landing_reference("ATR72-600")
            .unwrap()
            .cl_max_land();
        let class = slatless_turboprop_landing_cl_max().unwrap();
        let expected = (atr + (3.19 + 3.30) * FAR_TO_ONE_G_LIFT) / 3.0;
        assert!((class - expected).abs() < 1e-12);
        // About the "around 3.0" of Obert's text, well above the swept-jet fit.
        assert!((2.7..=3.0).contains(&class), "{class}");
        // No landing deflection is declared for the A400M: the class value as measured.
        let a400m = super::super::get("A400M")
            .unwrap()
            .performance
            .clone()
            .unwrap();
        assert_eq!(a400m.cl_max_land, class);
        assert!(a400m.cl_max_land_source.contains("Fig. 26.9"));
        assert!(a400m.cl_max_clean < a400m.cl_max_to && a400m.cl_max_to < class);
    }

    #[test]
    fn the_class_value_is_taken_down_the_measured_curves_below_its_own_deflections() {
        let class = slatless_turboprop_landing_cl_max().unwrap();
        for deg in [30.0, 35.0, 45.0] {
            assert_eq!(slatless_turboprop_landing_cl_max_at(Some(deg)), Some(class));
        }
        assert_eq!(slatless_turboprop_landing_cl_max_at(None), Some(class));
        // The curves end at the two class points.
        assert_eq!(DHC8_CLMAX_VMIN[7].1, DHC_LANDING_CLMAX_VMIN[0]);
        assert_eq!(DHC7_CLMAX_VMIN[5].1, DHC_LANDING_CLMAX_VMIN[1]);
        let mut last = 0.0;
        for step in 0..=30 {
            let value = slatless_turboprop_landing_cl_max_at(Some(f64::from(step))).unwrap();
            assert!(value > last && value <= class);
            last = value;
        }
        // 23 deg: (2.982 / 3.13 + 2.681 / 3.01) / 2 = 0.922 of the class value.
        let at_23 = slatless_turboprop_landing_cl_max_at(Some(23.0)).unwrap();
        assert!((at_23 / class - 0.922).abs() < 0.002, "{}", at_23 / class);
        // Validation on a type that is not a preset: the CN-235-300 flight-manual
        // stall speed at flaps 23 deg (the C295 record). The curve is 13 % high, the
        // class value as measured 23 %.
        let cn235 = super::super::published_landing_reference("C295")
            .unwrap()
            .cl_max_land();
        assert!((at_23 / cn235 - 1.13).abs() < 0.02, "{}", at_23 / cn235);
        assert!((class / cn235 - 1.23).abs() < 0.02, "{}", class / cn235);
    }

    #[test]
    fn the_c295_lands_on_the_stall_speed_of_the_same_wing_and_flap() {
        use super::super::landing_reference::{
            CN235_300_CLEAN_STALL_KT, CN235_300_LANDING_STALL_KT, CN235_300_STALL_MASS_KG,
            CN235_300_TAKEOFF_STALL_KT,
        };
        let reference = super::super::published_landing_reference("C295").unwrap();
        let performance = super::super::get("C295")
            .unwrap()
            .performance
            .clone()
            .unwrap();
        assert_eq!(performance.cl_max_land, reference.cl_max_land());
        assert!(performance.cl_max_land_source.contains("CN-235-300"));
        assert!(
            (reference.cl_max_land() - 2.285).abs() < 0.005,
            "{}",
            reference.cl_max_land()
        );
        // The same table at the lighter weight, 75 kt at 31,350 lb: the coefficient is
        // the wing's, not the weight's.
        let lighter = 31_350.0 / 34_830.0 * (CN235_300_LANDING_STALL_KT / 75.0_f64).powi(2);
        assert!((lighter - 1.0).abs() < 0.005, "{lighter}");
        // One-g coefficients of the same table at the take-off flap and clean, on the
        // record's area: what the share curve and the class clean value are checked on.
        let one_g = |stall_kt: f64| {
            reference.cl_max_land() * (CN235_300_LANDING_STALL_KT / stall_kt).powi(2)
        };
        const { assert!(CN235_300_STALL_MASS_KG > 15_700.0) };
        let takeoff = one_g(CN235_300_TAKEOFF_STALL_KT);
        let clean = one_g(CN235_300_CLEAN_STALL_KT);
        assert!(
            (performance.cl_max_to / takeoff - 1.0).abs() < 0.05,
            "{takeoff}"
        );
        assert!(
            (performance.cl_max_clean / clean - 1.0).abs() < 0.05,
            "{clean}"
        );
        // The C-295 MLW speed: 79 kt x sqrt(20,700 / 15,799) x 1.3 = 117.6 kt.
        let vref_kt =
            1.3 * CN235_300_LANDING_STALL_KT * (20_700.0 / CN235_300_STALL_MASS_KG).sqrt();
        assert!((vref_kt - 117.6).abs() < 0.1, "{vref_kt}");
    }
}

/// Landing flap deflection of the flapped transports when the reference does not state
/// it, degrees: the largest setting of the A320 (flaps 10/15/20/35 deg, Jane's All the
/// World's Aircraft 2008-09, Airbus A320 entry, PDF p. 315) and within the 30-40 deg of
/// the landing flaps of Obert 2009, Fig. 27.34, printed p. 361.
pub const DEFAULT_LANDING_FLAP_DEG: f64 = 35.0;

/// Fraction of the landing-flap drag increment at the lowest flap setting that still
/// gives the approach-climb configuration of CS 25.121(d), `VSR <= 1.10 VSR0`.
///
/// The flap fraction `x = d / d_land` is found from the pooled lift-share curve of
/// `flap_effectiveness` (Obert, Figs. 26.13-26.16): the setting whose `CLmax` is
/// `cl_max_land / max_vsr_ratio^2`, with the leading-edge increment `cl_le` kept. The
/// drag follows Raymer Eq. 12.61 (5th ed., p. 452), proportional to `(d - 10 deg)`
/// above the onset, with `landing_deflection_deg` the declared landing setting or [`DEFAULT_LANDING_FLAP_DEG`]. `None` for
/// non-physical inputs; 1 when no flap setting below landing qualifies.
pub fn approach_flap_drag_ratio(
    cl_max_clean: f64,
    cl_le: f64,
    cl_max_land: f64,
    max_vsr_ratio: f64,
    landing_deflection_deg: f64,
) -> Option<f64> {
    let increment = cl_max_land - cl_max_clean - cl_le;
    if !(cl_max_clean.is_finite()
        && cl_le.is_finite()
        && cl_max_land.is_finite()
        && max_vsr_ratio.is_finite()
        && max_vsr_ratio >= 1.0
        && landing_deflection_deg > FLAP_DRAG_ONSET_DEG
        && increment > 0.0)
    {
        return None;
    }
    let required = ((cl_max_land / (max_vsr_ratio * max_vsr_ratio) - cl_max_clean - cl_le)
        / increment)
        .clamp(0.0, 1.0);
    // Lowest x with share(x) >= required: the share is monotone, so bisect.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if super::flap_effectiveness::increment_share(mid)? >= required {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let deflection = hi * landing_deflection_deg;
    Some(
        (deflection - FLAP_DRAG_ONSET_DEG).max(0.0)
            / (landing_deflection_deg - FLAP_DRAG_ONSET_DEG),
    )
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod approach_flap_tests {
    use super::*;

    #[test]
    fn the_approach_setting_is_between_no_flap_and_landing_flap() {
        // CLmax_land 2.60, clean 1.50, no leading-edge increment: the 1.10 VSR0 criterion
        // needs CLmax 2.149, a share 0.59 of the flap increment, below landing flap.
        let ratio =
            approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.10, DEFAULT_LANDING_FLAP_DEG).unwrap();
        assert!(ratio > 0.0 && ratio < 1.0, "{ratio}");
        // The share curve is the pooled one: invert it by hand at the returned setting.
        let x = (ratio * (DEFAULT_LANDING_FLAP_DEG - FLAP_DRAG_ONSET_DEG) + FLAP_DRAG_ONSET_DEG)
            / DEFAULT_LANDING_FLAP_DEG;
        let share = super::super::flap_effectiveness::increment_share(x).unwrap();
        assert!(
            (share - (2.60 / 1.21 - 1.50) / 1.10).abs() < 1e-6,
            "{share}"
        );
    }

    #[test]
    fn the_limits_are_landing_flap_no_flap_and_invalid_input() {
        // A criterion of exactly 1.0 VSR0 needs the landing setting itself.
        assert!(
            (approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.0, DEFAULT_LANDING_FLAP_DEG).unwrap()
                - 1.0)
                .abs()
                < 1e-9
        );
        // A clean wing already within 1.10 VSR0 needs no flap: no drag.
        assert_eq!(
            approach_flap_drag_ratio(2.40, 0.0, 2.60, 1.10, DEFAULT_LANDING_FLAP_DEG),
            Some(0.0)
        );
        // The drag ratio falls as the allowed stall-speed margin grows.
        let tight =
            approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.05, DEFAULT_LANDING_FLAP_DEG).unwrap();
        let loose =
            approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.15, DEFAULT_LANDING_FLAP_DEG).unwrap();
        assert!(tight > loose);
        // No flap increment, a margin below 1 and non-finite inputs are rejected.
        assert!(
            approach_flap_drag_ratio(2.60, 0.0, 2.60, 1.10, DEFAULT_LANDING_FLAP_DEG).is_none()
        );
        assert!(approach_flap_drag_ratio(1.50, 0.0, 2.60, 0.9, DEFAULT_LANDING_FLAP_DEG).is_none());
        assert!(
            approach_flap_drag_ratio(f64::NAN, 0.0, 2.60, 1.10, DEFAULT_LANDING_FLAP_DEG).is_none()
        );
    }
}

/// Rise of the apparent Oswald factor per degree of trailing-edge flap, wing-mounted
/// engines: the average of Obert, Aerodynamic Design of Transport Aircraft (2009),
/// Fig. 27.39, printed p. 363 (A300, A310, B737, B747, B757, B777, Fokker 50).
pub const OSWALD_GAIN_PER_FLAP_DEG_WING_ENGINES: f64 = 0.0026;
/// The same for rear-fuselage engines, Obert Fig. 27.38, printed p. 363.
pub const OSWALD_GAIN_PER_FLAP_DEG_REAR_ENGINES: f64 = 0.0046;

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod declared_deflection_tests {
    use super::*;

    #[test]
    fn a_declared_landing_deflection_replaces_the_default_in_the_approach_drag_share() {
        // A smaller landing flap leaves less deflection above the 10 deg onset, so the
        // same lift share costs a smaller fraction of the landing drag.
        let at = |deg| approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.10, deg).unwrap();
        assert!(at(23.0) < at(30.0) && at(30.0) < at(DEFAULT_LANDING_FLAP_DEG));
        // A landing flap at or below the onset has no profile drag to share.
        assert!(approach_flap_drag_ratio(1.50, 0.0, 2.60, 1.10, 10.0).is_none());
    }

    #[test]
    fn declared_deflections_land_in_the_preset_performance_and_unset_means_default() {
        let c295 = crate::presets::get("C295")
            .unwrap()
            .performance
            .clone()
            .unwrap();
        assert_eq!(c295.landing_flap_deflection_deg, Some(23.0));
        assert_eq!(c295.takeoff_flap_deflection_deg, Some(10.0));
        let a350 = crate::presets::get("A350-900")
            .unwrap()
            .performance
            .clone()
            .unwrap();
        assert_eq!(a350.takeoff_flap_deflection_deg, None);
        assert_eq!(a350.landing_flap_deflection_deg, None);
    }
}
