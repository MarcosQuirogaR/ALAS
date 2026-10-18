// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reference-area and transport-planform resolution of [`WingConfig`] (kept
//! apart from the field table to bound the size of each file).

use super::config::WingConfig;
use super::planform::{
    validate_finite, validate_positive, validate_sweep, MainWingStation, MainWingStationKind,
    TransportPlanform, TransportPlanformError,
};
use crate::DesignVector;

impl WingConfig {
    /// Reference area of both semispans for `planform`, m^2, under this
    /// config's reference-wing convention: the reference trapezoid in
    /// [`super::ReferenceWingMode::Trapezoid`] (falling back to the drawn planform
    /// when the outboard lines are not physical), the drawn planform
    /// otherwise.
    #[must_use]
    pub fn reference_area_m2(&self, planform: &TransportPlanform) -> f64 {
        self.reference_wing.area_m2(planform)
    }

    /// Resolve the side-of-body/root/kink/tip planform for `design`.
    ///
    /// Existing configurations leave all optional transport fields unset and
    /// therefore produce the historical root/kink/tip geometry exactly. New
    /// configurations can make the side-of-body and outboard sweep explicit
    /// without consumers duplicating sweep or trailing-edge calculations.
    ///
    /// Sweep convention: `design.sweep_deg` is a leading-edge angle, positive
    /// aft. When a side-of-body or kink station is declared (every product
    /// preset), that one angle runs straight from the centreline to the tip,
    /// and `outboard_sweep_decrement_deg` and `outboard_le_sweep_deg` are not
    /// read; they shape only the legacy root/kink/tip replay. A published
    /// quarter-chord sweep is therefore converted to the leading edge through
    /// the outboard panel's taper before it is entered, as
    /// `tan(LE) = tan(c/4) + (c_kink - c_tip) / (4 (y_tip - y_kink))`.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when station order, dimensions,
    /// chords, or sweep angles cannot describe a finite physical wing.
    pub fn transport_planform(
        &self,
        design: &DesignVector,
    ) -> Result<TransportPlanform, TransportPlanformError> {
        validate_finite("span", design.span_m)?;
        validate_positive("span", design.span_m)?;
        validate_finite("root chord", design.root_chord_m)?;
        validate_positive("root chord", design.root_chord_m)?;
        validate_finite("kink chord", design.break_chord_m)?;
        validate_positive("kink chord", design.break_chord_m)?;
        validate_finite("tip chord", design.tip_chord_m)?;
        validate_positive("tip chord", design.tip_chord_m)?;
        validate_finite("inboard leading-edge sweep", design.sweep_deg)?;

        let wing_span_m = self.wing_span_m(design.span_m);
        validate_positive("wing span", wing_span_m)?;
        let semi_span_m = wing_span_m / 2.0;
        let side_of_body_span_fraction = self.side_of_body_span_fraction.unwrap_or(0.0);
        let kink_span_fraction = self.kink_span_fraction.unwrap_or(self.break_span_fraction);
        let transport_extension_active =
            self.side_of_body_span_fraction.is_some() || self.kink_span_fraction.is_some();
        let outboard_le_sweep_deg = if transport_extension_active {
            design.sweep_deg
        } else {
            self.outboard_le_sweep_deg
                .unwrap_or(design.sweep_deg - self.outboard_sweep_decrement_deg)
        };

        // Optional drawn inboard crank (geometry step B): the leading edge from the centreline to the kink sweeps by
        // this angle instead of the design-vector sweep; the outboard panel keeps the design-vector sweep.
        let inboard_le_sweep_deg = match self.inboard_le_sweep_deg {
            Some(sweep) if transport_extension_active => sweep,
            _ => design.sweep_deg,
        };
        validate_finite("side-of-body span fraction", side_of_body_span_fraction)?;
        validate_finite("kink span fraction", kink_span_fraction)?;
        if let Some(side_of_body_chord_ratio) = self.side_of_body_chord_ratio {
            validate_finite("side-of-body chord ratio", side_of_body_chord_ratio)?;
            validate_positive("side-of-body chord ratio", side_of_body_chord_ratio)?;
        }
        validate_finite("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        validate_sweep("inboard leading-edge sweep", inboard_le_sweep_deg)?;
        validate_sweep("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        if side_of_body_span_fraction < 0.0
            || side_of_body_span_fraction >= kink_span_fraction
            || kink_span_fraction >= 1.0
        {
            return Err(TransportPlanformError::InvalidStationOrder {
                side_of_body: side_of_body_span_fraction,
                kink: kink_span_fraction,
            });
        }
        let root = MainWingStation {
            kind: MainWingStationKind::Root,
            span_fraction: 0.0,
            y_m: 0.0,
            leading_edge_x_m: 0.0,
            chord_m: design.root_chord_m,
        };
        let kink_y_m = kink_span_fraction * semi_span_m;
        let kink = MainWingStation {
            kind: MainWingStationKind::Kink,
            span_fraction: kink_span_fraction,
            y_m: kink_y_m,
            leading_edge_x_m: kink_y_m * inboard_le_sweep_deg.to_radians().tan(),
            chord_m: design.break_chord_m,
        };
        let side_of_body_y_m = side_of_body_span_fraction * semi_span_m;
        let side_of_body = (side_of_body_span_fraction > 0.0).then(|| {
            let root_to_kink_fraction = side_of_body_span_fraction / kink_span_fraction;
            let interpolated_chord_m = design.root_chord_m
                + root_to_kink_fraction * (design.break_chord_m - design.root_chord_m);
            let leading_edge_x_m = side_of_body_y_m * inboard_le_sweep_deg.to_radians().tan();
            let kink_trailing_edge_x_m = kink.leading_edge_x_m + kink.chord_m;
            // The centreline root is a carry-through datum inside the body,
            // not an exposed aerodynamic chord. Preserve the trailing-edge
            // clip when it stays inside the root-to-kink chord envelope. If
            // that clip would make the exposed side-of-body section narrower
            // than the kink, keep the kink chord instead: an inboard section
            // must not grow outboard into the kink.
            let derived_chord_m = interpolated_chord_m
                .min(kink_trailing_edge_x_m - leading_edge_x_m)
                .max(kink.chord_m);
            MainWingStation {
                kind: MainWingStationKind::SideOfBody,
                span_fraction: side_of_body_span_fraction,
                y_m: side_of_body_y_m,
                leading_edge_x_m,
                chord_m: self
                    .side_of_body_chord_ratio
                    .map_or(derived_chord_m, |ratio| design.root_chord_m * ratio),
            }
        });
        let tip_y_m = semi_span_m;
        // A tip rake: both edges follow the straight lines (design sweep, linear
        // taper to the design tip chord) to the rake station; from there the
        // leading edge sweeps further aft so that the chord at the extreme tip
        // is `tip_rake_tip_chord_m`, with the trailing edge still on its straight
        // line to the tip.
        let rake = match (self.tip_rake_start_span_fraction, self.tip_rake_tip_chord_m) {
            (Some(fraction), Some(final_chord_m)) if transport_extension_active => {
                validate_finite("tip rake start", fraction)?;
                validate_finite("tip rake tip chord", final_chord_m)?;
                validate_positive("tip rake tip chord", final_chord_m)?;
                if fraction <= kink_span_fraction || fraction >= 1.0 {
                    return Err(TransportPlanformError::InvalidStationOrder {
                        side_of_body: kink_span_fraction,
                        kink: fraction,
                    });
                }
                Some((fraction, final_chord_m))
            }
            _ => None,
        };
        let straight_tip_leading_edge_x_m =
            kink.leading_edge_x_m + (tip_y_m - kink_y_m) * outboard_le_sweep_deg.to_radians().tan();
        let (tip_leading_edge_x_m, tip_chord_m) = match rake {
            Some((_, final_chord_m)) => (
                straight_tip_leading_edge_x_m + design.tip_chord_m - final_chord_m,
                final_chord_m,
            ),
            None => (straight_tip_leading_edge_x_m, design.tip_chord_m),
        };
        let tip = MainWingStation {
            kind: MainWingStationKind::Tip,
            span_fraction: 1.0,
            y_m: tip_y_m,
            leading_edge_x_m: tip_leading_edge_x_m,
            chord_m: tip_chord_m,
        };
        let tip_rake = rake.map(|(fraction, _)| {
            let rake_y_m = fraction * semi_span_m;
            let t = (rake_y_m - kink_y_m) / (tip_y_m - kink_y_m);
            MainWingStation {
                kind: MainWingStationKind::TipRake,
                span_fraction: fraction,
                y_m: rake_y_m,
                leading_edge_x_m: kink.leading_edge_x_m
                    + (rake_y_m - kink_y_m) * outboard_le_sweep_deg.to_radians().tan(),
                chord_m: kink.chord_m + t * (design.tip_chord_m - kink.chord_m),
            }
        });

        let planform = TransportPlanform {
            root,
            side_of_body,
            kink,
            tip,
            tip_rake,
            inboard_le_sweep_deg,
            outboard_le_sweep_deg,
        };
        let stations = planform.stations_with_rake();
        for pair in stations.windows(2) {
            if pair[1].chord_m > pair[0].chord_m + 1.0e-9 {
                return Err(TransportPlanformError::NonMonotoneChord {
                    inboard: pair[0].chord_m,
                    outboard: pair[1].chord_m,
                });
            }
        }
        if let Some(side_of_body) = planform.side_of_body {
            validate_positive("side-of-body chord", side_of_body.chord_m)?;
        }
        Ok(planform)
    }
}
