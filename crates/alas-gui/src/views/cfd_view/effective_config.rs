// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The typed effective configuration card: reference conventions, derived flow state and boundary table.

use super::study::show_regime_flags;
use super::widgets::{card, details, physical_value, value_table};
use crate::views::{tr, tr_fields};
use alas_cfd::CfdStudyConfig;
use egui::{Grid, RichText, ScrollArea, Ui};

/// A far-field sub-condition as it will actually be written.
///
/// Both apply only while the far field is `FixedValue`, which is exactly how
/// the Study tab enables them. Showing the stored value alone in the effective
/// table would claim a treatment the case dictionaries will not contain, so a
/// setting that does not apply says so instead of reading as active.
fn far_field_sub_condition(config: &CfdStudyConfig, value: String) -> String {
    if config.boundaries.far_field == alas_cfd::FarFieldCondition::FixedValue {
        value
    } else {
        tr_fields(
            "{value} (not applied: far field is not Fixed velocity)",
            &[("value", value)],
        )
    }
}

/// The resolved inner pressure tolerance and where it came from.
///
/// The effective configuration is what the case builder writes, so a reader
/// has to be able to tell a value that followed the preset policy from one the
/// study asked for: an override survives a preset change and an automatic
/// value does not.
fn pressure_tolerance_summary(config: &CfdStudyConfig) -> String {
    let preset = config.mesh.preset;
    let effective = config.effective_simulation();
    let resolved = physical_value(effective.pressure_relative_tolerance);
    if effective.compressible {
        return tr_fields(
            "{value} (automatic for {solver} at Mach-derived regime)",
            &[
                ("value", resolved),
                ("solver", effective.solver.executable().to_owned()),
            ],
        );
    }
    match config.solver.pressure_relative_tolerance {
        Some(_) => tr_fields("{value} (explicit)", &[("value", resolved)]),
        None => tr_fields(
            "{value} (automatic, {preset})",
            &[("value", resolved), ("preset", format!("{preset:?}"))],
        ),
    }
}

fn vector3(value: [f64; 3]) -> String {
    format!("({:.5}, {:.5}, {:.5})", value[0], value[1], value[2])
}

/// The typed effective configuration: reference conventions, derived flow
/// state, boundary table and regime flags, all resolved without launching a
/// process.  The same struct is written to `study.json` by the case builder.
pub(super) fn show_effective_configuration(config: &CfdStudyConfig, ui: &mut Ui) {
    let effective = config.effective_configuration();
    let reference = &effective.reference;
    card(
        ui,
        "Effective configuration",
        "Resolved from the current inputs before any process is launched. The same values are written to the case dictionaries and study.json.",
        |ui| {
            value_table(
                ui,
                "airfoil_cfd_effective_grid",
                &[
                    ("Airfoil".to_owned(), effective.airfoil_name.clone()),
                    ("Template".to_owned(), effective.template_version.clone()),
                    ("Mesh preset".to_owned(), format!("{:?}", effective.mesh_preset)),
                    ("Reference chord lRef [m]".to_owned(), format!("{:.5}", reference.chord_m)),
                    ("Reference area Aref [m\u{00b2}]".to_owned(), format!("{:.4e}", reference.area_m2)),
                    ("Extrusion span [m]".to_owned(), format!("{:.4e}", reference.span_m)),
                    ("Freestream velocity [m/s]".to_owned(), vector3(reference.freestream_velocity_m_s)),
                    ("Dynamic pressure q [Pa]".to_owned(), format!("{:.4}", reference.dynamic_pressure_pa)),
                    ("Reynolds number".to_owned(), format!("{:.4e}", effective.reynolds)),
                    (
                        "Mach number".to_owned(),
                        format!("{:.4} (a = {:.2} m/s, T = {:.2} K)", effective.mach, effective.speed_of_sound_m_s, effective.temperature_k),
                    ),
                    ("Flow regime".to_owned(), effective.flow_regime.as_str().to_owned()),
                    ("Selected solver".to_owned(), effective.solver.executable().to_owned()),
                    ("Compressible equations".to_owned(), effective.compressible.to_string()),
                    ("Static pressure used [Pa]".to_owned(), format!("{:.2}", effective.static_pressure_pa)),
                    ("Kinematic viscosity nu [m\u{00b2}/s]".to_owned(), format!("{:.4e}", effective.kinematic_viscosity_m2_s)),
                    ("Maximum iterations".to_owned(), effective.max_iterations.to_string()),
                    ("Startup iterations".to_owned(), effective.startup_iterations.to_string()),
                    (
                        "Pressure relative tolerance".to_owned(),
                        pressure_tolerance_summary(config),
                    ),
                    ("Final convection scheme".to_owned(), tr(effective.convection_scheme.as_str())),
                    ("Turbulence convection".to_owned(), tr(effective.turbulence_convection_scheme.as_str())),
                    ("Gradient limiter".to_owned(), format!("{:.3}", effective.gradient_limiter)),
                    ("Pressure relaxation".to_owned(), format!("{:.3}", effective.pressure_relaxation)),
                    ("Momentum relaxation".to_owned(), format!("{:.3}", effective.equation_relaxation)),
                    (
                        "Far-field turbulence".to_owned(),
                        far_field_sub_condition(
                            config,
                            tr(config.boundaries.far_field_turbulence.as_str()),
                        ),
                    ),
                    (
                        "Far-field velocity".to_owned(),
                        far_field_sub_condition(
                            config,
                            tr(config.boundaries.far_field_velocity.as_str()),
                        ),
                    ),
                    (
                        "Domain extents [c]".to_owned(),
                        format!(
                            "{:.1} / {:.1} / {:.1}",
                            effective.domain_extents_chords[0],
                            effective.domain_extents_chords[1],
                            effective.domain_extents_chords[2]
                        ),
                    ),
                ],
            );
            ui.add_space(4.0);
            details(ui, "airfoil_cfd_reference_frame", "Reference frame, turbulence inflow and pressure reference", |ui| {
                value_table(
                    ui,
                    "airfoil_cfd_reference_grid",
                    &[
                        ("Drag direction".to_owned(), vector3(reference.drag_direction)),
                        ("Lift direction".to_owned(), vector3(reference.lift_direction)),
                        ("Moment reference [m]".to_owned(), vector3(reference.moment_reference_m)),
                        ("Pitch axis".to_owned(), format!("{} Cm > 0 nose-up", vector3(reference.pitch_axis))),
                        (
                            "Kinematic pressure reference p/rho [m\u{00b2}/s\u{00b2}]".to_owned(),
                            format!("{:.5}", effective.pressure_reference_kinematic_m2_s2),
                        ),
                        ("Turbulence model".to_owned(), effective.turbulence_model.clone()),
                        ("Inflow k [m\u{00b2}/s\u{00b2}]".to_owned(), format!("{:.4e}", effective.turbulence.k_m2_s2)),
                        ("Inflow omega [1/s]".to_owned(), format!("{:.4e}", effective.turbulence.omega_s_inv)),
                        ("Eddy viscosity ratio nu_t/nu".to_owned(), format!("{:.4e}", effective.turbulence.nu_t_over_nu)),
                    ],
                );
            });
            details(ui, "airfoil_cfd_boundary_schema", "Boundary condition schema written to the case", |ui| {
                show_boundary_schema(&effective.boundaries, ui);
            });
            ui.add_space(4.0);
            show_regime_flags(&effective.regime, ui);
        },
    );
}

fn show_boundary_schema(boundaries: &[alas_cfd::PatchCondition], ui: &mut Ui) {
    ScrollArea::horizontal()
        .id_salt("airfoil_cfd_boundary_scroll")
        .show(ui, |ui| {
            Grid::new("airfoil_cfd_boundary_grid")
                .num_columns(7)
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(tr("Patch")).strong());
                    ui.label(RichText::new(tr("Role")).strong());
                    for field_name in ["U", "p", "k", "omega", "nut"] {
                        ui.label(RichText::new(field_name).strong());
                    }
                    ui.end_row();
                    for patch in boundaries {
                        ui.monospace(&patch.patch);
                        ui.label(tr(&patch.role));
                        ui.monospace(&patch.velocity);
                        ui.monospace(&patch.pressure);
                        ui.monospace(&patch.k);
                        ui.monospace(&patch.omega);
                        ui.monospace(&patch.nut);
                        ui.end_row();
                    }
                });
        });
}
