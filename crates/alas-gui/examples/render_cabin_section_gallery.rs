// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// An example is a console program: reporting what it wrote is the whole point,
// and there is no tracing subscriber installed to carry it instead.
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![doc = "Render a dimensioned cabin cross-section for every registered aircraft preset."]

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;
use alas_pipeline::{CabinScene, CabinSceneInputs};
use alas_report::families::geometry::figure_cabin_section;
use alas_report::render_svg;

/// What every generated file records about where its geometry came from.
const SOURCE: &str = "preset design vector through AircraftBuilder and PayloadLayout";

/// One rendered aircraft, for the gallery index.
struct Rendered {
    /// Registry name.
    preset: String,
    /// Display name.
    display: String,
    /// File stem shared by the scene and both themes.
    stem: String,
    /// Station statement and findings, lifted from the figure's own notes.
    notes: Vec<String>,
}

/// Turn a preset name into a file stem.
fn stem_of(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Read back the sentences the figure prints, so the index says the same thing
/// the drawing does rather than a second, hand-written summary of it.
fn notes_of(scene: &alas_report::Scene) -> Vec<String> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::SceneElement::Text { text, pos, .. } if pos[1] > 660.0 => {
                Some(text.clone())
            }
            _ => None,
        })
        .filter(|text| text.chars().count() > 40)
        .collect()
}

/// Escape the few characters that would otherwise break the index markup.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Resolve one preset and write its scene and both themed sections.
fn render(directory: &Path, name: &str) -> Result<Rendered, Box<dyn Error>> {
    let preset = presets::get(name)?;
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|error| std::io::Error::other(format!("{name} geometry: {error:?}")))?;
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0)?;
    let cabin = CabinScene::from_parts(
        &config,
        CabinSceneInputs {
            design: preset.design_vector,
            airplane: &airplane,
            layout: &layout,
            source: SOURCE,
        },
    )
    .map_err(std::io::Error::other)?;
    let stem = stem_of(name);
    std::fs::write(
        directory.join(format!("{stem}_cabin_scene_v2.json")),
        serde_json::to_string_pretty(&cabin)?,
    )?;
    let light = figure_cabin_section(&cabin, Some("light"));
    let dark = figure_cabin_section(&cabin, Some("dark"));
    for (scene, suffix) in [(&light, "light"), (&dark, "dark")] {
        std::fs::write(
            directory.join(format!("{stem}_section_{suffix}.svg")),
            render_svg(scene),
        )?;
        std::fs::write(
            directory.join(format!("{stem}_section_{suffix}.png")),
            alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?,
        )?;
    }
    Ok(Rendered {
        preset: name.to_owned(),
        display: preset.display_name.to_owned(),
        stem,
        notes: notes_of(&light),
    })
}

/// Write the contact sheet that presents every rendered section together.
fn write_index(directory: &Path, rendered: &[Rendered]) -> Result<(), Box<dyn Error>> {
    let mut cards = String::new();
    for entry in rendered {
        let notes = entry
            .notes
            .iter()
            .map(|note| format!("<li>{}</li>", escape(note)))
            .collect::<Vec<_>>()
            .join("");
        let _ = write!(
            cards,
            "<section class=\"card\"><h2>{} <span>{}</span></h2>\
             <img data-stem=\"{}\" src=\"{}_section_light.svg\" alt=\"{} cabin cross-section\">\
             <ul>{notes}</ul></section>",
            escape(&entry.display),
            escape(&entry.preset),
            entry.stem,
            entry.stem,
            escape(&entry.display),
        );
    }
    let page = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>ALAS cabin cross-sections</title><style>\
:root{{color-scheme:light dark}}\
body{{margin:0;padding:28px;font:14px/1.5 system-ui,sans-serif;background:#f2f3f5;color:#101418}}\
header{{max-width:1180px;margin:0 auto 22px}}h1{{margin:0 0 6px;font-size:22px}}\
p.lead{{margin:0;color:#4a545e;max-width:70ch}}\
button{{margin-top:14px;padding:7px 14px;border:1px solid #b9c1c8;border-radius:6px;background:#fff;cursor:pointer;font:inherit}}\
main{{max-width:1180px;margin:0 auto;display:grid;gap:22px}}\
.card{{background:#fff;border:1px solid #dfe2e8;border-radius:10px;padding:16px}}\
.card h2{{margin:0 0 10px;font-size:16px}}.card h2 span{{color:#7b858f;font-weight:400;font-size:13px}}\
.card img{{width:100%;height:auto;display:block}}\
.card ul{{margin:10px 0 0;padding-left:18px;color:#4a545e;font-size:12px}}\
body.dark{{background:#15191d;color:#f2f4f6}}body.dark .card{{background:#1d2228;border-color:#2f363d}}\
body.dark p.lead,body.dark .card ul{{color:#aab3bc}}body.dark button{{background:#1d2228;color:inherit;border-color:#3a424a}}\
</style></head><body><header><h1>ALAS cabin cross-sections</h1>\
<p class=\"lead\">One transverse plane per aircraft, cut through the resolved <code>alas.cabin-scene/v2</code> geometry. \
The station is chosen for coverage, nothing is resized to fit, and every figure states what it could not verify.</p>\
<button id=\"theme\">Switch to dark</button></header><main>{cards}</main>\
<script>const b=document.body,t=document.getElementById('theme');\
t.addEventListener('click',()=>{{const d=b.classList.toggle('dark');\
t.textContent=d?'Switch to light':'Switch to dark';\
document.querySelectorAll('img[data-stem]').forEach(i=>{{i.src=i.dataset.stem+'_section_'+(d?'dark':'light')+'.svg';}});}});\
</script></body></html>"
    );
    std::fs::write(directory.join("index.html"), page)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/cabin_sections"));
    std::fs::create_dir_all(&directory)?;
    let mut rendered = Vec::new();
    for name in presets::available() {
        match render(&directory, name) {
            Ok(entry) => {
                println!("{name}: {}_section_light.svg", entry.stem);
                for note in &entry.notes {
                    println!("    {note}");
                }
                rendered.push(entry);
            }
            Err(error) => eprintln!("{name}: {error}"),
        }
    }
    write_index(&directory, &rendered)?;
    println!(
        "\n{} of {} preset(s) rendered into {}",
        rendered.len(),
        presets::available().len(),
        directory.display()
    );
    Ok(())
}
