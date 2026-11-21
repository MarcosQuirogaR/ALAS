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
//! Geometry family and `ResidualRole::Constraint` role every other
//! geometry residual uses, so a candidate that fails one is rejected by the
//! same hard feasibility rules as `aspect_ratio_max`: not a
//! separate gate.

use crate::mdo::ResidualRole;

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

mod rake;
use rake::dihedral_tip_station;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Geometry;
use super::types::{CandidateDrag, ConstraintResidual};
use alas_aero::analysis::AeroAnalysis;

/// The wing-to-fuselage layout residuals for one sized candidate.
pub(super) fn layout_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let plane = &outcome.plane;
    let mut residuals = Vec::new();
    residuals.extend(root_to_kink_te_angle_residual(outcome, config, role));
    residuals.extend(wing_root_incidence_residuals(outcome, config, role));
    if let (Some(fuselage_length), Some(root), Some(tip)) = (
        fuselage_length_m(plane),
        root_station(plane),
        dihedral_tip_station(plane, config),
    ) {
        residuals.extend(dihedral_residuals(root, tip));
        residuals.extend(apex_on_fuselage_residuals(root, fuselage_length, role));
        residuals.extend(wingbox_depth_residual(root, plane, config, role));
        residuals.extend(vertical_position_residual(root, config, role));
    }
    residuals.extend(wave_drag_ceiling_residual(outcome, config, role));
    residuals
}

/// Exposed trailing-edge limit, deg: 90 plus a rounding guard, not a
/// relaxation (the ATR 72 edge is exactly 90). The pre-gate reads it too.
pub(crate) const TE_ANGLE_LIMIT_DEG: f64 = 90.0 + 1.0e-6;

/// Keep the exposed trailing edge, side of body to kink, within
/// [`TE_ANGLE_LIMIT_DEG`] of the aft fuselage axis
/// ([`crate::transport_planform::exposed_te_angle_deg`]).
fn root_to_kink_te_angle_residual(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let Some(angle_deg) =
        crate::transport_planform::exposed_te_angle_deg(&config.geometry.wing, &outcome.history.dv)
    else {
        return vec![ConstraintResidual::direct(
            "root_to_kink_te_angle_unavailable",
            Geometry,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )];
    };
    vec![ConstraintResidual::scaled(
        "root_to_kink_te_angle",
        Geometry,
        angle_deg,
        TE_ANGLE_LIMIT_DEG,
        "deg",
        angle_deg - TE_ANGLE_LIMIT_DEG,
        role,
    )]
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
    let wing = plane.wings.first()?;
    wing.xsecs.first()?;
    let (xyz_le, chord) = wing.design_root();
    Some(WingStation { xyz_le, chord })
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
///
/// A clean-sheet transport derives its setting angle (`sizing::wing_setting`): the
/// root incidence read is the one the candidate was evaluated with.
fn wing_root_incidence_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let incidence = outcome
        .sized
        .fuel_artifacts
        .tail_sizing
        .wing_incidence_deg
        .map_or(config.geometry.wing.root_twist_deg, |[root, _, _]| root);
    signed_window_residuals(
        "wing_root_incidence",
        incidence,
        WING_ROOT_INCIDENCE_WINDOW_DEG.0,
        WING_ROOT_INCIDENCE_WINDOW_DEG.1,
        "deg",
        role,
    )
}

/// The root incidence window, degrees: the measured span of the registered
/// airliners (+2.0 to +4.5) with two degrees of margin each side.
pub(crate) const WING_ROOT_INCIDENCE_WINDOW_DEG: (f64, f64) = (0.0, 6.5);

/// Mean geometric dihedral of the main wing, root to tip, degrees: the angle
/// the leading edge rises out of the `y`-`z` plane, positive tip-above-root.
/// `atan2` keeps the sign correct for the unswept semispan direction this
/// program always builds (`y` increasing outboard).
fn mean_dihedral_deg(root: WingStation, tip: WingStation) -> f64 {
    let dy = tip.xyz_le[1] - root.xyz_le[1];
    let dz = tip.xyz_le[2] - root.xyz_le[2];
    dz.atan2(dy).to_degrees()
}

/// The geometric dihedral against Raymer's initial-estimate guidelines,
/// reported, never ranked.
///
/// # Source and role
///
/// Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., sec. 4.1.5,
/// Table 4.2, p. 89: low wing 5 to 7 deg unswept (civil), 3 to 7 deg
/// subsonic swept; high wing 0 to 2 and -5 to -2 deg. The window here is the
/// envelope of the two subsonic columns for the root's vertical position
/// (low below the fuselage centreline, high otherwise; `geometry.wing.root_z_m`
/// is configuration, not a design variable): low [3, 7], high [-5, 2] deg.
/// Raymer states the table is an initial estimate from DATCOM data, to be
/// revised after the stability analysis of the layout: dihedral is chosen
/// for the dihedral effect, which sweep (about 1 deg per 10 deg) and wing
/// position also supply. A geometric band is therefore not a requirement:
/// the Learjet 60 flies with 2.5 deg (FlightSafety Learjet 60 Pilot
/// Training Manual, ch. 19), below the swept low-wing guideline. The
/// band this replaces, [3, 10] deg low and [-5, 5] deg high, was fitted on
/// the registered airliners and rejected that real aircraft. The
/// requirement the dihedral serves, a stabilizing dihedral effect, is
/// ranked by `lateral_dihedral_effect` (`residuals_lateral`).
fn dihedral_residuals(root: WingStation, tip: WingStation) -> Vec<ConstraintResidual> {
    let dihedral_deg = mean_dihedral_deg(root, tip);
    let low_wing = root.xyz_le[2] < 0.0;
    let (min_deg, max_deg) = if low_wing { (3.0, 7.0) } else { (-5.0, 2.0) };
    signed_window_residuals(
        "wing_dihedral",
        dihedral_deg,
        min_deg,
        max_deg,
        "deg",
        ResidualRole::Diagnostic,
    )
}

/// The wing root leading and trailing edges must sit on the fuselage that
/// carries them, and the apex station must sit inside a band conventional
/// transports use -- the geometric-interference check
/// `docs/optimizer-design-vector.md` lists with the body-clearance rows.
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
    role: ResidualRole,
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
            role,
        ),
        ConstraintResidual::scaled(
            "wing_root_te_on_fuselage",
            Geometry,
            root_te_x,
            tail_clearance_x_m,
            "m",
            root_te_x - tail_clearance_x_m,
            role,
        ),
    ]
    .into_iter()
    .chain(signed_window_residuals(
        "wing_apex_fraction",
        apex_fraction,
        MIN_APEX_FRACTION,
        MAX_APEX_FRACTION,
        "-",
        role,
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
    role: ResidualRole,
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
        role,
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
    role: ResidualRole,
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
        role,
    )]
}

/// The transonic wave drag at the candidate's own mid-cruise point must not
/// exceed a small ceiling: sweep must be consistent with the declared cruise
/// Mach for the section thickness and lift coefficient this candidate
/// actually flies at.
///
/// # The drag the missions fly
///
/// The wave drag is read from the candidate's cruise drag,
/// [`super::types::CandidateDrag::wave_cd`]: for a natively trimmed
/// candidate the trimmed drag table, which tabulates
/// `alas_aero::analysis::AeroAnalysis::wave_drag` (the law
/// `geometry.wing.drag_divergence_law` resolves to: by default the
/// Shevell-Kroo crest-critical method, else Korn's relation or Torenbeek's
/// Eq. 10.48 with the Lock rise) on the wing's sweep and thickness ratio; for
/// an external polar, its measured wave term. The residual and the fuel burn
/// therefore see one wave drag.
///
/// The lift coefficient is that of mid-cruise at the sizing cruise Mach, at
/// the level the design mission flies that mass at (mission closed: the
/// closure plan's segment begun at or above it, as the report reads it,
/// `cruise_mass::closure_mid_cruise_level_m`; under Hard MTOW the closure
/// plan's level at half its cruise distance; legal levels held to 300 ft/min
/// residual climb and 1.3 g buffet at the top-of-climb mass, plus steps; the
/// configured cruise altitude only without a plan), at
/// [`super::types::SizedCandidate::mid_cruise_mass_kg`]: the
/// design mission flown from the candidate's takeoff loading,
/// `TOW - (F - R) / 2` with `F` the fuel loaded at brake release and `R` the
/// fuel its dispatch keeps on landing ([`super::cruise_mass`]). Under Hard
/// MTOW that is the maximum loading's own mission, not its takeoff mass
/// combined with another mission's trip; under the mission-closed modes it
/// is the closed mission's own mid-cruise mass.
///
/// # Ceiling
///
/// Sweep is consistent with the cruise Mach when the aircraft cruises at or
/// below the drag-divergence Mach of the law in force, at the mid-cruise lift
/// coefficient, within the accuracy of the relation that locates that Mach:
/// the gate admits a cruise Mach up to
/// [`alas_aero::analysis::DRAG_DIVERGENCE_MACH_RMS_ERROR`] (0.023, the rms
/// error of the `M_dd` relations against the registered drag rises) above
/// it for Korn and Eq. 10.48; for Shevell-Kroo the method's own documented
/// scatter replaces it (`DivergenceLaw::divergence_mach_scatter`: 0.047, the
/// standard deviation over the 29 aircraft of Ciornei 2005, Tables C.1-C.4).
/// One estimate per law, never two added. The ceiling is the law's own wave
/// drag at that Mach, `alas_aero::analysis::DivergenceLaw::wave_cd` at
/// `DivergenceLaw::drag_divergence_mach(CL) + error`, so it is reached
/// exactly when the cruise Mach is that far past divergence:
/// - Shevell-Kroo (product default): divergence at `dCD/dM = 0.05`,
///   `M_div = M_cc [1.02 + 0.08 (1 - cos L)]`, and the ceiling
///   `0.0019 cos^3 L ((M_div + 0.047)/M_cc)^14.641`, about 40 to 60 counts
///   on the swept transports (it depends on sweep and on `M_div / M_cc`);
/// - Korn or Eq. 10.48 with the Lock rise `CDw = C (M - M_crit)^4`:
///   divergence at `dCD/dM = 0.1`, `M_dd - M_crit = (0.1 / 4C)^(1/3)`, and the
///   ceiling `C ((0.1 / 4C)^(1/3) + 0.023)^4`, 58.4 counts at `C = 20`.
///   A ceiling derived from one law and applied to the drag of another is not
///   a divergence criterion: the Lock value read against the Shevell table
///   sits 0.03 to 0.05 in Mach past Shevell's divergence. A gate with no
///   tolerance rejected certified aircraft at their published cruise Mach. An
///   insufficiently swept wing at a high cruise Mach fails this quickly: the
///   rise is steep past divergence under either law.
///
/// An external polar carries no law of its own; its ceiling is that of the
/// analysis' law on the candidate's wing.
///
/// # Role by design mode
///
/// The `M_dd` relation reads the drag rise of the registered aircraft with an rms error
/// of 0.023 in Mach, in-sample (`alas_aero::analysis::wave` module). A fixed certified
/// aircraft (`DesignMode::BaselineSandbox`) cruises at a published Mach, which is a fact
/// of the aircraft and not a design choice; a shortfall inside that rms (B787-9 0.004,
/// DC-10 0.011, LJ60 0.003 in Mach) is the relation's error, not a defect of the baseline,
/// so the row is a Diagnostic there and still reports the shortfall. The modes that design
/// or adapt the wing (clean sheet, reference adaptation) choose sweep for their cruise
/// Mach, and the row stays a hard requirement.
fn wave_drag_ceiling_residual(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let role = if config.optimizer.design_space.mode == alas_config::DesignMode::BaselineSandbox {
        ResidualRole::Diagnostic
    } else {
        role
    };
    let req = &config.requirements;
    let sized = &outcome.sized;
    // The mid-cruise point is flown where the mission flies it (the LJ60
    // starts cruise at 10,105 m against its configured 12,497 m): mission
    // closed, at the level its closure's plan flies the mid-cruise mass at,
    // as the report reads it (`cruise_mass::closure_mid_cruise_level_m`);
    // else at the closure plan's half-distance level. A plan on the
    // takeoff-band floor (an unswept wing at Mach 0.84 "cruised" at 482 m)
    // is no cruise: the gate reads the configured design altitude instead.
    let plan = sized.fuel_artifacts.frozen_plan.as_ref();
    let plan = plan.filter(|plan| plan.holds_a_cruise_level());
    let half_distance_m = plan.map(|plan| plan.level_at_cruise_fraction_m(0.5));
    let mid_altitude_m = super::cruise_mass::closure_mid_cruise_level_m(config, sized)
        .unwrap_or_else(|_| half_distance_m.unwrap_or(req.cruise_altitude_m));
    let atmosphere = alas_atmo::Atmosphere::new(mid_altitude_m);
    let mach = req.cruise_mach;
    let v_m_s = mach * atmosphere.speed_of_sound();
    let q_pa = 0.5 * atmosphere.density() * v_m_s.powi(2);
    let mid_cruise_mass_kg = sized.mid_cruise_mass_kg();
    let cl_mid = mid_cruise_mass_kg * req.gravity_m_s2 / (q_pa * outcome.plane.s_ref);
    if !cl_mid.is_finite() || cl_mid <= 0.0 {
        return Vec::new();
    }
    let wave_drag_cd = sized.fuel_artifacts.drag.wave_cd(cl_mid, mach);
    // The ceiling comes from the law the wave column is tabulated on.
    let drag_model = config.resolved_drag_model();
    let law = match &sized.fuel_artifacts.drag {
        CandidateDrag::Table(table) => *table.divergence_law(),
        CandidateDrag::External(_) => AeroAnalysis::new(
            &outcome.plane,
            AeroAnalysis::quarter_chord_sweep_deg(&outcome.plane, outcome.history.dv.sweep_deg),
            Some(config.geometry.clone()),
            Some(drag_model.clone()),
            Some(config.analysis.clone()),
        )
        .divergence_law(None),
    };
    let divergence_mach = law.drag_divergence_mach(cl_mid);
    // A hard row must not reject a design inside its own method's documented
    // prediction error, one estimate per law (not two added).
    let ceiling_mach = divergence_mach + law.divergence_mach_scatter();
    let wave_drag_ceiling_cd = law.wave_cd(ceiling_mach, cl_mid, drag_model.wave_drag_coefficient);
    // The start of cruise is reported beside the mid-cruise point the gate
    // is evaluated at: the loaded takeoff mass, climb burn not deducted, so
    // an upper bound of the heaviest cruise state.
    let start_mass_kg = sized.start_of_cruise_mass_kg();
    let start_altitude_m = plan.map_or(req.cruise_altitude_m, |plan| plan.cruise_altitude_m);
    let start_atmosphere = alas_atmo::Atmosphere::new(start_altitude_m);
    let start_q_pa =
        0.5 * start_atmosphere.density() * (mach * start_atmosphere.speed_of_sound()).powi(2);
    let cl_start = start_mass_kg * req.gravity_m_s2 / (start_q_pa * outcome.plane.s_ref);
    let start_drag_cd = sized.fuel_artifacts.drag.wave_cd(cl_start, mach);
    vec![ConstraintResidual::scaled(
        "sweep_consistent_with_cruise_mach",
        Geometry,
        wave_drag_cd,
        wave_drag_ceiling_cd,
        "-",
        wave_drag_cd - wave_drag_ceiling_cd,
        role,
    )
    .with_detail(format!(
        "gated at mid-cruise: mass {mid_cruise_mass_kg:.0} kg, level {mid_altitude_m:.0} m, CL {cl_mid:.4}, wave CD {wave_drag_cd:.5}, divergence Mach {divergence_mach:.3}, ceiling at Mach {ceiling_mach:.3}; \
         start of cruise (not gated): mass {start_mass_kg:.0} kg, level {start_altitude_m:.0} m, CL {cl_start:.4}, wave CD {start_drag_cd:.5}"
    ))]
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
    role: ResidualRole,
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
            role,
        ),
        ConstraintResidual::direct(
            min_id,
            Geometry,
            value,
            minimum,
            unit,
            minimum - value,
            ((minimum - value) / scale).max(0.0),
            role,
        ),
    ]
}
