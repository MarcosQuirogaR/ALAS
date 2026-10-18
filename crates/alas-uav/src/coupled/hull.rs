// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Non-wing parasite-drag components from a `UavSpec` and a hull description.
//!
//! The spec carries the empennage and the boom; the fuselage envelope, wheels
//! and nacelles are not spec fields yet (the airframe lane will derive them
//! from the component tree), so the caller describes them in [`Hull`].  The
//! result feeds [`crate::aero::drag_lowre::build_up`] (Cf transitional, Hoerner
//! form factors, Raymer interference factors; no Mach term).
//!
//! Tail surfaces: planform area `S = span * (c_root + c_tip)/2`, wetted area
//! `2 S (1 + 0.25 t/c)` (thin-surface estimate, an ESTIMATE not read on a
//! page), thickness from the airfoil coordinates.  The
//! `TailSurface::span_m` of a V pair is the span along the surfaces
//! (competitions.md: Xtra2UPV `b_t c_t = S_t`).

use alas_geom::aircraft::airfoil::Airfoil;

use crate::aero::drag_lowre::{table_12_6, Component, Junction, Shape};
use crate::spec::master::{EmpennageDriving, TailSurface};
use crate::spec::topology::TailKind;
use crate::spec::UavSpec;

use super::geometry::resolve_airfoil;
use super::slipstream_drag::MotorStation;
use super::CoupledError;

/// Fuselage envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fuselage {
    /// Length, m.
    pub length_m: f64,
    /// Equivalent diameter `sqrt(4 A_max / pi)`, m.
    pub diameter_m: f64,
    /// Wetted area, m2 (`None`: `0.85 pi d L`, a slender-body estimate).
    pub wetted_m2: Option<f64>,
}

/// What the spec does not carry yet: the fuselage and the itemised parts.
#[derive(Debug, Clone)]
pub struct Hull {
    /// Fuselage envelope (`None` for a flying wing).
    pub fuselage: Option<Fuselage>,
    /// Wheels, struts, nacelles, antennas (itemised parts) that stay in the
    /// airflow for the whole flight.
    pub items: Vec<Component>,
    /// Parts in the airflow on the ground only (retracted gear): their drag
    /// area is added to the ground roll and the rollout, not to the flight polar.
    pub ground_only: Vec<Component>,
    /// Excrescence and leakage allowance as a fraction of the component sum
    /// (Raymer Table 12.8: 5-10 % for propeller aircraft).
    pub excrescence_fraction: f64,
    /// Tractor motors in front of the wing (empty: no slipstream drag).
    pub motors: Vec<MotorStation>,
}

impl Default for Hull {
    fn default() -> Self {
        Self {
            fuselage: None,
            items: Vec::new(),
            ground_only: Vec::new(),
            excrescence_fraction: crate::aero::drag_lowre::DEFAULT_EXCRESCENCE_FRACTION,
            motors: Vec::new(),
        }
    }
}

/// A wheel and tyre of diameter `d` and width `w` (Raymer Table 12.6 regular
/// wheel, `CD = 0.25` on `d w`), with the gear interference factor 1.2.
pub fn wheel(name: &str, diameter_m: f64, width_m: f64, count: u32) -> Component {
    Component::new(
        name,
        Shape::Item {
            cd: table_12_6::WHEEL_REGULAR,
            frontal_m2: diameter_m * width_m,
        },
    )
    .with_count(count)
    .as_gear()
}

fn thickness_ratio(airfoil: &Airfoil) -> f64 {
    let x: Vec<f64> = (0..=100).map(|i| f64::from(i) / 100.0).collect();
    airfoil.max_thickness(&x)
}

fn surface(
    name: &str,
    s: &TailSurface,
    junction: Junction,
    count: u32,
) -> Result<Component, CoupledError> {
    let ok = s.span_m > 0.0 && s.root_chord_m > 0.0 && s.taper_ratio > 0.0;
    if !ok {
        return Err(CoupledError::Spec(format!(
            "{name}: bad tail surface {s:?}"
        )));
    }
    let tc = thickness_ratio(&resolve_airfoil(&s.airfoil)?);
    let mean_chord = 0.5 * s.root_chord_m * (1.0 + s.taper_ratio);
    let planform = s.span_m * mean_chord;
    Ok(Component::new(
        name,
        Shape::Surface {
            chord_m: mean_chord,
            wetted_m2: 2.0 * planform * (1.0 + 0.25 * tc),
            t_over_c: tc,
        },
    )
    .with_junction(junction)
    .with_count(count))
}

/// The parasite-drag components of `spec` plus `hull`.
///
/// # Errors
///
/// [`CoupledError::Spec`] for non-positive dimensions or unknown airfoils.
pub fn components(spec: &UavSpec, hull: &Hull) -> Result<Vec<Component>, CoupledError> {
    let mut out = Vec::new();
    if let Some(f) = hull.fuselage {
        if f.length_m <= 0.0 || f.diameter_m <= 0.0 {
            return Err(CoupledError::Spec("fuselage dimensions must be > 0".into()));
        }
        let wetted = f
            .wetted_m2
            .unwrap_or(0.85 * std::f64::consts::PI * f.diameter_m * f.length_m);
        out.push(Component::new(
            "fuselage",
            Shape::Body {
                length_m: f.length_m,
                diameter_m: f.diameter_m,
                wetted_m2: wetted,
            },
        ));
    }
    if let Some(b) = &spec.master.tail_boom {
        out.push(Component::new(
            "tail boom",
            Shape::Body {
                length_m: b.length_m,
                diameter_m: b.outer_diameter_m,
                wetted_m2: std::f64::consts::PI * b.outer_diameter_m * b.length_m,
            },
        ));
    }
    if let EmpennageDriving::Explicit {
        horizontal,
        vertical,
    } = &spec.master.empennage
    {
        let (h_junction, h_name) = match spec.topology.tail {
            TailKind::V { .. } | TailKind::InvV { .. } => (Junction::VTail, "V tail"),
            TailKind::T => (Junction::HTail, "horizontal tail (T)"),
            _ => (Junction::ConventionalTail, "horizontal tail"),
        };
        if let Some(h) = horizontal {
            out.push(surface(h_name, h, h_junction, 1)?);
        }
        if let Some(v) = vertical {
            out.push(surface("vertical tail", v, Junction::ConventionalTail, 1)?);
        }
    } else {
        return Err(CoupledError::Spec(
            "driven empennage needs the airframe lane to size the surfaces; use Explicit".into(),
        ));
    }
    out.extend(hull.items.iter().cloned());
    Ok(out)
}
