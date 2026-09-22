// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What one propulsion-deck evaluation costs, turboprop against turbofan.
//!
//! The turboprop kernel solves two bisections per call (the propeller
//! governor over blade angle, and the actuator-disk thrust bound) where the
//! turbofan deck evaluates closed forms. A mission integration calls the deck
//! many times per candidate, so this is the propulsion path's whole possible
//! contribution to a per-evaluation cost, measured rather than assumed.
//!
//! Usage:
//!
//! ```text
//! cargo run --release -p alas-prop --example deck_cost_probe -- [repeats]
//! ```

#![allow(clippy::print_stdout)]

use std::time::Instant;

use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
};

fn main() {
    let repeats: u32 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);

    let model = Pw127m568fModel::default();
    let command = TurbopropCommand {
        rating: Pw127mRating::MaximumCruise,
        power_fraction: 0.85,
        mode: TurbopropMode::Governed,
        propeller_speed_rpm: model.governed_propeller_speed_rpm,
    };
    // A cruise point, where both bisections run: the governor finds a blade
    // angle and the momentum bound is the binding one.
    let condition = TurbopropCondition {
        density_kg_m3: 0.728_5,
        true_airspeed_m_s: 141.5,
    };
    let warm = model.evaluate(condition, command);
    let started = Instant::now();
    for _ in 0..repeats {
        let _ = model.evaluate(condition, command);
    }
    let mean_s = started.elapsed().as_secs_f64() / f64::from(repeats);

    println!("turboprop deck, governed cruise point:");
    println!(
        "  status        {}",
        if warm.is_ok() { "evaluated" } else { "refused" }
    );
    println!("  mean_us       {:.3}", mean_s * 1.0e6);
    println!("  evaluations/s {:.0}", 1.0 / mean_s);
    if let Ok(output) = warm {
        println!("  thrust_n      {:.1}", output.propeller_thrust_n);
        println!("  eta_p         {:.4}", output.propulsive_efficiency);
    }
    println!();
    println!(
        "A mission integration of {} points therefore costs {:.1} ms of propulsion deck.",
        2_000,
        2_000.0 * mean_s * 1.0e3
    );
}
