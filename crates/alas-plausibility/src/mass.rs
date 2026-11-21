// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass, balance and structures plausibility rules.

use super::{Findings, RuleInfo, Severity};

pub(super) const W_CG_ENVELOPE: RuleInfo = RuleInfo {
    id: "CG-ENVELOPE",
    statement: "In every loading state the model hard CG constraints pass, ground reactions are admissible and the nose-gear load is 4-20 % of weight",
    source: "CS 25.23-25.27 (load distribution limits), CS 25.509; nose-gear share 5-20 % of weight (Raymer Sec. 11.3, Roskam Part IV)",
};
pub(super) const W_GEAR_MAIN_REACTION: RuleInfo = RuleInfo {
    id: "GEAR-MAIN-REACTION",
    statement: "The main-gear design static reaction is below the design weight (HIGH at or above it: the nose reaction would be negative) and leaves the nose gear at least 3 % of the weight (MEDIUM below)",
    source: "Static equilibrium; the 3 % MEDIUM threshold is a choice just below the lowest published nose share (3.7 %, 747-400 chart at 353 t; CG-ENVELOPE's 4 % band is the per-state check); published ground aft limits leave 3.7-8.7 % of the weight on the nose gear (Boeing 747-400 ACAP D6-58326-1 Rev F Fig. 7.4.1 and section 7.3; DC/MD-10 ACAP DAC-67803A section 7.3.2; Airbus A320/A340/A380 aircraft characteristics section 7-3-0), Raymer 6th ed. p. 343 (5 % class minimum)",
};
pub(super) const W_GEAR_AERO_AFT_BEHIND_GEAR: RuleInfo = RuleInfo {
    id: "GEAR-AERO-AFT-BEHIND-GEAR",
    statement: "The design aft CG the gear is sized at (aerodynamic aft limit or the takeoff state, whichever is further aft) lies ahead of the main-gear station (raw static nose reaction positive); HIGH when the raw moment balance gives a negative nose reaction: either the neutral point is too far aft (SM1, aerodynamic limit governs) or the takeoff loading CG is behind the gear (the finding says which). Gear sizing pulls the aerodynamic limit back, so this rule reads the unpulled value",
    source: "Static equilibrium about the main gear; published ground aft limits keep 3.7-8.7 % of the weight on the nose gear (Boeing 747-400 ACAP D6-58326-1 Rev F section 7.4, Airbus section 7-3-0); open fix SM1",
};
pub(super) const W_STATIC_MARGIN: RuleInfo = RuleInfo {
    id: "CG-STATIC-MARGIN",
    statement: "Static margin of every loading state, clean and against the critical (most-forward) neutral point, 5-30 % MAC usual (transports sit at 5-25 %); MEDIUM outside 3-35 %; HIGH is reported under the two rules below",
    source: "Raymer Sec. 16.2; Torenbeek Sec. 9; CS 25.173 stability",
};
pub(super) const W_STATIC_MARGIN_UNSTABLE: RuleInfo = RuleInfo {
    id: "CG-STATIC-MARGIN-UNSTABLE",
    statement: "No loading state has a negative static margin, clean or against the critical neutral point (HIGH: the model aircraft is statically unstable at that CG)",
    source: "CS 25.173 static longitudinal stability; Raymer Sec. 16.2",
};
pub(super) const W_STATIC_MARGIN_TOO_STABLE: RuleInfo = RuleInfo {
    id: "CG-STATIC-MARGIN-TOO-STABLE",
    statement: "No loading state has a static margin above 50 % MAC, clean or against the critical neutral point (HIGH: the neutral point or the CG is misplaced; transports sit at 5-25 %)",
    source: "Raymer Sec. 16.2; Torenbeek Sec. 9",
};
pub(super) const W_PUBLISHED_AFT_LIMIT: RuleInfo = RuleInfo {
    id: "CG-PUBLISHED-AFT-LIMIT",
    statement: "Where the manufacturer publishes a certified most-aft CG for the aircraft, the model critical neutral point lies aft of it (HIGH: the model aircraft has negative static margin at a CG that is certified); MEDIUM when the margin to it is below 5 % MAC",
    source: "Airbus A320 / A340 / A380 aircraft characteristics section 7-3-0 and A220 ACP (most-aft CG at maximum ramp weight, as tabulated in alas-config gear_load/published.rs); CS 25.173",
};
pub(super) const W_MASS_CLOSURE: RuleInfo = RuleInfo {
    id: "MASS-CLOSURE",
    statement: "OEW + payload + fuel does not exceed the takeoff mass (0.5 % tolerance) and is within 10 % below it (MEDIUM: tank- or payload-limited case); OEW/MTOW 0.35-0.65 usual (HIGH outside 0.2-0.85), fuel fraction 0.05-0.45 (HIGH outside 0.01-0.6)",
    source: "Mass conservation; Raymer Table 3.1-3.2 and Roskam Part I (transport empty-weight fraction 0.45-0.65)",
};
pub(super) const W_FREQUENCY_ORDER: RuleInfo = RuleInfo {
    id: "STRUCT-FREQUENCY",
    statement: "Wing bending frequencies are positive and strictly increasing; the first is 0.5-6 Hz (HIGH outside 0.2-15 Hz)",
    source: "Euler-Bernoulli beam modes; transport first wing bending 1-3 Hz (Wright & Cooper, Introduction to Aircraft Aeroelasticity, Ch. 4)",
};
pub(super) const W_DEFLECTION_SHAPE: RuleInfo = RuleInfo {
    id: "STRUCT-DEFLECTION",
    statement: "Static deflection is zero at the root, grows monotonically in magnitude to the tip, and the limit-load tip deflection is below 25 % of the semi-span (HIGH above 40 %)",
    source: "Cantilever statics; tip deflection at limit load of transport wings 10-20 % of semi-span (Megson, Aircraft Structures, Ch. 12)",
};

/// One loading state of the model CG assessment.
#[derive(Debug, Clone, PartialEq)]
pub struct CgState {
    /// State label.
    pub label: String,
    /// CG in % MAC.
    pub cg_pct_mac: f64,
    /// Static margin as a fraction of MAC.
    pub static_margin: f64,
    /// Static margin against the critical (most-forward) neutral point, fraction of MAC.
    pub critical_static_margin: f64,
    /// Nose-gear reaction over weight.
    pub nose_load_fraction: f64,
    /// Whether the two-group ground-reaction split is valid.
    pub ground_reactions_admissible: bool,
    /// Names of the violated hard constraints.
    pub violated: Vec<String>,
    /// Largest normalized exceedance among the violated hard constraints
    /// (violation over the constraint's own scale, dimensionless); zero when
    /// none is violated.
    pub worst_exceedance: f64,
}

/// Mass ledger of the analysed load case, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassFacts {
    /// Takeoff mass of the load case.
    pub takeoff_kg: f64,
    /// Operating empty mass.
    pub oew_kg: f64,
    /// Payload.
    pub payload_kg: f64,
    /// Fuel carried.
    pub fuel_kg: f64,
}

/// Wing structural results.
#[derive(Debug, Clone, PartialEq)]
pub struct StructFacts {
    /// Bending frequencies, Hz.
    pub frequencies_hz: Vec<f64>,
    /// Semi-span, m.
    pub semi_span_m: f64,
    /// Spanwise stations of the highest-load case, m.
    pub y_m: Vec<f64>,
    /// Deflection at those stations, m.
    pub deflection_m: Vec<f64>,
}

/// The main-gear design reaction against the weight it was sized at.
pub fn check_gear_reaction(
    out: &mut Findings,
    design_weight_kg: f64,
    main_reaction_kg: f64,
    raw_aft_nose_reaction_kg: f64,
    takeoff_state_governs: bool,
) {
    if raw_aft_nose_reaction_kg < 0.0 {
        out.add_measured(
            W_GEAR_AERO_AFT_BEHIND_GEAR.id,
            Severity::High,
            "",
            Some(-100.0 * raw_aft_nose_reaction_kg / design_weight_kg),
            format!(
                "raw nose reaction {:.0} kg ({:.1} % of the design weight) at the design aft CG, behind the main gear: {}",
                raw_aft_nose_reaction_kg,
                100.0 * raw_aft_nose_reaction_kg / design_weight_kg,
                if takeoff_state_governs {
                    "the takeoff-state loading CG governs (loading or mass-model CG)"
                } else {
                    "the aerodynamic aft limit governs (neutral point too far aft, SM1)"
                }
            ),
        );
    }
    if !(design_weight_kg.is_finite() && design_weight_kg > 0.0 && main_reaction_kg.is_finite()) {
        out.add(
            W_GEAR_MAIN_REACTION.id,
            Severity::High,
            format!(
                "main-gear design reaction {main_reaction_kg} kg at design weight {design_weight_kg} kg is not a finite load"
            ),
        );
        return;
    }
    let nose_share = 1.0 - main_reaction_kg / design_weight_kg;
    if nose_share <= 0.0 {
        out.add(
            W_GEAR_MAIN_REACTION.id,
            Severity::High,
            format!(
                "main-gear design reaction {:.0} kg is {:.1} % of the design weight {:.0} kg",
                main_reaction_kg,
                100.0 * main_reaction_kg / design_weight_kg,
                design_weight_kg
            ),
        );
    } else if nose_share < 0.03 {
        out.add(
            W_GEAR_MAIN_REACTION.id,
            Severity::Medium,
            format!(
                "nose gear carries {:.1} % of the design weight at the design aft CG",
                100.0 * nose_share
            ),
        );
    }
}

/// CG and static-margin rules over every loading state.
pub fn check_cg(out: &mut Findings, states: &[CgState]) {
    for s in states {
        if !s.violated.is_empty() {
            let mut kinds = s.violated.clone();
            kinds.sort();
            kinds.dedup();
            out.add_measured(
                W_CG_ENVELOPE.id,
                Severity::High,
                &kinds.join("+"),
                Some(s.worst_exceedance),
                format!(
                    "{}: violated {:?} (CG {:.1} % MAC, worst normalized exceedance {:.4})",
                    s.label, s.violated, s.cg_pct_mac, s.worst_exceedance
                ),
            );
        }
        if !s.ground_reactions_admissible {
            out.add_measured(
                W_CG_ENVELOPE.id,
                Severity::High,
                "GroundReactionsInadmissible",
                None,
                format!(
                    "{}: CG aft of the main gear, ground reactions not admissible",
                    s.label
                ),
            );
        } else if !(0.04..=0.20).contains(&s.nose_load_fraction) {
            out.add(
                W_CG_ENVELOPE.id,
                Severity::Medium,
                format!(
                    "{}: nose-gear load {:.1} % of weight",
                    s.label,
                    100.0 * s.nose_load_fraction
                ),
            );
        }
        // The clean rigid margin and the critical one (against the most
        // forward neutral point of the clean, high-lift and elastic
        // conditions, which the model gates on): the same bands apply to
        // both, so a clean value that is implausibly large is never hidden
        // behind the critical one.
        for (what, margin) in [
            ("static margin", s.static_margin),
            ("critical static margin", s.critical_static_margin),
        ] {
            let sm = 100.0 * margin;
            // A non-finite margin (a failed neutral-point condition) is HIGH, as before the split.
            let (rule, severity) = if !sm.is_finite() || sm < 0.0 {
                (W_STATIC_MARGIN_UNSTABLE.id, Some(Severity::High))
            } else if sm > 50.0 {
                (W_STATIC_MARGIN_TOO_STABLE.id, Some(Severity::High))
            } else {
                (W_STATIC_MARGIN.id, None)
            };
            let severity = if severity.is_some() {
                severity
            } else if !(3.0..=35.0).contains(&sm) {
                Some(Severity::Medium)
            } else if !(5.0..=30.0).contains(&sm) {
                Some(Severity::Low)
            } else {
                None
            };
            if let Some(severity) = severity {
                out.add(rule, severity, format!("{}: {what} {sm:.1} % MAC", s.label));
            }
        }
    }
}

/// Mass closure and mass-fraction rules.
pub fn check_mass(out: &mut Findings, m: &MassFacts) {
    let sum = m.oew_kg + m.payload_kg + m.fuel_kg;
    let miss = (sum - m.takeoff_kg) / m.takeoff_kg.max(1.0);
    if miss > 0.005 {
        out.add(
            W_MASS_CLOSURE.id,
            Severity::High,
            format!(
                "OEW {:.0} + payload {:.0} + fuel {:.0} = {sum:.0} kg exceeds the takeoff mass {:.0} kg by {:.2} %",
                m.oew_kg, m.payload_kg, m.fuel_kg, m.takeoff_kg, 100.0 * miss
            ),
        );
    } else if miss < -0.10 {
        out.add(
            W_MASS_CLOSURE.id,
            Severity::Medium,
            format!(
                "the load case sums to {sum:.0} kg, {:.1} % below the takeoff mass {:.0} kg",
                -100.0 * miss,
                m.takeoff_kg
            ),
        );
    }
    let oew_fraction = m.oew_kg / m.takeoff_kg.max(1.0);
    let fuel_fraction = m.fuel_kg / m.takeoff_kg.max(1.0);
    let band = |v: f64, usual: (f64, f64), high: (f64, f64)| {
        if v < high.0 || v > high.1 {
            Some(Severity::High)
        } else if v < usual.0 || v > usual.1 {
            Some(Severity::Medium)
        } else {
            None
        }
    };
    if let Some(sev) = band(oew_fraction, (0.35, 0.65), (0.2, 0.85)) {
        out.add(
            W_MASS_CLOSURE.id,
            sev,
            format!("OEW/MTOW {oew_fraction:.3}"),
        );
    }
    if let Some(sev) = band(fuel_fraction, (0.05, 0.45), (0.01, 0.6)) {
        out.add(
            W_MASS_CLOSURE.id,
            sev,
            format!("fuel fraction {fuel_fraction:.3}"),
        );
    }
}

/// Modal frequencies and deflection shape.
pub fn check_structure(out: &mut Findings, s: &StructFacts) {
    if let Some(&f1) = s.frequencies_hz.first() {
        if !f1.is_finite() || !(0.2..=15.0).contains(&f1) {
            out.add(
                W_FREQUENCY_ORDER.id,
                Severity::High,
                format!("first bending frequency {f1:.2} Hz"),
            );
        } else if !(0.5..=6.0).contains(&f1) {
            out.add(
                W_FREQUENCY_ORDER.id,
                Severity::Medium,
                format!("first bending frequency {f1:.2} Hz"),
            );
        }
    }
    if s.frequencies_hz.windows(2).any(|w| w[1] <= w[0])
        || s.frequencies_hz.iter().any(|f| *f <= 0.0)
    {
        out.add(
            W_FREQUENCY_ORDER.id,
            Severity::High,
            format!(
                "bending frequencies not positive and increasing: {:?}",
                s.frequencies_hz
            ),
        );
    }
    if s.deflection_m.len() >= 2 && s.deflection_m.len() == s.y_m.len() {
        let mag: Vec<f64> = s.deflection_m.iter().map(|d| d.abs()).collect();
        let tip = mag.last().copied().unwrap_or(0.0);
        if mag[0] > 0.01 * tip.max(1e-9) {
            out.add(
                W_DEFLECTION_SHAPE.id,
                Severity::High,
                format!(
                    "root deflection {:.3} m is not zero (tip {tip:.3} m)",
                    mag[0]
                ),
            );
        }
        if mag.windows(2).any(|w| w[1] + 1e-6 * tip < w[0]) {
            out.add(
                W_DEFLECTION_SHAPE.id,
                Severity::High,
                "deflection magnitude decreases along the span".into(),
            );
        }
        let ratio = tip / s.semi_span_m.max(1e-9);
        if ratio > 0.40 {
            out.add(
                W_DEFLECTION_SHAPE.id,
                Severity::High,
                format!("tip deflection {:.0} % of semi-span", 100.0 * ratio),
            );
        } else if ratio > 0.25 {
            out.add(
                W_DEFLECTION_SHAPE.id,
                Severity::Medium,
                format!("tip deflection {:.0} % of semi-span", 100.0 * ratio),
            );
        }
    }
}
/// The model critical neutral point against the most-aft CG the manufacturer
/// certifies (`published_aft_pct_mac`, % MAC in the source's frame; the model
/// MAC frame is the preset's published reference wing). A certified CG cannot
/// lie at or behind a real neutral point, so a critical neutral point at or
/// ahead of it is a model defect.
pub fn check_published_aft_limit(
    out: &mut Findings,
    critical_np_pct_mac: f64,
    published_aft_pct_mac: Option<f64>,
) {
    let Some(published) = published_aft_pct_mac else {
        return;
    };
    let margin = critical_np_pct_mac - published;
    if margin <= 0.0 {
        out.add(
            W_PUBLISHED_AFT_LIMIT.id,
            Severity::High,
            format!(
                "critical neutral point {critical_np_pct_mac:.1} % MAC is not aft of the published certified aft CG {published:.1} % MAC"
            ),
        );
    } else if margin < 5.0 {
        out.add(
            W_PUBLISHED_AFT_LIMIT.id,
            Severity::Medium,
            format!(
                "critical neutral point {critical_np_pct_mac:.1} % MAC is only {margin:.1} % MAC aft of the published certified aft CG {published:.1} % MAC"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> CgState {
        CgState {
            label: "TOW".into(),
            cg_pct_mac: 25.0,
            static_margin: 0.12,
            critical_static_margin: 0.09,
            nose_load_fraction: 0.09,
            ground_reactions_admissible: true,
            violated: Vec::new(),
            worst_exceedance: 0.0,
        }
    }

    fn run(f: impl FnOnce(&mut Findings)) -> Findings {
        let mut out = Findings::new("synthetic");
        f(&mut out);
        out
    }

    #[test]
    fn gear_reaction_rules() {
        // Boeing 747-400 ACAP: 4 x 92,800 kg of 397,800 kg on the mains.
        let ok = run(|o| check_gear_reaction(o, 397_800.0, 371_200.0, 5_000.0, false));
        assert!(ok.items().is_empty());
        // A main reaction above the weight means a negative nose reaction.
        let above = run(|o| check_gear_reaction(o, 396_893.0, 424_741.0, -27_848.0, false));
        assert_eq!(
            above.severity_of("GEAR-MAIN-REACTION"),
            Some(Severity::High)
        );
        assert_eq!(
            above.severity_of("GEAR-AERO-AFT-BEHIND-GEAR"),
            Some(Severity::High)
        );
        // The floor's output alone (main below W) must not hide the raw value.
        let floored = run(|o| check_gear_reaction(o, 396_893.0, 370_000.0, -27_848.0, false));
        assert_eq!(
            floored.severity_of("GEAR-AERO-AFT-BEHIND-GEAR"),
            Some(Severity::High)
        );
        assert_eq!(floored.severity_of("GEAR-MAIN-REACTION"), None);
        let equal = run(|o| check_gear_reaction(o, 1000.0, 1000.0, 0.0, false));
        assert_eq!(
            equal.severity_of("GEAR-MAIN-REACTION"),
            Some(Severity::High)
        );
        let thin = run(|o| check_gear_reaction(o, 1000.0, 980.0, 20.0, false));
        assert_eq!(
            thin.severity_of("GEAR-MAIN-REACTION"),
            Some(Severity::Medium)
        );
        let bad = run(|o| check_gear_reaction(o, 1000.0, f64::NAN, 20.0, false));
        assert_eq!(bad.severity_of("GEAR-MAIN-REACTION"), Some(Severity::High));
    }

    #[test]
    fn cg_rules() {
        assert!(run(|o| check_cg(o, &[state()])).items().is_empty());
        let bad = CgState {
            static_margin: -0.02,
            ground_reactions_admissible: false,
            violated: vec!["aft".into()],
            ..state()
        };
        let out = run(|o| check_cg(o, &[bad]));
        assert_eq!(out.severity_of("CG-ENVELOPE"), Some(Severity::High));
        assert_eq!(
            out.severity_of("CG-STATIC-MARGIN-UNSTABLE"),
            Some(Severity::High)
        );
        let broken = CgState {
            critical_static_margin: f64::NAN,
            ..state()
        };
        assert_eq!(
            run(|o| check_cg(o, &[broken])).severity_of("CG-STATIC-MARGIN-UNSTABLE"),
            Some(Severity::High)
        );
        let stiff = CgState {
            static_margin: 0.52,
            ..state()
        };
        let out = run(|o| check_cg(o, &[stiff]));
        assert_eq!(
            out.severity_of("CG-STATIC-MARGIN-TOO-STABLE"),
            Some(Severity::High)
        );
        assert_eq!(out.severity_of("CG-STATIC-MARGIN-UNSTABLE"), None);
        let light = CgState {
            nose_load_fraction: 0.02,
            ..state()
        };
        assert_eq!(
            run(|o| check_cg(o, &[light])).severity_of("CG-ENVELOPE"),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn mass_rules() {
        let good = MassFacts {
            takeoff_kg: 78_000.0,
            oew_kg: 42_000.0,
            payload_kg: 17_000.0,
            fuel_kg: 19_000.0,
        };
        assert!(run(|o| check_mass(o, &good)).items().is_empty());
        let open = MassFacts {
            fuel_kg: 24_000.0,
            ..good
        };
        assert_eq!(
            run(|o| check_mass(o, &open)).severity_of("MASS-CLOSURE"),
            Some(Severity::High)
        );
        let heavy = MassFacts {
            takeoff_kg: 100_000.0,
            oew_kg: 90_000.0,
            payload_kg: 5_000.0,
            fuel_kg: 5_000.0,
        };
        assert_eq!(
            run(|o| check_mass(o, &heavy)).severity_of("MASS-CLOSURE"),
            Some(Severity::High)
        );
    }

    #[test]
    fn structure_rules() {
        let y: Vec<f64> = (0..11).map(|i| i as f64).collect();
        let d: Vec<f64> = y.iter().map(|y| 0.012 * y * y).collect();
        let good = StructFacts {
            frequencies_hz: vec![1.8, 5.2, 11.0],
            semi_span_m: 17.0,
            y_m: y.clone(),
            deflection_m: d.clone(),
        };
        assert!(run(|o| check_structure(o, &good)).items().is_empty());
        let mut kink = d;
        kink[6] = 0.1;
        let bad = StructFacts {
            frequencies_hz: vec![3.0, 2.0],
            deflection_m: kink,
            ..good.clone()
        };
        let out = run(|o| check_structure(o, &bad));
        assert_eq!(out.severity_of("STRUCT-FREQUENCY"), Some(Severity::High));
        assert_eq!(out.severity_of("STRUCT-DEFLECTION"), Some(Severity::High));
    }

    #[test]
    fn a_critical_neutral_point_ahead_of_a_published_aft_cg_is_high() {
        let ahead = run(|o| check_published_aft_limit(o, 41.7, Some(43.0)));
        assert_eq!(
            ahead.severity_of("CG-PUBLISHED-AFT-LIMIT"),
            Some(Severity::High)
        );
        let close = run(|o| check_published_aft_limit(o, 46.0, Some(43.0)));
        assert_eq!(
            close.severity_of("CG-PUBLISHED-AFT-LIMIT"),
            Some(Severity::Medium)
        );
        let clear = run(|o| check_published_aft_limit(o, 52.0, Some(43.0)));
        assert!(clear.items().is_empty());
        // Nothing published: nothing to judge.
        assert!(run(|o| check_published_aft_limit(o, 10.0, None))
            .items()
            .is_empty());
    }
}
