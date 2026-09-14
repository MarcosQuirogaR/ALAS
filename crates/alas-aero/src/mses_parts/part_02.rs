// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Solve one MSES operating point and read back its surface pressure and Mach
/// distribution.
///
/// Retries at [`DEFAULT_RETRY_OFFSETS_DEG`] around `alpha_deg` (or the caller's
/// offsets) with a fresh mesh each attempt, since a fixed point has no adjacent
/// sweep step to continue from; the converged angle is reported back in the
/// result. `mses_dir` is the resolved folder holding the three executables.
pub fn run_mses_pressure_distribution(
    airfoil: &Airfoil,
    mach: f64,
    reynolds: f64,
    alpha_deg: f64,
    config: &MsesConfig,
    mses_dir: &Path,
    retry_offsets_deg: Option<&[f64]>,
) -> MsesPressureResult {
    run_mses_pressure_distribution_with_cancel(
        airfoil,
        mach,
        reynolds,
        alpha_deg,
        config,
        mses_dir,
        retry_offsets_deg,
        None,
    )
}

/// Run the MSES pressure entry point with cooperative cancellation.
// Every argument is an independently configured input of one MSES run, and
// each caller varies a different one; bundling them would hide which.
#[allow(clippy::too_many_arguments)]
pub fn run_mses_pressure_distribution_with_cancel(
    airfoil: &Airfoil,
    mach: f64,
    reynolds: f64,
    alpha_deg: f64,
    config: &MsesConfig,
    mses_dir: &Path,
    retry_offsets_deg: Option<&[f64]>,
    cancel: Option<&AtomicBool>,
) -> MsesPressureResult {
    run_mses_pressure_distribution_with_checkpoint_and_cancel(
        airfoil,
        mach,
        reynolds,
        alpha_deg,
        config,
        mses_dir,
        retry_offsets_deg,
        None,
        cancel,
    )
}

/// Run the MSES pressure entry point warm-started from a genuinely converged
/// polar checkpoint, with cooperative cancellation.
///
/// `checkpoint` is typically one entry of [`MsesPolarResult::checkpoints`]
/// from a prior [`run_mses_polar_with_cancel`] call on the *same*
/// airfoil/config/Mach/Reynolds/OSMAP -- picked as whichever checkpoint's
/// `alpha_deg` is nearest the pressure point actually wanted. Passing a
/// checkpoint from a different geometry, config, Mach, Reynolds, or OSMAP map
/// is safe (it is rejected by an exact identity check and this call falls
/// back to the same cold-start search [`run_mses_pressure_distribution`]
/// already performs) but wastes the opportunity to skip that search.
///
/// See [`Mses::pressure_with_checkpoint_and_cancel`] for what "warm-started"
/// means here: the checkpoint's converged flowfield is restored and bridged
/// toward the exact requested angle in bounded steps before any cold clean
/// mesh is attempted, cheaper than (and in addition to, never instead of)
/// the existing retry-offset search.
#[allow(clippy::too_many_arguments)]
pub fn run_mses_pressure_distribution_with_checkpoint_and_cancel(
    airfoil: &Airfoil,
    mach: f64,
    reynolds: f64,
    alpha_deg: f64,
    config: &MsesConfig,
    mses_dir: &Path,
    retry_offsets_deg: Option<&[f64]>,
    checkpoint: Option<&MsesConvergedCheckpoint>,
    cancel: Option<&AtomicBool>,
) -> MsesPressureResult {
    let base = MsesPressureResult {
        alpha_deg,
        requested_alpha_deg: alpha_deg,
        ..MsesPressureResult::default()
    };
    if !config.enabled {
        return base.into_failure(
            MsesStatus::Disabled,
            "MSES analysis is disabled in configuration".to_owned(),
        );
    }
    let repaneled = match airfoil.repanel(N_POINTS_PER_SIDE) {
        Ok(repaneled) => repaneled,
        Err(error) => return base.into_error(error.to_string()),
    };
    let offsets = retry_offsets_deg.unwrap_or(&DEFAULT_RETRY_OFFSETS_DEG);
    Mses::new(repaneled, config, mses_dir).pressure_with_checkpoint_and_cancel(
        alpha_deg, reynolds, mach, offsets, checkpoint, cancel,
    )
}

/// A section's name, or the placeholder upstream uses when it has none.
fn display_name(airfoil: &Airfoil) -> String {
    if airfoil.name.is_empty() {
        "optimized_root_section".to_owned()
    } else {
        airfoil.name.clone()
    }
}

/// Classify the required MSES installation before attempting a process spawn.
pub(crate) fn installation_status(directory: &Path) -> Option<MsesStatus> {
    if !directory.is_dir() {
        return Some(MsesStatus::Absent);
    }
    let missing = ["mset.exe", "mses.exe", "mplot.exe"]
        .iter()
        .filter(|name| !directory.join(name).is_file())
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        None
    } else {
        Some(MsesStatus::Incomplete)
    }
}

#[cfg(test)]
mod raw_export_tests {
    use super::*;

    #[test]
    fn retained_mplot_tables_replay_the_live_parser_contract() -> Result<(), MsesRawExportError> {
        let bl = "# x y s b0 Cp Ue rho Me\n\
                  0.2 0.1 0.2 0 -0.8 0 0 1.2\n\
                  0.8 0.1 0.8 0 -0.4 0 0 1.1\n\
                  0.3 -0.1 0.3 0 0.2 0 0 0.7\n\
                  1.2 -0.1 1.2 0 0.1 0 0 0.8\n"
            .to_owned();
        let flow = "# x y rho p u v q M Cp\n\
                    0.0 -0.5 1 1 1 0 1 0.7 0\n\
                    1.0 -0.5 1 1 1 0 1 0.8 0\n\n\
                    0.0 0.5 1 1 1 0 1 0.9 0\n\
                    1.0 0.5 1 1 1 0 1 1.0 0\n"
            .to_owned();
        let result = MsesPressureResult::replay_raw_exports(
            2.5,
            bl.clone(),
            flow.clone(),
            &[(0.0, 0.0), (1.0, 0.0)],
        )?;

        assert_eq!(result.status, MsesStatus::Ok);
        assert_eq!(result.x_upper, vec![0.2, 0.8]);
        assert_eq!(result.cp_upper, vec![-0.8, -0.4]);
        assert_eq!(result.x_lower, vec![0.3]);
        assert_eq!(result.field_row_offsets, vec![0, 2]);
        assert_eq!(result.raw_bl_dump, bl);
        assert_eq!(result.raw_flowfield_dump, flow);
        Ok(())
    }

    #[test]
    fn a_missing_optional_flowfield_keeps_the_surface_solution() -> Result<(), MsesRawExportError> {
        let bl = "0.2 0.1 0.2 0 -0.8 0 0 1.2\n\
                  0.3 -0.1 0.1 0 0.2 0 0 0.7\n"
            .to_owned();
        let result = MsesPressureResult::replay_raw_exports(
            1.0,
            bl,
            String::new(),
            &[(0.0, 0.0), (1.0, 0.0)],
        )?;

        assert_eq!(result.status, MsesStatus::Ok);
        assert_eq!(result.cp_upper, vec![-0.8]);
        assert_eq!(result.cp_lower, vec![0.2]);
        assert!(result.field_x.is_empty());
        Ok(())
    }

    #[test]
    fn surface_assignment_matches_reference_y_sign_and_sorts_x() -> Result<(), MsesRawExportError> {
        let bl = "0.0 -0.002 0.0 0 1.2 0 0 0.1\n\
                  0.4 0.050 0.4 0 -0.9 0 0 1.2\n\
                  0.9 -0.010 0.9 0 -0.2 0 0 0.8\n\
                  0.0 -0.002 0.0 0 1.2 0 0 0.1\n\
                  0.4 -0.040 0.4 0 0.3 0 0 0.7\n\
                  0.9 0.020 0.9 0 0.1 0 0 0.9\n"
            .to_owned();
        let result = MsesPressureResult::replay_raw_exports(
            1.0,
            bl,
            String::new(),
            &[(1.0, -0.01), (0.0, 0.0), (1.0, 0.02)],
        )?;

        assert_eq!(result.x_upper, vec![0.4, 0.9]);
        assert_eq!(result.mach_upper, vec![1.2, 0.9]);
        assert_eq!(result.x_lower, vec![0.0, 0.0, 0.4, 0.9]);
        assert_eq!(result.mach_lower, vec![0.1, 0.1, 0.7, 0.8]);
        Ok(())
    }

    #[test]
    fn flowfield_domain_uses_every_finite_mplot_sample() {
        let result = MsesPressureResult {
            field_x: vec![-2.0, 0.5, 3.0, f64::NAN],
            field_y: vec![-1.5, 0.0, 2.0, 10.0],
            field_mach: vec![0.2, 0.8, 1.1, 0.5],
            ..MsesPressureResult::default()
        };

        assert_eq!(
            result.flowfield_domain(),
            Some(MsesFlowfieldDomain {
                x_min: -2.0,
                x_max: 3.0,
                y_min: -1.5,
                y_max: 2.0,
            })
        );
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod polar_diagnostic_tests {
    use super::*;

    fn complete_polar() -> MsesPolarResult {
        MsesPolarResult {
            status: MsesStatus::Ok,
            requested_alpha_count: 1,
            converged_alpha_count: 1,
            alpha_deg: vec![1.0],
            cl: vec![0.5],
            cd: vec![0.03],
            cm: vec![-0.02],
            cdv: vec![0.02],
            cdw: vec![0.01],
            xtr_top: vec![0.4],
            xtr_bot: vec![0.6],
            ..MsesPolarResult::default()
        }
    }

    #[test]
    fn usable_polar_requires_aligned_finite_coefficient_columns() {
        let complete = complete_polar();
        assert!(complete.has_usable_data());

        let mut ragged = complete.clone();
        ragged.cdw.clear();
        assert!(!ragged.has_usable_data());
        assert!(!ragged.is_complete());

        let mut nonfinite = complete;
        nonfinite.cl[0] = f64::NAN;
        assert!(!nonfinite.has_usable_data());
    }

    #[test]
    fn partial_polar_retains_the_exact_nonconverged_requests() {
        let result = MsesPolarResult {
            requested_alpha_count: 3,
            converged_alpha_count: 1,
            point_diagnostics: vec![
                MsesPolarPointDiagnostic {
                    requested_alpha_deg: -1.0,
                    status: MsesPolarPointStatus::Converged,
                    solver_output: "Converged on tolerance".to_owned(),
                },
                MsesPolarPointDiagnostic {
                    requested_alpha_deg: 0.0,
                    status: MsesPolarPointStatus::NotConverged,
                    solver_output: "Newton iteration stopped".to_owned(),
                },
                MsesPolarPointDiagnostic {
                    requested_alpha_deg: 1.0,
                    status: MsesPolarPointStatus::NotConverged,
                    solver_output: "Newton iteration stopped".to_owned(),
                },
            ],
            ..MsesPolarResult::default()
        };

        assert_eq!(result.nonconverged_alpha_deg(), vec![0.0, 1.0]);
        assert_eq!(result.point_diagnostics[1].status.as_str(), "not_converged");
        assert_eq!(
            result.point_diagnostics[1].solver_output,
            "Newton iteration stopped"
        );
    }

    #[test]
    fn closest_checkpoint_selection_uses_total_cmp_and_ignores_nonfinite() {
        let make_cp = |alpha: f64, mach: f64, re: f64, name: &str| MsesConvergedCheckpoint {
            airfoil_name: name.to_owned(),
            airfoil_coordinates: vec![(0.0, 0.0), (1.0, 0.0)],
            n_crit: 9.0,
            xtr_upper: 1.0,
            xtr_lower: 1.0,
            mset_n: 215,
            mset_e: 1.2,
            mucon: 1.0,
            max_iterations: 100,
            mach,
            reynolds: re,
            osmap_path: None,
            alpha_deg: alpha,
            solver_output: "Converged on tolerance".to_owned(),
            mdat_case: vec![1, 2, 3],
        };

        let polar = MsesPolarResult {
            airfoil_name: "test_section".to_owned(),
            mach: 0.735,
            reynolds: 5.0e6,
            checkpoints: vec![
                make_cp(1.0, 0.735, 5.0e6, "test_section"),
                make_cp(2.0, 0.735, 5.0e6, "test_section"),
                make_cp(3.0, 0.735, 5.0e6, "test_section"),
                make_cp(f64::NAN, 0.735, 5.0e6, "test_section"),
                // Mismatched Mach
                make_cp(2.1, 0.800, 5.0e6, "test_section"),
                // Mismatched Reynolds
                make_cp(2.2, 0.735, 1.0e7, "test_section"),
                // Mismatched Airfoil
                make_cp(2.3, 0.735, 5.0e6, "other_section"),
            ],
            ..MsesPolarResult::default()
        };

        // Non-finite target alpha returns None
        assert!(polar.closest_checkpoint(f64::NAN).is_none());
        assert!(polar.closest_checkpoint(f64::INFINITY).is_none());
        assert!(polar.closest_checkpoint(f64::NEG_INFINITY).is_none());

        // Target 2.1 is closest to 2.0 (dist 0.1 vs 0.9 to 3.0), ignoring mismatched 2.1/2.2/2.3
        let closest = polar.closest_checkpoint(2.1).expect("closest checkpoint");
        assert_eq!(closest.alpha_deg, 2.0);

        // Target 2.6 is closest to 3.0 (dist 0.4 vs 0.6 to 2.0)
        let closest = polar.closest_checkpoint(2.6).expect("closest checkpoint");
        assert_eq!(closest.alpha_deg, 3.0);

        // Standalone selector helper
        let cps = vec![
            make_cp(0.0, 0.735, 5.0e6, "test"),
            make_cp(1.5, 0.735, 5.0e6, "test"),
            make_cp(f64::NAN, 0.735, 5.0e6, "test"),
        ];
        assert_eq!(select_closest_checkpoint(&cps, 1.2).map(|c| c.alpha_deg), Some(1.5));
        assert_eq!(select_closest_checkpoint(&cps, 0.5).map(|c| c.alpha_deg), Some(0.0));
        assert!(select_closest_checkpoint(&cps, f64::NAN).is_none());
    }
}
