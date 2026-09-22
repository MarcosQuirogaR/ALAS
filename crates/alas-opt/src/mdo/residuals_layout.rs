// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing-to-fuselage layout plausibility: geometric-interference and
//! configuration-band residuals that `mdo::residuals_geometry`'s
//! shape/aspect-ratio family does not cover.
//!
//! `docs/optimizer-design-vector.md` names the gap this module closes:
//! "Nothing in the optimizer checks that the wing does not intersect the
//! fuselage... There is also no residual relating `wing_x_shift_m` or
//! `tail_x_shift_m` to `fuselage_length_m`." Every residual below is read
//! from the **built** geometry or its declared configuration, never the
//! design vector alone, so a builder change that moves a station is caught
//! here too.
//!
//! # Frame, units and sign
//!
//! Aircraft axes: `x` aft from the fuselage nose reference, `y` to the right
//! wing tip, `z` up (`crates/alas-geom/src/aircraft/section_outline.rs`,
//! restated in `docs/optimizer-design-vector.md`). Lengths m, angles deg,
//! ratios dimensionless. Every residual is feasible iff `raw_residual <= 0`
//! (the [`ConstraintResidual`] convention), so a "maximum" limit reports
//! `actual - limit` and a "minimum" limit reports `limit - actual`.
//!
//! # Sourcing
//!
//! Each band cites the qualitative engineering principle from a primary
//! conceptual-design reference and, where the precise numeric window is not
//! itself independently a matter of physical necessity (a genuine
//! containment relation, e.g. a chord cannot end past the tail it is
//! mounted on), the number is anchored to this program's own eight
//! registered aircraft with an explicit margin, exactly as
//! `alas_config::optimizer::plausibility::PlausibilityLimits` already does
//! for `min_tip_washout_deg` and `max_tail_arm_fraction`. That measurement
//! is reproducible directly from `crates/alas-config/src/presets/**` and is
//! not a literature figure copied without verification.
//!
//! # Where these are enforced
//!
//! `mdo::residuals::build` calls [`layout_residuals`] under the same
//! Geometry family and `objective.geometry_constraints` policy every other
//! geometry residual uses, so a candidate that fails one is rejected by the
//! same relaxation and feasibility rules as `aspect_ratio_max`: not a
//! separate gate.

use alas_config::{AlasConfig, ConstraintPolicy};
use alas_geom::aircraft::airplane::Airplane;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Geometry;
use super::types::ConstraintResidual;

/// The wing-to-fuselage layout residuals for one sized candidate.
pub(super) fn layout_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let plane = &outcome.plane;
    let mut residuals = Vec::new();
    residuals.extend(wing_root_incidence_residuals(config, policy));
    if let (Some(fuselage_length), Some(root), Some(tip)) = (
        fuselage_length_m(plane),
        root_station(plane),
        tip_station(plane),
    ) {
        residuals.extend(dihedral_residuals(root, tip, policy));
        residuals.extend(apex_on_fuselage_residuals(root, fuselage_length, policy));
        residuals.extend(wingbox_depth_residual(root, plane, config, policy));
        residuals.extend(vertical_position_residual(root, config, policy));
    }
    residuals.extend(wave_drag_ceiling_residual(outcome, config, policy));
    residuals
}

/// Overall fuselage length in metres, from the primary body's sections.
///
/// Reproduced from `mdo::residuals_geometry::fuselage_length_m` rather than
/// imported: that module is at its reviewed line budget
/// (`docs/source-size-budgets.tsv`) and this is a five-line read of public
/// fields, the same "reproduce rather than couple" call `mdo::range` makes
/// for its own great-circle fallback.
fn fuselage_length_m(plane: &Airplane) -> Option<f64> {
    let fuselage = plane.fuselages.first()?;
    let first = fuselage.xsecs.first()?;
    let last = fuselage.xsecs.last()?;
    let length = last.xyz_c[0] - first.xyz_c[0];
    (length.is_finite() && length > 0.0).then_some(length)
}

/// One wing station's leading-edge coordinates and chord.
#[derive(Debug, Clone, Copy)]
struct WingStation {
    xyz_le: [f64; 3],
    chord: f64,
}

fn root_station(plane: &Airplane) -> Option<WingStation> {
    let section = plane.wings.first()?.xsecs.first()?;
    Some(WingStation {
        xyz_le: section.xyz_le,
        chord: section.chord,
    })
}

fn tip_station(plane: &Airplane) -> Option<WingStation> {
    let section = plane.wings.first()?.xsecs.last()?;
    Some(WingStation {
        xyz_le: section.xyz_le,
        chord: section.chord,
    })
}

/// Maximum thickness-to-chord ratio of the main wing's root section.
///
/// Reproduced from `mdo::residuals_geometry::root_thickness_ratio` for the
/// same reason as [`fuselage_length_m`] above.
fn root_thickness_ratio(plane: &Airplane) -> Option<f64> {
    let section = plane.wings.first()?.xsecs.first()?;
    let samples = alas_geom::aircraft::spacing::linspace(0.0, 1.0, 101);
    let thickness = section.airfoil.max_thickness(&samples);
    (thickness.is_finite() && thickness > 0.0).then_some(thickness)
}

/// Wing root incidence relative to the fuselage waterline datum,
/// `geometry.wing.root_twist_deg`: the fixed geometric angle between the
/// wing-root chord line and the fuselage reference line, positive
/// leading-edge-up. Not a design variable (`docs/optimizer-design-vector.md`
/// lists the sixteen that are), so this is a check on a candidate's declared
/// configuration rather than on the search's own freedom -- the same role
/// `wing_root_incidence` plays for a hand-edited or imported geometry
/// scaffold that the search never touches.
///
/// # Source and band
///
/// Raymer, *Aircraft Design: A Conceptual Approach*, the wing-incidence
/// selection discussion in the configuration-layout chapter: incidence is
/// chosen chiefly so the fuselage sits near a low angle of attack at the
/// cruise design point, and conventional swept-wing jet transports set it a
/// few degrees positive. Measured directly on this program's eight
/// registered airliners (`crates/alas-config/src/presets/**`,
/// `geometry.wing.root_twist_deg`), the value runs +2.0 deg (AVE, A320-200,
/// ATR72-600) to +4.5 deg (A380-800) -- the same span
/// `PlausibilityLimits::min_tip_washout_deg`'s own doc comment records for
/// the identical field. The window is that measured span with a two-degree
/// margin each side.
fn wing_root_incidence_residuals(
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    const MIN_DEG: f64 = 0.0;
    const MAX_DEG: f64 = 6.5;
    let incidence = config.geometry.wing.root_twist_deg;
    signed_window_residuals(
        "wing_root_incidence",
        incidence,
        MIN_DEG,
        MAX_DEG,
        "deg",
        policy,
    )
}

/// Mean geometric dihedral of the main wing, root to tip, degrees: the angle
/// the leading edge rises out of the `y`-`z` plane, positive tip-above-root.
/// `atan2` keeps the sign correct for the unswept semispan direction this
/// program always builds (`y` increasing outboard).
fn mean_dihedral_deg(root: WingStation, tip: WingStation) -> f64 {
    let dy = tip.xyz_le[1] - root.xyz_le[1];
    let dz = tip.xyz_le[2] - root.xyz_le[2];
    dz.atan2(dy).to_degrees()
}

/// Dihedral band by wing vertical position (low root at or below the
/// fuselage centreline vs. high/shoulder root at or above it -- root
/// vertical position, `geometry.wing.root_z_m`, is fixed configuration, not
/// a design variable, so the classification is stable for one candidate).
///
/// # Source and band
///
/// A high wing already contributes positive effective (pendulum/fin)
/// dihedral from the fuselage side area below it, so it is conventionally
/// built with little or none; a low wing has the opposite effect and is
/// conventionally built with more, offset somewhat by sweep's own
/// contribution to effective dihedral. This qualitative relation is standard
/// conceptual-design guidance (Raymer, *Aircraft Design: A Conceptual
/// Approach*, dihedral-selection discussion; Roskam, *Airplane Design Part
/// II*, wing geometry selection). Measured on this program's seven low-wing
/// registered airliners, geometric dihedral runs 7.18 deg (A340-300) to 8.58
/// deg (A320-200); the low-wing band is that span with margin, [3.0, 10.0]
/// deg. The one registered high/shoulder wing (ATR72-600) is built with 0.0
/// deg, consistent with the qualitative rule above; the high-wing band,
/// [-3.0, 5.0] deg, is therefore a single measured point plus the same
/// literature-qualitative margin rather than an independently-fitted range,
/// and is stated as such.
fn dihedral_residuals(
    root: WingStation,
    tip: WingStation,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let dihedral_deg = mean_dihedral_deg(root, tip);
    let low_wing = root.xyz_le[2] < 0.0;
    let (min_deg, max_deg) = if low_wing { (3.0, 10.0) } else { (-3.0, 5.0) };
    signed_window_residuals(
        "wing_dihedral",
        dihedral_deg,
        min_deg,
        max_deg,
        "deg",
        policy,
    )
}

/// The wing root leading and trailing edges must sit on the fuselage that
/// carries them, and the apex station must sit inside a band conventional
/// transports use -- the geometric-interference check
/// `docs/optimizer-design-vector.md` names as a known gap.
///
/// # Source and band
///
/// Torenbeek, *Synthesis of Subsonic Airplane Design*, the fuselage/wing
/// group-location discussion (component placement is driven chiefly by the
/// centre-of-gravity and tail-arm requirements the aft fuselage and
/// empennage then have to satisfy): a conventional aft-tailed transport's
/// wing sits roughly in the forward third to inboard half of the body.
/// Measured on this program's eight registered airliners, the root
/// leading-edge station runs 0.264 (A380-800) to 0.3755 (ATR72-600) of
/// fuselage length aft of the nose reference; the apex-fraction band, [0.15,
/// 0.50], is that span with margin. The hard containment bounds (leading
/// edge aft of the nose taper, trailing edge forward of the tailcone) use a
/// 2% fuselage-length clearance on each end so a candidate is rejected only
/// for genuinely leaving the body, not for using its declared taper length.
fn apex_on_fuselage_residuals(
    root: WingStation,
    fuselage_length_m: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    const CLEARANCE_FRACTION: f64 = 0.02;
    const MIN_APEX_FRACTION: f64 = 0.15;
    const MAX_APEX_FRACTION: f64 = 0.50;
    let root_le_x = root.xyz_le[0];
    let root_te_x = root_le_x + root.chord;
    let nose_clearance_m = CLEARANCE_FRACTION * fuselage_length_m;
    let tail_clearance_x_m = (1.0 - CLEARANCE_FRACTION) * fuselage_length_m;
    let apex_fraction = root_le_x / fuselage_length_m;
    vec![
        ConstraintResidual::scaled(
            "wing_root_le_on_fuselage",
            Geometry,
            root_le_x,
            nose_clearance_m,
            "m",
            nose_clearance_m - root_le_x,
            policy,
        ),
        ConstraintResidual::scaled(
            "wing_root_te_on_fuselage",
            Geometry,
            root_te_x,
            tail_clearance_x_m,
            "m",
            root_te_x - tail_clearance_x_m,
            policy,
        ),
    ]
    .into_iter()
    .chain(signed_window_residuals(
        "wing_apex_fraction",
        apex_fraction,
        MIN_APEX_FRACTION,
        MAX_APEX_FRACTION,
        "-",
        policy,
    ))
    .collect()
}

/// The exposed wingbox root depth (thickness ratio times root chord) must
/// fit within the fuselage cross-section that carries the carry-through
/// structure, with clearance for the cabin floor, systems and the
/// carry-through fairing.
///
/// # Source and band
///
/// A physical containment relation rather than a fitted literature number:
/// the primary structure box cannot be deeper than the body it passes
/// through. Obert, *Aerodynamic Design of Transport Aircraft*, discusses the
/// wing-fuselage carry-through box and its fairing allowance in the
/// wing-body integration chapter. Measured on this program's eight
/// registered airliners, the ratio of root box depth to fuselage height runs
/// 0.177 (A320-200) to 0.381 (A380-800); the ceiling, 0.60, carries roughly
/// 60% margin above the widest-body measured case.
fn wingbox_depth_residual(
    root: WingStation,
    plane: &Airplane,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    const MAX_DEPTH_FRACTION_OF_HEIGHT: f64 = 0.60;
    let Some(thickness_ratio) = root_thickness_ratio(plane) else {
        return Vec::new();
    };
    let box_depth_m = thickness_ratio * root.chord;
    let fuselage_height_m = config.geometry.fuselage.effective_height_m();
    let limit_m = MAX_DEPTH_FRACTION_OF_HEIGHT * fuselage_height_m;
    vec![ConstraintResidual::scaled(
        "wingbox_root_depth_fits_fuselage",
        Geometry,
        box_depth_m,
        limit_m,
        "m",
        box_depth_m - limit_m,
        policy,
    )]
}

/// The wing root must sit inside (or at the immediate boundary of) the
/// fuselage cross-section it is mounted on -- a coarse containment bound
/// rather than a low/high-wing-specific band, because `root_z_m` is fixed
/// configuration and this program does not model a separate pylon or strut
/// that could carry the wing outside the body's own envelope.
///
/// # Source and band
///
/// Geometric necessity: this model draws the wing root directly on the
/// fuselage loft, so a root far outside the body's own vertical extent is
/// not a high/low/mid-wing choice but a disconnected aeroplane. The ceiling,
/// one full fuselage diameter of vertical offset, is deliberately generous
/// (every registered aircraft's root sits within about 0.68 diameters, low
/// or high) so this catches only a configuration edit that placed the wing
/// implausibly far from the body, not a legitimate mount choice.
fn vertical_position_residual(
    root: WingStation,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let diameter_m = config.geometry.fuselage.diameter_m;
    if !(diameter_m.is_finite() && diameter_m > 0.0) {
        return Vec::new();
    }
    let offset_m = root.xyz_le[2].abs();
    vec![ConstraintResidual::scaled(
        "wing_root_within_fuselage_envelope",
        Geometry,
        offset_m,
        diameter_m,
        "m",
        offset_m - diameter_m,
        policy,
    )]
}

/// The Korn-equation transonic wave-drag estimate at the candidate's own
/// design cruise point must not exceed a small ceiling: sweep must be
/// consistent with the declared cruise Mach for the section thickness and
/// lift coefficient this candidate actually flies at.
///
/// # Reuse, not re-derivation
///
/// The drag-divergence Mach term is `alas_aero::analysis::AeroAnalysis::
/// wave_drag`'s own Korn-equation formula (Raymer, *Aircraft Design: A
/// Conceptual Approach*, the Korn-equation transonic wave-drag estimate),
/// reproduced here rather than called: `alas-opt` cannot add a dependency on
/// the internals of `alas-aero`'s trimmed analysis object for one scalar,
/// and this module already reproduces two more `alas_geom`-only helpers
/// above for the same reason `mdo::range` reproduces its own great-circle
/// distance rather than depending on `alas-route`. The kappa, onset-Mach and
/// wave-drag-rise coefficients are the same `config.drag_model` values the
/// real trim solve uses, so this is not a second, independently-tuned model.
///
/// # Band
///
/// Every registered aircraft's own reference geometry already sits within a
/// few hundredths of a Mach number of its own drag-divergence Mach at its
/// declared cruise point and design lift coefficient (by construction: that
/// small excess is the few drag counts of wave drag a real transonic
/// transport is designed to accept, not a modelling defect). The measured
/// wave-drag estimate at that point never exceeds about 0.4 drag counts
/// (4e-5) on any of the eight. The ceiling, 8 drag counts (8.0e-4), is set
/// well above that and is a small fraction of a transport's typical 180-220
/// count parasite CD0, consistent with the general transonic-design
/// practice (Raymer; Obert, *Aerodynamic Design of Transport Aircraft*,
/// drag-divergence/buffet-margin discussion) of keeping cruise wave drag a
/// minor term rather than a dominant one. An insufficiently swept wing at a
/// high cruise Mach fails this quickly: the Korn rise is quartic in the
/// Mach excess past `mach_dd`.
fn wave_drag_ceiling_residual(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    const WAVE_DRAG_CEILING_CD: f64 = 8.0e-4;
    let Some(thickness) = root_thickness_ratio(&outcome.plane) else {
        return Vec::new();
    };
    let sweep_deg = outcome.history.dv.sweep_deg;
    let cos_sweep = sweep_deg.to_radians().cos();
    if cos_sweep <= 0.0 {
        return Vec::new();
    }
    let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
    let mach = config.requirements.cruise_mach;
    let v_m_s = mach * atmosphere.speed_of_sound();
    let q_pa = 0.5 * atmosphere.density() * v_m_s.powi(2);
    let cl = config
        .requirements
        .required_cruise_cl(q_pa, outcome.plane.s_ref);
    let drag = &config.drag_model;
    if mach < drag.wave_drag_onset_mach {
        return Vec::new();
    }
    let mach_dd = drag.korn_technology_factor / cos_sweep
        - thickness / cos_sweep.powf(2.0)
        - cl / (10.0 * cos_sweep.powf(3.0));
    let wave_drag_cd = if mach > mach_dd {
        drag.wave_drag_coefficient * (mach - mach_dd).powf(4.0)
    } else {
        0.0
    };
    vec![ConstraintResidual::scaled(
        "sweep_consistent_with_cruise_mach",
        Geometry,
        wave_drag_cd,
        WAVE_DRAG_CEILING_CD,
        "-",
        wave_drag_cd - WAVE_DRAG_CEILING_CD,
        policy,
    )]
}

/// Stable `<stem>_min` / `<stem>_max` identifiers for a windowed quantity,
/// in the same shape `mdo::residuals_geometry::window_ids` uses: a `match`
/// rather than a runtime format, because [`ConstraintResidual::id`] must be
/// `&'static str` and the identifier set here is small and fixed.
const fn window_ids(stem: &str) -> (&'static str, &'static str) {
    match stem.as_bytes() {
        b"wing_root_incidence" => ("wing_root_incidence_min", "wing_root_incidence_max"),
        b"wing_dihedral" => ("wing_dihedral_min", "wing_dihedral_max"),
        b"wing_apex_fraction" => ("wing_apex_fraction_min", "wing_apex_fraction_max"),
        _ => ("layout_min", "layout_max"),
    }
}

/// One or two residuals bounding `value` inside `[minimum, maximum]`, in the
/// same shape `mdo::residuals_geometry::window_residuals` uses for its own
/// plausibility windows: both sides are reported so a rejected candidate
/// says which way it left the window.
fn signed_window_residuals(
    id_stem: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
    unit: &'static str,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if !value.is_finite() {
        return Vec::new();
    }
    let (min_id, max_id) = window_ids(id_stem);
    let scale = maximum.abs().max(minimum.abs()).max(1.0e-6);
    vec![
        ConstraintResidual::direct(
            max_id,
            Geometry,
            value,
            maximum,
            unit,
            value - maximum,
            ((value - maximum) / scale).max(0.0),
            policy,
        ),
        ConstraintResidual::direct(
            min_id,
            Geometry,
            value,
            minimum,
            unit,
            minimum - value,
            ((minimum - value) / scale).max(0.0),
            policy,
        ),
    ]
}
