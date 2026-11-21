// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The rated thrust per engine a clean-sheet jet brief starts from: the
//! CS 25.121(b) second segment, flown at `V2` on the `n - 1` remaining
//! engines with the gear up and take-off flaps, on the brief's own drag
//! estimate.
//!
//! The climb balance per remaining engine is
//! `T = W (gamma + CD / CL) / (n - 1)` at `CL = CLmax_TO / (V2 / VSR)^2`, with
//! `gamma` the CS 25.121(b)(1) gradient of the engine count (2.4, 2.7 and
//! 3.0 % for two, three and four engines; CS 25.121 states no single-engine
//! case, so one engine takes the twin's 2.4 % on itself, and more than four
//! take the four-engine 3.0 %, the highest value it states) and
//! `CD = CD0 + dCD_flaps + CL^2 / (pi A e)`. The sea-level-static rating is
//! that thrust over the take-off lapse at the `V2` Mach number.
//!
//! - `CD0 = Cfe Swet / Sref`, Raymer, *Aircraft Design: A Conceptual
//!   Approach*, 6th ed., Eq. (12.23), p. 416, with the civil-transport
//!   equivalent skin-friction coefficient 0.0026 of Table 12.3, p. 417 (the
//!   table has no business-jet row; the civil transport is its jet row).
//! - Wing and tail wetted areas: Raymer 6th ed. Eqs. (7.11)-(7.12), p. 204,
//!   on the exposed planform. The wing is exposed outside the fuselage width
//!   at its centreline root chord; the tails are counted whole. The section
//!   thickness is the one the Korn relation admits at the brief's cruise
//!   Mach number, its class sweep, its cruise lift coefficient and its
//!   section class (`t/c = k cos L - M cos^2 L - CL / (10 cos L)`).
//! - Fuselage: Torenbeek, *Synthesis of Subsonic Airplane Design* (1982),
//!   Eq. (B-6), p. 447, `pi D l (1 - 2/lambda)^(2/3) (1 + 1/lambda^2)`, valid
//!   for a slenderness `lambda = l / D` of at least 4.5, on the mean of the
//!   section's width and height and the estimated cabin length.
//! - Nacelles: a circular long thin body wets about `pi` times its average
//!   projected area (Raymer 6th ed., p. 205): `pi D L` on the catalogue
//!   engine's maximum nacelle diameter and length, so the drag follows the
//!   engine being sized.
//! - Oswald factor 0.70, the lower end of the range Raymer 6th ed. p. 444
//!   prints as typical ("between 0.7 and 0.85"): the wing flies with take-off
//!   flaps. Raymer's Eq. (12.49) for swept wings gives about 0.5 on a
//!   transport wing of aspect ratio 9 and is not used.
//! - Lapse: Torenbeek (1982) Eq. (4-33), p. 128,
//!   `T / T_to = 1 - .45 M (1 + B) / sqrt((1 + .75 B) G) + (.6 + .11 B / G) M^2`
//!   at the engine's bypass ratio `B`, with the gas-generator function `G`
//!   at the middle of the printed take-off range ".9 to 1.2" (the ends move
//!   the lapse by about 1.3 percent at `V2`).
//! - `V2` at the configured speed ratio (`PerformanceConfig::takeoff_speed_ratios`),
//!   ISA sea level, MTOW.
//!
//! Windmilling and asymmetric trim drag stay in the run's engine-out check,
//! which also flies the departure aerodrome and the engine deck; this is
//! the start the sizing then scales, and the run reports a brief that no
//! catalogue engine fits.

use serde_json::Value;

use super::geometry::{derive, estimated_fuselage_length_m};
use crate::engines::EngineSpec;
use crate::AlasConfig;

/// Raymer, 6th ed., Table 12.3, p. 417: civil transport `Cfe`.
const CIVIL_TRANSPORT_CFE: f64 = 0.0026;
/// Raymer, 6th ed., p. 444: the low end of the typical Oswald factor.
const TAKEOFF_OSWALD_FACTOR: f64 = 0.70;
/// Torenbeek (1982), p. 128: the middle of the printed take-off range of `G`.
const TAKEOFF_GAS_GENERATOR_FUNCTION: f64 = 1.05;
/// Torenbeek (1982), Eq. (B-6): the least slenderness it holds for.
const MIN_FUSELAGE_SLENDERNESS: f64 = 4.5;

/// The brief's take-off climb, everything but the engine being sized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SecondSegment {
    /// Engines installed.
    engines: usize,
    /// MTOW weight, N.
    weight_n: f64,
    /// Wing reference area, m^2.
    reference_area_m2: f64,
    /// Wetted area of the airframe without nacelles, m^2.
    airframe_wetted_m2: f64,
    /// Lift coefficient at `V2`.
    lift_coefficient: f64,
    /// Drag coefficient at `V2` without the nacelles' zero-lift drag.
    airframe_drag_coefficient: f64,
    /// CS 25.121(b) gradient.
    gradient: f64,
    /// `V2` Mach number.
    mach: f64,
}

impl SecondSegment {
    /// The second segment of `brief` (`data` is the file it was read from, so
    /// a key the file sets keeps its value in the derived geometry); `None`
    /// when the brief cannot be sized or its fuselage is outside Eq. (B-6).
    pub(super) fn of(brief: &AlasConfig, data: &Value) -> Option<Self> {
        let engines = brief.geometry.engine.spanwise_positions_m.len();
        let r = &brief.requirements;
        let weight_n = r.mtow_kg * r.gravity_m_s2;
        let mut config = brief.clone();
        config.geometry = derive(brief, data)?;
        let class = config.class_wing()?;
        let start = config.chords_for_area(class)?;
        let stats = super::fleet_statistics()?;
        let f = &config.geometry.fuselage;
        let width_m = f.diameter_m;
        let diameter_m = 0.5 * (width_m + f.effective_height_m());
        let length_m = estimated_fuselage_length_m(&config)?;

        let sweep = class.sweep_deg.to_radians().cos();
        let thickness = config.geometry.wing.korn_technology_factor() * sweep
            - r.cruise_mach * sweep * sweep
            - stats.cruise_lift_coefficient_at_mtow / (10.0 * sweep);
        let wetted = |exposed_m2: f64| {
            if thickness > 0.05 {
                exposed_m2 * (1.977 + 0.52 * thickness)
            } else {
                2.003 * exposed_m2
            }
        };
        let wing_m2 = wetted(class.area_m2 - start.root_chord_m * width_m);
        let e = &config.geometry.empennage;
        let ts = start.tail_scale;
        let hstab_m2 = (e.hstab_root_chord_m + e.hstab_tip_chord_m) * e.hstab_tip_le_m.1 * ts * ts;
        let fin = ts * e.vstab_scale_ratio;
        let fin_m2 =
            0.5 * (e.vstab_root_chord_m + e.vstab_tip_chord_m) * e.vstab_tip_le_m.2 * fin * fin;
        let slenderness = length_m / diameter_m;
        if slenderness < MIN_FUSELAGE_SLENDERNESS {
            return None;
        }
        let fuselage_m2 = std::f64::consts::PI
            * diameter_m
            * length_m
            * (1.0 - 2.0 / slenderness).powf(2.0 / 3.0)
            * (1.0 + 1.0 / (slenderness * slenderness));
        let airframe_wetted_m2 =
            wing_m2 + wetted(hstab_m2.abs()) + wetted(fin_m2.abs()) + fuselage_m2;

        let p = &brief.performance;
        let lift_coefficient = p.cl_max_to / p.takeoff_speed_ratios().v2.powi(2);
        let aspect_ratio = class.span_m * class.span_m / class.area_m2;
        let induced = lift_coefficient * lift_coefficient
            / (std::f64::consts::PI * aspect_ratio * TAKEOFF_OSWALD_FACTOR);
        let airframe_drag_coefficient = CIVIL_TRANSPORT_CFE * airframe_wetted_m2 / class.area_m2
            + p.oei_climb_delta_cd
            + induced;
        let gradient = second_segment_gradient(engines);
        let sea_level = alas_atmo::Atmosphere::try_new(0.0).ok()?;
        let v2_m_s =
            (2.0 * weight_n / (sea_level.density() * class.area_m2 * lift_coefficient)).sqrt();
        let segment = Self {
            engines,
            weight_n,
            reference_area_m2: class.area_m2,
            airframe_wetted_m2,
            lift_coefficient,
            airframe_drag_coefficient,
            gradient,
            mach: v2_m_s / sea_level.speed_of_sound(),
        };
        segment.is_finite().then_some(segment)
    }

    fn is_finite(&self) -> bool {
        [
            self.weight_n,
            self.reference_area_m2,
            self.airframe_wetted_m2,
            self.lift_coefficient,
            self.airframe_drag_coefficient,
            self.gradient,
            self.mach,
        ]
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
            && self.engines > 0
    }

    /// The sea-level-static rated thrust per engine, N, that `engine`'s
    /// nacelles and lapse ask for. A single-engine brief has no engine-out
    /// case; it is sized on its one engine at the twin's gradient.
    pub(super) fn rated_thrust_per_engine_n(&self, engine: &EngineSpec) -> Option<f64> {
        let nacelle_m2 = std::f64::consts::PI
            * 2.0
            * engine.nacelle_max_radius_m
            * engine.nacelle_length_m
            * self.engines as f64;
        let drag = self.airframe_drag_coefficient
            + CIVIL_TRANSPORT_CFE * nacelle_m2 / self.reference_area_m2;
        let remaining = self.engines.saturating_sub(1).max(1) as f64;
        let thrust_n = self.weight_n * (self.gradient + drag / self.lift_coefficient) / remaining;
        let thrust_n = thrust_n / takeoff_lapse(self.mach, engine.bypass_ratio)?;
        (thrust_n.is_finite() && thrust_n > 0.0).then_some(thrust_n)
    }
}

/// The CS 25.121(b)(1) second-segment gradient of `engines` engines: 2.4, 2.7
/// and 3.0 % for two, three and four; one engine takes the twin's value and
/// more than four the four-engine value (see the module documentation).
fn second_segment_gradient(engines: usize) -> f64 {
    match engines {
        0..=2 => 0.024,
        3 => 0.027,
        _ => 0.030,
    }
}

/// Torenbeek (1982) Eq. (4-33), p. 128: installed take-off thrust over the
/// static rating at Mach `mach` for bypass ratio `bypass_ratio`.
fn takeoff_lapse(mach: f64, bypass_ratio: f64) -> Option<f64> {
    let g = TAKEOFF_GAS_GENERATOR_FUNCTION;
    let b = bypass_ratio;
    let lapse = 1.0 - 0.45 * mach * (1.0 + b) / ((1.0 + 0.75 * b) * g).sqrt()
        + (0.6 + 0.11 * b / g) * mach * mach;
    (b.is_finite() && b >= 0.0 && lapse.is_finite() && lapse > 0.0).then_some(lapse)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{second_segment_gradient, takeoff_lapse};

    /// CS 25.121(b)(1) by engine count, with the documented ends.
    #[test]
    fn the_second_segment_gradient_follows_cs_25_121_by_engine_count() {
        let gradients: Vec<f64> = (1..=6).map(second_segment_gradient).collect();
        assert_eq!(gradients, vec![0.024, 0.024, 0.027, 0.030, 0.030, 0.030]);
    }

    /// Eq. (4-33) is one at rest and falls with Mach number faster at a
    /// higher bypass ratio.
    #[test]
    fn the_lapse_is_one_static_and_steeper_at_high_bypass() {
        assert_eq!(takeoff_lapse(0.0, 5.0), Some(1.0));
        let low = takeoff_lapse(0.23, 1.0).unwrap();
        let high = takeoff_lapse(0.23, 10.0).unwrap();
        assert!(high < low && low < 1.0, "{low} {high}");
        // B = 5, M = 0.235, G = 1.05: 1 - .2841 + .0621.
        let five = takeoff_lapse(0.235, 5.0).unwrap();
        assert!((five - 0.778).abs() < 0.001, "{five}");
    }
}
