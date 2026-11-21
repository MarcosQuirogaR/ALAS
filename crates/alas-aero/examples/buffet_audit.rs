// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strip-by-strip audit of the buffet-onset chain.
//!
//! `buffet_audit <preset>[@mach,mach,...]...` prints, for each preset at its
//! cruise altitude and at each Mach number (default: the cruise Mach), every
//! strip of the search window with its loading, normal conditions, table
//! onset and onset wing lift, and the wing's onset.

#![allow(clippy::print_stderr, clippy::print_stdout, clippy::unwrap_used)]

use alas_aero::analysis::{shipped_tables, strip_onset_cl_n, AeroAnalysis, SweepLine};
use alas_config::AlasConfig;

fn main() {
    for arg in std::env::args().skip(1) {
        let (name, machs) = arg.split_once('@').map_or((arg.as_str(), None), |(n, m)| {
            (
                n,
                Some(
                    m.split(',')
                        .map(|v| v.parse::<f64>().unwrap())
                        .collect::<Vec<_>>(),
                ),
            )
        });
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap();
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, design.sweep_deg),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        let req = &config.requirements;
        let h = req.cruise_altitude_m;
        let wing = &config.geometry.wing;
        let kink = wing.kink_span_fraction.unwrap_or(wing.break_span_fraction);
        for m in machs.unwrap_or_else(|| vec![req.cruise_mach]) {
            let atm = alas_atmo::Atmosphere::new(h);
            let q = 0.5 * atm.density() * (m * atm.speed_of_sound()).powi(2);
            let cl_cr = req.mtow_kg * req.gravity_m_s2 / (q * plane.s_ref);
            let onset = aero.buffet_onset(m, h);
            println!(
                "== {name} M={m} h={h} CLcr(MTOW)={cl_cr:.3} S={:.1} kink={kink:.3} CLb={:.3} n_mtow={:.3} flags={}",
                plane.s_ref,
                onset.cl_b().unwrap_or(f64::NAN),
                onset.cl_b().unwrap_or(f64::NAN) / cl_cr,
                match &onset {
                    alas_aero::analysis::BuffetOnset::Interpolated(_) => "interp".to_owned(),
                    alas_aero::analysis::BuffetOnset::Extrapolated(_, a) => format!("{a:?}"),
                    alas_aero::analysis::BuffetOnset::Unavailable(u) => format!("{u:?}"),
                }
            );
            let Ok(strips) = aero.buffet_strip_candidates(m, h, SweepLine::Shock, (1, 8)) else {
                println!("  strips failed");
                continue;
            };
            for s in strips.iter().filter(|s| s.eta > 0.0) {
                let on = strip_onset_cl_n(shipped_tables(), s);
                let fam: Vec<String> = s
                    .families
                    .iter()
                    .map(|(f, w)| format!("{f}:{w:.2}"))
                    .collect();
                match on {
                    Ok((cl_n, flags)) => println!(
                        "    eta={:.3} basic={:+.3} add={:.3} k05={:.3} sweep={:.1} Mn={:.3} tc_n={:.4} cam_n={:.4} cl_n_on={cl_n:.3} CLb={:.3} oldCLb={:.3} {flags:?} {}",
                        s.eta, s.basic_cl, s.additional_cl, s.local_cl(0.5) / 0.5, s.sweep_deg,
                        s.mach_normal, s.thickness_normal, s.camber_normal, s.wing_cl_at(cl_n),
                        cl_n * s.sweep_deg.to_radians().cos().powi(2) / (s.local_cl(0.5) / 0.5),
                        fam.join("+")
                    ),
                    Err(e) => println!(
                        "    eta={:.3} basic={:+.3} add={:.3} sweep={:.1} Mn={:.3} tc_n={:.4} cam_n={:.4} {e:?} {}",
                        s.eta, s.basic_cl, s.additional_cl, s.sweep_deg, s.mach_normal,
                        s.thickness_normal, s.camber_normal, fam.join("+")
                    ),
                }
            }
        }
    }
}
