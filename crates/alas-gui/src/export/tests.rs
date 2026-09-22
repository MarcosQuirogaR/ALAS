// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    archive_manifest, export_readiness, figure_file_name, sections_from_figures, write_stored_zip,
    ExportError, ExportedFigure,
};
use alas_report::{Color, Scene};
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
