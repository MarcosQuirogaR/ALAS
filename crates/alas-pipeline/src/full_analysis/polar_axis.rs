// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drag polar of a report on the compressible lattice (A1).

use alas_aero::analysis::{AeroAnalysis, PolarSweep};
use alas_aero::vlm::VlmSystem;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::neutral_point::{beta_for_mach, goethert_stretch};

/// Freestream Mach below which the Goethert stretch is the identity to
/// within the solver's tolerance and the incompressible sweep is used as is.
const INCOMPRESSIBLE_MACH: f64 = 1.0e-3;

/// The polar sweep of `aero` at `mach` and `altitude_m`.
///
/// With `compressible`, the schedule is solved on the Goethert-stretched
/// lattice of `plane` (`analysis` mesh) at the true geometric angles: lift,
/// pitching moment (`beta Cm'`) and Trefftz induced drag are the compressible
/// ones and `alpha_deg` is the geometric angle, so every column is consistent
/// with the angle axis. Otherwise the incompressible lattice is solved
/// (reusing `system` when given) and its angle relabelled by the whole-wing
/// Prandtl-Glauert factor, as `alas_aero` does.
///
/// # Errors
///
/// The solver failure, as a description. There is no silent fallback to the
/// relabel: a stretched lattice that cannot be solved is an error.
pub(crate) fn run_polar(
    aero: &AeroAnalysis<'_>,
    plane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    altitude_m: f64,
    compressible: bool,
    system: Option<&VlmSystem<'_>>,
) -> Result<PolarSweep, String> {
    if compressible && mach.abs() >= INCOMPRESSIBLE_MACH {
        let beta = beta_for_mach(mach);
        let stretched = goethert_stretch(plane, beta);
        let stretched_system = VlmSystem::assemble(
            &stretched,
            analysis.spanwise_resolution.max(1) as usize,
            analysis.chordwise_resolution.max(1) as usize,
        )
        .map_err(|e| format!("stretched vlm assembly error: {e}"))?;
        return aero
            .run_stretched_sweep_with_system(&stretched_system, mach, altitude_m, beta)
            .map_err(|e| format!("compressible polar sweep error: {e}"));
    }
    match system {
        Some(system) => aero.run_sweep_with_system(system, mach, altitude_m),
        None => aero.run_sweep(mach, altitude_m),
    }
    .map_err(|e| format!("polar sweep error: {e}"))
}

// A test builds a curated preset, so a failed unwrap or expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_aero::vlm::VlmSystem;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;
    use serde_json::json;

    fn a320() -> (Airplane, AlasConfig) {
        let config = AlasConfig::from_value(&json!({"preset": "A320-200"})).unwrap();
        let design = presets::get("A320-200").unwrap().design_vector;
        let mut plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        // The product pipeline gives the tail its dynamic-pressure ratio.
        alas_aero::analysis::apply_tail_efficiency(&mut plane, &config);
        (plane, config)
    }

    fn polar(compressible: bool, mach: f64) -> PolarSweep {
        let (plane, config) = a320();
        let analysis = config.analysis.clone();
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, 25.0),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(analysis.clone()),
        );
        run_polar(&aero, &plane, &analysis, mach, 11_000.0, compressible, None).unwrap()
    }

    #[test]
    fn the_compressible_polar_is_at_the_true_angle_and_steeper_than_the_relabel_basis() {
        let on = polar(true, 0.78);
        let off = polar(false, 0.78);
        assert_eq!(on.compressible_mach, Some(0.78));
        assert_eq!(off.compressible_mach, None);
        assert_eq!(on.alpha_deg, on.geometric_alpha_deg);
        assert_ne!(off.alpha_deg, off.geometric_alpha_deg);
        let slope = |p: &PolarSweep| {
            let last = p.cl.len() - 1;
            (p.cl[last] - p.cl[0]) / (p.geometric_alpha_deg[last] - p.geometric_alpha_deg[0])
        };
        // The compressible-to-incompressible slope ratio of the airplane is
        // the wing's DATCOM 4.1.3.2 Method 1 ratio (the tail gains slightly
        // less): within 3 % of it, the bound of the lattice's own taper spread.
        let (plane, _) = a320();
        let main = &plane.wings[0];
        let datcom = alas_stab::compressible_sweep::datcom_slope_ratio(
            main.aspect_ratio(),
            0.78,
            main.mean_sweep_angle(0.5),
        );
        let off_inc = polar(true, 1.0e-4);
        let incompressible_slope = {
            let last = off_inc.cl.len() - 1;
            (off_inc.cl[last] - off_inc.cl[0])
                / (off_inc.geometric_alpha_deg[last] - off_inc.geometric_alpha_deg[0])
        };
        let ratio_vs_incompressible_lattice = slope(&on) / incompressible_slope;
        assert!(
            (ratio_vs_incompressible_lattice / datcom - 1.0).abs() < 0.03,
            "{ratio_vs_incompressible_lattice} vs DATCOM {datcom}"
        );
        // Every drag column is at the compressible lift.
        for k in 0..on.cl.len() {
            let sum = on.cd_induced[k] + on.cd_parasite[k] + on.cd_wave[k];
            assert!((sum - on.cd[k]).abs() < 1e-12);
        }
    }

    #[test]
    fn a_nearly_incompressible_mach_keeps_the_incompressible_sweep() {
        let on = polar(true, 1.0e-4);
        assert_eq!(on.compressible_mach, None);
    }

    #[test]
    fn the_compressible_polar_moment_slope_is_the_trims_lattice_moment_slope() {
        // `Cm_alpha` of the polar (beta Cm' on the stretched lattice) must be
        // the lattice part of the trim's compressible `Cm_alpha`: the trim adds
        // the fuselage/nacelle body slope `cm_body_per_deg` on top.
        let (plane, config) = a320();
        let mach = 0.78;
        let on = polar(true, mach);
        let last = on.cm.len() - 1;
        let polar_slope =
            (on.cm[last] - on.cm[0]) / (on.geometric_alpha_deg[last] - on.geometric_alpha_deg[0]);
        let trim =
            alas_stab::trim::stability_and_trim(&plane, &config.analysis, 0.5, mach, 11_000.0)
                .unwrap();
        let comp = trim.compressible.unwrap();
        let lattice_slope = comp.cm_alpha - comp.cm_body_per_deg;
        assert!(
            (polar_slope / lattice_slope - 1.0).abs() < 0.02,
            "polar {polar_slope} vs trim lattice {lattice_slope}"
        );
    }

    /// The A320 with the compressible trim at `mach`, its aero analysis inputs.
    fn trimmed(
        mach: f64,
        cl_target: f64,
    ) -> (Airplane, AlasConfig, alas_stab::trim::StabilityTrimResult) {
        let (plane, config) = a320();
        let trim = alas_stab::trim::stability_and_trim(
            &plane,
            &config.analysis,
            cl_target,
            mach,
            11_000.0,
        )
        .unwrap();
        (plane, config, trim)
    }

    fn aero_of<'a>(plane: &'a Airplane, config: &AlasConfig) -> AeroAnalysis<'a> {
        AeroAnalysis::new(
            plane,
            AeroAnalysis::quarter_chord_sweep_deg(plane, 25.0),
            Some(config.geometry.clone()),
            Some(config.drag_model.clone()),
            Some(config.analysis.clone()),
        )
    }

    #[test]
    fn the_stretched_trimmed_point_closes_lift_and_moment_and_matches_the_stretched_polar() {
        let (mach, cl_target) = (0.78, 0.5);
        let (plane, config, trim) = trimmed(mach, cl_target);
        let flight = trim.attitude();
        assert!(flight.compressible && flight.converged);
        let beta = alas_stab::neutral_point::beta_for_mach(mach);
        let stretched = alas_stab::neutral_point::goethert_stretch(&plane, beta);
        let aero = aero_of(&plane, &config);
        let point = alas_aero::analysis::TrimPoint {
            trim_alpha_deg: flight.alpha_deg,
            trim_ih_deg: flight.incidence_deg,
            cl_alpha: flight.cl_alpha,
        };
        let perf = aero
            .trimmed_stretched_performance(
                &stretched,
                &point,
                mach,
                11_000.0,
                beta,
                flight.cm_body_per_deg,
            )
            .unwrap();
        // (a) The trim closes the target lift and the moment, body term included.
        assert!((perf.cl - cl_target).abs() < 1.0e-4, "CL {}", perf.cl);
        assert!(
            perf.cm_residual.abs() < 1.0e-4,
            "Cm residual {}",
            perf.cm_residual
        );
        // (c) Its induced drag is the Trefftz drag of the stretched lattice at
        // that attitude, solved here independently, plus the installation
        // loss on its wing lift.
        let mut tail_set = stretched.clone();
        for wing in tail_set
            .wings
            .iter_mut()
            .filter(|w| w.name == "Horizontal Stabilizer")
        {
            wing.twist_about_pitch_axis = true;
            for xsec in &mut wing.xsecs {
                xsec.twist = flight.incidence_deg;
            }
        }
        let (span, chord) = (
            config.analysis.spanwise_resolution.max(1) as usize,
            config.analysis.chordwise_resolution.max(1) as usize,
        );
        let system = VlmSystem::assemble(&tail_set, span, chord).unwrap();
        let atmosphere = alas_atmo::Atmosphere::new(11_000.0);
        let op = alas_aero::operating_point::OperatingPoint::new(
            atmosphere,
            mach * atmosphere.speed_of_sound(),
            flight.alpha_deg,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let solved = system.solve(&op).unwrap();
        let direct = system
            .trefftz_induced_drag_coefficient(&solved, &op)
            .unwrap()
            + aero.installation_induced_drag(alas_aero::analysis::wing_lift_coefficient(
                &solved, &op, &tail_set,
            ));
        assert!(
            (perf.cd_induced - direct).abs() < 1.0e-12,
            "{} vs {direct}",
            perf.cd_induced
        );
        assert!((perf.cl - solved.cl_lift).abs() < 1.0e-12);
        assert!(perf.cd > perf.cd_induced && perf.l_over_d > 5.0);
    }

    #[test]
    fn the_stretched_trimmed_point_with_an_identity_stretch_is_the_incompressible_one() {
        // beta = 1 and no body term: the stretched path is the physical lattice.
        let (plane, config, trim) = trimmed(0.2, 0.5);
        let aero = aero_of(&plane, &config);
        let point = alas_aero::analysis::TrimPoint {
            trim_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim.trim_ih_deg,
            cl_alpha: trim.cl_alpha,
        };
        let stretched = aero
            .trimmed_stretched_performance(&plane, &point, 0.2, 11_000.0, 1.0, 0.0)
            .unwrap();
        let physical = aero.trimmed_performance(&point, 0.2, 11_000.0).unwrap();
        assert_eq!(stretched.cl, physical.cl);
        assert_eq!(stretched.cd, physical.cd);
        assert_eq!(stretched.cd_induced, physical.cd_induced);
        assert_eq!(stretched.cm_residual, physical.cm_residual);
        assert_eq!(stretched.l_over_d, physical.l_over_d);
    }

    /// The static margin the lattice moment slope implies, and the trim's
    /// neutral-point margin, at `mach`: `-(Cm_alpha + body term) / CL_alpha`.
    fn polar_and_neutral_point_margins(mach: f64) -> (f64, f64) {
        let (_, _, trim) = trimmed(mach, 0.5);
        let comp = trim.compressible.unwrap();
        let on = polar(true, mach);
        let last = on.cm.len() - 1;
        let dalpha = on.geometric_alpha_deg[last] - on.geometric_alpha_deg[0];
        let cl_alpha = (on.cl[last] - on.cl[0]) / dalpha + comp.cl_body_per_deg;
        let cm_alpha = (on.cm[last] - on.cm[0]) / dalpha + comp.cm_body_per_deg;
        (-cm_alpha / cl_alpha, comp.static_margin)
    }

    #[test]
    fn the_polar_moment_slope_and_the_neutral_point_differ_by_the_same_gap_at_every_mach() {
        // The neutral point is the tail-efficiency-corrected
        // station of `corrected_np_from_solved_probes`, not the raw lattice
        // moment slope, so the two margins differ by a model gap (A320:
        // 0.038 of the mean chord at M 0.78 with the tail in the free stream);
        // what the compressible transform must preserve is that gap, which must
        // not depend on Mach. The tail carries its efficiency
        // as its dynamic-pressure ratio in the lattice, so the polar slope and the
        // neutral point describe one airframe and the gap is the model gap alone.
        let (polar_high, neutral_high) = polar_and_neutral_point_margins(0.78);
        let (polar_low, neutral_low) = polar_and_neutral_point_margins(0.2);
        let (gap_high, gap_low) = (polar_high - neutral_high, polar_low - neutral_low);
        assert!(
            (gap_high - gap_low).abs() < 5.0e-3,
            "gap M0.78 {gap_high:.4} vs M0.2 {gap_low:.4}"
        );
        // The margins themselves move with Mach as the aft-shifting
        // neutral point says.
        assert!(neutral_high > neutral_low && polar_high > polar_low);
    }
}
