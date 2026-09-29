// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One canonical MAC-frame API, replacing the
//! `aerodynamic_center(0.25) - 0.25 * c_ref` reconstruction repeated at
//! roughly a dozen call sites (`alas-pipeline`, `alas-mass`, `alas-report`,
//! `alas-opt`, `alas-payload`).
//!
//! `x` throughout this crate's geometry is metres, positive aft from the
//! aircraft's origin (the nose tip, for a product-built airplane).
//! [`Wing::mac_station`] is a pure wing-geometry quantity;
//! [`Airplane::mac_frame`] additionally decides which wing is "the" MAC and
//! which chord `%MAC` is stated against, see its own doc comment.
//!
//! [`Wing::aerodynamic_center`] keeps its existing meaning: the *geometric*
//! quarter-MAC point (or any other `chord_fraction`), not the aerodynamic
//! centre/neutral point a separate stability investigation tracks.
//! [`Wing::mac_station`] is a narrower, related quantity: the same
//! chord-weighted centroid at `chord_fraction = 0`, but integrated over the
//! projected (`|dy|`) planform measure rather than the unfolded YZ
//! quarter-chord path, and reporting `y`/`z` instead of zeroing
//! them for a symmetric wing.

use super::airplane::Airplane;
use super::segment_integrals::SegmentIntegrals;
use super::vector3::{add3, scale3, sub3};
use super::wing::Wing;

/// The chord-weighted mean-aerodynamic-chord station of one wing:
/// `(2/S) int c x_le dy`, `(2/S) int c y dy`, `(2/S) int c z dy`, over the
/// wing's projected (`|dy|`) planform measure -- the same measure
/// [`Wing::reference_area`]/[`Wing::reference_span`] use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacStation {
    /// The wing's own mean aerodynamic chord length, metres.
    pub chord_m: f64,
    /// Leading-edge x of the MAC station, metres, in the wing's geometry
    /// frame (positive aft).
    pub x_le_m: f64,
    /// Spanwise (y) station of the MAC, metres. Not zeroed for a symmetric
    /// wing: this is the physical semispan location.
    pub y_m: f64,
    /// z of the MAC station's leading edge, metres.
    pub z_m: f64,
}

impl Wing {
    /// This wing's chord-weighted MAC station: the same
    /// per-segment chord-weighted centroid [`Wing::aerodynamic_center`]
    /// uses, but integrated over the projected (`|dy|`) span rather than
    /// the unfolded YZ quarter-chord path, and reporting `y`/`z` instead of
    /// zeroing them.
    pub fn mac_station(&self) -> MacStation {
        let mut area_sum = 0.0;
        let mut chord_weighted_mac = 0.0;
        let mut weighted_station = [0.0; 3];
        for pair in self.xsecs.windows(2) {
            let span = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
            let integrals = SegmentIntegrals::of_segment(span, pair[0].chord, pair[1].chord);
            let station = add3(
                pair[0].xyz_le,
                scale3(sub3(pair[1].xyz_le, pair[0].xyz_le), integrals.le_fraction),
            );
            area_sum += integrals.area;
            chord_weighted_mac += integrals.mac_length * integrals.area;
            weighted_station = add3(weighted_station, scale3(station, integrals.area));
        }
        if area_sum <= 0.0 {
            let root = self.xsecs.first().map_or([0.0; 3], |xsec| xsec.xyz_le);
            let chord = self.xsecs.first().map_or(0.0, |xsec| xsec.chord);
            return MacStation {
                chord_m: chord,
                x_le_m: root[0],
                y_m: root[1],
                z_m: root[2],
            };
        }
        let station = scale3(weighted_station, 1.0 / area_sum);
        MacStation {
            chord_m: chord_weighted_mac / area_sum,
            x_le_m: station[0],
            y_m: station[1],
            z_m: station[2],
        }
    }
}

/// The chord and geometric leading-edge-of-MAC station a `%MAC` figure is
/// measured against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacFrame {
    /// The chord `%MAC` is a fraction of: [`Airplane::c_ref`], not
    /// necessarily the resolved main wing's own MAC (a CPACS document's
    /// declared reference length can differ; see
    /// `alas-pipeline`'s `cpacs::wing_roles` import-time warning).
    pub chord_m: f64,
    /// The main wing's own geometric leading-edge-of-MAC x station,
    /// metres, always from [`Wing::mac_station`] -- never reconstructed
    /// from `chord_m`, which would ignore the wing's own planform.
    pub x_lemac_m: f64,
}

impl MacFrame {
    /// `x_m`'s station as a percentage of `chord_m` aft of `x_lemac_m`.
    pub fn pct_mac(&self, x_m: f64) -> f64 {
        100.0 * (x_m - self.x_lemac_m) / self.chord_m
    }

    /// The inverse of [`Self::pct_mac`]: the absolute x station `pct`
    /// percent of `chord_m` aft of `x_lemac_m`.
    pub fn x_at_pct(&self, pct: f64) -> f64 {
        self.x_lemac_m + pct / 100.0 * self.chord_m
    }
}

impl Airplane {
    /// The airplane's main lifting surface: the wing with the greatest
    /// projected planform area, the rule CPACS import already uses to
    /// resolve `s_ref`/`c_ref`.
    pub fn main_wing(&self) -> Option<&Wing> {
        self.wings
            .iter()
            .max_by(|left, right| left.projected_area().total_cmp(&right.projected_area()))
    }

    /// The `%MAC` frame for this airplane: [`Self::c_ref`] as the
    /// stated chord, and [`Self::main_wing`]'s own
    /// [`Wing::mac_station`]`().x_le_m` as `x_LEMAC`, resolved once instead
    /// of at each of the call sites this replaces (see the module doc).
    ///
    /// `None` only when this airplane has no wing at all, which no
    /// constructible [`Airplane`] in this crate has.
    pub fn mac_frame(&self) -> Option<MacFrame> {
        let x_lemac_m = self.main_wing()?.mac_station().x_le_m;
        Some(MacFrame {
            chord_m: self.c_ref,
            x_lemac_m,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft::airfoil::Airfoil;
    use crate::aircraft::fuselage::Fuselage;
    use crate::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn straight_tapered_wing() -> Wing {
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca("naca0012")),
                WingXSec::new([2.0, 20.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            true,
        )
    }

    #[test]
    fn mac_station_of_a_planar_taper_matches_the_closed_form_projected_integral() {
        let wing = straight_tapered_wing();
        let station = wing.mac_station();
        // Same closed form as `theoretical_reference_mac`'s
        // trapezoid case: (2/3) * 4 * (1+.25+.0625)/1.25 = 2.8.
        assert!((station.chord_m - 2.8).abs() < 1e-9, "{}", station.chord_m);
        // Chord-weighted LE fraction (1+2*0.25)/(3+3*0.25) = 0.4, applied to
        // the LE run (2.0, 20.0, 0.0).
        assert!((station.x_le_m - 0.8).abs() < 1e-9);
        assert!((station.y_m - 8.0).abs() < 1e-9);
        assert!((station.z_m - 0.0).abs() < 1e-9);
    }

    /// A two-segment (root/kink/tip) wing so unfolded-YZ vs. projected-XY
    /// span weighting can actually disagree: a single-segment wing's
    /// per-segment MAC/LE-fraction depend only on its chords, so dihedral
    /// would not move any weighted quantity there.
    fn kinked_wing(kink_z: f64, tip_z: f64) -> Wing {
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 10.0, kink_z], 2.5, 0.0, naca("naca0012")),
                WingXSec::new([2.0, 20.0, tip_z], 1.0, 0.0, naca("naca0012")),
            ],
            true,
        )
    }

    #[test]
    fn mac_station_is_unaffected_by_dihedral_unlike_aerodynamic_center() {
        // Same planform, but the dihedral case raises the kink and tip in
        // Z: dihedral unfolds `aerodynamic_center`'s YZ measure but
        // must not move `mac_station`'s projected-XY measure.
        let flat = kinked_wing(0.0, 0.0);
        let dihedral = kinked_wing(1.0, 3.0);
        let flat_station = flat.mac_station();
        let dihedral_station = dihedral.mac_station();
        assert!((flat_station.chord_m - dihedral_station.chord_m).abs() < 1e-12);
        assert!((flat_station.x_le_m - dihedral_station.x_le_m).abs() < 1e-12);
        assert!((flat_station.y_m - dihedral_station.y_m).abs() < 1e-12);
        assert!(dihedral_station.z_m > 0.0, "{}", dihedral_station.z_m);
        assert!(
            (flat.aerodynamic_center(0.0)[0] - dihedral.aerodynamic_center(0.0)[0]).abs() > 1e-6,
            "dihedral should move the unfolded aerodynamic_center measure"
        );
    }

    fn airplane_with_main_wing_second() -> Airplane {
        let small_tail = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([30.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([31.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        let main = straight_tapered_wing();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![small_tail, main],
            fuselages: Vec::<Fuselage>::new(),
            s_ref: 0.0,
            c_ref: 2.8,
            b_ref: 0.0,
        }
    }

    #[test]
    fn main_wing_resolves_by_projected_area_regardless_of_vector_order() {
        let airplane = airplane_with_main_wing_second();
        let resolved = airplane.main_wing().expect("at least one wing");
        assert_eq!(resolved.name, "Main Wing");
    }

    #[test]
    fn mac_frame_uses_c_ref_for_chord_and_the_main_wing_geometry_for_x_lemac() {
        let airplane = airplane_with_main_wing_second();
        let frame = airplane.mac_frame().expect("at least one wing");
        assert!((frame.chord_m - airplane.c_ref).abs() < 1e-12);
        let expected_x_lemac = airplane.wings[1].mac_station().x_le_m;
        assert!((frame.x_lemac_m - expected_x_lemac).abs() < 1e-12);
    }

    #[test]
    fn pct_mac_and_x_at_pct_are_inverses() {
        let frame = MacFrame {
            chord_m: 4.0,
            x_lemac_m: 10.0,
        };
        let pct = frame.pct_mac(11.0);
        assert!((pct - 25.0).abs() < 1e-12);
        assert!((frame.x_at_pct(pct) - 11.0).abs() < 1e-12);
    }
}
