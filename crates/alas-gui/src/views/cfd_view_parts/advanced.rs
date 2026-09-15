// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Advanced tab: mesh and solver controls grouped by purpose, followed by the
//! typed effective configuration that the case builder writes to `study.json`.

use super::layout::{show_groups, Group};
use super::study::show_regime_flags;
use crate::state::AppState;
use crate::views::tr;
use alas_cfd::{CfdStudyConfig, ConvectionScheme, MeshPreset, MeshSettings, SolverSettings};
use egui::{ComboBox, DragValue, Grid, RichText, ScrollArea, Ui};

/// Mesh controls, grouped: sizing, domain extents and the near-wall stack.
const MESH_GROUPS: &[Group<MeshSettings>] = &[
    ("Mesh resolution", mesh_resolution_group),
    ("Domain extents [c]", domain_extent_group),
    ("Prism layers", prism_layer_group),
];

/// Solver controls, grouped: acceptance thresholds, run limits and schemes.
const SOLVER_GROUPS: &[Group<SolverSettings>] = &[
    ("Solver safeguards", solver_safeguard_group),
    ("Operational Limits", solver_limit_group),
    ("Final convection scheme", solver_scheme_group),
];

pub(crate) fn show_advanced_tab(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_advanced_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_mesh_card(&mut state.cfd.config.mesh, ui);
            ui.add_space(8.0);
            show_solver_card(&mut state.cfd.config.solver, ui);
            ui.add_space(8.0);
            show_effective_configuration(&state.cfd.config, ui);
        });
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
    }
}

fn show_mesh_card(mesh: &mut MeshSettings, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Mesh settings")).strong().size(16.0));
        ui.label(
            RichText::new(tr("The topology is versioned and regenerated when the section or compatible settings change. Gmsh generates the selected boundary-layer prism stack when enabled; qualification uses achieved y+ and sensitivity evidence, not the configured target alone."))
                .weak()
                .small(),
        );
        ui.add_space(4.0);
        show_groups(ui, mesh, MESH_GROUPS);
        if mesh.boundary_layers {
            ui.add_space(4.0);
            ui.colored_label(
                crate::theme::success_color(ui.visuals()),
                tr("Boundary-layer prisms are generated. Treat the configured target as sizing input; use the solved y+ statistics in Results to qualify wall resolution."),
            );
        }
    });
}

fn mesh_resolution_group(ui: &mut Ui, mesh: &mut MeshSettings) {
    Grid::new("airfoil_cfd_mesh_resolution_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Mesh preset"));
            ComboBox::from_id_salt("airfoil_cfd_mesh_preset")
                .selected_text(match mesh.preset {
                    MeshPreset::Coarse => tr("Coarse"),
                    MeshPreset::Medium => tr("Medium"),
                    MeshPreset::Fine => tr("Fine"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mesh.preset, MeshPreset::Coarse, tr("Coarse"));
                    ui.selectable_value(&mut mesh.preset, MeshPreset::Medium, tr("Medium"));
                    ui.selectable_value(&mut mesh.preset, MeshPreset::Fine, tr("Fine"));
                });
            ui.end_row();
            ui.label(tr("Wake refinement"));
            ui.add(DragValue::new(&mut mesh.wake_refinement).range(0..=6));
            ui.end_row();
        });
}

fn domain_extent_group(ui: &mut Ui, mesh: &mut MeshSettings) {
    Grid::new("airfoil_cfd_mesh_domain_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Upstream extent [c]"));
            ui.add(
                DragValue::new(&mut mesh.upstream_chords)
                    .speed(0.5)
                    .range(2.0..=100.0),
            );
            ui.end_row();
            ui.label(tr("Downstream extent [c]"));
            ui.add(
                DragValue::new(&mut mesh.downstream_chords)
                    .speed(0.5)
                    .range(5.0..=200.0),
            );
            ui.end_row();
            ui.label(tr("Half-height [c]"));
            ui.add(
                DragValue::new(&mut mesh.half_height_chords)
                    .speed(0.5)
                    .range(2.0..=100.0),
            );
            ui.end_row();
        });
}

fn prism_layer_group(ui: &mut Ui, mesh: &mut MeshSettings) {
    ui.checkbox(&mut mesh.boundary_layers, tr("Enable boundary layers"));
    Grid::new("airfoil_cfd_mesh_prism_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Prism layers"));
            ui.add_enabled(
                mesh.boundary_layers,
                DragValue::new(&mut mesh.n_layers).range(1..=30),
            );
            ui.end_row();
            ui.label(tr("First layer height [m]"));
            ui.add(
                DragValue::new(&mut mesh.first_layer_height_m)
                    .speed(1.0e-6)
                    .range(1.0e-9..=1.0),
            );
            ui.end_row();
            ui.label(tr("Target y+"));
            ui.add(
                DragValue::new(&mut mesh.target_y_plus)
                    .speed(1.0)
                    .range(1.0..=300.0),
            );
            ui.end_row();
        });
}

fn show_solver_card(solver: &mut SolverSettings, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Solver and resource settings")).strong().size(16.0));
        ui.add_space(4.0);
        show_groups(ui, solver, SOLVER_GROUPS);
        ui.add_space(4.0);
        ui.label(RichText::new(tr("Residual, force-stability, and continuity thresholds classify numerical status and remain visible as diagnostics. Finite native force histories and sweep points remain inspectable when a run is labelled unconverged." )).weak().small());
    });
}

fn solver_safeguard_group(ui: &mut Ui, solver: &mut SolverSettings) {
    Grid::new("airfoil_cfd_solver_safeguard_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Residual tolerance"));
            ui.add(
                DragValue::new(&mut solver.residual_tolerance)
                    .speed(1.0e-6)
                    .range(1.0e-12..=1.0e-1),
            );
            ui.end_row();
            ui.label(tr("Force stabilization tolerance"));
            ui.add(
                DragValue::new(&mut solver.force_tolerance)
                    .speed(0.001)
                    .range(1.0e-5..=1.0),
            );
            ui.end_row();
            ui.label(tr("Mass-balance tolerance"));
            ui.add(
                DragValue::new(&mut solver.mass_balance_tolerance)
                    .speed(1.0e-6)
                    .range(1.0e-10..=1.0),
            );
            ui.end_row();
            ui.label(tr("Force-history window"));
            ui.add(DragValue::new(&mut solver.force_window).range(3..=500));
            ui.end_row();
        });
}

fn solver_limit_group(ui: &mut Ui, solver: &mut SolverSettings) {
    Grid::new("airfoil_cfd_solver_limit_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Maximum SIMPLE iterations"));
            ui.add(DragValue::new(&mut solver.max_iterations).range(10..=100_000));
            ui.end_row();
            ui.label(tr("Utility timeout [s]"));
            ui.add(DragValue::new(&mut solver.timeout_seconds).range(1..=86_400));
            ui.end_row();
            ui.label(tr("Write interval"));
            ui.add(DragValue::new(&mut solver.write_interval).range(1..=100_000));
            ui.end_row();
        });
}

fn solver_scheme_group(ui: &mut Ui, solver: &mut SolverSettings) {
    ComboBox::from_id_salt("airfoil_cfd_convection_scheme")
        .selected_text(tr(solver.convection_scheme.as_str()))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut solver.convection_scheme,
                ConvectionScheme::BoundedUpwind,
                tr(ConvectionScheme::BoundedUpwind.as_str()),
            );
            ui.selectable_value(
                &mut solver.convection_scheme,
                ConvectionScheme::BoundedLinearUpwind,
                tr(ConvectionScheme::BoundedLinearUpwind.as_str()),
            );
        });
    Grid::new("airfoil_cfd_solver_scheme_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("Upwind startup iterations"));
            ui.add_enabled(
                solver.convection_scheme != ConvectionScheme::BoundedUpwind,
                DragValue::new(&mut solver.startup_iterations).range(0..=100_000),
            );
            ui.end_row();
        });
}

fn vector3(value: [f64; 3]) -> String {
    format!("({:.6}, {:.6}, {:.6})", value[0], value[1], value[2])
}

fn effective_row(ui: &mut Ui, label: &str, value: String) {
    ui.label(tr(label));
    ui.monospace(value);
    ui.end_row();
}

/// The typed effective configuration: reference conventions, derived flow
/// state, boundary table and regime flags, all resolved without launching a
/// process.  The same struct is written to `study.json` by the case builder.
fn show_effective_configuration(config: &CfdStudyConfig, ui: &mut Ui) {
    let effective = config.effective_configuration();
    let reference = &effective.reference;
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Effective configuration")).strong().size(16.0));
        ui.label(
            RichText::new(tr(
                "Resolved from the current inputs before any process is launched. The same values are written to the case dictionaries and study.json.",
            ))
            .weak()
            .small(),
        );
        Grid::new("airfoil_cfd_effective_grid")
            .num_columns(2)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                effective_row(ui, "Template", effective.template_version.clone());
                effective_row(ui, "Airfoil", effective.airfoil_name.clone());
                effective_row(ui, "Reference chord lRef [m]", format!("{:.6}", reference.chord_m));
                effective_row(ui, "Extrusion span [m]", format!("{:.6e}", reference.span_m));
                effective_row(ui, "Reference area Aref [m\u{00b2}]", format!("{:.6e}", reference.area_m2));
                effective_row(ui, "Freestream velocity [m/s]", vector3(reference.freestream_velocity_m_s));
                effective_row(ui, "Dynamic pressure q [Pa]", format!("{:.4}", reference.dynamic_pressure_pa));
                effective_row(ui, "Reynolds number", format!("{:.4e}", effective.reynolds));
                effective_row(ui, "Mach number", format!("{:.4} (a = {:.2} m/s, T = {:.2} K)", effective.mach, effective.speed_of_sound_m_s, effective.temperature_k));
                effective_row(ui, "Kinematic viscosity nu [m\u{00b2}/s]", format!("{:.6e}", effective.kinematic_viscosity_m2_s));
                effective_row(ui, "Drag direction", vector3(reference.drag_direction));
                effective_row(ui, "Lift direction", vector3(reference.lift_direction));
                effective_row(ui, "Moment reference [m]", vector3(reference.moment_reference_m));
                effective_row(ui, "Pitch axis", format!("{} Cm > 0 nose-up", vector3(reference.pitch_axis)));
                effective_row(ui, "Kinematic pressure reference p/rho [m\u{00b2}/s\u{00b2}]", format!("{:.6}", effective.pressure_reference_kinematic_m2_s2));
                effective_row(ui, "Turbulence model", effective.turbulence_model.clone());
                effective_row(ui, "Inflow k [m\u{00b2}/s\u{00b2}]", format!("{:.6e}", effective.turbulence.k_m2_s2));
                effective_row(ui, "Inflow omega [1/s]", format!("{:.6e}", effective.turbulence.omega_s_inv));
                effective_row(ui, "Eddy viscosity ratio nu_t/nu", format!("{:.6e}", effective.turbulence.nu_t_over_nu));
                effective_row(ui, "Domain extents [c]", format!("{:.1} upstream, {:.1} downstream, {:.1} half-height", effective.domain_extents_chords[0], effective.domain_extents_chords[1], effective.domain_extents_chords[2]));
                effective_row(ui, "Mesh preset", format!("{:?}", effective.mesh_preset));
                effective_row(ui, "Maximum iterations", effective.max_iterations.to_string());
                effective_row(ui, "Final convection scheme", tr(effective.convection_scheme.as_str()));
            });
        ui.add_space(6.0);
        ui.label(RichText::new(tr("Boundary conditions")).strong());
        Grid::new("airfoil_cfd_boundary_grid")
            .num_columns(7)
            .striped(true)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                ui.label(RichText::new(tr("Patch")).strong());
                ui.label(RichText::new(tr("Role")).strong());
                for field in ["U", "p", "k", "omega", "nut"] {
                    ui.label(RichText::new(field).strong());
                }
                ui.end_row();
                for patch in &effective.boundaries {
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
        ui.add_space(6.0);
        show_regime_flags(&effective.regime, ui);
    });
}
