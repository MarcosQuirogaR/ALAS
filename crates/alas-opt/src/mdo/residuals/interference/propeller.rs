// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propeller ground-clearance rows of a turboprop (CS 25.925(a), FAR 25.925(a)).
//! Frame, units and sign as in the parent module: metres, gap positive when
//! clear.
//!
//! - `propeller_ground_clearance`: the lowest blade tip stands at least 18 cm
//!   (7 in, nose-wheel aeroplane) above the ground with the gear statically
//!   deflected, level attitude.
//! - `propeller_ground_clearance_tyre_deflated`: the second sentence of the
//!   same paragraph, positive clearance in the level attitude with the
//!   critical tyre completely deflated and the corresponding strut bottomed.
//!   The strut stroke is Raymer, *Aircraft Design: A Conceptual Approach*,
//!   ch. 11, Eq. (11.12) (the stroke that absorbs the design sink speed at the
//!   gear load factor), read on the page; the fraction of it below the static
//!   position is Raymer Fig. 11.10; the flat-tyre drop is the tyre radius less
//!   the rim radius. Two cases are judged and the lower taken: the nose gear
//!   bottomed and flat (the aeroplane pitches nose down about the main
//!   wheels), and one main gear bottomed and flat (the aeroplane rolls about
//!   the opposite wheel). Heights are linear in these small rotations.
//!
//! Which way each simplification errs (a candidate failing here must be read
//! against it): the stroke uses the long end of Raymer's load-factor and
//! efficiency ranges and the 34 % below static (84 % for large transports
//! would leave 16 %), so the bottomed drop is long; the full flat-tyre drop of
//! one tyre is applied to the whole gear although a twin-wheel or multi-leg
//! gear (the A400M's three legs a side) does not drop the side; the flat
//! height is the rim-seat radius, neglecting flange and carcass; the roll is
//! about the opposite main wheel, not the axis from the nose wheel to it (lever
//! 1.49 against about 1.30 on the ATR); the nose case uses the main-wheel
//! station as the pitch axis. All lengthen the drop, so the row is
//! conservative. Not evaluated, and carded rather than described away: the
//! static *taxi* attitude of the first sentence of CS 25.925(a) ("take-off or
//! taxiing attitude, whichever is most critical": the model gives both gear
//! the same strut datum, so its static attitude is level by construction,
//! whereas the ATR table's -1.18 deg lowers the disc by about 0.08 m), and
//! the maximum-weight, most-adverse-CG lead-in beyond the single static plane.
//! The ATR plane is set from the propeller hub and tip, so its static row is
//! not independent evidence; the wing-tip point alone (0.55 m plane) leaves the
//! static row 0.88 m and the tyre-deflated row 0.45 m above their limits.

use alas_config::{ActiveEngineModel, AlasConfig};

use super::surfaces::propeller_clearance_m;
use super::{gap_row, gear_stations, GroundPose};
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// Minimum propeller-to-ground clearance, m: CS 25.925(a), 18 cm (7 in) for a
/// nose-wheel aeroplane, landing gear statically deflected, level take-off
/// attitude (EASA CS-25 Amdt 27, Easy Access Rules p.594; FAR 25.925(a) is
/// the same 7 in).
pub(crate) const PROPELLER_GROUND_CLEARANCE_M: f64 = 0.18;

/// Design vertical sink speed at touchdown, m/s: 10 ft/s, "most aircraft
/// require 10-ft/s {3-m/s} vertical velocity capability" (Raymer ch. 11,
/// stroke determination; the transport-category limit-descent velocity).
const SINK_SPEED_M_S: f64 = 3.048;

/// Gear load factor of the stroke equation: Raymer Table 11.5, commercial
/// 2.7 to 3; the lower end gives the longer stroke, which is the conservative
/// side for the drop.
const GEAR_LOAD_FACTOR: f64 = 2.7;

/// Shock-absorber efficiency: Raymer Table 11.4, oleopneumatic metered
/// orifice 0.75 to 0.90; the lower end gives the longer stroke.
const SHOCK_EFFICIENCY: f64 = 0.75;

/// Stroke safety margin, m: Raymer, "increased by about 1 in. {3 cm}".
const STROKE_MARGIN_M: f64 = 0.0254;

/// Minimum stroke, m: Raymer, "a stroke of 8 in. {20 cm} is usually
/// considered a minimum".
const MINIMUM_STROKE_M: f64 = 0.203;

/// Fraction of the stroke between the static position and fully bottomed:
/// Raymer Fig. 11.10 puts the static position 66 % of the way from fully
/// extended to fully compressed (84 % for large transports, which would leave
/// 0.16); the 0.34 left by 66 % is the conservative side.
const STROKE_BELOW_STATIC_FRACTION: f64 = 0.34;

/// Rim diameter over tyre outer diameter: 20 in over 46 in, the Goodyear
/// 46x17.0R20 main tyre the tyre table cites (`alas-perf` tyre classes). The
/// ratio of the other classes differs by a few hundredths, i.e. a few
/// centimetres of flat-tyre drop.
const RIM_TO_TYRE_DIAMETER: f64 = 20.0 / 46.0;

/// Strut stroke below the static position plus the flat-tyre drop of one
/// gear, m, for a tyre of outer diameter `tyre_diameter_m`.
fn bottomed_flat_drop_m(tyre_diameter_m: f64) -> f64 {
    let stroke = (SINK_SPEED_M_S.powi(2)
        / (2.0 * alas_atmo::STANDARD_GRAVITY * SHOCK_EFFICIENCY * GEAR_LOAD_FACTOR)
        + STROKE_MARGIN_M)
        .max(MINIMUM_STROKE_M);
    STROKE_BELOW_STATIC_FRACTION * stroke + 0.5 * tyre_diameter_m * (1.0 - RIM_TO_TYRE_DIAMETER)
}

/// The propeller rows of a turboprop candidate; none for any other engine.
pub(super) fn propeller_rows(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    pose: &GroundPose,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let Ok(ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model() else {
        return Vec::new();
    };
    let radius_m = 0.5 * spec.propeller_diameter_m;
    // Needs data: the static row is forced level here. CS 25.925(a) asks for the take-off or taxiing attitude, the most critical, at maximum weight and
    // the most adverse CG; that attitude needs each gear's static strut compression at that load (per-strut oleo static pressure and stroke curve, not on
    // disk; Raymer ch. 11 gives the design static position only, not the per-gear load). Until that exists, both gears carry one strut datum and the pose is level.
    let upright = GroundPose {
        pitch_rad: 0.0,
        roll_rad: 0.0,
        ..*pose
    };
    let mut rows = Vec::new();
    let Some(static_m) = propeller_clearance_m(&outcome.plane, &upright, radius_m) else {
        return rows;
    };
    rows.push(gap_row(
        "propeller_ground_clearance",
        static_m - PROPELLER_GROUND_CLEARANCE_M,
        role,
    ));
    if let Some(gap) = deflated_clearance_m(outcome, config, pose, &upright, radius_m) {
        rows.push(gap_row(
            "propeller_ground_clearance_tyre_deflated",
            gap,
            role,
        ));
    }
    rows
}

/// Lowest blade-tip height over every propeller, m, with the critical tyre
/// flat and its strut bottomed (nose gear, or one main gear).
fn deflated_clearance_m(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    pose: &GroundPose,
    upright: &GroundPose,
    radius_m: f64,
) -> Option<f64> {
    let stations = gear_stations(&outcome.plane, config)?;
    let aft_limit_x = outcome.x_np - config.requirements.target_static_margin * outcome.mac;
    let forward_limit_x = aft_limit_x
        - crate::envelope::usable_range::UsableRangeRequirement::of(config)
            .length_m(outcome.mtow_ceiling, outcome.mac);
    let diameter_m = config.geometry.fuselage.diameter_m;
    // Sized with the analysed loading CG, as the report's gear is, so the
    // tyre the clearance uses equals the report's tyre.
    let layout = alas_perf::landing_gear::size_landing_gear_at_design_state(
        outcome.mtow_ceiling,
        None,
        stations.x_nlg_m,
        stations.x_mlg_m,
        forward_limit_x,
        aft_limit_x,
        Some(outcome.cg_x),
        config.mass_model.pct_load_nlg_min,
        alas_perf::landing_gear::geometry::TipBackBasis::default(),
        diameter_m,
        diameter_m * 1.1,
        &stations.main_gear_x_m,
        &config.landing_gear,
    );
    let nose_drop_m = bottomed_flat_drop_m(layout.nlg_tire.diameter_m);
    let main_drop_m = bottomed_flat_drop_m(layout.mlg_tire.diameter_m);
    // A flat nose tyre pitches the airframe nose down, which the pose's
    // nose-up rotation pivot does not describe: the drop is taken about the
    // reaction station, as before the pivot was split from it.
    let x_main_m = stations.x_mlg_m;
    let wheelbase_m = x_main_m - stations.x_nlg_m;
    if !(wheelbase_m > 0.0 && pose.y_outboard_wheel_m > 0.0) {
        return None;
    }
    outcome
        .plane
        .fuselages
        .iter()
        .skip(1)
        .filter(|body| super::is_engine_body(body))
        .filter_map(|body| body.xsecs.first())
        .map(|front| {
            let level =
                upright.height_m([front.xyz_c[0], front.xyz_c[1], front.xyz_c[2] - radius_m]);
            let nose_down = nose_drop_m * (x_main_m - front.xyz_c[0]) / wheelbase_m;
            let roll_down = main_drop_m * (front.xyz_c[1].abs() + pose.y_outboard_wheel_m)
                / (2.0 * pose.y_outboard_wheel_m);
            (level - nose_down).min(level - roll_down)
        })
        .reduce(f64::min)
}
