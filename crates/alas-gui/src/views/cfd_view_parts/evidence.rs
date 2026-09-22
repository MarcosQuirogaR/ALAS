// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless PNG renders of the Airfoil CFD window, for reviewing the layout
//! without a display.
//!
//! Frames are laid out by egui at a fixed size, tessellated, and rasterized
//! by [`super::evidence_raster`], which samples the font atlas per pixel so the
//! text in these images is actually readable. They are layout evidence from a
//! headless test, never a desktop screenshot and never solver evidence.

use super::advanced::show_advanced_tab;
use super::evidence_raster::{render_frame_png, UserTexture, UserTextures};
use super::log::show_log_tab;
use super::results::show_results_tab;
use super::study::show_study_tab;
use crate::state::AppState;
use crate::theme::{apply_theme, AppTheme};
use egui::{pos2, vec2, Context, FullOutput, Rect, Ui};

/// Render one full window body (header, tabs, active tab) at a fixed size.
fn frame(
    state: &mut AppState,
    theme: AppTheme,
    size: egui::Vec2,
    tab: fn(&mut AppState, &mut Ui),
    tab_id: crate::cfd::CfdTab,
) -> (Context, FullOutput, egui::Color32) {
    // The tab strip must highlight the tab actually being drawn: loading a
    // persisted result switches the stored tab to Results, which would
    // otherwise contradict a Study or Advanced render.
    state.cfd.tab = tab_id;
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), size);
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    let mut output = None;
    for _ in 0..3 {
        let raw = egui::RawInput {
            screen_rect: Some(viewport),
            ..Default::default()
        };
        output = Some(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                super::super::show_header(state, ui);
                super::super::show_tabs(state, ui);
                ui.separator();
                tab(state, ui);
            });
        }));
    }
    let background = ctx.style().visuals.panel_fill;
    (ctx, output.expect("three frames"), background)
}

/// A result record whose only purpose is to exercise the Results layout:
/// coefficient table, residual legend, quality formatting.  The numbers are
/// synthetic placeholders and are never presented as solver evidence, which is
/// why the written file name says so.
fn layout_only_result() -> alas_cfd::CfdResults {
    let config = alas_cfd::CfdStudyConfig::default();
    let airfoil = alas_cfd::resolve_airfoil(&config.airfoil_name).unwrap_or_else(|_| {
        alas_cfd::AirfoilSnapshot {
            name: config.airfoil_name.clone(),
            coordinates: Vec::new(),
            coordinate_hash: String::new(),
        }
    });
    let mut residuals = Vec::new();
    for iteration in 1..=400_u64 {
        for (index, field) in ["Ux", "Uy", "k", "omega", "p"].into_iter().enumerate() {
            let decay = -1.0 - 4.0 * (iteration as f64 / 400.0) - index as f64 * 0.4;
            residuals.push(alas_cfd::ResidualSample {
                iteration,
                field: field.to_owned(),
                initial: 10.0_f64.powf(decay),
                final_residual: 10.0_f64.powf(decay - 1.3),
            });
        }
    }
    let forces = (1..=400_u64)
        .map(|iteration| {
            let progress = iteration as f64 / 400.0;
            alas_cfd::ForceSample {
                time: iteration as f64,
                cd: 0.0160,
                cl: 0.8615 * (1.0 - (-6.0 * progress).exp()),
                cm: -0.1470,
                cd_pressure: Some(0.009_031),
                cd_viscous: Some(0.006_983),
                cl_pressure: Some(0.861_499),
                cl_viscous: Some(0.000_119),
            }
        })
        .collect();
    alas_cfd::CfdResults {
        outcome: alas_cfd::CfdOutcome::Unconverged,
        case_dir: std::path::PathBuf::from("layout-only-placeholder-case"),
        provenance: alas_cfd::StudyProvenance {
            template_version: alas_cfd::TEMPLATE_VERSION.to_owned(),
            config: config.clone(),
            airfoil,
            effective_speed_m_s: config.effective_speed_m_s(),
            effective_reynolds: config.effective_reynolds(),
            frame: alas_cfd::FrameConvention::default(),
            reference: None,
            backend: None,
            openfoam_version: None,
            file_hashes: std::collections::BTreeMap::new(),
        },
        residuals,
        forces,
        mass_balance: Vec::new(),
        mesh_quality: alas_cfd::MeshQuality {
            passed: true,
            cells: Some(182_588),
            max_non_orthogonality_deg: Some(59.332_881),
            max_skewness: Some(3.194_727),
            min_volume_m3: Some(3.1e-12),
            ..Default::default()
        },
        fields: Vec::new(),
        surface: None,
        surface_error: Some("layout-only placeholder: no wall samples".to_owned()),
        command_logs: std::collections::BTreeMap::new(),
        field_updates: Default::default(),
        mesh_qualification: Default::default(),
        numerical_convergence: alas_cfd::CfdOutcome::Unconverged,
        status_detail: "Airfoil CFD finished: unconverged.".to_owned(),
    }
}

/// Writes headless renders of every CFD tab to an internal evidence
/// directory (2026-09-16); run with `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_cfd_layout_evidence_images() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/evidence/gui-2026-09-16");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    // (evidence file name stem, theme, language, viewport size, tab renderer,
    // whether a placeholder CFD result must be loaded first)
    type LayoutEvidenceCase = (
        &'static str,
        AppTheme,
        &'static str,
        egui::Vec2,
        fn(&mut AppState, &mut Ui),
        bool,
    );
    let cases: [LayoutEvidenceCase; 9] = [
        (
            "study-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 1000.0),
            show_study_tab,
            false,
        ),
        (
            "study-dark-narrow",
            AppTheme::Dark,
            "en",
            vec2(680.0, 1000.0),
            show_study_tab,
            false,
        ),
        (
            "study-light-wide",
            AppTheme::Light,
            "en",
            vec2(1500.0, 1000.0),
            show_study_tab,
            false,
        ),
        (
            "study-grey-es",
            AppTheme::Grey,
            "es",
            vec2(1500.0, 1000.0),
            show_study_tab,
            false,
        ),
        (
            "advanced-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 1000.0),
            show_advanced_tab,
            false,
        ),
        (
            "advanced-dark-narrow",
            AppTheme::Dark,
            "en",
            vec2(680.0, 1000.0),
            show_advanced_tab,
            false,
        ),
        (
            "results-dark-wide-layout-only",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 1200.0),
            show_results_tab,
            true,
        ),
        (
            "results-dark-narrow-layout-only",
            AppTheme::Dark,
            "en",
            vec2(680.0, 1200.0),
            show_results_tab,
            true,
        ),
        (
            "log-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 600.0),
            show_log_tab,
            false,
        ),
    ];
    for (name, theme, language, size, tab, with_result) in cases {
        use_language(language);
        let mut state = AppState {
            theme,
            ..Default::default()
        };
        if with_result {
            state.cfd.result = Some(layout_only_result());
        }
        let (ctx, output, background) = frame(&mut state, theme, size, tab, tab_from_name(name));
        let png = render_frame_png(&ctx, output, size, background, &decoded_contours(&state));
        std::fs::write(dir.join(format!("cfd-{name}.png")), png).expect("write png");
    }
    use_language("en");
}

/// A completed OpenFOAM case produced by the CFD lane, used so the tabs are
/// reviewed against real parsed evidence instead of a placeholder fixture.
/// Its recorded outcome is `failed` with a long refusal detail; that verdict is
/// rendered exactly as the artifact carries it.
const ACTUAL_RESULT_JSON: &str =
    "../../out/evidence/cfd-convergence-20260916/cases/V1-inletoutlet-coarse/results.json";

/// Writes headless renders driven by the **actual** persisted OpenFOAM result
/// above, through the same `load_result_json` path the desktop uses; run with
/// `--ignored`.  Skips (without failing) when the dispatch-local case is not
/// present in this checkout, since it is git-ignored evidence.
#[test]
#[ignore = "writes evidence images"]
fn write_cfd_actual_result_evidence_images() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let result_json = root.join(ACTUAL_RESULT_JSON);
    if !result_json.is_file() {
        eprintln!(
            "skipped: actual CFD result not present at {}",
            result_json.display()
        );
        return;
    }
    let dir = root.join("../../out/evidence/gui-2026-09-16");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    // (evidence file name stem, theme, language, viewport size, tab renderer)
    type ActualResultEvidenceCase = (
        &'static str,
        AppTheme,
        &'static str,
        egui::Vec2,
        fn(&mut AppState, &mut Ui),
    );
    let cases: [ActualResultEvidenceCase; 8] = [
        (
            "actual-results-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 2600.0),
            show_results_tab,
        ),
        (
            "actual-results-dark-narrow",
            AppTheme::Dark,
            "en",
            vec2(680.0, 3400.0),
            show_results_tab,
        ),
        (
            "actual-results-light-wide",
            AppTheme::Light,
            "en",
            vec2(1500.0, 2600.0),
            show_results_tab,
        ),
        (
            "actual-results-grey-es",
            AppTheme::Grey,
            "es",
            vec2(1500.0, 2600.0),
            show_results_tab,
        ),
        (
            "actual-study-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 1000.0),
            show_study_tab,
        ),
        (
            "actual-study-dark-narrow",
            AppTheme::Dark,
            "en",
            vec2(680.0, 1000.0),
            show_study_tab,
        ),
        (
            "actual-advanced-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 1300.0),
            show_advanced_tab,
        ),
        (
            "actual-log-dark-wide",
            AppTheme::Dark,
            "en",
            vec2(1500.0, 700.0),
            show_log_tab,
        ),
    ];
    for (name, theme, language, size, tab) in cases {
        use_language(language);
        let mut state = AppState {
            theme,
            ..Default::default()
        };
        state
            .cfd
            .load_result_json(&result_json)
            .expect("load actual CFD result");
        let (ctx, output, background) = frame(&mut state, theme, size, tab, tab_from_name(name));
        let png = render_frame_png(&ctx, output, size, background, &decoded_contours(&state));
        std::fs::write(dir.join(format!("cfd-{name}.png")), png).expect("write png");
    }
    use_language("en");
}

/// The `CfdTab` a render is named for, so the tab strip highlights the tab
/// whose content is actually drawn.
fn tab_from_name(name: &str) -> crate::cfd::CfdTab {
    use crate::cfd::CfdTab;
    if name.contains("study") {
        CfdTab::Study
    } else if name.contains("advanced") {
        CfdTab::Advanced
    } else if name.contains("log") {
        CfdTab::Log
    } else {
        CfdTab::Results
    }
}

/// Activate a language for the render.
///
/// `set_language` only sets the thread's language; the Spanish catalog has to
/// be registered first or every `tr` silently falls back to English -- which is
/// what made the earlier "es" renders come out in English.
fn use_language(language: &str) {
    if language == "es" {
        alas_i18n::es::install();
    }
    alas_i18n::set_language(Some(language));
}

/// Attach the verdicts an archived record predates, from backend-authored
/// evidence only.
///
/// The archived `results.json` is read unchanged and never written back. Every
/// verdict comes from the case's newest `reclassified-*.json`, which the CFD
/// lane writes beside the original rather than replacing it, and which carries
/// the solver-only verdict, the combined outcome and the full typed mesh
/// qualification as the classifier produced them. Nothing is recomputed or
/// combined here: reading `current_outcome` as the solver verdict would be
/// wrong, because on these two fine-preset cases it is already mesh-gated.
fn archived_case_with_current_verdicts(case: &str) -> Option<alas_cfd::CfdResults> {
    let case_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/cfd-convergence-20260916/cases")
        .join(case);
    let text = std::fs::read_to_string(case_dir.join("results.json")).ok()?;
    let mut result: alas_cfd::CfdResults = serde_json::from_str(&text).ok()?;
    let record = newest_reclassification(&case_dir)?;
    result.numerical_convergence = parse_outcome(record.get("numerical_convergence")?.as_str()?)?;
    result.outcome = parse_outcome(record.get("current_outcome")?.as_str()?)?;
    result.mesh_qualification =
        serde_json::from_value(record.get("mesh_qualification")?.clone()).ok()?;
    if let Some(updates) = record.get("field_update_evidence") {
        if let Ok(evidence) = serde_json::from_value(updates.clone()) {
            result.field_updates = evidence;
        }
    }
    let detail = record
        .get("current_status_detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&result.status_detail)
        .to_owned();
    result.status_detail = match record
        .get("mesh_qualification_summary")
        .and_then(serde_json::Value::as_str)
    {
        Some(summary) if !result.mesh_qualification.passed => format!("{summary} {detail}"),
        _ => detail,
    };
    Some(result)
}

/// The most recently written reclassification record of a case.
fn newest_reclassification(case_dir: &std::path::Path) -> Option<serde_json::Value> {
    let mut records = std::fs::read_dir(case_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("reclassified-")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by_key(|(modified, _)| *modified);
    let (_, newest) = records.last()?;
    serde_json::from_str(&std::fs::read_to_string(newest).ok()?).ok()
}

/// Match a recorded verdict string against `CfdOutcome::as_str`, so the
/// mapping stays the crate's.
fn parse_outcome(recorded: &str) -> Option<alas_cfd::CfdOutcome> {
    [
        alas_cfd::CfdOutcome::NumericallyConverged,
        alas_cfd::CfdOutcome::Unconverged,
        alas_cfd::CfdOutcome::Cancelled,
        alas_cfd::CfdOutcome::Failed,
    ]
    .into_iter()
    .find(|outcome| outcome.as_str() == recorded)
}

/// Writes the two qualification status examples: a case whose solver verdict
/// and whose mesh both fail, and a case whose solver converged on a mesh that
/// does not meet the declared contract.  Run with `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_cfd_qualification_status_images() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/evidence/gui-2026-09-16");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let size = vec2(1500.0, 1500.0);
    for (name, case) in [
        ("qualification-f6-unproven", "F6-fine-v6mesh"),
        ("qualification-g3-frozen", "G3-fine-p404"),
        ("qualification-p1-mesh-failed", "P1-fine-preltol001"),
    ] {
        let Some(result) = archived_case_with_current_verdicts(case) else {
            eprintln!("skipped {name}: archived case {case} not present");
            continue;
        };
        use_language("en");
        let mut state = AppState {
            theme: AppTheme::Dark,
            ..Default::default()
        };
        state.cfd.config = result.provenance.config.clone();
        state.cfd.refresh_preview();
        state.cfd.result = Some(result);
        let (ctx, output, background) = frame(
            &mut state,
            AppTheme::Dark,
            size,
            show_results_tab,
            crate::cfd::CfdTab::Results,
        );
        let png = render_frame_png(&ctx, output, size, background, &decoded_contours(&state));
        std::fs::write(dir.join(format!("cfd-{name}.png")), png).expect("write png");
    }
}

/// A completed run that carries both parsed wall samples and rendered contour
/// artifacts, so the Cp/Cf and contour cards can be reviewed with real data.
///
/// Its `case_dir` is recorded relative to the repository root; the contour card
/// resolves artifact paths from it, and a test's working directory is the
/// crate, not the root. Only that path is rewritten, to the same directory in
/// absolute form. No recorded value is altered.
fn surface_and_contour_case() -> Option<alas_cfd::CfdResults> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let case_dir = root.join("out/evidence/nasa-kc135-m030-a04-coarse-ti00052");
    let text = std::fs::read_to_string(case_dir.join("results.json")).ok()?;
    let mut result: alas_cfd::CfdResults = serde_json::from_str(&text).ok()?;
    // Not `canonicalize`: on Windows it returns a `\?` verbatim prefix,
    // which would show up in the case-folder row of the evidence image.
    result.case_dir = case_dir;
    Some(result)
}

/// Writes the Cp/Cf and contour review images from that run, wide and narrow.
#[test]
#[ignore = "writes evidence images"]
fn write_cfd_surface_and_contour_images() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/evidence/gui-2026-09-16");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let Some(result) = surface_and_contour_case() else {
        eprintln!("skipped: no case with wall samples and contour artifacts");
        return;
    };
    for (name, language, size) in [
        ("surface-contours-dark-wide", "en", vec2(1500.0, 4600.0)),
        ("surface-contours-dark-narrow", "en", vec2(680.0, 5200.0)),
        ("surface-contours-grey-es", "es", vec2(1500.0, 4600.0)),
    ] {
        use_language(language);
        let mut state = AppState {
            theme: AppTheme::Dark,
            ..Default::default()
        };
        if name.contains("grey") {
            state.theme = AppTheme::Grey;
        }
        state.cfd.config = result.provenance.config.clone();
        state.cfd.refresh_preview();
        state.cfd.result = Some(result.clone());
        let theme = state.theme;
        let (ctx, output, background) = frame(
            &mut state,
            theme,
            size,
            show_results_tab,
            crate::cfd::CfdTab::Results,
        );
        let png = render_frame_png(&ctx, output, size, background, &decoded_contours(&state));
        std::fs::write(dir.join(format!("cfd-{name}.png")), png).expect("write png");
    }
    use_language("en");
}

/// Decode the contour artifacts the Results tab actually loaded, keyed by the
/// texture id it registered them under.
///
/// The UI owns the load: `contour_textures` is filled while the frame is drawn,
/// from files on disk. This reads the same files with the same decoder the card
/// uses, so the rasterizer draws the artifact the UI is showing and nothing
/// else. A file that fails to decode is simply absent, which leaves its mesh
/// unpainted rather than filled.
fn decoded_contours(state: &AppState) -> UserTextures {
    state
        .cfd
        .contour_textures
        .iter()
        .filter_map(|(path, handle)| {
            let bytes = std::fs::read(path).ok()?;
            let icon = eframe::icon_data::from_png_bytes(&bytes).ok()?;
            Some((
                handle.id(),
                UserTexture {
                    width: icon.width as usize,
                    height: icon.height as usize,
                    rgba: icon.rgba,
                },
            ))
        })
        .collect()
}

/// Write a vertical slice of an already-written render, at 1:1, so a band of a
/// very tall page can be reviewed without downscaling the whole image.
fn write_cropped_band(
    source: &std::path::Path,
    target: &std::path::Path,
    rows: std::ops::Range<usize>,
) {
    let Ok(bytes) = std::fs::read(source) else {
        return;
    };
    let Ok(image) = eframe::icon_data::from_png_bytes(&bytes) else {
        return;
    };
    let width = image.width as usize;
    let height = image.height as usize;
    let start = rows.start.min(height);
    let end = rows.end.min(height);
    if end <= start {
        return;
    }
    let slice = image.rgba[start * width * 4..end * width * 4].to_vec();
    if let Ok(png) = alas_viz::raster::encode_png_rgba(width as u32, (end - start) as u32, &slice) {
        let _ = std::fs::write(target, png);
    }
}

/// Crops the contour band out of the wide surface render for 1:1 review.
#[test]
#[ignore = "writes evidence images"]
fn write_cfd_contour_band_image() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/evidence/gui-2026-09-16");
    write_cropped_band(
        &dir.join("cfd-surface-contours-dark-wide.png"),
        &dir.join("cfd-contour-band-dark.png"),
        2600..3720,
    );
}
