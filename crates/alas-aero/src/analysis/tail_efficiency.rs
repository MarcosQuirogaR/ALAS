// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product's horizontal-tail dynamic-pressure ratio `eta_h = q_h / q`.
//!
//! A stated `AnalysisConfig::tail_efficiency` wins; unset, the product uses
//! the empirical `EMPIRICAL_TAIL_EFFICIENCY` and says so
//! ([`resolve_tail_efficiency`]). [`physical_tail_efficiency`] computes the
//! ratio from the built airplane at its cruise state instead:
//!
//! 1. the wing wake of USAF DATCOM Sec. 4.4.1 / Roskam, *Airplane Design*
//!    Part VI, Eqs. 8.34-8.40 ([`AeroAnalysis::horizontal_tail_wake`]); and
//! 2. for a turboprop, the propeller-slipstream increment of Roskam Part VI
//!    Eq. 8.41 (p. 271),
//!    `eta = 1 + (S_slip / S_h) 2200 P_av / (q U_1 pi D_p^2)` in ft, hp and
//!    slug units, which is `1 + (S_slip / S_h) T / (q A_disc)` in SI: the
//!    momentum-theory rise of the slipstream's dynamic pressure at a thrust
//!    `T` over a disc of area `A_disc = pi D_p^2 / 4`.
//!
//! The two terms are added. Roskam prints Eq. 8.41 as the propeller-airplane
//! alternative to the jet-airplane wake (Eqs. 8.36-8.40); a tail inside the
//! slipstream of a wing-mounted propeller is also inside the wing wake, so the
//! sum is this module's reading, not a printed one.
//!
//! Inputs chosen here, not by the source: the cruise state is the design
//! requirements' Mach and altitude at the maximum take-off weight
//! (`DesignRequirements::required_cruise_cl`); the propeller thrust is the
//! level-flight drag (`q S (CD0 + CL^2 / (pi A e))`, with the installed span
//! efficiency of [`installed_span_efficiency`]) shared equally by the engines,
//! so `P_av` is the thrust power `T U_1`; the slipstream is a straight
//! cylinder of the propeller diameter along the body x axis from the
//! nacelle's inlet station (no contraction, no bending by the downwash or the
//! attitude), and `S_slip` is the tail's plan area inside it.
//!
//! Limits, stated in [`TailEfficiency::provenance`]: no primary-source method
//! gives the dynamic-pressure loss of a low tail in the fuselage boundary
//! layer or in a nacelle wake (Roskam Part VI Sec. 8.1.5.2 lists the wing
//! friction loss and the propeller slipstream only; Torenbeek, Sadraey and
//! Raymer print the constants 0.85 to 0.95 for a low tail without a method),
//! so those losses are not modelled and no constant stands in for them.

use alas_atmo::Atmosphere;
use alas_config::analysis::EMPIRICAL_TAIL_EFFICIENCY;
use alas_config::engines::PropulsionTechnology;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::{Airplane, HORIZONTAL_STABILIZER_NAME};
use alas_geom::aircraft::wing::Wing;

use super::installation::installed_span_efficiency;
use super::tail_wake::TailWake;
use super::AeroAnalysis;

/// Samples along each tail panel of the slipstream-area integral.
const PANEL_SAMPLES: usize = 200;

/// Where a tail efficiency came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailEfficiencySource {
    /// `AnalysisConfig::tail_efficiency` was set explicitly.
    Stated,
    /// The wing wake (DATCOM 4.4.1, Roskam Eqs. 8.34-8.40), with the
    /// slipstream increment (Eq. 8.41) when a propeller wets the tail
    /// ([`physical_tail_efficiency`]; not the product default).
    Computed,
    /// The product default, [`EMPIRICAL_TAIL_EFFICIENCY`]: empirical, NASA
    /// CR-2144 and the TN D-3797 tail increment.
    Empirical,
    /// No tail, no wing, or a failed lattice solve: the free stream, `1.0`.
    FreeStream,
}

/// The tail's dynamic-pressure ratio and how it was obtained.
#[derive(Debug, Clone, PartialEq)]
pub struct TailEfficiency {
    /// `q_h / q`, dimensionless.
    pub ratio: f64,
    /// Where the ratio came from.
    pub source: TailEfficiencySource,
    /// The wing-wake result, when computed.
    pub wake: Option<TailWake>,
    /// Fraction of the tail's plan area inside a propeller slipstream.
    pub slipstream_area_fraction: f64,
    /// Eq. 8.41 increment over the submerged area, `S_slip / S_h * T / (q A)`.
    pub slipstream_increment: f64,
    /// Cruise Mach, altitude (m) and wing lift coefficient it was taken at.
    pub cruise: (f64, f64, f64),
    /// One sentence for a report: method, inputs, and what is not modelled.
    pub provenance: String,
}

impl TailEfficiency {
    /// Key under which a report's scalar summary carries [`Self::ratio`].
    pub const RATIO_KEY: &'static str = "tail_dynamic_pressure_ratio";
    /// Key of `1.0` when the ratio was stated, `0.0` when computed.
    pub const STATED_KEY: &'static str = "tail_eta_stated";
    /// Key of the wing-wake ratio of Roskam Eqs. 8.34-8.40.
    pub const WAKE_KEY: &'static str = "tail_eta_wing_wake";
    /// Key of the Eq. 8.41 slipstream increment.
    pub const SLIPSTREAM_KEY: &'static str = "tail_eta_slipstream_increment";

    /// The scalars a report's summary carries about this ratio.
    #[must_use]
    pub fn summary_entries(&self) -> Vec<(&'static str, f64)> {
        let mut entries = vec![
            (Self::RATIO_KEY, self.ratio),
            (
                Self::STATED_KEY,
                f64::from(u8::from(self.source == TailEfficiencySource::Stated)),
            ),
            (Self::SLIPSTREAM_KEY, self.slipstream_increment),
        ];
        if let Some(wake) = self.wake {
            entries.push((Self::WAKE_KEY, wake.ratio));
        }
        entries
    }
}

/// The product's tail dynamic-pressure ratio of `plane` under `config`: the
/// stated value, else the empirical [`EMPIRICAL_TAIL_EFFICIENCY`], labelled as
/// such (the free stream for an airplane without a horizontal stabilizer).
///
/// The computed wing-wake ratio ([`physical_tail_efficiency`]) is not the
/// default: it gives 1 for every preset and leaves the tail increment of the
/// NASA tunnel models overpredicted by 0.075 (TN D-3797) and 0.094 (RM
/// A58H12) of the mean chord, which no sourced fuselage, nacelle or tail-body
/// term closes.
#[must_use]
pub fn resolve_tail_efficiency(plane: &Airplane, config: &AlasConfig) -> TailEfficiency {
    let has_tail = plane
        .wings
        .iter()
        .any(|w| w.name == HORIZONTAL_STABILIZER_NAME);
    if config.analysis.tail_efficiency.is_some() || !has_tail || plane.s_ref <= 0.0 {
        return physical_tail_efficiency(plane, config);
    }
    let req = &config.requirements;
    TailEfficiency {
        ratio: EMPIRICAL_TAIL_EFFICIENCY,
        source: TailEfficiencySource::Empirical,
        wake: None,
        slipstream_area_fraction: 0.0,
        slipstream_increment: 0.0,
        cruise: (req.cruise_mach, req.cruise_altitude_m, 0.0),
        provenance: format!(
            "empirical default eta_t = {EMPIRICAL_TAIL_EFFICIENCY:.2}: NASA CR-2144 Boeing 747              flight derivatives (0.75-0.80) and the tail increment of NASA TN D-3797 (measured              0.275 of the mean chord against 0.345 for the lattice with a free-stream tail, a              ratio of 0.80); not a wake loss (the DATCOM 4.4.1 wing wake gives 1 for these              tails) and no sourced fuselage, nacelle or tail-body term closes the difference"
        ),
    }
}

/// The tail's dynamic-pressure ratio computed from the geometry: the stated
/// value, else the wing wake plus the slipstream increment. Not the product
/// default (see [`resolve_tail_efficiency`]); kept for study.
#[must_use]
pub fn physical_tail_efficiency(plane: &Airplane, config: &AlasConfig) -> TailEfficiency {
    let req = &config.requirements;
    let cruise = (req.cruise_mach, req.cruise_altitude_m, 0.0);
    if let Some(ratio) = config.analysis.tail_efficiency {
        return TailEfficiency {
            ratio,
            source: TailEfficiencySource::Stated,
            wake: None,
            slipstream_area_fraction: 0.0,
            slipstream_increment: 0.0,
            cruise,
            provenance: format!("stated in the configuration: eta_t = {ratio:.3}"),
        };
    }
    let free_stream = |why: &str| TailEfficiency {
        ratio: 1.0,
        source: TailEfficiencySource::FreeStream,
        wake: None,
        slipstream_area_fraction: 0.0,
        slipstream_increment: 0.0,
        cruise,
        provenance: format!("free stream, eta_t = 1: {why}"),
    };
    let Some(tail) = plane
        .wings
        .iter()
        .find(|w| w.name == HORIZONTAL_STABILIZER_NAME)
    else {
        return free_stream("the airplane has no horizontal stabilizer");
    };
    if plane.s_ref <= 0.0 {
        return free_stream("the airplane has no reference area");
    }
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let speed = req.cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * speed * speed;
    let cl = req.required_cruise_cl(q, plane.s_ref);
    let aero = AeroAnalysis::new(
        plane,
        AeroAnalysis::quarter_chord_sweep_deg(plane, 25.0),
        Some(config.geometry.clone()),
        Some(config.resolved_drag_model()),
        Some(config.analysis.clone()),
    );
    let Some(wake) = aero.horizontal_tail_wake(req.cruise_mach, req.cruise_altitude_m, cl) else {
        return free_stream("the wing-wake model could not be evaluated");
    };

    let (fraction, increment) = slipstream(plane, config, tail, &aero, q, cl);
    let ratio = wake.ratio + increment;
    let mut provenance = format!(
        "wing wake, USAF DATCOM 4.4.1 / Roskam Part VI Eqs. 8.34-8.40, at Mach {:.2}, {:.0} m, \
         CL {:.2}: wake ratio {:.3} (tail {:.2} m above the wake centre line, half-width {:.2} m)",
        req.cruise_mach,
        req.cruise_altitude_m,
        cl,
        wake.ratio,
        wake.height_above_wake_m,
        wake.half_width_m
    );
    if config.geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop {
        provenance.push_str(&format!(
            "; propeller slipstream, Roskam Part VI Eq. 8.41: {:.1} % of the tail area submerged, \
             increment {:+.3}",
            100.0 * fraction,
            increment
        ));
    }
    provenance.push_str(
        "; fuselage boundary-layer and nacelle-wake losses of a low tail are not modelled (no \
         primary-source method; Torenbeek, Sadraey and Raymer print only the constants 0.85-0.95)",
    );
    TailEfficiency {
        ratio,
        source: TailEfficiencySource::Computed,
        wake: Some(wake),
        slipstream_area_fraction: fraction,
        slipstream_increment: increment,
        cruise: (req.cruise_mach, req.cruise_altitude_m, cl),
        provenance,
    }
}

/// [`resolve_tail_efficiency`], then give the horizontal stabilizer of `plane`
/// the resulting ratio.
pub fn apply_tail_efficiency(plane: &mut Airplane, config: &AlasConfig) -> TailEfficiency {
    let resolved = resolve_tail_efficiency(plane, config);
    plane.set_horizontal_tail_dynamic_pressure_ratio(resolved.ratio);
    resolved
}

/// `(S_slip / S_h, S_slip / S_h * T / (q A))` of Eq. 8.41, zero for anything
/// but a turboprop with a declared propeller.
fn slipstream(
    plane: &Airplane,
    config: &AlasConfig,
    tail: &Wing,
    aero: &AeroAnalysis<'_>,
    q_pa: f64,
    cl: f64,
) -> (f64, f64) {
    let engine = &config.geometry.engine;
    let Some(spec) = engine
        .turboprop
        .as_ref()
        .filter(|_| engine.propulsion_technology == PropulsionTechnology::Turboprop)
    else {
        return (0.0, 0.0);
    };
    let diameter = spec.propeller_diameter_m;
    let discs: Vec<[f64; 2]> = plane
        .fuselages
        .iter()
        .filter(|f| f.name.starts_with("Nacelle"))
        .filter_map(|f| f.xsecs.first().map(|x| [x.xyz_c[1], x.xyz_c[2]]))
        .collect();
    if discs.is_empty() || diameter.is_nan() || diameter <= 0.0 {
        return (0.0, 0.0);
    }
    let fraction = submerged_fraction(tail, &discs, 0.5 * diameter);
    if fraction <= 0.0 {
        return (0.0, 0.0);
    }
    // Level-flight thrust: drag over the engines.
    let Some(main) = plane.wings.first() else {
        return (0.0, 0.0);
    };
    let e = installed_span_efficiency(&config.geometry, !plane.fuselages.is_empty()).unwrap_or(1.0);
    let cd0 = aero
        .parasite_breakdown(
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            None,
            None,
        )
        .cd0;
    let cd = cd0 + cl * cl / (std::f64::consts::PI * main.aspect_ratio().max(1e-6) * e);
    let thrust_per_engine = q_pa * plane.s_ref * cd / discs.len() as f64;
    let disc_area = std::f64::consts::PI * diameter * diameter / 4.0;
    let increment = fraction * thrust_per_engine / (q_pa * disc_area);
    (fraction, increment)
}

/// Fraction of the tail's plan area whose `(y, z)` lies within `radius` of
/// any disc centre in `discs`, over both halves of a symmetric tail.
fn submerged_fraction(tail: &Wing, discs: &[[f64; 2]], radius: f64) -> f64 {
    let mut total = 0.0;
    let mut wet = 0.0;
    let halves: &[f64] = if tail.symmetric { &[1.0, -1.0] } else { &[1.0] };
    for pair in tail.xsecs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let dy = (b.xyz_le[1] - a.xyz_le[1]).abs() / PANEL_SAMPLES as f64;
        for k in 0..PANEL_SAMPLES {
            let t = (k as f64 + 0.5) / PANEL_SAMPLES as f64;
            let chord = a.chord + t * (b.chord - a.chord);
            let y = a.xyz_le[1] + t * (b.xyz_le[1] - a.xyz_le[1]);
            let z = a.xyz_le[2] + t * (b.xyz_le[2] - a.xyz_le[2]);
            for sign in halves {
                let area = chord * dy;
                total += area;
                let ys = sign * y;
                if discs.iter().any(|d| (ys - d[0]).hypot(z - d[1]) < radius) {
                    wet += area;
                }
            }
        }
    }
    if total > 0.0 {
        wet / total
    } else {
        0.0
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    fn build(preset: &str) -> (Airplane, AlasConfig) {
        let entry = alas_config::presets::get(preset).expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let config =
            AlasConfig::from_value(&serde_json::json!({ "preset": preset })).expect("config");
        (plane, config)
    }

    #[test]
    fn the_unset_product_default_is_the_labelled_empirical_value() {
        let (plane, config) = build("A320-200");
        let eta = resolve_tail_efficiency(&plane, &config);
        assert_eq!(eta.ratio, EMPIRICAL_TAIL_EFFICIENCY);
        assert_eq!(eta.source, TailEfficiencySource::Empirical);
        assert!(eta.provenance.contains("empirical"));
        assert!(eta.provenance.contains("TN D-3797"));
    }

    #[test]
    fn a_stated_efficiency_wins_and_is_reported_as_stated() {
        let (plane, mut config) = build("A320-200");
        config.analysis.tail_efficiency = Some(0.9);
        let eta = resolve_tail_efficiency(&plane, &config);
        assert_eq!(eta.ratio, 0.9);
        assert_eq!(eta.source, TailEfficiencySource::Stated);
    }

    #[test]
    fn the_unset_default_is_the_wake_ratio_for_a_jet() {
        let (plane, config) = build("A320-200");
        let eta = physical_tail_efficiency(&plane, &config);
        assert_eq!(eta.source, TailEfficiencySource::Computed);
        let wake = eta.wake.expect("wake");
        assert_eq!(eta.slipstream_increment, 0.0);
        assert_eq!(eta.ratio, wake.ratio);
        assert!(eta.provenance.contains("not modelled"));
    }

    #[test]
    fn the_slipstream_area_integral_counts_the_tail_inside_a_disc() {
        let (plane, _) = build("A320-200");
        let tail = plane
            .wings
            .iter()
            .find(|w| w.name == HORIZONTAL_STABILIZER_NAME)
            .expect("tail");
        let z = tail.xsecs[0].xyz_le[2];
        assert!((submerged_fraction(tail, &[[0.0, z]], 1e6) - 1.0).abs() < 1e-12);
        assert_eq!(submerged_fraction(tail, &[[0.0, z + 1e3]], 1.0), 0.0);
    }

    #[test]
    fn a_turboprop_increment_is_thrust_over_q_times_disc_area_times_the_submerged_share() {
        let (plane, config) = build("ATR72-600");
        let eta = physical_tail_efficiency(&plane, &config);
        assert!(eta.slipstream_increment >= 0.0);
        assert!(eta.slipstream_area_fraction >= 0.0 && eta.slipstream_area_fraction <= 1.0);
        assert!(
            (eta.ratio - eta.wake.expect("wake").ratio - eta.slipstream_increment).abs() < 1e-12
        );
    }
}
