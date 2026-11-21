// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the preview pane shows for one item: a thumbnail (the aircraft's top
//! view, a run's first summary figure or a PNG), a few facts read from the
//! file's header, and the start of its text. Built on a worker thread, once
//! per selection, never per frame.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use super::files_model::{classify, format_modified, ItemKind};

/// Largest file read for a preview, in bytes.
const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
/// Largest PNG decoded for a thumbnail, in bytes.
const MAX_IMAGE_BYTES: u64 = 6 * 1024 * 1024;
/// Longest side of a decoded thumbnail, in pixels.
const MAX_IMAGE_SIDE: usize = 640;
/// Lines and characters of text shown.
const MAX_LINES: usize = 40;
const MAX_CHARS: usize = 6000;

/// What the preview pane shows.
#[derive(Default)]
pub(super) struct Preview {
    pub title: String,
    pub kind: Option<ItemKind>,
    /// `(label, value)` pairs; labels go through the translation catalogue.
    pub facts: Vec<(String, String)>,
    pub text: String,
    /// The aircraft's plan view, for a file that holds an aircraft.
    pub scene: Option<Arc<alas_report::scene::Scene>>,
    /// A picture to show, until the page turns it into a texture.
    pub image: Option<egui::ColorImage>,
}

/// The first PNG of a run folder: at the top, then in `plots`.
fn first_figure(folder: &Path) -> Option<PathBuf> {
    for dir in [folder.to_path_buf(), folder.join("plots")] {
        let Ok(items) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut pngs: Vec<PathBuf> = items
            .flatten()
            .map(|item| item.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
            .collect();
        pngs.sort();
        if let Some(first) = pngs.into_iter().next() {
            return Some(first);
        }
    }
    None
}

/// Decode a PNG into a picture no larger than [`MAX_IMAGE_SIDE`] a side.
fn decode_png(path: &Path) -> Option<egui::ColorImage> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let icon = eframe::icon_data::from_png_bytes(&bytes).ok()?;
    let (width, height) = (icon.width as usize, icon.height as usize);
    let step = width.max(height).div_ceil(MAX_IMAGE_SIDE).max(1);
    if step == 1 {
        return Some(egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            &icon.rgba,
        ));
    }
    let (small_w, small_h) = (width / step, height / step);
    let mut rgba = Vec::with_capacity(small_w * small_h * 4);
    for y in 0..small_h {
        for x in 0..small_w {
            let at = ((y * step) * width + x * step) * 4;
            rgba.extend_from_slice(icon.rgba.get(at..at + 4)?);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [small_w, small_h],
        &rgba,
    ))
}

fn text_of(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or_default()
}

fn json_facts(
    value: &Value,
    facts: &mut Vec<(String, String)>,
    theme: &str,
) -> (Option<String>, Option<Arc<alas_report::scene::Scene>>) {
    let mut title = None;
    if let Some(header) = value.get("alas_my_aircraft") {
        title = Some(text_of(header.get("name")).to_owned());
        facts.push(("Name".to_owned(), text_of(header.get("name")).to_owned()));
        if let Some(unix) = header.get("saved_unix").and_then(Value::as_u64) {
            facts.push(("Saved".to_owned(), format_modified(Some(unix))));
        }
    } else if let Some(header) = value.get("alas_object") {
        title = Some(text_of(header.get("name")).to_owned());
        facts.push(("Name".to_owned(), text_of(header.get("name")).to_owned()));
        if let Some(unix) = header.get("saved_unix").and_then(Value::as_u64) {
            facts.push(("Saved".to_owned(), format_modified(Some(unix))));
        }
    } else if let Some(airports) = value
        .as_array()
        .filter(|list| list.iter().all(|each| each.get("icao").is_some()))
    {
        let names: Vec<String> = airports
            .iter()
            .map(|a| format!("{} {}", text_of(a.get("icao")), text_of(a.get("name"))))
            .collect();
        facts.push(("Airports".to_owned(), names.join(", ")));
    }
    let mut scene = None;
    if let Some((document, _)) = crate::views::files_thumbnail::aircraft_document(value) {
        if let Ok(config) = alas_config::AlasConfig::from_value(document) {
            if !config.preset.is_empty() {
                facts.push(("Based on".to_owned(), config.preset.clone()));
            }
            facts.push((
                "MTOW".to_owned(),
                format!("{:.0} kg", config.requirements.mtow_kg),
            ));
            facts.push((
                "Passengers".to_owned(),
                config.requirements.num_passengers.to_string(),
            ));
        }
        scene = crate::views::files_thumbnail::aircraft_scene(value, theme).map(Arc::new);
    }
    (title.filter(|name| !name.is_empty()), scene)
}

fn run_facts(folder: &Path, facts: &mut Vec<(String, String)>) {
    let manifest = folder.join("run_manifest.json");
    let value = std::fs::metadata(&manifest)
        .ok()
        .filter(|meta| meta.len() <= MAX_PREVIEW_BYTES)
        .and_then(|_| std::fs::read_to_string(&manifest).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let Some(value) = value else {
        facts.push(("Status".to_owned(), "No manifest".to_owned()));
        return;
    };
    let failures = value
        .get("tool_failures")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    facts.push(("Status".to_owned(), "Completed".to_owned()));
    if failures > 0 {
        facts.push(("Tool failures".to_owned(), failures.to_string()));
    }
    if let Some(seconds) = value.get("total_wall_time_s").and_then(Value::as_f64) {
        facts.push(("Duration".to_owned(), format!("{seconds:.0} s")));
    }
    let method = text_of(value.pointer("/search/executed_method"));
    if !method.is_empty() {
        facts.push(("Search".to_owned(), method.to_owned()));
    }
}

fn first_lines(raw: &str) -> String {
    raw.lines()
        .take(MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(MAX_CHARS)
        .collect()
}

/// Build the preview of `path` (a worker-thread job). `theme` is the figure
/// theme name the aircraft thumbnail is drawn in.
pub(super) fn make_preview(path: &Path, theme: &str) -> Preview {
    let mut preview = Preview {
        title: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..Preview::default()
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return preview;
    };
    let is_dir = meta.is_dir();
    let run_folder = is_dir
        && (path.join("run_manifest.json").is_file()
            || path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("run-")));
    let mut head = Vec::new();
    if !is_dir && meta.len() <= MAX_PREVIEW_BYTES {
        if let Ok(raw) = std::fs::read(path) {
            head = raw.iter().copied().take(1024).collect();
        }
    }
    let kind = classify(path, is_dir, run_folder, &head);
    preview.kind = Some(kind);
    if is_dir {
        if run_folder {
            preview.facts.push((
                "Date".to_owned(),
                format_modified(
                    meta.modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                ),
            ));
            run_facts(path, &mut preview.facts);
            preview.image = first_figure(path).and_then(|figure| decode_png(&figure));
        }
        return preview;
    }
    if kind == ItemKind::Image {
        preview.image = decode_png(path);
        return preview;
    }
    let textual = matches!(
        path.extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some(
            "json"
                | "yaml"
                | "yml"
                | "txt"
                | "md"
                | "csv"
                | "tsv"
                | "log"
                | "dat"
                | "toml"
                | "vsp3"
                | "xml"
        )
    );
    if !textual || meta.len() > MAX_PREVIEW_BYTES {
        return preview;
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return preview;
    };
    let ext_json = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let shown = match ext_json.then(|| serde_json::from_str::<Value>(&raw)) {
        Some(Ok(value)) => {
            let (title, scene) = json_facts(&value, &mut preview.facts, theme);
            if let Some(title) = title {
                preview.title = title;
            }
            preview.scene = scene;
            serde_json::to_string_pretty(&value).unwrap_or(raw)
        }
        _ => raw,
    };
    preview.text = first_lines(&shown);
    preview
}

/// Only the thumbnail of an aircraft file (what the large-icons view needs).
pub(super) fn make_thumbnail(path: &Path, theme: &str) -> Option<Arc<alas_report::scene::Scene>> {
    if std::fs::metadata(path).ok()?.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    crate::views::files_thumbnail::aircraft_scene(&value, theme).map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_folder_preview_reads_the_manifest_status() {
        let dir = std::env::temp_dir().join(format!("alas-files-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let run = dir.join("run-1");
        std::fs::create_dir_all(&run).expect("run");
        std::fs::write(
            run.join("run_manifest.json"),
            r#"{"tool_failures": ["avl"], "total_wall_time_s": 41.6, "search": {"executed_method": "lshade"}}"#,
        )
        .expect("manifest");
        let preview = make_preview(&run, "dark");
        assert_eq!(preview.kind, Some(ItemKind::RunFolder));
        let fact = |label: &str| {
            preview
                .facts
                .iter()
                .find(|(name, _)| name == label)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(fact("Status").as_deref(), Some("Completed"));
        assert_eq!(fact("Tool failures").as_deref(), Some("1"));
        assert_eq!(fact("Duration").as_deref(), Some("42 s"));
        assert_eq!(fact("Search").as_deref(), Some("lshade"));
        assert!(preview.image.is_none());
    }

    #[test]
    fn a_saved_aircraft_preview_lists_its_header_facts_and_a_top_view() {
        let dir = std::env::temp_dir().join(format!("alas-files-ac-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let config = serde_json::to_value(alas_config::AlasConfig::default()).expect("config");
        let file = serde_json::json!({
            "alas_my_aircraft": {"name": "Demo", "saved_unix": 0},
            "workspace": config
        });
        let path = dir.join("demo.json");
        std::fs::write(&path, serde_json::to_string(&file).expect("json")).expect("write");
        let preview = make_preview(&path, "dark");
        assert_eq!(preview.kind, Some(ItemKind::Aircraft));
        assert_eq!(preview.title, "Demo");
        for label in ["Name", "MTOW", "Passengers"] {
            assert!(
                preview.facts.iter().any(|(name, _)| name == label),
                "{label}"
            );
        }
        assert!(preview.scene.is_some());
    }
}
