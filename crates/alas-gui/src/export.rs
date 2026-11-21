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
    /// A live snapshot contains figures, but the pipeline has not reached its
    /// final feasibility and export stages.
    IncompletePipelineRun,
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
            Self::IncompletePipelineRun => formatter.write_str(
                "the pipeline result is incomplete; wait for the run to finish before exporting",
            ),
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

/// `scene` for a standalone SVG: a Blue Marble globe gets its rasterized texture embedded, which
/// the vector writer alone cannot draw (it paints only the dark disc the GUI draws the Earth on).
pub fn embed_globe_textures(scene: &Scene) -> Scene {
    alas_viz::with_embedded_globe_textures(scene, alas_viz::EXPORT_GLOBE_SCALE)
}

/// Export every available result figure as ordered SVG files in one ZIP archive.
pub fn export_figure_archive(
    state: &AppState,
    output_dir: &Path,
) -> Result<ExportReceipt, ExportError> {
    let figures = collect_export_figures(state)?;
    let path = unused_output_path(output_dir, FIGURE_ARCHIVE_NAME);
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
    let path = unused_output_path(output_dir, PDF_REPORT_NAME);
    write_output(&path, &document, "write PDF report")?;
    Ok(ExportReceipt {
        path,
        figure_count: figures.len(),
    })
}

/// `base_name` with a UTC time stamp before its extension ("alas-figures-20261117-093015.zip"),
/// inside `output_dir` made absolute; a name already on disk gets a counter, so an earlier
/// archive is never overwritten.
fn unused_output_path(output_dir: &Path, base_name: &str) -> PathBuf {
    let dir = std::path::absolute(output_dir).unwrap_or_else(|_| output_dir.to_path_buf());
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let stamp = utc_stamp(seconds);
    let (stem, extension) = base_name.rsplit_once('.').unwrap_or((base_name, ""));
    let name = |counter: usize| {
        let suffix = if counter == 0 {
            String::new()
        } else {
            format!("-{counter}")
        };
        if extension.is_empty() {
            format!("{stem}-{stamp}{suffix}")
        } else {
            format!("{stem}-{stamp}{suffix}.{extension}")
        }
    };
    (0..10_000)
        .map(|counter| dir.join(name(counter)))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| dir.join(name(0)))
}

/// `YYYYMMDD-HHMMSS` of a Unix time in UTC (proleptic Gregorian calendar).
fn utc_stamp(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let second_of_day = unix_seconds % 86_400;
    // Civil-from-days (H. Hinnant): days since 1970-01-01 to year, month, day.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60
    )
}

fn collect_export_figures(state: &AppState) -> Result<Vec<ExportedFigure>, ExportError> {
    export_readiness(
        state.pipeline_result.is_some(),
        state.pipeline_result_complete,
    )?;
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
        scene: summary_scene(
            without_repeated_findings(&format!(
                "{}\n{}",
                alas_report::format_summary(report, Some(&result.config)),
                result.format_delivery()
            )),
            &theme,
        ),
    }];

    for (index, descriptor) in alas_report::RESULT_FIGURES.iter().enumerate() {
        // A figure the Results gallery already holds is not solved again.
        let built = match state.gallery_cached_figure(descriptor.id) {
            Some(cached) => Some(cached.map(|scene| (*scene).clone())),
            None => build_result_figure(state, descriptor.id, &result.config, &theme),
        };
        if let Some(Some(scene)) = built {
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

/// Test probes: every exported figure rasterised as the GUI draws it, one PNG
/// per SVG of the archive (same stem), for a visual review without a screen.
#[cfg(test)]
pub(crate) fn write_export_pngs(state: &AppState, dir: &Path) -> Result<usize, String> {
    let figures = collect_export_figures(state).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for figure in &figures {
        let scene = embed_globe_textures(&figure.scene);
        let png = alas_viz::raster::render_scene_png(&scene)?;
        let name = figure.file_name.replace(".svg", ".png");
        std::fs::write(dir.join(&name), png).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(figures.len())
}

fn export_readiness(has_result: bool, complete: bool) -> Result<(), ExportError> {
    if !has_result {
        Err(ExportError::NoPipelineRun)
    } else if !complete {
        Err(ExportError::IncompletePipelineRun)
    } else {
        Ok(())
    }
}

fn figure_file_name(registry_position: usize, id: &str) -> String {
    format!(
        "{registry_position:02}_{}.svg",
        alas_report::export_file_stem(id)
    )
}

const SUMMARY_WIDTH: f64 = 760.0;
const SUMMARY_MARGIN: f64 = 36.0;
const SUMMARY_FONT: f64 = 11.5;
const SUMMARY_LINE_HEIGHT: f64 = 19.0;
/// Characters per line at [`SUMMARY_FONT`] (points): the usable width over a conservative average
/// glyph width of 0.58 em, with the font converted to pixels.
const SUMMARY_WRAP_CHARS: usize = ((SUMMARY_WIDTH - 2.0 * SUMMARY_MARGIN)
    / (0.58 * SUMMARY_FONT * alas_report::scene::CSS_PIXELS_PER_POINT))
    as usize;

/// `line` broken at spaces into pieces of at most `limit` characters; continuation pieces are
/// indented two spaces. A single word longer than `limit` is kept whole.
fn wrap_summary_line(line: &str, limit: usize) -> Vec<String> {
    if line.chars().count() <= limit {
        return vec![line.to_owned()];
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in line.split(' ') {
        let needed = current.chars().count() + 1 + word.chars().count();
        if !current.is_empty() && needed > limit {
            pieces.push(std::mem::take(&mut current));
            current.push_str("  ");
        }
        if !current.is_empty() && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }
    pieces
}

/// The design-summary page in the figure theme `theme`, long lines wrapped to the page width.
/// `summary` with each finding line ("- WARNING: ...", "- ERROR: ...") kept
/// once: the design summary and the delivery verdict both list the run's
/// findings, so the exported page repeated three of them word for word
/// (A320-200, measured).
fn without_repeated_findings(summary: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    summary
        .lines()
        .filter(|line| {
            let finding = line.trim_start();
            !(finding.starts_with("- ") && !seen.insert(finding.to_owned()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn summary_scene(summary: String, theme: &str) -> Scene {
    let palette = alas_report::theme::get_palette(Some(theme));
    let lines: Vec<(String, bool)> = summary
        .lines()
        .enumerate()
        .flat_map(|(index, line)| {
            // The first line is a title banner of "=" rules around the name: keep the name only.
            let text = if index == 0 {
                line.trim_matches(|c: char| c == '=' || c == ' ').to_owned()
            } else {
                line.to_owned()
            };
            wrap_summary_line(&text, SUMMARY_WRAP_CHARS)
                .into_iter()
                .map(move |piece| (piece, index == 0))
        })
        .collect();
    let title_gap = SUMMARY_LINE_HEIGHT * 0.6;
    let height = 2.0 * SUMMARY_MARGIN + title_gap + lines.len() as f64 * SUMMARY_LINE_HEIGHT;
    let mut scene = Scene::new(SUMMARY_WIDTH, height, Some(Color::from_hex(palette.bg)));
    scene.title = Some("ALAS Design Summary".to_owned());
    scene.suppress_derived_title();
    for (row, (text, is_title)) in lines.into_iter().enumerate() {
        let y = SUMMARY_MARGIN
            + row as f64 * SUMMARY_LINE_HEIGHT
            + if row > 0 { title_gap } else { 0.0 };
        // Middle of the line, not its top: the rasteriser placed the
        // super- and subscripts of a top-aligned line (m², i_h, m/s²) a line
        // low (measured on the A320-200 summary PNG); centred lines draw them
        // in place, as the axis labels do.
        scene.add(SceneElement::Text {
            text,
            pos: [SUMMARY_MARGIN, y + 0.5 * SUMMARY_LINE_HEIGHT],
            font_size: if is_title { 17.0 } else { SUMMARY_FONT },
            color: Color::from_hex(if is_title {
                palette.title
            } else {
                palette.tick
            }),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: is_title,
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
            alas_report::render_svg(&embed_globe_textures(&figure.scene)).into_bytes(),
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
    let mut file = std::io::BufWriter::new(
        File::create(path).map_err(|source| io_error("create figure archive", path, source))?,
    );
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
    write_u16(&mut file, 0, path)?;
    file.flush()
        .map_err(|source| io_error("write figure archive", path, source))
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

fn write_all(
    file: &mut std::io::BufWriter<File>,
    contents: &[u8],
    path: &Path,
) -> Result<(), ExportError> {
    file.write_all(contents)
        .map_err(|source| io_error("write figure archive", path, source))
}

fn write_u16(
    file: &mut std::io::BufWriter<File>,
    value: u16,
    path: &Path,
) -> Result<(), ExportError> {
    write_all(file, &value.to_le_bytes(), path)
}

fn write_u32(
    file: &mut std::io::BufWriter<File>,
    value: u32,
    path: &Path,
) -> Result<(), ExportError> {
    write_all(file, &value.to_le_bytes(), path)
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0_u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc = CRC_TABLE[usize::from((crc as u8) ^ *byte)] ^ (crc >> 8);
    }
    !crc
}

mod plot_data;
pub use plot_data::{
    collect_plot_data, default_plot_export_dir, export_all_plot_data, export_plot_data, line_plot,
    run_label, write_plot_bundle, PLOT_DATA_ARCHIVE_NAME, PLOT_DATA_DIR_NAME,
};

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
