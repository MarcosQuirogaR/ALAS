// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Jet-inlet propulsive normal force on the neutral point.
//!
//! An engine inlet at angle of attack turns its captured stream tube onto the
//! nacelle axis; the momentum change is a normal force `N = mdot V alpha_i`
//! at the inlet lip, with `alpha_i = (1 + d eps_u / d alpha) alpha` ahead of
//! the wing. About a neutral point at `x_np` its moment derivative is
//! destabilising for an inlet ahead of it. Roskam, *Airplane Design* Part VI
//! (DARcorporation), sect. 8.2.8.2 "Jet driven airplanes", Eq. 8.111, printed
//! p. 343: `(dCm/dCL)_N = Sum 2 mdot_i (1 + d eps_u/d alpha)_i l_ni /
//! (rho U S c CLa)` (written there with the per-degree constant 0.035 =
//! 2/57.3), `l_n` from the inlet lip, positive forward; the upwash gradient
//! is read from Fig. 8.115 (p. 327, after the USAF DATCOM chart that
//! [`super::fuselage`] digitises as Fig. 4.2.2.1-22) at the inlet's distance
//! ahead of the local wing leading edge over the local chord. An inlet aft of
//! the wing (an aft-fuselage pod, or one past the local trailing edge) sits in
//! the wing's downwash instead and takes `1 - d eps / d alpha` of the
//! lattice; an inlet over the local chord sees flow the wing has already
//! turned, and its normal force is about zero (Raymer, *Aircraft Design: A
//! Conceptual Approach*, 6th ed., sect. 16.3.9, pp. 627-628).
//!
//! The neutral point moves by `-Sum N_alpha (x_np - x_inlet) / (q S CLa)`,
//! `CLa` the total lift-curve slope. The engine air flow and the flight
//! speed are those of the power setting each stability condition is flown
//! at (CS 25.175: climb at maximum continuous thrust, cruise and the maximum
//! operating speed at thrust equal to drag, landing at the thrust of a 3 deg
//! glide path), supplied by the caller per condition ([`JetInletSpec`]). Each
//! podded nacelle (`airplane.fuselages[1..]`) is one engine.
//!
//! Magnitude: no handbook works Eq. 8.111 through numerically; Raymer gives
//! the power-on reduction of the static margin of jets as about 1-3 % MAC
//! (*Aircraft Design: A Conceptual Approach*, 6th ed., sect. 16.3.3,
//! p. 624), the band the registered jets fall in at these power settings.

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;

use crate::trim::main_wing;

/// The engine operating state of one stability condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InletCondition {
    /// Total (core plus bypass) air mass flow of one engine, kg/s.
    pub mass_flow_kg_s: f64,
    /// Flight Mach number of the condition.
    pub mach: f64,
    /// Pressure altitude of the condition, m.
    pub altitude_m: f64,
}

/// The engine state of each neutral-point condition (CS 25.175).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JetInletSpec {
    /// Climb, flaps and gear up, maximum continuous thrust at 1.3 V_SR1
    /// (CS 25.175(a)): applied to the clean low-speed condition.
    pub climb: InletCondition,
    /// Landing, landing flaps and gear down at V_REF with the thrust of a
    /// 3 deg glide path (CS 25.175(d)): applied to the high-lift condition.
    pub approach: InletCondition,
    /// Cruise, thrust equal to drag at the cruise point (CS 25.175(b)).
    pub cruise: InletCondition,
    /// Thrust equal to drag at the maximum operating speed, when the
    /// configuration states one.
    pub max_operating: Option<InletCondition>,
}

/// The slopes of one neutral-point condition the inlet term reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InletSlopes {
    /// Total lift-curve slope, 1/rad.
    pub cl_alpha_total: f64,
    /// Wing-body lift-curve slope the upwash chart scales with, 1/rad.
    pub cl_alpha_wb: f64,
    /// Lattice downwash gradient, for inlets aft of the wing.
    pub deps_dalpha: f64,
}

/// The neutral-point shift of every podded inlet at one condition, m
/// (negative: forward). `x_np` is the condition's neutral point. Zero
/// without pods, with a non-positive flow or slope, or at zero speed.
#[must_use]
pub fn jet_inlet_shift_m(
    airplane: &Airplane,
    condition: InletCondition,
    x_np: f64,
    slopes: InletSlopes,
) -> f64 {
    let InletSlopes {
        cl_alpha_total,
        cl_alpha_wb,
        deps_dalpha,
    } = slopes;
    let InletCondition {
        mass_flow_kg_s: mdot,
        mach,
        altitude_m,
    } = condition;
    let atmosphere = Atmosphere::new(altitude_m);
    let speed = mach * atmosphere.speed_of_sound();
    let density = atmosphere.density();
    if airplane.fuselages.len() < 2
        || !(cl_alpha_total.is_finite() && cl_alpha_total > 0.1)
        || !(speed > 1.0 && density > 0.0 && mdot.is_finite() && mdot > 0.0)
        || !x_np.is_finite()
    {
        return 0.0;
    }
    let wing = main_wing(airplane);
    let moment_per_alpha: f64 = airplane.fuselages[1..]
        .iter()
        .filter_map(|pod| {
            let first = pod.xsecs.first()?;
            let x_inlet = first.xyz_c[0];
            let section =
                wing.and_then(|w| super::fuselage::wing_section_at(w, first.xyz_c[1].abs()));
            let flow_angle_factor = match section {
                Some((x_le, chord)) if !pod.fuselage_mounted && x_inlet < x_le && chord > 0.0 => {
                    1.0 + super::fuselage::forebody_upwash_gradient(
                        (x_le - x_inlet) / chord,
                        cl_alpha_wb,
                    )
                }
                Some((x_le, chord)) if !pod.fuselage_mounted && x_inlet <= x_le + chord => 0.0,
                _ => 1.0 - deps_dalpha.clamp(0.0, 1.0),
            };
            Some(mdot * speed * flow_angle_factor * (x_np - x_inlet))
        })
        .sum();
    let q = 0.5 * density * speed * speed;
    let s_ref = airplane
        .s_ref
        .max(crate::reference_dimensions::REFERENCE_AREA_FLOOR_M2);
    -moment_per_alpha / (q * s_ref * cl_alpha_total)
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

    fn plane(inlet_x: f64) -> Airplane {
        let naca = Airfoil::from_name("naca0012").expect("valid NACA name");
        let wing = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([20.0, 0.0, 0.0], 8.0, 0.0, naca.clone()),
                WingXSec::new([30.0, 25.0, 0.0], 2.5, 0.0, naca),
            ],
            true,
        );
        let station = |x: f64, y: f64, r: f64| {
            FuselageXSec::new([x, y, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        let fuselage = Fuselage::new(
            "Fuselage",
            vec![station(0.0, 0.0, 3.0), station(60.0, 0.0, 1.0)],
        );
        let pod = |y: f64| {
            Fuselage::new(
                "Nacelle",
                vec![station(inlet_x, y, 1.2), station(inlet_x + 6.0, y, 1.0)],
            )
        };
        let (s_ref, b_ref, c_ref) = (
            wing.reference_area(),
            wing.reference_span(),
            wing.mean_aerodynamic_chord(),
        );
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [28.0, 0.0, 0.0],
            wings: vec![wing],
            fuselages: vec![fuselage, pod(8.0), pod(-8.0)],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn an_inlet_ahead_of_the_neutral_point_moves_it_forward_in_proportion_to_its_arm() {
        let condition = InletCondition {
            mass_flow_kg_s: 600.0,
            mach: 0.25,
            altitude_m: 0.0,
        };
        let slopes = InletSlopes {
            cl_alpha_total: 5.5,
            cl_alpha_wb: 5.0,
            deps_dalpha: 0.4,
        };
        let shift = |x_inlet: f64| jet_inlet_shift_m(&plane(x_inlet), condition, 30.0, slopes);
        let ahead = shift(18.0);
        assert!(ahead < 0.0, "inlet ahead of the NP: {ahead:+.3} m");
        // Hand check at sea level, Mach 0.25: two engines, factor 1 + upwash
        // (chart >= 1), arm 12 m.
        let atmosphere = Atmosphere::new(0.0);
        let v = 0.25 * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * v * v;
        let s = plane(18.0).s_ref;
        let bare = 2.0 * 600.0 * v * 12.0 / (q * s * 5.5);
        assert!(
            -ahead >= bare && -ahead < 1.6 * bare,
            "{ahead:+.4} vs {bare:.4}"
        );
        // An inlet over the local chord (leading edge 23.2 m, chord 6.24 m
        // at y = 8 m) sees turned flow: no force.
        assert_eq!(shift(25.0), 0.0);
        // An inlet behind the neutral point (in the wing's downwash) is
        // stabilising.
        assert!(shift(40.0) > 0.0);
        let still = InletCondition {
            mach: 0.0,
            ..condition
        };
        assert_eq!(jet_inlet_shift_m(&plane(18.0), still, 30.0, slopes), 0.0);
    }
}
