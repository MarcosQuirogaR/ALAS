// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reference-area and transport-planform resolution of [`WingConfig`] (kept
//! apart from the field table to bound the size of each file).

use super::config::WingConfig;
use super::planform::{
    MainWingStation, MainWingStationKind, TransportPlanform, TransportPlanformError,
};
use super::planform_inputs::{validate_finite, validate_positive, validate_sweep};
use crate::DesignVector;

impl WingConfig {
    /// Whether the centreline root chord is a planform datum and no chord of the wing.
    ///
    /// True with a pinned side-of-body chord: the wing inside the body is then set by
    /// the side-of-body section and the edge lines, so this chord is never lofted and
    /// states only the reference trapezoid. A user-defined section at or inboard of that
    /// station is refused at build, so it does not change this. With a derived
    /// side-of-body chord the root chord sets the exposed chord, so the order against the
    /// root still means a reverse taper. The frozen reference builder clears the pinned
    /// ratio and is never in this case.
    pub fn root_chord_is_only_a_datum(&self) -> bool {
        self.side_of_body_chord_ratio.is_some()
            && self.side_of_body_span_fraction.unwrap_or(0.0) > 0.0
    }

    /// Drops the outboard trailing-edge break, the edge stations and the swept-back tip rake,
    /// the options the frozen reference builder does not have.
    pub fn clear_outboard_edge_options(&mut self) {
        self.tip_rake_start_span_fraction = None;
        self.tip_rake_tip_chord_m = None;
        self.tip_rake_reference_tip_chord_m = None;
        self.te_break_span_fraction = None;
        self.te_break_chord_offset_m = None;
        self.te_break_2_span_fraction = None;
        self.te_break_2_chord_offset_m = None;
        self.edge_stations.clear();
    }

    /// Chord at the extreme tip of a swept-back tip for the design tip chord `tip_chord_m`, m.
    ///
    /// The rake is a feature of the tip, read off a drawing against the straight-edge tip
    /// chord `reference_tip_chord_m`: it keeps its ratio to the tip chord,
    /// `rake_chord_m * (tip_chord_m / reference_tip_chord_m)`, which is `rake_chord_m`
    /// exactly when the tip chord is the reference (the quotient is then exactly one).
    /// Without a reference the configured chord is absolute.
    #[must_use]
    pub fn tip_rake_chord_m(
        rake_chord_m: f64,
        reference_tip_chord_m: Option<f64>,
        tip_chord_m: f64,
    ) -> f64 {
        reference_tip_chord_m.map_or(rake_chord_m, |reference| {
            rake_chord_m * (tip_chord_m / reference)
        })
    }

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

        // Optional drawn inboard crank: the leading edge from the centreline to the kink sweeps by
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
        // line to the tip. The tip chord is a design variable: with a reference tip
        // chord the rake chord keeps its ratio to it (`Self::tip_rake_chord_m`), so
        // every candidate keeps the swept-back tip it was drawn with. An absolute
        // rake chord at or above the candidate's tip chord (or a reference at or
        // below the rake chord) has no swept-back tip and is refused by name rather
        // than built as a leading edge running forward of its straight line.
        let rake = match (self.tip_rake_start_span_fraction, self.tip_rake_tip_chord_m) {
            (Some(fraction), Some(rake_chord_m)) if transport_extension_active => {
                validate_finite("tip rake start", fraction)?;
                validate_finite("tip rake tip chord", rake_chord_m)?;
                validate_positive("tip rake tip chord", rake_chord_m)?;
                if let Some(reference_m) = self.tip_rake_reference_tip_chord_m {
                    validate_finite("tip rake reference tip chord", reference_m)?;
                    validate_positive("tip rake reference tip chord", reference_m)?;
                }
                if fraction <= kink_span_fraction || fraction >= 1.0 {
                    return Err(TransportPlanformError::TipRakeStartOutOfOrder {
                        kink: kink_span_fraction,
                        rake_start: fraction,
                    });
                }
                let final_chord_m = Self::tip_rake_chord_m(
                    rake_chord_m,
                    self.tip_rake_reference_tip_chord_m,
                    design.tip_chord_m,
                );
                if final_chord_m >= design.tip_chord_m {
                    return Err(TransportPlanformError::TipRakeChordNotBelowTip {
                        rake_tip_chord_m: final_chord_m,
                        tip_chord_m: design.tip_chord_m,
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

        // Outboard trailing-edge breaks (at most two): the leading edge stays on the straight
        // outboard line and the chord is the straight kink-to-tip taper chord plus the offset.
        let requested = [
            self.te_break_span_fraction
                .zip(self.te_break_chord_offset_m),
            self.te_break_2_span_fraction
                .zip(self.te_break_2_chord_offset_m),
        ];
        let mut te_breaks = Vec::new();
        if transport_extension_active {
            let rake_start = tip_rake.map_or(1.0, |rake| rake.span_fraction);
            let mut previous = kink_span_fraction;
            for (fraction, offset_m) in requested.into_iter().flatten() {
                validate_finite("trailing-edge break span", fraction)?;
                validate_finite("trailing-edge break chord offset", offset_m)?;
                if fraction <= previous || fraction >= rake_start {
                    return Err(TransportPlanformError::InvalidTrailingEdgeBreak {
                        kink: kink_span_fraction,
                        fraction,
                    });
                }
                previous = fraction;
                let break_y_m = fraction * semi_span_m;
                let t = (break_y_m - kink_y_m) / (tip_y_m - kink_y_m);
                let chord_m = kink.chord_m + t * (design.tip_chord_m - kink.chord_m) + offset_m;
                validate_positive("trailing-edge break chord", chord_m)?;
                te_breaks.push(MainWingStation {
                    kind: MainWingStationKind::TeBreak,
                    span_fraction: fraction,
                    y_m: break_y_m,
                    leading_edge_x_m: kink.leading_edge_x_m
                        + (break_y_m - kink_y_m) * outboard_le_sweep_deg.to_radians().tan(),
                    chord_m,
                });
            }
        }

        let mut planform = TransportPlanform {
            root,
            side_of_body,
            kink,
            tip,
            tip_rake,
            te_breaks,
            edge_stations: Vec::new(),
            custom_stations: self
                .custom_sections
                .iter()
                .filter(|section| {
                    section.span_fraction.is_finite()
                        && section.leading_edge_x_m.is_finite()
                        && section.chord_m.is_finite()
                })
                .map(|section| MainWingStation {
                    kind: MainWingStationKind::EdgeStation,
                    span_fraction: section.span_fraction,
                    y_m: section.span_fraction * tip.y_m,
                    leading_edge_x_m: section.leading_edge_x_m,
                    chord_m: section.chord_m,
                })
                .collect(),
            inboard_le_sweep_deg,
            outboard_le_sweep_deg,
        };
        // The chord of the straight edges never grows outboard; a trailing-edge break is a local
        // bulge of the drawn edge and may stand above its neighbours. A root chord that is only a
        // datum has no chord to compare: the physical wing starts at the side of body.
        let root_is_datum = self.root_chord_is_only_a_datum();
        let stations: Vec<_> = planform
            .stations_with_rake()
            .into_iter()
            .filter(|station| {
                station.kind != MainWingStationKind::TeBreak
                    && !(root_is_datum && station.kind == MainWingStationKind::Root)
            })
            .collect();
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
        // After the order check: an edge station is a local feature of the drawn
        // edges, as a trailing-edge break is, and may stand above its neighbours.
        if transport_extension_active {
            self.apply_edge_stations(&mut planform)?;
        }
        // The edge lines hold to the centreline: they must not cross on the way.
        if let Some(lines) = planform.inboard_edge_lines() {
            validate_positive(
                "chord at the centreline on the inboard edge lines",
                lines.trailing.x_at(0.0) - lines.leading.x_at(0.0),
            )?;
            if let Some(side_of_body) = planform.without_blends().side_of_body {
                validate_positive("side-of-body chord on the edge lines", side_of_body.chord_m)?;
            }
        }
        Ok(planform)
    }
}
