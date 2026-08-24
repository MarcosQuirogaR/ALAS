// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/geometry/wing_structure.py.
// Reference: alas @ rust-port-baseline.

//! Generic rib/spar wingbox FEM geometry for the main wing.
//!
//! Generalizes the reference scripts' one-off wingbox (hardcoded to one
//! aircraft's span/chord/sweep/kink and a fixed blended airfoil) to any main
//! wing this program's own `DesignVector`/`WingConfig` describe, and to an
//! arbitrary number of spars at arbitrary chord fractions.
//!
//! * Planform (`x_le`) and dihedral (`z_le`) come from the *same*
//!   root -> break(kink) -> tip piecewise-linear formulas
//!   `AircraftBuilder._build_main_wing` uses upstream (`dx_break`/`dx_tip`
//!   are copied verbatim, not reinvented), so the structural wingbox stays
//!   geometrically consistent with the aerodynamic wing everything else
//!   analyses.
//! * Airfoil shape at each spanwise station is sampled directly from the
//!   *actual built* root/tip [`Airfoil`] objects (their own
//!   `upper_coordinates()`/`lower_coordinates()`) rather than a hardcoded
//!   blend, so any bumps or thickness/camber morphing the optimizer applies
//!   is reflected automatically.
//! * Every spar gets its own 3-point (root/break/tip) kinked reference line
//!   -- the reference reserved that treatment for its one rear spar, whose
//!   position happens to kink at the break because the planform itself
//!   does. Generalizing it to every spar is what lets `spar_chord_fractions`
//!   be an arbitrary-length list instead of a fixed front/rear pair.
//!
//! Coordinate convention matches native aerodynamic model's airplane frame: X = chordwise
//! (aft-positive), Y = spanwise (outboard-positive, root = 0), Z = up
//! (including dihedral). Twist (washout) is **not** applied to the FEM
//! cross-sections -- a documented simplification matching the reference
//! scripts' own fidelity level (a few degrees of twist has a second-order
//! effect on spanwise bending stiffness).
//!
//! Ribs are cut perpendicular to the local leading edge (streamwise at the
//! root -- the root rib is the clamped wall and must be a clean streamwise
//! cut). Root-adjacent "transition" ribs are truncated where their
//! perpendicular cut would otherwise extend past the wing root (`Y < 0`);
//! `alas-struct::mesh` (not this module) is what has to bridge that
//! truncation with skin triangles.
//!
//! Split into one file per responsibility to stay under this repository's
//! per-file line limit: [`types`] holds the data this module produces and
//! its error type, [`support`] the small numeric/geometric primitives its
//! methods share, and `planform`/`spars`/`stations` each contribute one
//! `impl WingStructureGeometry` block -- planform and airfoil sampling, rib
//! length and spar-intersection geometry, and the full station generator,
//! respectively.

mod planform;
mod spars;
mod stations;
mod support;
mod types;

use alas_config::{DesignVector, WingConfig};

use crate::aircraft::airfoil::Airfoil;
use support::airfoil_surfaces;
use types::SparReferenceLine;

pub use types::{RibStation, WingStructureError};

/// Computes generic rib/spar FEM geometry for one main-wing design.
///
/// Construct once per analysis with the design's own [`DesignVector`],
/// [`WingConfig`], and the *actual* root/tip airfoil objects (matching
/// whichever ones the aerodynamic model analyses), then call
/// [`WingStructureGeometry::get_rib_stations`].
#[derive(Debug, Clone, PartialEq)]
pub struct WingStructureGeometry {
    /// Half of `dv.span_m`.
    pub semi_span: f64,
    /// `wing_cfg.break_span_fraction`, carried as its own field since every
    /// planform method compares against it.
    pub break_eta: f64,
    /// `break_eta * semi_span`.
    pub y_break: f64,
    /// Inboard leading-edge sweep, in radians.
    pub sweep_in: f64,
    /// Outboard leading-edge sweep, in radians.
    pub sweep_out: f64,
    /// Leading-edge X offset at the break station.
    pub dx_break: f64,
    /// Leading-edge X offset at the tip -- the same formula
    /// `AircraftBuilder._build_main_wing` uses, duplicated here rather than
    /// shared because that builder has no Rust counterpart yet.
    pub dx_tip: f64,
    /// Root chord, metres.
    pub c_root: f64,
    /// Break chord, metres.
    pub c_break: f64,
    /// Tip chord, metres.
    pub c_tip: f64,
    /// Root leading-edge Z, metres.
    pub z_root: f64,
    /// Break leading-edge Z, metres.
    pub z_break: f64,
    /// Tip leading-edge Z, metres.
    pub z_tip: f64,
    /// Spar chord fractions, sorted ascending (paired with
    /// [`WingStructureGeometry::spar_full_span`] at each index).
    pub spar_fracs: Vec<f64>,
    /// Whether each spar in [`WingStructureGeometry::spar_fracs`] runs the
    /// full span (`true`) or stops at the break station (`false`).
    pub spar_full_span: Vec<bool>,

    root_xu: Vec<f64>,
    root_zu: Vec<f64>,
    root_xl: Vec<f64>,
    root_zl: Vec<f64>,
    tip_xu: Vec<f64>,
    tip_zu: Vec<f64>,
    tip_xl: Vec<f64>,
    tip_zl: Vec<f64>,

    // TE-line slopes dTE_x/dy on each panel, used by `get_rib_lengths`'s
    // root-plane truncation check.
    a_in: f64,
    a_out: f64,
    x_kink_te: f64,

    spar_ref_pts: Vec<SparReferenceLine>,
}

impl WingStructureGeometry {
    /// Build the wingbox geometry for one design.
    ///
    /// `spar_full_span` defaults to "every spar runs the full span" when
    /// `None`, matching the Python default. When both are `Some`, the two
    /// are paired by position and truncated to the shorter of the two --
    /// `Iterator::zip`'s behaviour, which is also Python's `zip`'s, so a
    /// caller-supplied length mismatch is reproduced rather than rejected,
    /// exactly as upstream does.
    ///
    /// # Errors
    ///
    /// [`WingStructureError::NoSpars`] if `spar_chord_fractions` is empty.
    pub fn new(
        dv: &DesignVector,
        wing_cfg: &WingConfig,
        root_section: &Airfoil,
        tip_airfoil: &Airfoil,
        spar_chord_fractions: &[f64],
        spar_full_span: Option<&[bool]>,
    ) -> Result<Self, WingStructureError> {
        if spar_chord_fractions.is_empty() {
            return Err(WingStructureError::NoSpars);
        }

        let semi_span = dv.span_m / 2.0;
        let break_eta = wing_cfg.break_span_fraction;
        let y_break = break_eta * semi_span;

        let sweep_in = dv.sweep_deg.to_radians();
        let sweep_out = (dv.sweep_deg - wing_cfg.outboard_sweep_decrement_deg).to_radians();

        let dx_break = y_break * sweep_in.tan();
        let dx_tip = dx_break + (semi_span - y_break) * sweep_out.tan();

        let c_root = dv.root_chord_m;
        let c_break = dv.break_chord_m;
        let c_tip = dv.tip_chord_m;

        let z_root = wing_cfg.root_z_m;
        let z_break = wing_cfg.break_z_m;
        let z_tip = wing_cfg.tip_z_m;

        let owned_full_span;
        let full_span_in: &[bool] = match spar_full_span {
            Some(flags) => flags,
            None => {
                owned_full_span = vec![true; spar_chord_fractions.len()];
                &owned_full_span
            }
        };

        // Sort fracs and full_span together (not two independent sorts) so a
        // partial-span spar's own flag stays attached to its own fraction
        // regardless of input order -- `sorted(zip(fracs, full_span))` in
        // Python, whose tuple comparison breaks a tied fraction by comparing
        // the boolean (False < True) before falling back to input order,
        // which `Vec::sort_by`'s stability also preserves.
        let mut paired: Vec<(f64, bool)> = spar_chord_fractions
            .iter()
            .copied()
            .zip(full_span_in.iter().copied())
            .collect();
        paired.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let spar_fracs: Vec<f64> = paired.iter().map(|p| p.0).collect();
        let spar_full_span: Vec<bool> = paired.iter().map(|p| p.1).collect();

        let (root_xu, root_zu, root_xl, root_zl) = airfoil_surfaces(root_section);
        let (tip_xu, tip_zu, tip_xl, tip_zl) = airfoil_surfaces(tip_airfoil);

        let a_in = sweep_in.tan() + (c_break - c_root) / y_break.max(1e-9);
        let a_out = sweep_out.tan() + (c_tip - c_break) / (semi_span - y_break).max(1e-9);
        let x_kink_te = dx_break + c_break;

        let mut geometry = Self {
            semi_span,
            break_eta,
            y_break,
            sweep_in,
            sweep_out,
            dx_break,
            dx_tip,
            c_root,
            c_break,
            c_tip,
            z_root,
            z_break,
            z_tip,
            spar_fracs,
            spar_full_span,
            root_xu,
            root_zu,
            root_xl,
            root_zl,
            tip_xu,
            tip_zu,
            tip_xl,
            tip_zl,
            a_in,
            a_out,
            x_kink_te,
            spar_ref_pts: Vec::new(),
        };
        geometry.spar_ref_pts = geometry.compute_spar_reference_points();
        Ok(geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("4-digit NACA name parses")
    }

    fn default_geometry() -> WingStructureGeometry {
        WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &naca("naca4412"),
            &naca("naca2410"),
            &[0.25, 0.70],
            None,
        )
        .expect("two full-span spars is a valid configuration")
    }

    #[test]
    fn get_rib_lengths_short_circuits_on_a_streamwise_cut() {
        // aft_y == 0 is the root rib's own cut direction; both returned
        // lengths must equal the local chord with no line-intersection
        // logic evaluated at all.
        let geometry = default_geometry();
        let (nominal, actual) = geometry.get_rib_lengths(0.0, 0.0, 1.0, 0.0);
        assert_eq!(nominal, geometry.local_chord(0.0));
        assert_eq!(actual, geometry.local_chord(0.0));

        // The same short circuit applies away from the root too, as long as
        // the cut is exactly streamwise.
        let (nominal_mid, actual_mid) = geometry.get_rib_lengths(10.0, 3.0, 1.0, 0.0);
        assert_eq!(nominal_mid, geometry.local_chord(10.0 / geometry.semi_span));
        assert_eq!(actual_mid, nominal_mid);
    }

    #[test]
    fn a_partial_span_spar_has_no_intersection_beyond_its_own_break_endpoint() {
        // Sorted by (fraction, full_span): (0.25, true), (0.50, false),
        // (0.70, true) -- the partial-span spar lands at index 1.
        let geometry = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &naca("naca4412"),
            &naca("naca2410"),
            &[0.25, 0.70, 0.50],
            Some(&[true, true, false]),
        )
        .expect("a mixed full/partial-span spar list is valid");
        assert_eq!(geometry.spar_full_span, vec![true, false, true]);

        let eta = geometry.break_eta + 0.05;
        let y = eta * geometry.semi_span;
        let x_le_val = geometry.x_le(eta);
        let (aft_x, aft_y) = geometry.rib_vector(eta);
        let (l_nominal, _) = geometry.get_rib_lengths(y, x_le_val, aft_x, aft_y);

        let intersections =
            geometry.compute_spar_intersections(y, x_le_val, aft_x, aft_y, l_nominal);
        assert_eq!(
            intersections[1], None,
            "the partial-span spar is entry index 1 after sorting by fraction"
        );
        assert!(intersections[0].is_some());
        assert!(intersections[2].is_some());
    }

    #[test]
    fn spar_fractions_and_full_span_flags_are_paired_through_the_sort() {
        // A center spar appended after two full-span spars must land in the
        // middle of the sorted list still carrying its own `false` flag,
        // not the flag of whichever spar started in the middle position.
        let geometry = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &naca("naca4412"),
            &naca("naca2410"),
            &[0.70, 0.25, 0.50],
            Some(&[true, true, false]),
        )
        .expect("a mixed full/partial-span spar list is valid");

        assert_eq!(geometry.spar_fracs, vec![0.25, 0.50, 0.70]);
        assert_eq!(geometry.spar_full_span, vec![true, false, true]);
    }

    #[test]
    fn an_empty_spar_list_is_rejected() {
        let result = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &naca("naca4412"),
            &naca("naca2410"),
            &[],
            None,
        );
        assert_eq!(result, Err(WingStructureError::NoSpars));
    }

    #[test]
    fn local_chord_x_le_and_z_le_are_continuous_at_the_break() {
        let geometry = default_geometry();
        let just_inboard = geometry.break_eta - 1e-9;
        let just_outboard = geometry.break_eta + 1e-9;
        assert!(
            (geometry.local_chord(just_inboard) - geometry.local_chord(just_outboard)).abs() < 1e-6
        );
        assert!((geometry.x_le(just_inboard) - geometry.x_le(just_outboard)).abs() < 1e-6);
        assert!((geometry.z_le(just_inboard) - geometry.z_le(just_outboard)).abs() < 1e-6);
    }

    #[test]
    fn rib_vector_is_streamwise_only_at_the_root() {
        let geometry = default_geometry();
        assert_eq!(geometry.rib_vector(0.0), (1.0, 0.0));
        let (dx, dy) = geometry.rib_vector(0.1);
        assert!(dy != 0.0, "an off-root cut has a spanwise component");
        assert!((dx * dx + dy * dy - 1.0).abs() < 1e-12, "not unit length");
    }

    #[test]
    fn get_rib_stations_pinches_the_trailing_edge_shut() {
        let geometry = default_geometry();
        let stations = geometry.get_rib_stations(6, 20);
        for station in &stations {
            let last_extrados = station.extrados.last().expect("non-empty rib");
            let last_intrados = station.intrados.last().expect("non-empty rib");
            assert!(
                (last_extrados[2] - last_intrados[2]).abs() < 1e-12,
                "station {} does not pinch its trailing edge",
                station.index
            );
        }
    }
}
