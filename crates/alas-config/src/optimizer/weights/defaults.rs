// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shipped defaults for [`ObjectiveWeights`].

use super::ObjectiveWeights;

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            transport_planform_constraints_enabled: true,
            transport_shape_priors_enabled: false,
            // Body-attitude window (D8, re-derived from sources, deg, geometric
            // angle of the fuselage reference line to the flight path at
            // mid-cruise, nose-up positive; compressible trim). Lower bound 0:
            // the fuselage must not fly nose-down in cruise. Sadraey (2013),
            // Aircraft Design, sec. 5.5 (p. 196): "the fuselage angle of
            // attack must be zero in cruise" for minimum fuselage drag;
            // Jenkinson et al. (1999), Civil Jet Aircraft Design, "Wing/body
            // setting": the fuselage is at its minimum drag attitude at the
            // mid-cruise condition, "approximately level"; Raymer (6th ed.)
            // Ch. 4 "Wing incidence" (p. 88) and Ch. 23.2 (p. 837): "a few
            // degrees nose-up" / "a small angle of attack". The former 2.0 minimum was not sourced: it was written
            // on the incompressible attitude, which the Goethert trim (AERL-02)
            // shows overstated alpha by 1 to 1.5 deg. The 4.0 maximum is an
            // unsourced engineering limit (no primary source gives a number
            // above "a few degrees"); it is unchanged because compressibility
            // only lowers the attitude.
            geometric_body_alpha_min_deg: 0.0,
            geometric_body_alpha_max_deg: 4.0,
            min_root_wingbox_depth_m: 1.20,
            min_break_wingbox_depth_m: 0.65,
            min_break_wingbox_width_m: 2.50,
            min_flap_area_fraction: 0.11,
            max_root_bending_box_slenderness: 700.0,
            tankable_span_start_fraction: 0.10,
            tankable_span_end_fraction: 0.75,
            max_break_root_chord_ratio: 0.65,
            min_break_root_chord_ratio: 0.40,
            min_tip_root_chord_ratio: 0.16,
            min_inboard_te_sweep_deg: 0.0,
            max_inboard_te_sweep_deg: 22.0,
            failure_cost: 1_000.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
