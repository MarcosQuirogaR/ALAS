// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodynamic and performance plausibility rules: payload-range corners,
//! drag polar, lift limits, V-n diagram and V-speed schedule.

use super::{Findings, RuleInfo, Severity};

pub(super) const A_PR_ORDER: RuleInfo = RuleInfo {
    id: "PAYRANGE-ORDER",
    statement: "Payload-range corners A-D have non-decreasing range and non-increasing payload; the last corner (ferry) carries no more than 5 % of the first payload",
    source: "Definition of the payload-range diagram (Raymer Sec. 17; Torenbeek Sec. 8.5)",
};
pub(super) const A_PR_MONOTONIC: RuleInfo = RuleInfo {
    id: "PAYRANGE-BOUNDS",
    statement: "Corner payloads lie in 0..(MTOW - OEW); the longest range exceeds the max-payload range by at least 10 %",
    source: "Mass closure; a flat chart means fuel capacity adds nothing (Torenbeek Sec. 8.5)",
};
pub(super) const A_POLAR_CONVEX: RuleInfo = RuleInfo {
    id: "POLAR-CONVEX",
    statement: "Between CL 0.05 and the cruise CL the drag polar CD(CL) is convex: dCD/dCL never falls by more than 0.01 from one interval to the next",
    source: "Parabolic polar CD = CD0 + k (CL - CLmind)^2 (Raymer Sec. 12.5); a concave stretch is a solver artefact",
};
pub(super) const A_POLAR_CD0: RuleInfo = RuleInfo {
    id: "POLAR-CD0",
    statement: "Minimum drag coefficient 0.012-0.035 usual; MEDIUM outside 0.010-0.045; HIGH outside 0.006-0.07",
    source: "Raymer Table 12.3 (transport CD0 0.014-0.026, turboprop 0.020-0.035)",
};
pub(super) const A_POLAR_LD: RuleInfo = RuleInfo {
    id: "POLAR-LD-MAX",
    statement: "Maximum lift-to-drag ratio 10-24 usual; MEDIUM outside 9-27; HIGH outside 6-35",
    source: "Raymer Table 3.6; Torenbeek Sec. 7 (transport L/D(max) 14-22)",
};
pub(super) const A_CLMAX: RuleInfo = RuleInfo {
    id: "AERO-CLMAX",
    statement: "Take-off CLmax 1.4-2.8 and landing CLmax 1.8-3.6 usual; landing >= take-off; HIGH outside 1.0-4.2",
    source: "Raymer Table 5.2; Roskam Part I Table 3.1 (flapped CLmax of transports)",
};
pub(super) const A_VN_ORDER: RuleInfo = RuleInfo {
    id: "VN-SPEED-ORDER",
    statement:
        "VS < VA <= VC < VD and the operating cruise EAS does not exceed VC by more than 3 %",
    source: "CS 25.335 (design speeds); CS 25.335(b) VD >= 1.25 VC",
};
pub(super) const A_VN_LOAD_FACTORS: RuleInfo = RuleInfo {
    id: "VN-LOAD-FACTORS",
    statement: "Positive limit load factor >= 2.1 + 24,000/(W + 10,000) (not above 3.8); negative limit about -1.0 (-0.9 to -1.5); ultimate = 1.5 x limit",
    source: "CS 25.337(b)(c), CS 25.303",
};
pub(super) const A_VSPEED_ORDER: RuleInfo = RuleInfo {
    id: "VSPEED-ORDER",
    statement: "Take-off speeds ordered VSR < VR, V1 <= VR <= V2; landing VSR0 < VAPP and touchdown speed VTD <= VAPP; all finite and positive",
    source: "CS 25.107, CS 25.149",
};
pub(super) const A_VSPEED_MARGINS: RuleInfo = RuleInfo {
    id: "VSPEED-MARGINS",
    statement: "V2 >= 1.13 VSR, VR >= 1.05 VMC, VAPP >= 1.23 VSR0 (and at most 1.6 VSR0)",
    source: "CS 25.107(b), 25.107(e), 25.125(b)",
};

/// Take-off speed and landing speed schedule, m/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VSpeedFacts {
    /// Take-off reference stall speed.
    pub v_stall_to: f64,
    /// Landing reference stall speed.
    pub v_stall_land: f64,
    /// Minimum control speed.
    pub v_mc: f64,
    /// Decision speed proxy.
    pub v1: f64,
    /// Rotation speed.
    pub vr: f64,
    /// Take-off safety speed.
    pub v2: f64,
    /// Approach speed.
    pub v_app: f64,
    /// Touchdown speed.
    pub v_td: f64,
}

/// V-n diagram numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VnFacts {
    /// Stall speed, kt.
    pub vs_kt: f64,
    /// Manoeuvring speed, kt.
    pub va_kt: f64,
    /// Design cruise speed, kt.
    pub vc_kt: f64,
    /// Design dive speed, kt.
    pub vd_kt: f64,
    /// Operating cruise EAS, kt.
    pub v_cruise_op_kt: f64,
    /// Positive limit load factor.
    pub n_pos: f64,
    /// Negative limit load factor.
    pub n_neg: f64,
    /// Positive ultimate load factor.
    pub n_ult_pos: f64,
    /// Design takeoff mass, kg.
    pub mass_kg: f64,
}

fn banded(value: f64, usual: (f64, f64), medium: (f64, f64), high: (f64, f64)) -> Option<Severity> {
    if !value.is_finite() || value < high.0 || value > high.1 {
        Some(Severity::High)
    } else if value < medium.0 || value > medium.1 {
        Some(Severity::Medium)
    } else if value < usual.0 || value > usual.1 {
        Some(Severity::Low)
    } else {
        None
    }
}

/// Payload-range corners as `(range_nm, payload_kg)` for A..D.
pub fn check_payload_range(
    out: &mut Findings,
    corners: &[(f64, f64); 4],
    oew_kg: f64,
    mtow_kg: f64,
) {
    for pair in corners.windows(2) {
        if pair[1].0 + 1e-6 < pair[0].0 || pair[1].1 > pair[0].1 + 1e-6 {
            out.add(
                A_PR_ORDER.id,
                Severity::High,
                format!(
                    "corner order broken: ({:.0} nmi, {:.0} kg) then ({:.0} nmi, {:.0} kg)",
                    pair[0].0, pair[0].1, pair[1].0, pair[1].1
                ),
            );
        }
    }
    if corners[3].1 > 0.05 * corners[0].1.max(1.0) {
        out.add(
            A_PR_ORDER.id,
            Severity::Medium,
            format!(
                "ferry corner still carries {:.0} kg of {:.0} kg",
                corners[3].1, corners[0].1
            ),
        );
    }
    let budget = (mtow_kg - oew_kg).max(0.0);
    if corners.iter().any(|c| c.1 < -1e-6 || c.1 > budget + 1.0) {
        out.add(
            A_PR_MONOTONIC.id,
            Severity::High,
            format!("payload outside 0..{budget:.0} kg (MTOW {mtow_kg:.0} - OEW {oew_kg:.0})"),
        );
    }
    if corners[3].0 < 1.1 * corners[0].0 {
        out.add(
            A_PR_MONOTONIC.id,
            Severity::Medium,
            format!(
                "ferry range {:.0} nmi is under 110 % of the max-payload range {:.0} nmi",
                corners[3].0, corners[0].0
            ),
        );
    }
}

/// Drag polar on the positive-lift branch. `cruise_cl` bounds the convexity check.
pub fn check_polar(out: &mut Findings, cl: &[f64], cd: &[f64], l_over_d: &[f64], cruise_cl: f64) {
    let mut pts: Vec<(f64, f64)> = cl
        .iter()
        .zip(cd)
        .map(|(&a, &b)| (a, b))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(min_cd) = pts.iter().map(|p| p.1).reduce(f64::min) {
        if let Some(sev) = banded(min_cd, (0.012, 0.035), (0.010, 0.045), (0.006, 0.07)) {
            out.add(A_POLAR_CD0.id, sev, format!("minimum CD {min_cd:.4}"));
        }
    }
    if let Some(max_ld) = l_over_d
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .reduce(f64::max)
    {
        if let Some(sev) = banded(max_ld, (10.0, 24.0), (9.0, 27.0), (6.0, 35.0)) {
            out.add(A_POLAR_LD.id, sev, format!("maximum L/D {max_ld:.1}"));
        }
    }
    let band: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|p| p.0 >= 0.05 && p.0 <= cruise_cl.max(0.5))
        .collect();
    let slopes: Vec<(f64, f64)> = band
        .windows(2)
        .filter(|w| w[1].0 - w[0].0 > 1e-6)
        .map(|w| {
            (
                0.5 * (w[0].0 + w[1].0),
                (w[1].1 - w[0].1) / (w[1].0 - w[0].0),
            )
        })
        .collect();
    let worst = slopes.windows(2).map(|w| (w[1].0, w[1].1 - w[0].1)).fold(
        None::<(f64, f64)>,
        |acc, item| match acc {
            Some(a) if a.1 <= item.1 => Some(a),
            _ => Some(item),
        },
    );
    if let Some((at, drop)) = worst {
        if drop < -0.01 {
            out.add(
                A_POLAR_CONVEX.id,
                Severity::Medium,
                format!("dCD/dCL falls by {:.3} near CL {at:.2}", -drop),
            );
        }
    }
}

/// Take-off and landing maximum lift coefficients from the configuration.
pub fn check_cl_max(out: &mut Findings, cl_max_to: f64, cl_max_land: f64) {
    if let Some(sev) = banded(cl_max_to, (1.4, 2.8), (1.4, 2.8), (1.0, 4.2)) {
        out.add(A_CLMAX.id, sev, format!("take-off CLmax {cl_max_to:.2}"));
    }
    if let Some(sev) = banded(cl_max_land, (1.8, 3.6), (1.8, 3.6), (1.0, 4.2)) {
        out.add(A_CLMAX.id, sev, format!("landing CLmax {cl_max_land:.2}"));
    }
    if cl_max_land + 1e-9 < cl_max_to {
        out.add(
            A_CLMAX.id,
            Severity::Medium,
            format!("landing CLmax {cl_max_land:.2} is below take-off CLmax {cl_max_to:.2}"),
        );
    }
}

/// V-n diagram speeds and load factors.
pub fn check_vn(out: &mut Findings, vn: &VnFacts) {
    let ordered = vn.vs_kt < vn.va_kt && vn.va_kt <= vn.vc_kt && vn.vc_kt < vn.vd_kt;
    if !ordered
        || [vn.vs_kt, vn.va_kt, vn.vc_kt, vn.vd_kt]
            .iter()
            .any(|v| !v.is_finite())
    {
        out.add(
            A_VN_ORDER.id,
            Severity::High,
            format!(
                "VS {:.0} VA {:.0} VC {:.0} VD {:.0} kt",
                vn.vs_kt, vn.va_kt, vn.vc_kt, vn.vd_kt
            ),
        );
    } else if vn.v_cruise_op_kt > 1.03 * vn.vc_kt {
        out.add(
            A_VN_ORDER.id,
            Severity::Medium,
            format!(
                "operating cruise EAS {:.0} kt exceeds VC {:.0} kt",
                vn.v_cruise_op_kt, vn.vc_kt
            ),
        );
    }
    let w_lb = vn.mass_kg * 2.204_62;
    let required = (2.1 + 24_000.0 / (w_lb + 10_000.0)).min(3.8);
    if vn.n_pos + 1e-9 < required {
        out.add(
            A_VN_LOAD_FACTORS.id,
            Severity::High,
            format!(
                "n_pos {:.2} below the CS 25.337 minimum {required:.2}",
                vn.n_pos
            ),
        );
    }
    if vn.n_pos > 3.8 + 1e-9 {
        out.add(
            A_VN_LOAD_FACTORS.id,
            Severity::Medium,
            format!("n_pos {:.2} above 3.8", vn.n_pos),
        );
    }
    if !(-1.5..=-0.9).contains(&vn.n_neg) {
        out.add(
            A_VN_LOAD_FACTORS.id,
            Severity::Medium,
            format!("n_neg {:.2} outside -1.5..-0.9", vn.n_neg),
        );
    }
    if (vn.n_ult_pos - 1.5 * vn.n_pos).abs() > 0.01 {
        out.add(
            A_VN_LOAD_FACTORS.id,
            Severity::Medium,
            format!(
                "ultimate {:.2} is not 1.5 x limit {:.2}",
                vn.n_ult_pos, vn.n_pos
            ),
        );
    }
}

/// Take-off and landing V-speed schedule.
pub fn check_v_speeds(out: &mut Findings, v: &VSpeedFacts) {
    let to_order = v.v1 <= v.vr + 1e-9 && v.vr <= v.v2 + 1e-9 && v.v_stall_to < v.vr;
    let all_finite = [
        v.v_stall_to,
        v.v_stall_land,
        v.v_mc,
        v.v1,
        v.vr,
        v.v2,
        v.v_app,
        v.v_td,
    ]
    .iter()
    .all(|x| x.is_finite() && *x > 0.0);
    if !all_finite || !to_order || v.v_td > v.v_app + 1e-9 || v.v_stall_land >= v.v_app {
        out.add(
            A_VSPEED_ORDER.id,
            Severity::High,
            format!(
                "VSR {:.1} VMC {:.1} V1 {:.1} VR {:.1} V2 {:.1} VSR0 {:.1} VAPP {:.1} VTD {:.1} m/s",
                v.v_stall_to, v.v_mc, v.v1, v.vr, v.v2, v.v_stall_land, v.v_app, v.v_td
            ),
        );
        return;
    }
    if v.v2 + 1e-9 < 1.13 * v.v_stall_to {
        out.add(
            A_VSPEED_MARGINS.id,
            Severity::High,
            format!("V2/VSR = {:.3} < 1.13", v.v2 / v.v_stall_to),
        );
    }
    if v.vr + 1e-9 < 1.05 * v.v_mc {
        out.add(
            A_VSPEED_MARGINS.id,
            Severity::High,
            format!("VR/VMC = {:.3} < 1.05", v.vr / v.v_mc),
        );
    }
    let ratio = v.v_app / v.v_stall_land;
    if ratio + 1e-9 < 1.23 {
        out.add(
            A_VSPEED_MARGINS.id,
            Severity::High,
            format!("VAPP/VSR0 = {ratio:.3} < 1.23"),
        );
    } else if ratio > 1.6 {
        out.add(
            A_VSPEED_MARGINS.id,
            Severity::Medium,
            format!("VAPP/VSR0 = {ratio:.3} > 1.6"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: impl FnOnce(&mut Findings)) -> Findings {
        let mut out = Findings::new("synthetic");
        f(&mut out);
        out
    }

    #[test]
    fn healthy_payload_range_passes_and_broken_order_fails() {
        let good = [
            (0.0, 20_000.0),
            (1_500.0, 20_000.0),
            (2_800.0, 9_000.0),
            (3_600.0, 0.0),
        ];
        assert!(run(|o| check_payload_range(o, &good, 42_000.0, 78_000.0))
            .items()
            .is_empty());
        let bad = [
            (1_500.0, 20_000.0),
            (1_200.0, 20_000.0),
            (2_800.0, 40_000.0),
            (3_600.0, 0.0),
        ];
        let out = run(|o| check_payload_range(o, &bad, 42_000.0, 78_000.0));
        assert_eq!(out.severity_of("PAYRANGE-ORDER"), Some(Severity::High));
        assert_eq!(out.severity_of("PAYRANGE-BOUNDS"), Some(Severity::High));
    }

    #[test]
    fn convex_polar_passes_and_a_dent_is_flagged() {
        let cl: Vec<f64> = (0..20).map(|i| 0.05 + 0.04 * i as f64).collect();
        let cd: Vec<f64> = cl
            .iter()
            .map(|c| 0.02 + 0.045 * (c - 0.2) * (c - 0.2))
            .collect();
        let ld: Vec<f64> = cl.iter().zip(&cd).map(|(l, d)| l / d).collect();
        assert!(run(|o| check_polar(o, &cl, &cd, &ld, 0.8))
            .items()
            .is_empty());
        let mut dented = cd.clone();
        dented[10] -= 0.01;
        let out = run(|o| check_polar(o, &cl, &dented, &ld, 0.8));
        assert!(out.has("POLAR-CONVEX"));
    }

    #[test]
    fn absurd_polar_levels_are_high() {
        let cl = [0.1, 0.3, 0.5];
        let cd = [0.004, 0.005, 0.007];
        let ld = [40.0, 60.0, 70.0];
        let out = run(|o| check_polar(o, &cl, &cd, &ld, 0.8));
        assert_eq!(out.severity_of("POLAR-CD0"), Some(Severity::High));
        assert_eq!(out.severity_of("POLAR-LD-MAX"), Some(Severity::High));
    }

    #[test]
    fn clmax_bands() {
        assert!(run(|o| check_cl_max(o, 2.2, 2.9)).items().is_empty());
        assert_eq!(
            run(|o| check_cl_max(o, 4.5, 2.9)).severity_of("AERO-CLMAX"),
            Some(Severity::High)
        );
        assert!(run(|o| check_cl_max(o, 2.4, 2.0)).has("AERO-CLMAX"));
    }

    #[test]
    fn vn_diagram_rules() {
        let good = VnFacts {
            vs_kt: 130.0,
            va_kt: 220.0,
            vc_kt: 280.0,
            vd_kt: 350.0,
            v_cruise_op_kt: 270.0,
            n_pos: 2.5,
            n_neg: -1.0,
            n_ult_pos: 3.75,
            mass_kg: 70_000.0,
        };
        assert!(run(|o| check_vn(o, &good)).items().is_empty());
        let bad = VnFacts {
            n_pos: 2.1,
            va_kt: 300.0,
            vc_kt: 280.0,
            ..good
        };
        let out = run(|o| check_vn(o, &bad));
        assert_eq!(out.severity_of("VN-SPEED-ORDER"), Some(Severity::High));
        assert_eq!(out.severity_of("VN-LOAD-FACTORS"), Some(Severity::High));
    }

    #[test]
    fn v_speed_rules() {
        let good = VSpeedFacts {
            v_stall_to: 60.0,
            v_stall_land: 62.0,
            v_mc: 58.0,
            v1: 70.0,
            vr: 72.0,
            v2: 76.0,
            v_app: 78.0,
            v_td: 72.0,
        };
        assert!(run(|o| check_v_speeds(o, &good)).items().is_empty());
        let out = run(|o| {
            check_v_speeds(
                o,
                &VSpeedFacts {
                    v1: 62.0,
                    vr: 64.0,
                    v2: 66.0,
                    ..good
                },
            )
        });
        assert_eq!(out.severity_of("VSPEED-MARGINS"), Some(Severity::High));
        let out = run(|o| check_v_speeds(o, &VSpeedFacts { v1: 80.0, ..good }));
        assert_eq!(out.severity_of("VSPEED-ORDER"), Some(Severity::High));
    }
}
