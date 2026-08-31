// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn write_figures(
    model: &AuditModel,
    local: &NastranResults,
    msc: Option<&NastranResults>,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let local_result = structural_result(model, local.clone());
    write_scene(
        &figure_structures_loads(Some(&local_result), Some("dark")),
        &output.join("figures/structures_loads_nastran95.svg"),
    )?;
    write_scene(
        &figure_structures_modes(Some(&local_result), Some("dark")),
        &output.join("figures/structures_modes_nastran95.svg"),
    )?;
    if let Some(msc) = msc {
        let msc_result = structural_result(model, msc.clone());
        write_scene(
            &figure_structures_loads(Some(&msc_result), Some("dark")),
            &output.join("figures/structures_loads_msc.svg"),
        )?;
        write_scene(
            &figure_structures_modes(Some(&msc_result), Some("dark")),
            &output.join("figures/structures_modes_msc.svg"),
        )?;
        let mut comparison_result = structural_result(model, msc.clone());
        comparison_result.nastran95 = Some(local.clone());
        write_scene(
            &figure_structures_loads(Some(&comparison_result), Some("dark")),
            &output.join("figures/structures_loads_msc_vs_nastran95.svg"),
        )?;
        write_scene(
            &figure_structures_modes(Some(&comparison_result), Some("dark")),
            &output.join("figures/structures_modes_msc_vs_nastran95.svg"),
        )?;
    }
    Ok(())
}

fn structural_result(model: &AuditModel, nastran: NastranResults) -> StructuralAnalysisResult {
    StructuralAnalysisResult {
        status: "ok".to_owned(),
        error: None,
        wsg: Some(model.wsg.clone()),
        sizing: Some(model.sizing.clone()),
        mesh_health: None,
        analysis: Some(model.analysis.clone()),
        nastran: Some(nastran),
        nastran95: None,
        patran: None,
        torenbeek_wing_mass_kg: f64::NAN,
    }
}

fn write_scene(scene: &Scene, path: &Path) -> Result<(), Box<dyn Error>> {
    let svg = render_svg(scene);
    fs::write(path, svg)?;
    Ok(())
}

fn error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(std::io::Error::other(message.into()))
}

