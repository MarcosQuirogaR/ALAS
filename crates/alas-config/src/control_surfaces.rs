// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/control_surfaces_config.py
// Reference: alas @ rust-port-baseline.

//! Where the control surfaces sit on the wing and tail.
//!
//! These are a drawing input and nothing more. The aerodynamic model treats
//! the wing and tail as plain lifting surfaces with no deflectable
//! sub-geometry, so changing a flap's span here changes what the
//! control-surface diagram shows and changes no computed number anywhere.
//! That is worth stating plainly, because a settings page full of flap
//! geometry invites the assumption that the flaps are being modelled.
//!
//! Each surface is given as a chord fraction of the local chord and a span
//! run between two fractions of the semi-span. The defaults are typical
//! transport proportions.

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
            spoiler_chord_fraction: 0.10,
            spoiler_span_start_frac: 0.10,
            spoiler_span_end_frac: 0.64,
            elevator_chord_fraction: 0.35,
            elevator_span_start_frac: 0.05,
            elevator_span_end_frac: 0.95,
            rudder_chord_fraction: 0.35,
            rudder_span_start_frac: 0.10,
            rudder_span_end_frac: 0.90,
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
