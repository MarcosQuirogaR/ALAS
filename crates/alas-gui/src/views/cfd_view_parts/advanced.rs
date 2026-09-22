// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Advanced tab: one dense case-setup card whose mesh and solver groups reflow
//! into as many readable columns as the window allows, followed by the typed
//! effective configuration written to `study.json`, packed across the full
//! card width with its schema detail in named expandable blocks.

use super::layout::{
    adaptive_field, field, field_enabled, scientific_field, show_groups, Group, FIELD_WIDTH,
};
use super::study::show_regime_flags;
use super::widgets::{card, details, physical_value, value_table};
use crate::state::AppState;
use crate::views::{tr, tr_fields};
use alas_cfd::{CfdStudyConfig, ConvectionScheme, MeshPreset};
use egui::{ComboBox, DragValue, Grid, RichText, ScrollArea, Ui};

/// Mesh and solver controls in one responsive row of labelled groups: six
/// narrow groups fill a wide window instead of three wide groups leaving most
/// of two cards empty, and they stack in the same order when the window is
/// narrow.
const SETUP_GROUPS: &[Group<CfdStudyConfig>] = &[
    (
        "Mesh resolution",
        "The topology is versioned and regenerated when the section or compatible settings change.",
        mesh_resolution_group,
    ),
    (
        "Domain extents [c]",
        "Far-field box measured in chords from the section.",
        domain_extent_group,
    ),
    (
        "Prism layers",
        "Gmsh generates the selected boundary-layer prism stack when enabled; qualification uses achieved y+ and sensitivity evidence, not the configured target alone.",
        prism_layer_group,
    ),
    (
        "Solver safeguards",
        "Residual, force-stability, and continuity thresholds classify numerical status and remain visible as diagnostics.",
        solver_safeguard_group,
    ),
    (
        "Operational Limits",
        "Iteration ceiling, per-utility timeout and write cadence for the isolated case.",
        solver_limit_group,
    ),
    (
        "Final convection scheme",
        "Scheme used after the upwind startup stage; startup iterations only apply to a higher-order final scheme.",
        solver_scheme_group,
    ),
];

pub(crate) fn show_advanced_tab(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_advanced_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_setup_card(&mut state.cfd.config, ui);
            ui.add_space(8.0);
            show_effective_configuration(&state.cfd.config, ui);
        });
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
    }
}

fn show_setup_card(config: &mut CfdStudyConfig, ui: &mut Ui) {
    card(
        ui,
        "Mesh and solver settings",
        "The topology is versioned and regenerated when the section or compatible settings change. Finite native force histories and sweep points remain inspectable when a run is labelled unconverged.",
        |ui| {
            show_groups(ui, config, SETUP_GROUPS);
            if config.mesh.boundary_layers {
                ui.add_space(4.0);
                ui.colored_label(
                    crate::theme::success_color(ui.visuals()),
                    tr("Boundary-layer prisms are generated; qualify wall resolution with the solved y+ statistics in Results."),
                )
                .on_hover_text(tr(
                    "Boundary-layer prisms are generated. Treat the configured target as sizing input; use the solved y+ statistics in Results to qualify wall resolution.",
                ));
            }
        },
    );
}

fn mesh_resolution_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    let mesh = &mut config.mesh;
    Grid::new("airfoil_cfd_mesh_resolution_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Mesh preset"));
            ComboBox::from_id_salt("airfoil_cfd_mesh_preset")
                .width(FIELD_WIDTH)
                .selected_text(match mesh.preset {
                    MeshPreset::Coarse => tr("Coarse"),
                    MeshPreset::Medium => tr("Medium"),
                    MeshPreset::Fine => tr("Fine"),
                })
                .show_ui(ui, |ui| {
                    for preset in [MeshPreset::Coarse, MeshPreset::Medium, MeshPreset::Fine] {
                        let label = match preset {
                            MeshPreset::Coarse => tr("Coarse"),
                            MeshPreset::Medium => tr("Medium"),
                            MeshPreset::Fine => tr("Fine"),
                        };
                        ui.selectable_value(&mut mesh.preset, preset, label);
                    }
                });
            ui.end_row();
            ui.label(tr("Wake refinement"));
            field(ui, DragValue::new(&mut mesh.wake_refinement).range(0..=6));
            ui.end_row();
        });
}

fn domain_extent_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    let mesh = &mut config.mesh;
    Grid::new("airfoil_cfd_mesh_domain_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Upstream extent [c]"));
            field(
                ui,
                DragValue::new(&mut mesh.upstream_chords)
                    .speed(0.5)
                    .range(2.0..=100.0),
            );
            ui.end_row();
            ui.label(tr("Downstream extent [c]"));
            field(
                ui,
                DragValue::new(&mut mesh.downstream_chords)
                    .speed(0.5)
                    .range(5.0..=200.0),
            );
            ui.end_row();
            ui.label(tr("Half-height [c]"));
            field(
                ui,
                DragValue::new(&mut mesh.half_height_chords)
                    .speed(0.5)
                    .range(2.0..=100.0),
            );
            ui.end_row();
        });
}

fn prism_layer_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    let mesh = &mut config.mesh;
    ui.checkbox(&mut mesh.boundary_layers, tr("Enable boundary layers"));
    Grid::new("airfoil_cfd_mesh_prism_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Prism layers"));
            field_enabled(
                ui,
                mesh.boundary_layers,
                DragValue::new(&mut mesh.n_layers).range(1..=30),
            );
            ui.end_row();
            ui.label(tr("First layer height [m]"));
            scientific_field(
                ui,
                DragValue::new(&mut mesh.first_layer_height_m)
                    .speed(1.0e-6)
                    .range(1.0e-9..=1.0),
            );
            ui.end_row();
            ui.label(tr("Target y+"));
            field(
                ui,
                DragValue::new(&mut mesh.target_y_plus)
                    .speed(1.0)
                    .range(1.0..=300.0),
            );
            ui.end_row();
        });
}

fn solver_safeguard_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    // Read the preset before borrowing the solver: the automatic pressure
    // tolerance is resolved against it.
    let preset = config.mesh.preset;
    let solver = &mut config.solver;
    let resolved = solver.effective_pressure_relative_tolerance(preset);
    Grid::new("airfoil_cfd_solver_safeguard_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Residual tolerance"));
            scientific_field(
                ui,
                DragValue::new(&mut solver.residual_tolerance)
                    .speed(1.0e-6)
                    .range(1.0e-12..=1.0e-1),
            );
            ui.end_row();
            ui.label(tr("Force stabilization tolerance"));
            field(
                ui,
                DragValue::new(&mut solver.force_tolerance)
                    .speed(0.001)
                    .range(1.0e-5..=1.0),
            );
            ui.end_row();
            ui.label(tr("Mass-balance tolerance"));
            scientific_field(
                ui,
                DragValue::new(&mut solver.mass_balance_tolerance)
                    .speed(1.0e-6)
                    .range(1.0e-10..=1.0),
            );
            ui.end_row();
            ui.label(tr("Force-history window"));
            field(ui, DragValue::new(&mut solver.force_window).range(3..=500));
            ui.end_row();
            show_pressure_relative_tolerance(ui, solver, preset, resolved);
        });
}

/// The inner pressure relative tolerance: an optional override over a
/// per-preset automatic policy.
///
/// `None` is not a missing value, it is "follow the mesh preset", so the row
/// shows the automatic checkbox and the value the backend actually resolves
/// for the selected preset rather than a blank or a fabricated zero. Ticking
/// the box clears the override; clearing it seeds the editor with the resolved
/// value so the first drag starts from what the run would have used. The
/// policy itself is never reimplemented here:
/// `SolverSettings::effective_pressure_relative_tolerance` is the only source.
fn show_pressure_relative_tolerance(
    ui: &mut Ui,
    solver: &mut alas_cfd::SolverSettings,
    preset: MeshPreset,
    resolved: f64,
) {
    let mut automatic = solver.pressure_relative_tolerance.is_none();
    ui.label(tr("Pressure relative tolerance"))
        .on_hover_text(tr("Relative tolerance of the inner pressure solve. Leave automatic to take the measured per-preset value; an explicit override is always kept, including one loaded from an older study."));
    if ui
        .checkbox(&mut automatic, tr("Automatic"))
        .on_hover_text(tr("Automatic resolves per mesh preset from the values measured at all three shipped presets. Switching preset then changes the effective value; an explicit override does not follow the preset."))
        .changed()
    {
        solver.pressure_relative_tolerance = if automatic { None } else { Some(resolved) };
    }
    ui.end_row();
    match solver.pressure_relative_tolerance.as_mut() {
        Some(explicit) => {
            ui.label(RichText::new(tr("Override")).weak());
            adaptive_field(
                ui,
                DragValue::new(explicit).speed(0.005).range(1.0e-6..=0.5),
            );
        }
        None => {
            ui.label(RichText::new(tr("Resolved")).weak());
            ui.monospace(physical_value(resolved))
                .on_hover_text(tr_fields(
                    "Automatic for the {preset} preset.",
                    &[("preset", format!("{preset:?}"))],
                ));
        }
    }
    ui.end_row();
}

fn solver_limit_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    let solver = &mut config.solver;
    Grid::new("airfoil_cfd_solver_limit_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Maximum SIMPLE iterations"));
            field(
                ui,
                DragValue::new(&mut solver.max_iterations).range(10..=100_000),
            );
            ui.end_row();
            ui.label(tr("Utility timeout [s]"));
            field(
                ui,
                DragValue::new(&mut solver.timeout_seconds).range(1..=86_400),
            );
            ui.end_row();
            ui.label(tr("Write interval"));
            field(
                ui,
                DragValue::new(&mut solver.write_interval).range(1..=100_000),
            );
            ui.end_row();
        });
}

fn solver_scheme_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    let solver = &mut config.solver;
    // The scheme name is long, so it gets its own full-column row: pairing it
    // with a label would push the editor past a narrow column.
    ComboBox::from_id_salt("airfoil_cfd_convection_scheme")
        .width((ui.available_width() - 8.0).clamp(140.0, 280.0))
        .selected_text(tr(solver.convection_scheme.as_str()))
        .show_ui(ui, |ui| {
            for scheme in [
                ConvectionScheme::BoundedUpwind,
                ConvectionScheme::BoundedLinearUpwind,
            ] {
                ui.selectable_value(&mut solver.convection_scheme, scheme, tr(scheme.as_str()));
            }
        });
    Grid::new("airfoil_cfd_solver_scheme_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Upwind startup iterations"));
            field_enabled(
                ui,
                solver.convection_scheme != ConvectionScheme::BoundedUpwind,
                DragValue::new(&mut solver.startup_iterations).range(0..=100_000),
            );
            ui.end_row();
        });
}

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
fn show_effective_configuration(config: &CfdStudyConfig, ui: &mut Ui) {
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
