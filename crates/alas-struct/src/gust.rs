// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quasi-static discrete vertical gust load factor of the wing.
//!
//! CS-25.341(a) requires a dynamic analysis of the flexible aeroplane in a
//! `1 - cos` gust. This module is the conceptual-design form of that case: the
//! design gust velocity of CS-25.341(a)(4)-(6) applied to the rigid-aeroplane
//! quasi-static load factor of Pratt, with the gust alleviation factor that
//! accounts for the plunging motion and the lift build-up time. It is **not**
//! the dynamic analysis the rule asks for: structural flexibility, unsteady
//! aerodynamics, the search over gust gradients and the pitching response are
//! not modelled, and the result is a screening load that has to be replaced by
//! a dynamic analysis before it is read as a certification loads envelope.
//!
//! # Sources (each read on its page)
//!
//! - EASA, *Easy Access Rules for Large Aeroplanes (CS-25)*, Amendment 27
//!   (Jan 2023): CS 25.341(a)(2)-(6), PDF pp. 218-219 (`U_ds = U_ref F_g
//!   (H/107)^(1/6)`; `U_ref` 17.07 m/s EAS at sea level, 13.41 m/s at
//!   4 572 m, 6.36 m/s at 18 288 m, linear between, half of it at `V_D`;
//!   `F_g = 0.5 (F_gz + F_gm)` at sea level, rising linearly to 1.0 at the
//!   maximum operating altitude; `F_gz = 1 - Z_mo/76 200`, `F_gm =
//!   sqrt(R2 tan(pi R1 / 4))`, `R1 = MLW/MTOW`, `R2 = MZFW/MTOW`; gradients
//!   9-107 m); CS 25.335(b), PDF p. 212 (`V_C/M_C <= 0.8 V_D/M_D`); AMC to
//!   CS 25.335, PDF p. 217 (a margin of 0.07 Mach between `M_C` and `M_D`);
//!   CS 25.303, PDF p. 201 (factor of safety 1.5).
//! - E. Torenbeek, *Advanced Aircraft Design* (2013), book pp. 325-326 (PDF
//!   pp. 341-342): Eq. 11.7 `n = 1 + K_g 0.5 rho_sl U_DE V_EAS S / W dCL/dalpha`,
//!   Eq. 11.8 `K_g = 0.88 mu / (5.3 + mu)`, `mu = 2 W b / (rho g S^2) /
//!   (dCL/dalpha)`, Eq. 11.10 (DATCOM) `dCL/dalpha = 2 pi A / ({(tan^2
//!   Lambda_0.5 + 1 - M^2) A^2 + 4}^(1/2) + 2)`; the equation is a
//!   quasi-static initial-analysis method that Torenbeek flags as less
//!   complete than the method of large aircraft companies.
//!
//! # Assumptions, stated rather than hidden
//!
//! - The Pratt formula is calibrated for a gust of 25 mean geometric chords
//!   (gradient `H = 12.5 c`), so the design velocity is evaluated there
//!   (clamped to the 9-107 m range of CS 25.341(a)(3)) instead of searching
//!   gradients.
//! - `R1` and `R2` of the flight profile alleviation factor are not inputs of
//!   the structural sizing; unity makes `F_gm = 1`, the upper bound of the
//!   rule, so the design gust is never under-stated by their absence.
//! - The maximum operating altitude `Z_mo` is the design cruise altitude, and
//!   `M_C` is the design cruise Mach number (a lower bound of the selected
//!   `M_C`).
//! - `dCL/dalpha` is that of the exposed planform at the case Mach number; the
//!   complete aeroplane's is lower (Torenbeek, p. 326), so the load is
//!   slightly conservative on that count and unconservative at the force break
//!   where Eq. 11.10 under-states the gradient by 5-10 %.

use alas_atmo::Atmosphere;
use alas_geom::wing_structure::WingStructureGeometry;

/// ISA sea-level density that defines equivalent airspeed, kg/m^3.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// CS 25.341(a)(5)(i) reference gust velocity breakpoints, `(altitude m, m/s
/// EAS)`: PDF p. 219.
const REFERENCE_GUST_BREAKPOINTS: [(f64, f64); 3] =
    [(0.0, 17.07), (4_572.0, 13.41), (18_288.0, 6.36)];

/// CS 25.341(a)(3) gust gradient range, m.
const GRADIENT_RANGE_M: (f64, f64) = (9.0, 107.0);

/// Gust gradient in mean geometric chords for the Pratt formula (a gust of 25
/// chords from zero to zero velocity).
const PRATT_GRADIENT_CHORDS: f64 = 12.5;

/// Mach margin between `M_C` and `M_D` of the AMC to CS 25.335, PDF p. 217.
pub const MACH_MARGIN_C_TO_D: f64 = 0.07;

/// Altitude stations of the critical-altitude search.
const ALTITUDE_STATIONS: usize = 61;

/// Mach number above which the subsonic lift-curve slope is not evaluated.
const MACH_CEILING: f64 = 0.95;

/// The planform quantities the gust load factor reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GustWing {
    /// Planform area of both semi-wings, m^2.
    pub area_m2: f64,
    /// Span, m.
    pub span_m: f64,
    /// Tangent of the half-chord sweep angle, `Lambda_0.5`.
    pub tan_half_chord_sweep: f64,
}

impl GustWing {
    /// The planform of the structural geometry (box-end tip, as the sizing).
    pub fn from_geometry(wsg: &WingStructureGeometry) -> Self {
        const STATIONS: usize = 200;
        let semi_span = wsg.semi_span;
        let chord = |i: usize| wsg.local_chord(i as f64 / (STATIONS - 1) as f64);
        let mut half = 0.0;
        for i in 0..STATIONS - 1 {
            half += 0.5 * (chord(i) + chord(i + 1)) * semi_span / (STATIONS - 1) as f64;
        }
        let half_chord_x = |eta: f64| wsg.x_le(eta) + 0.5 * wsg.local_chord(eta);
        Self {
            area_m2: 2.0 * half,
            span_m: 2.0 * semi_span,
            tan_half_chord_sweep: (half_chord_x(1.0) - half_chord_x(0.0)) / semi_span.max(1.0e-9),
        }
    }

    /// Aspect ratio `b^2 / S`.
    pub fn aspect_ratio(&self) -> f64 {
        self.span_m * self.span_m / self.area_m2
    }

    /// Mean geometric chord `S / b`, m.
    pub fn mean_chord_m(&self) -> f64 {
        self.area_m2 / self.span_m
    }
}

/// The speeds the gust is evaluated at, equivalent airspeed, m/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GustSpeeds {
    /// Design cruising speed `V_C`.
    pub cruise_eas_m_s: f64,
    /// Design dive speed `V_D`.
    pub dive_eas_m_s: f64,
    /// Design cruising Mach number `M_C`, which limits `V_C` at altitude.
    pub cruise_mach: f64,
}

/// One critical discrete-gust condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiscreteGust {
    /// `"gust-VC"` or `"gust-VD"`.
    pub name: &'static str,
    /// Altitude of the critical condition, m.
    pub altitude_m: f64,
    /// Equivalent airspeed, m/s.
    pub eas_m_s: f64,
    /// Mach number.
    pub mach: f64,
    /// Design gust velocity `U_ds`, m/s EAS.
    pub design_gust_velocity_m_s: f64,
    /// Wing lift-curve slope, 1/rad.
    pub lift_curve_slope_per_rad: f64,
    /// Aeroplane mass parameter `mu`.
    pub mass_ratio: f64,
    /// Gust alleviation factor `K_g`.
    pub alleviation_factor: f64,
    /// Load factor increment `Delta n` over 1 g.
    pub load_factor_increment: f64,
}

impl DiscreteGust {
    /// Limit load factor `1 + Delta n`.
    pub fn limit_load_factor(&self) -> f64 {
        1.0 + self.load_factor_increment
    }
}

/// CS 25.341(a)(5)(i) reference gust velocity, m/s EAS, at an altitude in
/// metres (clamped to the table), at `V_B` to `V_C`.
pub fn reference_gust_velocity_m_s(altitude_m: f64) -> f64 {
    let table = REFERENCE_GUST_BREAKPOINTS;
    let h = altitude_m.max(table[0].0);
    for pair in table.windows(2) {
        let ((h0, u0), (h1, u1)) = (pair[0], pair[1]);
        if h <= h1 {
            return u0 + (u1 - u0) * (h - h0) / (h1 - h0);
        }
    }
    table[table.len() - 1].1
}

/// CS 25.341(a)(6) flight profile alleviation factor at `altitude_m`, for a
/// maximum operating altitude `max_operating_altitude_m`, and the mass ratios
/// `r1 = MLW/MTOW`, `r2 = MZFW/MTOW`.
pub fn flight_profile_alleviation_factor(
    altitude_m: f64,
    max_operating_altitude_m: f64,
    r1: f64,
    r2: f64,
) -> f64 {
    let f_gz = 1.0 - max_operating_altitude_m / 76_200.0;
    let f_gm = (r2 * (std::f64::consts::PI * r1 / 4.0).tan()).sqrt();
    let sea_level = 0.5 * (f_gz + f_gm);
    if max_operating_altitude_m <= 0.0 {
        return 1.0;
    }
    let fraction = (altitude_m / max_operating_altitude_m).clamp(0.0, 1.0);
    sea_level + (1.0 - sea_level) * fraction
}

/// DATCOM lift-curve slope of a straight-tapered wing, 1/rad (Torenbeek 2013,
/// Eq. 11.10), `Mach < 1`.
pub fn lift_curve_slope_per_rad(aspect_ratio: f64, tan_half_chord_sweep: f64, mach: f64) -> f64 {
    let root =
        ((tan_half_chord_sweep.powi(2) + 1.0 - mach * mach) * aspect_ratio.powi(2) + 4.0).sqrt();
    2.0 * std::f64::consts::PI * aspect_ratio / (root + 2.0)
}

/// One altitude's gust condition at a given equivalent airspeed.
#[allow(clippy::too_many_arguments)] // every argument is a distinct physical input of Eq. 11.7-11.8
fn condition(
    name: &'static str,
    wing: &GustWing,
    mass_kg: f64,
    gravity_m_s2: f64,
    altitude_m: f64,
    eas_m_s: f64,
    reference_factor: f64,
    max_operating_altitude_m: f64,
) -> Option<DiscreteGust> {
    let atmosphere = Atmosphere::new(altitude_m);
    let density = atmosphere.density();
    let sigma = density / SEA_LEVEL_DENSITY_KG_M3;
    let mach = eas_m_s / sigma.sqrt() / atmosphere.speed_of_sound();
    if !(mach.is_finite() && mach > 0.0 && mach < MACH_CEILING) {
        return None;
    }
    let gradient =
        (PRATT_GRADIENT_CHORDS * wing.mean_chord_m()).clamp(GRADIENT_RANGE_M.0, GRADIENT_RANGE_M.1);
    let design_velocity = reference_factor
        * reference_gust_velocity_m_s(altitude_m)
        * flight_profile_alleviation_factor(altitude_m, max_operating_altitude_m, 1.0, 1.0)
        * (gradient / 107.0).powf(1.0 / 6.0);
    let slope = lift_curve_slope_per_rad(wing.aspect_ratio(), wing.tan_half_chord_sweep, mach);
    let mass_ratio = 2.0 * mass_kg / (density * wing.area_m2 * wing.mean_chord_m() * slope);
    let alleviation = 0.88 * mass_ratio / (5.3 + mass_ratio);
    let increment = alleviation
        * 0.5
        * SEA_LEVEL_DENSITY_KG_M3
        * design_velocity
        * eas_m_s
        * slope
        * wing.area_m2
        / (mass_kg * gravity_m_s2);
    (increment.is_finite() && increment >= 0.0).then_some(DiscreteGust {
        name,
        altitude_m,
        eas_m_s,
        mach,
        design_gust_velocity_m_s: design_velocity,
        lift_curve_slope_per_rad: slope,
        mass_ratio,
        alleviation_factor: alleviation,
        load_factor_increment: increment,
    })
}

/// The critical positive discrete gust at `V_C` and at `V_D`: the altitude, from
/// sea level to `max_operating_altitude_m`, with the largest load factor
/// increment, the equivalent airspeed being limited by `M_C` (`M_D = M_C +
/// 0.07`) where the Mach limit is below the speed.
///
/// `mass_kg` is the design gross mass, `gravity_m_s2` the gravitational
/// acceleration. `None` when an input is not finite and positive.
///
/// The result is a pure function of its inputs and the relieved-load fixed
/// point of the sizing asks for it on every pass, so the latest answer is kept
/// per thread and returned when every input matches bit for bit.
pub fn critical_discrete_gusts(
    wing: &GustWing,
    mass_kg: f64,
    gravity_m_s2: f64,
    speeds: &GustSpeeds,
    max_operating_altitude_m: f64,
) -> Option<[DiscreteGust; 2]> {
    type Key = [u64; 9];
    thread_local! {
        static LATEST: std::cell::RefCell<Option<(Key, Option<[DiscreteGust; 2]>)>> =
            const { std::cell::RefCell::new(None) };
    }
    let key: Key = [
        wing.area_m2.to_bits(),
        wing.span_m.to_bits(),
        wing.tan_half_chord_sweep.to_bits(),
        mass_kg.to_bits(),
        gravity_m_s2.to_bits(),
        speeds.cruise_eas_m_s.to_bits(),
        speeds.dive_eas_m_s.to_bits(),
        speeds.cruise_mach.to_bits(),
        max_operating_altitude_m.to_bits(),
    ];
    if let Some(hit) = LATEST.with(|latest| {
        latest
            .borrow()
            .as_ref()
            .filter(|(held, _)| *held == key)
            .map(|(_, value)| *value)
    }) {
        return hit;
    }
    let value = compute_critical_discrete_gusts(
        wing,
        mass_kg,
        gravity_m_s2,
        speeds,
        max_operating_altitude_m,
    );
    LATEST.with(|latest| *latest.borrow_mut() = Some((key, value)));
    value
}

fn compute_critical_discrete_gusts(
    wing: &GustWing,
    mass_kg: f64,
    gravity_m_s2: f64,
    speeds: &GustSpeeds,
    max_operating_altitude_m: f64,
) -> Option<[DiscreteGust; 2]> {
    let valid = [
        wing.area_m2,
        wing.span_m,
        mass_kg,
        gravity_m_s2,
        speeds.cruise_eas_m_s,
        speeds.dive_eas_m_s,
        speeds.cruise_mach,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0)
        && wing.tan_half_chord_sweep.is_finite()
        && max_operating_altitude_m.is_finite();
    if !valid {
        return None;
    }
    let ceiling = max_operating_altitude_m.max(0.0);
    let mut best: [Option<DiscreteGust>; 2] = [None, None];
    for i in 0..ALTITUDE_STATIONS {
        let altitude = ceiling * i as f64 / (ALTITUDE_STATIONS - 1) as f64;
        let atmosphere = Atmosphere::new(altitude);
        let sigma = atmosphere.density() / SEA_LEVEL_DENSITY_KG_M3;
        let eas_at_mach = |mach: f64| mach * atmosphere.speed_of_sound() * sigma.sqrt();
        let cruise = speeds.cruise_eas_m_s.min(eas_at_mach(speeds.cruise_mach));
        let dive = speeds
            .dive_eas_m_s
            .min(eas_at_mach(speeds.cruise_mach + MACH_MARGIN_C_TO_D));
        let candidates = [
            condition(
                "gust-VC",
                wing,
                mass_kg,
                gravity_m_s2,
                altitude,
                cruise,
                1.0,
                ceiling,
            ),
            // CS 25.341(a)(5)(ii): half the reference velocity at V_D.
            condition(
                "gust-VD",
                wing,
                mass_kg,
                gravity_m_s2,
                altitude,
                dive,
                0.5,
                ceiling,
            ),
        ];
        for (slot, candidate) in best.iter_mut().zip(candidates) {
            if let Some(candidate) = candidate {
                if slot
                    .is_none_or(|held| candidate.load_factor_increment > held.load_factor_increment)
                {
                    *slot = Some(candidate);
                }
            }
        }
    }
    Some([best[0]?, best[1]?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_gust_velocity_follows_cs_25_341_a_5_i() {
        assert!((reference_gust_velocity_m_s(0.0) - 17.07).abs() < 1e-12);
        assert!((reference_gust_velocity_m_s(4_572.0) - 13.41).abs() < 1e-12);
        assert!((reference_gust_velocity_m_s(18_288.0) - 6.36).abs() < 1e-12);
        // Midway between sea level and 4 572 m.
        assert!((reference_gust_velocity_m_s(2_286.0) - 15.24).abs() < 1e-12);
        // Above the table the value is held, below sea level it is clamped.
        assert!((reference_gust_velocity_m_s(30_000.0) - 6.36).abs() < 1e-12);
        assert!((reference_gust_velocity_m_s(-100.0) - 17.07).abs() < 1e-12);
    }

    #[test]
    fn the_flight_profile_alleviation_rises_to_unity_at_the_ceiling() {
        // R1 = R2 = 1: F_gm = sqrt(tan(pi/4)) = 1.
        let zmo = 12_000.0;
        let sea_level = flight_profile_alleviation_factor(0.0, zmo, 1.0, 1.0);
        assert!((sea_level - 0.5 * (1.0 - zmo / 76_200.0 + 1.0)).abs() < 1e-12);
        assert!((flight_profile_alleviation_factor(zmo, zmo, 1.0, 1.0) - 1.0).abs() < 1e-12);
        // Heavier landing and zero-fuel fractions raise F_gm and the gust.
        let light = flight_profile_alleviation_factor(0.0, zmo, 0.7, 0.7);
        let heavy = flight_profile_alleviation_factor(0.0, zmo, 0.9, 0.9);
        assert!(light < heavy && heavy < sea_level);
    }

    #[test]
    fn the_datcom_slope_reduces_to_the_prandtl_lifting_line_for_a_straight_incompressible_wing() {
        // Lambda = 0, M = 0: 2 pi A / (sqrt(A^2 + 4) + 2), the classic
        // elliptic-loading finite-wing slope.
        let a = 8.0_f64;
        let expected = 2.0 * std::f64::consts::PI * a / ((a * a + 4.0).sqrt() + 2.0);
        assert!((lift_curve_slope_per_rad(a, 0.0, 0.0) - expected).abs() < 1e-12);
        // Sweep and Mach move the slope in the stated directions.
        assert!(lift_curve_slope_per_rad(a, 0.5, 0.0) < expected);
        assert!(lift_curve_slope_per_rad(a, 0.0, 0.7) > expected);
    }

    fn a320_like() -> (GustWing, GustSpeeds) {
        (
            GustWing {
                area_m2: 122.4,
                span_m: 34.1,
                tan_half_chord_sweep: 0.40,
            },
            GustSpeeds {
                cruise_eas_m_s: 144.0,
                dive_eas_m_s: 180.0,
                cruise_mach: 0.78,
            },
        )
    }

    #[test]
    fn a_jet_transport_at_gross_mass_is_manoeuvre_critical() {
        // Torenbeek (2013), p. 325: wing weight methods assume manoeuvre loads
        // are critical, which holds for the high wing loading of a jet
        // transport and fails for low wing loading. At 78 t on 122.4 m^2 the
        // limit gust load factor stays below the 2.5 manoeuvre limit.
        let (wing, speeds) = a320_like();
        let gusts = critical_discrete_gusts(&wing, 78_000.0, 9.80665, &speeds, 11_887.2).unwrap();
        assert_eq!(gusts[0].name, "gust-VC");
        assert_eq!(gusts[1].name, "gust-VD");
        assert!(gusts[0].limit_load_factor() > 1.3, "{gusts:?}");
        assert!(gusts[0].limit_load_factor() < 2.5, "{gusts:?}");
        // The gust at V_D has half the reference velocity at a higher speed.
        assert!(
            gusts[1].limit_load_factor() < gusts[0].limit_load_factor(),
            "{gusts:?}"
        );
    }

    #[test]
    fn a_lighter_wing_loading_raises_the_gust_load_factor() {
        let (wing, speeds) = a320_like();
        let heavy = critical_discrete_gusts(&wing, 78_000.0, 9.80665, &speeds, 11_887.2).unwrap();
        let light = critical_discrete_gusts(&wing, 45_000.0, 9.80665, &speeds, 11_887.2).unwrap();
        assert!(light[0].load_factor_increment > heavy[0].load_factor_increment);
        // The alleviation factor falls with the mass parameter.
        assert!(light[0].alleviation_factor < heavy[0].alleviation_factor);
    }

    #[test]
    fn the_equation_is_reproduced_at_a_hand_checked_point() {
        // Sea level, V_EAS = 144 m/s, W = 78 t, S = 122.4 m^2, A = 9.5.
        let (wing, _) = a320_like();
        let g = condition(
            "gust-VC", &wing, 78_000.0, 9.80665, 0.0, 144.0, 1.0, 12_000.0,
        )
        .unwrap();
        let rho = Atmosphere::new(0.0).density();
        let a = wing.aspect_ratio();
        let slope = lift_curve_slope_per_rad(a, 0.40, g.mach);
        assert!((g.lift_curve_slope_per_rad - slope).abs() < 1e-12);
        let mu = 2.0 * 78_000.0 / (rho * 122.4 * (122.4 / 34.1) * slope);
        assert!((g.mass_ratio - mu).abs() < 1e-9);
        let kg = 0.88 * mu / (5.3 + mu);
        let dn = kg * 0.5 * 1.225 * g.design_gust_velocity_m_s * 144.0 * slope * 122.4
            / (78_000.0 * 9.80665);
        assert!((g.load_factor_increment - dn).abs() < 1e-12);
    }

    #[test]
    fn invalid_inputs_decline_to_produce_a_gust() {
        let (wing, speeds) = a320_like();
        assert!(critical_discrete_gusts(&wing, f64::NAN, 9.80665, &speeds, 11_887.2).is_none());
        assert!(critical_discrete_gusts(&wing, 0.0, 9.80665, &speeds, 11_887.2).is_none());
        let bad = GustSpeeds {
            cruise_mach: 0.0,
            ..speeds
        };
        assert!(critical_discrete_gusts(&wing, 78_000.0, 9.80665, &bad, 11_887.2).is_none());
    }
}
