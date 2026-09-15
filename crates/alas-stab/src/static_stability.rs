// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Analyses.Stability.Fidelity_Zero (static branch) and the
// methods it reaches: Static_Stability/Approximations/datcom.py,
// .../Supporting_Functions/convert_sweep.py, .../Tube_Wing/taw_cmalpha.py,
// .../Tube_Wing/taw_cnbeta.py, .../Supporting_Functions/extend_to_ref_area.py,
// Center_of_Gravity/compute_mission_center_of_gravity.py, and
// Dynamic_Stability/Full_Linearized_Equations/Supporting_Functions/ep_alpha.py.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! mission reference `Fidelity_Zero` static longitudinal/directional stability.
//!
//! [`static_stability`] is `mission reference.Analyses.Stability.Fidelity_Zero.__call__`'s
//! **static branch**: the DATCOM lift-curve slope of the main wing and each
//! wing, the downwash gradient per wing, the tube-and-wing pitching-moment
//! slope (`taw_cmalpha`), the yaw-moment slope (`taw_cnbeta`, present because
//! this program's aircraft always has a `vertical_stabilizer`), and the static
//! margin and neutral point those produce.
//!
//! # Scope: the static branch only
//!
//! Everything after upstream's `if np.count_nonzero(...moments_of_inertia
//! .tensor) > 0:` is the dynamic-stability block, and it is dead code for every
//! aircraft this program builds: the tensor it is gated on is never populated
//! (`docs/PORTING.md`'s Mission section records this independently). It is not
//! translated and is not a `deviation-candidate`, since it can never run.
//!
//! # This is reached from one place, on an assembled vehicle
//!
//! The only constructor of `Fidelity_Zero()` in the reference is
//! `external tools/mission_runner/mission_builder.py:89-91`, which sets
//! `stability.geometry = vehicle`. So this row is scoped, like
//! `alas-mass::torenbeek`, to the fields each function actually reads off that
//! vehicle and off the synthetic `conditions` (a flat [`StaticStabilityInput`])
//! rather than to the whole mission reference `Vehicle`/`Data` shape.
//!
//! # Every centre of gravity in this file is the origin
//!
//! This is the load-bearing finding, and it is why several terms below are
//! hardcoded to `0.0`. `vehicle_builder.py` and `mission_builder.py` never
//! assign *any* of the mass-property vectors these methods read:
//!
//! - **Wing `aerodynamic_center`** defaults to `[0, 0, 0]` (`Wing.py:77`) and
//!   is never set, so `x_ac_surf` (`taw_cmalpha`) and `ac_vLE` (`taw_cnbeta`)
//!   are `0.0`. This is the already-recorded `deviation-candidate`: the static
//!   margin is measured against the wing *origins*, not their aerodynamic
//!   centres, which is why the fixture's static margins are large and
//!   unphysical, faithfully reproducing upstream.
//! - **`compute_mission_center_of_gravity`** (the CG `taw_cmalpha` builds) is a
//!   weighted average of `zero_fuel_center_of_gravity` and the fuel component's
//!   `center_of_gravity + origin`. Neither is ever populated: the first
//!   defaults to `[[0, 0, 0]]` (`Vehicle.py:295`), and `finalize()` supplies a
//!   zero-mass `Physical_Component()` fuel with a `[[0, 0, 0]]` CG and origin
//!   because the vehicle has no `'fuel'` key. So the numerator is identically
//!   the origin *regardless of the masses*, and this CG is `0.0`; its two mass
//!   inputs (`conditions.weights.total_mass`,
//!   `mass_properties.max_zero_fuel`) therefore do not appear in this port.
//! - **`mass_properties.center_of_gravity`**: the vehicle's own design CG,
//!   feeding [`neutral_point`](StaticStabilityResult::neutral_point) and
//!   `taw_cnbeta`'s `x_cg`, is never set either, staying `[[0, 0, 0]]`. It is
//!   carried here as a real [`StaticStabilityInput::cg_x_m`] input (always
//!   `0.0` in every fixture case) so the formula is general, not hardcoded;
//!   a unit test exercises a nonzero value.
//!
//! `Fidelity_Zero.__call__` reads that last vector as `center_of_gravity[0]`
//! (a whole row) where every other reader in mission reference uses `[0][0]` (the scalar
//! x); with `center_of_gravity` never populated its y/z are `0.0` too, so the
//! distinction never shows numerically. That is a harmless latent indexing bug:
//! the same category as CLAUDE.md's `mesh_line` note, and this port takes
//! the scalar x, not the broadcast row.
//!
//! # Other always-default fields, hardcoded with a note
//!
//! `Wing.Airfoil` is never populated, so `taw_cmalpha`'s `al0`/`cmac`
//! (zero-angle lift/moment coefficients) are `0.0`. The vertical tail's
//! `exposed_root_chord_offset` defaults to `0.0` and is never set, so
//! [`extend_to_ref_area`] reduces to an identity for the single, `symmetric =
//! false` fin this program builds, but the general trapezoid formula is
//! translated anyway, and a unit test pins the identity.
//!
//! # Unread `conditions` fields
//!
//! `__call__` unpacks `freestream.dynamic_pressure` (never read again anywhere)
//! and `freestream.velocity`/`.density` (read again only inside the dynamic
//! branch); the static branch's use of velocity, density and viscosity is
//! `taw_cnbeta`'s alone, which reads them off `conditions.freestream` directly.
//! Only those three (plus mach and angle of attack) are carried here.

use std::f64::consts::PI;

/// Roskam Airplane Design Part VI, Table 8.1: the 2D section lift-curve slope
/// at Mach 0, per radian, DATCOM assumes.
const SECTION_CLA_M0: f64 = 6.13;

/// One lifting surface's fields as `datcom`, `ep_alpha` and `taw_cmalpha`'s
/// per-surface loop read them off a mission reference `Wing`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StabilityWing {
    /// `aspect_ratio`.
    pub aspect_ratio: f64,
    /// `sweeps.quarter_chord`, radians.
    pub sweep_quarter_chord_rad: f64,
    /// `taper`.
    pub taper: f64,
    /// `areas.reference`, square metres.
    pub area_ref_m2: f64,
    /// `origin[0][0]`, metres.
    pub origin_x_m: f64,
    /// `dynamic_pressure_ratio` (the tail efficiency `eta`).
    pub dynamic_pressure_ratio: f64,
    /// `vertical`: a vertical surface contributes nothing to `taw_cmalpha`.
    pub vertical: bool,
    /// `twists.root`, radians.
    pub twist_root_rad: f64,
    /// `twists.tip`, radians.
    pub twist_tip_rad: f64,
}

/// The main wing's extra fields, beyond [`StabilityWing`], that the fuselage
/// pitching-moment term, the reference geometry and the neutral point read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainWing {
    /// The fields shared with every surface.
    pub wing: StabilityWing,
    /// `origin[0][2]`, metres: the wing vertical position `taw_cnbeta` reads.
    pub origin_z_m: f64,
    /// `chords.root`, metres.
    pub chord_root_m: f64,
    /// `spans.projected`, metres.
    pub span_m: f64,
    /// `chords.mean_aerodynamic`, metres.
    pub mac_m: f64,
}

/// The vertical stabilizer's extra fields for `taw_cnbeta` and
/// [`extend_to_ref_area`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalStabilizer {
    /// The fields shared with every surface.
    pub wing: StabilityWing,
    /// `origin[0][2]`, metres.
    pub origin_z_m: f64,
    /// `chords.root`, metres.
    pub chord_root_m: f64,
    /// `chords.tip`, metres.
    pub chord_tip_m: f64,
    /// `spans.projected`, metres.
    pub span_m: f64,
    /// `symmetric`: a single centred fin is `false`, halving the reference
    /// extension. This program always builds `false`.
    pub symmetric: bool,
}

/// The fuselage fields `taw_cmalpha`'s body term and `taw_cnbeta`'s fuselage
/// contribution read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fuselage {
    /// `width`, metres.
    pub width_m: f64,
    /// `lengths.total`, metres.
    pub length_m: f64,
    /// `areas.side_projected`, square metres.
    pub side_projected_area_m2: f64,
    /// `heights.maximum`, metres.
    pub height_max_m: f64,
    /// `heights.at_quarter_length`, metres.
    pub height_at_quarter_length_m: f64,
    /// `heights.at_three_quarters_length`, metres.
    pub height_at_three_quarters_length_m: f64,
    /// `heights.at_wing_root_quarter_chord`, metres.
    pub height_at_wing_root_quarter_chord_m: f64,
}

/// Everything `Fidelity_Zero.__call__`'s static branch reads off the vehicle
/// and the flight condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticStabilityInput {
    /// `freestream.mach_number`.
    pub mach: f64,
    /// `aerodynamics.angle_of_attack`, radians.
    pub alpha_rad: f64,
    /// `freestream.velocity`, metres per second (read by `taw_cnbeta`).
    pub velocity_m_s: f64,
    /// `freestream.density`, kilograms per cubic metre (read by `taw_cnbeta`).
    pub density_kg_m3: f64,
    /// `freestream.dynamic_viscosity`, pascal-seconds (read by `taw_cnbeta`).
    pub dynamic_viscosity_pa_s: f64,
    /// `mass_properties.center_of_gravity[0][0]`, metres, always `0.0` in
    /// this program; see the module doc.
    pub cg_x_m: f64,
    /// `reference_area`, square metres.
    pub reference_area_m2: f64,
    /// `wings['main_wing']`.
    pub main_wing: MainWing,
    /// `wings['horizontal_stabilizer']`.
    pub horizontal_stabilizer: StabilityWing,
    /// `wings['vertical_stabilizer']`, if present. `Cn_beta` is `0.0` without
    /// one (upstream's `else: np.zeros_like(mach)`).
    pub vertical_stabilizer: Option<VerticalStabilizer>,
    /// `fuselages['fuselage']`, if present. Absent means no fuselage body term
    /// in either moment slope (upstream's `else: 0.` / empty loop).
    pub fuselage: Option<Fuselage>,
}

/// The static-stability results `Fidelity_Zero.__call__` returns, flattened to
/// scalars (every upstream quantity is a `(1, 1)` array on a single condition).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticStabilityResult {
    /// `conditions.lift_curve_slope`: the main wing's DATCOM slope, per radian.
    pub cl_alpha: f64,
    /// `static.Cm_alpha`: pitching-moment slope `dCm/dalpha`, per radian.
    pub cm_alpha: f64,
    /// `static.Cm0`: zero-alpha pitching moment.
    pub cm0: f64,
    /// `static.CM`: `Cm_alpha * alpha + Cm0` at the given angle of attack.
    pub cm: f64,
    /// `static.Cn_beta`: yaw-moment slope; `0.0` with no vertical stabilizer.
    pub cn_beta: f64,
    /// `static.static_margin`: `-Cm_alpha / cl_alpha`.
    pub static_margin: f64,
    /// `static.neutral_point`: `cg_x + mac * static_margin`.
    pub neutral_point: f64,
}

/// The DATCOM 3D lift-curve slope `dCL/dalpha` [per rad] for a subsonic
/// trapezoidal surface: `datcom`. `aspect_ratio` is passed explicitly so the
/// caller can supply the vertical tail's *extended* value (see the module doc);
/// every other input is the surface's own.
pub fn datcom(aspect_ratio: f64, sweep_quarter_chord_rad: f64, taper: f64, mach: f64) -> f64 {
    let half_chord_sweep =
        convert_sweep_quarter_to_half(sweep_quarter_chord_rad, taper, aspect_ratio);

    // Upstream initialises Beta and cla_M to ones and overwrites only the
    // `mach < 1` / `mach > 1` masks, so `mach == 1` keeps them at 1.0.
    let (beta, cla_m) = if mach < 1.0 {
        let beta = (1.0 - mach * mach).sqrt();
        (beta, SECTION_CLA_M0 / beta)
    } else if mach > 1.0 {
        let beta = (mach * mach - 1.0).sqrt();
        (beta, 4.0 / beta)
    } else {
        (1.0, 1.0)
    };
    let k = cla_m / (2.0 * PI / beta);

    let radicand = (aspect_ratio * aspect_ratio * beta * beta / (k * k))
        * (1.0 + half_chord_sweep.tan().powi(2) / (beta * beta))
        + 4.0;
    2.0 * PI * aspect_ratio / (2.0 + radicand.sqrt())
}

/// Convert a quarter-chord sweep to half-chord sweep, `convert_sweep(wing,
/// 0.25, 0.5)`, the one call DATCOM makes. The `old_ref_chord_fraction == 0.0`
/// branch (which reads `sweeps.leading_edge`) is never reached from here, so it
/// is not translated. Radians in and out.
fn convert_sweep_quarter_to_half(
    sweep_quarter_chord_rad: f64,
    taper: f64,
    aspect_ratio: f64,
) -> f64 {
    let term = 4.0 * (1.0 - taper) / (aspect_ratio * (1.0 + taper));
    // old = 0.25 (quarter chord) -> leading edge -> new = 0.5 (half chord).
    let sweep_le = (sweep_quarter_chord_rad.tan() + 0.25 * term).atan();
    (sweep_le.tan() - 0.5 * term).atan()
}

/// The downwash gradient `d(epsilon)/d(alpha)`: `ep_alpha`. Blakelock's
/// `2 CL_alpha / (pi * (span^2 / Sref))`, with `span = sqrt(AR * Sref)` as the
/// caller (`Fidelity_Zero.__call__`'s loop) derives it, not the projected span.
pub fn ep_alpha(cl_alpha: f64, area_ref: f64, aspect_ratio: f64) -> f64 {
    let span = (aspect_ratio * area_ref).sqrt();
    2.0 * cl_alpha / PI / (span * span / area_ref)
}

/// The extended-to-centreline reference trapezoid of a vertical tail:
/// `extend_to_ref_area`, returning `(aspect_ratio, area, span, root_le_shift)`.
/// `exposed_root_chord_offset` is `0.0` (never set; see the module doc), so for
/// the single `symmetric = false` fin this reduces to the surface's own values,
/// but the general formula is translated. `spans.exposed` is absent, so the
/// projected-span branch is the one taken.
pub fn extend_to_ref_area(
    span_m: f64,
    chord_root_m: f64,
    chord_tip_m: f64,
    sweep_quarter_chord_rad: f64,
    symmetric: bool,
) -> (f64, f64, f64, f64) {
    let symm = if symmetric { 1.0 } else { 0.0 };
    // `Wing.exposed_root_chord_offset` default; never overridden.
    let exposed_root_chord_offset = 0.0;

    let b1 = span_m * 0.5 * (2.0 - symm);
    let b = b1 + exposed_root_chord_offset;
    let c_root = chord_tip_m + (b / b1) * (chord_root_m - chord_tip_m);
    let s = 0.5 * b * (c_root + chord_tip_m);
    let dx_le = -exposed_root_chord_offset * sweep_quarter_chord_rad.tan();
    let ar = b * b / s;

    (ar * (1.0 + symm), s * (1.0 + symm), b * (1.0 + symm), dx_le)
}

/// The tube-and-wing pitching-moment slope and zero-alpha moment
/// `(Cm_alpha, Cm0)`: `taw_cmalpha`, minus the `CM` term (the caller forms
/// it). `cl_alpha` values per surface are `datcom`; `x_ac_surf` and the mission
/// CG are `0.0` (see the module doc). `surfaces` is main, horizontal
/// stabilizer, then vertical stabilizer: upstream's `wings` order.
fn taw_cmalpha(input: &StaticStabilityInput, surfaces: &[StabilityWing]) -> (f64, f64) {
    let s_ref = input.reference_area_m2;
    let mac = input.main_wing.mac_m;

    let cm_alpha_body = match input.fuselage {
        Some(fus) => {
            let w_f = fus.width_m;
            let l_f = fus.length_m;
            let sweep = input.main_wing.wing.sweep_quarter_chord_rad;
            let x_rqc = input.main_wing.wing.origin_x_m
                + 0.5 * w_f * sweep.tan()
                + 0.25
                    * input.main_wing.chord_root_m
                    * (1.0 - (w_f / input.main_wing.span_m) * (1.0 - input.main_wing.wing.taper));
            let p = x_rqc / l_f;
            let kf = 1.5012 * p * p + 0.538 * p + 0.0331;
            kf * w_f * w_f * l_f / s_ref / mac
        }
        None => 0.0,
    };

    let mut cm_alpha_surf = 0.0;
    let mut cm0_surf = 0.0;
    for surf in surfaces {
        let s = surf.area_ref_m2;
        let eta = surf.dynamic_pressure_ratio;
        let cl_alpha = datcom(
            surf.aspect_ratio,
            surf.sweep_quarter_chord_rad,
            surf.taper,
            input.mach,
        );
        let downw = 1.0 - ep_alpha(cl_alpha, s, surf.aspect_ratio);
        let not_vertical = if surf.vertical { 0.0 } else { 1.0 };

        // `Airfoil` is never populated, so al0 = cmac = 0 (see the module doc).
        let cl0_surf = cl_alpha * (surf.twist_root_rad + surf.taper * surf.twist_tip_rad) / 2.0;

        // `aerodynamic_center` and the mission CG are both the origin.
        let l_surf = surf.origin_x_m + 0.0 - 0.0;
        cm_alpha_surf += -l_surf * s / (mac * s_ref) * (cl_alpha * eta * downw) * not_vertical;
        cm0_surf += s * eta * cl0_surf * l_surf * downw * not_vertical / (mac * s_ref);
    }

    (cm_alpha_surf + cm_alpha_body, cm0_surf)
}

/// The tube-and-wing yaw-moment slope `Cn_beta`: `taw_cnbeta`. Reads the
/// vertical tail through [`extend_to_ref_area`], adds the fuselage
/// contribution, and takes `ac_vLE` and `x_cg` from the module doc's zero
/// findings (`x_cg` is the caller's `cg_x`, always `0.0`). Upstream reads the
/// fuselage in a loop and then again for `d_i`/`h_max`, so with no fuselage it
/// would `NameError`; this program always supplies one, and its absence yields
/// `NaN` here rather than a panic.
fn taw_cnbeta(input: &StaticStabilityInput, vstab: &VerticalStabilizer) -> f64 {
    let s = input.reference_area_m2;
    let b = input.main_wing.span_m;
    let ar = input.main_wing.wing.aspect_ratio;
    let z_w = input.main_wing.origin_z_m;
    let x_cg = input.cg_x_m;

    let (extended_ar, s_v, b_v, dx_le) = extend_to_ref_area(
        vstab.span_m,
        vstab.chord_root_m,
        vstab.chord_tip_m,
        vstab.wing.sweep_quarter_chord_rad,
        vstab.symmetric,
    );
    let x_v = vstab.wing.origin_x_m + dx_le;
    // `vert.aerodynamic_center[0]`, never populated (see the module doc).
    let ac_vle = 0.0;

    let Some(fus) = input.fuselage else {
        return f64::NAN;
    };
    let h_max = fus.height_max_m;
    let d_i = fus.height_at_wing_root_quarter_chord_m;

    let re_fuse =
        input.density_kg_m3 * input.velocity_m_s * fus.length_m / input.dynamic_viscosity_pa_s;
    let x1 = x_cg / fus.length_m;
    let x2 = fus.length_m * fus.length_m / fus.side_projected_area_m2;
    let x3 = (fus.height_at_quarter_length_m / fus.height_at_three_quarters_length_m).sqrt();
    let x4 = h_max / fus.width_m;
    let kn_1 = 3.2413 * x1 - 0.663_345 + 6.1086 * (-0.22 * x2).exp();
    let kn_2 = (-0.2023 + 1.3422 * x3 - 0.1454 * x3 * x3) * kn_1;
    let kn_3 = 0.7870 + 0.1038 * x4 + 0.1834 * x4 * x4 - 2.811 * (-4.0 * x4).exp();
    let k_n = (-0.47899 + kn_3 * kn_2) * 0.001;
    let k_rel = 1.0 + 0.8 * (re_fuse / 1.0e6).ln() / 50.0_f64.ln();
    let fuse_cnb = -57.3 * k_n * k_rel * fus.side_projected_area_m2 * fus.length_m / s / b;

    let l_v = x_v + ac_vle - x_cg;
    // `datcom(vert, M)` reads the *extended* aspect ratio but the fin's own
    // (unmodified) taper: `extend_to_ref_area` does not touch `taper`.
    let cla_v = datcom(
        extended_ar,
        vstab.wing.sweep_quarter_chord_rad,
        vstab.wing.taper,
        input.mach,
    );

    let bf = b_v / d_i;
    let k_v = if bf < 2.0 {
        0.76
    } else if bf < 3.5 {
        0.76 + 0.24 * (bf - 2.0) / 1.5
    } else {
        1.0
    };

    // `main_wing.sweeps.quarter_chord` is always set (never `None`), so the
    // `convert_sweep` fallback branch is unreached and not translated.
    let quarter_chord_sweep = input.main_wing.wing.sweep_quarter_chord_rad;
    let k_sweep = 1.0 + quarter_chord_sweep.cos();
    let dsdb_e = 0.724 + 3.06 * ((s_v / s) / k_sweep) + 0.4 * z_w / h_max + 0.009 * ar;
    let cy_bv = -k_v * cla_v * dsdb_e * (s_v / s);
    let cn_beta_v = -cy_bv * l_v / b;

    // CnBeta_w is 0.0 upstream (the wing contribution is assumed negligible
    // except at very high angles of attack).
    cn_beta_v + fuse_cnb
}

/// The full static-stability branch of `Fidelity_Zero.__call__`.
pub fn static_stability(input: &StaticStabilityInput) -> StaticStabilityResult {
    let m = &input.main_wing.wing;
    let cl_alpha = datcom(
        m.aspect_ratio,
        m.sweep_quarter_chord_rad,
        m.taper,
        input.mach,
    );

    // `for surf in geometry.wings`: main, horizontal stabilizer, vertical
    // stabilizer, in that order.
    let mut surfaces = vec![input.main_wing.wing, input.horizontal_stabilizer];
    if let Some(v) = &input.vertical_stabilizer {
        surfaces.push(v.wing);
    }

    let (cm_alpha, cm0) = taw_cmalpha(input, &surfaces);
    let cm = cm_alpha * input.alpha_rad + cm0;

    let cn_beta = match &input.vertical_stabilizer {
        Some(v) => taw_cnbeta(input, v),
        None => 0.0,
    };

    let static_margin = -cm_alpha / cl_alpha;
    let neutral_point = input.cg_x_m + input.main_wing.mac_m * static_margin;

    StaticStabilityResult {
        cl_alpha,
        cm_alpha,
        cm0,
        cm,
        cn_beta,
        static_margin,
        neutral_point,
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn probe_vstab() -> VerticalStabilizer {
        VerticalStabilizer {
            wing: StabilityWing {
                aspect_ratio: 1.5,
                sweep_quarter_chord_rad: 0.74,
                taper: 0.34,
                area_ref_m2: 62.0,
                origin_x_m: 38.0,
                dynamic_pressure_ratio: 1.0,
                vertical: true,
                twist_root_rad: 0.0,
                twist_tip_rad: 0.0,
            },
            origin_z_m: 2.0,
            chord_root_m: 9.5,
            chord_tip_m: 3.2,
            span_m: 9.8,
            symmetric: false,
        }
    }

    fn probe_fuselage() -> Fuselage {
        Fuselage {
            width_m: 6.2,
            length_m: 76.72,
            side_projected_area_m2: 428.0,
            height_max_m: 6.2,
            height_at_quarter_length_m: 6.2,
            height_at_three_quarters_length_m: 5.77,
            height_at_wing_root_quarter_chord_m: 6.2,
        }
    }

    fn probe_input() -> StaticStabilityInput {
        StaticStabilityInput {
            mach: 0.4,
            alpha_rad: 0.02,
            velocity_m_s: 131.0,
            density_kg_m3: 0.9,
            dynamic_viscosity_pa_s: 1.69e-5,
            cg_x_m: 0.0,
            reference_area_m2: 529.0,
            main_wing: MainWing {
                wing: StabilityWing {
                    aspect_ratio: 9.89,
                    sweep_quarter_chord_rad: 0.593,
                    taper: 0.097,
                    area_ref_m2: 529.0,
                    origin_x_m: 26.24,
                    dynamic_pressure_ratio: 1.0,
                    vertical: false,
                    twist_root_rad: 0.07,
                    twist_tip_rad: 0.0,
                },
                origin_z_m: -2.1,
                chord_root_m: 16.5,
                span_m: 71.75,
                mac_m: 9.63,
            },
            horizontal_stabilizer: StabilityWing {
                aspect_ratio: 4.33,
                sweep_quarter_chord_rad: 0.598,
                taper: 0.275,
                area_ref_m2: 112.6,
                origin_x_m: 36.94,
                dynamic_pressure_ratio: 0.9,
                vertical: false,
                twist_root_rad: -0.035,
                twist_tip_rad: -0.035,
            },
            vertical_stabilizer: Some(probe_vstab()),
            fuselage: Some(probe_fuselage()),
        }
    }

    #[test]
    fn datcom_increases_with_aspect_ratio() {
        // The DATCOM formula is monotone in aspect ratio: a slenderer wing has
        // a steeper 3D lift-curve slope.
        let low = datcom(4.0, 0.4, 0.3, 0.4);
        let high = datcom(10.0, 0.4, 0.3, 0.4);
        assert!(high > low, "low={low} high={high}");
    }

    #[test]
    fn datcom_is_bounded_by_the_slender_wing_limit() {
        // 2*pi*AR/(2+...) never reaches the 2D limit; it stays below 2*pi*AR/2.
        let ar = 8.0;
        let cla = datcom(ar, 0.5, 0.3, 0.3);
        assert!(cla < PI * ar, "cla={cla}");
        assert!(cla > 0.0, "cla={cla}");
    }

    #[test]
    fn ep_alpha_matches_the_closed_form() {
        // 2 CL_alpha / (pi * AR), since span^2/Sref = AR.
        let cla = 5.0;
        let ar = 9.0;
        let expected = 2.0 * cla / (PI * ar);
        assert!((ep_alpha(cla, 100.0, ar) - expected).abs() < 1e-15);
    }

    #[test]
    fn extend_to_ref_area_is_identity_for_a_single_offset_free_fin() {
        // symmetric = false, exposed_root_chord_offset = 0: the extended
        // trapezoid is the fin itself.
        let vstab = probe_vstab();
        let (ar, area, span, dx) = extend_to_ref_area(
            vstab.span_m,
            vstab.chord_root_m,
            vstab.chord_tip_m,
            vstab.wing.sweep_quarter_chord_rad,
            vstab.symmetric,
        );
        let area_direct = 0.5 * vstab.span_m * (vstab.chord_root_m + vstab.chord_tip_m);
        assert!((area - area_direct).abs() < 1e-12, "area={area}");
        assert!((span - vstab.span_m).abs() < 1e-12, "span={span}");
        assert!(
            (ar - vstab.span_m * vstab.span_m / area_direct).abs() < 1e-12,
            "ar={ar}"
        );
        assert_eq!(dx, 0.0);
    }

    #[test]
    fn no_vertical_stabilizer_gives_zero_cn_beta() {
        let mut input = probe_input();
        input.vertical_stabilizer = None;
        let result = static_stability(&input);
        assert_eq!(result.cn_beta, 0.0);
    }

    #[test]
    fn cm_reduces_to_cm0_at_zero_alpha() {
        let mut input = probe_input();
        input.alpha_rad = 0.0;
        let result = static_stability(&input);
        assert_eq!(result.cm, result.cm0);
    }

    #[test]
    fn neutral_point_tracks_a_nonzero_cg() {
        // What the fixture cannot see: cg_x is always 0.0 there, but the
        // formula is neutral_point = cg_x + mac*static_margin, so shifting cg
        // by delta shifts the neutral point by exactly delta.
        let base = static_stability(&probe_input());
        let mut shifted_input = probe_input();
        shifted_input.cg_x_m = 3.0;
        let shifted = static_stability(&shifted_input);
        assert!(
            (shifted.neutral_point - base.neutral_point - 3.0).abs() < 1e-12,
            "delta={}",
            shifted.neutral_point - base.neutral_point
        );
    }

    #[test]
    fn a_vertical_surface_contributes_nothing_to_cm_alpha() {
        // taw_cmalpha multiplies each surface by (1 - vertical); flipping the
        // vertical stabilizer's flag must not change Cm_alpha (it is already
        // excluded), while the main wing and tail carry it.
        let input = probe_input();
        let with_fin = static_stability(&input);
        let mut no_fin = input;
        no_fin.vertical_stabilizer = None;
        let without_fin = static_stability(&no_fin);
        assert!(
            (with_fin.cm_alpha - without_fin.cm_alpha).abs() < 1e-12,
            "with={} without={}",
            with_fin.cm_alpha,
            without_fin.cm_alpha
        );
    }
}
