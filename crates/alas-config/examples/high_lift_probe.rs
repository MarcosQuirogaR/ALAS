// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared high-lift take-off CLmax per preset (diagnostic): the Raymer
//! Eq. 12.21 breakdown, the landing value of the same method and the
//! Vref-recovered landing coefficient it can be compared with.
#![allow(clippy::print_stdout, missing_docs)]

use alas_config::presets;

fn main() {
    println!("preset clean flapSwept flapRatio hingeDeg slatRatio leDeg | flapL slat | TO landing(handbook) landing(Vref) class_TO");
    for preset in presets::registry() {
        let Some(parts) = presets::preset_breakdown(preset) else {
            println!("{}: no declared system", preset.name);
            continue;
        };
        let performance = preset.performance.as_ref();
        println!(
            "{}: clean {:.3} flapL {:.3} slat {:.3} | TO {:.3} landing {:.3} | Vref-landing {:.3} | stored TO {:.3}",
            preset.name,
            parts.clean,
            parts.flap_landing,
            parts.leading_edge,
            parts.takeoff(),
            parts.landing(),
            performance.map_or(f64::NAN, |p| p.cl_max_land),
            performance.map_or(f64::NAN, |p| p.cl_max_to),
        );
    }
}
