// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent landing-gear anchors registered per preset.

use serde_json::{json, Value};

/// Return the independent landing-gear anchors registered for a preset.
///
/// These values are deliberately kept beside the model-derived output rather
/// than fed into the sizing call.  A source wheelbase/track is a published
/// geometry reference with its own definition; it does not supply the datum
/// needed to move `x_nlg` or `x_mlg`.  The topology counts are likewise
/// exported as evidence metadata so the parity harness can distinguish an
/// aircraft reference from a preliminary ALAS layout.
pub(super) fn source_gear_reference(name: &str) -> Value {
    match name {
        "A220-300" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_a220_acp_2025",
            "definition": "Airbus ACP nominal wheelbase from NLG axle to MLG axle; main-gear track",
            "wheelbase_m": 15.23238,
            "track_width_m": 6.731,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 38.68928,
            "longitudinal_stations_m": {"nlg": 3.401568, "mlg": [18.633948, 18.633948]},
            "station_source_definition": "Airbus A220 ACP DM BD500-A-J06-10-00AAA-030A-A Rev 2023-11-01, pp.150-156; dimensions originate at the geometric nose-tip drawing extension and are nominal (weight/CG can change wheelbase)",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 2,
            "mlg_wheels_per_strut": [2, 2],
            "main_wheels_total": 4,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum or attachment hardpoints not asserted",
        }),
        "A320-200" => json!({
            "status": "registered_independent_reference",
            "source_cite": "easa_tcds_a064_i12",
            "definition": "A320-family source wheelbase from NLG axle to MLG axle; main-gear track",
            "wheelbase_m": 12.64,
            "track_width_m": 7.59,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 37.57,
            "longitudinal_stations_m": {"nlg": 5.07, "mlg": [17.71, 17.71]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-004-A01; dimensions originate at the geometric nose-tip drawing extension",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 2,
            "mlg_wheels_per_strut": [2, 2],
            "main_wheels_total": 4,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        "A340-300" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_ac_a340_2025",
            "definition": "Airbus AC source wheelbase to the wing MLG bogie centre; wing-gear centreline track",
            "wheelbase_m": 25.375,
            "track_width_m": 10.684,
            "centerline_wheelbase_m": 26.372,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 63.66,
            "longitudinal_stations_m": {"nlg": 6.67, "mlg": [32.05, 32.05, 33.04]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-007-A01; AC 7-2-0 gives 25.375 m primary wheelbase and 26.372 m NLG-to-centreline wheelbase",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 3,
            "mlg_wheels_per_strut": [4, 4, 2],
            "main_wheels_total": 10,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        "A380-800" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_ac_a380_2025",
            "definition": "Airbus AC source wheelbase to the wing-gear axle reference; wing-gear centreline track and bogie topology; body-gear wheelbase is retained separately",
            "wheelbase_m": 28.61,
            "body_wheelbase_m": 31.88,
            "track_width_m": 14.34,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 72.73,
            "longitudinal_stations_m": {"nlg": 4.97, "mlg": [33.58, 33.58, 36.85, 36.85]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-001-A01; 28.61 m is NLG-to-WLG and 31.88 m is NLG-to-BLG",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 4,
            "mlg_wheels_per_strut": [4, 4, 6, 6],
            "main_wheels_total": 20,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        _ => json!({
            "status": "no_registered_independent_reference",
            "source_cite": Value::Null,
            "definition": "No source topology/position anchor is registered for this preset",
            "wheelbase_m": Value::Null,
            "track_width_m": Value::Null,
            "n_nlg_wheels": Value::Null,
            "n_mlg_struts": Value::Null,
            "mlg_wheels_per_strut": Value::Null,
            "main_wheels_total": Value::Null,
            "absolute_station_status": "unavailable",
        }),
    }
}
