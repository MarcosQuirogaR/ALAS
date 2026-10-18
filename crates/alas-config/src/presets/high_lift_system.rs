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
//! dCL_x    = 0.9 dcl_max,x (S_flapped,x / S_ref) cos(Lambda_HL,x)
//! dcl_max  = k_device c'/c      k = 1.3 Fowler, 1.6 double slotted, 1.9 triple
//!                               slotted (trailing edge); 0.4 slat, 0.3 Krueger
//! ```
//!
//! Constants, none fitted to an aircraft or a field length:
//! - `f_TO = 0.70`, the midpoint of "lift increments of about 60-80% of these
//!   values should be used" for take-off flap settings (Raymer p. 415;
//!   Jenkinson PDF p. 183 "For take-off values use 60 to 80%"). The slat is
//!   kept at its full value, as in Raymer's worked A320 (2.29 take-off).
//! - `c'/c = 1.275` Fowler chord extension, the midpoint of "typical values are
//!   1.25-1.30" (Jenkinson PDF p. 183), and `1.075` for the slat, the midpoint
//!   of "typically 1.05-1.10" (Jenkinson PDF p. 182).
//! - Flap and slat spans and chord fractions are the declared
//!   [`ControlSurfacesConfig`] values the vortex-lattice geometry and the
//!   `CLmax` geometry scaling already read; the flap hinge line sits at
//!   `(1 - c_f/c)` of the local chord and the slat hinge line at the leading
//!   edge (Jenkinson p. 183: hinge sweep approximated by the trailing-edge
//!   sweep for flaps and the leading-edge sweep for slats).
//! - The clean value is the configuration's own `cl_max_clean` (Jenkinson
//!   PDF p. 179: "modern turbofan aircraft have a clean wing maximum lift
//!   coefficient of about 1.5").
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

/// Raymer Table 12.2 slat row: `0.4 c'/c`.
pub const SLAT_INCREMENT_PER_CHORD_RATIO: f64 = 0.4;
/// Raymer Table 12.2 Krueger row: `0.3` (no chord extension).
pub const KRUEGER_INCREMENT: f64 = 0.3;
/// Take-off flap increment as a fraction of the landing increment (Raymer p. 415, 60-80 %).
pub const TAKEOFF_FLAP_FRACTION: f64 = 0.70;
/// Fowler chord extension `c'/c` (Jenkinson PDF p. 183, 1.25-1.30).
pub const FOWLER_CHORD_RATIO: f64 = 1.275;
/// Slat chord extension `c'/c` (Jenkinson PDF p. 182, 1.05-1.10).
pub const SLAT_CHORD_RATIO: f64 = 1.075;

/// A declared high-lift system with its document basis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeclaredHighLift {
    /// Registered aircraft.
    pub preset_name: &'static str,
    /// Trailing-edge device.
    pub trailing_edge: TrailingEdgeDevice,
    /// Share of the leading-edge device span that is Krueger flap, not slat.
    pub krueger_share: f64,
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
    /// Leading-edge device wing area over `S_ref`.
    pub slatted_area_ratio: f64,
    /// Leading-edge sweep over the slat span, degrees.
    pub leading_edge_sweep_deg: f64,
}

/// The coefficients the method returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighLiftBreakdown {
    /// Clean wing.
    pub clean: f64,
    /// Trailing-edge increment at the landing setting.
    pub flap_landing: f64,
    /// Leading-edge increment.
    pub leading_edge: f64,
}

impl HighLiftBreakdown {
    /// `CLmax` at the take-off flap setting.
    pub fn takeoff(&self) -> f64 {
        self.clean + TAKEOFF_FLAP_FRACTION * self.flap_landing + self.leading_edge
    }

    /// `CLmax` at the landing flap setting (a cross-check against Vref).
    pub fn landing(&self) -> f64 {
        self.clean + self.flap_landing + self.leading_edge
    }
}

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

/// Measure the declared flap and slat runs on a planform.
pub fn measure(
    planform: &TransportPlanform,
    controls: &ControlSurfacesConfig,
) -> Option<HighLiftGeometry> {
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
    let leading_edge_sweep_deg = (station_at(planform, s_out)?.0 - station_at(planform, s_in)?.0)
        .atan2(s_out - s_in)
        .to_degrees();
    Some(HighLiftGeometry {
        flapped_area_ratio: chord_area(planform, f_in, f_out) / s_ref,
        hinge_sweep_deg,
        slatted_area_ratio: chord_area(planform, s_in, s_out) / s_ref,
        leading_edge_sweep_deg,
    })
}

/// Raymer Eq. 12.21 for the declared system on a measured wing.
pub fn breakdown(
    clean: f64,
    system: &DeclaredHighLift,
    geometry: &HighLiftGeometry,
) -> HighLiftBreakdown {
    let flap_section =
        system.trailing_edge.section_increment_per_chord_ratio() * FOWLER_CHORD_RATIO;
    let slat_section =
        (1.0 - system.krueger_share) * SLAT_INCREMENT_PER_CHORD_RATIO * SLAT_CHORD_RATIO
            + system.krueger_share * KRUEGER_INCREMENT;
    HighLiftBreakdown {
        clean,
        flap_landing: 0.9
            * flap_section
            * geometry.flapped_area_ratio
            * geometry.hinge_sweep_deg.to_radians().cos(),
        leading_edge: 0.9
            * slat_section
            * geometry.slatted_area_ratio
            * geometry.leading_edge_sweep_deg.to_radians().cos(),
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
    let geometry = measure(&planform, &ControlSurfacesConfig::default())?;
    let clean = preset.performance.as_ref().map_or_else(
        || PerformanceConfig::default().cl_max_clean,
        |p| p.cl_max_clean,
    );
    Some(breakdown(clean, system, &geometry))
}

pub(super) fn apply(preset: &mut AircraftPreset) {
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
    performance.cl_max_to_source = format!(
        "Raymer, Aircraft Design: A Conceptual Approach, 5th ed., Eq. 12.21 and Table 12.2 (printed p. 415): CLmax_TO = clean {:.3} + {TAKEOFF_FLAP_FRACTION} x flap {:.3} + leading edge {:.3} (landing flap increment 0.9 dcl_max Sflapped/Sref cos(hinge sweep); c'/c {FOWLER_CHORD_RATIO} flap, {SLAT_CHORD_RATIO} slat; declared control-surface spans); {} {}",
        parts.clean, parts.flap_landing, parts.leading_edge, system.source, system.applicability
    );
}

const COMMON_GAP: &str = "Conceptual handbook estimate, not a measured CLmax: the 60-80 % take-off share is taken at its 70 % midpoint, flap and slat span and chord are the declared control-surface defaults, and the clean coefficient is the configuration's 1.5 class value.";

const SYSTEMS: [DeclaredHighLift; 9] = [
    DeclaredHighLift {
        preset_name: "A320-200",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        source: "Jane's All the World's Aircraft 2008-09, Airbus A320 entry, PDF p. 315: five-segment leading-edge slats per wing, two-segment Fowler trailing-edge flaps, slats 18/22/27 deg and flaps 10/15/20/35 deg; PDF p. 316 flap area 21.10 m2 (cross-check: 21.10/0.30 chord = 0.58 Sref flapped).",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A340-300",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        source: "Jane's All the World's Aircraft 2008-09, Airbus A330/A340 entry, PDF p. 323: 'High-lift devices consist of full-span slats, flaps and aileron droop', flaps 32 deg, slats 21-24 deg. Flap slot count is not printed in the library: Fowler (single-slot row) adopted.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A380-800",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        source: "Jane's All the World's Aircraft 2008-09, Airbus A380 entry, PDF p. 330: 'Three-section, single-slotted flaps incorporate droop-nose device', slats in two sections inboard of engines, three between engines and three outboard.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "B787-9",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.25,
        source: "EASA TCDS IM.A.115 (library easa_2025_b787tcds) lists Krueger flaps (D-05-9); Jane's All the World's Aircraft 2008-09, Boeing 787 entry, PDF p. 742: inboard and outboard flaps with spoilers ahead of each, variable-camber trailing edge. Flap slot count and the Krueger span share (one inboard Krueger in four leading-edge sections) are assumptions.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "B747-400",
        trailing_edge: TrailingEdgeDevice::TripleSlotted,
        krueger_share: 3.0 / 14.0,
        source: "Jane's All the World's Aircraft 2008-09, Boeing 747-400 entry, PDF p. 726: 'Three-section Krueger flaps inboard of engines; variable camber slats between (five-section) and outboard (six-section)'; two flap assemblies per wing of fore, mid and aft flap forming three slots; PDF p. 729 flaps 78.69 m2, leading-edge flaps 43.85 m2, quarter-chord sweep 37.5 deg. Sadraey, Aircraft Design: A Systems Engineering Approach, PDF p. 259: 747 triple-slotted Fowler. Krueger share = 3 of 14 sections.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "DC-10",
        trailing_edge: TrailingEdgeDevice::DoubleSlotted,
        krueger_share: 0.0,
        source: "No DC-10 high-lift description is in the library; the Boeing DC-10 ACAP chart basis is 'flaps 50' (landing), a deflection reached by double- and triple-slotted systems (Torenbeek, Advanced Aircraft Design, Sec. 10.9.1). Double slotted adopted as a documented assumption.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "A220-300",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        source: "Jane's All the World's Aircraft 2008-09, Bombardier CSeries entry, PDF p. 111: 'Fowler flaps; three-section leading-edge slats' per wing. Flap slot count is not printed in the library: Fowler (single-slot row) adopted.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "E195-E2",
        trailing_edge: TrailingEdgeDevice::DoubleSlotted,
        krueger_share: 0.0,
        source: "Jane's All the World's Aircraft 2008-09, Embraer 170/190 entry, PDF p. 82: flaps deployed in six stages with main and aft elements, four-section leading-edge slats (E1 wing). The E2 wing is new and no E2 flap description is in the library: the E1 double-slotted system is adopted as an assumption.",
        applicability: COMMON_GAP,
    },
    DeclaredHighLift {
        preset_name: "C919",
        trailing_edge: TrailingEdgeDevice::FowlerSingleSlotted,
        krueger_share: 0.0,
        source: "No C919 high-lift description is in the library (ACAP lists only take-off and landing flap configurations by name): single-slot Fowler adopted as the Raymer baseline row, an assumption.",
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
            slatted_area_ratio: 0.75,
            leading_edge_sweep_deg: 27.0,
        }
    }

    fn system(trailing_edge: TrailingEdgeDevice, krueger_share: f64) -> DeclaredHighLift {
        DeclaredHighLift {
            preset_name: "test",
            trailing_edge,
            krueger_share,
            source: "",
            applicability: "",
        }
    }

    #[test]
    fn the_increments_follow_raymer_equation_12_21() {
        let parts = breakdown(
            1.5,
            &system(TrailingEdgeDevice::FowlerSingleSlotted, 0.0),
            &geometry(),
        );
        let flap = 0.9 * 1.3 * FOWLER_CHORD_RATIO * 0.60 * 20.0_f64.to_radians().cos();
        let slat = 0.9 * 0.4 * SLAT_CHORD_RATIO * 0.75 * 27.0_f64.to_radians().cos();
        assert!((parts.flap_landing - flap).abs() < 1e-12);
        assert!((parts.leading_edge - slat).abs() < 1e-12);
        assert!((parts.takeoff() - (1.5 + 0.7 * flap + slat)).abs() < 1e-12);
        assert!(parts.takeoff() < parts.landing());
    }

    #[test]
    fn more_slots_and_more_slat_than_krueger_raise_the_coefficient() {
        let take_off =
            |device, krueger| breakdown(1.5, &system(device, krueger), &geometry()).takeoff();
        let single = take_off(TrailingEdgeDevice::FowlerSingleSlotted, 0.0);
        let double = take_off(TrailingEdgeDevice::DoubleSlotted, 0.0);
        let triple = take_off(TrailingEdgeDevice::TripleSlotted, 0.0);
        assert!(single < double && double < triple);
        assert!(take_off(TrailingEdgeDevice::DoubleSlotted, 1.0) < double);
    }

    #[test]
    fn registered_jets_take_their_coefficient_from_the_declared_system() {
        for system in declared_systems() {
            let preset = super::super::get(system.preset_name).unwrap();
            let parts = preset_breakdown(preset).unwrap();
            let stored = preset.performance.as_ref().unwrap().cl_max_to;
            assert_eq!(stored, parts.takeoff(), "{}", system.preset_name);
            assert!(
                (1.9..3.0).contains(&stored) && stored < parts.landing(),
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
    fn presets_without_a_declared_system_keep_their_source_backed_or_class_value() {
        let atr = super::super::get("ATR72-600").unwrap();
        assert!(atr
            .performance
            .as_ref()
            .unwrap()
            .cl_max_to_source
            .contains("V2 min @ MTOW"));
        assert!(declared_system("ATR72-600").is_none());
        for name in ["AVE", "A400M"] {
            assert!(declared_system(name).is_none());
            let preset = super::super::get(name).unwrap();
            assert!(!preset
                .performance
                .as_ref()
                .unwrap()
                .cl_max_to_source
                .contains("Eq. 12.21"));
        }
    }
}
