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
//! (signed) distance from the nacelle inlet to the quarter-chord point of the
//! wing mean aerodynamic chord (Torenbeek 1982, Eq. E-41 and Fig. E-13, printed
//! pp. 480-481), for wing pods and aft-fuselage pods alike: positive when the
//! inlet sits ahead of that point. An aft-fuselage pod's inlet is normally
//! behind the wing, so its `l_n` is normally negative, and the term comes
//! out aft/stabilizing rather than forward - the reduced `|k_n|` (`-2.5`
//! rather than `-4.0`) says only that the effect is smaller in magnitude
//! than a wing pod's, not which direction it points.
//!
//! # Wing pod vs aft-fuselage pod
//!
//! The builder marks every nacelle it hangs from the fuselage
//! (`Fuselage::fuselage_mounted`: the rear-fuselage pair of a business jet and
//! a trijet's centre engine), and such a pod is an aft-fuselage pod
//! (`k_n = -2.5`) wherever its inlet stands: the LJ60 inlets are ahead of the
//! local wing trailing edge at their 1.32 m station, so the inlet position
//! alone would call them wing pods. A body without the mark (one read back
//! from an exchange file) falls back on the built geometry: a nacelle whose
//! inlet lies aft of the main wing's trailing edge at its own span station is
//! on no wing. The trailing edge is the root one where the station lies
//! outside the wing's sections.

use alas_geom::aircraft::airplane::Airplane;

use crate::trim::main_wing;

/// One nacelle's own Torenbeek term, kept for the diagnostics struct one
/// layer up.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct NacelleTerm {
    /// Name of the `Fuselage` entry this term was built from.
    pub is_aft_fuselage_pod: bool,
    /// `k_n` used for this nacelle (`-4.0` wing pod, `-2.5` aft-fuselage pod).
    pub k_n: f64,
    /// This nacelle's own contribution to the neutral-point shift, m
    /// (negative: forward).
    pub shift_m: f64,
}

/// Sum of Torenbeek `D_n` over `airplane.fuselages[1..]`, in metres of
/// neutral-point shift (negative: forward), plus every nacelle's own term.
/// `cl_alpha_wf` is the Mach-consistent wing(-body) lift-curve slope [1/rad]
/// the correlation normalises by; `x_wing_ac` is the quarter-chord point of the
/// wing mean aerodynamic chord, the reference of `l_n` (Torenbeek 1982,
/// Eq. E-41, printed p. 481: positive ahead of, negative behind 1/4 MAC).
pub fn nacelle_terms(
    airplane: &Airplane,
    cl_alpha_wf: f64,
    x_wing_ac: f64,
    x_wing_root_te: f64,
) -> (f64, Vec<NacelleTerm>) {
    if cl_alpha_wf.abs() < 1e-9 || airplane.fuselages.len() < 2 {
        return (0.0, Vec::new());
    }
    let s_ref = airplane
        .s_ref
        .max(crate::reference_dimensions::REFERENCE_AREA_FLOOR_M2);
    let c_ref = airplane
        .c_ref
        .max(crate::reference_dimensions::REFERENCE_CHORD_FLOOR_M);
    let mut total_shift_m = 0.0_f64;
    let mut terms = Vec::with_capacity(airplane.fuselages.len() - 1);
    let wing = main_wing(airplane);
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
        let x_local_te = wing
            .and_then(|wing| super::fuselage::wing_section_at(wing, y_abs))
            .map_or(x_wing_root_te, |(x_le, chord)| x_le + chord);
        let is_aft_fuselage_pod = nacelle.fuselage_mounted || x_inlet > x_local_te;
        let (k_n, l_n) = if is_aft_fuselage_pod {
            (-2.5, x_wing_ac - x_inlet)
        } else {
            (-4.0, x_wing_ac - x_inlet)
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
    use alas_geom::aircraft::wing::{Wing, WingXSec};

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
        let (shift, terms) = nacelle_terms(&plane, 5.0, 12.0, 15.0);
        assert_eq!(shift, 0.0);
        assert!(terms.is_empty());
    }

    #[test]
    fn a_wing_mounted_pod_uses_k_n_minus_4_and_shifts_forward() {
        let plane = probe_airplane(vec![nacelle_at(6.0, 6.0)]);
        let (shift, terms) = nacelle_terms(&plane, 5.0, 12.0, 15.0);
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
        let (shift, terms) = nacelle_terms(&plane, 5.0, 12.0, 15.0);
        assert_eq!(terms.len(), 1);
        assert!(terms[0].is_aft_fuselage_pod);
        assert_eq!(terms[0].k_n, -2.5);
        assert!(shift.is_finite() && shift != 0.0, "shift={shift}");
    }

    #[test]
    fn a_rear_fuselage_pod_beside_the_body_is_an_aft_fuselage_pod() {
        // The LJ60 pair: off the centreline beside the body, aft of the wing.
        let beside = probe_airplane(vec![nacelle_at(28.0, 1.32)]);
        let (_, terms) = nacelle_terms(&beside, 5.0, 12.0, 15.0);
        assert!(terms[0].is_aft_fuselage_pod);
        assert_eq!(terms[0].k_n, -2.5);
        // An outboard pod whose inlet is aft of the root trailing edge but ahead
        // of the swept wing at its own station is still a wing pod.
        let outboard = probe_airplane(vec![nacelle_at(15.5, 10.0)]);
        let (_, terms) = nacelle_terms(&outboard, 5.0, 12.0, 15.0);
        assert!(!terms[0].is_aft_fuselage_pod);
    }

    #[test]
    fn the_built_lj60_pods_are_aft_fuselage_pods() {
        // The built LJ60 inlets sit ahead of the local wing trailing edge, so
        // only the builder's mount mark makes them aft-fuselage pods.
        let preset = alas_config::presets::get("LJ60").expect("LJ60 preset");
        let plane = alas_geom::builder::AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("LJ60 builds");
        let wing = crate::trim::main_wing(&plane).expect("main wing");
        let pods = &plane.fuselages[1..];
        assert_eq!(pods.len(), 2);
        for pod in pods {
            assert!(pod.fuselage_mounted, "{}", pod.name);
            let first = pod.xsecs[0].xyz_c;
            let (x_le, chord) = super::super::fuselage::wing_section_at(wing, first[1].abs())
                .expect("pod inside the span");
            assert!(
                first[0] < x_le + chord,
                "inlet {} te {}",
                first[0],
                x_le + chord
            );
        }
        let (_, terms) = nacelle_terms(&plane, 5.0, 10.0, 12.0);
        assert!(terms.iter().all(|t| t.is_aft_fuselage_pod && t.k_n == -2.5));
    }
}
