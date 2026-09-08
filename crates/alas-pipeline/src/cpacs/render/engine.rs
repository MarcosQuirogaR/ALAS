// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Standard CPACS engine geometry and take-off analysis fields.

use std::fmt::Write as FmtWrite;

use alas_config::{ActiveEngineModel, AlasConfig};

use super::{number, write_number_element, write_text, ENGINE_UID};

pub(super) fn write_engine_definition(xml: &mut String, config: &AlasConfig) {
    let engine = &config.geometry.engine;
    let nacelle_length_m = engine.nacelle_length_m();
    let nacelle_diameter_m = 2.0 * engine.radius_scale_m;
    let _ = writeln!(xml, "    <engines>");
    let _ = writeln!(xml, "      <engine uID=\"{ENGINE_UID}\">");
    write_text(xml, 8, "name", &engine.engine_name);
    let typed = engine.active_model().ok();
    match typed {
        Some(ActiveEngineModel::Turboprop(spec)) => write_text(
            xml,
            8,
            "description",
            &format!(
                "ALAS turboprop: {} driving {}; take-off shaft power {} kW.",
                engine.engine_name,
                spec.propeller_model,
                number(spec.takeoff_shaft_power_kw),
            ),
        ),
        Some(ActiveEngineModel::Turbofan(spec)) => write_text(
            xml,
            8,
            "description",
            &format!(
                "ALAS conceptual turbofan: take-off thrust {} N, BPR {}, OPR {}, FPR {}.",
                number(spec.rated_thrust_kn * 1_000.0),
                number(spec.bypass_ratio),
                number(spec.overall_pressure_ratio),
                number(spec.fan_pressure_ratio),
            ),
        ),
        None => write_text(
            xml,
            8,
            "description",
            "ALAS propulsion binding unavailable.",
        ),
    }
    if !engine.nacelle_profile.is_empty() {
        let _ = writeln!(xml, "        <geometry>");
        write_number_element(xml, 10, "length", nacelle_length_m);
        write_number_element(xml, 10, "diameter", nacelle_diameter_m);
        let _ = writeln!(xml, "        </geometry>");
    }
    if let Some(ActiveEngineModel::Turbofan(spec)) = typed {
        let _ = writeln!(xml, "        <analysis>");
        write_number_element(xml, 10, "thrust00", spec.rated_thrust_kn * 1_000.0);
        write_number_element(xml, 10, "fpr00", spec.fan_pressure_ratio);
        write_number_element(xml, 10, "bpr00", spec.bypass_ratio);
        write_number_element(xml, 10, "opr00", spec.overall_pressure_ratio);
        let _ = writeln!(xml, "        </analysis>");
    }
    let _ = writeln!(xml, "      </engine>");
    let _ = writeln!(xml, "    </engines>");
}

/// Emit quantities that CPACS 3.5's jet-engine analysis fields cannot encode.
pub(super) fn write_toolspecific_propulsion(xml: &mut String, config: &AlasConfig) {
    let Ok(ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model() else {
        return;
    };
    let _ = writeln!(xml, "  <toolspecific>");
    let _ = writeln!(xml, "    <tool>");
    write_text(xml, 6, "name", "ALAS propulsion");
    write_text(xml, 6, "version", "1.0");
    let _ = writeln!(
        xml,
        "      <propulsion xmlns=\"https://alas.dev/cpacs/propulsion/1.0\">"
    );
    write_text(xml, 8, "technology", "turboprop");
    write_text(xml, 8, "engineUID", ENGINE_UID);
    write_text(xml, 8, "engineModel", &config.geometry.engine.engine_name);
    write_text(xml, 8, "propellerModel", &spec.propeller_model);
    write_number_element(xml, 8, "takeoffShaftPowerKW", spec.takeoff_shaft_power_kw);
    write_number_element(
        xml,
        8,
        "maximumReserveShaftPowerKW",
        spec.maximum_reserve_shaft_power_kw,
    );
    write_number_element(
        xml,
        8,
        "maximumContinuousShaftPowerKW",
        spec.maximum_continuous_shaft_power_kw,
    );
    write_number_element(
        xml,
        8,
        "maximumClimbShaftPowerKW",
        spec.maximum_climb_shaft_power_kw,
    );
    write_number_element(
        xml,
        8,
        "maximumCruiseShaftPowerKW",
        spec.maximum_cruise_shaft_power_kw,
    );
    write_number_element(
        xml,
        8,
        "maximumCruiseFuelFlowKgH",
        spec.maximum_cruise_fuel_flow_kg_h,
    );
    write_number_element(xml, 8, "propellerDiameterM", spec.propeller_diameter_m);
    write_number_element(
        xml,
        8,
        "governedPropellerSpeedRPM",
        spec.governed_propeller_speed_rpm,
    );
    write_number_element(xml, 8, "reductionRatio", spec.reduction_ratio);
    let _ = writeln!(xml, "      </propulsion>");
    let _ = writeln!(xml, "    </tool>");
    let _ = writeln!(xml, "  </toolspecific>");
}
