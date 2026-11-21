// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed-AVL neutral point with and without slender bodies.
//!
//! `ALAS_AVL_EXE=... cargo run --release -p alas-pipeline --example sm1_avl_np_probe <mach> <preset> [...]`
//! writes the built lifting surfaces as an AVL deck, optionally appends the
//! fuselage (and the podded nacelles) as AVL `BODY` slender-body elements of
//! the area-equivalent radius, runs AVL and prints `CLa`, `Cma` and the
//! neutral point `Xnp` (percent MAC from the leading edge of the MAC) for
//! three layouts: lifting surfaces, + fuselage, + fuselage and nacelles.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use alas_aero::avl::{render_geometry, AvlDeckRequest};
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::fuselage::Fuselage;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let mach: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(0.2);
    let exe = PathBuf::from(std::env::var("ALAS_AVL_EXE").expect("ALAS_AVL_EXE"));
    for name in args {
        if let Err(error) = run(&name, mach, &exe) {
            println!("{name}: ERROR {error}");
        }
    }
}

/// An AVL `BODY` section and its profile file for `fuselage`: the area-equivalent
/// radius as a closed airfoil-style outline in units of the body length.
fn body(dir: &Path, tag: &str, fuselage: &Fuselage) -> (String, String) {
    body_range(dir, tag, fuselage, f64::NEG_INFINITY, f64::INFINITY)
}

/// [`body`] clipped to `x_from..x_to` (sections interpolated at the cuts).
fn body_range(
    dir: &Path,
    tag: &str,
    fuselage: &Fuselage,
    x_from: f64,
    x_to: f64,
) -> (String, String) {
    let mut sorted = fuselage.xsecs.clone();
    sorted.sort_by(|a, b| a.xyz_c[0].total_cmp(&b.xyz_c[0]));
    let at = |x: f64| {
        let mut s = sorted[0];
        for pair in sorted.windows(2) {
            if x >= pair[0].xyz_c[0] && x <= pair[1].xyz_c[0] {
                let t = (x - pair[0].xyz_c[0]) / (pair[1].xyz_c[0] - pair[0].xyz_c[0]);
                s = pair[0];
                s.xyz_c[0] = x;
                s.width = pair[0].width + t * (pair[1].width - pair[0].width);
                s.height = pair[0].height + t * (pair[1].height - pair[0].height);
                break;
            }
        }
        s
    };
    let (lo, hi) = (
        x_from.max(sorted[0].xyz_c[0]),
        x_to.min(sorted[sorted.len() - 1].xyz_c[0]),
    );
    let mut xsecs = vec![at(lo)];
    xsecs.extend(
        sorted
            .iter()
            .filter(|s| s.xyz_c[0] > lo && s.xyz_c[0] < hi)
            .cloned(),
    );
    xsecs.push(at(hi));
    let (x0, x1) = (xsecs[0].xyz_c[0], xsecs[xsecs.len() - 1].xyz_c[0]);
    let length = x1 - x0;
    let radius = |i: usize| 0.5 * (xsecs[i].width * xsecs[i].height).sqrt();
    let mut profile = format!("{tag}\n");
    // Airfoil-style: tail along the top to the nose, back along the bottom.
    for i in (0..xsecs.len()).rev() {
        writeln!(
            profile,
            "{:.8} {:.8}",
            (xsecs[i].xyz_c[0] - x0) / length,
            radius(i) / length
        )
        .ok();
    }
    for (i, section) in xsecs.iter().enumerate().skip(1) {
        writeln!(
            profile,
            "{:.8} {:.8}",
            (section.xyz_c[0] - x0) / length,
            -radius(i) / length
        )
        .ok();
    }
    let file = format!("{tag}.dat");
    std::fs::write(dir.join(&file), profile).unwrap();
    let section = format!(
        "\nBODY\n{tag}\n24 1.0\nSCALE\n{length:.6} {length:.6} {length:.6}\nTRANSLATE\n{x0:.6} {:.6} {:.6}\nBFIL\n{file}\n",
        xsecs[0].xyz_c[1], xsecs[0].xyz_c[2]
    );
    (section, file)
}

fn run(name: &str, mach: f64, exe: &Path) -> Result<(), String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let plane = &report.airplane;
    let frame = plane.mac_frame().ok_or("no MAC frame")?;
    let dir = std::env::temp_dir().join(format!("sm1-avl-{name}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let base = render_geometry(AvlDeckRequest {
        airplane: plane,
        mach,
        chordwise_vortices: 12,
        spanwise_vortices: 40,
    })
    .map_err(|e| e.to_string())?;
    let fuselage_section = body(&dir, "fuselage", &plane.fuselages[0]).0;
    let nacelle_sections: String = plane
        .fuselages
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, nacelle)| body(&dir, &format!("nacelle{i}"), nacelle).0)
        .collect();
    let wing = plane.wings.iter().find(|w| w.name == "Main Wing").unwrap();
    let (x_le_root, x_te_root) = (
        wing.xsecs[0].xyz_le[0],
        wing.xsecs[0].xyz_le[0] + wing.xsecs[0].chord,
    );
    let part =
        |tag: &str, from: f64, to: f64| body_range(&dir, tag, &plane.fuselages[0], from, to).0;
    // Body alone: a negligible wing keeps AVL's surface requirement; the
    // moment about the CG is the body's (Munk) moment, compared by the caller
    // with `2 (k2 - k1) V / (S c)` (k2 - k1 about 0.9-0.95 at fineness 10).
    let tiny = {
        let mut p = plane.clone();
        p.wings.truncate(1);
        for w in &mut p.wings {
            for x in &mut w.xsecs {
                x.chord *= 1.0e-3;
                x.xyz_le[1] *= 1.0e-3;
            }
        }
        render_geometry(AvlDeckRequest {
            airplane: &p,
            mach,
            chordwise_vortices: 4,
            spanwise_vortices: 4,
        })
        .unwrap()
    };
    let layouts = [
        ("bodyonly", format!("{tiny}{fuselage_section}")),
        ("surfaces", base.clone()),
        (
            "+fore",
            format!("{base}{}", part("fore", f64::NEG_INFINITY, x_le_root)),
        ),
        (
            "+aft",
            format!("{base}{}", part("aft", x_te_root, f64::INFINITY)),
        ),
        (
            "+mid",
            format!("{base}{}", part("mid", x_le_root, x_te_root)),
        ),
        ("+fuselage", format!("{base}{fuselage_section}")),
        (
            "+fuselage+nacelles",
            format!("{base}{fuselage_section}{nacelle_sections}"),
        ),
    ];
    for (label, deck) in layouts {
        let tag = label.replace('+', "_");
        let deck_path = dir.join(format!("{tag}.avl"));
        std::fs::write(&deck_path, deck).unwrap();
        let st = dir.join(format!("{tag}.st"));
        let _ = std::fs::remove_file(&st);
        let mut child = Command::new(exe)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let script = format!(
            "LOAD {}.avl\nOPER\nA A 2.0\nX\nST\n{}.st\n\nQUIT\n",
            tag, tag
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = std::fs::read_to_string(&st).unwrap_or_default();
        let value = |key: &str| -> Option<f64> {
            let at = text.find(key)?;
            let rest = &text[at + key.len()..];
            let rest = rest.trim_start_matches([' ', '=']);
            rest.split_whitespace().next()?.parse().ok()
        };
        match (value("CLa ="), value("Cma ="), value("Xnp =")) {
            (Some(cla), Some(cma), Some(xnp)) => println!(
                "{name} M{mach} {label:20} CLa {cla:.3} Cma {cma:.3} Xnp {:.1} % MAC (CG {:.1} %)",
                100.0 * (xnp - frame.x_lemac_m) / frame.chord_m,
                100.0 * (plane.xyz_ref[0] - frame.x_lemac_m) / frame.chord_m,
            ),
            _ => println!(
                "{name} M{mach} {label}: no result; stdout tail: {}",
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .rev()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        }
    }
    Ok(())
}
