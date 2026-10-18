// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

//! Physical verification of the generic BEMT blade against wind-tunnel
//! measurements of APC thin-electric propellers (UIUC Propeller Data Site,
//! Selig and co-workers, volumes 1 and 4: <https://m-selig.ae.illinois.edu/props/propDB.html>).
//!
//! The measured files are NOT part of the repository and are never
//! redistributed.  Point `ALAS_UIUC_PROPS` at a folder holding the files you
//! downloaded yourself (the names are the site's: `apce_12x6_static_0629od.txt`,
//! `apce_12x6_0635od_6044.txt`, ...); without it the test reports a skip.
//! The site has no robots.txt and asks for citation; fetch politely (one file
//! every 5 s).
//!
//! Conventions: `J = V / (n D)`, `CT = T / (rho n^2 D^4)`, `CP = P / (rho n^3
//! D^5)`; the tunnel density is taken as 1.2 kg/m3, mu = 1.81e-5 Pa*s (room
//! conditions; the site publishes only the nominal rpm).  Errors are
//! normalised by the first (lowest J) measured value of the same file, CT0
//! and CP0, so that points near zero thrust do not blow up a relative error.
//!
//! Status (measured on 8 props, 2026-10-12; see the lane report): the
//! generic blade has NO systematic forward-speed over-efficiency on this
//! class.  Bias of CT is -6 % and of efficiency -6 % for J/J0 < 0.6, close to
//! zero for 0.6-0.85; the zero-thrust advance ratio J0 is predicted 2-8 %
//! low for low-pitch blades, so thrust at J/J0 > 0.85 is UNDER-predicted.

use alas_uav::power::BemtGeometry;
use std::path::{Path, PathBuf};

const RHO: f64 = 1.2;
const MU: f64 = 1.81e-5;

struct Prop {
    tag: &'static str,
    d_in: f64,
    p_in: f64,
    /// Forward-flight files (site names) with their nominal rpm, and the static file.
    forward: &'static [(&'static str, f64)],
    static_file: &'static str,
}

const PROPS: [Prop; 4] = [
    Prop {
        tag: "APC 12x6E (vol 4)",
        d_in: 12.0,
        p_in: 6.0,
        forward: &[
            ("apce_12x6_0632od_5007.txt", 5007.0),
            ("apce_12x6_0635od_6044.txt", 6044.0),
        ],
        static_file: "apce_12x6_static_0629od.txt",
    },
    Prop {
        tag: "APC 12x8E (vol 4)",
        d_in: 12.0,
        p_in: 8.0,
        forward: &[
            ("apce_12x8_0625od_4991.txt", 4991.0),
            ("apce_12x8_0628od_6024.txt", 6024.0),
        ],
        static_file: "apce_12x8_static_0621od.txt",
    },
    Prop {
        tag: "APC 11x8.5E (vol 1)",
        d_in: 11.0,
        p_in: 8.5,
        forward: &[
            ("apce_11x8.5_pg0455_5012.txt", 5012.0),
            ("apce_11x8.5_pg0459_5999.txt", 5999.0),
        ],
        static_file: "apce_11x8.5_static_jb0452.txt",
    },
    Prop {
        tag: "APC 14x7E (vol 4)",
        d_in: 14.0,
        p_in: 7.0,
        forward: &[
            ("apce_14x7_1010od_5015.txt", 5015.0),
            ("apce_14x7_1013od_6038.txt", 6038.0),
        ],
        static_file: "apce_14x7_static_1006od.txt",
    },
];

fn rows(path: &Path) -> Option<Vec<Vec<f64>>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(
        text.lines()
            .skip(1)
            .map(|l| {
                l.split_whitespace()
                    .filter_map(|x| x.parse::<f64>().ok())
                    .collect::<Vec<_>>()
            })
            .filter(|v| v.len() >= 3)
            .collect(),
    )
}

fn coefficients(g: &BemtGeometry, d: f64, rpm: f64, j: f64) -> (f64, f64) {
    let n = rpm / 60.0;
    let s = g
        .solve(n * std::f64::consts::TAU, j * n * d, RHO, MU)
        .unwrap();
    (
        s.thrust_n / (RHO * n * n * d.powi(4)),
        s.torque_n_m * std::f64::consts::TAU / (RHO * n * n * d.powi(5)),
    )
}

#[derive(Default, Debug)]
struct Stats {
    rms_ct: f64,
    rms_cp: f64,
    static_ct: f64,
    static_cp: f64,
    j0_measured: Option<f64>,
    j0_predicted: f64,
}

fn evaluate(dir: &Path, p: &Prop) -> Option<Stats> {
    let d = p.d_in * 0.0254;
    let g = BemtGeometry::generic(d, p.p_in * 0.0254, 2, 0.15).unwrap();
    let mut s = Stats::default();
    // Static: files at rpm >= 4000 (below that the tunnel Re is far lower).
    let st = rows(&dir.join(p.static_file))?;
    let sel: Vec<_> = st.iter().filter(|v| v[0] >= 4000.0).collect();
    for v in &sel {
        let (ct, cp) = coefficients(&g, d, v[0], 0.0);
        s.static_ct += (ct / v[1] - 1.0) / sel.len() as f64;
        s.static_cp += (cp / v[2] - 1.0) / sel.len() as f64;
    }
    let (mut sq_t, mut sq_p, mut count) = (0.0, 0.0, 0.0);
    for (name, rpm) in p.forward {
        let data = rows(&dir.join(name))?;
        let mut pts: Vec<(f64, f64, f64)> = Vec::new();
        for v in &data {
            let fresh = pts.last().is_none_or(|l| (l.0 - v[0]).abs() > 1.0e-9);
            if v.len() >= 4 && v[1] > -0.003 && fresh {
                pts.push((v[0], v[1], v[2]));
            }
        }
        let (ct0, cp0) = (pts[0].1, pts[0].2);
        for &(j, ct, cp) in &pts {
            let (ct_p, cp_p) = coefficients(&g, d, *rpm, j);
            sq_t += ((ct_p - ct) / ct0).powi(2);
            sq_p += ((cp_p - cp) / cp0).powi(2);
            count += 1.0;
        }
        // Measured J0 only when the file crosses zero thrust (highest rpm).
        for w in pts.windows(2) {
            if w[0].1 > 0.0 && w[1].1 <= 0.0 {
                s.j0_measured = Some(w[0].0 + (w[1].0 - w[0].0) * w[0].1 / (w[0].1 - w[1].1));
            }
        }
        let mut j = 0.2;
        while j < 1.3 && coefficients(&g, d, *rpm, j).0 > 0.0 {
            j += 0.005;
        }
        s.j0_predicted = j;
    }
    s.rms_ct = (sq_t / count).sqrt();
    s.rms_cp = (sq_p / count).sqrt();
    Some(s)
}

#[test]
fn generic_blade_against_uiuc_apc_thin_electric_measurements() {
    let Some(dir) = std::env::var_os("ALAS_UIUC_PROPS").map(PathBuf::from) else {
        println!("SKIP: set ALAS_UIUC_PROPS to a folder of UIUC propeller files");
        return;
    };
    let mut checked = 0;
    for p in &PROPS {
        let Some(s) = evaluate(&dir, p) else {
            println!("SKIP {}: files not found in {}", p.tag, dir.display());
            continue;
        };
        println!(
            "UIUC|{}|rms CT/CT0 {:.1} %|rms CP/CP0 {:.1} %|static dCT {:+.1} % dCP {:+.1} %|J0 predicted {:.3} measured {:?}",
            p.tag,
            100.0 * s.rms_ct,
            100.0 * s.rms_cp,
            100.0 * s.static_ct,
            100.0 * s.static_cp,
            s.j0_predicted,
            s.j0_measured
        );
        // Bands = 1.5 x the errors measured on 2026-10-12 (no tuning to them).
        assert!(s.rms_ct < 0.18, "{}: CT rms {}", p.tag, s.rms_ct);
        assert!(s.rms_cp < 0.18, "{}: CP rms {}", p.tag, s.rms_cp);
        assert!(s.static_ct.abs() < 0.12, "{}: static CT", p.tag);
        assert!(s.static_cp.abs() < 0.22, "{}: static CP", p.tag);
        if let Some(j0) = s.j0_measured {
            assert!(
                (s.j0_predicted / j0 - 1.0).abs() < 0.10,
                "{}: J0 {} vs {}",
                p.tag,
                s.j0_predicted,
                j0
            );
        }
        checked += 1;
    }
    assert!(checked >= 1, "no UIUC file set was readable");
}
