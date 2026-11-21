//! Local probe: payload-adjusted design mission of the preset's own design vector.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::unwrap_used,
    missing_docs
)]
use alas_config::{presets, AlasConfig};
use alas_opt::assess_product_candidate;
use serde_json::json;

fn main() {
    for name in std::env::args().skip(1) {
        let config = AlasConfig::from_value(&json!({"preset": name, "optimizer": {"objective": {"mtow_sizing": "payload_adjusted"}}})).unwrap();
        let preset = presets::get(&name).unwrap();
        let m = config.design_mission();
        println!(
            "{name}: design range {:?} payload {:?} reserve {:?}",
            m.range, m.payload_kg, m.reserve
        );
        match assess_product_candidate(&config, &preset.design_vector) {
            Ok(a) => println!(
                "{name}: MTOW {:.0} kg ramp {:.0} design-mission takeoff fuel {:.0} trip {:.0} usable cap {:.0} L/D {:.3} closed {} oew-state-free hard_viol {:.3}",
                a.sized.takeoff_mass_kg, a.sized.ramp_fuel_kg, a.sized.design_mission_fuel_kg, a.sized.design_mission_trip_fuel_kg,
                a.sized.usable_capacity_kg, a.sized.lift_to_drag, a.sized.sizing_closed, a.hard_violation_sum
            ),
            Err(e) => println!("{name}: ERROR {e}"),
        }
    }
}
