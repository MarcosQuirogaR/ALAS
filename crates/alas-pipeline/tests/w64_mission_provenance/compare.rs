// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field-by-field W6.4 condition comparison kept apart from the public path.

use alas_mission::segments::Conditions;
use alas_testkit::Comparison;

use super::PointEvidence;

pub(super) fn compare_point(
    comparison: &mut Comparison,
    prefix: &str,
    actual: &Conditions,
    row: usize,
    expected: &PointEvidence,
    expected_throttle_scale: f64,
) {
    let name = |field: &str| format!("{prefix}/{field}");
    comparison
        .scalar(
            &name("altitude_m"),
            actual.altitude_m[row],
            expected.altitude_m,
        )
        .scalar(
            &name("temperature_k"),
            actual.temperature_k[row],
            expected.temperature_k,
        )
        .scalar(
            &name("pressure_pa"),
            actual.pressure_pa[row],
            expected.pressure_pa,
        )
        .scalar(
            &name("density_kg_m3"),
            actual.density_kg_m3[row],
            expected.density_kg_m3,
        )
        .scalar(
            &name("speed_of_sound_m_s"),
            actual.speed_of_sound_m_s[row],
            expected.speed_of_sound_m_s,
        )
        .scalar(
            &name("dynamic_viscosity_pa_s"),
            actual.dynamic_viscosity_pa_s[row],
            expected.dynamic_viscosity_pa_s,
        )
        .scalar(
            &name("velocity_m_s"),
            actual.velocity_m_s[row],
            expected.velocity_m_s,
        )
        .scalar(&name("mach"), actual.mach[row], expected.mach)
        .scalar(
            &name("reynolds_number_per_m"),
            actual.reynolds_number_per_m[row],
            expected.reynolds_number_per_m,
        )
        .scalar(
            &name("dynamic_pressure_pa"),
            actual.dynamic_pressure_pa[row],
            expected.dynamic_pressure_pa,
        )
        .scalar(
            &name("angle_of_attack_rad"),
            actual.angle_of_attack_rad[row],
            expected.angle_of_attack_rad,
        )
        .scalar(
            &name("body_angle_rad"),
            actual.body_inertial_rotations_rad[row][1],
            expected.body_angle_rad,
        )
        .scalar(
            &name("lift_coefficient"),
            actual.lift_coefficient[row],
            expected.lift_coefficient,
        )
        .scalar(
            &name("drag_coefficient"),
            actual.drag_coefficient[row],
            expected.drag_coefficient,
        )
        .scalar(
            &name("throttle_scale_from_reference"),
            actual.throttle[row] / expected.throttle,
            expected_throttle_scale,
        )
        .scalar(
            &name("thrust_n"),
            actual.thrust_force_vector_n[row][0],
            expected.thrust_n,
        )
        .scalar(
            &name("mass_rate_kg_s"),
            actual.vehicle_mass_rate_kg_s[row],
            expected.mass_rate_kg_s,
        )
        .scalar(
            &name("mass_kg"),
            actual.total_mass_kg[row],
            expected.mass_kg,
        )
        .scalar(
            &name("cd_parasite"),
            actual.drag_breakdown[row].parasite_total,
            expected.cd_parasite,
        )
        .scalar(
            &name("cd_induced"),
            actual.drag_breakdown[row].induced_total,
            expected.cd_induced,
        )
        .scalar(
            &name("cd_compressible"),
            actual.drag_breakdown[row].compressible_total,
            expected.cd_compressible,
        )
        .scalar(
            &name("cd_miscellaneous"),
            actual.drag_breakdown[row].miscellaneous_total,
            expected.cd_miscellaneous,
        )
        .scalar(
            &name("cd_total"),
            actual.drag_breakdown[row].total,
            expected.cd_total,
        );
}
