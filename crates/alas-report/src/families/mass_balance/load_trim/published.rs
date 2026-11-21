// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Published CG envelopes drawn on the sheet as a comparison, only where a
//! sourced reference exists. They are not model results: they stay out of the
//! gate, the margins and the operational envelope.
//!
//! Each corner is (mass kg, %MAC) as the reference prints it; the sheet
//! converts to balance index with its own MAC, LEMAC and index definition.
//! The citations are those of the matrix reference set for each aircraft.

use super::PublishedLine;

/// B747-400 forward edge: 8.4 %MAC for weights up to about 805,000 lb.
const B744_FORWARD: [(f64, f64); 2] = [(0.0, 8.4), (365_150.0, 8.4)];
const B744_FORWARD_SOURCE: &str = "Boeing 747-400 ACAP Rev F (Dec 2024), s.7.4.1";
const B744_FORWARD_REFERENCE: &str = "Boeing 747-400 Airplane Characteristics for Airport \
Planning, D6-58326-1 Rev F, Dec 2024, section 7.4.1, printed p7-9: forward edge of the \
landing-gear-loading envelope up to about 805,000 lb, planning envelope, not AFM";

/// ATR 72-600 take-off and landing forward limit: 10.0 %MAC up to
/// 12,850 kg, linear to 14.0 at 15,000 kg, constant to 18,000 kg, linear to
/// 20.8 at the maximum ramp weight 22,970 kg.
const ATR72_FORWARD: [(f64, f64); 5] = [
    (0.0, 10.0),
    (12_850.0, 10.0),
    (15_000.0, 14.0),
    (18_000.0, 14.0),
    (22_970.0, 20.8),
];
/// ATR 72-600 in-flight forward limit: 10.0 %MAC up to 18,000 kg, linear to
/// 17.9 at the maximum take-off weight 22,800 kg.
const ATR72_FLIGHT_FORWARD: [(f64, f64); 3] = [(0.0, 10.0), (18_000.0, 10.0), (22_800.0, 17.9)];
/// ATR 72-600 take-off and landing aft limit: 37.0 %MAC up to 22,970 kg (the
/// in-flight aft limit, 39 %MAC, is less restrictive and not drawn).
const ATR72_AFT: [(f64, f64); 2] = [(0.0, 37.0), (22_970.0, 37.0)];
const ATR72_SOURCE: &str = "ATR 72-600 technical specification, CG limits";
const ATR72_REFERENCE: &str = "ATR 72-600 technical specification, centre-of-gravity limits \
section (take-off and landing; flight); MAC 2.303 m, reference-chord leading edge 13.604 m aft \
of station 0 (ATR Weight and Balance Manual LIM.1 p.03, 15 Jan 2021). EASA TCDS EASA.A.084 \
Issue 14 (23 Feb 2026), items 14 and 16, refers the CG range and the MAC to the AFM and the \
Weight and Balance Manual and prints no envelope";

fn line(source: &str, reference: &str, forward: bool, points: &[(f64, f64)]) -> PublishedLine {
    PublishedLine {
        source: source.to_owned(),
        reference: reference.to_owned(),
        forward,
        points: points.to_vec(),
    }
}

/// The manufacturer planning envelope the preset registers (flight limits,
/// every table mass the source states a limit at), converted from the
/// manufacturer's MAC frame to the model's: a station is the same physical
/// point whichever chord and leading edge it is measured against.
fn planning_lines(preset: &str, model_lemac_m: f64, model_mac_m: f64) -> Vec<PublishedLine> {
    let Some(envelope) = alas_config::presets::get(preset)
        .ok()
        .and_then(|p| p.reference.planning_cg_envelope)
    else {
        return Vec::new();
    };
    let frame = envelope.mac_reference;
    let to_model = |pct: f64| {
        let x = frame.lemac_from_aircraft_nose_m + pct / 100.0 * frame.mean_aerodynamic_chord_m;
        (x - model_lemac_m) / model_mac_m * 100.0
    };
    let source = format!("{}, {}", envelope.source.document, envelope.source.location);
    let reference = format!(
        "{}, {}, {}: flight limits, planning envelope subordinate to the {}",
        envelope.source.document,
        envelope.source.revision,
        envelope.source.location,
        envelope.controlling_document,
    );
    let forward: Vec<(f64, f64)> = envelope
        .vertices()
        .iter()
        .map(|v| (v.mass_kg, to_model(v.flight.forward_pct_mac)))
        .collect();
    let aft: Vec<(f64, f64)> = envelope
        .vertices()
        .iter()
        .filter_map(|v| Some((v.mass_kg, to_model(v.flight.aft_pct_mac?))))
        .collect();
    vec![
        line(&source, &reference, true, &forward),
        line(&source, &reference, false, &aft),
    ]
}

/// The published envelope lines for the preset `preset`, if it has a
/// sourced reference; `model_lemac_m` and `model_mac_m` are the model's MAC
/// frame the lines are drawn in.
pub(super) fn published_for(
    preset: &str,
    model_lemac_m: f64,
    model_mac_m: f64,
) -> Vec<PublishedLine> {
    match preset {
        "B747-400" => vec![
            line(B744_FORWARD_SOURCE, B744_FORWARD_REFERENCE, true, &B744_FORWARD),
        ],
        "ATR72-600" => vec![
            line(ATR72_SOURCE, ATR72_REFERENCE, true, &ATR72_FORWARD),
            line(ATR72_SOURCE, ATR72_REFERENCE, true, &ATR72_FLIGHT_FORWARD),
            line(ATR72_SOURCE, ATR72_REFERENCE, false, &ATR72_AFT),
        ],
        other => planning_lines(other, model_lemac_m, model_mac_m),
    }
}
