// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    archive_manifest, export_readiness, figure_file_name, sections_from_figures, summary_scene,
    unused_output_path, utc_stamp, without_repeated_findings, wrap_summary_line, write_stored_zip,
    ExportError, ExportedFigure, FIGURE_ARCHIVE_NAME,
};
use alas_report::{Color, Scene, SceneElement};
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_figure(file_name: &str, category: &str, title: &str) -> ExportedFigure {
    ExportedFigure {
        file_name: file_name.to_owned(),
        category: category.to_owned(),
        title: title.to_owned(),
        scene: Scene::new(100.0, 80.0, Some(Color::rgb(255, 255, 255))),
    }
}

fn temporary_path(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!("alas-export-{name}-{nonce}"))
}

#[test]
fn archive_names_preserve_registry_identity_and_manifest_order() {
    assert_eq!(figure_file_name(10, "threeview"), "10_threeview.svg");
    let figures = vec![
        sample_figure("00_design_summary.svg", "Design Summary", "Design Summary"),
        sample_figure("01_aero_panel.svg", "Aerodynamics", "Lift and Drag"),
        sample_figure("02_mission_profile.svg", "Mission", "Mission Profile"),
    ];
    let manifest = archive_manifest(&figures);

    assert!(
        manifest.find("00_design_summary.svg").expect("summary")
            < manifest.find("01_aero_panel.svg").expect("aero")
    );
    assert!(
        manifest.find("01_aero_panel.svg").expect("aero")
            < manifest.find("02_mission_profile.svg").expect("mission")
    );
}

#[test]
fn live_snapshots_are_rejected_until_the_final_result_is_available() {
    assert!(matches!(
        export_readiness(true, false),
        Err(ExportError::IncompletePipelineRun)
    ));
    assert!(matches!(
        export_readiness(false, false),
        Err(ExportError::NoPipelineRun)
    ));
    assert!(export_readiness(true, true).is_ok());
}

#[test]
fn report_sections_preserve_first_seen_category_and_figure_order() {
    let figures = vec![
        sample_figure("00_design_summary.svg", "Design Summary", "Design Summary"),
        sample_figure("01_aero_panel.svg", "Aerodynamics", "Lift and Drag"),
        sample_figure("02_drag.svg", "Aerodynamics", "Drag"),
        sample_figure("03_mission_profile.svg", "Mission", "Mission Profile"),
    ];
    let sections = sections_from_figures(&figures);

    assert_eq!(
        sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>(),
        ["Design Summary", "Aerodynamics", "Mission"]
    );
    assert_eq!(
        sections[1]
            .figures
            .iter()
            .map(|figure| figure.file_name.as_str())
            .collect::<Vec<_>>(),
        ["01_aero_panel.svg", "02_drag.svg"]
    );
}

#[test]
fn archive_write_error_names_the_failed_output_path() {
    let parent = temporary_path("blocked-parent");
    std::fs::write(&parent, b"not a directory").expect("block parent path");
    let path = parent.join("alas-figures.zip");
    let result = write_stored_zip(&path, &[("one.svg".to_owned(), b"svg".to_vec())]);
    let _ = std::fs::remove_file(&parent);

    match result {
        Err(ExportError::Io {
            path: error_path, ..
        }) => assert_eq!(error_path, parent),
        _ => panic!("expected a directory creation error"),
    }
}

#[test]
fn archive_writes_svg_entries_in_manifest_order() {
    let directory = temporary_path("archive-order");
    std::fs::create_dir_all(&directory).expect("create archive directory");
    let path = directory.join("alas-figures.zip");
    let entries = vec![
        ("manifest.txt".to_owned(), b"manifest".to_vec()),
        ("00_design_summary.svg".to_owned(), b"summary".to_vec()),
        ("01_aero_panel.svg".to_owned(), b"aero".to_vec()),
    ];

    write_stored_zip(&path, &entries).expect("write stored archive");
    let bytes = std::fs::read(&path).expect("read stored archive");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(
        zip_local_entry_names(&bytes),
        ["manifest.txt", "00_design_summary.svg", "01_aero_panel.svg"]
    );
}

fn zip_local_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while bytes.get(offset..offset + 4) == Some(&[0x50, 0x4b, 0x03, 0x04]) {
        let compressed_size = u32::from_le_bytes(
            bytes[offset + 18..offset + 22]
                .try_into()
                .expect("size bytes"),
        ) as usize;
        let name_length = u16::from_le_bytes(
            bytes[offset + 26..offset + 28]
                .try_into()
                .expect("name length"),
        ) as usize;
        let extra_length = u16::from_le_bytes(
            bytes[offset + 28..offset + 30]
                .try_into()
                .expect("extra length"),
        ) as usize;
        let name_start = offset + 30;
        let name_end = name_start + name_length;
        entries.push(
            String::from_utf8(bytes[name_start..name_end].to_vec()).expect("UTF-8 entry name"),
        );
        offset = name_end + extra_length + compressed_size;
    }
    entries
}

#[test]
fn the_table_driven_crc32_matches_the_standard_check_value() {
    assert_eq!(super::crc32(b"123456789"), 0xcbf4_3926);
    assert_eq!(super::crc32(b""), 0);
}

#[test]
fn the_utc_stamp_is_the_calendar_date_and_time_of_the_unix_second() {
    assert_eq!(utc_stamp(0), "19700101-000000");
    // 2026-11-17 09:30:15 UTC.
    assert_eq!(utc_stamp(1_794_907_815), "20261117-093015");
    // Leap day.
    assert_eq!(utc_stamp(951_782_400 + 3_661), "20000229-010101");
}

#[test]
fn an_export_is_stamped_absolute_and_never_overwrites_an_earlier_one() {
    let dir = temporary_path("unused-output");
    std::fs::create_dir_all(&dir).expect("temporary directory");
    let first = unused_output_path(&dir, FIGURE_ARCHIVE_NAME);
    assert!(first.is_absolute(), "{first:?}");
    let name = first
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    assert!(
        name.starts_with("alas-figures-") && name.ends_with(".zip"),
        "{name}"
    );
    std::fs::write(&first, b"earlier archive").expect("write");
    let second = unused_output_path(&dir, FIGURE_ARCHIVE_NAME);
    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).expect("read"), b"earlier archive");
}

#[test]
fn summary_lines_are_wrapped_to_the_page_and_follow_the_figure_theme() {
    let long = format!(
        "Cruise point : {}",
        "design mission analysed loading ".repeat(8)
    );
    let pieces = wrap_summary_line(&long, 60);
    assert!(pieces.len() > 1);
    assert!(
        pieces.iter().all(|piece| piece.chars().count() <= 62),
        "{pieces:?}"
    );
    let scene = summary_scene(format!("=== ALAS Design Summary ===\n{long}"), "dark");
    let dark = alas_report::theme::get_palette(Some("dark"));
    assert_eq!(scene.background, Some(Color::from_hex(dark.bg)));
    let heights = scene.height;
    assert!(heights > 100.0);
    for element in &scene.elements {
        if let SceneElement::Text { pos, .. } = element {
            assert!(pos[1] < scene.height, "{pos:?} outside {heights}");
        }
    }
}

/// With `ALAS_PREVIEW_SVG_DIR` set, the summary page is written there in both themes for a
/// visual check; without it the pages are only built.
#[test]
fn the_summary_page_in_both_themes_has_no_line_wider_than_the_page() {
    let summary = [
        "=========== ALAS Design Summary ===========",
        "Cruise alpha (reported): 0.69 deg",
        "Cruise point : design mission: analysed loading, full mission fuel; from 73707 kg, mission-flown cruise CL 0.570 at the design altitude with the reserve fuel carried",
        "Trimmed body alpha: 0.74 deg (reported = 0.74 deg, h-stab incidence = 1.42 deg, L/D = 17.30)",
        "MTOW (specified) : 78000 kg",
        "Span : 34.22 m",
        "Wing area : 122.6 m\u{b2}",
        "Route cruise pt : Madrid Barajas (LEMD)-Palma de Mallorca (LEPA), 645 km from 58985 kg",
        "Delivery verdict: ACCEPTED - FEASIBLE (calibrated reference preset)",
    ]
    .join("\n");
    for theme in ["dark", "light"] {
        let scene = summary_scene(summary.clone(), theme);
        for element in &scene.elements {
            if let SceneElement::Text {
                text,
                pos,
                font_size,
                ..
            } = element
            {
                let width = text.chars().count() as f64
                    * font_size
                    * alas_report::scene::CSS_PIXELS_PER_POINT
                    * 0.56;
                assert!(
                    pos[0] + width <= scene.width,
                    "{theme}: {text:?} runs off the page"
                );
            }
        }
        if let Some(dir) = std::env::var_os("ALAS_PREVIEW_SVG_DIR") {
            let path = std::path::Path::new(&dir).join(format!("summary_{theme}.svg"));
            std::fs::write(path, alas_report::render_svg(&scene)).expect("write svg");
        }
    }
}

#[test]
fn a_finding_listed_by_both_the_summary_and_the_delivery_is_exported_once() {
    let text = "Summary\n- WARNING: a\n- WARNING: b\nDelivery verdict: ok\n  - WARNING: a\n- ERROR: c\nCruise : 1";
    assert_eq!(
        without_repeated_findings(text),
        "Summary\n- WARNING: a\n- WARNING: b\nDelivery verdict: ok\n- ERROR: c\nCruise : 1"
    );
}
