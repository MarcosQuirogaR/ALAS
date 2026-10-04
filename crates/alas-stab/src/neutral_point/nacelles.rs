// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Torenbeek's nacelle/pylon neutral-point term.
//!
//! `fuselage_cm_alpha` reads `airplane.fuselages[0]` only and the VLM meshes
//! wings only, so every podded nacelle the builder appends
//! (`airplane.fuselages[1..]`) is aerodynamically invisible to the
//! `reference_compatibility` neutral point. Torenbeek's
//! correlation: `D_n = sum_n k_n b_n^2 l_n / (S c CLa_wf)`, `k_n = -4.0` for a
//! wing-mounted pod, `-2.5` for an aft-fuselage-mounted pod, `l_n` the
//! (signed) distance from the nacelle inlet to the local wing quarter-chord
//! (wing pods) or to the wing's own quarter-MAC point (aft-fuselage pods,
//! which have no "local" wing station under them): positive when the inlet
//! sits ahead of that reference. An aft-fuselage pod's inlet is normally
//! behind the wing, so its `l_n` is normally negative, and the term comes
//! out aft/stabilizing rather than forward - the reduced `|k_n|` (`-2.5`
//! rather than `-4.0`) says only that the effect is smaller in magnitude
//! than a wing pod's, not which direction it points.
//!
//! # Wing pod vs aft-fuselage pod: a position heuristic
//!
//! `Fuselage` carries no explicit mount-type field (see its module doc: pods
//! are distinguished from the primary fuselage by name substring only). This
//! module classifies by position instead: a nacelle whose inlet sits within
//! [`CENTERLINE_HALF_WIDTH_M`] of the aircraft centreline *and* aft of the
//! main wing's root trailing edge is treated as an aft-fuselage pod (`k_n =
//! -2.5`); every other nacelle in `airplane.fuselages[1..]` is treated as a
//! wing pod (`k_n = -4.0`). This matches every preset nacelle placement the
//! builder currently generates (`AircraftBuilder::build_engines`: wing-
//! spanwise or fuselage-centreline mounts only), but is a heuristic, not a
//! read of a mount-type field; document any future belly- or tail-mounted
//! pod placement against it before trusting the classification.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

/// Nacelle inlets within this lateral distance of the aircraft centreline
/// are classified as fuselage-mounted rather than wing-mounted.
const CENTERLINE_HALF_WIDTH_M: f64 = 0.75;

/// One nacelle's own Torenbeek term, kept for the diagnostics struct one
/// layer up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NacelleTerm {
    /// Name of the `Fuselage` entry this term was built from.
    pub is_aft_fuselage_pod: bool,
    /// `k_n` used for this nacelle (`-4.0` wing pod, `-2.5` aft-fuselage pod).
    pub k_n: f64,
    /// This nacelle's own contribution to the neutral-point shift, m
    /// (negative: forward).
    pub shift_m: f64,
}

/// The local quarter-chord station of `wing` at lateral station `y_abs`
/// (absolute value; the wing is assumed symmetric about `y=0`), by linear
/// interpolation between the bounding cross-sections. Falls back to the
/// wing's own quarter-MAC point when `y_abs` falls outside every section
/// pair (an aft-fuselage-centreline mount, or a malformed section list).
fn local_quarter_chord(wing: &Wing, y_abs: f64) -> f64 {
    for pair in wing.xsecs.windows(2) {
        let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        if y_abs >= lo && y_abs <= hi && (hi - lo).abs() > 1e-9 {
            let t = (y_abs - y0) / (y1 - y0);
            let x_le = pair[0].xyz_le[0] + t * (pair[1].xyz_le[0] - pair[0].xyz_le[0]);
            let chord = pair[0].chord + t * (pair[1].chord - pair[0].chord);
            return x_le + 0.25 * chord;
        }
    }
    wing.aerodynamic_center(0.25)[0]
}

/// Sum of Torenbeek `D_n` over `airplane.fuselages[1..]`, in metres of
/// neutral-point shift (negative: forward), plus every nacelle's own term.
/// `cl_alpha_wf` is the Mach-consistent wing(-body) lift-curve slope [1/rad]
/// the correlation normalises by; `main_wing` supplies the local
/// quarter-chord line and `x_wing_ac` its own quarter-MAC fallback for
/// centreline mounts.
pub fn nacelle_terms(
    airplane: &Airplane,
    cl_alpha_wf: f64,
    main_wing: &Wing,
    x_wing_ac: f64,
    x_wing_root_te: f64,
) -> (f64, Vec<NacelleTerm>) {
    if cl_alpha_wf.abs() < 1e-9 || airplane.fuselages.len() < 2 {
        return (0.0, Vec::new());
    }
    let s_ref = airplane.s_ref.max(1.0);
    let c_ref = airplane.c_ref.max(0.1);
    let mut total_shift_m = 0.0_f64;
    let mut terms = Vec::with_capacity(airplane.fuselages.len() - 1);
    for nacelle in &airplane.fuselages[1..] {
        let Some(first) = nacelle.xsecs.first() else {
            continue;
        };
        let x_inlet = first.xyz_c[0];
        let y_abs = first.xyz_c[1].abs();
        let b_n = nacelle
            .xsecs
            .iter()
            .map(|x| x.width)
            .fold(0.0_f64, f64::max);
        let is_aft_fuselage_pod = y_abs < CENTERLINE_HALF_WIDTH_M && x_inlet > x_wing_root_te;
        let (k_n, l_n) = if is_aft_fuselage_pod {
            (-2.5, x_wing_ac - x_inlet)
        } else {
            (-4.0, local_quarter_chord(main_wing, y_abs) - x_inlet)
        };
        let shift_fraction = k_n * b_n * b_n * l_n / (s_ref * c_ref * cl_alpha_wf);
        let shift_m = shift_fraction * c_ref;
        total_shift_m += shift_m;
        terms.push(NacelleTerm {
            is_aft_fuselage_pod,
            k_n,
            shift_m,
        });
    }
    (total_shift_m, terms)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn main_wing() -> Wing {
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([10.0, 0.0, 0.0], 5.0, 0.0, naca("naca0012")),
                WingXSec::new([15.0, 12.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            true,
        )
    }

    fn nacelle_at(x: f64, y: f64) -> Fuselage {
        let station = |dx: f64| {
            FuselageXSec::new([x + dx, y, 0.0], Some(0.9), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new("Nacelle R", vec![station(0.0), station(3.0)])
    }

    fn probe_airplane(nacelles: Vec<Fuselage>) -> Airplane {
        let wing = main_wing();
        let mut fuselages = vec![Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
                FuselageXSec::new([30.0, 0.0, 0.0], Some(0.8), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        )];
        fuselages.extend(nacelles);
        let s_ref = wing.reference_area();
        let b_ref = wing.reference_span();
        let c_ref = wing.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [12.0, 0.0, 0.0],
            wings: vec![wing],
            fuselages,
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn no_nacelle_term_without_any_podded_fuselage() {
        let plane = probe_airplane(vec![]);
        let (shift, terms) = nacelle_terms(&plane, 5.0, &plane.wings[0], 12.0, 15.0);
        assert_eq!(shift, 0.0);
        assert!(terms.is_empty());
    }

    #[test]
    fn a_wing_mounted_pod_uses_k_n_minus_4_and_shifts_forward() {
        let plane = probe_airplane(vec![nacelle_at(6.0, 6.0)]);
        let (shift, terms) = nacelle_terms(&plane, 5.0, &plane.wings[0], 12.0, 15.0);
        assert_eq!(terms.len(), 1);
        assert!(!terms[0].is_aft_fuselage_pod);
        assert_eq!(terms[0].k_n, -4.0);
        assert!(shift < 0.0, "shift={shift}");
    }

    #[test]
    fn an_aft_fuselage_pod_uses_k_n_minus_2_5() {
        // Mounted aft of the wing root trailing edge (x_wing_root_te=15):
        // l_n = x_wing_ac - x_inlet is negative, so (per the module doc) the
        // term comes out aft/stabilizing rather than forward, unlike a wing
        // pod. The classification and k_n are what this test pins; the sign
        // follows from where the pod actually sits.
        let plane = probe_airplane(vec![nacelle_at(28.0, 0.1)]);
        let (shift, terms) = nacelle_terms(&plane, 5.0, &plane.wings[0], 12.0, 15.0);
        assert_eq!(terms.len(), 1);
        assert!(terms[0].is_aft_fuselage_pod);
        assert_eq!(terms[0].k_n, -2.5);
        assert!(shift.is_finite() && shift != 0.0, "shift={shift}");
    }
}
