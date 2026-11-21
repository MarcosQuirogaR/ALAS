//! Per-phase trip fuel, speed, L/D and implied TSFC of a preset's design vector (diagnostic).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::unwrap_used,
    missing_docs
)]
use alas_config::{presets, AlasConfig};
use alas_opt::assess_product_candidate;
use alas_opt::mdo::candidate_mission_model;
use alas_opt::mdo::mission_model::StepPhase;
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for pair in args.chunks(2) {
        let name = &pair[0];
        let range_nmi: f64 = pair[1].parse().unwrap();
        let mut cfg = json!({"preset": name, "optimizer": {"objective": {"mtow_sizing": "payload_adjusted"}}});
        if let Ok(extra) = std::env::var("EXTRA") {
            let v: serde_json::Value = serde_json::from_str(&extra).unwrap();
            for (k, val) in v.as_object().unwrap() {
                cfg[k] = val.clone();
            }
        }
        let config = AlasConfig::from_value(&cfg).unwrap();
        let preset = presets::get(name).unwrap();
        let a = assess_product_candidate(&config, &preset.design_vector).unwrap();
        let model = candidate_mission_model(&config, &a.sized.fuel_artifacts).unwrap();
        let m0 = a.sized.takeoff_mass_kg;
        if let Some(p) = &a.sized.fuel_artifacts.frozen_plan {
            println!(
                "  plan scales {:?} cruise alt {:.0}",
                p.climb_rate_scales, p.cruise_altitude_m
            );
        }
        println!(
            "  profile rates: init {} s1 {} s2 {}",
            config.mission.profile.initial_climb_rate_m_s,
            config.mission.profile.step_climb_1_rate_m_s,
            config.mission.profile.step_climb_2_rate_m_s
        );
        let flown = model.fly_trip(m0, range_nmi * 1852.0).unwrap();
        if std::env::var("FLOW_CHECK").is_ok() {
            use alas_mass::fuel_plan::FuelBurnModel;
            for (mach, mass, label) in [
                (
                    0.775,
                    8437.0,
                    "FL430 M0.775 18600 lb ISA-7 (AOPA 1040 lb/h = 471.7 kg/h)",
                ),
                (
                    0.74,
                    8437.0,
                    "FL430 M0.74 18600 lb ISA-7 (AOPA LRC 840 lb/h = 381.0 kg/h)",
                ),
            ] {
                let mut m = model.clone();
                m.cruise_altitude_m = 13106.0;
                m.cruise_mach = mach;
                m.isa_deviation_c = -7.0;
                m.cruise_tas_m_s = m.cruise_true_airspeed_m_s(13106.0).unwrap();
                match m.cruise_fuel_flow_kg_s(mass) {
                    Ok(ff) => println!(
                        "  FLOWCHECK {label}: model {:.1} kg/h ({:.0} lb/h)",
                        ff * 3600.0,
                        ff * 3600.0 / 0.45359237
                    ),
                    Err(e) => println!("  FLOWCHECK {label}: error {e}"),
                }
            }
        }
        println!("{name}: m0 {m0:.0} range {range_nmi} nmi trip fuel {:.0} kg time {:.0} min climb fuel {:.0} kg climb time {:.0} min footprint {:.0} km desc footprint {:.0} km", flown.leg.fuel_kg, flown.leg.time_s/60.0, flown.climb_fuel_kg, flown.climb_time_s/60.0, flown.climb_footprint_m/1e3, flown.descent_footprint_m/1e3);
        for (label, ph) in [
            ("takeoff", StepPhase::Takeoff),
            ("climb", StepPhase::Climb),
            ("cruise", StepPhase::Cruise),
            ("descent", StepPhase::Descent),
            ("landing", StepPhase::Landing),
            ("ground", StepPhase::Ground),
        ] {
            let (mut fuel, mut t, mut d, mut thr_t, mut ld_t, mut mach_t, mut drag_t) =
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for s in flown.steps.iter().filter(|s| s.phase == ph) {
                let dt = s.time_s[1] - s.time_s[0];
                fuel += s.mass_kg[0] - s.mass_kg[1];
                t += dt;
                d += s.range_m[1] - s.range_m[0];
                thr_t += s.thrust_n * dt;
                drag_t += s.drag_n * dt;
                ld_t += s.lift_to_drag * dt;
                mach_t += s.mach * dt;
            }
            if t > 0.0 {
                let tsfc = fuel / thr_t; // kg/(N s)
                println!("  {label:8} fuel {fuel:8.0} kg time {:6.1} min dist {:7.0} km mean M {:.3} mean L/D {:.2} mean thrust {:.0} N mean drag {:.0} N implied TSFC {:.3} kg/(kgf h)", t/60.0, d/1e3, mach_t/t, ld_t/t, thr_t/t, drag_t/t, tsfc*9.80665*3600.0);
            }
        }
        if std::env::var("DUMP_CLIMB").is_ok() {
            for s in flown.steps.iter().filter(|s| s.phase == StepPhase::Climb) {
                println!("  climb h {:.0}-{:.0} m M {:.3} tas {:.1} thrust {:.0} drag {:.0} m {:.0} bound {} roc {:.2} m/s", s.altitude_m[0], s.altitude_m[1], s.mach, s.tas_m_s[0], s.thrust_n, s.drag_n, s.mass_kg[0], s.bound_limited, (s.altitude_m[1]-s.altitude_m[0])/(s.time_s[1]-s.time_s[0]));
            }
        }
        println!("  climb bands {:?}", flown.climb_bands);
    }
}
