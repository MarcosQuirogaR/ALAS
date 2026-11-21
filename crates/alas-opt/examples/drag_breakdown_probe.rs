//! Cruise drag breakdown of a preset's design vector (diagnostic).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::unwrap_used,
    missing_docs
)]
use alas_atmo::Atmosphere;
use alas_config::{presets, AlasConfig};
use alas_opt::assess_product_candidate;
use alas_opt::mdo::CandidateDrag;
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for pair in args.chunks(2) {
        let (name, mach) = (&pair[0], pair[1].parse::<f64>().unwrap());
        let config = AlasConfig::from_value(&json!({"preset": name, "optimizer": {"objective": {"mtow_sizing": "payload_adjusted"}}})).unwrap();
        let preset = presets::get(name).unwrap();
        let a = assess_product_candidate(&config, &preset.design_vector).unwrap();
        let arts = &a.sized.fuel_artifacts;
        let alt = std::env::var("ALT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                arts.frozen_plan
                    .as_ref()
                    .map_or(11000.0, |p| p.cruise_altitude_m)
            });
        let mass = std::env::var("MASS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.93 * a.sized.takeoff_mass_kg);
        let atm = Atmosphere::new(alt);
        let q = 0.5 * atm.density() * (mach * atm.speed_of_sound()).powi(2);
        let cl = mass * 9.80665 / (q * arts.reference_area_m2);
        println!(
            "{name}: M {mach} alt {alt:.0} m mass {mass:.0} S {:.1} CL {cl:.3}",
            arts.reference_area_m2
        );
        if let CandidateDrag::Table(t) = &arts.drag {
            let cd0 = t.cd0(mach, alt);
            let ind = t.induced_cd_at(cl, mach);
            let vis = t.viscous_lift_cd(cl, cd0);
            let up = t.upsweep_attitude_cd(cl, mach);
            let wave = t.wave_cd(cl, mach);
            let cd = cd0 + vis + up + ind + wave;
            println!("  CD0 {cd0:.5} induced {ind:.5} visc_lift {vis:.5} upsweep {up:.5} wave {wave:.5} total {cd:.5} L/D {:.2}", cl / cd);
            for (l, c) in t.parasite_components(mach, alt) {
                println!("    {l}: {c:.5}");
            }
        }
    }
}
