// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing-box torque of the aerodynamic load about the elastic axis.
//!
//! The spanwise load of [`crate::loads`] acts on the elastic axis of the beam.
//! A real wing carries its lift at the aerodynamic centre, ahead of the box
//! mid-chord, and a swept-back wing also moves the outboard load aft of the
//! inboard station. Both appear as torque about the elastic axis, which the
//! closed box carries as a constant shear flow `q = T / (2 A)` around its
//! cell (Bredt-Batho), on top of the shear flow of the vertical shear.
//!
//! # Model
//!
//! - The elastic axis is the line through the box mid-chord,
//!   `x_ea(y) = x_le(y) + x_box c(y)`, `x_box = (front + rear spar) / 2`.
//! - The load acts at the quarter chord `x_ac(y) = x_le(y) + c(y) / 4`.
//! - The torque about the local axis direction `e = (sin L, cos L)` of a
//!   station `y` from the load outboard of it is `T(y) = cos L (integral over
//!   y' > y of q(y') [(y' - y) tan L - (x_ac(y') - x_ea(y))] dy')`, with `tan
//!   L = dx_ea/dy` the local sweep of the elastic axis. A load on the axis
//!   gives zero.
//! - Only the aerodynamic part of the load is offset: the inertia of the box,
//!   its fuel and its engines is taken to act on the axis, so the relieved
//!   share does not twist the box.
//!
//! Torenbeek, *Advanced Aircraft Design* (2013), section 11.3.4, book p. 334
//! (PDF p. 350), states the same physics qualitatively: a straight wing has a
//! nose-up torque of about the lift bending moment divided by the aspect
//! ratio because the centre of pressure is ahead of the elastic axis, and
//! sweepback shifts the centre of pressure aft and gives the inboard wing
//! negative torsion. This model reproduces both signs from the geometry.
//!
//! # Not modelled
//!
//! The section pitching moment of cambered and aft-loaded sections (largest,
//! nose-down, at `V_D`), aileron deflection and spoiler loads (CS 25.349(a):
//! they need control-surface moments that are not inputs here), flap loads
//! (CS 25.345), and engine inertia offsets. All of them add torque in
//! magnitude somewhere on the span; the model therefore under-states the
//! torque case and says so in [`crate::scope`].

use alas_geom::wing_structure::WingStructureGeometry;

/// Torque of the aerodynamic load `q_aero` (N/m, positive up, on the stations
/// `y`) about the local elastic axis, N m, nose-up positive.
///
/// `box_axis_fraction` is the chord fraction of the elastic axis, the mean of
/// the outermost spar fractions. `y` runs from the root (index 0) to the tip.
pub fn aerodynamic_torque_nm(
    wsg: &WingStructureGeometry,
    y: &[f64],
    q_aero: &[f64],
    box_axis_fraction: f64,
) -> Vec<f64> {
    let n = y.len();
    if n < 2 || q_aero.len() != n || wsg.semi_span <= 0.0 {
        return vec![0.0; n];
    }
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let x_axis: Vec<f64> = eta
        .iter()
        .map(|&e| wsg.x_le(e) + box_axis_fraction * wsg.local_chord(e))
        .collect();
    let x_load: Vec<f64> = eta
        .iter()
        .map(|&e| wsg.x_le(e) + 0.25 * wsg.local_chord(e))
        .collect();
    (0..n)
        .map(|i| {
            // Local sweep of the axis: forward difference, backward at the tip.
            let (a, b) = if i + 1 < n { (i, i + 1) } else { (i - 1, i) };
            let tan_sweep = (x_axis[b] - x_axis[a]) / (y[b] - y[a]);
            let integrand =
                |j: usize| q_aero[j] * ((y[j] - y[i]) * tan_sweep - (x_load[j] - x_axis[i]));
            let mut torque = 0.0;
            for j in i..n - 1 {
                torque += 0.5 * (integrand(j) + integrand(j + 1)) * (y[j + 1] - y[j]);
            }
            torque / (1.0 + tan_sweep * tan_sweep).sqrt()
        })
        .collect()
}

/// The loads whose torque about the elastic axis [`torque_of_loads_nm`]
/// sums. Frame: `x` aft, `y` to the right, `z` up; forces up and torques
/// nose-up are positive. Every `x` is absolute in the wing-structure frame.
#[derive(Debug, Clone, Default)]
pub struct TorqueLoads<'a> {
    /// Running lifts `(q N/m, x m)`, both on the station grid.
    pub lifts: Vec<(&'a [f64], &'a [f64])>,
    /// Running pitching couple, N m/m, nose-up positive, on the station grid.
    pub couple_n_per_m: Option<&'a [f64]>,
    /// Point forces `(y m, force up N, x m)`.
    pub forces: Vec<(f64, f64, f64)>,
    /// Point couples `(y m, nose-up N m)`.
    pub couples: Vec<(f64, f64)>,
}

/// Torque about the local elastic axis of everything outboard of each
/// station, N m, nose-up positive: the generalisation of
/// [`aerodynamic_torque_nm`] (which it reproduces for one quarter-chord lift)
/// to lifts at other chord positions, pitching couples and point loads.
///
/// A force `F` up at `(y_k, x_k)` outboard of station `i` contributes
/// `F ((y_k - y_i) tan L_i - (x_k - x_ea_i))`; a couple `C` contributes `C`;
/// the sum is projected on the swept axis with `cos L_i`.
pub fn torque_of_loads_nm(
    wsg: &WingStructureGeometry,
    y: &[f64],
    box_axis_fraction: f64,
    loads: &TorqueLoads<'_>,
) -> Vec<f64> {
    let n = y.len();
    if n < 2 || wsg.semi_span <= 0.0 {
        return vec![0.0; n];
    }
    let x_axis: Vec<f64> = y
        .iter()
        .map(|&yi| {
            let e = yi / wsg.semi_span;
            wsg.x_le(e) + box_axis_fraction * wsg.local_chord(e)
        })
        .collect();
    (0..n)
        .map(|i| {
            let (a, b) = if i + 1 < n { (i, i + 1) } else { (i - 1, i) };
            let tan_sweep = (x_axis[b] - x_axis[a]) / (y[b] - y[a]);
            let arm = |yk: f64, xk: f64| (yk - y[i]) * tan_sweep - (xk - x_axis[i]);
            let mut torque = 0.0;
            for j in i..n - 1 {
                let mut f0 = 0.0;
                let mut f1 = 0.0;
                for &(q, x) in &loads.lifts {
                    f0 += q[j] * arm(y[j], x[j]);
                    f1 += q[j + 1] * arm(y[j + 1], x[j + 1]);
                }
                if let Some(m) = loads.couple_n_per_m {
                    f0 += m[j];
                    f1 += m[j + 1];
                }
                torque += 0.5 * (f0 + f1) * (y[j + 1] - y[j]);
            }
            for &(yk, force, xk) in &loads.forces {
                if yk > y[i] {
                    torque += force * arm(yk, xk);
                }
            }
            for &(yk, couple) in &loads.couples {
                if yk > y[i] {
                    torque += couple;
                }
            }
            torque / (1.0 + tan_sweep * tan_sweep).sqrt()
        })
        .collect()
}

/// Area enclosed by the box cell between the outermost active spars, m^2:
/// the cover width between them times the mean of their depths. Zero when
/// fewer than two spars are active.
pub fn enclosed_area_m2(chord_m: f64, spar_fractions: &[f64], spar_heights_m: &[f64]) -> f64 {
    let active: Vec<(f64, f64)> = spar_fractions
        .iter()
        .copied()
        .zip(spar_heights_m.iter().copied())
        .filter(|&(_, h)| h > 0.0)
        .collect();
    match (active.first(), active.last()) {
        (Some(&(f0, h0)), Some(&(f1, h1))) if active.len() >= 2 => {
            (f1 - f0).max(0.0) * chord_m * 0.5 * (h0 + h1)
        }
        _ => 0.0,
    }
}

/// Constant shear flow of a torque in a closed cell, N/m (Bredt-Batho,
/// `q = T / (2 A)`); zero for a cell without area.
pub fn bredt_shear_flow_n_per_m(torque_nm: f64, enclosed_area_m2: f64) -> f64 {
    if enclosed_area_m2 > 0.0 {
        torque_nm.abs() / (2.0 * enclosed_area_m2)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_general_torque_reproduces_the_quarter_chord_lift_torque() {
        use alas_config::{presets, AlasConfig};
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        let design = presets::get("A320-200").unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let wing = plane.wings.iter().find(|w| w.name == "Main Wing").unwrap();
        let (fr, fl) = config.structures.resolved_spars();
        let wsg = WingStructureGeometry::new(
            &design,
            &config.geometry.wing,
            &wing.xsecs[0].airfoil,
            &wing.xsecs.last().unwrap().airfoil,
            &fr,
            Some(&fl),
        )
        .unwrap();
        let y = crate::sizing::sizing_stations(&wsg, &config.structures);
        let q = crate::loads::elliptic_distributed_load(&y, wsg.semi_span, 5.0e5);
        let (front, rear) = crate::sizing::box_chord_band(&wsg);
        let axis = 0.5 * (front + rear);
        let x: Vec<f64> = y
            .iter()
            .map(|&yi| {
                let e = yi / wsg.semi_span;
                wsg.x_le(e) + 0.25 * wsg.local_chord(e)
            })
            .collect();
        let old = aerodynamic_torque_nm(&wsg, &y, &q, axis);
        let new = torque_of_loads_nm(
            &wsg,
            &y,
            axis,
            &TorqueLoads {
                lifts: vec![(&q, &x)],
                ..Default::default()
            },
        );
        for (a, b) in old.iter().zip(&new) {
            assert!((a - b).abs() <= 1e-9 * (a.abs() + 1.0));
        }
        // A pure couple outboard of a station reaches it undiminished (cos L
        // projection only); one inboard does not.
        let couples = torque_of_loads_nm(
            &wsg,
            &y,
            axis,
            &TorqueLoads {
                couples: vec![(0.5 * wsg.semi_span, -1.0e5)],
                ..Default::default()
            },
        );
        assert!(couples[0] < -0.9e5 && couples[0] > -1.0e5 - 1e-6);
        assert_eq!(couples[y.len() - 1], 0.0);
    }

    #[test]
    fn bredt_batho_is_torque_over_twice_the_enclosed_area() {
        assert!((bredt_shear_flow_n_per_m(-1.0e6, 2.5) - 2.0e5).abs() < 1e-6);
        assert_eq!(bredt_shear_flow_n_per_m(1.0e6, 0.0), 0.0);
    }

    #[test]
    fn the_cell_area_uses_the_outermost_active_spars() {
        let area = enclosed_area_m2(4.0, &[0.25, 0.5, 0.7], &[0.8, 0.0, 0.6]);
        assert!((area - 0.45 * 4.0 * 0.7).abs() < 1e-12);
        assert_eq!(enclosed_area_m2(4.0, &[0.25, 0.7], &[0.8, 0.0]), 0.0);
    }
}
