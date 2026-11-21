// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off CLmax parts of the presets with an outboard trailing-edge break,
//! with the break on and switched off in place (diagnostic).
#![allow(clippy::print_stdout, clippy::unwrap_used, missing_docs)]

use alas_config::presets;

fn main() {
    for name in ["A340-300", "A380-800"] {
        let on = presets::get(name).unwrap();
        let mut off = on.clone();
        off.geometry.wing.te_break_span_fraction = None;
        off.geometry.wing.te_break_chord_offset_m = None;
        off.geometry.wing.te_break_2_span_fraction = None;
        off.geometry.wing.te_break_2_chord_offset_m = None;
        for (label, preset) in [("break on ", on), ("break off", &off)] {
            let parts = presets::preset_breakdown(preset).unwrap();
            let planform = preset
                .geometry
                .wing
                .transport_planform(&preset.design_vector)
                .unwrap();
            println!(
                "{name} {label}: S_drawn {:.3} clean {:.4} flap_inc {:.4} slat {:.4} CLmax_TO {:.4} landing {:.4}",
                planform.reference_area_m2(),
                parts.clean,
                parts.flap_increment(),
                parts.leading_edge,
                parts.takeoff(),
                parts.landing()
            );
        }
    }
}
