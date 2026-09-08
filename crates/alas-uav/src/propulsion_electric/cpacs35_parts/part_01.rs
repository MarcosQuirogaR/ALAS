// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fmt;

use crate::procurement::{ProcurementEstimate, ProcurementLine};

use super::{
    CataloguePowertrainSelection, ElectricMissionResult, ElectricPropulsionResult, ElectricalCheck,
};

/// Namespace used by the ALAS UAV electrical CPACS 3.5 extension.
pub const UAV_ELECTRICAL_NAMESPACE: &str = "https://alas.local/cpacs/uav-electrical/1";

/// Version of the UAV electrical extension payload.
pub const UAV_ELECTRICAL_EXTENSION_VERSION: &str = "1";

/// Electrical and optional procurement data retained with a CPACS aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct UavCpacs35ElectricalData {
    /// Discrete catalogue selection used for the calculation.
    pub selection: CataloguePowertrainSelection,
    /// Coupled electrical operating point to retain.
    pub operating_point: ElectricPropulsionResult,
    /// Optional solved multi-phase mission using the same selected powertrain.
    pub mission: Option<ElectricMissionResult>,
    /// Dated procurement estimate when price evidence is available.
    pub procurement: Option<ProcurementEstimate>,
}

/// Why a CPACS electrical extension could not be rendered or injected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpacsUavExtensionError {
    /// A value required by XML Schema double is non-finite.
    NonFinite {
        /// Field whose value cannot be represented as an XML Schema double.
        field: &'static str,
    },
    /// The supplied document is not an injectable CPACS root document.
    InvalidDocument(String),
}

impl fmt::Display for CpacsUavExtensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { field } => {
                write!(formatter, "UAV CPACS extension has a non-finite {field}")
            }
            Self::InvalidDocument(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CpacsUavExtensionError {}

/// Render the CPACS 3.5 `toolspecific` payload without a surrounding document.
pub fn render_uav_cpacs35_toolspecific(
    data: &UavCpacs35ElectricalData,
) -> Result<String, CpacsUavExtensionError> {
    validate_data(data)?;
    let mut xml = String::with_capacity(4_096);
    xml.push_str("  <toolspecific>\n");
    xml.push_str("    <tool>\n");
    xml.push_str("      <name>ALAS UAV electrical propulsion</name>\n");
    xml.push_str("      <version>");
    xml.push_str(UAV_ELECTRICAL_EXTENSION_VERSION);
    xml.push_str("</version>\n");
    xml.push_str("      <uavElectrical xmlns=\"");
    xml.push_str(UAV_ELECTRICAL_NAMESPACE);
    xml.push_str("\">\n");
    write_text(
        &mut xml,
        8,
        "schemaVersion",
        UAV_ELECTRICAL_EXTENSION_VERSION,
    );
    write_selection(&mut xml, data);
    write_operating_point(&mut xml, &data.operating_point);
    if let Some(mission) = &data.mission {
        write_mission(&mut xml, mission);
    }
    if !data.operating_point.checks.is_empty() {
        xml.push_str("        <checks>\n");
        for check in &data.operating_point.checks {
            write_check(&mut xml, check);
        }
        xml.push_str("        </checks>\n");
    }
    if !data.operating_point.assumptions.is_empty() {
        xml.push_str("        <assumptions>\n");
        for assumption in &data.operating_point.assumptions {
            write_text(&mut xml, 10, "assumption", assumption);
        }
        xml.push_str("        </assumptions>\n");
    }
    if let Some(procurement) = &data.procurement {
        write_procurement(&mut xml, procurement);
    }
    xml.push_str("      </uavElectrical>\n");
    xml.push_str("    </tool>\n");
    xml.push_str("  </toolspecific>\n");
    Ok(xml)
}

/// Inject a rendered extension and its schema location into a CPACS 3.5 document.
///
/// `toolspecific_schema_location` must name a file or URL available to the
/// downstream XML Schema validator.  Existing tool-specific payloads are
/// rejected so callers do not accidentally overwrite another tool's data.
pub fn enrich_cpacs35_with_uav_electrical(
    document: &str,
    toolspecific_schema_location: &str,
    data: &UavCpacs35ElectricalData,
) -> Result<String, CpacsUavExtensionError> {
    if toolspecific_schema_location.trim().is_empty() {
        return Err(CpacsUavExtensionError::InvalidDocument(
            "a CPACS toolspecific schema location is required".to_owned(),
        ));
    }
    if document.contains("<toolspecific") {
        return Err(CpacsUavExtensionError::InvalidDocument(
            "CPACS document already contains a toolspecific payload".to_owned(),
        ));
    }
    let root_start = cpacs_root_start(document).ok_or_else(|| {
        CpacsUavExtensionError::InvalidDocument("CPACS document has no <cpacs> root".to_owned())
    })?;
    let root_end = document[root_start..]
        .find('>')
        .map(|offset| root_start + offset)
        .ok_or_else(|| {
            CpacsUavExtensionError::InvalidDocument("CPACS root tag is not closed".to_owned())
        })?;
    let root_tag = &document[root_start..=root_end];
    if root_tag.contains("xsi:schemaLocation=") {
        return Err(CpacsUavExtensionError::InvalidDocument(
            "CPACS document already declares xsi:schemaLocation".to_owned(),
        ));
    }
    let closing = document.rfind("</cpacs>").ok_or_else(|| {
        CpacsUavExtensionError::InvalidDocument(
            "CPACS document has no </cpacs> close tag".to_owned(),
        )
    })?;
    let toolspecific = render_uav_cpacs35_toolspecific(data)?;
    let mut enriched = String::with_capacity(document.len() + toolspecific.len() + 160);
    enriched.push_str(&document[..root_end]);
    if !root_tag.contains("xmlns:xsi=") {
        enriched.push_str(" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"");
    }
    enriched.push_str(" xsi:schemaLocation=\"");
    escape_xml_into(&mut enriched, UAV_ELECTRICAL_NAMESPACE);
    enriched.push(' ');
    escape_xml_into(&mut enriched, toolspecific_schema_location);
    enriched.push('"');
    enriched.push_str(&document[root_end..closing]);
    enriched.push_str(&toolspecific);
    enriched.push_str(&document[closing..]);
    Ok(enriched)
}

fn validate_data(data: &UavCpacs35ElectricalData) -> Result<(), CpacsUavExtensionError> {
    let point = &data.operating_point;
    for (field, value) in [
        ("speed", point.condition.speed_m_s),
        ("density", point.condition.air_density_kg_m3),
        ("throttle", point.condition.throttle),
        ("RPM", point.per_motor.rpm),
        ("per-motor thrust", point.per_motor.thrust_n),
        ("per-motor current", point.per_motor.motor_current_a),
        ("per-motor shaft power", point.per_motor.shaft_power_w),
        (
            "per-motor electrical power",
            point.per_motor.motor_electrical_power_w,
        ),
        ("battery voltage", point.battery_terminal_voltage_v),
        ("battery current", point.battery_current_a),
        ("battery power", point.battery_power_w),
        ("total thrust", point.total_thrust_n),
    ] {
        if !value.is_finite() {
            return Err(CpacsUavExtensionError::NonFinite { field });
        }
    }
    if data.selection.motor_count == 0 {
        return Err(CpacsUavExtensionError::InvalidDocument(
            "CPACS electrical selection must have at least one motor".to_owned(),
        ));
    }
    for check in &point.checks {
        let values = match check {
            ElectricalCheck::CurrentOverload {
                actual_a, limit_a, ..
            }
            | ElectricalCheck::PowerOverload {
                actual_w: actual_a,
                limit_w: limit_a,
                ..
            } => [*actual_a, *limit_a],
            ElectricalCheck::Unverified { .. } => continue,
        };
        if values.iter().any(|value| !value.is_finite()) {
            return Err(CpacsUavExtensionError::NonFinite {
                field: "electrical check value",
            });
        }
    }
    if let Some(mission) = &data.mission {
        if mission.phases.is_empty() {
            return Err(CpacsUavExtensionError::InvalidDocument(
                "CPACS electrical mission must contain at least one phase".to_owned(),
            ));
        }
        for (field, value) in [
            ("mission total duration", mission.total_duration_s),
            ("mission total distance", mission.total_distance_m),
            ("mission propulsion energy", mission.propulsion_energy_wh),
            (
                "mission maximum battery current",
                mission.maximum_battery_current_a,
            ),
            (
                "mission maximum battery power",
                mission.maximum_battery_power_w,
            ),
        ] {
            if !value.is_finite() {
                return Err(CpacsUavExtensionError::NonFinite { field });
            }
        }
        for phase in &mission.phases {
            if phase.phase.name.trim().is_empty()
                || phase.phase.duration_s <= 0.0
                || phase.phase.condition.speed_m_s <= 0.0
                || phase.phase.condition.air_density_kg_m3 <= 0.0
                || !(0.0..=1.0).contains(&phase.phase.condition.throttle)
            {
                return Err(CpacsUavExtensionError::InvalidDocument(
                    "CPACS electrical mission phase has an invalid steady-flight condition"
                        .to_owned(),
                ));
            }
            for (field, value) in [
                ("mission duration", phase.phase.duration_s),
                ("mission speed", phase.phase.condition.speed_m_s),
                ("mission density", phase.phase.condition.air_density_kg_m3),
                ("mission throttle", phase.phase.condition.throttle),
                ("mission RPM", phase.propulsion.per_motor.rpm),
                (
                    "mission per-motor thrust",
                    phase.propulsion.per_motor.thrust_n,
                ),
                (
                    "mission per-motor current",
                    phase.propulsion.per_motor.motor_current_a,
                ),
                (
                    "mission per-motor electrical power",
                    phase.propulsion.per_motor.motor_electrical_power_w,
                ),
                ("mission thrust", phase.propulsion.total_thrust_n),
                (
                    "mission battery current",
                    phase.propulsion.battery_current_a,
                ),
                ("mission battery power", phase.propulsion.battery_power_w),
                ("mission distance", phase.distance_m),
                ("mission propulsion energy", phase.propulsion_energy_wh),
            ] {
                if !value.is_finite() {
                    return Err(CpacsUavExtensionError::NonFinite { field });
                }
            }
        }
    }
    Ok(())
}

fn cpacs_root_start(document: &str) -> Option<usize> {
    let leading = document.len().checked_sub(document.trim_start().len())?;
    let without_leading = &document[leading..];
    let after_declaration = if without_leading.starts_with("<?xml") {
        let declaration_end = without_leading.find("?>")? + 2;
        let remaining = &without_leading[declaration_end..];
        declaration_end + remaining.len().checked_sub(remaining.trim_start().len())?
    } else {
        0
    };
    let root_start = leading + after_declaration;
    let after_name = document[root_start..].strip_prefix("<cpacs")?;
    let next = after_name.chars().next()?;
    (next == '>' || next.is_ascii_whitespace()).then_some(root_start)
}

fn write_selection(xml: &mut String, data: &UavCpacs35ElectricalData) {
    let selection = &data.selection;
    xml.push_str("        <selection>\n");
    write_text(xml, 10, "batteryId", &selection.battery_id);
    write_text(xml, 10, "motorId", &selection.motor_id);
    write_text(xml, 10, "escId", &selection.esc_id);
    write_text(xml, 10, "propellerId", &selection.propeller_id);
    write_number(xml, 10, "motorCount", f64::from(selection.motor_count));
    xml.push_str("        </selection>\n");
}

fn write_operating_point(xml: &mut String, point: &ElectricPropulsionResult) {
    xml.push_str("        <operatingPoint>\n");
    write_number(xml, 10, "speedMps", point.condition.speed_m_s);
    write_number(xml, 10, "densityKgM3", point.condition.air_density_kg_m3);
    write_number(xml, 10, "throttle", point.condition.throttle);
    write_number(xml, 10, "rpm", point.per_motor.rpm);
    write_number(xml, 10, "perMotorThrustN", point.per_motor.thrust_n);
    write_number(
        xml,
        10,
        "perMotorShaftPowerW",
        point.per_motor.shaft_power_w,
    );
    write_number(xml, 10, "perMotorCurrentA", point.per_motor.motor_current_a);
    write_number(
        xml,
        10,
        "perMotorElectricalPowerW",
        point.per_motor.motor_electrical_power_w,
    );
    write_number(xml, 10, "batteryVoltageV", point.battery_terminal_voltage_v);
    write_number(xml, 10, "batteryCurrentA", point.battery_current_a);
    write_number(xml, 10, "batteryPowerW", point.battery_power_w);
    write_number(xml, 10, "totalThrustN", point.total_thrust_n);
    xml.push_str("        </operatingPoint>\n");
}

fn write_mission(xml: &mut String, mission: &ElectricMissionResult) {
    xml.push_str("        <mission>\n");
    write_text(xml, 10, "evidence", &mission.evidence);
    write_number(xml, 10, "totalDurationS", mission.total_duration_s);
    write_number(xml, 10, "totalDistanceM", mission.total_distance_m);
    write_number(xml, 10, "propulsionEnergyWh", mission.propulsion_energy_wh);
    write_number(
        xml,
        10,
        "maximumBatteryCurrentA",
        mission.maximum_battery_current_a,
    );
    write_number(
        xml,
        10,
        "maximumBatteryPowerW",
        mission.maximum_battery_power_w,
    );
    for phase in &mission.phases {
        xml.push_str("          <phase>\n");
        write_text(xml, 12, "name", &phase.phase.name);
        write_number(xml, 12, "durationS", phase.phase.duration_s);
        write_number(xml, 12, "speedMps", phase.phase.condition.speed_m_s);
        write_number(
            xml,
            12,
            "densityKgM3",
            phase.phase.condition.air_density_kg_m3,
        );
        write_number(xml, 12, "throttle", phase.phase.condition.throttle);
        write_number(xml, 12, "totalThrustN", phase.propulsion.total_thrust_n);
        write_number(
            xml,
            12,
            "perMotorCurrentA",
            phase.propulsion.per_motor.motor_current_a,
        );
        write_number(
            xml,
            12,
            "perMotorElectricalPowerW",
            phase.propulsion.per_motor.motor_electrical_power_w,
        );
        write_number(
            xml,
            12,
            "batteryCurrentA",
            phase.propulsion.battery_current_a,
        );
        write_number(xml, 12, "batteryPowerW", phase.propulsion.battery_power_w);
        write_number(xml, 12, "distanceM", phase.distance_m);
        write_number(xml, 12, "propulsionEnergyWh", phase.propulsion_energy_wh);
        xml.push_str("          </phase>\n");
    }
    xml.push_str("        </mission>\n");
}

fn write_check(xml: &mut String, check: &ElectricalCheck) {
    match check {
        ElectricalCheck::CurrentOverload {
            component_id,
            actual_a,
            limit_a,
        } => {
            xml.push_str("          <check kind=\"current_overload\">\n");
            write_text(xml, 12, "componentId", component_id);
            write_number(xml, 12, "actual", *actual_a);
            write_number(xml, 12, "limit", *limit_a);
            xml.push_str("          </check>\n");
        }
        ElectricalCheck::PowerOverload {
            component_id,
            actual_w,
            limit_w,
        } => {
            xml.push_str("          <check kind=\"power_overload\">\n");
            write_text(xml, 12, "componentId", component_id);
            write_number(xml, 12, "actual", *actual_w);
            write_number(xml, 12, "limit", *limit_w);
            xml.push_str("          </check>\n");
        }
        ElectricalCheck::Unverified {
            component_id,
            field,
        } => {
            xml.push_str("          <check kind=\"unverified\">\n");
            write_text(xml, 12, "componentId", component_id);
            write_text(xml, 12, "field", field);
            xml.push_str("          </check>\n");
        }
    }
}

fn write_procurement(xml: &mut String, procurement: &ProcurementEstimate) {
    xml.push_str("        <procurement complete=\"");
    xml.push_str(if procurement.complete {
        "true"
    } else {
        "false"
    });
    xml.push_str("\">\n");
    for line in &procurement.lines {
        write_procurement_line(xml, line);
    }
    for subtotal in &procurement.subtotals {
        xml.push_str("          <subtotal currency=\"");
        xml.push_str(subtotal.currency.code());
        xml.push_str("\">");
        xml.push_str(&subtotal.amount_minor.to_string());
        xml.push_str("</subtotal>\n");
    }
    xml.push_str("        </procurement>\n");
}
