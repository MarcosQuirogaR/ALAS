// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion-side budget of the ATR 72-600 climb:
//! shaft power available at maximum-climb rating, the propeller efficiency and
//! the propeller thrust at the 170 KCAS climb schedule, against the PW120 chart
//! fit the model's lapse rests on. No airframe drag enters; this is the
//! thrust side only.
//!
//! `cargo run -p alas-prop --example turboprop_climb_budget`

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
};

const KNOT_M_S: f64 = 0.514_444;
const SHP_W: f64 = 745.699_872;

fn isa_density(height_m: f64) -> f64 {
    1.225 * (1.0 - 2.255_77e-5 * height_m).powf(4.255_88)
}

fn main() {
    let model = Pw127m568fModel::default();
    println!(
        "{:>7} {:>6} {:>7} {:>8} {:>8} {:>8} {:>7} {:>8} {:>9} {:>9}",
        "h [ft]",
        "sigma",
        "TAS",
        "P alt.",
        "P now",
        "P chart",
        "eta",
        "T prop",
        "T*V/Pshaft",
        "(per eng.)"
    );
    for height_ft in [0.0, 3_000.0, 6_000.0, 10_000.0, 14_000.0, 17_000.0] {
        let height_m = height_ft * 0.3048;
        let rho = isa_density(height_m);
        let sigma = rho / 1.225;
        let tas = 170.0 * KNOT_M_S / sigma.sqrt();
        // Old form: the climb ceiling multiplied by the maximum-continuous lapse.
        let lapse_mct = model.power_lapse_fraction(Pw127mRating::MaximumContinuous, rho);
        // Now: the climb rating, flat until it meets the maximum-continuous line.
        let old_shp = model.available_shaft_power_w(Pw127mRating::MaximumClimb, rho) / SHP_W;
        // Alternative tried and rejected: the lesser of the ceiling and the continuous capability.
        let new_shp = model
            .maximum_climb_power_w
            .min(model.maximum_continuous_power_w * lapse_mct)
            / SHP_W;
        // Nita (2008) eq. 3.5.11 on McCormick's PW120 maximum-cruise chart,
        // P_CR / P_TO = 0.9 sigma^0.728, with the normal take-off rating.
        let chart_shp = 0.9 * 2_475.0 * sigma.powf(0.728);
        let out = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: rho,
                    true_airspeed_m_s: tas,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumClimb,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .expect("climb point");
        println!(
            "{height_ft:>7.0} {sigma:>6.3} {tas:>7.1} {new_shp:>8.0} {old_shp:>8.0} {chart_shp:>8.0} {:>7.3} {:>8.0} {:>9.3}",
            out.propulsive_efficiency,
            out.propeller_thrust_n,
            out.propeller_thrust_n * tas / out.engine_shaft_power_w,
        );
    }
}
