// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The extended-area increments of the high-lift condition, split out of
//! `super` for the line budget.

use alas_geom::aircraft::airplane::Airplane;

use crate::neutral_point::HighLiftDevices;

/// The extended-area increments of the high-lift condition: lift-slope
/// fractions of the wing and the stations they act at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HighLiftScaling {
    /// Slope increment of the extended flaps, as a fraction of the wing slope.
    pub(crate) delta_flap: f64,
    /// Station the flap-extension lift acts at (geometry axes, stretched if
    /// the condition is), m.
    pub(crate) x_flap: f64,
    /// Slope increment of the extended slats, as a fraction of the wing slope.
    pub(crate) delta_slat: f64,
    /// Station the slat-extension lift acts at, m.
    pub(crate) x_slat: f64,
}

impl HighLiftScaling {
    /// Clean configuration.
    pub(crate) const NONE: Self = Self {
        delta_flap: 0.0,
        x_flap: 0.0,
        delta_slat: 0.0,
        x_slat: 0.0,
    };

    /// Wing slope scale `r = 1 + delta_flap + delta_slat`.
    pub(crate) fn slope_scale(&self) -> f64 {
        1.0 + self.delta_flap + self.delta_slat
    }

    /// Shift of the wing's aerodynamic centre from the added lift acting at
    /// the added-lift stations: `(d_f (x_f - x_w) + d_s (x_s - x_w)) / r`.
    pub(crate) fn ac_shift(&self, x_wing_ac: f64) -> f64 {
        (self.delta_flap * (self.x_flap - x_wing_ac) + self.delta_slat * (self.x_slat - x_wing_ac))
            / self.slope_scale()
    }

    /// The same increments with every station divided by `beta`: the
    /// Goethert-stretched geometry of a compressible condition.
    pub(crate) fn stretched(self, beta: f64) -> Self {
        Self {
            x_flap: self.x_flap / beta,
            x_slat: self.x_slat / beta,
            ..self
        }
    }
}

/// Slope increments and acting stations of the deployed `devices` on the main
/// wing's built planform (geometry axes). Each device run contributes
/// `delta = g A / S_ref` with `A` the full-chord area of the run (both sides)
/// and `g = c'/c - 1` for a slat and, for the flap, `g = e - (c_f/c) sin^2
/// delta_f`, `e = c'/c - 1`: the deflection cancels part of the extension gain
/// (Torenbeek 1982, App. G, p. 537, eq. G-19; zero at the floor). The section extended to `c' = (1 + e) c` carries its aerodynamic
/// centre at `c'/4` of the extended chord (thin-airfoil theory: the section AC
/// sits at the quarter chord of whatever chord is deployed); the added lift acts
/// at `x_le + 0.25 (2 + e) c` for a trailing-edge extension and at
/// `x_le - (0.5 + 0.75 e) c` for a leading-edge extension (the difference of
/// the extended and original section moments about the leading edge).
pub(crate) fn high_lift_scaling(airplane: &Airplane, devices: &HighLiftDevices) -> HighLiftScaling {
    let Some(wing) = crate::trim::main_wing(airplane).filter(|wing| wing.xsecs.len() >= 2) else {
        return HighLiftScaling::NONE;
    };
    // The devices are on the structure: a run starting inside the body takes the
    // box there, not the plan of the wing around it.
    let carried = wing.carried_box();
    let wing = carried.as_ref();
    let semi_span = wing
        .xsecs
        .iter()
        .map(|xsec| xsec.xyz_le[1].abs())
        .fold(0.0_f64, f64::max);
    if semi_span <= 0.0 || airplane.s_ref <= 0.0 {
        return HighLiftScaling::NONE;
    }
    // (area, area-weighted acting x) over `span` for an extension `ext`.
    let run = |span: (f64, f64), ext: f64, trailing: bool| -> (f64, f64) {
        let (y_in, y_out) = (span.0 * semi_span, span.1 * semi_span);
        let (mut area, mut moment) = (0.0, 0.0);
        for pair in wing.xsecs.windows(2) {
            let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
            let (lo, hi) = (y0.min(y1).max(y_in), y0.max(y1).min(y_out));
            if (y1 - y0).abs() < 1e-9 || hi <= lo {
                continue;
            }
            const STEPS: usize = 16;
            let dy = (hi - lo) / STEPS as f64;
            for step in 0..STEPS {
                let y = lo + (step as f64 + 0.5) * dy;
                let t = (y - y0) / (y1 - y0);
                let x_le = pair[0].xyz_le[0] + t * (pair[1].xyz_le[0] - pair[0].xyz_le[0]);
                let chord = pair[0].chord + t * (pair[1].chord - pair[0].chord);
                let x = if trailing {
                    x_le + 0.25 * (2.0 + ext) * chord
                } else {
                    x_le - (0.5 + 0.75 * ext) * chord
                };
                area += 2.0 * chord * dy;
                moment += 2.0 * chord * dy * x;
            }
        }
        if area > 0.0 {
            (area, moment / area)
        } else {
            (0.0, 0.0)
        }
    };
    let (flap_area, x_flap) = {
        let (run_area, x_run) = run(devices.flap_span, devices.flap_extension, true);
        // An inboard aileron between two flaps does not extend the chord.
        let (gap_area, x_gap) = devices
            .flap_gap
            .map_or((0.0, 0.0), |gap| run(gap, devices.flap_extension, true));
        let area = run_area - gap_area;
        if area > 0.0 {
            (area, (run_area * x_run - gap_area * x_gap) / area)
        } else {
            (0.0, 0.0)
        }
    };
    let (slat_area, x_slat) = run(devices.slat_span, devices.slat_extension, false);
    let flap_gain = (devices.flap_extension
        - devices.flap_chord_fraction * devices.flap_deflection_deg.to_radians().sin().powi(2))
    .max(0.0);
    HighLiftScaling {
        delta_flap: flap_gain * flap_area / airplane.s_ref,
        x_flap,
        delta_slat: devices.slat_extension * slat_area / airplane.s_ref,
        x_slat,
    }
}

/// The lift the deployed `devices` add by extending the chord, as the
/// high-lift neutral-point condition carries it (Raymer, *Aircraft Design: A
/// Conceptual Approach*, 6th ed., printed p. 413; see
/// [`crate::neutral_point::NpConditionsInput`]), for the other balances that
/// must see the same flapped wing (the take-off and landing trims).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedChordLift {
    /// Wing-slope increment of the flap and slat runs, a fraction of the wing
    /// slope (zero when nothing extends): the flap's `(e - (c_f/c) sin^2
    /// delta_f) S_flap / S` and the slat's `e S_slat / S`.
    pub slope_increment: f64,
    /// Station the added lift acts at (geometry axes), m; `NaN` when nothing
    /// extends.
    pub x_acting_m: f64,
}

/// [`ExtendedChordLift`] of `devices` on the main wing of `airplane`: the
/// same strip integration as the neutral point's high-lift condition.
#[must_use]
pub fn extended_chord_lift(airplane: &Airplane, devices: &HighLiftDevices) -> ExtendedChordLift {
    let scaling = high_lift_scaling(airplane, devices);
    let slope_increment = scaling.delta_flap + scaling.delta_slat;
    let x_acting_m = if slope_increment > 0.0 {
        (scaling.delta_flap * scaling.x_flap + scaling.delta_slat * scaling.x_slat)
            / slope_increment
    } else {
        f64::NAN
    };
    ExtendedChordLift {
        slope_increment,
        x_acting_m,
    }
}
