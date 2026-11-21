// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/control_surfaces_config.py

//! Where the control surfaces sit on the wing and tail.
//!
//! These are a drawing input, with one exception. The aerodynamic model treats
//! the wing and tail as plain lifting surfaces with no deflectable
//! sub-geometry, so changing a flap's span here changes what the
//! control-surface diagram shows and changes no computed number anywhere.
//! That is worth stating plainly, because a settings page full of flap
//! geometry invites the assumption that the flaps are being modelled.
//!
//! The exception is the rudder: its chord fraction, span run and maximum
//! deflection set the rudder area and authority of the one-engine-inoperative
//! yaw balance (minimum control speed, trim drag, crosswind check).
//!
//! Each surface is given as a chord fraction of the local chord and a span
//! run between two fractions of the semi-span. The defaults are typical
//! transport proportions; a registered aircraft declares its own runs in
//! `preset_control_surfaces`. A zero chord fraction states the aircraft does
//! not have that surface.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Chord and span fractions for each control surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct ControlSurfacesConfig {
    /// Slat chord as a fraction of local wing chord.
    #[config(
        label = "Slat chord fraction",
        help = "Leading-edge slat chord as a fraction of local wing chord."
    )]
    pub slat_chord_fraction: f64,

    /// Inboard end of the slat run.
    #[config(
        label = "Slat span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the slat run, as a fraction of wing semi-span from the root."
    )]
    pub slat_span_start_frac: f64,

    /// Outboard end of the slat run.
    #[config(
        label = "Slat span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the slat run, as a fraction of wing semi-span from the root."
    )]
    pub slat_span_end_frac: f64,

    /// Flap chord as a fraction of local wing chord.
    #[config(
        label = "Flap chord fraction",
        help = "Trailing-edge flap chord as a fraction of local wing chord."
    )]
    pub flap_chord_fraction: f64,

    /// Inboard end of the flap run.
    #[config(
        label = "Flap span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the flap run (just outside the fuselage), as a fraction of wing semi-span."
    )]
    pub flap_span_start_frac: f64,

    /// Outboard end of the flap run.
    #[config(
        label = "Flap span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the flap run, as a fraction of wing semi-span."
    )]
    pub flap_span_end_frac: f64,

    /// Aileron chord as a fraction of local wing chord.
    #[config(
        label = "Aileron chord fraction",
        help = "Aileron chord as a fraction of local wing chord."
    )]
    pub aileron_chord_fraction: f64,

    /// Inboard end of the aileron run.
    #[config(
        label = "Aileron span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the aileron run, as a fraction of wing semi-span."
    )]
    pub aileron_span_start_frac: f64,

    /// Outboard end of the aileron run.
    #[config(
        label = "Aileron span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the aileron run, as a fraction of wing semi-span."
    )]
    pub aileron_span_end_frac: f64,

    /// Inboard aileron chord as a fraction of local wing chord.
    #[serde(default)]
    #[config(
        label = "Inboard aileron chord fraction",
        help = "Chord of the inboard (high-speed) aileron or flaperon as a fraction of local wing chord. Zero means the aircraft has none."
    )]
    pub inboard_aileron_chord_fraction: f64,

    /// Inboard end of the inboard aileron run.
    #[serde(default)]
    #[config(
        label = "Inboard aileron span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the inboard aileron run, as a fraction of wing semi-span."
    )]
    pub inboard_aileron_span_start_frac: f64,

    /// Outboard end of the inboard aileron run.
    #[serde(default)]
    #[config(
        label = "Inboard aileron span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the inboard aileron run, as a fraction of wing semi-span."
    )]
    pub inboard_aileron_span_end_frac: f64,

    /// Spoiler chord as a fraction of local wing chord.
    #[config(
        label = "Spoiler chord fraction",
        help = "Spoiler/speedbrake chord as a fraction of local wing chord (ahead of the flaps)."
    )]
    pub spoiler_chord_fraction: f64,

    /// Inboard end of the spoiler run.
    #[config(
        label = "Spoiler span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the spoiler run, as a fraction of wing semi-span."
    )]
    pub spoiler_span_start_frac: f64,

    /// Outboard end of the spoiler run.
    #[config(
        label = "Spoiler span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the spoiler run (typically spanning the flap run), as a fraction of wing semi-span."
    )]
    pub spoiler_span_end_frac: f64,

    /// Elevator chord as a fraction of local stabilizer chord.
    #[config(
        label = "Elevator chord fraction",
        help = "Elevator chord as a fraction of local horizontal-stabilizer chord."
    )]
    pub elevator_chord_fraction: f64,

    /// Inboard end of the elevator run.
    #[config(
        label = "Elevator span start",
        unit = "fraction of semi-span",
        help = "Inboard end of the elevator run, as a fraction of h-stab semi-span."
    )]
    pub elevator_span_start_frac: f64,

    /// Outboard end of the elevator run.
    #[config(
        label = "Elevator span end",
        unit = "fraction of semi-span",
        help = "Outboard end of the elevator run, as a fraction of h-stab semi-span."
    )]
    pub elevator_span_end_frac: f64,

    /// Rudder chord as a fraction of local fin chord.
    #[config(
        label = "Rudder chord fraction",
        help = "Rudder chord as a fraction of local vertical-stabilizer chord."
    )]
    pub rudder_chord_fraction: f64,

    /// Root-ward end of the rudder run.
    #[config(
        label = "Rudder span start",
        unit = "fraction of semi-span",
        help = "Root-ward end of the rudder run, as a fraction of v-stab span."
    )]
    pub rudder_span_start_frac: f64,

    /// Tip-ward end of the rudder run.
    #[config(
        label = "Rudder span end",
        unit = "fraction of semi-span",
        help = "Tip-ward end of the rudder run, as a fraction of v-stab span."
    )]
    pub rudder_span_end_frac: f64,

    /// Largest rudder deflection each way.
    #[serde(default = "default_rudder_max_deflection_deg")]
    #[config(
        label = "Rudder maximum deflection",
        unit = "deg",
        help = "Largest rudder deflection each way [deg]. With rudder chord and span it sets the authority for the one-engine-inoperative yaw balance.",
        long_help = "Largest rudder deflection each way. With the rudder chord and span it sets the rudder authority of the one-engine-inoperative yaw balance (minimum control speed, trim drag, crosswind). Transports use 25 to 30 degrees for a single-hinge rudder and up to 35 degrees for a double-hinge one (Torenbeek, Synthesis of Subsonic Airplane Design, Sec. 9.6.1; Roskam, Airplane Design Part VII, Sec. 2.3)."
    )]
    pub rudder_max_deflection_deg: f64,
}

/// Default of [`ControlSurfacesConfig::rudder_max_deflection_deg`]: 30 degrees,
/// the upper end of Torenbeek Sec. 9.6.1 for a single-hinge rudder.
const fn default_rudder_max_deflection_deg() -> f64 {
    30.0
}

impl Default for ControlSurfacesConfig {
    fn default() -> Self {
        Self {
            slat_chord_fraction: 0.15,
            slat_span_start_frac: 0.08,
            slat_span_end_frac: 0.95,
            flap_chord_fraction: 0.25,
            flap_span_start_frac: 0.10,
            flap_span_end_frac: 0.62,
            aileron_chord_fraction: 0.20,
            aileron_span_start_frac: 0.66,
            aileron_span_end_frac: 0.95,
            inboard_aileron_chord_fraction: 0.0,
            inboard_aileron_span_start_frac: 0.0,
            inboard_aileron_span_end_frac: 0.0,
            spoiler_chord_fraction: 0.10,
            spoiler_span_start_frac: 0.10,
            spoiler_span_end_frac: 0.64,
            elevator_chord_fraction: 0.35,
            elevator_span_start_frac: 0.05,
            elevator_span_end_frac: 0.95,
            rudder_chord_fraction: 0.35,
            rudder_span_start_frac: 0.10,
            rudder_span_end_frac: 0.90,
            rudder_max_deflection_deg: default_rudder_max_deflection_deg(),
        }
    }
}

impl ControlSurfacesConfig {
    /// Whether the wing has spoilers or speedbrakes: a positive chord over a
    /// span run of positive length. A zero chord states the aircraft has none.
    pub fn has_spoilers(&self) -> bool {
        self.spoiler_chord_fraction > 0.0
            && self.spoiler_span_end_frac > self.spoiler_span_start_frac
    }

    /// The span band, fractions of the semi-span, that the inboard aileron cuts
    /// out of the flap run where it sits between an inboard and an outboard
    /// flap (747, 787, DC-10). `None` without an inboard aileron inside the run.
    pub fn flap_gap(&self) -> Option<(f64, f64)> {
        let (start, end) = (self.flap_span_start_frac, self.flap_span_end_frac);
        let (gap_start, gap_end) = (
            self.inboard_aileron_span_start_frac.max(start),
            self.inboard_aileron_span_end_frac.min(end),
        );
        (self.inboard_aileron_chord_fraction > 0.0 && gap_end > gap_start)
            .then_some((gap_start, gap_end))
    }

    /// The trailing-edge flap bands, fractions of the semi-span: the flap run
    /// less [`Self::flap_gap`], so no span carries both a flap and the inboard
    /// aileron. Empty when the wing has no flap.
    pub fn flap_bands(&self) -> Vec<(f64, f64)> {
        let (start, end) = (self.flap_span_start_frac, self.flap_span_end_frac);
        if !(self.flap_chord_fraction > 0.0 && end > start) {
            return Vec::new();
        }
        match self.flap_gap() {
            None => vec![(start, end)],
            Some((gap_start, gap_end)) => [(start, gap_start), (gap_end, end)]
                .into_iter()
                .filter(|(a, b)| b > a)
                .collect(),
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inboard_aileron_between_two_flaps_is_cut_out_of_the_flap_run() {
        let mut config = ControlSurfacesConfig::default();
        assert_eq!(
            config.flap_bands(),
            vec![(config.flap_span_start_frac, config.flap_span_end_frac)]
        );
        config.inboard_aileron_chord_fraction = 0.2;
        config.inboard_aileron_span_start_frac = 0.35;
        config.inboard_aileron_span_end_frac = 0.40;
        assert_eq!(config.flap_gap(), Some((0.35, 0.40)));
        assert_eq!(config.flap_bands(), vec![(0.10, 0.35), (0.40, 0.62)]);
        config.flap_chord_fraction = 0.0;
        assert!(config.flap_bands().is_empty());
    }

    #[test]
    fn every_span_run_starts_inboard_of_where_it_ends() {
        // A reversed pair draws a surface of negative length, which the
        // diagram renders as nothing at all rather than as an error.
        let config = ControlSurfacesConfig::default();
        for (name, start, end) in [
            (
                "slat",
                config.slat_span_start_frac,
                config.slat_span_end_frac,
            ),
            (
                "flap",
                config.flap_span_start_frac,
                config.flap_span_end_frac,
            ),
            (
                "aileron",
                config.aileron_span_start_frac,
                config.aileron_span_end_frac,
            ),
            (
                "spoiler",
                config.spoiler_span_start_frac,
                config.spoiler_span_end_frac,
            ),
            (
                "elevator",
                config.elevator_span_start_frac,
                config.elevator_span_end_frac,
            ),
            (
                "rudder",
                config.rudder_span_start_frac,
                config.rudder_span_end_frac,
            ),
        ] {
            assert!(start < end, "{name}: {start} is not inboard of {end}");
            assert!((0.0..=1.0).contains(&start), "{name} starts off the span");
            assert!((0.0..=1.0).contains(&end), "{name} ends off the span");
        }
    }

    #[test]
    fn the_rudder_deflection_limit_defaults_to_the_transport_upper_range() {
        let config = ControlSurfacesConfig::default();
        assert!((config.rudder_max_deflection_deg - 30.0).abs() < 1e-12);
        // A legacy file without the field loads with the default.
        let mut value = serde_json::to_value(&config).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("rudder_max_deflection_deg");
        let loaded: ControlSurfacesConfig = serde_json::from_value(value).unwrap();
        assert!((loaded.rudder_max_deflection_deg - 30.0).abs() < 1e-12);
    }

    #[test]
    fn the_flaps_and_the_ailerons_do_not_overlap() {
        // They share the trailing edge, so an overlap is a drawing showing
        // two surfaces occupying the same structure.
        let config = ControlSurfacesConfig::default();
        assert!(config.flap_span_end_frac <= config.aileron_span_start_frac);
    }

    #[test]
    fn the_spoilers_sit_over_the_flap_run() {
        let config = ControlSurfacesConfig::default();
        assert!(config.spoiler_span_start_frac >= config.flap_span_start_frac);
        assert!(config.spoiler_span_end_frac <= config.aileron_span_start_frac);
    }
}
