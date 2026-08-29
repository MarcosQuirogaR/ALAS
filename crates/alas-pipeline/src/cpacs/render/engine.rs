// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Standard CPACS engine geometry and take-off analysis fields.

use std::fmt::Write as FmtWrite;

use alas_config::AlasConfig;

use super::{number, write_number_element, write_text, ENGINE_UID};

pub(super) fn write_engine_definition(xml: &mut String, config: &AlasConfig) {
    let engine = &config.geometry.engine;
    let nacelle_length_m = engine.nacelle_length_m();
    let nacelle_diameter_m = 2.0 * engine.radius_scale_m;
    let _ = writeln!(xml, "    <engines>");
    let _ = writeln!(xml, "      <engine uID=\"{ENGINE_UID}\">");
    write_text(xml, 8, "name", &engine.engine_name);
    write_text(
        xml,
        8,
        "description",
        &format!(
            "ALAS conceptual turbofan: take-off thrust {} N, BPR {}, OPR {}, FPR {}.",
            number(engine.thrust_kn * 1_000.0),
            number(engine.bypass_ratio),
            number(engine.overall_pressure_ratio),
            number(engine.fan_pressure_ratio),
        ),
    );
    if !engine.nacelle_profile.is_empty() {
        let _ = writeln!(xml, "        <geometry>");
        write_number_element(xml, 10, "length", nacelle_length_m);
        write_number_element(xml, 10, "diameter", nacelle_diameter_m);
        let _ = writeln!(xml, "        </geometry>");
    }
    let _ = writeln!(xml, "        <analysis>");
    write_number_element(xml, 10, "thrust00", engine.thrust_kn * 1_000.0);
    write_number_element(xml, 10, "fpr00", engine.fan_pressure_ratio);
    write_number_element(xml, 10, "bpr00", engine.bypass_ratio);
    write_number_element(xml, 10, "opr00", engine.overall_pressure_ratio);
    let _ = writeln!(xml, "        </analysis>");
    let _ = writeln!(xml, "      </engine>");
    let _ = writeln!(xml, "    </engines>");
}
