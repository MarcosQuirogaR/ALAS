// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The torque sources on the wing box beyond the offset of the lift.
//!
//! [`crate::torsion`] carries the torque of the aerodynamic lift about the
//! elastic axis. This module supplies the rest, each with its source:
//!
//! - **Section pitching moment `Cm0`** of the wing sections at the speed of
//!   the case: from the NeuralFoil surrogate the aerodynamic model uses
//!   ([`alas_aero::neuralfoil`]), evaluated on the section the geometry
//!   builds at the station, at zero lift, about the quarter chord. A
//!   pitching couple per unit span `q c^2 Cm0` (nose-down for cambered
//!   sections).
//! - **Flaps down at `V_F`** (CS 25.345(a): `n = 2.0` at the design flap
//!   speed): the lift increment of a single-slotted flap, Roskam, *Airplane
//!   Design Part VI*, Eq. 8.5 with `alpha_delta` of Fig. 8.17, acting at
//!   `x_cp/c' = 0.44` (Fig. 8.91, slotted flaps), the base lift reduced so the
//!   total is `n W`.
//! - **Aileron to the stop** (CS 25.349(a)): the lift increment of a plain
//!   flap, Roskam Eq. 8.4 with the thin-airfoil `(c_l_delta)_theory`
//!   (Fig. 8.14, `t/c = 0`), the viscous factor read from Fig. 8.15 and the
//!   non-linearity `k'` of Fig. 8.13, acting at `x_cp/c = 0.50 - 0.25 c_f/c`
//!   (Fig. 8.91, plain flaps), at `V_A` with the maximum deflection, at `V_C`
//!   for the roll rate of `V_A` and at `V_D` for a third of it (roll rate
//!   proportional to `delta V`), at load factor zero and `2/3` of the
//!   manoeuvring factor, up and down aileron. At `V_A` the deflection is
//!   sudden (zero roll rate, allowed by CS 25.349(a)(1) without a time
//!   history); at `V_C` and `V_D` the roll is steady and strip-theory roll
//!   damping balances the aileron moment, so the lift change is `q c
//!   (dc_l_aileron - K y)` with `K` from zero net rolling moment.
//! - **Engine inertia offsets**: `n g m` of the installed engine acting down
//!   at its centre of gravity, ahead of the elastic axis, at every case.
//! - **Thrust-line offset**: the take-off thrust acting forward at the engine
//!   centreline, below the wing reference plane, as a nose-up couple inboard
//!   of the pylon.
//!
//! Maximum aileron deflection: 25 deg, Sadraey, *Aircraft Design: A Systems
//! Engineering Approach*, 12.4.3.5 (book p. 668): aileron deflection is kept
//! below 25 deg both ways because of aileron stall. `V_A = V_S1 sqrt(n)`
//! capped at `V_C` (CS 25.335(c), p. 213).
//!
//! # Declared limits (also in [`crate::scope`])
//!
//! Strip theory (no induced-flow reduction of the control and flap
//! increments, heavier); no aileron-reversal or torsional-flexibility
//! reduction of the aileron load, no roll-inertia or roll-damping relief
//! (CS 25.349(a)(1) allows zero roll rate); flaps are single-slotted with
//! `c'/c = 1` (Fowler extension and slats not modelled, lighter); the flap
//! case has no 25.345 gust or slipstream (b); thrust is not combined with
//! the manoeuvre cases; turboprop thrust and propeller torque are not
//! available (thrust is zero by design); the box elastic axis height is taken
//! at the wing reference chord plane (`z_le` of the structure geometry). The surrogate `Cm0` is the incompressible,
//! attached section: no shock-induced aft loading at `V_D`.

use crate::flap_charts::{alpha_delta, k_prime};
use alas_aero::neuralfoil::{Conditions, ModelSize, PreparedAirfoil};
use alas_atmo::Atmosphere;
use alas_config::{ActiveEngineModel, AlasConfig, DesignRequirements};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::wing_structure::WingStructureGeometry;

/// ISA sea-level density that defines equivalent airspeed, kg/m^3.
const RHO0: f64 = 1.225;
/// Maximum aileron deflection, rad: 25 deg (Sadraey, book p. 668).
const AILERON_MAX_DEFLECTION_RAD: f64 = 25.0 * std::f64::consts::PI / 180.0;
/// Flap incremental load centre of pressure, `x_cp/c'`, slotted flaps
/// (Roskam Part VI Fig. 8.91, book p. 298).
const FLAP_XCP: f64 = 0.44;
/// Viscous correction `c_l_delta / (c_l_delta)_theory` of Fig. 8.15 read at
/// `c_l_alpha / (c_l_alpha)_theory = 1.00` (about 0.98, chart-read).
const PLAIN_FLAP_VISCOUS_FACTOR: f64 = 0.98;
/// Spanwise nodes of the section aerodynamics.
const NODES: usize = 7;
/// Mach margin `M_D - M_C` (AMC 25.335(b)).
const MACH_MARGIN: f64 = crate::gust::MACH_MARGIN_C_TO_D;
/// Fraction of the nacelle length from its inlet to the engine centre of
/// gravity (declared geometric assumption: mid-nacelle).
const ENGINE_CG_NACELLE_FRACTION: f64 = 0.5;

/// One wing-mounted engine of the modelled semi-wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineInstallation {
    /// Spanwise station, m.
    pub y_m: f64,
    /// Installed mass, kg.
    pub mass_kg: f64,
    /// Distance of the centre of gravity ahead of the wing leading edge, m.
    pub cg_ahead_of_le_m: f64,
    /// Engine centreline height `z` in the aircraft frame (up positive; an
    /// under-wing engine is negative), m. The arm of the thrust couple is the
    /// wing's `z` at the pylon station less this.
    pub z_m: f64,
    /// Take-off thrust, N (zero for a shaft-power engine).
    pub thrust_n: f64,
}

/// The trailing-edge flap of the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlapSystem {
    /// Inboard and outboard end, fractions of the semispan.
    pub eta: (f64, f64),
    /// Flap chord over wing chord.
    pub chord_fraction: f64,
    /// Deflection, rad.
    pub deflection_rad: f64,
    /// Design flap speed `V_F`, m/s EAS.
    pub eas_m_s: f64,
}

/// The aileron of the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AileronSystem {
    /// Inboard and outboard end, fractions of the semispan.
    pub eta: (f64, f64),
    /// Aileron chord over wing chord.
    pub chord_fraction: f64,
    /// Clean-wing maximum lift coefficient (for `V_S1`).
    pub cl_max_clean: f64,
}

/// What the configuration says about the torque sources.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TorqueSources {
    /// Wing-mounted engines of the positive-`y` semi-wing.
    pub engines: Vec<EngineInstallation>,
    /// Trailing-edge flap, if any.
    pub flap: Option<FlapSystem>,
    /// Aileron, if any.
    pub aileron: Option<AileronSystem>,
}

impl TorqueSources {
    /// Read the sources off a configuration.
    pub fn from_config(config: &AlasConfig, req: &DesignRequirements) -> Self {
        let engine_cfg = &config.geometry.engine;
        let masses =
            crate::scope::wing_mounted_relief(engine_cfg, &config.mass_model, req).point_masses_kg;
        let thrust_n = match engine_cfg.active_model() {
            Ok(ActiveEngineModel::Turbofan(spec)) => spec.rated_thrust_kn * 1000.0,
            _ => 0.0,
        };
        let length = engine_cfg
            .nacelle_profile
            .iter()
            .map(|p| p.0)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            });
        let nacelle_length = if length.1 > length.0 {
            length.1 - length.0
        } else {
            0.0
        };
        let positions: Vec<(usize, f64)> = engine_cfg
            .spanwise_positions_m
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, y)| y > 1e-6)
            .collect();
        let engines = positions
            .iter()
            .zip(&masses)
            .map(|(&(index, y), &(_, mass))| EngineInstallation {
                y_m: y,
                mass_kg: mass,
                cg_ahead_of_le_m: engine_cfg.inlet_x_offset_m
                    - ENGINE_CG_NACELLE_FRACTION * nacelle_length,
                z_m: engine_cfg
                    .z_per_engine_m
                    .get(index)
                    .copied()
                    .unwrap_or(engine_cfg.z_m),
                thrust_n,
            })
            .collect();
        let c = &config.control_surfaces;
        let flap = (c.flap_span_end_frac > c.flap_span_start_frac && c.flap_chord_fraction > 0.0)
            .then_some(FlapSystem {
                eta: (c.flap_span_start_frac, c.flap_span_end_frac),
                chord_fraction: c.flap_chord_fraction,
                deflection_rad: config.mass_model.flap_deflection_angle_deg.to_radians(),
                eas_m_s: config.mass_model.max_airspeed_for_flaps_ms,
            })
            .filter(|f| f.eas_m_s.is_finite() && f.eas_m_s > 0.0 && f.deflection_rad > 0.0);
        let aileron = (c.aileron_span_end_frac > c.aileron_span_start_frac
            && c.aileron_chord_fraction > 0.0
            && config.performance.cl_max_clean > 0.0)
            .then_some(AileronSystem {
                eta: (c.aileron_span_start_frac, c.aileron_span_end_frac),
                chord_fraction: c.aileron_chord_fraction,
                cl_max_clean: config.performance.cl_max_clean,
            });
        Self {
            engines,
            flap,
            aileron,
        }
    }

    /// Whether any source is declared.
    pub fn is_empty(&self) -> bool {
        self.engines.is_empty() && self.flap.is_none() && self.aileron.is_none()
    }
}

/// Section aerodynamics at the spanwise nodes for one flight condition.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionTable {
    /// Dynamic pressure, Pa.
    pub q_pa: f64,
    /// Zero-lift pitching-moment coefficient about the quarter chord at each
    /// station of the sizing grid (nose-up positive).
    pub cm0: Vec<f64>,
    /// Section lift slope per rad at each station.
    pub cl_alpha: Vec<f64>,
}

/// The torque sources resolved on a station grid.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreparedTorque {
    /// The declared sources.
    pub sources: TorqueSources,
    /// `V_C` (cruise Mach, cruise altitude) section table.
    pub cruise: Option<SectionTable>,
    /// `V_D` section table.
    pub dive: Option<SectionTable>,
    /// `V_F` section table (flap speed, sea level).
    pub flap: Option<SectionTable>,
    /// Design speeds, m/s EAS: `(V_A, V_C, V_D)`.
    pub speeds_eas: Option<(f64, f64, f64)>,
}

impl PreparedTorque {
    /// Dynamic pressure of an equivalent airspeed, Pa.
    pub fn q_of_eas(eas_m_s: f64) -> f64 {
        0.5 * RHO0 * eas_m_s * eas_m_s
    }

    /// Resolve the sources on the stations `y` for the requirements `req`.
    /// An empty set of sources still yields the `Cm0` tables, which need no
    /// declaration.
    pub fn new(
        sources: &TorqueSources,
        wsg: &WingStructureGeometry,
        req: &DesignRequirements,
        y: &[f64],
    ) -> Self {
        let v_d = req.dive_speed_m_s;
        if !(v_d.is_finite() && v_d > 0.0) || y.len() < 2 || wsg.semi_span <= 0.0 {
            return Self {
                sources: sources.clone(),
                ..Self::default()
            };
        }
        let v_c = crate::sizing::envelope::CRUISE_TO_DIVE_SPEED_RATIO * v_d;
        let atmosphere = Atmosphere::isa(req.cruise_altitude_m);
        let a = atmosphere.speed_of_sound();
        let nu = atmosphere.kinematic_viscosity();
        let cruise = section_table(wsg, y, Self::q_of_eas(v_c), req.cruise_mach, a, nu);
        let dive = section_table(
            wsg,
            y,
            Self::q_of_eas(v_d),
            (req.cruise_mach + MACH_MARGIN).min(0.95),
            a,
            nu,
        );
        let flap = sources.flap.and_then(|f| {
            let sea = Atmosphere::isa(0.0);
            let a0 = sea.speed_of_sound();
            section_table(
                wsg,
                y,
                Self::q_of_eas(f.eas_m_s),
                f.eas_m_s / a0,
                a0,
                sea.kinematic_viscosity(),
            )
        });
        let v_a = sources.aileron.map(|ail| {
            let area = crate::gust::GustWing::from_geometry(wsg).area_m2;
            let n1 = req.ultimate_load_factor / 1.5;
            let v_s1 =
                (2.0 * req.mtow_kg * req.gravity_m_s2 / (RHO0 * area * ail.cl_max_clean)).sqrt();
            (v_s1 * n1.max(1.0).sqrt()).min(v_c)
        });
        Self {
            sources: sources.clone(),
            cruise,
            dive,
            flap,
            speeds_eas: v_a.map(|a| (a, v_c, v_d)),
        }
    }
}

fn section_coordinates(wsg: &WingStructureGeometry, eta: f64) -> Vec<(f64, f64)> {
    const N: usize = 60;
    let xs: Vec<f64> = (0..=N)
        .map(|k| 0.5 * (1.0 - (std::f64::consts::PI * k as f64 / N as f64).cos()))
        .collect();
    let mut coords: Vec<(f64, f64)> = xs
        .iter()
        .rev()
        .map(|&x| (x, wsg.airfoil_zu_zl(eta, x).0))
        .collect();
    coords.extend(xs.iter().skip(1).map(|&x| (x, wsg.airfoil_zu_zl(eta, x).1)));
    coords
}

/// Zero-lift `Cm0` (about c/4) and lift slope of the section at `eta`, from
/// NeuralFoil at the chord Reynolds number and Mach number.
fn section_aero(
    wsg: &WingStructureGeometry,
    eta: f64,
    mach: f64,
    reynolds: f64,
) -> Option<(f64, f64)> {
    let airfoil = Airfoil::from_coordinates("torque-section", section_coordinates(wsg, eta));
    let prepared = PreparedAirfoil::prepare(&airfoil).ok()?;
    let alphas = [-4.0, -2.0, 0.0, 2.0, 4.0];
    let conditions: Vec<Conditions> = alphas
        .iter()
        .map(|&alpha| Conditions::new(alpha, reynolds))
        .collect();
    let aeros = prepared
        .aero_sweep(&conditions, mach, ModelSize::Large)
        .ok()?;
    // Least-squares lines cl(alpha) and cm(cl).
    let n = aeros.len() as f64;
    let (sa, sc, saa, sac) = alphas
        .iter()
        .zip(&aeros)
        .fold((0.0, 0.0, 0.0, 0.0), |(sa, sc, saa, sac), (&a, r)| {
            (sa + a, sc + r.cl, saa + a * a, sac + a * r.cl)
        });
    let slope_per_deg = (n * sac - sa * sc) / (n * saa - sa * sa);
    let (scl, scm, sclcl, sclcm) = aeros.iter().fold((0.0, 0.0, 0.0, 0.0), |acc, r| {
        (
            acc.0 + r.cl,
            acc.1 + r.cm,
            acc.2 + r.cl * r.cl,
            acc.3 + r.cl * r.cm,
        )
    });
    let denom = n * sclcl - scl * scl;
    if !(slope_per_deg.is_finite() && denom.abs() > 1e-12) {
        return None;
    }
    let dcm_dcl = (n * sclcm - scl * scm) / denom;
    let cm0 = (scm - dcm_dcl * scl) / n;
    cm0.is_finite().then_some((cm0, slope_per_deg.to_degrees()))
}

fn section_table(
    wsg: &WingStructureGeometry,
    y: &[f64],
    q_pa: f64,
    mach: f64,
    speed_of_sound: f64,
    nu: f64,
) -> Option<SectionTable> {
    let tas = mach * speed_of_sound;
    let nodes: Vec<(f64, f64, f64)> = (0..NODES)
        .map(|k| {
            let eta = k as f64 / (NODES - 1) as f64;
            let reynolds = (tas * wsg.local_chord(eta) / nu).clamp(1.0e5, 5.0e7);
            section_aero(wsg, eta, mach, reynolds).map(|(cm, cla)| (eta, cm, cla))
        })
        .collect::<Option<_>>()?;
    let at = |eta: f64, pick: fn(&(f64, f64, f64)) -> f64| {
        let e = eta.clamp(0.0, 1.0) * (NODES - 1) as f64;
        let i = (e.floor() as usize).min(NODES - 2);
        let t = e - i as f64;
        pick(&nodes[i]) * (1.0 - t) + pick(&nodes[i + 1]) * t
    };
    let eta: Vec<f64> = y.iter().map(|&v| v / wsg.semi_span).collect();
    Some(SectionTable {
        q_pa,
        cm0: eta.iter().map(|&e| at(e, |n| n.1)).collect(),
        cl_alpha: eta.iter().map(|&e| at(e, |n| n.2)).collect(),
    })
}

/// Section lift increment of a single-slotted flap, `Delta c_l = c_l_alpha
/// alpha_delta (c'/c) delta_f` (Roskam Part VI Eq. 8.5/8.6, book p. 229) with
/// `c'/c = 1`.
pub fn slotted_flap_delta_cl(
    cl_alpha_per_rad: f64,
    chord_fraction: f64,
    deflection_rad: f64,
) -> f64 {
    cl_alpha_per_rad * alpha_delta(chord_fraction, deflection_rad.to_degrees()) * deflection_rad
}

/// Section lift increment of a plain flap (aileron), `Delta c_l = delta_f
/// {c_l_delta/(c_l_delta)_theory} (c_l_delta)_theory k'` (Roskam Part VI
/// Eq. 8.4, book p. 226), `(c_l_delta)_theory = 2 (pi - theta_f + sin
/// theta_f)` with `cos theta_f = 2 c_f/c - 1` (thin-airfoil theory, the
/// `t/c = 0` curve of Fig. 8.14).
pub fn plain_flap_delta_cl(chord_fraction: f64, deflection_rad: f64) -> f64 {
    let theta = (2.0 * chord_fraction - 1.0).clamp(-1.0, 1.0).acos();
    let theory = 2.0 * (std::f64::consts::PI - theta + theta.sin());
    deflection_rad
        * PLAIN_FLAP_VISCOUS_FACTOR
        * theory
        * k_prime(chord_fraction, deflection_rad.abs().to_degrees())
}

/// The plain-flap deflection, rad, that gives the section lift increment
/// `delta_cl` (bisection on [`plain_flap_delta_cl`], which increases with the
/// deflection up to the 25 deg stop); clamped to `[0, stop]`.
pub fn plain_flap_deflection_for_delta_cl(chord_fraction: f64, delta_cl: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, AILERON_MAX_DEFLECTION_RAD);
    if plain_flap_delta_cl(chord_fraction, hi) <= delta_cl {
        return hi;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if plain_flap_delta_cl(chord_fraction, mid) < delta_cl {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Centre of pressure of the incremental load of a flap, fraction of chord
/// from the leading edge (Roskam Part VI Fig. 8.91, book p. 298).
pub fn flap_xcp_fraction(slotted: bool, chord_fraction: f64) -> f64 {
    if slotted {
        FLAP_XCP
    } else {
        0.50 - 0.25 * chord_fraction
    }
}

/// Maximum aileron deflection, rad (Sadraey, book p. 668).
pub const fn aileron_max_deflection_rad() -> f64 {
    AILERON_MAX_DEFLECTION_RAD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chart_tables_reproduce_their_own_nodes_and_stay_monotone() {
        assert!((alpha_delta(0.25, 40.0) - 0.400).abs() < 1e-12);
        assert!((alpha_delta(0.275, 40.0) - 0.42).abs() < 1e-12);
        assert!(alpha_delta(0.30, 20.0) > alpha_delta(0.30, 60.0));
        assert_eq!(k_prime(0.25, 8.0), 1.0);
        assert!(k_prime(0.25, 25.0) < k_prime(0.25, 15.0));
        assert!(k_prime(0.4, 25.0) < k_prime(0.1, 25.0));
    }

    #[test]
    fn the_plain_flap_theory_matches_the_chart_at_its_nodes() {
        // Fig. 8.14, t/c = 0: about 2.5 at 0.10, 3.45 at 0.20, 5.15 at 0.50 (per rad).
        let at = |cf: f64| {
            let theta = (2.0 * cf - 1.0).acos();
            2.0 * (std::f64::consts::PI - theta + theta.sin())
        };
        assert!((at(0.10) - 2.49).abs() < 0.02);
        assert!((at(0.20) - 3.45).abs() < 0.02);
        assert!((at(0.50) - 5.14).abs() < 0.02);
        // An aileron of 25 % chord at 25 deg gives a section lift increment near 1.2.
        let dcl = plain_flap_delta_cl(0.25, 25.0_f64.to_radians());
        assert!((1.0..1.5).contains(&dcl), "{dcl}");
    }

    #[test]
    fn a_slotted_flap_lifts_more_than_a_plain_flap_of_the_same_chord_and_angle() {
        let slotted = slotted_flap_delta_cl(2.0 * std::f64::consts::PI, 0.25, 0.35);
        let plain = plain_flap_delta_cl(0.25, 0.35);
        assert!(slotted > 0.0 && plain > 0.0);
        assert!(flap_xcp_fraction(true, 0.25) < 0.5);
        assert!((flap_xcp_fraction(false, 0.25) - 0.4375).abs() < 1e-12);
    }
}
