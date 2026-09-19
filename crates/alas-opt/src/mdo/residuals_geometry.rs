// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planform, wing-loading and accommodation requirement family.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveWeights};
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::tail_volume_coefficients;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Geometry;
use super::types::ConstraintResidual;

/// The span limit, wing-area cap, wing-loading floor, transport body-attitude
/// window, tail-volume window and passenger/cargo-capacity shortfall.
pub(super) fn geometry_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    weights: &ObjectiveWeights,
    policy: ConstraintPolicy,
    target_num_passengers: i64,
    target_cargo_payload_kg: f64,
) -> Vec<ConstraintResidual> {
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let req = &config.requirements;
    let objective = &config.optimizer.objective;
    let dv: DesignVector = outcome.history.dv;
    let plane: &Airplane = &outcome.plane;
    let mut residuals = Vec::new();

    if objective.max_span_m > 0.0 {
        residuals.push(ConstraintResidual::scaled(
            "span",
            Geometry,
            dv.span_m,
            objective.max_span_m,
            "m",
            dv.span_m - objective.max_span_m,
            policy,
        ));
    }

    residuals.push(ConstraintResidual::scaled(
        "wing_area",
        Geometry,
        plane.s_ref,
        req.max_wing_area_m2,
        "m^2",
        plane.s_ref - req.max_wing_area_m2,
        policy,
    ));

    // The requirement is a design wing loading, MTOW over area: the design
    // gross mass the components were sized against. That is the closed
    // takeoff mass of a coupled clean-sheet design and the declared MTOW of
    // a fixed aircraft, which does not stop being the same wing when it is
    // dispatched light on a short route.
    let wing_loading_kg_m2 = outcome.sized.design_gross_mass_kg / plane.s_ref.max(1e-9);
    residuals.push(ConstraintResidual::scaled(
        "wing_loading",
        Geometry,
        wing_loading_kg_m2,
        req.min_wing_loading_kg_m2,
        "kg/m^2",
        req.min_wing_loading_kg_m2 - wing_loading_kg_m2,
        policy,
    ));

    // The clean-sheet transport search must keep the aircraft body attitude
    // in the configured cruise window.  This is a requirement on the
    // three-dimensional trimmed aircraft, not a guessed local MSES alpha
    // limit: the pipeline still maps this solved body angle through the
    // section twist and downwash explicitly.  Registered aircraft retain
    // their measured body attitude for parity/audit reporting; applying a
    // generic 2 to 4 degree design target to them would rewrite the reference
    // aircraft rather than test it.
    if config.optimizer.design_space.mode == alas_config::DesignMode::CleanSheet
        && req.aircraft_type == "passenger"
        && weights.transport_planform_constraints_enabled
    {
        residuals.push(body_alpha_window_residual(
            outcome.geometric_body_alpha_deg,
            weights.geometric_body_alpha_min_deg,
            weights.geometric_body_alpha_max_deg,
            policy,
        ));
    }

    // A tail-volume window is a plausibility band, not a requirement: the
    // surveyed tools rank it as a preference (research note
    // `.agent/reports/research-2026-09-05-mdo-drivers.md`, tier S), and the
    // legacy objective scores it as a quadratic add-on. Under a hard family
    // it is therefore ranked soft; diagnostic and off follow the family.
    let preference = match policy {
        ConstraintPolicy::Hard => ConstraintPolicy::Soft,
        other => other,
    };
    let (vh, vv) = tail_volume_coefficients(plane);
    if let Some(vh) = vh {
        residuals.push(tail_volume_residual(
            "tail_volume_h",
            vh,
            weights.min_hstab_volume_coef,
            weights.max_hstab_volume_coef,
            preference,
        ));
    }
    if let Some(vv) = vv {
        residuals.push(tail_volume_residual(
            "tail_volume_v",
            vv,
            weights.min_vstab_volume_coef,
            weights.max_vstab_volume_coef,
            preference,
        ));
    }

    residuals.extend(plausibility_residuals(outcome, config, policy));

    if req.aircraft_type == "cargo" {
        // Clarified ledger App Features 2, decision D10: the entered cargo
        // mass is a *target to match*, not a floor to clear and not a licence
        // to load without limit. It is therefore reported as a two-sided
        // deviation from the target under the Soft policy, which is a cost
        // contribution rather than a rejection: a candidate that cannot reach
        // the requested payload is ranked worse than one that can, and so is
        // one that only reaches it by carrying more than was asked for, while
        // what actually rejects an overloaded aircraft stays where it
        // belongs, in the mass, balance and volume residuals that measure the
        // physical limits.
        //
        // The pair is normalized by the target itself and enters the scalar
        // cost through the objective's existing `soft_penalty_weight`
        // (`mdo::cost::assemble`), so no new weight or coefficient is
        // introduced and every other cost term keeps its meaning.
        residuals.extend(cargo_target_residuals(
            outcome.sized.carried_cargo_payload_kg,
            target_cargo_payload_kg,
            match policy {
                ConstraintPolicy::Hard => ConstraintPolicy::Soft,
                other => other,
            },
        ));
    } else if target_num_passengers > 0 {
        // Every study (registered aircraft or clean-sheet alike) sizes
        // its cabin from the class mix and fills the candidate floor; there
        // is no copied integer passenger target to violate. This residual
        // only appears when `DesignRequirements::min_passenger_capacity` is
        // set: a one-sided floor check on the resolved capacity, not an
        // equality constraint.
        residuals.push(ConstraintResidual::scaled(
            "passenger_shortfall",
            Geometry,
            outcome.sized.carried_passengers as f64,
            target_num_passengers as f64,
            "passengers",
            (target_num_passengers - outcome.sized.carried_passengers) as f64,
            policy,
        ));
    }

    residuals
}

/// Quarter-chord fraction the aerodynamic centre of a surface is taken at,
/// matching `alas_stab::trim`'s own convention so the tail arm measured here
/// is the one the stability solve uses.
const AC_CHORD_FRACTION: f64 = 0.25;

/// The validity-domain residuals: the design must still be a transport
/// aeroplane the disciplines' correlations describe.
///
/// These read the *built* geometry rather than the design vector, so a
/// pathology that only appears after the builder resolves the planform (a
/// span that inflates the aspect ratio against the projected reference area,
/// a tail the fuselage cannot carry) is visible here. Each limit and its
/// rationale is documented on
/// [`alas_config::optimizer::PlausibilityLimits`]. They enter the Geometry
/// family and therefore follow its configured policy and the
/// constraint-relaxation rules; they are not a separate rejection path.
///
/// Units: aspect ratio, fineness, chord ratios and thickness ratios are
/// dimensionless; the tail-arm fraction is metres over metres, positive aft.
fn plausibility_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let limits = &config.optimizer.plausibility;
    if !limits.enabled || limits.validate().is_err() {
        return Vec::new();
    }
    let plane: &Airplane = &outcome.plane;
    let dv: DesignVector = outcome.history.dv;
    let mut residuals = Vec::new();

    // Aspect ratio on the same projected reference quantities every other
    // aerodynamic coefficient uses, so this is the ratio the induced-drag
    // credit is actually computed from.
    let aspect_ratio = plane.b_ref.powi(2) / plane.s_ref.max(1e-9);
    residuals.extend(window_residuals(
        "aspect_ratio",
        aspect_ratio,
        limits.min_aspect_ratio,
        limits.max_aspect_ratio,
        "-",
        policy,
    ));

    // Fuselage fineness: overall length over the largest equivalent diameter
    // of its cross-sections, the definition the wetted-area and skin-friction
    // build-up uses.
    if let Some(fineness) = fuselage_fineness(plane) {
        residuals.extend(window_residuals(
            "fuselage_fineness",
            fineness,
            limits.min_fuselage_fineness,
            limits.max_fuselage_fineness,
            "-",
            policy,
        ));
    }

    // Tail arm as a fraction of the body that has to carry it.
    if let (Some(wing), Some(hstab)) = (plane.wings.first(), plane.wings.get(1)) {
        let arm_m = hstab.aerodynamic_center(AC_CHORD_FRACTION)[0]
            - wing.aerodynamic_center(AC_CHORD_FRACTION)[0];
        let body_length_m = fuselage_length_m(plane).unwrap_or(dv.fuselage_length_m);
        if body_length_m > 0.0 && arm_m.is_finite() {
            residuals.extend(window_residuals(
                "tail_arm_fraction",
                arm_m / body_length_m,
                limits.min_tail_arm_fraction,
                limits.max_tail_arm_fraction,
                "-",
                policy,
            ));
        }
    }

    // Planform: a tip that has not vanished, and a trailing-edge break that
    // is a blend rather than a notch or an inversion.
    if dv.root_chord_m > 0.0 {
        residuals.extend(window_residuals(
            "tip_root_chord_ratio",
            dv.tip_chord_m / dv.root_chord_m,
            limits.min_tip_root_chord_ratio,
            limits.max_tip_root_chord_ratio,
            "-",
            policy,
        ));
        if limits.require_monotonic_planform_break {
            // The raw residual is the larger of the two orderings' misses, in
            // metres of chord; the limit it is scaled by is the root chord,
            // so the reported value is a chord fraction like its neighbours.
            let below_tip = dv.tip_chord_m - dv.break_chord_m;
            let above_root = dv.break_chord_m - dv.root_chord_m;
            residuals.push(ConstraintResidual::scaled(
                "planform_break_ordering",
                Geometry,
                dv.break_chord_m,
                dv.root_chord_m,
                "m",
                below_tip.max(above_root),
                policy,
            ));
        }
    }

    // Geometric washout, from the built sections. This has to be measured
    // here and not from the design vector: `dv.tip_twist_deg` is the tip
    // section's *absolute* incidence, which the builder writes straight onto
    // the tip while the root takes `geometry.wing.root_twist_deg` (+2.0 to
    // +4.5 degrees on the registered aircraft). The washout is the
    // difference, and reading the variable alone misreads a +1 degree tip on
    // a +4 degree root as wash-in when it is three degrees of washout. The
    // trim phase rotates every section of the surface by the same incidence,
    // so the difference taken here is unchanged by it.
    if let Some(washout_deg) = tip_washout_deg(plane) {
        residuals.extend(twist_window_residuals(
            washout_deg,
            limits.min_tip_washout_deg,
            limits.max_tip_washout_deg,
            policy,
        ));
    }

    // Root section thickness, as the built section reports it rather than as
    // the design vector's multiplier, so a preset airfoil and a scaled one
    // are compared on the same quantity.
    if let Some(thickness) = root_thickness_ratio(plane) {
        residuals.extend(window_residuals(
            "root_thickness_ratio",
            thickness,
            limits.min_root_thickness_ratio,
            limits.max_root_thickness_ratio,
            "-",
            policy,
        ));
    }

    residuals
}

/// Overall fuselage length in metres, from the primary body's sections.
fn fuselage_length_m(plane: &Airplane) -> Option<f64> {
    let fuselage = plane.fuselages.first()?;
    let first = fuselage.xsecs.first()?;
    let last = fuselage.xsecs.last()?;
    let length = last.xyz_c[0] - first.xyz_c[0];
    (length.is_finite() && length > 0.0).then_some(length)
}

/// Fuselage fineness ratio: length over the largest equivalent diameter.
///
/// A non-circular section is reduced to the diameter of the circle with the
/// same area, `sqrt(width * height)`, which is the same equivalent the wetted
/// area build-up uses for an elliptical body.
fn fuselage_fineness(plane: &Airplane) -> Option<f64> {
    let fuselage = plane.fuselages.first()?;
    let length = fuselage_length_m(plane)?;
    let diameter = fuselage
        .xsecs
        .iter()
        .map(|section| (section.width * section.height).max(0.0).sqrt())
        .fold(0.0_f64, f64::max);
    (diameter > 0.0).then(|| length / diameter)
}

/// The two-sided cargo target-matching pair, kg.
///
/// `cargo_target_shortfall` is positive when the candidate carries less than
/// the requested payload and `cargo_target_excess` when it carries more, so
/// at most one of them is ever on the violating side and the direction of the
/// miss is explicit. Both are normalized by the requested mass, so the
/// penalty is the relative deviation from the target and a small freighter's
/// tonne counts as much as a large one's proportionally.
///
/// A target of zero (or a non-finite one) produces no residuals at all: there
/// is nothing to match, and scaling by it would be a division by zero rather
/// than a requirement.
fn cargo_target_residuals(
    carried_kg: f64,
    target_kg: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if !(target_kg.is_finite() && target_kg > 0.0) || !carried_kg.is_finite() {
        return Vec::new();
    }
    let shortfall = target_kg - carried_kg;
    let excess = carried_kg - target_kg;
    vec![
        ConstraintResidual::scaled(
            "cargo_target_shortfall",
            Geometry,
            carried_kg,
            target_kg,
            "kg",
            shortfall,
            policy,
        ),
        ConstraintResidual::scaled(
            "cargo_target_excess",
            Geometry,
            carried_kg,
            target_kg,
            "kg",
            excess,
            policy,
        ),
    ]
}

/// Geometric twist of the main wing's tip section relative to its root,
/// degrees, negative for washout.
///
/// Read from the built sections. `WingXSec::twist` is mutated in place by the
/// trim phase, which applies one incidence to the whole surface, so the
/// difference taken here is the geometric twist distribution and not the
/// trimmed attitude.
fn tip_washout_deg(plane: &Airplane) -> Option<f64> {
    let wing = plane.wings.first()?;
    let root = wing.xsecs.first()?;
    let tip = wing.xsecs.last()?;
    let washout = tip.twist - root.twist;
    washout.is_finite().then_some(washout)
}

/// Degrees of twist that count as one unit of violation.
///
/// The washout window brackets zero, so it cannot be scaled by the magnitude
/// of its own bound the way a positive quantity is. One degree is the natural
/// reference: it is the resolution the design variable is declared at and the
/// order of the difference between a flat wing and a conventional one.
const TWIST_VIOLATION_SCALE_DEG: f64 = 1.0;

/// Twist below which a window miss is builder roundoff rather than wash-in,
/// degrees. The same role `types::NUMERICAL_SLACK` plays for the scaled
/// residuals, stated in this residual's own unit because its upper bound is
/// zero and has no magnitude to take a relative slack from. A flat wing sits
/// exactly on the bound, so without it a section coordinate that rounds one
/// ulp the wrong way would reject the reference aircraft.
const TWIST_NUMERICAL_SLACK_DEG: f64 = 1.0e-5;

/// The two-sided washout residual, scaled in degrees.
///
/// `ConstraintResidual::scaled` divides by the magnitude of the limit, which
/// is zero on the upper bound of this window, so the normalization is stated
/// explicitly here instead.
fn twist_window_residuals(
    washout_deg: f64,
    minimum_deg: f64,
    maximum_deg: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let over = washout_deg - maximum_deg;
    let under = minimum_deg - washout_deg;
    vec![
        ConstraintResidual::direct(
            "tip_washout_max",
            Geometry,
            washout_deg,
            maximum_deg,
            "deg",
            over,
            ((over - TWIST_NUMERICAL_SLACK_DEG) / TWIST_VIOLATION_SCALE_DEG).max(0.0),
            policy,
        ),
        ConstraintResidual::direct(
            "tip_washout_min",
            Geometry,
            washout_deg,
            minimum_deg,
            "deg",
            under,
            ((under - TWIST_NUMERICAL_SLACK_DEG) / TWIST_VIOLATION_SCALE_DEG).max(0.0),
            policy,
        ),
    ]
}

/// Maximum thickness of the main wing's root section, as a chord fraction.
fn root_thickness_ratio(plane: &Airplane) -> Option<f64> {
    let section = plane.wings.first()?.xsecs.first()?;
    let samples = alas_geom::aircraft::spacing::linspace(0.0, 1.0, 101);
    let thickness = section.airfoil.max_thickness(&samples);
    (thickness.is_finite() && thickness > 0.0).then_some(thickness)
}

/// One or two residuals bounding `value` inside `[minimum, maximum]`.
///
/// Both sides are reported separately rather than folded into a single
/// nearest-bound residual, because a rejected candidate has to say which way
/// it left the window: "aspect ratio above its limit" and "aspect ratio below
/// its limit" are different design errors with opposite corrections.
fn window_residuals(
    id_stem: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
    unit: &'static str,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if !value.is_finite() {
        // A non-finite geometric quantity is a failed build, which the
        // evaluator already rejects upstream; emitting a residual here would
        // turn it into a scaled miss with a direction it does not have.
        return Vec::new();
    }
    let (max_id, min_id) = window_ids(id_stem);
    vec![
        ConstraintResidual::scaled(
            max_id,
            Geometry,
            value,
            maximum,
            unit,
            value - maximum,
            policy,
        ),
        ConstraintResidual::scaled(
            min_id,
            Geometry,
            value,
            minimum,
            unit,
            minimum - value,
            policy,
        ),
    ]
}

/// Stable `<stem>_max` / `<stem>_min` identifiers for a windowed quantity.
const fn window_ids(id_stem: &str) -> (&'static str, &'static str) {
    match id_stem.as_bytes() {
        b"aspect_ratio" => ("aspect_ratio_max", "aspect_ratio_min"),
        b"fuselage_fineness" => ("fuselage_fineness_max", "fuselage_fineness_min"),
        b"tail_arm_fraction" => ("tail_arm_fraction_max", "tail_arm_fraction_min"),
        b"tip_root_chord_ratio" => ("tip_root_chord_ratio_max", "tip_root_chord_ratio_min"),
        b"root_thickness_ratio" => ("root_thickness_ratio_max", "root_thickness_ratio_min"),
        _ => ("plausibility_max", "plausibility_min"),
    }
}

/// A two-sided residual for the configured geometric body-attitude window.
///
/// The raw value is positive only outside the interval; inside, the margin
/// to the nearer bound is reported as a negative value.  The residual uses a
/// degree unit and the nearest violated bound so the optimizer receives a
/// useful direction without treating the interval as a local-section solver
/// validity claim.
fn body_alpha_window_residual(
    actual_deg: f64,
    min_deg: f64,
    max_deg: f64,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    let (limit_deg, raw_residual) = if actual_deg < min_deg {
        (min_deg, min_deg - actual_deg)
    } else if actual_deg > max_deg {
        (max_deg, actual_deg - max_deg)
    } else {
        let slack_to_min = actual_deg - min_deg;
        let slack_to_max = max_deg - actual_deg;
        if slack_to_min < slack_to_max {
            (min_deg, -slack_to_min)
        } else {
            (max_deg, -slack_to_max)
        }
    };
    // A degree interval crosses zero, so scaling by the bound's magnitude is
    // well-defined for the configured positive transport window.  The
    // generic constructor still protects malformed zero/negative bounds.
    ConstraintResidual::scaled(
        "geometric_body_alpha",
        Geometry,
        actual_deg,
        limit_deg,
        "deg",
        raw_residual,
        policy,
    )
}

/// A two-sided window residual: violated below `min_coef` or above
/// `max_coef`, and otherwise reported against whichever bound is nearer with
/// a negative (compliant) raw residual.
fn tail_volume_residual(
    id: &'static str,
    value: f64,
    min_coef: f64,
    max_coef: f64,
    policy: ConstraintPolicy,
) -> ConstraintResidual {
    let (limit, raw_residual) = if value < min_coef {
        (min_coef, min_coef - value)
    } else if value > max_coef {
        (max_coef, value - max_coef)
    } else {
        let slack_to_min = value - min_coef;
        let slack_to_max = max_coef - value;
        if slack_to_min < slack_to_max {
            (min_coef, -slack_to_min)
        } else {
            (max_coef, -slack_to_max)
        }
    };
    ConstraintResidual::scaled(id, Geometry, value, limit, "-", raw_residual, policy)
}
