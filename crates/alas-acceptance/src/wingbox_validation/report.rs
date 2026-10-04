// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Comparison metrics and reproducible retained summary.

use super::{f06, model::Model, quadrature};
use serde_json::{json, Value};
use std::{fs, io, path::Path};

pub(super) fn compare(model: &Model, result: &f06::Result) -> io::Result<Value> {
    let n = model.case.y.len();
    if n < 2 || model.case.y[0] != 0.0 || model.case.y[n - 1] <= 0.0 {
        return Err(io::Error::other("invalid centreline station grid"));
    }
    if !complete_ends(&result.stress_ends, n) || !complete_ends(&result.force_ends, n) {
        return Err(io::Error::other(
            "beam force/stress endpoint coverage incomplete",
        ));
    }
    let displacement = profile(&result.displacement, n)?;
    let stress = profile(&result.stress, n)?;
    let moment = profile(&result.moment, n)?;
    let native_stress: Vec<_> = (0..n)
        .map(|station| {
            model
                .case
                .spar_stress
                .iter()
                .map(|spar| spar.stress_pa[station])
                .fold(0.0_f64, f64::max)
        })
        .collect();
    let native_max = maximum(&native_stress);
    let nastran_max = maximum(&stress);
    let length = model.case.y[n - 1];
    let mid = length / 2.0;
    let moment_scale = model
        .case
        .moment_nm
        .iter()
        .map(|m| m.abs())
        .fold(0.0_f64, f64::max);
    let moment_error = moment
        .iter()
        .zip(&model.case.moment_nm)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    let bounds = (0..n)
        .map(|station| {
            quadrature::station_bound(
                &model.case.y,
                &model.case.moment_nm,
                &model.ei,
                &model.case.q_net,
                &model.point_forces,
                station,
            )
        })
        .collect::<io::Result<Vec<_>>>()?;
    let roundoff: Vec<_> = displacement
        .iter()
        .copied()
        .map(quadrature::printed_roundoff)
        .collect();
    let native_mid = interpolate(&model.case.y, &model.case.deflection_m, mid);
    let nastran_mid = interpolate(&model.case.y, &displacement, mid);
    let criterion = consistency(
        [model.case.tip_deflection_m, native_mid],
        [displacement[n - 1], nastran_mid],
        [bounds[n - 1], interpolate(&model.case.y, &bounds, mid)],
        [roundoff[n - 1], interpolate(&model.case.y, &roundoff, mid)],
    );
    Ok(json!({
        "preset": model.name, "version": result.version, "stations": n, "semi_span_m": length,
        "sizing_converged": model.converged,
        "tip_m": metric(model.case.tip_deflection_m, displacement[n-1])?,
        "mid_span_m": metric(native_mid, nastran_mid)?,
        "root_stress_pa": metric(native_stress[0], stress[0])?,
        "max_stress_pa": metric(native_stress[native_max], stress[nastran_max])?,
        "native_max_station_m": model.case.y[native_max], "nastran_max_station_m": model.case.y[nastran_max],
        "max_station_difference_percent_span": 100.0 * (model.case.y[native_max] - model.case.y[nastran_max]) / length,
        "max_moment_error_percent_root_scale": relative_percent(moment_error, moment_scale),
        "displacement_criterion": criterion,
        "nastran_deflection_m": displacement, "nastran_cap_stress_pa": stress,
    }))
}

fn complete_ends(ends: &std::collections::BTreeSet<(usize, usize, bool)>, n: usize) -> bool {
    let expected = (1..n)
        .flat_map(|eid| [(eid, eid, false), (eid, eid + 1, true)])
        .collect();
    *ends == expected
}

pub(super) fn shell_compare(model: &Model, result: &f06::Result) -> io::Result<Value> {
    let n = model.case.y.len();
    let displacement = profile(&result.displacement, n)?;
    Ok(
        json!({ "version": result.version, "tip_m": metric(model.case.tip_deflection_m, displacement[n-1])?, "mid_span_m": metric(interpolate(&model.case.y, &model.case.deflection_m, model.case.y[n-1]/2.0), interpolate(&model.case.y, &displacement, model.case.y[n-1]/2.0))?, "nastran_deflection_m": displacement }),
    )
}

fn profile(map: &std::collections::BTreeMap<usize, f64>, n: usize) -> io::Result<Vec<f64>> {
    (1..=n)
        .map(|id| {
            map.get(&id)
                .copied()
                .filter(|v| v.is_finite())
                .ok_or_else(|| io::Error::other(format!("F06 station {id} missing or non-finite")))
        })
        .collect()
}

fn metric(native: f64, nastran: f64) -> io::Result<Value> {
    let difference = native - nastran;
    if !native.is_finite() || !nastran.is_finite() || !difference.is_finite() {
        return Err(io::Error::other("non-finite comparison metric"));
    }
    let percent = relative_percent(difference, nastran);
    Ok(
        json!({"native": native, "nastran": nastran, "absolute_difference": difference.abs(),
        "difference_percent": percent, "reference_status": if percent.is_some() { "finite" } else { "near_zero" }}),
    )
}

fn relative_percent(difference: f64, reference: f64) -> Option<f64> {
    let scale = difference.abs().max(reference.abs());
    if !difference.is_finite()
        || !reference.is_finite()
        || reference.abs() <= f64::MIN_POSITIVE.max(f64::EPSILON * scale)
    {
        return None;
    }
    let value = 100.0 * (difference / reference);
    value.is_finite().then_some(value)
}

fn percent_text(value: &Value, precision: usize) -> String {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .map_or_else(|| "N/A".into(), |v| format!("{v:+.precision$}%"))
}

fn consistency(native: [f64; 2], nastran: [f64; 2], bound: [f64; 2], roundoff: [f64; 2]) -> Value {
    let consistent = (0..2).all(|i| (native[i] - nastran[i]).abs() <= bound[i] + roundoff[i]);
    json!({"verdict": if consistent {"consistent_with_native_quadrature_bound"} else {"outside_native_quadrature_bound"},
        "tip_native_quadrature_bound_m": bound[0], "mid_native_quadrature_bound_m": bound[1],
        "tip_f06_roundoff_m": roundoff[0], "mid_f06_roundoff_m": roundoff[1]})
}

fn maximum(values: &[f64]) -> usize {
    values
        .iter()
        .enumerate()
        .fold((0, f64::NEG_INFINITY), |best, (i, &v)| {
            if v > best.1 {
                (i, v)
            } else {
                best
            }
        })
        .0
}

fn interpolate(y: &[f64], values: &[f64], position: f64) -> f64 {
    let i = y
        .windows(2)
        .position(|pair| position >= pair[0] && position <= pair[1])
        .unwrap_or(0);
    let fraction = (position - y[i]) / (y[i + 1] - y[i]);
    values[i] + fraction * (values[i + 1] - values[i])
}

pub(super) fn write(path: &Path, rows: &[Value]) -> io::Result<()> {
    fs::write(
        path.join("comparison.json"),
        serde_json::to_vec_pretty(rows).map_err(io::Error::other)?,
    )?;
    let mut summary = String::from("# Equivalent wing-box beam validation\n\nSI units; positive-y semi-wing from aircraft centreline, z up. Linear static SOL 101, fully clamped root. Difference = 100 (native / Nastran - 1). Mid-span interpolates both station profiles at half semi-span.\n\n| Preset | Tip native / MSC (m) | Difference | Mid native / MSC (m) | Difference | Root cap native / MSC (MPa) | Difference | Max stress native / MSC (MPa) | Max station native / MSC (m) | Station difference (% span) |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for row in rows {
        let v = |key: &str, field: &str| row[key][field].as_f64().unwrap_or(f64::NAN);
        let p = |key: &str, precision| percent_text(&row[key]["difference_percent"], precision);
        summary.push_str(&format!("| {} | {:.6} / {:.6} | {} | {:.6} / {:.6} | {} | {:.3} / {:.3} | {} | {:.3} / {:.3} | {:.6} / {:.6} | {} |\n", row["preset"].as_str().unwrap_or("?"), v("tip_m","native"),v("tip_m","nastran"),p("tip_m",3),v("mid_span_m","native"),v("mid_span_m","nastran"),p("mid_span_m",3),v("root_stress_pa","native")/1e6,v("root_stress_pa","nastran")/1e6,p("root_stress_pa",4),v("max_stress_pa","native")/1e6,v("max_stress_pa","nastran")/1e6,row["native_max_station_m"].as_f64().unwrap_or(f64::NAN),row["nastran_max_station_m"].as_f64().unwrap_or(f64::NAN),percent_text(&row["max_station_difference_percent_span"],3)));
    }
    summary.push_str("\n## Idealisation and reproducibility\n\nCBEAM/PBEAM at the unchanged native stations; endpoint properties vary linearly. Reference MAT1 uses cap E and nu, I1=actual compatible EI/E_cap. Transformed A and I2 retain material axial ratios. J is an unloaded Bredt outer-cell proxy; torsion is not validated. End stress recovery at +/-maximum spar depth/2 gives maximum cap fibre stress. K1=K2=0 removes transverse shear. No GRAV or density load is added because q_net already contains final structure and declared fuel relief. Constant segment-average PLOAD1 reproduces native double-trapezoid station moments, and concentrated PLOAD1 applies point inertia at actual positions. The final product stiffness-sized response supplies the loads, so swept member mass relief is retained even though the validation beam is straight.\n\nNative trapezoidal curvature integration and tapered-element integration can differ; the conditional displacement criterion below quantifies the native quadrature bound. Max-stress stations can be ambiguous on near-constant strength-sized stress plateaus. Agreement is numerical verification of this beam idealisation, not aircraft physical validation. Shell deformation is a separate higher-fidelity check with transverse shear, Poisson and taper effects; it has no ribs or buckling/nonlinear validation.\n\nReproduce: `cargo run -p alas-acceptance --bin wingbox_nastran_validation -- --output-dir .agent/nastran/<new-run>`. Optional `--shell` exports and solves the separate straight shell box. Existing result directories are retained rather than overwritten. Native profiles and assumptions are in each preset's native.json; decks, F06 and solver status are alongside them.\n\nPrimary card semantics: [MSC Linear Static Analysis User Guide](https://documentation-be.hexagon.com/bundle/MSC_Nastran_2025.1_Linear_Static_Analysis_User_Guide/raw/resource/enus/MSC_Nastran_2025.1_Linear_Static_Analysis_User_Guide.pdf), [MSC Quick Reference Guide](https://documentation-be.hexagon.com/bundle/MSC_Nastran_2022.1_Quick_Reference_Guide/raw/resource/enus/MSC_Nastran_2022.1_Quick_Reference_Guide.pdf).\n");
    for row in rows {
        summary.push_str(&format!("\n{}: MSC {}; {} stations; moment equilibrium residual {} of maximum native moment; product sizing converged={}.", row["preset"].as_str().unwrap_or("?"),row["version"].as_str().unwrap_or("?"),row["stations"],percent_text(&row["max_moment_error_percent_root_scale"],6),row["sizing_converged"]));
        let c = &row["displacement_criterion"];
        summary.push_str(&format!(" Displacement verdict: {}; native quadrature bounds tip/mid={:.9}/{:.9} m, F06 printing bounds={:.9}/{:.9} m.",
            c["verdict"].as_str().unwrap_or("not assessed"),
            c["tip_native_quadrature_bound_m"].as_f64().unwrap_or(f64::NAN),
            c["mid_native_quadrature_bound_m"].as_f64().unwrap_or(f64::NAN),
            c["tip_f06_roundoff_m"].as_f64().unwrap_or(f64::NAN),
            c["mid_f06_roundoff_m"].as_f64().unwrap_or(f64::NAN)));
        if let Some(error) = row["shell"]["error"].as_str() {
            summary.push_str(&format!(" Shell check unavailable: {error}."));
        } else if row.get("shell").is_some() {
            summary.push_str(&format!(
                " Shell tip native/MSC={:.6}/{:.6} m ({}); mid-span={:.6}/{:.6} m ({}).",
                row["shell"]["tip_m"]["native"].as_f64().unwrap_or(f64::NAN),
                row["shell"]["tip_m"]["nastran"]
                    .as_f64()
                    .unwrap_or(f64::NAN),
                percent_text(&row["shell"]["tip_m"]["difference_percent"], 3),
                row["shell"]["mid_span_m"]["native"]
                    .as_f64()
                    .unwrap_or(f64::NAN),
                row["shell"]["mid_span_m"]["nastran"]
                    .as_f64()
                    .unwrap_or(f64::NAN),
                percent_text(&row["shell"]["mid_span_m"]["difference_percent"], 3)
            ));
        }
    }
    summary.push_str("\n\n## Numerical criterion\n\nThe displacement verdict checks tip and interpolated mid-span absolute differences against an independently derived native trapezoid error bound plus F06 decimal-print rounding. For virtual-work f(s)=(target-s) M(s)/EI(s), each cell uses the Peano kernel K=r(h-r)/2: |T(f)-integral(f)| <= integral(K sup|f''|) + sum(K(point)|jump f'|). The exported EI is positive and linear per cell, M is piecewise quadratic under segment-average q, and point forces create derivative jumps (target-point) F/EI(point). Smooth bounds use exact extrema of M and M', minimum EI and its slope on each subinterval. Shared-station interpolation also interpolates these bounds. F06 beam tables print seven significant digits, giving half a last-decimal-unit rounding bound per field.\n\nThis establishes native quadrature error against the continuum idealisation, not the MSC tapered-element integration error. A verdict of consistent_with_native_quadrature_bound is numerical consistency at these two locations; it does not prove absence of unit, section or boundary-condition bugs, nor physical aircraft validity. Endpoint stresses, moments and peak locations remain reported measurements. Analytic cantilever and compatible-section tests are independent checks. Percentages are N/A for zero/subnormal references or references at binary64 machine precision relative to the compared difference; their absolute differences remain available in comparison.json.\n");
    fs::write(path.join("summary.md"), &summary)?;
    let escaped = summary
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    fs::write(path.join("summary.html"),format!("<!doctype html><meta charset=utf-8><title>Wing-box numerical validation</title><style>body{{max-width:1200px;margin:40px auto;font:16px system-ui;background:#f6f7f9;color:#172635}}pre{{white-space:pre-wrap;line-height:1.6;background:white;padding:24px;overflow:auto}}</style><h1>Wing-box numerical validation</h1><p>External solver evidence retained at {}</p><pre>{escaped}</pre>",path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn endpoint_coverage_rejects_a_missing_interior_end_and_wrong_grid() {
        let full = BTreeSet::from([(1, 1, false), (1, 2, true), (2, 2, false), (2, 3, true)]);
        assert!(complete_ends(&full, 3));
        let mut missing = full.clone();
        missing.remove(&(1, 2, true));
        assert!(!complete_ends(&missing, 3));
        let mut wrong = full.clone();
        wrong.remove(&(2, 3, true));
        wrong.insert((2, 3, false));
        assert!(!complete_ends(&wrong, 3));
    }

    #[test]
    fn zero_and_near_zero_references_retain_absolute_differences_without_percentages() {
        for (native, reference) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, f64::EPSILON.powi(2)),
            (0.0, f64::MIN_POSITIVE / 2.0),
        ] {
            let value = metric(native, reference).unwrap();
            assert!(value["difference_percent"].is_null());
            assert_eq!(value["absolute_difference"], (native - reference).abs());
            assert_eq!(percent_text(&value["difference_percent"], 3), "N/A");
        }
        assert_eq!(metric(-2.0, -1.0).unwrap()["difference_percent"], 100.0);
        assert!(metric(f64::INFINITY, 1.0).is_err());
        assert!(metric(f64::MAX, -f64::MAX).is_err());
    }

    #[test]
    fn displacement_verdict_uses_cantilever_bound_and_rejects_a_unit_scale_error() {
        // L=EI=1, tip force=3. Closed form at x is 3*x^2*(3-x)/6.
        let y = [0.0, 0.5, 1.0];
        let moment = [3.0, 1.5, 0.0];
        let bounds = [2, 1].map(|i| {
            quadrature::station_bound(&y, &moment, &[1.0; 3], &[0.0; 3], &[(1.0, 3.0)], i).unwrap()
        });
        let native = [1.125, 0.375];
        let closed_form = [1.0, 0.3125];
        assert_eq!(
            consistency(native, closed_form, bounds, [0.0; 2])["verdict"],
            "consistent_with_native_quadrature_bound"
        );
        assert_eq!(
            consistency(native, closed_form.map(|v| 1000.0 * v), bounds, [0.0; 2])["verdict"],
            "outside_native_quadrature_bound"
        );
    }
}
