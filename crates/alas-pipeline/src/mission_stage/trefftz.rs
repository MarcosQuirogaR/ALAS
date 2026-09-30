// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Far-field (Trefftz-plane) inviscid induced drag for the product mission
//! lattice.
//!
//! # Why the near-field value is replaced
//!
//! The mission surrogate is trained on the SUAVE 2.5.2 port of VORLAX
//! (`alas_aero::vorlax`). Its induced drag is a *near-field* sum: the
//! streamwise component of every strip's normal force, less a leading-edge
//! suction force estimated from the first chordwise panel's load. VORLAX
//! itself evaluates that suction only for cosine chordwise spacing; SUAVE
//! forces the calculation on linear spacing, "on the recorded grounds that
//! the trend is right even though the magnitude is understated" (see the
//! module documentation of `alas_aero::vorlax::forces`). Understated suction
//! is overstated induced drag. On a flat, untwisted, unswept AR-12 wing at
//! Mach 0.3 the port returns `CL^2 / (pi AR CDi)` = 0.38 (rectangular) and
//! 0.39-0.41 (taper 0.5, 15x5 to 40x8 lattice), where lifting-line theory
//! gives about 0.97-0.99; with the suction switched off it gives 0.14, so
//! the suction the port recovers is about 70 % of what the loading needs.
//! Refining the lattice does not converge it. That is the cause of the
//! mission's low effective span efficiency (0.41-0.55 on the ATR, against
//! the polar's lattice): a discretisation-dependent suction estimate, not a
//! physical loss.
//!
//! # What replaces it
//!
//! The same solved circulation, integrated in the Trefftz plane far
//! downstream, where the induced drag is the kinetic energy the trailing
//! vortex system leaves per unit length (Munk, *The Minimum Induced Drag of
//! Aerofoils*, NACA Report 121, 1921; Drela, *Flight Vehicle Aerodynamics*,
//! MIT Press, 2014, Trefftz-plane far-field drag):
//!
//! `D_i = -(rho / 2) sum_i Gamma_i (V_n)_i l_i`,
//!
//! where each strip's bound circulation is `Gamma = V c c_l / 2`
//! (Kutta-Joukowski on the strip's own lift), each strip sheds its
//! horseshoe legs as two-dimensional point vortices at its edges, `V_n` is
//! the wash those vortices induce normal to the strip at its midpoint, and
//! `l` is the strip's span in the Trefftz plane. The far field needs no
//! leading-edge model: it is the kinetic energy the wake leaves behind,
//! which is what induced drag is. Nothing in it is fitted. Every surface's
//! wake acts on every other surface, so the wing's downwash on the tail and
//! the tail's download are both counted; per-surface drag is each surface's
//! own `Gamma` times the total wash it sits in, which sums exactly to the
//! aircraft value.
//!
//! # The fuselage-carried lift
//!
//! The mission flies the aircraft lift `1.14 x` the lattice's (SUAVE
//! Fidelity-Zero `fuselage_lift_correction`,
//! [`alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION`], from the
//! Stanford AA241 notes). That extra lift is carried across the body by the
//! wing-body span loading, whose wake is shed from the same span the lattice
//! already covers (the lattice wing runs through the centreline). So the
//! aircraft sheds the lattice's vorticity scaled by 1.14, and because the
//! Trefftz drag is a quadratic form in the circulation, its induced drag is
//! the lattice's times `1.14^2`: the span efficiency belongs to the loading
//! shape, not its magnitude. The tables therefore store that aircraft
//! value, and the lift the viscous and compressibility terms read stays the
//! lattice's, which is the lift the wing sections carry. Without this the
//! mission would fly 14 % more lift than it pays induced drag for, a span
//! efficiency 30 % above the lattice's own.
//!
//! Linear-theory assumptions, stated rather than hidden: the wake leaves
//! each strip along the body `x` axis without roll-up (the lattice's own
//! wake model), so the Trefftz plane sees the strips at their body `y`, `z`
//! positions; the Prandtl-Glauert stretching is streamwise, so the
//! crossflow plane and this integral are unchanged by Mach number, and the
//! compressibility effect enters through the solved circulation. A vertical
//! surface carries no load at zero sideslip on a symmetric aircraft, which
//! is the only condition the mission trains at, so it is omitted.
//!
//! Numerical settings: the lattice is SUAVE's default (15 cosine-spaced
//! strips per surface side, 5 chordwise panels) and the wash is taken at each
//! strip's midpoint. That quadrature converges at first order: on a flat AR-12
//! wing it overstates the span efficiency by about 4 % at 15 strips, 2 % at
//! 30 and 1 % at 60, towards lifting-line theory (tested). The mission's
//! inviscid induced drag is therefore low by about that much.
//!
//! Product mode only: the frozen SUAVE reference path trains the upstream
//! surrogate unchanged, so its fixtures stay bit-identical.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION;
use alas_aero::lift_surrogate::{LiftSurrogate, TrainingGrid, TrainingTables};
use alas_aero::vorlax::VortexDistribution;
use alas_aero::vorlax::{self, VlmCaseResult, VlmCondition, VlmGeometry, VlmSettings};

use super::MissionReferenceMode;

/// The mission lift surrogate: SUAVE's own in the frozen path, and the same
/// lattice with far-field induced drag in Product mode.
pub(super) fn mission_lift_surrogate(
    geometry: &VlmGeometry,
    reference_mode: MissionReferenceMode,
) -> Result<LiftSurrogate, String> {
    let settings = VlmSettings::default();
    let grid = TrainingGrid::default();
    match reference_mode {
        MissionReferenceMode::ReferenceCompatibility => {
            LiftSurrogate::train(geometry, &settings, &grid)
                .map_err(|error| format!("mission lift surrogate failed: {error}"))
        }
        MissionReferenceMode::Product => trefftz_surrogate(geometry, &settings, &grid),
    }
}

/// Train the surrogate with the lift tables exactly as
/// [`LiftSurrogate::train`] forms them and the induced-drag tables from the
/// Trefftz plane.
fn trefftz_surrogate(
    geometry: &VlmGeometry,
    settings: &VlmSettings,
    grid: &TrainingGrid,
) -> Result<LiftSurrogate, String> {
    let n_alpha = grid.angle_of_attack_rad.len();
    let n_mach = grid.mach.len();
    // Mach-major, as `LiftSurrogate::train` flattens it.
    let conditions: Vec<VlmCondition> = grid
        .mach
        .iter()
        .flat_map(|&mach| {
            grid.angle_of_attack_rad
                .iter()
                .map(move |&angle_of_attack_rad| VlmCondition {
                    angle_of_attack_rad,
                    mach,
                    side_slip_angle_rad: 0.0,
                    pitch_rate_rad_s: 0.0,
                    roll_rate_rad_s: 0.0,
                    yaw_rate_rad_s: 0.0,
                    velocity_m_s: 0.0,
                })
        })
        .collect();
    let results = vorlax::run(geometry, settings, &conditions)
        .map_err(|error| format!("mission lift surrogate failed: {error}"))?;
    let vd = &results.distribution;
    let areas = &vd.wing_areas_m2;
    let vertical = surface_is_vertical(geometry);
    // The aircraft's loading is the lattice's times the fuselage-lift
    // correction (module documentation), so its induced drag is the square.
    let aircraft_scale = FUSELAGE_LIFT_CORRECTION * FUSELAGE_LIFT_CORRECTION;
    let drag_areas: Vec<Vec<f64>> = results
        .cases
        .iter()
        .map(|case| {
            trefftz_drag_areas_m2(vd, case, &vertical)
                .map(|areas| areas.iter().map(|area| aircraft_scale * area).collect())
        })
        .collect::<Result<_, _>>()?;

    let reshape = |flat: &dyn Fn(usize) -> f64| -> Vec<Vec<f64>> {
        (0..n_alpha)
            .map(|i| (0..n_mach).map(|j| flat(j * n_alpha + i)).collect())
            .collect()
    };
    let lift_table = reshape(&|k| results.cases[k].cl);
    let drag_table = reshape(&|k| drag_areas[k].iter().sum::<f64>() / geometry.reference_area_m2);
    let mut wing_lift_tables = BTreeMap::new();
    let mut wing_drag_tables = BTreeMap::new();
    let mut wing_tags = Vec::with_capacity(geometry.wings.len());
    let mut surface = 0usize;
    for wing in &geometry.wings {
        let count = if wing.symmetric { 2 } else { 1 };
        let sides = surface..surface + count;
        let lift = reshape(&|k| {
            sides
                .clone()
                .map(|s| results.cases[k].cl_wing[s] * f64::from(areas[s]))
                .sum::<f64>()
                / wing.area_reference_m2
        });
        let drag = reshape(&|k| {
            sides.clone().map(|s| drag_areas[k][s]).sum::<f64>() / wing.area_reference_m2
        });
        wing_lift_tables.insert(wing.tag.clone(), lift);
        wing_drag_tables.insert(wing.tag.clone(), drag);
        wing_tags.push(wing.tag.clone());
        surface += count;
    }
    LiftSurrogate::from_training(
        grid,
        &wing_tags,
        &TrainingTables {
            lift_coefficient: lift_table,
            drag_coefficient: drag_table,
            wing_lift_coefficient: wing_lift_tables,
            wing_drag_coefficient: wing_drag_tables,
        },
    )
    .map_err(|error| format!("mission lift surrogate failed: {error}"))
}

/// Whether each lattice surface (one side of a wing) is vertical, in the
/// order the lattice holds them: a symmetric wing contributes two.
fn surface_is_vertical(geometry: &VlmGeometry) -> Vec<bool> {
    geometry
        .wings
        .iter()
        .flat_map(|wing| {
            let count = if wing.symmetric { 2 } else { 1 };
            std::iter::repeat_n(wing.vertical, count)
        })
        .collect()
}

/// One loaded strip as the Trefftz plane sees it.
struct TrefftzStrip {
    surface: usize,
    /// The `(y, z)` of the edge with the smaller `y`, then the larger, m.
    edges: [[f64; 2]; 2],
    /// Bound circulation over freestream speed, `c c_l / 2`, m.
    circulation_m: f64,
}

/// Each surface's induced drag times the vehicle's reference area, m^2, at
/// one solved condition: `C_Di,s S = -sum_strips (Gamma/V) (V_n/V) l`.
fn trefftz_drag_areas_m2(
    vd: &VortexDistribution,
    case: &VlmCaseResult,
    vertical: &[bool],
) -> Result<Vec<f64>, String> {
    let leading = vd.leading_edge_panels();
    let n_strips = leading.len();
    if case.cl_y.len() != n_strips || vertical.len() != vd.n_w {
        return Err("mission lattice strips do not match its solution".to_owned());
    }
    let panels = &vd.panels;
    let mut strips = Vec::with_capacity(n_strips);
    for (surface, &is_vertical) in vertical.iter().enumerate() {
        if is_vertical {
            continue;
        }
        let first = vd.spanwise_breaks[surface];
        let last = vd
            .spanwise_breaks
            .get(surface + 1)
            .copied()
            .unwrap_or(n_strips);
        for (strip, &panel) in leading.iter().enumerate().take(last).skip(first) {
            let a = [f64::from(panels.yah[panel]), f64::from(panels.zah[panel])];
            let b = [f64::from(panels.ybh[panel]), f64::from(panels.zbh[panel])];
            let edges = if a[0] <= b[0] { [a, b] } else { [b, a] };
            strips.push(TrefftzStrip {
                surface,
                edges,
                circulation_m: 0.5 * f64::from(vd.chord_lengths_m[panel]) * case.cl_y[strip],
            });
        }
    }

    let mut drag_areas = vec![0.0; vd.n_w];
    for strip in &strips {
        let [left, right] = strip.edges;
        let span = [right[0] - left[0], right[1] - left[1]];
        let length = span[0].hypot(span[1]);
        if length <= 0.0 {
            continue;
        }
        // Upward normal of a strip whose edges run towards +y.
        let normal = [-span[1] / length, span[0] / length];
        let midpoint = [0.5 * (left[0] + right[0]), 0.5 * (left[1] + right[1])];
        // With x aft, y to starboard and z up, a lifting horseshoe's bound
        // vorticity points along +y, so its leg at the larger y trails +Gamma
        // along +x (downstream) and its leg at the smaller y -Gamma, on either
        // side of the aircraft; a +x point vortex at `q` induces
        // `Gamma / (2 pi r^2) (-(z - z_q), y - y_q)` at `(y, z)`.
        let mut wash = [0.0; 2];
        for source in &strips {
            for (edge, sign) in [(source.edges[1], 1.0), (source.edges[0], -1.0)] {
                let dy = midpoint[0] - edge[0];
                let dz = midpoint[1] - edge[1];
                let r2 = dy * dy + dz * dz;
                // A leg through the point itself induces nothing there by
                // symmetry; this is reached only on an exact coincidence.
                if r2 > 0.0 {
                    let strength = sign * source.circulation_m / (2.0 * PI * r2);
                    wash[0] -= strength * dz;
                    wash[1] += strength * dy;
                }
            }
        }
        let normal_wash = wash[0] * normal[0] + wash[1] * normal[1];
        drag_areas[strip.surface] -= strip.circulation_m * normal_wash * length;
    }
    if drag_areas.iter().all(|value| value.is_finite()) {
        Ok(drag_areas)
    } else {
        Err("the mission lattice's Trefftz-plane induced drag is not finite".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_aero::vorlax::VlmWing;

    fn planar_wing(taper: f64, aspect_ratio: f64) -> VlmGeometry {
        let area_m2 = 61.0;
        let span_m = (area_m2 * aspect_ratio).sqrt();
        let root_m = 2.0 * area_m2 / (span_m * (1.0 + taper));
        VlmGeometry {
            reference_area_m2: area_m2,
            center_of_gravity_m: [0.0; 3],
            mean_aerodynamic_chord_m: root_m,
            reference_span_m: span_m,
            moment_reference_m: [0.0, 0.0],
            wings: vec![VlmWing {
                tag: "main_wing".to_owned(),
                symmetric: true,
                vertical: false,
                vortex_lift: false,
                span_projected_m: span_m,
                chord_root_m: root_m,
                chord_tip_m: root_m * taper,
                taper,
                aspect_ratio,
                sweep_quarter_chord_rad: 0.0,
                sweep_leading_edge_rad: None,
                twist_root_rad: 0.0,
                twist_tip_rad: 0.0,
                dihedral_rad: 0.0,
                area_reference_m2: area_m2,
                origin_m: [0.0; 3],
            }],
        }
    }

    fn condition(alpha_deg: f64) -> VlmCondition {
        VlmCondition {
            angle_of_attack_rad: alpha_deg.to_radians(),
            mach: 0.3,
            side_slip_angle_rad: 0.0,
            pitch_rate_rad_s: 0.0,
            roll_rate_rad_s: 0.0,
            yaw_rate_rad_s: 0.0,
            velocity_m_s: 100.0,
        }
    }

    /// The strip circulations integrate back to the lattice's own lift, so
    /// the Trefftz integral is fed the loading the lattice solved.
    #[test]
    fn strip_circulation_reproduces_the_lattice_lift() {
        let geometry = planar_wing(0.5, 12.0);
        let results = vorlax::run(&geometry, &VlmSettings::default(), &[condition(4.0)])
            .unwrap_or_else(|error| panic!("{error}"));
        let vd = &results.distribution;
        let case = &results.cases[0];
        let leading = vd.leading_edge_panels();
        let lift_area: f64 = leading
            .iter()
            .enumerate()
            .map(|(strip, &panel)| {
                let width = f64::from(vd.panels.ybh[panel] - vd.panels.yah[panel]).abs();
                f64::from(vd.chord_lengths_m[panel]) * case.cl_y[strip] * width
            })
            .sum();
        let cl = lift_area / geometry.reference_area_m2;
        assert!((cl / case.cl - 1.0).abs() < 1e-3, "{cl} vs {}", case.cl);
    }

    /// The Trefftz-plane span efficiency of a flat, unswept, untwisted wing
    /// at AR 12 on the lattice the frozen port uses.
    fn span_efficiencies(taper: f64, spanwise: usize, chordwise: usize) -> (f64, f64) {
        let geometry = planar_wing(taper, 12.0);
        let settings = VlmSettings {
            number_spanwise_vortices: spanwise,
            number_chordwise_vortices: chordwise,
            ..VlmSettings::default()
        };
        let results = vorlax::run(&geometry, &settings, &[condition(4.0)])
            .unwrap_or_else(|error| panic!("{error}"));
        let case = &results.cases[0];
        let areas =
            trefftz_drag_areas_m2(&results.distribution, case, &surface_is_vertical(&geometry))
                .unwrap_or_else(|error| panic!("{error}"));
        let cdi = areas.iter().sum::<f64>() / geometry.reference_area_m2;
        let span_efficiency = |drag: f64| case.cl * case.cl / (PI * 12.0 * drag);
        (span_efficiency(cdi), span_efficiency(case.cdi))
    }

    /// Lifting-line theory gives about 0.99 for taper 0.3-0.5 and about 0.94
    /// for a rectangular planform at AR 12 (Glauert's delta; Anderson,
    /// *Fundamentals of Aerodynamics*, Fig. 5.20). The Trefftz value converges
    /// there as the lattice is refined, with the midpoint rule's first-order
    /// error: at the 15-strip lattice the mission trains on it overstates the
    /// span efficiency by about 4 % (induced drag low by the same), halving
    /// with every doubling. The frozen near-field value stays near 0.4 at
    /// every density, which is the defect this replaces.
    #[test]
    fn trefftz_span_efficiency_converges_to_lifting_line() {
        for (taper, lifting_line) in [(0.3, 0.99), (0.5, 0.99), (1.0, 0.94)] {
            let (coarse, coarse_near_field) = span_efficiencies(taper, 15, 5);
            let (medium, _) = span_efficiencies(taper, 30, 8);
            let (fine, fine_near_field) = span_efficiencies(taper, 60, 8);
            assert!(
                coarse > medium && medium > fine,
                "taper {taper}: {coarse} {medium} {fine}"
            );
            // First-order convergence: the step halves with the spacing.
            let ratio = (coarse - medium) / (medium - fine);
            assert!((1.5..3.0).contains(&ratio), "taper {taper}: ratio {ratio}");
            let extrapolated = 2.0 * fine - medium;
            assert!(
                (extrapolated - lifting_line).abs() < 0.015,
                "taper {taper}: extrapolated {extrapolated}"
            );
            assert!(
                coarse / extrapolated - 1.0 < 0.045,
                "taper {taper}: {coarse}"
            );
            assert!(coarse_near_field < 0.45 && fine_near_field < 0.45);
        }
    }

    /// Linear interpolation in a polar sweep column at one lift coefficient.
    fn at_polar_cl(cl: &[f64], column: &[f64], target: f64) -> f64 {
        cl.windows(2)
            .zip(column.windows(2))
            .find(|(x, _)| (x[0] - target) * (x[1] - target) <= 0.0 && x[0] != x[1])
            .map(|(x, y)| y[0] + (target - x[0]) / (x[1] - x[0]) * (y[1] - y[0]))
            .unwrap_or_else(|| panic!("CL {target} is outside the polar sweep"))
    }

    /// The product mission and the aircraft polar at the same lift
    /// coefficient and the design cruise Mach and altitude.
    ///
    /// Before the far-field fix the mission's induced drag ran 57-70 % above
    /// the polar's on the ATR (aircraft span efficiency 0.51-0.55) and its
    /// L/D 9-19 % low; A220 L/D was 11-17 % low at cruise lift. Now the
    /// mission's aircraft inviscid span efficiency is 0.82-0.99 at CL 0.45-0.85
    /// (0.73 at the ATR's CL 0.32, where washout and trim loading weigh most),
    /// and the L/D agrees with the polar within 5 %. Where it does not (the
    /// A220 above CL 0.45, 9 % at CL 0.64), the whole excess is the
    /// compressibility term: SUAVE's crest-critical fit in the mission against
    /// Korn-Lock in the polar, two different models this fix does not touch.
    #[test]
    fn product_mission_and_polar_agree_at_the_same_lift() {
        use crate::full_analysis::FullAnalysis;
        use alas_config::AlasConfig;
        for name in ["A320-200", "A220-300", "ATR72-600"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .unwrap_or_else(|error| panic!("{name} preset: {error}"));
            let design = alas_config::presets::get(name)
                .unwrap_or_else(|error| panic!("{name} preset: {error}"))
                .design_vector;
            let report = FullAnalysis::new(config.clone())
                .run(&design, true)
                .unwrap_or_else(|error| panic!("{name} report: {error}"));
            let analyses = super::super::build_analyses(&config, &report)
                .unwrap_or_else(|error| panic!("{name} mission analyses: {error}"));
            let mach = config.requirements.cruise_mach;
            let atmosphere = analyses.atmosphere(config.requirements.cruise_altitude_m, 0.0);
            let speed_m_s = mach * atmosphere.speed_of_sound_m_s;
            let reynolds_per_m =
                atmosphere.density_kg_m3 * speed_m_s / atmosphere.dynamic_viscosity_pa_s;
            let polar = &report.polar;
            let trim = analyses.drag_settings.trim_drag_correction_factor;
            let aspect_ratio = analyses.wings[0].aspect_ratio;
            for alpha_deg in [2.0_f64, 3.0, 4.0, 5.0] {
                let aero = analyses.aerodynamics(
                    alpha_deg.to_radians(),
                    mach,
                    atmosphere.temperature_k,
                    reynolds_per_m,
                );
                let cl = aero.lift_coefficient;
                if !(0.3..=0.9).contains(&cl) {
                    continue;
                }
                let drag = &aero.drag;
                let inviscid = drag.induced_total - drag.induced_viscous;
                let span_efficiency = cl * cl / (PI * aspect_ratio * inviscid);
                assert!(
                    (0.70..=1.04).contains(&span_efficiency),
                    "{name} CL {cl:.3}: inviscid span efficiency {span_efficiency:.3}"
                );
                let polar_cd = at_polar_cl(&polar.cl, &polar.cd, cl);
                let ratio = polar_cd / drag.total;
                assert!(
                    ratio < 1.05,
                    "{name} CL {cl:.3}: mission L/D {:.2} above polar {:.2}",
                    cl / drag.total,
                    cl / polar_cd
                );
                if ratio < 0.95 {
                    let mission_without = drag.total - trim * drag.compressible_total;
                    let polar_without = polar_cd - at_polar_cl(&polar.cl, &polar.cd_wave, cl);
                    assert!(
                        mission_without <= polar_without,
                        "{name} CL {cl:.3}: L/D {:.2} vs {:.2} and the gap is not compressibility",
                        cl / drag.total,
                        cl / polar_cd
                    );
                }
            }
        }
    }
}
