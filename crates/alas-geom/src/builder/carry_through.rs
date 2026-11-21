// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main wing inside the fuselage.
//!
//! The planform's centreline root (`TransportPlanform::root`) is a design datum:
//! the theoretical chord and leading edge the inboard panel extends to, which
//! the design vector, the reference area and the mass correlations are stated
//! on. Its chord is not a section of the aircraft. The physical wing is two straight
//! edges, a fillet is a blend body and not wing:
//!
//! - Each edge stays on its own inboard line, the longest straight run of its built
//!   polyline between the side of body and the kink, from `y = 0` to where that run
//!   starts. A fillet or a blend is a short run beside it, so it does not set the
//!   line, and the fractions that place the runs are not design variables, so the
//!   choice does not switch between candidates. `TransportPlanform::inboard_edge_lines`
//!   is the one place that decides it. Every section inside that range lies on the
//!   lines, the side of body included, and the chord grows towards `y = 0` because the
//!   leading edge sweeps more than the trailing edge.
//! - The fillet stations stay in the configuration as data for a drawn blend. The
//!   lattice, the box, the tanks and the masses do not read them. The reference
//!   planform does, and it is captured before the replacement: `S_ref`, `c_ref`,
//!   `b_ref` and the gross area of [`PlanformDatum`] are the published and design
//!   numbers whatever the blend is.
//! - A wingtip device's trailing-edge flare is drawn surface, not planform. Where a
//!   wing declares one, the drawn trailing edge leaves its straight line at the last
//!   station on it (the last outboard station with no trailing-edge offset) and runs
//!   aft through the edge stations outboard of it to the flared tip. The datum's
//!   gross area, gross MAC and gross MAC leading edge keep the straight trailing
//!   edge outboard of that station (Learjet 60: FlightSafety Learjet 60 Pilot
//!   Training Manual fig 1-2 plan view, the trailing edge on the straight line to
//!   y 6.11 m, then flaring into the winglet root). The drawn wing keeps the flare;
//!   the datum takes the offsets off its chords, linear in span between the
//!   stations as the loft draws them.
//! - Only the trailing-edge flare is taken out. A drawn leading-edge tip rake stays
//!   in the gross fields, as on every raked preset: they are straight on the
//!   trailing edge and drawn (raked) on the leading edge. They are therefore not the
//!   straight-edge planform of `tip_chord_m` and `tip_le_x_m`, which are straight on
//!   both edges; on the Learjet 60 the leading-edge rake triangle they keep is
//!   0.049 m^2 over both semispans. Taking the rake out as well would change every
//!   raked preset with a side of body and is a separate item.
//! - The panels inboard of the side of body are flat at the side-of-body height and at
//!   the side-of-body twist: the centre section is a constant-incidence box. A section
//!   twists about its leading edge, so its trailing edge is at `x_le + c cos(twist)`;
//!   the inner chords are set from that projected edge, which carries on from the
//!   projected trailing edge at the side of body with the slope of the line.
//!
//! Two things were wrong with the datum lofted as a section, and this shape keeps both
//! away:
//!
//! - A section in a dihedral frame is normal to its tilted spanwise axis, so its
//!   twist moved the trailing edge off the centreline by
//!   `c sin(dihedral) sin(twist)` and opened a notch between the two halves that
//!   read as a short chord at `y = 0`. On the flat inner panels the spanwise axis
//!   is the aircraft y axis and the `y = 0` section lies in the symmetry plane.
//! - The spar box inside the body must have the width and the station of the box at
//!   the side of body, which is what a centre wing box is. The triangle is outside
//!   the spars: [`Wing::carried_box`] holds the side-of-body section across the
//!   body for the consumers of the box, recorded in [`PlanformDatum`].
//!
//! The section keeps the side-of-body depth: a centre box does not thicken as the
//! straight edges widen its chord, so the sections inboard of the side of body
//! carry the side-of-body section thinned by the chord ratio and the depth in
//! metres is the side-of-body depth at every station, `y = 0` included. The design twist
//! line inboard of the side of body stays in [`PlanformDatum`], for the box and for
//! the washout measures of the design variable.
//!
//! The datum survives in [`PlanformDatum`], so `S_ref`, `c_ref`, the taper, the
//! mean sweep and the correlations stated on the root station (see
//! [`Wing::design_root`]) are the same numbers with the physical wing inside the
//! body as they were with the datum lofted. The lofted sections feed what is
//! aerodynamic about the whole wing: the lattice, projected and unfolded area, the
//! wing's own MAC and its aerodynamic centre, the exports. The carried box feeds
//! the spar-box and tank volumes, the mass and structure integrals, the wing-box
//! stretch of the lower holds, the exposed-wing drag terms and the wing station of
//! the tail arms and balance, none of which the part buried in the body should move.
//!
//! Frame: x aft, y to starboard, z up, metres, in the aircraft axes.

use std::borrow::Cow;

use alas_config::{
    DesignVector, MainWingStationKind, TransportPlanform, WingHeights, STATION_SPAN_TOLERANCE,
};

use super::{AircraftBuilder, GeometryContract};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::{PlanformDatum, Wing, WingXSec};
use crate::airfoil_library::{morph_airfoil, DEFAULT_MORPH_N_POINTS};

impl AircraftBuilder {
    /// Whether the side-of-body section is carried through the fuselage: product
    /// geometry with a side-of-body station. A user-defined section at or inboard of
    /// it has been refused by then.
    pub(super) fn carries_through_body(&self, planform: &TransportPlanform) -> bool {
        self.geometry_contract == GeometryContract::Product && planform.side_of_body.is_some()
    }

    /// The loft sections between the root and the kink: the side of body when it
    /// is lofted, and the inboard edge stations. Both sit on the root-to-kink
    /// twist line and use the root section.
    ///
    /// A derived side-of-body chord lies on the straight root-to-kink panel, and
    /// meshing that station only when its trailing-edge clip happened to be active
    /// made the lattice section count depend on the design vector (adjacent
    /// candidates differed by eight sections and fifteen percent in induced drag),
    /// so it is meshed either always, when the wing is carried through the body,
    /// or only when the chord is pinned.
    pub(super) fn inboard_loft_sections(
        &self,
        planform: &TransportPlanform,
        heights: &WingHeights,
        root_section: &Airfoil,
    ) -> Vec<WingXSec> {
        let wing = &self.geometry.wing;
        let side_of_body = planform.side_of_body.filter(|_| {
            wing.side_of_body_chord_ratio.is_some() || self.carries_through_body(planform)
        });
        side_of_body
            .iter()
            .chain(
                planform
                    .edge_stations
                    .iter()
                    .filter(|station| station.y_m < planform.kink.y_m),
            )
            .map(|station| {
                let fraction = station.y_m / planform.kink.y_m;
                let z_m = if heights.interior.is_empty() {
                    heights.root_z_m + fraction * (heights.break_z_m - heights.root_z_m)
                } else {
                    heights.at(planform, station.y_m)
                };
                WingXSec::new(
                    [station.leading_edge_x_m, station.y_m, z_m],
                    station.chord_m,
                    wing.root_twist_deg + fraction * (wing.break_twist_deg - wing.root_twist_deg),
                    root_section.clone(),
                )
            })
            .collect()
    }

    /// Put the sections inboard of the edge runs on the two lines and flatten the
    /// panels inside the body (the module doc states the wing), recording the datum
    /// they were lofted from. Call last: the height at the side of body is the one
    /// the profile interpolation left there. `tip_flare_m` is the wingtip device's
    /// trailing-edge flare in the built tip chord, which the datum leaves out.
    pub(super) fn carry_side_of_body_through(
        &self,
        dv: &DesignVector,
        planform: &TransportPlanform,
        tip_flare_m: f64,
        wing: &mut Wing,
    ) {
        let (Some(side_of_body), Some(lines)) = (
            planform
                .without_blends()
                .side_of_body
                .filter(|_| self.carries_through_body(planform)),
            planform.inboard_edge_lines(),
        ) else {
            return;
        };
        let tolerance_m = 1.0e-9 * planform.tip.y_m;
        let Some(index) = wing
            .xsecs
            .iter()
            .position(|xsec| (xsec.xyz_le[1] - side_of_body.y_m).abs() <= tolerance_m)
        else {
            return;
        };
        let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
            return;
        };
        let (gross_area_m2, mac) = {
            let unflared = self.without_tip_flare(planform, tip_flare_m, wing);
            (unflared.projected_area(), unflared.mac_station())
        };
        let x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        // A swept-back tip is a device of the leading edge: the straight edges
        // reach the design tip chord, which is what the taper and sweep describe.
        let straight_tip_x_m = if planform.tip_rake.is_some() {
            x_wing_global
                + planform.kink.leading_edge_x_m
                + (planform.tip.y_m - planform.kink.y_m)
                    * planform.outboard_le_sweep_deg.to_radians().tan()
        } else {
            tip.xyz_le[0]
        };
        let twist_deg = wing.xsecs[index].twist;
        // The design planform, measured on the drawn wing before the blends leave it
        // and with the trailing edge straight to the tip, like the tip chord below.
        let datum = PlanformDatum {
            root_le_m: root.xyz_le,
            root_chord_m: root.chord,
            root_chord_axis: wing.frame_of_xsec(0).0,
            root_twist_deg: root.twist,
            side_of_body_twist_deg: twist_deg,
            tip_le_x_m: straight_tip_x_m,
            // From the planform, not the built tip: a wingtip device's trailing-edge
            // flare belongs to the drawn surface and not to the taper and sweep.
            tip_chord_m: if planform.tip_rake.is_some() {
                dv.tip_chord_m
            } else {
                planform.tip.chord_m
            },
            gross_area_m2,
            gross_mac_m: mac.chord_m,
            gross_mac_le_x_m: mac.x_le_m,
            side_of_body_y_m: side_of_body.y_m,
            side_of_body_le_x_m: x_wing_global + side_of_body.leading_edge_x_m,
            side_of_body_chord_m: side_of_body.chord_m,
        };
        for xsec in &mut wing.xsecs[index..] {
            let y_m = xsec.xyz_le[1];
            if lines.replaces_at(y_m) && !planform.is_custom_station(y_m) {
                let (leading_x_m, chord_m) =
                    lines.section_at(y_m, xsec.xyz_le[0] - x_wing_global, xsec.chord);
                xsec.xyz_le[0] = x_wing_global + leading_x_m;
                xsec.chord = chord_m;
            }
        }
        let z_m = wing.xsecs[index].xyz_le[2];
        // How far the twist brings the side-of-body trailing edge forward of
        // `x_le + c`: the edge lines are stated on `x_le + c`, the sections end at
        // `x_le + c cos(twist)`.
        let cos_twist = twist_deg.to_radians().cos();
        let twist_shift_m = datum.side_of_body_chord_m * (1.0 - cos_twist);
        for xsec in &mut wing.xsecs[..index] {
            let y_m = xsec.xyz_le[1];
            let leading_x_m = lines.leading.x_at(y_m);
            let trailing_x_m = lines.trailing.x_at(y_m) - twist_shift_m;
            xsec.xyz_le[0] = x_wing_global + leading_x_m;
            xsec.xyz_le[2] = z_m;
            xsec.twist = twist_deg;
            xsec.chord = (trailing_x_m - leading_x_m) / cos_twist;
        }
        // The centre section is a box of the side-of-body depth: the chord grows
        // inboard with the straight edges, the depth does not, so the section
        // thins in t/c by the chord ratio instead of thickening towards y = 0.
        let sob_airfoil = wing.xsecs[index].airfoil.clone();
        let sob_chord_m = wing.xsecs[index].chord;
        for xsec in &mut wing.xsecs[..index] {
            let factor = sob_chord_m / xsec.chord;
            xsec.airfoil = if (factor - 1.0).abs() > 1.0e-12 {
                morph_airfoil(
                    &sob_airfoil.coordinates,
                    factor,
                    1.0,
                    DEFAULT_MORPH_N_POINTS,
                )
            } else {
                sob_airfoil.clone()
            };
        }
        wing.planform_datum = Some(datum);
    }

    /// `wing` with its trailing edge back on the straight line outboard of the last
    /// station on it, borrowed as built when the wing tip has no flare (module doc).
    /// The last station on the line is the last outboard lofted station without a
    /// trailing-edge offset: the kink, a trailing-edge break, the rake start, a
    /// user-defined section, or an edge station that declares none. Outboard of it
    /// every edge station's declared trailing-edge offset and, at the tip, the flare
    /// (with the offset of an edge station on the tip of a raked wing) come off the
    /// chords, interpolated linearly in span between the stations as the loft and
    /// the mesh interpolate the chord. The leading edge stays as drawn.
    fn without_tip_flare<'w>(
        &self,
        planform: &TransportPlanform,
        tip_flare_m: f64,
        wing: &'w Wing,
    ) -> Cow<'w, Wing> {
        if tip_flare_m == 0.0 {
            return Cow::Borrowed(wing);
        }
        let config = &self.geometry.wing;
        let tip_y_m = planform.tip.y_m;
        let tolerance_m = 1.0e-9 * tip_y_m;
        // The span an edge station is resolved to, as `apply_edge_stations` does.
        let declared_te_offset_m = |y_m: f64| {
            config
                .edge_stations
                .iter()
                .find(|edge| (edge.span_fraction * tip_y_m - y_m).abs() <= tolerance_m)
                .map_or(0.0, |edge| edge.te_offset_m)
        };
        let tip_edge_offset_m = if planform.tip_rake.is_some() {
            config
                .edge_stations
                .iter()
                .find(|edge| {
                    (edge.span_fraction - planform.tip.span_fraction).abs()
                        <= STATION_SPAN_TOLERANCE
                })
                .map_or(0.0, |edge| edge.te_offset_m)
        } else {
            0.0
        };
        let outboard: Vec<(f64, f64)> = planform
            .outboard_stations()
            .iter()
            .filter(|station| station.y_m < tip_y_m - tolerance_m)
            .map(|station| {
                let offset_m = if station.kind == MainWingStationKind::EdgeStation {
                    declared_te_offset_m(station.y_m)
                } else {
                    0.0
                };
                (station.y_m, offset_m)
            })
            .collect();
        let custom_y_m = config
            .custom_sections
            .iter()
            .map(|section| section.span_fraction * tip_y_m)
            .filter(|&y_m| y_m < tip_y_m - tolerance_m);
        let start_y_m = outboard
            .iter()
            .filter(|&&(_, offset_m)| offset_m == 0.0)
            .map(|&(y_m, _)| y_m)
            .chain(custom_y_m)
            .fold(planform.kink.y_m, f64::max);
        let knots: Vec<(f64, f64)> = std::iter::once((start_y_m, 0.0))
            .chain(
                outboard
                    .into_iter()
                    .filter(|&(y_m, _)| y_m > start_y_m + tolerance_m),
            )
            .chain(std::iter::once((tip_y_m, tip_edge_offset_m + tip_flare_m)))
            .collect();
        let mut unflared = wing.clone();
        for xsec in &mut unflared.xsecs {
            let y_m = xsec.xyz_le[1];
            let Some(pair) = knots
                .windows(2)
                .find(|pair| y_m <= pair[1].0 + tolerance_m)
                .filter(|_| y_m > start_y_m + tolerance_m)
            else {
                continue;
            };
            let ((y_in_m, inboard_m), (y_out_m, outboard_m)) = (pair[0], pair[1]);
            let t = ((y_m - y_in_m) / (y_out_m - y_in_m)).min(1.0);
            xsec.chord -= inboard_m + t * (outboard_m - inboard_m);
        }
        Cow::Owned(unflared)
    }
}

// A failed expect in a test is the assertion failing on a registered preset.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use crate::builder::AircraftBuilder;

    #[test]
    fn the_edges_stay_on_their_lines_to_the_centreline_and_the_box_holds_the_side_of_body() {
        for preset in alas_config::presets::registry() {
            let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .expect("preset builds");
            let wing = &plane.wings[0];
            let Some(datum) = wing.planform_datum else {
                continue;
            };
            let name = preset.name;
            let planform = preset
                .geometry
                .wing
                .transport_planform(&preset.design_vector)
                .expect("preset planform");
            let lines = planform.inboard_edge_lines().expect("a side of body");
            let x_global =
                preset.geometry.wing.root_datum_x_m + preset.design_vector.wing_x_shift_m;
            let (le_sob, chord_sob) = (datum.side_of_body_le_x_m, datum.side_of_body_chord_m);
            let twist_sob = datum.side_of_body_twist_deg;
            let te_sob = le_sob + chord_sob * twist_sob.to_radians().cos();
            let y_sob = datum.side_of_body_y_m;
            let trailing_x = |index: usize| wing.xyz_of_xsec(index, 1.0, 0.0)[0];
            let apex = &wing.xsecs[0];
            assert!(
                apex.xyz_le[1].abs() < 1e-12,
                "{name}: apex off the centreline"
            );
            let carried = wing.carried_box();
            let mut previous_chord = f64::INFINITY;
            for (index, xsec) in wing.xsecs.iter().enumerate() {
                let y = xsec.xyz_le[1];
                if y < lines.leading.run_start_y_m - 1e-9 {
                    assert!(
                        (xsec.xyz_le[0] - x_global - lines.leading.x_at(y)).abs() < 1e-9,
                        "{name}: section {index} LE off its line"
                    );
                }
                if y >= y_sob * (1.0 - 1e-9) {
                    if y < lines.trailing.run_start_y_m - 1e-9 {
                        assert!(
                            (xsec.xyz_le[0] + xsec.chord - x_global - lines.trailing.x_at(y)).abs()
                                < 1e-9,
                            "{name}: section {index} TE off its line"
                        );
                    }
                    continue;
                }
                assert!(
                    xsec.chord <= previous_chord + 1e-9,
                    "{name}: chord does not grow towards the centreline"
                );
                previous_chord = xsec.chord;
                let te = te_sob + lines.trailing.slope * (y - y_sob);
                assert!(
                    (trailing_x(index) - te).abs() < 1e-9,
                    "{name}: section {index} projected TE"
                );
                assert_eq!(
                    xsec.xyz_le[2], apex.xyz_le[2],
                    "{name}: inner panel not flat"
                );
                assert!(
                    (xsec.twist - twist_sob).abs() < 1e-12,
                    "{name}: section {index} twist not held"
                );
                let samples: Vec<f64> = (0..=100).map(|k| f64::from(k) / 100.0).collect();
                let depth =
                    |i: usize| wing.xsecs[i].airfoil.max_thickness(&samples) * wing.xsecs[i].chord;
                let sob_index = wing
                    .xsecs
                    .iter()
                    .position(|s| (s.xyz_le[1] - y_sob).abs() < 1e-9)
                    .expect("side of body section");
                assert!(
                    (depth(index) - depth(sob_index)).abs() < 2.0e-3 * depth(sob_index),
                    "{name}: section {index} depth differs from the side-of-body depth"
                );
                let boxed = &carried.xsecs[index];
                assert!(
                    (boxed.xyz_le[0] - le_sob).abs() < 1e-9,
                    "{name}: box station"
                );
                assert!((boxed.chord - chord_sob).abs() < 1e-9, "{name}: box chord");
                let design = datum.root_twist_deg
                    + y / y_sob * (datum.side_of_body_twist_deg - datum.root_twist_deg);
                assert!((boxed.twist - design).abs() < 1e-12, "{name}: box twist");
            }
            assert!(
                apex.chord >= chord_sob - 1e-9,
                "{name}: short chord at y = 0"
            );
            assert_eq!(
                wing.carried_root(),
                (carried.xsecs[0].xyz_le, carried.xsecs[0].chord)
            );
        }
    }

    #[test]
    fn the_datum_keeps_the_trailing_edge_straight_outboard_of_the_last_station_on_it() {
        let entry = alas_config::presets::get("LJ60").expect("preset");
        let wing_config = &entry.geometry.wing;
        let flare = wing_config
            .wingtip_device
            .as_ref()
            .and_then(|device| device.root_te_flare)
            .expect("the LJ60 winglet flare");
        let kink = wing_config.kink_span_fraction.expect("a kink");
        // The last outboard edge station on the straight trailing edge.
        let start = wing_config
            .edge_stations
            .iter()
            .filter(|edge| edge.span_fraction > kink && edge.te_offset_m == 0.0)
            .map(|edge| edge.span_fraction)
            .fold(kink, f64::max);
        let flared_stations: Vec<_> = wing_config
            .edge_stations
            .iter()
            .filter(|edge| edge.span_fraction > start)
            .copied()
            .collect();
        assert!(
            flared_stations.iter().any(|edge| edge.te_offset_m > 0.0),
            "the flare starts on an edge station"
        );
        // The same wing drawn with the straight trailing edge: no offset outboard of
        // the start and no flare at the tip, the stations lofted where they were.
        let mut straight = entry.geometry.clone();
        for edge in &mut straight.wing.edge_stations {
            if edge.span_fraction > start {
                edge.te_offset_m = 0.0;
            }
        }
        if let Some(device) = straight.wing.wingtip_device.as_mut() {
            device.root_te_flare = None;
        }
        let build = |geometry: alas_config::GeometryConfig| {
            AircraftBuilder::new(Some(geometry))
                .build(Some(&entry.design_vector), false)
                .expect("builds")
                .wings
                .swap_remove(0)
        };
        let (drawn, straight) = (build(entry.geometry.clone()), build(straight));
        let tip = |wing: &crate::aircraft::wing::Wing| wing.xsecs.last().expect("tip").chord;
        assert!(
            (tip(&drawn) - tip(&straight) - flare.offset_m).abs() < 1e-12,
            "the flare is drawn"
        );
        let (a, b) = (
            drawn.planform_datum.expect("datum"),
            straight.planform_datum.expect("datum"),
        );
        assert_eq!(a.tip_chord_m, b.tip_chord_m);
        for (name, x, y) in [
            ("gross area", a.gross_area_m2, b.gross_area_m2),
            ("gross MAC", a.gross_mac_m, b.gross_mac_m),
            ("gross MAC LE", a.gross_mac_le_x_m, b.gross_mac_le_x_m),
        ] {
            assert!(
                (x - y).abs() <= 1e-12 * x.abs().max(1.0),
                "{name}: {x} vs {y}"
            );
        }
        // What the datum leaves out is the drawn wedge: the offsets, linear between
        // the start, the flared edge stations and the flared tip, on both semispans.
        let semispan = drawn.xsecs.last().expect("tip").xyz_le[1];
        let mut knots = vec![(start * semispan, 0.0)];
        knots.extend(
            flared_stations
                .iter()
                .map(|edge| (edge.span_fraction * semispan, edge.te_offset_m)),
        );
        knots.push((semispan, flare.offset_m));
        let wedge: f64 = knots
            .windows(2)
            .map(|pair| (pair[1].0 - pair[0].0) * (pair[0].1 + pair[1].1))
            .sum();
        let drawn_minus_straight = drawn.projected_area() - straight.projected_area();
        assert!(
            (drawn_minus_straight - wedge).abs() < 1e-9,
            "{drawn_minus_straight} vs {wedge}"
        );
    }
}
