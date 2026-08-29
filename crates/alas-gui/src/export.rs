// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ordered figure archive and sectioned PDF export for completed GUI runs.
//!
//! The export contract deliberately stays within the existing scene and SVG
//! architecture: the ZIP uses stored entries written by the standard library,
//! and the PDF is rendered from the same scene as its archived SVG. This keeps
//! report-quality vector sources available without adding an archive, PDF, or
//! rasterization dependency.

use std::fmt;
use std::fs::File;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};

use alas_report::scene::{TextAlign, TextBaseline};
use alas_report::{Color, PdfFigure, PdfSection, Scene, SceneElement};

use crate::scene::build_result_figure;
use crate::state::AppState;

/// Stable name of the user-facing figure archive.
pub const FIGURE_ARCHIVE_NAME: &str = "alas-figures.zip";
/// Stable name of the user-facing, sectioned PDF report.
pub const PDF_REPORT_NAME: &str = "alas-design-report.pdf";
const ARCHIVE_MANIFEST_NAME: &str = "manifest.txt";

#[derive(Debug, Clone, PartialEq)]
struct ExportedFigure {
    file_name: String,
    category: String,
    title: String,
    scene: Scene,
}

/// Locations and figure count written by an export action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReceipt {
    /// Output file created by the action.
    pub path: PathBuf,
    /// Number of SVG figure sources included in the output.
    pub figure_count: usize,
}

/// Errors that can prevent export from completing.
#[derive(Debug)]
pub enum ExportError {
    /// The GUI has no result data from which to build figures.
    NoPipelineRun,
    /// Baseline-only data cannot form the existing design summary report.
    NoOptimizedReport,
    /// The registry supplied no scenes that this run could export.
    NoFigures,
    /// A scene was rejected by the vector PDF backend.
    Pdf(alas_report::PdfError),
    /// An output path could not be created or written.
    Io {
        /// The attempted filesystem action.
        operation: &'static str,
        /// Path involved in the action.
        path: PathBuf,
        /// Operating-system error.
        source: io::Error,
    },
    /// The dependency-free ZIP writer supports the standard 32-bit ZIP size.
    ArchiveEntryTooLarge {
        /// Entry name that could not be represented.
        name: String,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPipelineRun => formatter.write_str("no completed pipeline run exists"),
            Self::NoOptimizedReport => {
                formatter.write_str("this run has no optimized report to export")
            }
            Self::NoFigures => {
                formatter.write_str("this run produced no exportable result figures")
            }
            Self::Pdf(error) => error.fmt(formatter),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} '{}': {source}",
                path.display()
            ),
            Self::ArchiveEntryTooLarge { name } => {
                write!(
                    formatter,
                    "archive entry '{name}' exceeds the ZIP size limit"
                )
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pdf(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<alas_report::PdfError> for ExportError {
    fn from(error: alas_report::PdfError) -> Self {
        Self::Pdf(error)
    }
}

/// Export every available result figure as ordered SVG files in one ZIP archive.
pub fn export_figure_archive(
    state: &AppState,
    output_dir: &Path,
) -> Result<ExportReceipt, ExportError> {
    let figures = collect_export_figures(state)?;
    let path = output_dir.join(FIGURE_ARCHIVE_NAME);
    write_archive(&path, &figures)?;
    Ok(ExportReceipt {
        path,
        figure_count: figures.len(),
    })
}

/// Export the same ordered figures as a sectioned vector PDF report.
pub fn export_pdf_report(
    state: &AppState,
    output_dir: &Path,
) -> Result<ExportReceipt, ExportError> {
    let figures = collect_export_figures(state)?;
    let document = alas_report::render_sectioned_pdf(&sections_from_figures(&figures))?;
    let path = output_dir.join(PDF_REPORT_NAME);
    write_output(&path, &document, "write PDF report")?;
    Ok(ExportReceipt {
        path,
        figure_count: figures.len(),
    })
}

fn collect_export_figures(state: &AppState) -> Result<Vec<ExportedFigure>, ExportError> {
    let result = state
        .pipeline_result
        .as_ref()
        .ok_or(ExportError::NoPipelineRun)?;
    let report = result
        .optimized_report
        .as_ref()
        .ok_or(ExportError::NoOptimizedReport)?;
    let theme = state.theme.figure_theme_name().to_owned();
    let mut figures = vec![ExportedFigure {
        file_name: "00_design_summary.svg".to_owned(),
        category: "Design Summary".to_owned(),
        title: "Design Summary".to_owned(),
        scene: summary_scene(format!(
            "{}\n{}",
            alas_report::format_summary(report, Some(&result.config)),
            alas_pipeline::format_feasibility(&result.feasibility)
        )),
    }];

    for (index, descriptor) in alas_report::RESULT_FIGURES.iter().enumerate() {
        if let Some(Some(scene)) = build_result_figure(state, descriptor.id, &result.config, &theme)
        {
            figures.push(ExportedFigure {
                file_name: figure_file_name(index + 1, descriptor.id),
                category: descriptor.category.to_owned(),
                title: descriptor.title.to_owned(),
                scene,
            });
        }
    }
    if figures.len() == 1 {
        return Err(ExportError::NoFigures);
    }
    Ok(figures)
}

fn figure_file_name(registry_position: usize, id: &str) -> String {
    format!(
        "{registry_position:02}_{}.svg",
        alas_report::export_file_stem(id)
    )
}

fn summary_scene(summary: String) -> Scene {
    let mut scene = Scene::new(640.0, 760.0, Some(Color::rgb(255, 255, 255)));
    scene.title = Some("ALAS Design Summary".to_owned());
    for (index, line) in summary.lines().enumerate() {
        let y = 42.0 + index as f64 * 20.0;
        scene.add(SceneElement::Text {
            text: line.to_owned(),
            pos: [36.0, y],
            font_size: if index == 0 { 17.0 } else { 11.5 },
            color: Color::rgb(15, 23, 42),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: index == 0,
        });
    }
    scene
}

fn sections_from_figures(figures: &[ExportedFigure]) -> Vec<PdfSection> {
    let mut sections = Vec::<PdfSection>::new();
    for figure in figures {
        if sections
            .last()
            .is_none_or(|section| section.title != figure.category)
        {
            sections.push(PdfSection {
                title: figure.category.clone(),
                figures: Vec::new(),
            });
        }
        if let Some(section) = sections.last_mut() {
            section.figures.push(PdfFigure {
                file_name: figure.file_name.clone(),
                title: figure.title.clone(),
                scene: figure.scene.clone(),
            });
        }
    }
    sections
}

fn write_archive(path: &Path, figures: &[ExportedFigure]) -> Result<(), ExportError> {
    let mut entries = Vec::with_capacity(figures.len() + 1);
    entries.push((
        ARCHIVE_MANIFEST_NAME.to_owned(),
        archive_manifest(figures).into_bytes(),
    ));
    for figure in figures {
        entries.push((
            figure.file_name.clone(),
            alas_report::render_svg(&figure.scene).into_bytes(),
        ));
    }
    write_stored_zip(path, &entries)
}

fn archive_manifest(figures: &[ExportedFigure]) -> String {
    let mut manifest = String::from("ALAS Figure Archive\n\nOrder\tSection\tFile\tTitle\n");
    for (index, figure) in figures.iter().enumerate() {
        manifest.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            index + 1,
            figure.category,
            figure.file_name,
            figure.title
        ));
    }
    manifest
}

fn write_stored_zip(path: &Path, entries: &[(String, Vec<u8>)]) -> Result<(), ExportError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|source| io_error("create export directory", parent, source))?;
    let mut file =
        File::create(path).map_err(|source| io_error("create figure archive", path, source))?;
    let mut central_entries = Vec::with_capacity(entries.len());

    for (name, contents) in entries {
        let name_bytes = name.as_bytes();
        let name_len = zip_u16(name, name_bytes.len())?;
        let contents_len = zip_u32(name, contents.len())?;
        let offset = zip_u32_from_u64(
            name,
            file.stream_position()
                .map_err(|source| io_error("write figure archive", path, source))?,
        )?;
        let crc = crc32(contents);
        write_u32(&mut file, 0x0403_4b50, path)?;
        write_u16(&mut file, 20, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u32(&mut file, crc, path)?;
        write_u32(&mut file, contents_len, path)?;
        write_u32(&mut file, contents_len, path)?;
        write_u16(&mut file, name_len, path)?;
        write_u16(&mut file, 0, path)?;
        write_all(&mut file, name_bytes, path)?;
        write_all(&mut file, contents, path)?;
        central_entries.push((name_bytes, crc, contents_len, offset));
    }

    let central_offset = zip_u32_from_u64(
        name_from_path(path),
        file.stream_position()
            .map_err(|source| io_error("write figure archive", path, source))?,
    )?;
    for (name, crc, contents_len, offset) in &central_entries {
        let name_len = zip_u16(name_from_path(path), name.len())?;
        write_u32(&mut file, 0x0201_4b50, path)?;
        write_u16(&mut file, 20, path)?;
        write_u16(&mut file, 20, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u32(&mut file, *crc, path)?;
        write_u32(&mut file, *contents_len, path)?;
        write_u32(&mut file, *contents_len, path)?;
        write_u16(&mut file, name_len, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u16(&mut file, 0, path)?;
        write_u32(&mut file, 0, path)?;
        write_u32(&mut file, *offset, path)?;
        write_all(&mut file, name, path)?;
    }
    let central_end = file
        .stream_position()
        .map_err(|source| io_error("write figure archive", path, source))?;
    let central_size = zip_u32_from_u64(
        name_from_path(path),
        central_end
            .checked_sub(u64::from(central_offset))
            .ok_or_else(|| ExportError::ArchiveEntryTooLarge {
                name: name_from_path(path).to_owned(),
            })?,
    )?;
    let count = zip_u16("archive entry count", central_entries.len())?;
    write_u32(&mut file, 0x0605_4b50, path)?;
    write_u16(&mut file, 0, path)?;
    write_u16(&mut file, 0, path)?;
    write_u16(&mut file, count, path)?;
    write_u16(&mut file, count, path)?;
    write_u32(&mut file, central_size, path)?;
    write_u32(&mut file, central_offset, path)?;
    write_u16(&mut file, 0, path)
}

fn write_output(path: &Path, contents: &[u8], operation: &'static str) -> Result<(), ExportError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|source| io_error("create export directory", parent, source))?;
    std::fs::write(path, contents).map_err(|source| io_error(operation, path, source))
}

fn zip_u16(name: &str, value: usize) -> Result<u16, ExportError> {
    u16::try_from(value).map_err(|_| ExportError::ArchiveEntryTooLarge {
        name: name.to_owned(),
    })
}

fn zip_u32(name: &str, value: usize) -> Result<u32, ExportError> {
    u32::try_from(value).map_err(|_| ExportError::ArchiveEntryTooLarge {
        name: name.to_owned(),
    })
}

fn zip_u32_from_u64(name: &str, value: u64) -> Result<u32, ExportError> {
    u32::try_from(value).map_err(|_| ExportError::ArchiveEntryTooLarge {
        name: name.to_owned(),
    })
}

fn name_from_path(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive")
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> ExportError {
    ExportError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

fn write_all(file: &mut File, contents: &[u8], path: &Path) -> Result<(), ExportError> {
    file.write_all(contents)
        .map_err(|source| io_error("write figure archive", path, source))
}

fn write_u16(file: &mut File, value: u16, path: &Path) -> Result<(), ExportError> {
    write_all(file, &value.to_le_bytes(), path)
}

fn write_u32(file: &mut File, value: u32, path: &Path) -> Result<(), ExportError> {
    write_all(file, &value.to_le_bytes(), path)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
        }
    }
    !crc
}

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
