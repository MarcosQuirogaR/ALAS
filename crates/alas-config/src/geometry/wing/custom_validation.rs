// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validation and ordering of the user-defined stations of a [`WingConfig`].

use super::config::WingConfig;
use super::planform::{MainWingStationKind, TransportPlanform, WingSection, WingSectionError};

impl WingConfig {
    /// Validate user-defined stations without resolving their airfoils.
    ///
    /// The builder performs the planform-dependent taper check after it has a
    /// design vector. Keeping the structural checks here also lets the GUI
    /// reject malformed saved values before a build is attempted.
    pub fn validate_custom_sections(&self) -> Result<(), WingSectionError> {
        let mut previous = None;
        for (index, section) in self.custom_sections.iter().enumerate() {
            for (field, value) in [
                ("span_fraction", section.span_fraction),
                ("leading_edge_x_m", section.leading_edge_x_m),
                ("chord_m", section.chord_m),
                ("z_m", section.z_m),
                ("twist_deg", section.twist_deg),
            ] {
                if !value.is_finite() {
                    return Err(WingSectionError::NonFinite {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(0.0..1.0).contains(&section.span_fraction) {
                return Err(WingSectionError::SpanOutOfRange {
                    index,
                    value: section.span_fraction,
                });
            }
            if section.chord_m <= 0.0 {
                return Err(WingSectionError::NonPositiveChord {
                    index,
                    value: section.chord_m,
                });
            }
            if section.airfoil.trim().is_empty() {
                return Err(WingSectionError::EmptyAirfoil(index));
            }
            if let Some(previous) = previous {
                if section.span_fraction <= previous {
                    return Err(WingSectionError::InvalidOrder {
                        index,
                        previous,
                        current: section.span_fraction,
                    });
                }
            }
            previous = Some(section.span_fraction);
        }
        Ok(())
    }

    /// Validate custom chords against the defining planform stations.
    pub fn validate_custom_sections_against_planform(
        &self,
        planform: &TransportPlanform,
    ) -> Result<(), WingSectionError> {
        self.validate_custom_sections()?;
        let planform_stations = planform.stations();
        let mut stations: Vec<(f64, f64)> = planform_stations
            .iter()
            .filter(|station| {
                station.kind != MainWingStationKind::SideOfBody
                    || self.side_of_body_chord_ratio.is_some()
            })
            .map(|station| (station.span_fraction, station.chord_m))
            .collect();
        stations.extend(
            self.custom_sections
                .iter()
                .map(|section| (section.span_fraction, section.chord_m)),
        );
        stations.sort_by(|left, right| left.0.total_cmp(&right.0));
        for pair in stations.windows(2) {
            if (pair[1].0 - pair[0].0).abs() <= 1.0e-9 {
                if let Some(station) = planform_stations.iter().find(|station| {
                    (station.span_fraction - pair[1].0).abs() <= 1.0e-9
                        && station.kind != MainWingStationKind::Root
                        && station.kind != MainWingStationKind::Tip
                        && (station.kind != MainWingStationKind::SideOfBody
                            || self.side_of_body_chord_ratio.is_some())
                }) {
                    return Err(WingSectionError::DuplicatePlanformStation {
                        span_fraction: pair[1].0,
                        kind: station.kind,
                    });
                }
            }
            if pair[1].1 > pair[0].1 + 1.0e-9 {
                return Err(WingSectionError::NonMonotoneChord {
                    inboard: pair[0].1,
                    outboard: pair[1].1,
                });
            }
        }
        Ok(())
    }

    /// Return custom stations in validated centerline-to-tip order.
    pub fn custom_sections_sorted(&self) -> Result<Vec<WingSection>, WingSectionError> {
        self.validate_custom_sections()?;
        let mut sections = self.custom_sections.clone();
        sections.sort_by(|left, right| left.span_fraction.total_cmp(&right.span_fraction));
        Ok(sections)
    }
}
