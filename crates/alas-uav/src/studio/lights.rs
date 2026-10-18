// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Traffic-light vocabulary of the feasibility reports and the ESTIMATE
//! thresholds they use (split from `feasibility` for the module size budget).

/// Static-margin band, fraction of the wing MAC (ACC rule band quoted in
/// UAV-PLAN 6.2; the band is the competition's, not a physical optimum).
pub const STATIC_MARGIN_BAND: (f64, f64) = (0.05, 0.50);
/// Static thrust over weight below which the aircraft is flagged (ESTIMATE).
pub const MIN_STATIC_THRUST_TO_WEIGHT: f64 = 0.25;
/// Share of estimated (uncalibrated) mass above which the mass light is amber.
pub const MAX_ESTIMATED_MASS_SHARE: f64 = 0.3;
/// Horizontal and vertical tail volume floors (ESTIMATE, ~0.7 x Raymer Table 6.4).
pub const MIN_TAIL_VOLUME: (f64, f64) = (0.35, 0.025);
/// Current share of a rating above which the light is amber.
pub const RATING_MARGIN: f64 = 0.8;

/// Traffic light, ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Light {
    /// Within limits.
    Green,
    /// Close to a limit or resting on an assumption.
    Amber,
    /// Violates a limit.
    Red,
    /// The model could not run.
    Unknown,
}

/// Discipline of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Area {
    /// Packaging and collisions.
    Packaging,
    /// Mass and CG.
    Mass,
    /// Stability.
    Stability,
    /// Aerodynamics.
    Aero,
    /// Electrical.
    Electrical,
    /// Performance.
    Performance,
}

/// One traffic-light row.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Discipline.
    pub area: Area,
    /// Light.
    pub light: Light,
    /// Short label.
    pub label: String,
    /// Value shown (SI, unit in `unit`).
    pub value: Option<f64>,
    /// Limit it was compared to.
    pub limit: Option<f64>,
    /// SI unit of both.
    pub unit: &'static str,
    /// Spec field path or node to edit.
    pub field: Option<String>,
    /// Explanation, including assumptions.
    pub note: String,
}

/// A feasibility report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// Rows.
    pub items: Vec<Item>,
    /// Fidelity level that produced it (0 or 1).
    pub level: u8,
}

impl Report {
    /// Worst light.
    pub fn worst(&self) -> Light {
        self.items
            .iter()
            .map(|i| i.light)
            .max()
            .unwrap_or(Light::Green)
    }

    pub(super) fn push(
        &mut self,
        area: Area,
        light: Light,
        label: &str,
        note: String,
    ) -> &mut Item {
        self.items.push(Item {
            area,
            light,
            label: label.to_owned(),
            value: None,
            limit: None,
            unit: "",
            field: None,
            note,
        });
        let n = self.items.len();
        &mut self.items[n - 1]
    }
}

/// Requirements the lights compare against.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirements {
    /// Maximum allowed stall speed, m/s.
    pub max_stall_m_s: Option<f64>,
    /// Available runway, m.
    pub runway_m: Option<f64>,
    /// Static-margin band.
    pub static_margin: (f64, f64),
}

impl Default for Requirements {
    fn default() -> Self {
        Self {
            max_stall_m_s: None,
            runway_m: None,
            static_margin: STATIC_MARGIN_BAND,
        }
    }
}
