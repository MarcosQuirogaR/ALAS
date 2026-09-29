// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Terminal summaries printed after a headless run.

use alas_pipeline::PipelineResult;

pub(super) fn print_cpacs_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    if let Some(export) = &result.cpacs_export {
        println!(
            "\n--- CPACS aircraft: {} (v{}) ---",
            export.path.display(),
            export.cpacs_version
        );
    }
}

pub(super) fn print_mses_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    let mses = match result.mses_result {
        Some(ref m) => m,
        None => return,
    };
    if !mses.has_usable_data() {
        println!("\n--- MSES analysis: {} ---", mses.status.as_str());
        if let Some(ref err) = mses.error {
            println!("  {err}");
        }
        return;
    }

    println!("\n--- MSES 2-D polar analysis ---");
    println!("  airfoil          : {}", mses.airfoil_name);
    println!(
        "  Mach / Re        : {:.3} / {:.3e}",
        mses.mach, mses.reynolds
    );
    println!("  converged points : {}", mses.alpha_deg.len());
    if !mses.is_complete() {
        println!(
            "  sweep status     : {} of {} requested points converged",
            mses.converged_alpha_count, mses.requested_alpha_count
        );
        let nonconverged = mses.nonconverged_alpha_deg();
        if !nonconverged.is_empty() {
            let values = nonconverged
                .iter()
                .map(|alpha| format!("{alpha:.4}"))
                .collect::<Vec<_>>()
                .join(", ");
            println!("  not converged    : {values} deg (solver transcripts retained)");
        }
    }
    if let Some(ref pressure) = result.mses_pressure {
        if pressure.status.as_str() != "ok" {
            println!(
                "  pressure distribution: {} ({:?})",
                pressure.status.as_str(),
                pressure.error
            );
        }
    }
}

pub(super) fn print_structural_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    let st = match result.structural_result {
        Some(ref s) => s,
        None => return,
    };
    if st.status.as_str() != "ok" {
        println!("\n--- Structural analysis: {} ---", st.status.as_str());
        if let Some(ref err) = st.error {
            println!("  {err}");
        }
        return;
    }

    println!("\n--- Wingbox structural sizing ---");
    if let Some(ref sizing) = st.sizing {
        println!(
            "  semi-wing mass   : {:.1} kg (Torenbeek: {:.1} kg)",
            sizing.total_mass_kg, st.torenbeek_wing_mass_kg
        );
        println!(
            "  ribs / spacing   : {} / {:.2} m installed (max {:.2} m allowable)",
            sizing.num_ribs,
            sizing.installed_rib_spacing_m(),
            sizing.rib_spacing_m
        );
        println!("  sizing load case : {}", sizing.sizing_load_case);
    }
    if let Some(ref health) = st.mesh_health {
        println!(
            "  mesh elements    : CQUAD4: {}, CTRIA3: {}",
            health.n_cquad4, health.n_ctria3
        );
    }
    if let Some(ref ana) = st.analysis {
        if let Some(lc) = ana.load_cases.first() {
            println!(
                "  tip deflection (case: {}) : {:.3} m",
                lc.name, lc.tip_deflection_m
            );
        }
        if let Some(&first_freq) = ana.modal.frequencies_hz.first() {
            println!("  first bending mode freq : {:.2} Hz", first_freq);
        }
    }
}
