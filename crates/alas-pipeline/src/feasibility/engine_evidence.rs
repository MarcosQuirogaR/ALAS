// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The evidence behind the cruise TSFC of the catalogue engine.
//!
//! The cruise TSFC anchors the high-altitude fuel flow of the turbofan deck,
//! so trip fuel and range follow it nearly one to one (Breguet). The
//! catalogue states for each engine whether that figure is a published datum
//! of the engine, the figure of a sibling engine, or an estimate with no
//! source. Fuel results that rest on anything but a published datum are
//! flagged, not reported as if they were validated.

use alas_config::{ActiveEngineModel, EngineConfig};

use super::{warning, FindingCode, PhysicalFinding};

/// A warning when the active turbofan burns the catalogue cruise TSFC of its
/// engine and the catalogue does not hold a published datum for it. `None`
/// for a published figure, for a turboprop, for an engine outside the
/// catalogue, and for a cruise TSFC the configuration states itself (the
/// catalogue's evidence class does not describe that number).
pub(super) fn finding(engine: &EngineConfig) -> Option<PhysicalFinding> {
    let catalogue = alas_config::engines::get(&engine.engine_name).ok()?;
    let ActiveEngineModel::Turbofan(spec) = engine.active_model().ok()? else {
        return None;
    };
    if spec.cruise_tsfc_kg_kgf_hr != catalogue.cruise_tsfc_kg_kgf_hr {
        return None;
    }
    let class = match catalogue.cruise_tsfc_evidence.as_str() {
        "published" => return None,
        "family-proxy" => "the figure of a sibling engine (family proxy)",
        "unsourced-estimate" => "an estimate with no source",
        _ => "of unrecorded origin",
    };
    Some(warning(
        FindingCode::PropulsionCruiseTsfcNotPublished,
        format!(
            "the cruise TSFC of {} ({} kg/(kgf h), uninstalled) is {class}, not a published datum of this engine; trip fuel, block fuel and range scale with it nearly one to one and are not validated",
            catalogue.display_name, catalogue.cruise_tsfc_kg_kgf_hr
        ),
        Some(catalogue.cruise_tsfc_kg_kgf_hr),
        None,
        "kg/(kgf h)",
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn engine(name: &str) -> EngineConfig {
        let mut engine = EngineConfig {
            engine_name: name.to_owned(),
            ..EngineConfig::default()
        };
        engine.apply_engine_spec();
        engine
    }

    #[test]
    fn only_a_published_cruise_tsfc_is_reported_without_a_warning() {
        for name in ["CFM56-5C", "Trent XWB-84", "CF6-80C2"] {
            assert!(finding(&engine(name)).is_none(), "{name}");
        }
        for (name, class) in [("GE9X", "no source"), ("GEnx-1B", "family proxy")] {
            let found = finding(&engine(name)).unwrap();
            assert_eq!(found.code, FindingCode::PropulsionCruiseTsfcNotPublished);
            assert!(found.message.contains(class), "{name}: {}", found.message);
        }
    }
}
