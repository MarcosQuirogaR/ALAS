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
    let base = MsesPressureResult {
        alpha_deg,
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
    Mses::new(repaneled, config, mses_dir).pressure(alpha_deg, reynolds, mach, offsets)
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
}

