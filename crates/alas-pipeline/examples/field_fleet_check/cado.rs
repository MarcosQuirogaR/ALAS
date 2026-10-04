// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CADO input parsing and the historical category assignment.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

pub(super) struct Aircraft {
    pub(super) fields: BTreeMap<String, String>,
}

impl Aircraft {
    pub(super) fn text(&self, name: &str) -> &str {
        self.fields.get(name).map_or("", String::as_str)
    }

    pub(super) fn number(&self, name: &str) -> Result<f64, String> {
        let value = self
            .text(name)
            .parse::<f64>()
            .map_err(|error| format!("{}: {name}: {error}", self.text("name")))?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("{}: non-finite {name}", self.text("name")))
        }
    }

    pub(super) fn category(&self) -> &str {
        match self.text("engine_type") {
            "turbofan" => {
                if matches!(self.text("airplane_type"), "general" | "business") {
                    "business_jet"
                } else if self
                    .number("fuselage_width")
                    .is_ok_and(|width| width >= 4.5)
                {
                    "widebody"
                } else if self.text("airplane_type") == "regional" {
                    "regional_jet"
                } else if matches!(self.text("airplane_type"), "short_medium" | "long_range") {
                    "narrowbody"
                } else {
                    "other"
                }
            }
            "turboprop" => {
                if self.number("mtow").is_ok_and(|mass| mass > 8_618.0) {
                    "turboprop"
                } else {
                    "turboprop_light"
                }
            }
            _ => "other",
        }
    }

    pub(super) fn metadata(&self) -> Value {
        let landing_exclusion = match self.text("name") {
            "MD-90-30ER" => "CADO LFL 440 m is inconsistent with transport landing distance",
            name if name.starts_with("787-") => "CADO LFL copies or exceeds TOFL",
            _ => "",
        };
        json!({
            "name": self.text("name"),
            "category": self.category(),
            "propulsion": self.text("engine_type"),
            "published_tofl_m": self.number("tofl").ok(),
            "published_lfl_m": self.number("lfl").ok(),
            "published_vapp_m_s": self.number("approach_speed").ok().map(|speed| speed / 3.6),
            "landing_exclusion": landing_exclusion,
            "class_basis": if matches!(self.category(), "business_jet" | "turboprop_light" | "other") {
                "simple-flap extrapolation; absent from earlier four-class audit"
            } else {
                "historical class mapping"
            },
            "condition": "ISA sea level, zero wind, dry level runway, CADO MTOW/MLW; CADO source conditions unspecified",
        })
    }
}

pub(super) fn read(path: &Path) -> Result<Vec<Aircraft>, String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines = source.lines();
    let header: Vec<_> = lines
        .next()
        .ok_or_else(|| "CADO column header is absent".to_owned())?
        .split(';')
        .collect();
    lines.next(); // Declared units; approach speed is km/h and shaft power is kW.
    lines.next(); // Mach override applies to jet cruise/max speed, neither used here.
    Ok(lines
        .filter(|line| !line.is_empty())
        .map(|line| Aircraft {
            fields: header
                .iter()
                .zip(line.split(';'))
                .filter(|(name, _)| !name.is_empty())
                .map(|(name, value)| ((*name).to_owned(), value.to_owned()))
                .collect(),
        })
        .collect())
}
