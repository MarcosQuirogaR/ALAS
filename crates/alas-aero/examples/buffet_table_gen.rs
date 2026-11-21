// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Offline generator of the buffet-onset tables and the preset station lister.
//!
//! `buffet_table_gen stations` prints, per registered preset, the critical
//! strip (family, sweep, normal Mach, normal thickness and camber) the runtime
//! reads at the declared cruise point.
//!
//! `buffet_table_gen generate <out> <mses_dir> <threads> <family>...` appends
//! one `cell` record per finished (Mach, thickness, camber) cell to `<out>`;
//! a cell already in the file is skipped, so the run can be resumed. For each
//! cell MSES is stepped in incidence (1.0 deg) on the family section at the
//! cell's thickness and camber, and the onset `cl_n` is where the pre-shock
//! Mach number (nose peak x/c < 0.15 excluded; shock = steepest Mach drop of
//! at least 0.15 at 0.25 <= x/c <= 0.9) reaches 1.38 (Obert 2009 ch. 17
//! pp. 109-110), linearly interpolated between the last two incidences.

#![allow(clippy::print_stderr, clippy::print_stdout, clippy::unwrap_used)]

use alas_aero::analysis::{table_cell_section, AeroAnalysis, BuffetOnset};
use alas_config::AlasConfig;
use alas_geom::airfoil_library::AirfoilLibrary;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const MACH_AXIS: &[f64] = &[0.66, 0.71, 0.76];
const TC_AXIS: &[f64] = &[0.09, 0.12, 0.155];
const CAMBER_AXIS: &[f64] = &[0.007, 0.016, 0.029];

/// Axes (Mach, thickness, camber) of a family. The near-symmetric 64A-series
/// shape has a library camber of about 0.006, so its camber axis stays near that
/// (a 0.029 camber would be a 5x rescale of its mean line, not the same family).
fn axes(family: &str) -> (&'static [f64], &'static [f64], &'static [f64]) {
    if family == "naca64a109" {
        (MACH_AXIS, &[0.08, 0.10, 0.12], &[0.004, 0.007, 0.011])
    } else {
        (MACH_AXIS, TC_AXIS, CAMBER_AXIS)
    }
}
const REYNOLDS_N: f64 = 2.0e7;
const M_ONSET: f64 = 1.38;

/// Reynolds number of a cell run; `REYNOLDS_N_OVERRIDE` is for sensitivity
/// checks only (the shipped tables are generated at [`REYNOLDS_N`]).
fn reynolds_run() -> f64 {
    std::env::var("REYNOLDS_N_OVERRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(REYNOLDS_N)
}
const MAX_ALPHA_DEG: f64 = 9.0;

/// Largest incidence of a continuation sweep; `MAX_ALPHA_DEG_OVERRIDE` shortens it.
fn max_alpha_deg() -> f64 {
    std::env::var("MAX_ALPHA_DEG_OVERRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MAX_ALPHA_DEG)
}

/// Solver-setting variant of a retry rung, from the environment:
/// `MSET_N`, `XTR` (forced transition x/c on both surfaces), `NCRIT`, `MUCON`.
fn apply_variant(mses: &mut alas_config::MsesConfig) {
    let get = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f64>().ok());
    if let Some(v) = get("MSET_N") {
        mses.mset_n = v as i64;
    }
    if let Some(v) = get("XTR") {
        mses.xtr_upper = v;
        mses.xtr_lower = v;
    }
    if let Some(v) = get("NCRIT") {
        mses.n_crit = v;
    }
    if let Some(v) = get("MUCON") {
        mses.mucon = v;
    }
}

thread_local! {
    /// Set when the last cell onset was extrapolated from converged points.
    static EXTRAPOLATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Set when the last cell had no onset because the solver tool failed (a
    /// timeout, launch or parse failure, missing installation) rather than
    /// because the flow did not converge: such a cell says nothing about the
    /// section and must be re-run, never tabulated as `nan`.
    static TOOL_FAILURE: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
    /// Set when the last cell's sweep was partial (stopped early by a tool
    /// failure, or converged angles lost in read-back): where and why.
    static PARTIAL: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The [`TOOL_FAILURE`] status of a sweep with no converged angle, `None`
/// when the solver ran and simply did not converge.
fn tool_failure(status: alas_aero::mses::MsesStatus) -> Option<&'static str> {
    use alas_aero::mses::MsesStatus;
    match status {
        MsesStatus::Timeout
        | MsesStatus::LaunchFailure
        | MsesStatus::ParseFailure
        | MsesStatus::Incomplete
        | MsesStatus::Absent
        | MsesStatus::Disabled
        | MsesStatus::NotRun => Some(status.as_str()),
        MsesStatus::Ok
        | MsesStatus::PartialConvergence
        | MsesStatus::OutsideValidityDomain
        | MsesStatus::Error => None,
    }
}

/// Cell statuses `assemble` refuses: a tool failure is not a flow result.
const TOOL_FAILURE_STATUSES: [&str; 9] = [
    "readback_failure",
    "error",
    "timeout",
    "launch_failure",
    "parse_failure",
    "incomplete",
    "absent",
    "disabled",
    "not_run",
];

fn trapz(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| 0.5 * (y[0] + y[1]) * (x[1] - x[0]))
        .sum()
}

/// Linear interpolation of `y(x)` at `at`, clamped to the ends.
fn interp(x: &[f64], y: &[f64], at: f64) -> f64 {
    if x.is_empty() {
        return f64::NAN;
    }
    if at <= x[0] {
        return y[0];
    }
    for i in 0..x.len() - 1 {
        if at <= x[i + 1] {
            let t = (at - x[i]) / (x[i + 1] - x[i]);
            return y[i] + t * (y[i + 1] - y[i]);
        }
    }
    y[x.len() - 1]
}

/// `integral cp (x - xref) dx` on one surface.
fn moment_integral(x: &[f64], cp: &[f64], xref: f64) -> f64 {
    x.windows(2)
        .zip(cp.windows(2))
        .map(|(x, c)| 0.5 * (c[0] * (x[0] - xref) + c[1] * (x[1] - xref)) * (x[1] - x[0]))
        .sum()
}

/// Maximum Mach ahead of the upper-surface shock and the shock position.
fn pre_shock_mach(x: &[f64], m: &[f64]) -> (f64, f64) {
    let mut shock: Option<(usize, f64)> = None;
    for i in 0..x.len().saturating_sub(2) {
        if x[i] < 0.25 || x[i] > 0.9 || m[i] < 1.0 {
            continue;
        }
        let drop = m[i] - m[i + 2];
        if drop >= 0.15 && shock.is_none_or(|s| drop > s.1) {
            shock = Some((i, drop));
        }
    }
    let end = shock.map_or(x.len() - 1, |s| s.0);
    let m_pre = (0..=end)
        .filter(|&i| x[i] >= 0.15)
        .map(|i| m[i])
        .fold(0.0, f64::max);
    (m_pre, shock.map_or(1.0, |s| x[s.0]))
}

fn stations() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(plane) = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
        else {
            println!("{name} BUILD FAILED");
            continue;
        };
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, design.sweep_deg),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        let req = &config.requirements;
        match aero.buffet_strip(req.cruise_mach, req.cruise_altitude_m) {
            Ok(s) => println!(
                "{name} M={} family={} eta={:.2} k={:.3} sweep={:.1} Mn={:.3} tc_n={:.4} cam_n={:.4}",
                req.cruise_mach, s.family, s.eta, s.local_cl(0.5) / 0.5, s.sweep_deg, s.mach_normal,
                s.thickness_normal, s.camber_normal
            ),
            Err(e) => println!("{name} M={} UNAVAILABLE {e:?}", req.cruise_mach),
        }
    }
}

/// Onset `cl_n` of one cell by alpha continuation: one polar sweep restarts
/// every angle from the previous converged state (steps of `CONT_STEP_DEG`,
/// default 0.5), each converged angle's Mach distribution is read from its own
/// checkpoint, and the onset is read from those angles only. Non-converged
/// angles are skipped, never interpolated over: the crossing is bracketed by
/// two converged angles, or extrapolated from the last two under the same
/// rule as [`cell_onset`].
fn cell_onset_continuation(
    base: &alas_geom::aircraft::airfoil::Airfoil,
    mach_n: f64,
    tc: f64,
    camber: f64,
    mses_dir: &Path,
    mses: &alas_config::MsesConfig,
) -> Option<f64> {
    let section = table_cell_section(base, tc, camber)?;
    let step: f64 = std::env::var("CONT_STEP_DEG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);
    let n = (max_alpha_deg() / step).round() as usize;
    let alphas: Vec<f64> = (0..=n).map(|i| i as f64 * step).collect();
    // Stall watchdog (one sweep per process): when no angle has converged
    // for `STALL_MIN` minutes, further angles are Mach-ramp retries of a
    // flow with no steady solution, so the sweep is stopped and keeps the
    // angles converged so far. Progress is read from the modification time of
    // the work folder's `mdat.case.last_converged`.
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let finished = std::sync::atomic::AtomicBool::new(false);
    let stall_s = 60.0
        * std::env::var("STALL_MIN")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(15.0);
    let results = std::thread::scope(|scope| {
        scope.spawn(|| {
            let prefix = format!("alas_mses_{}_", std::process::id());
            let started = std::time::SystemTime::now();
            while !finished.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(20));
                let mut latest = started;
                for entry in std::fs::read_dir(std::env::temp_dir())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    if !entry.file_name().to_string_lossy().starts_with(&prefix) {
                        continue;
                    }
                    if let Ok(t) = std::fs::metadata(entry.path().join("mdat.case.last_converged"))
                        .and_then(|m| m.modified())
                    {
                        latest = latest.max(t);
                    }
                }
                let idle = std::time::SystemTime::now()
                    .duration_since(latest)
                    .map_or(0.0, |d| d.as_secs_f64());
                if idle > stall_s {
                    eprintln!(
                        "  stall watchdog: no converged angle for {:.0} s, stopping",
                        idle
                    );
                    cancel.store(true, Ordering::SeqCst);
                    return;
                }
            }
        });
        let r = alas_aero::mses::run_mses_pressure_continuation_with_status(
            &section,
            mach_n,
            reynolds_run(),
            &alphas,
            mses,
            mses_dir,
            Some(&cancel),
        );
        finished.store(true, Ordering::SeqCst);
        r
    });
    let status = results.status;
    if std::env::var("CELL_DEBUG").is_ok() {
        eprintln!(
            "  sweep status {} {:?} stopped {:?} readback failures {}",
            status.as_str(),
            results.error,
            results.stopped,
            results.readback_failures
        );
    }
    // A sweep a tool failure stopped early, or one that lost converged angles
    // in read-back, is partial: its onset counts only when converged points
    // bracket the criterion, else the cell is a tool failure to re-run.
    let partial: Option<(&'static str, String)> =
        match (&results.stopped, results.readback_failures) {
            (Some(stop), _) => Some((
                stop.status.as_str(),
                format!("{}@{:.2}", stop.status.as_str(), stop.alpha_deg),
            )),
            (None, 0) => None,
            (None, lost) => Some(("readback_failure", format!("readback_failure x{lost}"))),
        };
    PARTIAL.with(|f| f.replace(partial.as_ref().map(|p| p.1.clone())));
    let results = results.results;
    if results.is_empty() {
        TOOL_FAILURE.with(|f| f.set(tool_failure(status).or(partial.as_ref().map(|p| p.0))));
    }
    let mut points: Vec<(f64, f64, f64)> = Vec::new(); // alpha, cl_n, pre-shock Mach
    for r in &results {
        let cl = trapz(&r.x_lower, &r.cp_lower) - trapz(&r.x_upper, &r.cp_upper);
        let (m_pre, x_shock) = pre_shock_mach(&r.x_upper, &r.mach_upper);
        if let Ok(path) = std::env::var("POLAR_OUT") {
            // Pressure-only section moment about c/4, nose up positive, and the upper-surface
            // pressure at the trailing edge: the force/moment-break and TE-divergence quantities
            // of Obert (2009) Fig. 28.9.
            let cm = moment_integral(&r.x_upper, &r.cp_upper, 0.25)
                - moment_integral(&r.x_lower, &r.cp_lower, 0.25);
            let line = format!(
                "{} {:.3} {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} {:.4} {:.4}\n",
                std::env::var("POLAR_LABEL")
                    .unwrap_or_default()
                    .replace(' ', "_"),
                r.alpha_deg,
                cl,
                cm,
                interp(&r.x_upper, &r.cp_upper, 0.90),
                interp(&r.x_upper, &r.cp_upper, 0.95),
                interp(&r.x_upper, &r.cp_upper, 0.99),
                interp(&r.x_lower, &r.cp_lower, 0.95),
                m_pre,
                x_shock
            );
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
        }
        if std::env::var("CELL_DEBUG").is_ok() {
            eprintln!("  alpha {:.2} cl_n {cl:.3} m_pre {m_pre:.3}", r.alpha_deg);
        }
        points.push((r.alpha_deg, cl, m_pre));
    }
    for i in 0..points.len() {
        if points[i].2 >= M_ONSET {
            if i > 0 && points[i].2 > points[i - 1].2 {
                let (_, cl0, m0) = points[i - 1];
                return Some(cl0 + (M_ONSET - m0) / (points[i].2 - m0) * (points[i].1 - cl0));
            }
            if let Some((status, _)) = &partial {
                TOOL_FAILURE.with(|f| f.set(Some(status)));
                return None;
            }
            return Some(points[i].1);
        }
    }
    if let Some((status, _)) = &partial {
        TOOL_FAILURE.with(|f| f.set(Some(status)));
        return None;
    }
    let k = points.len();
    if k >= 2 {
        let (_, cl0, m0) = points[k - 2];
        let (_, cl1, m1) = points[k - 1];
        if m1 >= M_ONSET - 0.18 && m1 > m0 {
            EXTRAPOLATED.with(|f| f.set(true));
            return Some(cl1 + (M_ONSET - m1) * (cl1 - cl0) / (m1 - m0));
        }
    }
    None
}

/// Onset `cl_n` of one cell, `None` when MSES fails before the onset.
fn cell_onset(
    base: &alas_geom::aircraft::airfoil::Airfoil,
    mach_n: f64,
    tc: f64,
    camber: f64,
    mses_dir: &Path,
    mses: &alas_config::MsesConfig,
) -> Option<f64> {
    if std::env::var("CONTINUATION").is_ok() {
        return cell_onset_continuation(base, mach_n, tc, camber, mses_dir, mses);
    }
    let section = table_cell_section(base, tc, camber)?;
    let mut previous: Option<(f64, f64)> = None; // cl_n, pre-shock Mach
    let mut earlier: Option<(f64, f64)> = None;
    let mut alpha = 0.0;
    while alpha <= MAX_ALPHA_DEG {
        let r = alas_aero::mses::run_mses_pressure_distribution(
            &section,
            mach_n,
            reynolds_run(),
            alpha,
            mses,
            mses_dir,
            Some(&[0.0]),
        );
        let r = if r.x_upper.is_empty() || r.x_lower.is_empty() {
            // One retry half a degree on: a single non-converged incidence
            // should not void the cell.
            alpha += 0.5;
            let again = alas_aero::mses::run_mses_pressure_distribution(
                &section,
                mach_n,
                reynolds_run(),
                alpha,
                mses,
                mses_dir,
                Some(&[0.0]),
            );
            if again.x_upper.is_empty() || again.x_lower.is_empty() {
                if std::env::var("CELL_DEBUG").is_ok() {
                    eprintln!(
                        "  alpha {alpha:.1} FAILED {:?} {:?}",
                        again.status, again.error
                    );
                }
                // MSES stops converging as the shock strengthens. When the
                // last converged point is within 0.18 of the onset Mach the
                // onset is extrapolated linearly from the last two converged
                // points (recorded as `extrap` in the cell log).
                return match (earlier, previous) {
                    (Some((cl0, m0)), Some((cl1, m1))) if m1 >= M_ONSET - 0.18 && m1 > m0 => {
                        EXTRAPOLATED.with(|f| f.set(true));
                        Some(cl1 + (M_ONSET - m1) * (cl1 - cl0) / (m1 - m0))
                    }
                    _ => None,
                };
            }
            again
        } else {
            r
        };
        let cl = trapz(&r.x_lower, &r.cp_lower) - trapz(&r.x_upper, &r.cp_upper);
        let (m_pre, _) = pre_shock_mach(&r.x_upper, &r.mach_upper);
        if std::env::var("CELL_DEBUG").is_ok() {
            eprintln!(
                "  alpha {alpha:.1} cl_n {cl:.3} m_pre {m_pre:.3} status {:?}",
                r.status
            );
        }
        if m_pre >= M_ONSET {
            return Some(match previous {
                Some((cl0, m0)) if m_pre > m0 => cl0 + (M_ONSET - m0) / (m_pre - m0) * (cl - cl0),
                _ => cl,
            });
        }
        earlier = previous;
        previous = Some((cl, m_pre));
        alpha += 1.0;
    }
    None
}

fn generate(out: &Path, mses_dir: &Path, threads: usize, families: &[String]) {
    let existing = std::fs::read_to_string(out).unwrap_or_default();
    let mut done = std::collections::HashSet::new();
    let mut jobs: Vec<(String, usize, usize, usize)> = Vec::new();
    for family in families {
        for i in 0..axes(family).0.len() {
            for j in 0..axes(family).1.len() {
                for k in 0..axes(family).2.len() {
                    jobs.push((family.clone(), i, j, k));
                }
            }
        }
    }
    for line in existing.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        // A failed (`nan`) cell is re-run when RETRY_NAN is set.
        if p.first() == Some(&"#cell") && !(p[5] == "nan" && std::env::var("RETRY_NAN").is_ok()) {
            done.insert((
                p[1].to_owned(),
                p[2].parse::<usize>().unwrap(),
                p[3].parse::<usize>().unwrap(),
                p[4].parse::<usize>().unwrap(),
            ));
        }
    }
    jobs.retain(|j| !done.contains(j));
    println!("{} cells to run", jobs.len());
    let mut mses = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .unwrap()
        .mses;
    mses.enabled = true;
    apply_variant(&mut mses);
    mses.mses_dir = mses_dir.to_string_lossy().into_owned();
    let next = AtomicUsize::new(0);
    let file = Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out)
            .unwrap(),
    );
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let n = next.fetch_add(1, Ordering::SeqCst);
                let Some((family, i, j, k)) = jobs.get(n).cloned() else {
                    return;
                };
                let base = base_section(&family);
                EXTRAPOLATED.with(|f| f.set(false));
                TOOL_FAILURE.with(|f| f.set(None));
                PARTIAL.with(|f| f.replace(None));
                let started = std::time::Instant::now();
                let value = cell_onset(
                    &base,
                    axes(&family).0[i],
                    axes(&family).1[j],
                    axes(&family).2[k],
                    mses_dir,
                    &mses,
                );
                let secs = started.elapsed().as_secs_f64();
                let text = value.map_or("nan".to_owned(), |v| format!("{v:.4}"));
                println!(
                    "{family} {i} {j} {k} Mn={} tc={} cam={} -> {text} ({secs:.0} s)",
                    axes(&family).0[i],
                    axes(&family).1[j],
                    axes(&family).2[k]
                );
                let mut f = file.lock().unwrap();
                let extrap = if EXTRAPOLATED.with(|f| f.get()) {
                    " extrap"
                } else {
                    ""
                };
                let failure = TOOL_FAILURE
                    .with(std::cell::Cell::take)
                    .map_or_else(String::new, |status| format!(" tool_failure={status}"));
                let partial = PARTIAL
                    .with(std::cell::RefCell::take)
                    .map_or_else(String::new, |partial| format!(" partial={partial}"));
                writeln!(
                    f,
                    "#cell {family} {i} {j} {k} {text} seconds={secs:.0}{extrap}{failure}{partial}"
                )
                .unwrap();
            });
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("stations") => stations(),
        Some("report") => report(),
        Some("cell") => single_cell(&args[1..]),
        Some("generate") => generate(
            Path::new(&args[1]),
            Path::new(&args[2]),
            args[3].parse().unwrap(),
            &args[4..],
        ),
        Some("assemble") => assemble(Path::new(&args[1]), Path::new(&args[2]), &args[3], &args[4]),
        _ => eprintln!(
            "usage: buffet_table_gen stations | generate <out> <mses_dir> <threads> <family>..."
        ),
    }
}

#[allow(dead_code)]
fn _use(o: BuffetOnset) -> Option<f64> {
    o.cl_b()
}

/// `assemble <cells> <out> <mses_version> <command>`: the shipped data file
/// from the raw cell records.
#[allow(dead_code)]
fn assemble(cells: &Path, out: &Path, mses_version: &str, command: &str) {
    let text = std::fs::read_to_string(cells).unwrap();
    let mut families: Vec<String> = Vec::new();
    for line in text.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.first() == Some(&"#cell") && !families.contains(&p[1].to_owned()) {
            families.push(p[1].to_owned());
        }
    }
    let join = |a: &[f64]| {
        a.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut o = String::new();
    o.push_str("# Buffet-onset tables: normal-section lift coefficient cl_n at which the\n");
    o.push_str("# pre-shock local Mach number reaches 1.38 (Obert, Aerodynamic Design of\n");
    o.push_str("# Transport Aircraft, 2009, ch. 17 pp. 109-110, Figs. 17.5-17.6).\n");
    o.push_str(&format!(
        "# Solver: MSES {mses_version} (mset/mses/mplot), viscous, free transition.\n"
    ));
    o.push_str(
        "# Section: library airfoil of the family with thickness and mean line scaled to the\n",
    );
    o.push_str("# axis values; axes are in the plane normal to the 0.55 c shock line.\n");
    o.push_str("# Onset detection: incidence stepped 1.0 deg from 0 deg; nose suction peak\n");
    o.push_str("# (x/c < 0.15) excluded; shock = steepest upper-surface Mach drop >= 0.15 over\n");
    o.push_str("# two samples at 0.25 <= x/c <= 0.9; onset linearly interpolated in cl_n.\n");
    o.push_str(
        "# Where MSES stopped converging within 0.18 of the onset Mach (cell log `extrap`), the\n",
    );
    o.push_str(
        "# onset is the linear extrapolation of cl_n against pre-shock Mach from the last two\n",
    );
    o.push_str(
        "# converged incidences; a `nan` cell had no such pair and is unavailable at runtime.\n",
    );
    o.push_str(
        "# Cells re-run by alpha continuation (variant=r7mu in the comment before the cell):\n",
    );
    o.push_str(
        "# one polar sweep restarts every angle from the previous converged state (0.25 deg steps,\n",
    );
    o.push_str(
        "# MSET n 191, MUCON 1.5, free transition), each angle's distribution is read from its own\n",
    );
    o.push_str(
        "# checkpoint, a sweep is stopped when no angle converged for 15 min. Angles that do not\n",
    );
    o.push_str(
        "# converge are skipped, never interpolated over; `extrap` marks the extrapolation rule above.\n",
    );
    o.push_str(
        "# KNOWN MODEL BIAS (open fix item): the runtime CL_b reads about 10 % low on the A320 and\n",
    );
    o.push_str(
        "# 15 % low on the B747-400 against Obert 2009 Figs. 28.13 / 28.10 (p. 375: 0.88 at M 0.78,\n",
    );
    o.push_str(
        "# 0.72 at M 0.85). Cause open: not the Reynolds number (tables at Re 2e7; Obert Fig. 28.7\n",
    );
    o.push_str(
        "# p. 373 puts the 1.38 at a 0.55 c shock near Re 3e7) nor the shock-position criterion of\n",
    );
    o.push_str(
        "# Fig. 28.7 (it lowers the A320 to -13 %); the critical strip is the most loaded one.\n",
    );
    o.push_str(&format!("# Generated by: {command}\n"));
    o.push_str(
        "# Assembled by: buffet_table_gen assemble (alas-aero example) from the raw cell log.\n",
    );
    o.push_str("# Cell log: one record per cell, `nan` where MSES failed before onset.\n");
    o.push_str(&format!(
        "reynolds_n {REYNOLDS_N:e}\ncriterion pre_shock_mach {M_ONSET}\n"
    ));
    for family in &families {
        o.push_str(&format!(
            "family {family}\naxis mach {}\naxis tc {}\naxis camber {}\n",
            join(axes(family).0),
            join(axes(family).1),
            join(axes(family).2)
        ));
        // One record per cell: the last finite value, else `nan`. A later
        // `nan` record (a re-run that failed) never replaces a finite one.
        let mut cells: Vec<(String, String, String, String, String)> = Vec::new();
        for line in text.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.first() == Some(&"#cell") && p[1] == family {
                let key = (p[2].to_owned(), p[3].to_owned(), p[4].to_owned());
                let finite = p[5].parse::<f64>().is_ok_and(f64::is_finite);
                match cells
                    .iter_mut()
                    .find(|c| (c.0.clone(), c.1.clone(), c.2.clone()) == key)
                {
                    Some(c) if finite || c.3 == "nan" => {
                        c.3 = p[5].to_owned();
                        c.4 = p[6..].join(" ");
                    }
                    Some(_) => {}
                    None => cells.push((key.0, key.1, key.2, p[5].to_owned(), p[6..].join(" "))),
                }
            }
        }
        // A cell whose only records are tool failures has no flow result.
        if let Some(c) = cells.iter().find(|c| {
            c.3 == "nan"
                && TOOL_FAILURE_STATUSES
                    .iter()
                    .any(|status| c.4.contains(&format!("tool_failure={status}")))
        }) {
            panic!(
                "cell {family} {} {} {} has no onset because the solver tool failed ({}); re-run it",
                c.0, c.1, c.2, c.4
            );
        }
        for c in &cells {
            if c.4.contains("variant=") || c.4.contains("extrap") {
                o.push_str(&format!("# cell {} {} {}: {}\n", c.0, c.1, c.2, c.4));
            }
            o.push_str(&format!("cell {} {} {} {}\n", c.0, c.1, c.2, c.3));
        }
        o.push_str("end\n");
    }
    std::fs::write(out, o).unwrap();
}

/// `report`: buffet onset and the load factor to it at the declared MTOW,
/// cruise Mach and altitude, for every registered preset, with the runtime
/// cost of one evaluation.
fn report() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(plane) = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
        else {
            println!("{name} BUILD FAILED");
            continue;
        };
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, design.sweep_deg),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        let req = &config.requirements;
        let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * v * v;
        let cl_cruise = req.mtow_kg * req.gravity_m_s2 / (q * plane.s_ref);
        let started = std::time::Instant::now();
        let mut onset = aero.buffet_onset(req.cruise_mach, req.cruise_altitude_m);
        for _ in 0..4 {
            onset = aero.buffet_onset(req.cruise_mach, req.cruise_altitude_m);
        }
        let ms = started.elapsed().as_secs_f64() * 200.0;
        let (status, detail) = match &onset {
            BuffetOnset::Interpolated(e) => (
                "interpolated",
                format!(
                    "CLb={:.3} n={:.3} cln_onset={:.3} family={}",
                    e.cl_b,
                    e.cl_b / cl_cruise,
                    e.cl_n_onset,
                    e.strip.family
                ),
            ),
            BuffetOnset::Extrapolated(e, axes) => (
                "EXTRAPOLATED",
                format!(
                    "CLb={:.3} n={:.3} axes={axes:?} family={}",
                    e.cl_b,
                    e.cl_b / cl_cruise,
                    e.strip.family
                ),
            ),
            BuffetOnset::Unavailable(r) => ("UNAVAILABLE", format!("{r:?}")),
        };
        println!(
            "{name} M={} CLcr={cl_cruise:.3} {status} {detail} ({ms:.0} ms/eval)",
            req.cruise_mach
        );
    }
}

/// The family's base section: the library airfoil, or, when `SECTIONS_FILE`
/// names a section data file in the digitised-sections format (`@NAME`, one
/// description line, then `x z` pairs in Selig order), the section of that
/// name from the file, normalized as the library normalizes its sections.
fn base_section(family: &str) -> alas_geom::aircraft::airfoil::Airfoil {
    if let Ok(path) = std::env::var("SECTIONS_FILE") {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut coords: Vec<(f64, f64)> = Vec::new();
        let (mut inside, mut description) = (false, false);
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('@') {
                inside = name.trim() == family;
                description = inside;
                continue;
            }
            if description {
                description = false;
                continue;
            }
            if inside {
                let v: Vec<f64> = line
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                if let [x, z] = v[..] {
                    coords.push((x, z));
                }
            }
        }
        if !coords.is_empty() {
            return alas_geom::aircraft::airfoil::Airfoil::from_coordinates(
                family,
                alas_geom::airfoil_library::normalize_coordinates(&coords),
            );
        }
    }
    AirfoilLibrary::get(family).unwrap()
}

/// `cell <family> <mach> <tc> <camber> <mses_dir>`: one cell with progress.
#[allow(dead_code)]
fn single_cell(args: &[String]) {
    let base = base_section(&args[0]);
    let mut mses = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .unwrap()
        .mses;
    mses.enabled = true;
    apply_variant(&mut mses);
    mses.mses_dir = args[4].clone();
    let v = cell_onset(
        &base,
        args[1].parse().unwrap(),
        args[2].parse().unwrap(),
        args[3].parse().unwrap(),
        Path::new(&args[4]),
        &mses,
    );
    println!("onset {v:?}");
    if EXTRAPOLATED.with(|f| f.get()) {
        println!("extrapolated");
    }
    if let Some(status) = TOOL_FAILURE.with(std::cell::Cell::get) {
        println!("tool_failure {status}");
    }
    if let Some(partial) = PARTIAL.with(std::cell::RefCell::take) {
        println!("partial {partial}");
    }
}
