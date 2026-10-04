// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The start point, search box and dependent geometry of an aircraft that
//! names no registered preset.
//!
//! The shipped defaults describe one coherent aircraft, the AVE reference
//! twin (358.7 t, 350 seats, M 0.84, a 6.2 m body). A brief that keeps those
//! inputs keeps that aircraft. A brief that changes them (a 70 t, 168-seat,
//! 3.96 m-body transport, say) cannot start from a 535 m^2, 71.75 m wing in a
//! 60-80 m span box: the smallest wing in that box already fails the wing
//! loading floor, the empty mass is that of a widebody and the mass never
//! closes. Such a brief is therefore sized here, from class relations only:
//!
//! - wing area from the cruise lift coefficient at MTOW,
//!   `S = m g / (q CL)`, with `q = (gamma/2) p(h) M^2` (ISA) and `CL` the
//!   fleet median ([`FleetStatistics`]);
//! - span from the fleet-median aspect ratio, `b = sqrt(A S)`, held under the
//!   aerodrome reference code's span limit;
//! - inboard leading-edge sweep from simple sweep theory at the fleet-median
//!   normal Mach number, `cos L = M_n / M` (unswept when `M <= M_n`);
//! - chords at the fleet-median taper ratios, scaled to close `S` exactly on
//!   the built planform;
//! - the fuselage from the cabin (seats abreast under CS 25.817, rows at the
//!   configured economy pitch) and, in a cabin-sized clean sheet, re-solved
//!   from the detailed layout by the optimizer;
//! - the wing at the fleet-median quarter-MAC station along the fuselage.
//!
//! The geometry those relations depend on (nose, tail cone, wing heights,
//! side-of-body station, engine station and the empennage at constant tail
//! volume coefficients) is derived at load time from the same relations in
//! [`geometry`], before the file is overlaid, so every explicit key in the
//! file still wins.
//!
//! Two optional keys let a user state the start and the box directly:
//! `optimizer.design_space.initial_design` (name to value) and
//! `optimizer.design_space.bounds` (name to `[lower, upper]`). They apply to
//! any clean-sheet run, with or without a preset, and override the derived
//! values variable by variable. No constraint, tolerance or validity limit
//! is changed by any of this: the derivation only moves where the search
//! starts and which box it searches.

mod fleet;
mod geometry;
mod planform;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

pub use fleet::{fleet_statistics, FleetStatistics};
pub(crate) use geometry::seed_preset_less_defaults;
use planform::WingPlanformSummary;

use crate::design_variables::{DesignVariableSpec, DesignVector, SPECS};
use crate::{AerodromeReferenceCode, AlasConfig, DesignMode, VariableEnvelope};

/// Ratio of specific heats of air, dimensionless (`q = (gamma/2) p M^2`).
const HEAT_CAPACITY_RATIO: f64 = 1.4;

/// Search window on the cruise lift coefficient at MTOW, as factors on the
/// fleet median; the wing-area window is its reciprocal. The registered
/// turbofans span about +-12 % about the median; the window is wider because
/// a new brief's best wing loading is the unknown the search exists to find.
pub const LIFT_COEFFICIENT_WINDOW: (f64, f64) = (0.7, 1.3);

/// Search window on the aspect ratio, as factors on the fleet median. It
/// contains every registered turbofan (DC-10 0.64, A220-300 1.14 of the
/// median) with margin. Engineering choice.
pub const ASPECT_RATIO_WINDOW: (f64, f64) = (0.6, 1.3);

/// Search window on the root and trailing-edge break chords, as factors on
/// the derived start. Engineering choice, wide enough to reach every wing
/// area in [`LIFT_COEFFICIENT_WINDOW`] at the start span.
pub const INBOARD_CHORD_WINDOW: (f64, f64) = (0.7, 1.45);

/// Search window on the tip chord, as factors on the derived start. The
/// upper factor is the global box's own (3.0 m over 1.6 m); the lower one
/// reaches the most tapered registered wing (AVE, 0.48 of the median taper)
/// with margin.
pub const TIP_CHORD_WINDOW: (f64, f64) = (0.4, 1.9);

/// Half-width of the sweep window about the derived start, deg.
pub const SWEEP_HALF_WINDOW_DEG: f64 = 8.0;

/// A class span at or above this fraction of the aerodrome code's span limit
/// is near the band edge, and the derived span box then runs to the limit.
/// Engineering choice.
pub const SPAN_HEADROOM_FRACTION: f64 = 0.95;

/// The sizing inputs a clean-sheet start is derived from.
#[derive(Debug, Clone, PartialEq)]
struct Brief {
    mtow_kg: f64,
    num_passengers: i64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    aircraft_type: String,
    cargo_payload_kg: f64,
    fuselage_diameter_m: f64,
    fuselage_height_m: Option<f64>,
    aerodrome_code: AerodromeReferenceCode,
}

impl Brief {
    fn of(config: &AlasConfig) -> Self {
        let r = &config.requirements;
        Self {
            mtow_kg: r.mtow_kg,
            num_passengers: r.num_passengers,
            cruise_mach: r.cruise_mach,
            cruise_altitude_m: r.cruise_altitude_m,
            aircraft_type: r.aircraft_type.clone(),
            cargo_payload_kg: r.cargo_payload_kg,
            fuselage_diameter_m: config.geometry.fuselage.diameter_m,
            fuselage_height_m: config.geometry.fuselage.height_m,
            aerodrome_code: config.optimizer.objective.aerodrome_reference_code,
        }
    }

    fn shipped_default() -> &'static Self {
        static DEFAULT: OnceLock<Brief> = OnceLock::new();
        DEFAULT.get_or_init(|| Self::of(&AlasConfig::default()))
    }
}

/// The class-sized wing and the sweep a brief asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClassWing {
    /// Projected reference area, m^2.
    pub area_m2: f64,
    /// Projected span, m.
    pub span_m: f64,
    /// Inboard leading-edge sweep, deg.
    pub sweep_deg: f64,
}

/// Cruise dynamic pressure `(gamma/2) p M^2` in the ISA, Pa; `None` outside
/// the atmosphere model or for a non-positive Mach number.
pub(crate) fn cruise_dynamic_pressure_pa(mach: f64, altitude_m: f64) -> Option<f64> {
    if !(mach.is_finite() && mach > 0.0) {
        return None;
    }
    let pressure_pa = alas_atmo::Atmosphere::try_isa(altitude_m).ok()?.pressure();
    let q = 0.5 * HEAT_CAPACITY_RATIO * pressure_pa * mach * mach;
    (q.is_finite() && q > 0.0).then_some(q)
}

/// Inboard leading-edge sweep from simple sweep theory, deg: the sweep at
/// which the Mach number normal to the leading edge equals `normal_mach`,
/// capped at the design space's 45 deg guardrail.
pub(crate) fn sweep_for_mach_deg(mach: f64, normal_mach: f64) -> f64 {
    if mach <= normal_mach {
        return 0.0;
    }
    (normal_mach / mach)
        .acos()
        .to_degrees()
        .min(spec("sweep_deg").preset_upper)
}

fn spec(name: &str) -> &'static DesignVariableSpec {
    SPECS
        .iter()
        .find(|spec| spec.name == name)
        .unwrap_or(&SPECS[0])
}

impl AlasConfig {
    /// Whether this run's start, box, dependent geometry and cabin are
    /// derived from its brief: no preset is named, the design mode is clean
    /// sheet and the configuration is marked a clean-sheet brief
    /// (`optimizer.design_space.clean_sheet_brief`).
    ///
    /// Loading a file sets the mark when the file names no preset and its
    /// brief departs from the shipped reference aircraft's (see
    /// [`Self::brief_departs_from_reference`]). The mark, not the brief, is
    /// read afterwards, because the analysis itself rewrites brief fields (a
    /// named cabin style writes the passenger count it seats, an MTOW mode
    /// the takeoff mass it sizes) and the decision must not flip mid-run.
    pub fn derives_clean_sheet_start(&self) -> bool {
        self.preset.is_empty()
            && self.optimizer.design_space.mode == DesignMode::CleanSheet
            && self.optimizer.design_space.clean_sheet_brief == Some(true)
    }

    /// Apply a committed clean-sheet brief's derived values: resolve an
    /// `Auto` aerodrome code to the letter of the class span, then (when
    /// `with_geometry`) re-derive the brief-dependent geometry, keeping
    /// every field `explicit` names (the file being loaded, or an empty
    /// object for the desktop application). The same function serves the
    /// file and the desktop paths, so one brief gives one configuration.
    /// Does nothing unless [`Self::derives_clean_sheet_start`].
    pub fn apply_clean_sheet_commit(&mut self, explicit: &serde_json::Value, with_geometry: bool) {
        if !self.derives_clean_sheet_start() {
            return;
        }
        let objective = &self.optimizer.objective;
        self.optimizer.objective.derived_aerodrome_code = (objective.aerodrome_reference_code
            == AerodromeReferenceCode::Auto)
            .then(|| self.class_aerodrome_code());
        if with_geometry {
            if let Some(geometry) = geometry::derive(self, explicit) {
                self.geometry = geometry;
            }
        }
    }

    /// The aerodrome reference code whose span band holds the class wing this
    /// brief implies (ICAO Annex 14 Table 1-1), or
    /// [`AerodromeReferenceCode::Unrestricted`] when that span is beyond code F
    /// or the brief cannot be sized. A clean-sheet brief that states no code
    /// takes this one rather than defaulting to code F.
    pub fn class_aerodrome_code(&self) -> AerodromeReferenceCode {
        let mut unlimited = self.clone();
        unlimited.optimizer.objective.aerodrome_reference_code =
            AerodromeReferenceCode::Unrestricted;
        unlimited
            .class_wing()
            .and_then(|wing| AerodromeReferenceCode::for_span_m(wing.span_m))
            .unwrap_or(AerodromeReferenceCode::Unrestricted)
    }

    /// Whether the sizing brief (MTOW, passengers, cruise Mach and altitude,
    /// fuselage diameter and height, freight payload, aircraft type,
    /// aerodrome code) differs from the shipped reference aircraft's.
    pub fn brief_departs_from_reference(&self) -> bool {
        Brief::of(self) != *Brief::shipped_default()
    }

    /// The design vector a run starts from when its caller supplies none:
    /// the registered vector of a named preset, the class-sized start of a
    /// derived clean sheet, or the shipped reference vector, with any
    /// explicit `optimizer.design_space.initial_design` entries of a
    /// clean-sheet run applied on top.
    pub fn configured_nominal_design(&self) -> DesignVector {
        let mut design = if self.preset.is_empty() {
            self.clean_sheet_design().unwrap_or_default()
        } else {
            match crate::presets::get(&self.preset) {
                Ok(preset) => preset.design_vector,
                Err(error) => {
                    tracing::warn!(%error, "configured preset has no registered design vector");
                    DesignVector::default()
                }
            }
        };
        if self.optimizer.design_space.mode == DesignMode::CleanSheet {
            design = self.optimizer.design_space.apply_explicit_start(design);
        }
        design
    }

    /// The class-sized start of a derived clean sheet, before explicit
    /// entries; `None` when the run does not derive one or the brief is not
    /// physical (non-positive MTOW or Mach, altitude outside the ISA).
    pub fn clean_sheet_design(&self) -> Option<DesignVector> {
        if !self.derives_clean_sheet_start() {
            return None;
        }
        let wing = self.class_wing()?;
        let mut design = self.chords_for_area(wing)?;
        design.fuselage_length_m = self.estimated_fuselage_length_m()?;
        design.wing_x_shift_m = self.clean_sheet_wing_x_shift_m(&design)?;
        Some(design)
    }

    /// The wing shift that puts the quarter-MAC of `design` at the fleet
    /// median station along its own fuselage length, m.
    pub fn clean_sheet_wing_x_shift_m(&self, design: &DesignVector) -> Option<f64> {
        let stats = fleet_statistics()?;
        let wing = WingPlanformSummary::of(&self.geometry.wing, design)?;
        let target_m = stats.quarter_mac_station_per_length * design.fuselage_length_m;
        Some(target_m - self.geometry.wing.root_datum_x_m - wing.quarter_mac_x_m)
    }

    /// The fuselage-length interval a cabin-sized clean sheet solves the
    /// shortest seating length in, m: an explicit `bounds` entry, else for a
    /// derived clean sheet from just over the two tapers plus one diameter to
    /// the design space's guardrail, else the global box.
    pub fn fuselage_sizing_interval_m(&self) -> (f64, f64) {
        let spec = spec("fuselage_length_m");
        if self.optimizer.design_space.mode == DesignMode::CleanSheet {
            if let Some(&(lower, upper)) = self.optimizer.design_space.bounds.get(spec.name) {
                return (lower, upper);
            }
        }
        if self.derives_clean_sheet_start() {
            let f = &self.geometry.fuselage;
            let shortest_m = f.cabin_start_x_m + f.tailcone_length_m + f.diameter_m;
            if shortest_m.is_finite() && shortest_m < spec.preset_upper {
                return (shortest_m.max(spec.preset_lower), spec.preset_upper);
            }
        }
        (spec.lower, spec.upper)
    }

    /// Replace the non-fixed clean-sheet bounds of `envelope` with the
    /// derived class box (widened to contain the nominal) and then with any
    /// explicit `bounds` entries.
    pub(crate) fn apply_clean_sheet_bounds(&self, envelope: &mut [VariableEnvelope]) {
        if self.optimizer.design_space.mode != DesignMode::CleanSheet {
            return;
        }
        let nominal = envelope.iter().map(|v| v.nominal).collect::<Vec<_>>();
        let derived = DesignVector::from_array(&nominal)
            .ok()
            .and_then(|nominal| self.clean_sheet_box(&nominal));
        for (index, variable) in envelope.iter_mut().enumerate() {
            if variable.fixed {
                continue;
            }
            // The derived box is clipped to the design space's guardrails,
            // then both boxes are widened to contain the start: the start is
            // always a candidate the search may return.
            if let Some(&(lower, upper)) = derived.as_ref().and_then(|b| b.get(index)) {
                let guard = &SPECS[index];
                let lower = lower.clamp(guard.preset_lower, guard.preset_upper);
                let upper = upper.clamp(guard.preset_lower, guard.preset_upper);
                variable.lower = lower.min(variable.nominal);
                variable.upper = upper.max(variable.nominal);
            }
            if let Some(&(lower, upper)) = self.optimizer.design_space.bounds.get(variable.name) {
                variable.lower = lower.min(variable.nominal);
                variable.upper = upper.max(variable.nominal);
                variable.fixed = variable.lower >= variable.upper;
            }
        }
    }

    /// The derived class box, in design-vector order; `None` when the run
    /// does not derive one.
    fn clean_sheet_box(&self, nominal: &DesignVector) -> Option<Vec<(f64, f64)>> {
        if !self.derives_clean_sheet_start() {
            return None;
        }
        let wing = self.class_wing()?;
        let start = self.chords_for_area(wing)?;
        let (cl_lo, cl_hi) = LIFT_COEFFICIENT_WINDOW;
        let (ar_lo, ar_hi) = ASPECT_RATIO_WINDOW;
        let aspect_ratio = wing.span_m.powi(2) / wing.area_m2;
        let mut span_hi = (ar_hi * aspect_ratio * wing.area_m2 / cl_lo).sqrt();
        let span_lo = (ar_lo * aspect_ratio * wing.area_m2 / cl_hi).sqrt();
        if let Some(limit) = self.max_design_span_m() {
            // A class span close to the band edge keeps the whole band above
            // it: the upper bound is the code limit itself.
            span_hi = if wing.span_m >= SPAN_HEADROOM_FRACTION * limit {
                limit
            } else {
                span_hi.min(limit)
            };
        }
        let length_m = nominal.fuselage_length_m;
        let scaled = |value: f64, (lo, hi): (f64, f64)| (value * lo, value * hi);
        let shifted = |name: &str, centre: f64| {
            let s = spec(name);
            let scale = length_m / spec("fuselage_length_m").default;
            (centre + s.lower * scale, centre + s.upper * scale)
        };
        let max_sweep = spec("sweep_deg").preset_upper;
        Some(
            SPECS
                .iter()
                .map(|s| match s.name {
                    "span_m" => (span_lo, span_hi.max(span_lo)),
                    "root_chord_m" => scaled(start.root_chord_m, INBOARD_CHORD_WINDOW),
                    "break_chord_m" => scaled(start.break_chord_m, INBOARD_CHORD_WINDOW),
                    "tip_chord_m" => scaled(start.tip_chord_m, TIP_CHORD_WINDOW),
                    "sweep_deg" => (
                        (wing.sweep_deg - SWEEP_HALF_WINDOW_DEG).max(0.0),
                        (wing.sweep_deg + SWEEP_HALF_WINDOW_DEG).min(max_sweep),
                    ),
                    "wing_x_shift_m" => shifted(s.name, nominal.wing_x_shift_m),
                    "tail_x_shift_m" => shifted(s.name, nominal.tail_x_shift_m),
                    "fuselage_length_m" => (
                        length_m * s.lower / s.default,
                        length_m * s.upper / s.default,
                    ),
                    _ => (s.lower, s.upper),
                })
                .collect(),
        )
    }

    /// The class-sized wing area, span and sweep of this brief.
    pub(crate) fn class_wing(&self) -> Option<ClassWing> {
        let stats = fleet_statistics()?;
        let r = &self.requirements;
        let q = cruise_dynamic_pressure_pa(r.cruise_mach, r.cruise_altitude_m)?;
        let weight_n = r.mtow_kg * r.gravity_m_s2;
        if !(weight_n.is_finite() && weight_n > 0.0) {
            return None;
        }
        let area_m2 = weight_n / (q * stats.cruise_lift_coefficient_at_mtow);
        let mut span_m = (stats.aspect_ratio * area_m2).sqrt();
        if let Some(limit) = self.max_design_span_m() {
            span_m = span_m.min(limit);
        }
        Some(ClassWing {
            area_m2,
            span_m,
            sweep_deg: sweep_for_mach_deg(r.cruise_mach, stats.normal_mach),
        })
    }

    /// The start vector whose fleet-median-tapered chords close `wing`'s
    /// area on this configuration's built planform. The chord scale enters
    /// the side-of-body clip non-linearly, so a few fixed-point steps follow
    /// the linear first guess.
    fn chords_for_area(&self, wing: ClassWing) -> Option<DesignVector> {
        let stats = fleet_statistics()?;
        let at = |root_chord_m: f64| DesignVector {
            span_m: wing.span_m,
            root_chord_m,
            break_chord_m: root_chord_m * stats.break_root_chord_ratio,
            tip_chord_m: root_chord_m * stats.tip_root_chord_ratio,
            sweep_deg: wing.sweep_deg,
            ..DesignVector::default()
        };
        let area = |design: &DesignVector| {
            WingPlanformSummary::of(&self.geometry.wing, design).map(|w| w.area_m2)
        };
        let mut root_chord_m = wing.area_m2 / area(&at(1.0))?;
        for _ in 0..4 {
            root_chord_m *= wing.area_m2 / area(&at(root_chord_m))?;
        }
        (root_chord_m.is_finite() && root_chord_m > 0.0).then(|| at(root_chord_m))
    }

    /// First estimate of the fuselage length, m: the two tapers plus the
    /// economy rows the brief needs (cabin-sized runs re-solve it from the
    /// detailed layout), or the reference length scaled by the diameter for
    /// a freighter.
    pub(crate) fn estimated_fuselage_length_m(&self) -> Option<f64> {
        geometry::estimated_fuselage_length_m(self)
    }
}
