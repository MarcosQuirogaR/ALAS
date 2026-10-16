// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The manufacturer reference-wing convention: which wing the aircraft
//! reference area, span, mean aerodynamic chord (MAC) and MAC leading-edge
//! station are quoted for.
//!
//! # Construction ([`ReferenceWingMode::Trapezoid`])
//!
//! Take the OUTBOARD straight panel of the planform: the kink station and the
//! next station outboard of it (the tip-rake start when the wing declares a
//! swept-back tip, otherwise the tip). Extend that panel's leading-edge and
//! trailing-edge straight lines inboard to the aircraft centreline (y = 0) and
//! outboard to the tip station's y. The resulting single trapezoid is the
//! reference wing. Everything inboard of the kink is discarded: the Yehudi /
//! inboard trailing-edge extension, the side-of-body carry-through, any
//! fuselage fairing and the rake of the tip. All lengths are metres in the
//! XY (planform) projection, leading-edge x positive aft, y spanwise from the
//! centreline, so dihedral does not enter.
//!
//! With root chord `cr`, tip chord `ct`, taper `l = ct / cr`, semispan
//! `s = b / 2`:
//!
//! - `S = (cr + ct) s`  (both semispans)
//! - `MAC = (2/3) cr (1 + l + l^2) / (1 + l)`
//! - `y_MAC = (b/6) (1 + 2 l) / (1 + l)`; the chord at `y_MAC` equals `MAC`
//! - `x_LE,MAC = x_LE,root + (x_LE,tip - x_LE,root) y_MAC / s`
//!
//! # Published convention ([`ReferenceWingMode::Published`])
//!
//! A manufacturer's reference area, MAC and MAC station are valid only at the
//! registered geometry, so they are stored as a MAPPING onto the built
//! trapezoid above, not as frozen numbers. At preset registration
//! ([`ReferenceWingConfig::register`]) the built trapezoid (`S_t`, `MAC_t`,
//! `x_t` = `root_datum_x_m + wing_x_shift_m + trapezoid MAC LE`) is recorded.
//! A candidate (optimizer, clean sheet, GUI edit) with built trapezoid
//! (`S_c`, `MAC_c`, `x_c`, including its own `wing_x_shift_m`) gets
//!
//! - `S_ref = S_pub S_c / S_t`
//! - `c_ref = MAC_pub MAC_c / MAC_t`
//! - `x_LEMAC = x_pub + (x_c - x_t)`   (x aft of the nose tip)
//!
//! which gives the published numbers exactly at the registered geometry,
//! scales with span and chords, and moves with the wing station. The MAC
//! station is in the model frame (x aft of the nose tip): station data from a
//! TCDS (measured from a datum ahead of the nose) are converted before they
//! are stored.
//!
//! # Sources
//!
//! Raymer, Aircraft Design: A Conceptual Approach, 6th ed., ch. 4 "Wing
//! Geometry": the "reference" (trapezoidal) wing "extends through the fuselage
//! to the aircraft centerline", its area "includes the part of the reference
//! wing that sticks into the fuselage" and is the area that
//! nondimensionalises the aerodynamic coefficients; Fig. 4.17 gives the
//! trapezoid MAC and `y_MAC` used above. Raymer does not say which panel of a
//! cranked wing is extended; taking the outboard one is the manufacturer
//! type-certificate practice, and the existing
//! `Wing::theoretical_reference_mac` test in `alas-geom` reproduces the
//! EASA-certified B787-9 MAC to 0.14 % on exactly this construction (it does
//! not reproduce the A340-300 value, whose crank here is an area-calibrated
//! fit). Boeing and Airbus airport-planning "gross wing area" definitions are
//! recalled, not retrieved: use `published_area_m2` to check a preset against
//! the published figure.

use serde::{Deserialize, Serialize};

use super::planform::TransportPlanform;

/// Relative disagreement between the constructed reference trapezoid and a
/// published reference area above which a warning is raised (1 %).
pub const PUBLISHED_AREA_TOLERANCE: f64 = 0.01;

/// Which wing the reference quantities describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReferenceWingMode {
    /// The drawn planform itself (every station, inboard extensions
    /// included). The historical behaviour.
    #[default]
    Drawn,
    /// The manufacturer reference trapezoid from the outboard panel; see the
    /// module documentation.
    Trapezoid,
    /// The manufacturer's PUBLISHED reference values as a CONVENTION mapping
    /// (see the module documentation, "Published convention"): the published
    /// area, MAC and MAC leading-edge station hold exactly at the registered
    /// geometry and follow the built trapezoid of any other candidate (span,
    /// chords, sweep) and its `wing_x_shift_m`. The drawn planform keeps
    /// the wetted area, lattice panels and mass. The reference span is the
    /// drawn span. A value that is not published falls back to the reference
    /// trapezoid of the candidate planform (MAC and its leading edge), then
    /// to the drawn planform's own. Needs `published_area_m2`.
    Published,
}

/// The `reference_wing` group of [`super::WingConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceWingConfig {
    /// How the reference wing is obtained from the drawn planform.
    #[serde(default)]
    pub mode: ReferenceWingMode,
    /// Manufacturer-published reference area, m^2: a consistency check only.
    /// When the constructed trapezoid differs from it by more than 1 % the
    /// geometry build records a warning; it never changes the geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_area_m2: Option<f64>,
    /// Manufacturer-published mean aerodynamic chord, m
    /// ([`ReferenceWingMode::Published`] only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_mac_m: Option<f64>,
    /// Manufacturer-published leading-edge station of the MAC, m aft of the
    /// aircraft nose tip, in the model frame ([`ReferenceWingMode::Published`]
    /// only; TCDS stations are measured from a datum ahead of the nose and
    /// must be converted first). The value belongs to the registered geometry
    /// and moves with `wing_x_shift_m` and the built planform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_mac_le_x_m: Option<f64>,
    /// The built reference trapezoid at the registered geometry
    /// ([`RegisteredBasis`]), filled in by [`Self::register`] when a preset is
    /// loaded. Without it the published numbers are used as fixed values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered: Option<RegisteredBasis>,
}

/// The built reference trapezoid of the registered geometry: the denominators
/// of the [`ReferenceWingMode::Published`] mapping.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredBasis {
    /// Trapezoid area of both semispans, m^2.
    pub area_m2: f64,
    /// Trapezoid MAC, m.
    pub mac_m: f64,
    /// Trapezoid MAC leading edge, m aft of the nose tip
    /// (`root_datum_x_m + wing_x_shift_m + trapezoid MAC LE`).
    pub mac_le_x_m: f64,
}

/// The reference quantities of [`ReferenceWingMode::Published`] for one
/// candidate (`None` where the manufacturer publishes none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedReference {
    /// Reference area, m^2.
    pub area_m2: f64,
    /// MAC, m.
    pub mac_m: Option<f64>,
    /// MAC leading edge, m aft of the nose tip.
    pub mac_le_x_m: Option<f64>,
}

impl ReferenceWingConfig {
    /// Reference area of both semispans for `planform`, m^2: the published area (`Published`), the reference
    /// trapezoid (`Trapezoid`, falling back to the drawn planform when the outboard lines are not physical) or the
    /// drawn planform.
    #[must_use]
    pub fn area_m2(&self, planform: &TransportPlanform) -> f64 {
        let drawn = || planform.reference_area_m2();
        match self.mode {
            ReferenceWingMode::Published => self
                .published_reference(planform, 0.0)
                .map_or_else(drawn, |p| p.area_m2),
            ReferenceWingMode::Trapezoid => planform
                .reference_trapezoid()
                .map_or_else(drawn, |t| t.area_m2),
            ReferenceWingMode::Drawn => drawn(),
        }
    }

    /// The [`ReferenceWingMode::Published`] reference: the manufacturer's `area_m2`, MAC and MAC leading-edge
    /// station (`None` where the manufacturer publishes none).
    #[must_use]
    pub const fn published(area_m2: f64, mac_m: Option<f64>, mac_le_x_m: Option<f64>) -> Self {
        Self {
            mode: ReferenceWingMode::Published,
            published_area_m2: Some(area_m2),
            published_mac_m: mac_m,
            published_mac_le_x_m: mac_le_x_m,
            registered: None,
        }
    }

    /// Record the built reference trapezoid of the registered geometry
    /// (`planform` at wing datum `x_wing_global_m` = `root_datum_x_m +
    /// wing_x_shift_m`). Only for [`ReferenceWingMode::Published`] without a
    /// basis; a no-op otherwise or when the trapezoid is not constructible.
    pub fn register(&mut self, planform: &TransportPlanform, x_wing_global_m: f64) {
        if self.mode != ReferenceWingMode::Published || self.registered.is_some() {
            return;
        }
        self.registered = planform.reference_trapezoid().map(|t| RegisteredBasis {
            area_m2: t.area_m2,
            mac_m: t.mac_m,
            mac_le_x_m: x_wing_global_m + t.mac_leading_edge_x_m,
        });
    }

    /// The published reference mapped onto `planform` built at wing datum
    /// `x_wing_global_m`: published value x (candidate trapezoid / registered
    /// trapezoid) for area and MAC, and the published LEMAC plus the shift of
    /// the trapezoid MAC leading edge since registration. Exactly the
    /// published numbers at the registered geometry. Without a basis (or when
    /// the candidate trapezoid is not constructible) the published numbers
    /// are returned unscaled. `None` unless `Published` with an area.
    #[must_use]
    pub fn published_reference(
        &self,
        planform: &TransportPlanform,
        x_wing_global_m: f64,
    ) -> Option<PublishedReference> {
        if self.mode != ReferenceWingMode::Published {
            return None;
        }
        let area_m2 = self.published_area_m2?;
        let mapped = self.registered.zip(planform.reference_trapezoid());
        let Some((basis, t)) = mapped else {
            return Some(PublishedReference {
                area_m2,
                mac_m: self.published_mac_m,
                mac_le_x_m: self.published_mac_le_x_m,
            });
        };
        Some(PublishedReference {
            area_m2: area_m2 * (t.area_m2 / basis.area_m2),
            mac_m: self.published_mac_m.map(|m| m * (t.mac_m / basis.mac_m)),
            mac_le_x_m: self
                .published_mac_le_x_m
                .map(|x| x + ((x_wing_global_m + t.mac_leading_edge_x_m) - basis.mac_le_x_m)),
        })
    }

    /// Whether this is the default (serialization is then omitted).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The published-area check, `None` when it passes or is not given.
    ///
    /// Returns `(relative_difference)` of `constructed_area_m2` against the
    /// published value when it exceeds [`PUBLISHED_AREA_TOLERANCE`].
    #[must_use]
    pub fn published_area_disagreement(&self, constructed_area_m2: f64) -> Option<f64> {
        let published = self.published_area_m2?;
        let relative = (constructed_area_m2 - published) / published;
        (relative.abs() > PUBLISHED_AREA_TOLERANCE || !relative.is_finite()).then_some(relative)
    }

    /// Validation message when `published_area_m2` is set but unusable.
    #[must_use]
    pub fn validation_error(&self) -> Option<String> {
        for (name, value) in [
            ("published_mac_m", self.published_mac_m),
            ("published_mac_le_x_m", self.published_mac_le_x_m),
        ] {
            if let Some(v) = value {
                if !(v.is_finite() && v > 0.0) {
                    return Some(format!(
                        "geometry.wing.reference_wing.{name} must be finite and positive, got {v}"
                    ));
                }
            }
        }
        if self.mode == ReferenceWingMode::Published && self.published_area_m2.is_none() {
            return Some(
                "geometry.wing.reference_wing.mode = Published needs published_area_m2".to_owned(),
            );
        }
        match self.published_area_m2 {
            Some(value) if !(value.is_finite() && value > 0.0) => Some(format!(
                "geometry.wing.reference_wing.published_area_m2 must be finite and positive, got {value}"
            )),
            _ => None,
        }
    }
}

/// The reference trapezoid, in the wing datum frame of the
/// [`TransportPlanform`] (leading-edge x relative to the centreline root).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ReferenceTrapezoid {
    /// Chord at the centreline, m.
    pub root_chord_m: f64,
    /// Chord at the tip station, m.
    pub tip_chord_m: f64,
    /// Leading-edge x at the centreline, m.
    pub root_leading_edge_x_m: f64,
    /// Leading-edge x at the tip station, m.
    pub tip_leading_edge_x_m: f64,
    /// Semispan (tip station y), m.
    pub semi_span_m: f64,
    /// Reference span, m (twice the semispan).
    pub span_m: f64,
    /// Reference area of both semispans, m^2.
    pub area_m2: f64,
    /// Mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Spanwise station of the MAC, m.
    pub mac_y_m: f64,
    /// Leading-edge x of the MAC, m.
    pub mac_leading_edge_x_m: f64,
}

impl TransportPlanform {
    /// The manufacturer reference trapezoid from the outboard panel; see the
    /// module documentation of [`super::reference`] for the construction.
    ///
    /// Returns `None` when the outboard lines do not give a physical
    /// trapezoid (a non-positive chord at the centreline or tip).
    #[must_use]
    pub fn reference_trapezoid(&self) -> Option<ReferenceTrapezoid> {
        let inner = self.kink;
        let outer = self.tip_rake.unwrap_or(self.tip);
        let run = outer.y_m - inner.y_m;
        if !(run.is_finite() && run > 0.0) {
            return None;
        }
        let semi_span_m = self.tip.y_m;
        let chord_slope = (outer.chord_m - inner.chord_m) / run;
        let le_slope = (outer.leading_edge_x_m - inner.leading_edge_x_m) / run;
        let chord_at = |y: f64| inner.chord_m + (y - inner.y_m) * chord_slope;
        let le_at = |y: f64| inner.leading_edge_x_m + (y - inner.y_m) * le_slope;
        let (root_chord_m, tip_chord_m) = (chord_at(0.0), chord_at(semi_span_m));
        if !(root_chord_m.is_finite()
            && tip_chord_m.is_finite()
            && root_chord_m > 0.0
            && tip_chord_m > 0.0
            && semi_span_m > 0.0)
        {
            return None;
        }
        let (root_x, tip_x) = (le_at(0.0), le_at(semi_span_m));
        let taper = tip_chord_m / root_chord_m;
        let mac_m = (2.0 / 3.0) * root_chord_m * (1.0 + taper + taper * taper) / (1.0 + taper);
        let mac_y_m = semi_span_m / 3.0 * (1.0 + 2.0 * taper) / (1.0 + taper);
        Some(ReferenceTrapezoid {
            root_chord_m,
            tip_chord_m,
            root_leading_edge_x_m: root_x,
            tip_leading_edge_x_m: tip_x,
            semi_span_m,
            span_m: 2.0 * semi_span_m,
            area_m2: (root_chord_m + tip_chord_m) * semi_span_m,
            mac_m,
            mac_y_m,
            mac_leading_edge_x_m: root_x + (tip_x - root_x) * mac_y_m / semi_span_m,
        })
    }
}

// Tests assert on fixtures constructed here.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::planform::{MainWingStation, MainWingStationKind};
    use super::*;

    fn station(kind: MainWingStationKind, y: f64, le: f64, chord: f64) -> MainWingStation {
        MainWingStation {
            kind,
            span_fraction: 0.0,
            y_m: y,
            leading_edge_x_m: le,
            chord_m: chord,
        }
    }

    /// Cranked wing with a known answer. Outboard panel: kink y = 10, LE 5,
    /// chord 8; tip y = 30, LE 25, chord 2. Extended to y = 0 the LE is at
    /// x = -5 and the chord is 8 + 10 * 0.3 = 11; tip chord 2. Inboard panel
    /// (not part of the reference) is deliberately fat and straked.
    fn cranked() -> TransportPlanform {
        TransportPlanform {
            root: station(MainWingStationKind::Root, 0.0, 0.0, 15.0),
            side_of_body: None,
            kink: station(MainWingStationKind::Kink, 10.0, 5.0, 8.0),
            tip: station(MainWingStationKind::Tip, 30.0, 25.0, 2.0),
            tip_rake: None,
            inboard_le_sweep_deg: 26.0,
            outboard_le_sweep_deg: 45.0,
        }
    }

    #[test]
    fn trapezoid_of_a_cranked_wing_matches_the_hand_calculation() {
        let t = cranked().reference_trapezoid().expect("physical");
        // cr = 11, ct = 2, s = 30: S = 13 * 30 = 390; taper = 2/11.
        assert!((t.root_chord_m - 11.0).abs() < 1e-12);
        assert!((t.tip_chord_m - 2.0).abs() < 1e-12);
        assert!((t.root_leading_edge_x_m + 5.0).abs() < 1e-12);
        assert!((t.area_m2 - 390.0).abs() < 1e-9);
        assert!((t.span_m - 60.0).abs() < 1e-12);
        let l = 2.0 / 11.0;
        let mac = 2.0 / 3.0 * 11.0 * (1.0 + l + l * l) / (1.0 + l);
        assert!((t.mac_m - mac).abs() < 1e-12, "{} vs {mac}", t.mac_m);
        // The chord at y_MAC equals MAC and the LE x lies on the LE line.
        let chord_at_mac = 11.0 + (2.0 - 11.0) * t.mac_y_m / 30.0;
        assert!((chord_at_mac - t.mac_m).abs() < 1e-12);
        let le_at_mac = -5.0 + t.mac_y_m;
        assert!((t.mac_leading_edge_x_m - le_at_mac).abs() < 1e-12);
        // Independent numeric integral of c^2 and c * x_le over the semispan.
        let n = 200_000;
        let (mut sc, mut sc2, mut scx) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let y = (f64::from(i) + 0.5) * 30.0 / f64::from(n);
            let c = 11.0 + (2.0 - 11.0) * y / 30.0;
            sc += c;
            sc2 += c * c;
            scx += c * (-5.0 + y);
        }
        assert!((sc2 / sc - t.mac_m).abs() < 1e-6);
        assert!((scx / sc - t.mac_leading_edge_x_m).abs() < 1e-6);
    }

    #[test]
    fn a_single_panel_wing_is_its_own_reference() {
        let planform = TransportPlanform {
            root: station(MainWingStationKind::Root, 0.0, 0.0, 6.0),
            side_of_body: None,
            kink: station(MainWingStationKind::Kink, 5.0, 2.0, 5.0),
            tip: station(MainWingStationKind::Tip, 15.0, 6.0, 3.0),
            tip_rake: None,
            inboard_le_sweep_deg: 21.8,
            outboard_le_sweep_deg: 21.8,
        };
        let t = planform.reference_trapezoid().expect("physical");
        assert!((t.area_m2 - planform.reference_area_m2()).abs() < 1e-9);
    }

    #[test]
    fn a_swept_back_tip_is_replaced_by_the_straight_outboard_lines() {
        let mut planform = cranked();
        // Rake start on the straight lines at y = 20: LE 15, chord 5.
        planform.tip_rake = Some(station(MainWingStationKind::TipRake, 20.0, 15.0, 5.0));
        planform.tip = station(MainWingStationKind::Tip, 30.0, 29.0, 1.0);
        let t = planform.reference_trapezoid().expect("physical");
        assert!((t.tip_chord_m - 2.0).abs() < 1e-12);
        assert!((t.area_m2 - 390.0).abs() < 1e-9);
    }

    #[test]
    fn published_mode_needs_a_positive_area_and_positive_mac_values() {
        let published = ReferenceWingConfig {
            mode: ReferenceWingMode::Published,
            published_area_m2: Some(361.6),
            published_mac_m: Some(7.27),
            published_mac_le_x_m: Some(28.083),
            registered: None,
        };
        assert!(published.validation_error().is_none());
        let no_area = ReferenceWingConfig {
            published_area_m2: None,
            ..published
        };
        assert!(no_area.validation_error().is_some());
        let bad_mac = ReferenceWingConfig {
            published_mac_m: Some(-1.0),
            ..published
        };
        assert!(bad_mac.validation_error().is_some());
    }

    #[test]
    fn published_area_check_flags_more_than_one_percent_only() {
        let config = ReferenceWingConfig {
            mode: ReferenceWingMode::Trapezoid,
            published_area_m2: Some(390.0),
            ..Default::default()
        };
        assert!(config.published_area_disagreement(393.0).is_none());
        assert!(config.published_area_disagreement(394.5).is_some());
        assert!(ReferenceWingConfig::default()
            .published_area_disagreement(1.0)
            .is_none());
    }
}
