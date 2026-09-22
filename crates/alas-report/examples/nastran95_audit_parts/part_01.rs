// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use alas_config::materials;
use alas_config::{DesignRequirements, DesignVector, EngineConfig, MassModelConfig};
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_pipeline::structural::StructuralAnalysisResult;
use alas_report::families::structures::{figure_structures_loads, figure_structures_modes};
use alas_report::{render_svg, Scene};
use alas_struct::analytical::analyze_structure;
use alas_struct::mesh::build_wing_mesh_bdf;
use alas_struct::nastran::NastranResults;
use alas_struct::nastran95::{run_nastran95_analysis, Nastran95Solver};
use alas_struct::sizing::{size_wingbox, WingboxSizing};
use alas_struct::{mesh::Deck, mesh::MeshNodeIndex};
use serde_json::{json, Value};

const DEFAULT_OUTPUT: &str = "out/nastran95_audit";
const SHORT_WORK_ROOT: &str = "C:/nas-run/alas_nastran95_audit";

struct AuditModel {
    deck: Deck,
    node_index: MeshNodeIndex,
    requirements: DesignRequirements,
    config: alas_config::StructuresConfig,
    wsg: WingStructureGeometry,
    sizing: WingboxSizing,
    analysis: alas_struct::analytical::StructuralAnalysisReport,
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    fs::create_dir_all(output.join("figures"))?;
    fs::create_dir_all(output.join("local_nastran95"))?;
    fs::create_dir_all(output.join("msc_nastran"))?;

    let model = build_model()?;
    let local_solver = Nastran95Solver::from_env()
        .ok_or_else(|| error("set ALAS_NASTRAN95_DIR to the local NASTRAN-95 build"))?;
    let local_work = Path::new(SHORT_WORK_ROOT).join(std::process::id().to_string());
    let local_results = run_nastran95_analysis(
        &model.deck,
        &model.node_index,
        &model.config,
        &model.requirements,
        &local_work,
        &local_solver,
    );
    copy_solver_artifacts(&local_work, &output.join("local_nastran95"))?;

    let msc_results = run_msc(&model, &output.join("msc_nastran"))?;
    let comparison = comparison_json(&model, &local_results, msc_results.as_ref());
    fs::write(
        output.join("comparison.json"),
        serde_json::to_string_pretty(&comparison)?,
    )?;
    fs::write(output.join("report_es.md"), report_markdown(&comparison))?;
    write_summary_tables(&comparison, &output)?;

    write_figures(&model, &local_results, msc_results.as_ref(), &output)?;
    fs::write(
        output.join("README.txt"),
        "This directory contains retained BDF/F06/OP2 solver artifacts,\n\
comparison.json, report_es.md, summary_table.md, summary_table.typ, and\n\
combined MSC/NASTRAN-95 structural SVG figures.\n\
Local NASTRAN-95 is invoked separately under NOSA 1.3; no MSC files are\n\
needed for the local SOL 101/SOL 103 run.\n",
    )?;
    Ok(())
}

fn build_model() -> Result<AuditModel, Box<dyn Error>> {
    let config = alas_config::StructuresConfig {
        num_ribs_override: Some(5),
        mesh_chordwise_points: 6,
        n_modes: 6,
        skin_material: "Al 7075-T6".to_owned(),
        spar_web_material: "Al 7075-T6".to_owned(),
        spar_cap_material: "Al 7075-T6".to_owned(),
        rib_material: "Al 7075-T6".to_owned(),
        run_sol_vibration_sine: false,
        run_sol_vibration_random: false,
        run_patran_export: false,
        timeout_s: 300.0,
        ..alas_config::StructuresConfig::default()
    };
    let requirements = DesignRequirements::default();
    let design = DesignVector::default();
    let wing = alas_config::WingConfig::default();
    let root_base = AirfoilLibrary::get(&wing.root_airfoil)
        .ok_or_else(|| error(format!("root airfoil not found: {}", wing.root_airfoil)))?;
    let root_section = build_section(&design, &root_base.coordinates)
        .map_err(|err| error(format!("root airfoil section: {err:?}")))?;
    let tip_airfoil = AirfoilLibrary::get(&wing.tip_airfoil)
        .ok_or_else(|| error(format!("tip airfoil not found: {}", wing.tip_airfoil)))?;
    let wsg = WingStructureGeometry::new(
        &design,
        &wing,
        &root_section,
        &tip_airfoil,
        &[0.15, 0.60],
        Some(&[true, true]),
    )
    .map_err(|err| error(format!("wing structure geometry: {err:?}")))?;

    let skin = materials::get(&config.skin_material)
        .map_err(|err| error(format!("skin material: {err:?}")))?;
    let web = materials::get(&config.spar_web_material)
        .map_err(|err| error(format!("web material: {err:?}")))?;
    let cap = materials::get(&config.spar_cap_material)
        .map_err(|err| error(format!("cap material: {err:?}")))?;
    let rib = materials::get(&config.rib_material)
        .map_err(|err| error(format!("rib material: {err:?}")))?;
    let sizing = size_wingbox(&wsg, &config, &requirements, skin, web, cap, rib);
    let analysis = analyze_structure(
        &wsg,
        &sizing,
        &config,
        &requirements,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        skin,
        web,
        cap,
    );
    let (deck, _, node_index) = build_wing_mesh_bdf(
        &wsg,
        &sizing,
        &config,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &requirements,
        skin,
        web,
        cap,
        rib,
    )
    .map_err(|err| error(format!("wing mesh: {err:?}")))?;
    Ok(AuditModel {
        deck,
        node_index,
        requirements,
        config,
        wsg,
        sizing,
        analysis,
    })
}

fn run_msc(model: &AuditModel, output: &Path) -> Result<Option<NastranResults>, Box<dyn Error>> {
    let Some(launcher) = std::env::var_os("ALAS_MSC_LAUNCHER").map(PathBuf::from) else {
        return Ok(None);
    };
    let Some(solver) = std::env::var_os("ALAS_MSC_SOLVER").map(PathBuf::from) else {
        return Ok(None);
    };
    if !launcher.is_file() || !solver.is_file() {
        return Ok(None);
    }
    let mut config = model.config.clone();
    config.nastran_solver_path = solver.display().to_string();
    let results = alas_struct::nastran::run_nastran_analysis(
        &model.deck,
        &model.node_index,
        &config,
        &model.requirements,
        output,
        Some(&launcher),
    );
    Ok(Some(results))
}

fn copy_solver_artifacts(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    if source == destination {
        return Ok(());
    }
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_solver_artifacts(&path, &target)?;
        } else if path.extension().is_some_and(|extension| {
            matches!(
                extension.to_str(),
                Some("bdf" | "f06" | "op2" | "log" | "err")
            )
        }) {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

fn comparison_json(
    model: &AuditModel,
    local: &NastranResults,
    msc: Option<&NastranResults>,
) -> Value {
    let static_cases = local
        .static_solve
        .tip_deflection_m
        .iter()
        .map(|(label, local_value)| {
            let msc_value = msc.and_then(|result| result.static_solve.tip_deflection_m.get(label));
            json!({
                "case": label,
                "local_nastran95_tip_deflection_m": local_value,
                "msc_tip_deflection_m": msc_value,
                "relative_difference_local_minus_msc": relative_difference(local_value, msc_value),
            })
        })
        .collect::<Vec<_>>();
    let modal_matches = local
        .modes
        .frequencies_hz
        .iter()
        .map(|&frequency| {
            let nearest = msc.and_then(|result| {
                result
                    .modes
                    .frequencies_hz
                    .iter()
                    .min_by(|a, b| (*a - frequency).abs().total_cmp(&(*b - frequency).abs()))
                    .copied()
            });
            json!({
                "local_nastran95_hz": frequency,
                "nearest_msc_hz": nearest,
                "relative_difference_local_minus_nearest_msc": relative_difference(frequency, nearest),
            })
        })
        .collect::<Vec<_>>();
    let msc_stress = msc.map(|result| {
        result
            .static_solve
            .root_von_mises_max_pa
            .iter()
            .map(|(label, value)| json!({ "case": label, "von_mises_max_pa": value }))
            .collect::<Vec<_>>()
    });
    json!({
        "mesh": {
            "grids": model.deck.grids().len(),
            "elements": model.deck.element_count(),
            "root_grid": model.node_index.root_nid,
            "tip_grid": model.node_index.tip_nid,
        },
        "local_nastran95": {
            "sol101_status": local.static_solve.status.as_str(),
            "sol101_tip_deflections_m": static_cases,
            "sol101_stress_available": !local.static_solve.root_von_mises_max_pa.is_empty(),
            "sol101_root_von_mises_max_pa": [],
            "sol103_status": local.modes.status.as_str(),
            "sol103_frequencies_hz": local.modes.frequencies_hz,
            "sol111_status": local.vibration.status.as_str(),
            "sol111_error": local.vibration.error,
        },
        "msc": msc.map(|result| json!({
            "sol101_status": result.static_solve.status.as_str(),
            "sol101_stress_available": !result.static_solve.root_von_mises_max_pa.is_empty(),
            "sol101_root_von_mises_max_pa": msc_stress,
            "sol103_status": result.modes.status.as_str(),
            "sol103_frequencies_hz": result.modes.frequencies_hz,
        })),
        "modal_nearest_frequency_comparison": modal_matches,
    })
}

fn relative_difference(local: f64, msc: Option<f64>) -> Option<f64> {
    let msc = msc?;
    (msc.abs() > 1.0e-30).then_some((local - msc) / msc)
}

fn report_markdown(comparison: &Value) -> String {
    let local = &comparison["local_nastran95"];
    let msc = comparison.get("msc").filter(|value| !value.is_null());
    let mut report = String::from(
        "# Auditoria local de NASTRAN-95\n\n\
El backend Rust genera un BDF NASTRAN-95, ejecuta SOL 101 y SOL 103,\n\
conserva los ficheros de entrada/salida y compara las magnitudes con MSC\n\
cuando la instalacion esta disponible.\n\n",
    );
    report.push_str(&format!(
        "- Mallado: {} grids y {} elementos.\n- SOL 101 local: `{}`.\n- SOL 103 local: `{}` ({} modos elasticos).\n",
        comparison["mesh"]["grids"],
        comparison["mesh"]["elements"],
        local["sol101_status"],
        local["sol103_status"],
        local["sol103_frequencies_hz"].as_array().map_or(0, Vec::len),
    ));
    let local_stress_count = local["sol101_root_von_mises_max_pa"]
        .as_array()
        .map_or(0, Vec::len);
    report.push_str(&format!(
        "- Esfuerzo SOL 101 local: no disponible en NASTRAN-95 ({} casos leidos).\n",
        local_stress_count
    ));
    if let Some(case) = local["sol101_tip_deflections_m"]
        .as_array()
        .and_then(|cases| cases.first())
    {
        if let (Some(local_value), Some(msc_value), Some(relative)) = (
            case["local_nastran95_tip_deflection_m"].as_f64(),
            case["msc_tip_deflection_m"].as_f64(),
            case["relative_difference_local_minus_msc"].as_f64(),
        ) {
            report.push_str(&format!(
                "- Caso `{}`: deflexion local {:.6} m frente a MSC {:.6} m ({:.3}% relativo).\n",
                case["case"],
                local_value,
                msc_value,
                100.0 * relative
            ));
        }
    }
    if let Some(msc) = msc {
        report.push_str(&format!(
            "- MSC SOL 101: `{}`; MSC SOL 103: `{}`; esfuerzo disponible: `{}`.\n",
            msc["sol101_status"], msc["sol103_status"], msc["sol101_stress_available"]
        ));
        report.push_str(
            "\nLas diferencias numericas de SOL 101 y la correspondencia de SOL 103\n\
estan en `comparison.json`; el emparejamiento modal es por frecuencia mas\n\
cercana, no por indice.\n",
        );
    } else {
        report.push_str("\nMSC no estaba configurado; no se inventa una comparacion comercial.\n");
    }
    report.push_str(
        "\nEsta auditoria no solicita SOL 111. Si se solicita, el backend local devuelve\n\
un error explicito porque NASTRAN-95 no implementa el contrato moderno de\n\
SOL 111; no se fabrica un resultado.\n",
    );
    report
}

fn write_summary_tables(comparison: &Value, output: &Path) -> Result<(), Box<dyn Error>> {
    let local = &comparison["local_nastran95"];
    let msc = comparison.get("msc").filter(|value| !value.is_null());
    let mut markdown = String::from(
        "# NASTRAN-95 audit summary\n\n\
| Case | Local SOL 101 tip (m) | MSC SOL 101 tip (m) | Relative difference | MSC von Mises max (Pa) |\n\
|---|---:|---:|---:|---:|\n",
    );
    let mut typst = String::from(
        "#table(\n\
  columns: (auto, auto, auto, auto, auto),\n\
  [Case], [Local tip (m)], [MSC tip (m)], [Relative], [MSC von Mises max (Pa)],\n",
    );
    if let Some(cases) = local["sol101_tip_deflections_m"].as_array() {
        for case in cases {
            let label = case["case"].as_str().map_or("unknown", |value| value);
            let local_tip = case
                .get("local_nastran95_tip_deflection_m")
                .and_then(Value::as_f64);
            let msc_tip = case.get("msc_tip_deflection_m").and_then(Value::as_f64);
            let relative = case
                .get("relative_difference_local_minus_msc")
                .and_then(Value::as_f64)
                .map(|value| 100.0 * value);
            let stress = msc.and_then(|value| stress_for_case(value, label));
            markdown.push_str(&format!(
                "| {label} | {} | {} | {}% | {} |\n",
                number(local_tip),
                number(msc_tip),
                number(relative),
                number(stress)
            ));
            typst.push_str(&format!(
                "  [{label}], [{}], [{}], [{}%], [{}],\n",
                number(local_tip),
                number(msc_tip),
                number(relative),
                number(stress)
            ));
        }
    }
    typst.push_str(")\n\n");
    markdown.push_str(&format!(
        "\nLocal SOL 103 frequencies (Hz): {}\n\nMSC SOL 103 frequencies (Hz): {}\n",
        frequency_list(&local["sol103_frequencies_hz"]),
        msc.map_or_else(
            || "n/a".to_owned(),
            |value| frequency_list(&value["sol103_frequencies_hz"]),
        )
    ));
    typst.push_str(&format!(
        "Local SOL 103 frequencies (Hz): {}\\\\\n\
MSC SOL 103 frequencies (Hz): {}\n",
        frequency_list(&local["sol103_frequencies_hz"]),
        msc.map_or_else(
            || "n/a".to_owned(),
            |value| frequency_list(&value["sol103_frequencies_hz"]),
        )
    ));
    fs::write(output.join("summary_table.md"), markdown)?;
    fs::write(output.join("summary_table.typ"), typst)?;
    Ok(())
}

fn stress_for_case(msc: &Value, label: &str) -> Option<f64> {
    msc["sol101_root_von_mises_max_pa"]
        .as_array()?
        .iter()
        .find(|case| case["case"].as_str() == Some(label))
        .and_then(|case| case["von_mises_max_pa"].as_f64())
}

fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.6}"))
}

fn frequency_list(value: &Value) -> String {
    value.as_array().map_or_else(
        || "n/a".to_owned(),
        |values| {
            values
                .iter()
                .filter_map(Value::as_f64)
                .map(|value| format!("{value:.6}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}
