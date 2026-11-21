// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flow angle of the afterbody behind the wing.
//!
//! The upsweep data of Torenbeek (1982) Fig. 3-27b were measured on an
//! isolated fuselage. Behind the wing of a complete aircraft the body sees
//! the wing's downwash: USAF DATCOM (1978) Sec. 4.2.2.1, Method 1 (after
//! Multhopp), printed pp. 4.2.2.1-1 and -2, Sketch (a), gives each body
//! increment aft of the wing the flow-angle gradient `(x_1 / l_h)(1 - d eps
//! / d alpha)`, `x_1` measured from the trailing edge of the wing root chord
//! to the centroid of the increment (at most `l_h`), `l_h` from the same
//! trailing edge to the tail's aerodynamic centre, and `eps` the downwash at
//! the body trailing edge. The afterbody's flow angle is that factor applied
//! to the body attitude less the downwash, averaged over the afterbody
//! stations:
//!
//! `alpha_a = m (alpha - eps0 - k CL)`, `m` the station mean of
//! `min(x_1 / l_h, 1)` over the afterbody.
//!
//! The downwash is read off the main wing's lattice at the fuselage trailing
//! edge (not at the stabilizer, which a T-tail lifts out of the body's
//! flow), as the flow-angle change between two solves, and expressed per unit
//! wing lift: `eps = eps0 + k CL_w`. The downwash of a lifting wing is
//! proportional to its lift (the lifting-line relation `eps = 2 CL_w / (pi
//! A)` far behind it), so the aircraft lift coefficient `CL` of the state
//! carries both the compressibility of the lift curve and the lift a
//! deployed high-lift system adds at constant angle of attack.

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;

use super::parasite::body_length_and_diameter;
use super::performance::HSTAB_NAME;
use super::AeroAnalysis;
use crate::vlm::VlmSystem;

/// Angles of attack, deg, of the two level solves the downwash differences.
const PROBE_ALPHA_DEG: [f64; 2] = [0.0, 4.0];

/// Name of the main wing among the airplane's surfaces.
const MAIN_WING_NAME: &str = "Main Wing";

/// Chord fraction of the tail's aerodynamic centre.
const TAIL_AC_CHORD_FRACTION: f64 = 0.25;

/// Tail arm, in reference chords behind the wing root trailing edge, of an
/// airplane without a horizontal stabilizer (as the neutral-point analysis
/// takes it).
const TAILLESS_ARM_CHORDS: f64 = 3.0;

/// Stations of the afterbody mean.
const AFTERBODY_STATIONS: usize = 200;

/// The afterbody flow-angle model of [`AeroAnalysis::afterbody_flow`].
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct AfterbodyFlow {
    /// Station mean `m` of `min(x_1 / l_h, 1)` over the afterbody.
    pub ramp_mean: f64,
    /// Downwash at the body trailing edge at zero wing lift, deg.
    pub eps0_deg: f64,
    /// Downwash at the body trailing edge per unit wing lift coefficient,
    /// deg.
    pub eps_per_cl_deg: f64,
}

impl AfterbodyFlow {
    /// The isolated-fuselage angle: no wing ahead of the body.
    pub const ISOLATED: Self = Self {
        ramp_mean: 1.0,
        eps0_deg: 0.0,
        eps_per_cl_deg: 0.0,
    };

    /// Afterbody flow angle, deg, at body attitude `body_alpha_deg` and
    /// aircraft lift coefficient `cl`.
    pub fn alpha_deg(&self, body_alpha_deg: f64, cl: f64) -> f64 {
        self.ramp_mean * (body_alpha_deg - self.eps0_deg - self.eps_per_cl_deg * cl)
    }
}

impl AeroAnalysis<'_> {
    /// The afterbody flow-angle model of the module doc, from the lattice
    /// at the configured resolution; [`AfterbodyFlow::ISOLATED`] without a
    /// main wing or a body, or when a solve fails.
    pub fn afterbody_flow(&self) -> AfterbodyFlow {
        *self.afterbody_flow_cache.get_or_init(|| {
            self.solve_afterbody_flow()
                .unwrap_or(AfterbodyFlow::ISOLATED)
        })
    }

    fn solve_afterbody_flow(&self) -> Option<AfterbodyFlow> {
        let fuselage = self.plane.fuselages.first()?;
        let wing = self.plane.wings.iter().find(|w| w.name == MAIN_WING_NAME)?;
        self.aft_body(fuselage)?;
        let (root_le, root_chord) = wing.design_root();
        let x_te = root_le[0] + root_chord;
        let x_tail = self
            .plane
            .wings
            .iter()
            .find(|w| w.name == HSTAB_NAME)
            .map(|tail| tail.aerodynamic_center(TAIL_AC_CHORD_FRACTION)[0]);
        let l_h = x_tail
            .map_or(TAILLESS_ARM_CHORDS * self.plane.c_ref, |x| x - x_te)
            .max(1e-3);
        let end = fuselage
            .xsecs
            .iter()
            .max_by(|a, b| a.xyz_c[0].total_cmp(&b.xyz_c[0]))?;
        let (length, _) = body_length_and_diameter(fuselage);
        let nose_x = fuselage.xsecs.first().map_or(0.0, |xsec| xsec.xyz_c[0]);
        let x_start = nose_x + self.geometry.fuselage.aft_body_start_m(length);
        let x_end = end.xyz_c[0];
        let ramp_mean = (0..AFTERBODY_STATIONS)
            .map(|i| {
                let x = x_start + (i as f64 + 0.5) / AFTERBODY_STATIONS as f64 * (x_end - x_start);
                ((x - x_te) / l_h).clamp(0.0, 1.0)
            })
            .sum::<f64>()
            / AFTERBODY_STATIONS as f64;
        let mut wing_alone = self.plane.clone();
        wing_alone.wings = vec![wing.clone()];
        let probes = self.downwash_probes(&wing_alone, end.xyz_c)?;
        let [(cl_lo, eps_lo), (cl_hi, eps_hi)] = probes;
        let eps_per_cl_deg = (eps_hi - eps_lo) / (cl_hi - cl_lo);
        let flow = AfterbodyFlow {
            ramp_mean,
            eps0_deg: eps_lo - eps_per_cl_deg * cl_lo,
            eps_per_cl_deg,
        };
        (ramp_mean.is_finite() && flow.eps0_deg.is_finite() && eps_per_cl_deg.is_finite())
            .then_some(flow)
    }

    /// `(CL, downwash deg at point)` of the two probe solves of `plane`.
    fn downwash_probes(&self, plane: &Airplane, point: [f64; 3]) -> Option<[(f64, f64); 2]> {
        let (spanwise, chordwise) = self.mesh_resolution();
        let system = {
            let mut cache = self.vlm_cache.lock().ok()?;
            VlmSystem::assemble_cached(plane, spanwise, chordwise, &mut cache).ok()?
        };
        let probe = |alpha_deg: f64| -> Option<(f64, f64)> {
            let op = Self::level_op_point(Atmosphere::new(0.0), 100.0, alpha_deg);
            let solved = system.solve(&op).ok()?;
            let velocity = crate::vlm::velocity_at_points(
                &[point],
                &solved.panels,
                &solved.vortex_strengths,
                &op,
            );
            let freestream = op.freestream_velocity_geometry_axes();
            // The same angle function on both vectors: its sign convention
            // cancels in the difference.
            let angle = |v: [f64; 3]| v[2].atan2(v[0]).to_degrees();
            let local = angle(*velocity.first()?);
            let free = angle(freestream);
            Some((solved.cl_lift, free - local))
        };
        let (lo, hi) = (probe(PROBE_ALPHA_DEG[0])?, probe(PROBE_ALPHA_DEG[1])?);
        // Orient the downwash so it grows with lift: the flow behind a
        // lifting wing turns down.
        let sign = if (hi.1 - lo.1) * (hi.0 - lo.0) < 0.0 {
            -1.0
        } else {
            1.0
        };
        ((hi.0 - lo.0).abs() > 1e-9).then_some([(lo.0, sign * lo.1), (hi.0, sign * hi.1)])
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn wing(name: &str, x: f64, z: f64, semi_span: f64, chord: f64) -> Wing {
        let airfoil = Airfoil::from_name("naca0012").expect("a 4-digit NACA name");
        Wing::new(
            name,
            vec![
                WingXSec::new([x, 0.0, z], chord, 0.0, airfoil.clone()),
                WingXSec::new([x + 0.5, semi_span, z], 0.6 * chord, 0.0, airfoil),
            ],
            true,
        )
    }

    fn airplane(wings: Vec<Wing>) -> Airplane {
        let station = |x: f64, z: f64, radius: f64| {
            FuselageXSec::new([x, 0.0, z], Some(radius), None, None, 2.0)
                .expect("a radius with no width or height")
        };
        let body = Fuselage::new(
            "Fuselage",
            vec![
                station(-6.0, 0.0, 1.0),
                station(8.0, 0.0, 1.0),
                station(16.0, 0.8, 0.2),
            ],
        );
        Airplane {
            name: "downwash probe".to_owned(),
            xyz_ref: [1.0, 0.0, 0.0],
            wings,
            fuselages: vec![body],
            s_ref: 38.4,
            c_ref: 2.5,
            b_ref: 16.0,
        }
    }

    #[test]
    fn the_afterbody_flies_the_body_attitude_less_the_wing_downwash_on_the_datcom_ramp() {
        let plane = airplane(vec![
            wing(MAIN_WING_NAME, 0.0, 0.0, 8.0, 3.0),
            wing(HSTAB_NAME, 14.0, 2.0, 3.0, 1.5),
        ]);
        let aero = AeroAnalysis::new(&plane, 0.0, None, None, None);
        let flow = aero.afterbody_flow();
        // The ramp rises from the root trailing edge (x = 3) to the tail's
        // aerodynamic centre, so its mean over the afterbody is below one.
        assert!(flow.ramp_mean > 0.3 && flow.ramp_mean < 1.0, "{flow:?}");
        // Lifting line far behind the wing: 2 / (pi A) rad per unit CL, 5.4
        // deg at A = 6.7; the body trailing edge is not that far.
        assert!(
            flow.eps_per_cl_deg > 1.5 && flow.eps_per_cl_deg < 6.0,
            "{flow:?}"
        );
        // An untwisted symmetric wing sheds no downwash without lift.
        assert!(flow.eps0_deg.abs() < 0.05, "{flow:?}");
        let expected = flow.ramp_mean * (4.0 - flow.eps0_deg - flow.eps_per_cl_deg * 0.4);
        assert!((flow.alpha_deg(4.0, 0.4) - expected).abs() < 1e-12);
        // Without a main wing the afterbody flies the isolated-body angle.
        let alone = airplane(vec![wing(HSTAB_NAME, 14.0, 2.0, 3.0, 1.5)]);
        let isolated = AeroAnalysis::new(&alone, 0.0, None, None, None).afterbody_flow();
        assert_eq!(isolated, AfterbodyFlow::ISOLATED);
        assert_eq!(isolated.alpha_deg(4.0, 0.4), 4.0);
    }
}
