// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render a compact V2 adapter-request artifact for direct protocol review.

use alas_aero::flowunsteady::{
    render_request, FlowUnsteadyControlSurface, FlowUnsteadyFlightCondition, FlowUnsteadyRequest,
    FlowUnsteadySection, FlowUnsteadySolverRequest, FlowUnsteadySurface,
};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = FlowUnsteadyRequest {
        area_m2: 100.0,
        chord_m: 4.0,
        span_m: 30.0,
        moment_reference_m: [1.0, 0.0, 0.0],
        lifting_surfaces: vec![FlowUnsteadySurface {
            name: "Main Wing".to_owned(),
            symmetric_about_xz: true,
            sections: vec![FlowUnsteadySection {
                leading_edge_m: [2.0, 0.0, 0.0],
                chord_m: 5.0,
                twist_deg: 1.0,
                airfoil_name: "naca2412".to_owned(),
                airfoil_coordinates: vec![(1.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
            }],
        }],
        controls: vec![FlowUnsteadyControlSurface {
            role: "flap",
            surface_name: "Main Wing",
            edge: "trailing",
            chord_fraction: 0.25,
            span_start_fraction: 0.1,
            span_end_fraction: 0.6,
            deflection_deg: 0.0,
            applied_to_geometry: false,
        }],
        flight_condition: FlowUnsteadyFlightCondition {
            altitude_m: 11_000.0,
            pressure_pa: 22_632.0,
            temperature_k: 216.65,
            density_kg_m3: 0.364,
            speed_of_sound_m_s: 295.0,
            true_airspeed_m_s: 236.0,
            mach: 0.8,
            beta_deg: 0.0,
            angular_rates_rad_s: [0.0, 0.0, 0.0],
        },
        solver: FlowUnsteadySolverRequest {
            model: "unsteady_vortex_lattice",
            steps_per_reference_chord: 20,
            wake_age_reference_chords: 20.0,
            settling_reference_chords: 10.0,
            averaging_reference_chords: 5.0,
        },
        alpha_deg: vec![-2.0, 2.0],
    };
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(render_request(&request)?.as_bytes())?;
    Ok(())
}
