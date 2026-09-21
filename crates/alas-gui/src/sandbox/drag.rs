// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parameter-based direct manipulation in the sandbox viewport.
//!
//! A handle is a model point attached to one parameter and one model-space
//! direction along which that parameter moves it: the wing tip moves along
//! `+y` and changes the span, the kink leading edge moves along `+x` and
//! changes the sweep, the tip trailing edge moves down and changes the tip
//! twist. Dragging projects the pointer motion onto the handle direction on
//! screen and writes the parameter back through the shared field inventory,
//! so a drag is exactly one field edit with the same validation and the same
//! undo transaction as a typed value. Airfoil shapes are never deformed by
//! dragging; only the assigned profile can change, through the field list.
//!
//! Handle positions come from the analytic planform and the builder's tail
//! and nacelle placement formulas, not from the tessellated mesh.

use alas_config::{AlasConfig, DesignVector, FuselageSection};
use alas_report::families::geometry::SceneFraming;
use serde_json::Value;

use crate::state::AppState;

use super::fields::{self, Discipline, FieldKind, SandboxField};

/// What one handle edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// Wing tip leading edge: span.
    Span,
    /// Wing root trailing edge: root chord.
    RootChord,
    /// Wing break trailing edge: break chord.
    BreakChord,
    /// Wing tip trailing edge: tip chord.
    TipChord,
    /// Wing break leading edge: inboard leading-edge sweep.
    Sweep,
    /// Wing tip trailing edge, vertical: tip twist.
    TipTwist,
    /// Wing root leading edge: longitudinal wing position.
    WingPosition,
    /// Horizontal tail root leading edge: longitudinal tail position.
    TailPosition,
    /// Horizontal tail tip leading edge, spanwise: tail scale.
    TailScale,
    /// Horizontal tail tip leading edge, longitudinal: tail tip sweep.
    TailTipSweep,
    /// Vertical tail tip leading edge, vertical: fin height.
    FinHeight,
    /// Vertical tail tip leading edge, longitudinal: fin tip sweep.
    FinTipSweep,
    /// Fuselage tail end: fuselage length.
    FuselageLength,
    /// Fuselage nose: nose height.
    NoseHeight,
    /// Fuselage cabin crown: diameter.
    FuselageDiameter,
    /// Nacelle mid-body, spanwise: symmetric engine spanwise position.
    NacelleSpan,
    /// Nacelle inlet, longitudinal: inlet offset ahead of the wing.
    NacelleInlet,
    /// Nacelle aft body, vertical: engine vertical offset.
    NacelleHeight,
    /// A user-created wing section's spanwise station.
    CustomWingSectionSpan,
    /// A user-created wing section's leading-edge X position.
    CustomWingSectionLeadingEdge,
    /// A user-created wing section's chord.
    CustomWingSectionChord,
    /// A user-created wing section's vertical position.
    CustomWingSectionHeight,
    /// A user-created wing section's twist.
    CustomWingSectionTwist,
    /// A user-created fuselage section's longitudinal station.
    CustomFuselageSectionStation,
    /// A user-created fuselage section's width.
    CustomFuselageSectionWidth,
    /// A user-created fuselage section's height.
    CustomFuselageSectionHeight,
    /// A user-created fuselage section's vertical position.
    CustomFuselageSectionZ,
    /// A generated fuselage station's longitudinal position.
    GeneratedFuselageSectionStation,
    /// A generated fuselage station's local width.
    GeneratedFuselageSectionWidth,
    /// A generated fuselage station's local height.
    GeneratedFuselageSectionHeight,
    /// A generated fuselage station's vertical position.
    GeneratedFuselageSectionZ,
}

/// One draggable handle.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    /// What it edits.
    pub kind: HandleKind,
    /// The discipline it belongs to.
    pub discipline: Discipline,
    /// The field it writes.
    pub field_id: &'static str,
    /// English label for hover text.
    pub label: &'static str,
    /// The model point the handle sits on.
    pub point: [f64; 3],
    /// Unit model direction of increasing parameter.
    pub axis: [f64; 3],
    /// Parameter units per metre of motion along `axis`.
    pub per_metre: f64,
    /// For `Vec3` fields, the component written.
    pub component: Option<usize>,
    /// Index in a custom or generated section list, when this is a dynamic
    /// section handle. Static planform handles leave this unset.
    pub section_index: Option<usize>,
}

fn handle(
    kind: HandleKind,
    discipline: Discipline,
    field_id: &'static str,
    label: &'static str,
    point: [f64; 3],
    axis: [f64; 3],
    per_metre: f64,
) -> Handle {
    Handle {
        kind,
        discipline,
        field_id,
        label,
        point,
        axis,
        per_metre,
        component: None,
        section_index: None,
    }
}

fn custom_handle(
    kind: HandleKind,
    discipline: Discipline,
    label: &'static str,
    point: [f64; 3],
    axis: [f64; 3],
    per_metre: f64,
    component: usize,
    section_index: usize,
) -> Handle {
    Handle {
        kind,
        discipline,
        // Dynamic section values are routed by `section_index` and
        // `component`; this placeholder keeps the handle self-describing to
        // diagnostics without pretending the field is in the static schema.
        field_id: "geometry.custom_sections",
        label,
        point,
        axis,
        per_metre,
        component: Some(component),
        section_index: Some(section_index),
    }
}

fn generated_fuselage_handle(
    kind: HandleKind,
    label: &'static str,
    point: [f64; 3],
    axis: [f64; 3],
    per_metre: f64,
    field_id: &'static str,
    section_index: usize,
) -> Handle {
    Handle {
        section_index: Some(section_index),
        ..handle(
            kind,
            Discipline::Fuselage,
            field_id,
            label,
            point,
            axis,
            per_metre,
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum GeneratedFuselageStationPart {
    Nose(f64),
    CabinStart,
    CabinEnd,
    Tail(f64),
}

fn generated_fuselage_rows(
    config: &AlasConfig,
    design: &DesignVector,
) -> Vec<(FuselageSection, GeneratedFuselageStationPart)> {
    let fuselage = &config.geometry.fuselage;
    let length_m = design.fuselage_length_m;
    if !(length_m > 0.0) {
        return Vec::new();
    }
    let cabin_start_m = fuselage.cabin_start_x_m;
    let cabin_end_m = length_m - fuselage.tailcone_length_m;
    let radius_m = fuselage.diameter_m / 2.0;
    let height_scale = fuselage.effective_height_m() / fuselage.diameter_m.max(1e-9);
    let mut rows = Vec::with_capacity(20);
    for index in 0..9 {
        let angle = std::f64::consts::FRAC_PI_2 * index as f64 / 9.0;
        let xi = 1.0 - angle.cos();
        let radius_scale = (1.0 - (1.0 - xi).powi(2)).sqrt();
        let width_m = radius_m * radius_scale * 2.0;
        rows.push((
            FuselageSection {
                x_fraction: xi * cabin_start_m / length_m,
                width_m,
                height_m: width_m * height_scale,
                z_m: fuselage.cabin_z_m
                    + (fuselage.nose_z_m - fuselage.cabin_z_m) * (1.0 - xi).powi(2),
                shape: 2.0,
            },
            GeneratedFuselageStationPart::Nose(xi),
        ));
    }
    rows.push((
        FuselageSection {
            x_fraction: cabin_start_m / length_m,
            width_m: radius_m * 2.0,
            height_m: radius_m * 2.0 * height_scale,
            z_m: fuselage.cabin_z_m,
            shape: 2.0,
        },
        GeneratedFuselageStationPart::CabinStart,
    ));
    rows.push((
        FuselageSection {
            x_fraction: cabin_end_m / length_m,
            width_m: radius_m * 2.0,
            height_m: radius_m * 2.0 * height_scale,
            z_m: fuselage.cabin_z_m,
            shape: 2.0,
        },
        GeneratedFuselageStationPart::CabinEnd,
    ));
    for index in 1..10 {
        let xi = index as f64 / 9.0;
        let radius_scale = 1.0 - xi.powf(1.5);
        let width_m = radius_m * radius_scale * 2.0;
        rows.push((
            FuselageSection {
                x_fraction: (cabin_end_m + xi * fuselage.tailcone_length_m) / length_m,
                width_m,
                height_m: width_m * height_scale,
                z_m: fuselage.cabin_z_m + (fuselage.tail_z_m - fuselage.cabin_z_m) * xi.powf(1.5),
                shape: 2.0,
            },
            GeneratedFuselageStationPart::Tail(xi),
        ));
    }
    for (index, override_section) in fuselage.generated_sections.iter().enumerate() {
        let Some((section, _)) = rows.get_mut(index) else {
            break;
        };
        section.width_m = override_section.width_m;
        section.height_m = override_section.height_m;
        section.z_m = override_section.z_m;
        section.shape = override_section.shape;
    }
    rows
}

/// Every handle of the aircraft, or of one discipline when focused.
pub fn handles(
    config: &AlasConfig,
    design: &DesignVector,
    focus: Option<Discipline>,
) -> Vec<Handle> {
    let mut out = Vec::new();
    let g = &config.geometry;
    if let Ok(planform) = g.wing.transport_planform(design) {
        let x0 = g.wing.root_datum_x_m + design.wing_x_shift_m;
        let root = &planform.root;
        let kink = &planform.kink;
        let tip = &planform.tip;
        let wing = Discipline::Wing;
        out.push(handle(
            HandleKind::Span,
            wing,
            "design.span_m",
            "Span",
            [x0 + tip.leading_edge_x_m, tip.y_m, g.wing.tip_z_m],
            [0.0, 1.0, 0.0],
            2.0,
        ));
        out.push(handle(
            HandleKind::RootChord,
            wing,
            "design.root_chord_m",
            "Root chord",
            [
                x0 + root.leading_edge_x_m + root.chord_m,
                root.y_m,
                g.wing.root_z_m,
            ],
            [1.0, 0.0, 0.0],
            1.0,
        ));
        out.push(handle(
            HandleKind::BreakChord,
            wing,
            "design.break_chord_m",
            "Break chord",
            [
                x0 + kink.leading_edge_x_m + kink.chord_m,
                kink.y_m,
                g.wing.break_z_m,
            ],
            [1.0, 0.0, 0.0],
            1.0,
        ));
        out.push(handle(
            HandleKind::TipChord,
            wing,
            "design.tip_chord_m",
            "Tip chord",
            [
                x0 + tip.leading_edge_x_m + tip.chord_m,
                tip.y_m,
                g.wing.tip_z_m,
            ],
            [1.0, 0.0, 0.0],
            1.0,
        ));
        if kink.y_m > 0.1 {
            let sweep = design.sweep_deg.to_radians();
            let per_metre = (sweep.cos().powi(2) / kink.y_m).to_degrees();
            out.push(handle(
                HandleKind::Sweep,
                wing,
                "design.sweep_deg",
                "Leading-edge sweep",
                [x0 + kink.leading_edge_x_m, kink.y_m, g.wing.break_z_m],
                [1.0, 0.0, 0.0],
                per_metre,
            ));
        }
        if tip.chord_m > 0.05 {
            out.push(handle(
                HandleKind::TipTwist,
                wing,
                "design.tip_twist_deg",
                "Tip twist",
                [
                    x0 + tip.leading_edge_x_m + tip.chord_m,
                    tip.y_m,
                    g.wing.tip_z_m - 0.6,
                ],
                [0.0, 0.0, -1.0],
                (1.0 / tip.chord_m).to_degrees(),
            ));
        }
        out.push(handle(
            HandleKind::WingPosition,
            wing,
            "design.wing_x_shift_m",
            "Wing position",
            [x0, 0.0, g.wing.root_z_m],
            [1.0, 0.0, 0.0],
            1.0,
        ));

        for (index, section) in g.wing.custom_sections.iter().enumerate() {
            let x = x0 + section.leading_edge_x_m;
            let y = section.span_fraction * tip.y_m;
            let z = section.z_m;
            let chord = section.chord_m.max(0.01);
            out.push(custom_handle(
                HandleKind::CustomWingSectionSpan,
                wing,
                "Custom wing section span",
                [x, y, z],
                [0.0, 1.0, 0.0],
                1.0 / tip.y_m.max(1e-9),
                0,
                index,
            ));
            out.push(custom_handle(
                HandleKind::CustomWingSectionLeadingEdge,
                wing,
                "Custom wing section leading edge",
                [x, y, z],
                [1.0, 0.0, 0.0],
                1.0,
                1,
                index,
            ));
            out.push(custom_handle(
                HandleKind::CustomWingSectionChord,
                wing,
                "Custom wing section chord",
                [x + chord, y, z],
                [1.0, 0.0, 0.0],
                1.0,
                2,
                index,
            ));
            out.push(custom_handle(
                HandleKind::CustomWingSectionHeight,
                wing,
                "Custom wing section height",
                [x, y, z],
                [0.0, 0.0, 1.0],
                1.0,
                3,
                index,
            ));
            out.push(custom_handle(
                HandleKind::CustomWingSectionTwist,
                wing,
                "Custom wing section twist",
                [x + chord, y, z],
                [0.0, 0.0, 1.0],
                (1.0 / chord).to_degrees(),
                4,
                index,
            ));
        }
    }

    let e = &g.empennage;
    let ts = design.tail_scale;
    let x_hstab = design.fuselage_length_m - e.hstab_offset_from_tail_m + design.tail_x_shift_m;
    let (htx, hty, htz) = e.hstab_tip_le_m;
    let htail = Discipline::HorizontalTail;
    out.push(handle(
        HandleKind::TailPosition,
        htail,
        "design.tail_x_shift_m",
        "Tail position",
        [x_hstab, 0.0, e.hstab_z_m],
        [1.0, 0.0, 0.0],
        1.0,
    ));
    if hty.abs() > 0.1 {
        out.push(handle(
            HandleKind::TailScale,
            htail,
            "design.tail_scale",
            "Tail scale",
            [x_hstab + htx * ts, hty * ts, e.hstab_z_m + htz],
            [0.0, 1.0, 0.0],
            1.0 / hty,
        ));
    }
    if ts > 1e-6 {
        out.push(Handle {
            component: Some(0),
            ..handle(
                HandleKind::TailTipSweep,
                htail,
                "geometry.empennage.hstab_tip_le_m",
                "Tail tip sweep",
                [
                    x_hstab + htx * ts + e.hstab_tip_chord_m * ts,
                    hty * ts,
                    e.hstab_z_m + htz,
                ],
                [1.0, 0.0, 0.0],
                1.0 / ts,
            )
        });
    }
    let x_vstab = design.fuselage_length_m - e.vstab_offset_from_tail_m + design.tail_x_shift_m;
    let (vtx, vty, vtz) = e.vstab_tip_le_m;
    let vtail = Discipline::VerticalTail;
    if ts > 1e-6 {
        out.push(Handle {
            component: Some(2),
            ..handle(
                HandleKind::FinHeight,
                vtail,
                "geometry.empennage.vstab_tip_le_m",
                "Fin height",
                [x_vstab + vtx * ts, vty, e.vstab_z_m + vtz * ts],
                [0.0, 0.0, 1.0],
                1.0 / ts,
            )
        });
        out.push(Handle {
            component: Some(0),
            ..handle(
                HandleKind::FinTipSweep,
                vtail,
                "geometry.empennage.vstab_tip_le_m",
                "Fin tip sweep",
                [
                    x_vstab + vtx * ts + e.vstab_tip_chord_m * ts,
                    vty,
                    e.vstab_z_m + vtz * ts,
                ],
                [1.0, 0.0, 0.0],
                1.0 / ts,
            )
        });
    }

    let f = &g.fuselage;
    let fus = Discipline::Fuselage;
    for (index, (section, part)) in generated_fuselage_rows(config, design).iter().enumerate() {
        let x = section.x_fraction * design.fuselage_length_m;
        let z = section.z_m;
        let station_field = match part {
            GeneratedFuselageStationPart::Nose(xi) if *xi > 1e-9 => {
                Some(("geometry.fuselage.cabin_start_x_m", 1.0 / xi))
            }
            GeneratedFuselageStationPart::CabinStart => {
                Some(("geometry.fuselage.cabin_start_x_m", 1.0))
            }
            GeneratedFuselageStationPart::CabinEnd => {
                Some(("geometry.fuselage.tailcone_length_m", -1.0))
            }
            GeneratedFuselageStationPart::Tail(xi) if *xi < 1.0 - 1e-9 => {
                Some(("geometry.fuselage.tailcone_length_m", -1.0 / (1.0 - xi)))
            }
            GeneratedFuselageStationPart::Tail(_) => Some(("design.fuselage_length_m", 1.0)),
            _ => None,
        };
        if let Some((field_id, per_metre)) = station_field {
            out.push(generated_fuselage_handle(
                HandleKind::GeneratedFuselageSectionStation,
                "Generated fuselage station",
                [x, 0.0, z],
                [1.0, 0.0, 0.0],
                per_metre,
                field_id,
                index,
            ));
        }
        if section.width_m > 0.01 {
            out.push(generated_fuselage_handle(
                HandleKind::GeneratedFuselageSectionWidth,
                "Generated fuselage section width",
                [x, section.width_m * 0.5, z],
                [0.0, 1.0, 0.0],
                2.0,
                "geometry.fuselage.generated_sections",
                index,
            ));
        }
        if section.height_m > 0.01 {
            out.push(generated_fuselage_handle(
                HandleKind::GeneratedFuselageSectionHeight,
                "Generated fuselage section height",
                [x, 0.0, z + section.height_m * 0.5],
                [0.0, 0.0, 1.0],
                2.0,
                "geometry.fuselage.generated_sections",
                index,
            ));
        }
        out.push(generated_fuselage_handle(
            HandleKind::GeneratedFuselageSectionZ,
            "Generated fuselage section vertical position",
            [x, 0.0, z],
            [0.0, 0.0, 1.0],
            1.0,
            "geometry.fuselage.generated_sections",
            index,
        ));
    }
    for (index, section) in f.custom_sections.iter().enumerate() {
        let x = section.x_fraction * design.fuselage_length_m;
        let z = section.z_m;
        out.push(custom_handle(
            HandleKind::CustomFuselageSectionStation,
            fus,
            "Custom fuselage section station",
            [x, 0.0, z],
            [1.0, 0.0, 0.0],
            1.0 / design.fuselage_length_m.max(1e-9),
            0,
            index,
        ));
        out.push(custom_handle(
            HandleKind::CustomFuselageSectionWidth,
            fus,
            "Custom fuselage section width",
            [x, section.width_m * 0.5, z],
            [0.0, 1.0, 0.0],
            2.0,
            1,
            index,
        ));
        out.push(custom_handle(
            HandleKind::CustomFuselageSectionHeight,
            fus,
            "Custom fuselage section height",
            [x, 0.0, z + section.height_m * 0.5],
            [0.0, 0.0, 1.0],
            2.0,
            2,
            index,
        ));
        out.push(custom_handle(
            HandleKind::CustomFuselageSectionZ,
            fus,
            "Custom fuselage section vertical position",
            [x, 0.0, z],
            [0.0, 0.0, 1.0],
            1.0,
            3,
            index,
        ));
    }
    out.push(handle(
        HandleKind::FuselageLength,
        fus,
        "design.fuselage_length_m",
        "Fuselage length",
        [design.fuselage_length_m, 0.0, f.tail_z_m],
        [1.0, 0.0, 0.0],
        1.0,
    ));
    out.push(handle(
        HandleKind::NoseHeight,
        fus,
        "geometry.fuselage.nose_z_m",
        "Nose height",
        [0.0, 0.0, f.nose_z_m],
        [0.0, 0.0, 1.0],
        1.0,
    ));
    out.push(handle(
        HandleKind::FuselageDiameter,
        fus,
        "geometry.fuselage.diameter_m",
        "Fuselage diameter",
        [
            f.cabin_start_x_m + 2.0,
            0.0,
            f.cabin_z_m + f.effective_height_m() / 2.0,
        ],
        [0.0, 0.0, 1.0],
        2.0,
    ));

    if let Some(&y_pos) = g.engine.spanwise_positions_m.iter().find(|y| **y > 0.0) {
        if let Ok(planform) = g.wing.transport_planform(design) {
            let x0 = g.wing.root_datum_x_m + design.wing_x_shift_m;
            let le = planform.leading_edge_x_at(y_pos).unwrap_or(0.0);
            let x_inlet = x0 + le - g.engine.inlet_x_offset_m;
            let z_wing = interpolated_wing_z(config, &planform, y_pos);
            let z = z_wing + g.engine.z_m;
            let length = g.engine.nacelle_length_m();
            let prop = Discipline::Propulsion;
            out.push(handle(
                HandleKind::NacelleInlet,
                prop,
                "geometry.engine.inlet_x_offset_m",
                "Inlet offset",
                [x_inlet, y_pos, z],
                [-1.0, 0.0, 0.0],
                1.0,
            ));
            out.push(handle(
                HandleKind::NacelleSpan,
                prop,
                "geometry.engine.spanwise_positions_m",
                "Engine spanwise position",
                [x_inlet + 0.5 * length, y_pos, z],
                [0.0, 1.0, 0.0],
                1.0,
            ));
            out.push(handle(
                HandleKind::NacelleHeight,
                prop,
                "geometry.engine.z_m",
                "Engine height",
                [x_inlet + length, y_pos, z],
                [0.0, 0.0, 1.0],
                1.0,
            ));
        }
    }

    match focus {
        Some(discipline) => out.retain(|h| h.discipline == discipline),
        None => out.retain(|h| {
            matches!(
                h.kind,
                HandleKind::Span
                    | HandleKind::RootChord
                    | HandleKind::TipChord
                    | HandleKind::Sweep
                    | HandleKind::WingPosition
                    | HandleKind::TailPosition
                    | HandleKind::FuselageLength
                    | HandleKind::NacelleSpan
                    | HandleKind::CustomWingSectionSpan
                    | HandleKind::CustomWingSectionLeadingEdge
                    | HandleKind::CustomWingSectionChord
                    | HandleKind::CustomWingSectionHeight
                    | HandleKind::CustomWingSectionTwist
                    | HandleKind::CustomFuselageSectionStation
                    | HandleKind::CustomFuselageSectionWidth
                    | HandleKind::CustomFuselageSectionHeight
                    | HandleKind::CustomFuselageSectionZ
                    | HandleKind::GeneratedFuselageSectionStation
                    | HandleKind::GeneratedFuselageSectionWidth
                    | HandleKind::GeneratedFuselageSectionHeight
                    | HandleKind::GeneratedFuselageSectionZ
            )
        }),
    }
    out
}

fn interpolated_wing_z(
    config: &AlasConfig,
    planform: &alas_config::geometry::TransportPlanform,
    y: f64,
) -> f64 {
    let wing = &config.geometry.wing;
    let y_break = planform.kink.y_m;
    let semi_span = planform.tip.y_m;
    if y <= y_break {
        wing.root_z_m + (wing.break_z_m - wing.root_z_m) * (y / (y_break + 1e-9))
    } else {
        wing.break_z_m
            + (wing.tip_z_m - wing.break_z_m) * ((y - y_break) / (semi_span - y_break + 1e-9))
    }
}

/// A drag in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveDrag {
    /// The handle being dragged.
    pub handle: Handle,
    /// Parameter value when the pointer went down.
    pub start_value: f64,
    /// Unit screen direction of increasing parameter, in points.
    pub screen_axis: egui::Vec2,
    /// Parameter units per screen point along `screen_axis`.
    pub per_point: f64,
    /// Accumulated pointer motion since the pointer went down.
    pub total: egui::Vec2,
    /// Whether the parameter changed from its start value.
    pub changed: bool,
}

/// The scalar the handle reads from a field value.
fn scalar_of(handle: &Handle, value: &Value) -> Option<f64> {
    match handle.component {
        Some(index) => value.as_array()?.get(index)?.as_f64(),
        None => match handle.kind {
            HandleKind::NacelleSpan => value
                .as_array()?
                .iter()
                .filter_map(Value::as_f64)
                .find(|y| *y > 0.0),
            _ => value.as_f64(),
        },
    }
}

fn is_custom_section_handle(kind: HandleKind) -> bool {
    matches!(
        kind,
        HandleKind::CustomWingSectionSpan
            | HandleKind::CustomWingSectionLeadingEdge
            | HandleKind::CustomWingSectionChord
            | HandleKind::CustomWingSectionHeight
            | HandleKind::CustomWingSectionTwist
            | HandleKind::CustomFuselageSectionStation
            | HandleKind::CustomFuselageSectionWidth
            | HandleKind::CustomFuselageSectionHeight
            | HandleKind::CustomFuselageSectionZ
    )
}

fn is_generated_fuselage_section_handle(kind: HandleKind) -> bool {
    matches!(
        kind,
        HandleKind::GeneratedFuselageSectionWidth
            | HandleKind::GeneratedFuselageSectionHeight
            | HandleKind::GeneratedFuselageSectionZ
    )
}

fn custom_section_scalar(values: &Value, handle: &Handle) -> Option<f64> {
    let index = handle.section_index?;
    let component = handle.component?;
    let pointer = match handle.kind {
        HandleKind::CustomWingSectionSpan
        | HandleKind::CustomWingSectionLeadingEdge
        | HandleKind::CustomWingSectionChord
        | HandleKind::CustomWingSectionHeight
        | HandleKind::CustomWingSectionTwist => "/geometry/wing/custom_sections",
        HandleKind::CustomFuselageSectionStation
        | HandleKind::CustomFuselageSectionWidth
        | HandleKind::CustomFuselageSectionHeight
        | HandleKind::CustomFuselageSectionZ => "/geometry/fuselage/custom_sections",
        _ => return None,
    };
    let key = match handle.kind {
        HandleKind::CustomWingSectionSpan | HandleKind::CustomFuselageSectionStation => {
            if matches!(handle.kind, HandleKind::CustomWingSectionSpan) {
                "span_fraction"
            } else {
                "x_fraction"
            }
        }
        HandleKind::CustomWingSectionLeadingEdge => "leading_edge_x_m",
        HandleKind::CustomWingSectionChord => "chord_m",
        HandleKind::CustomWingSectionHeight => "z_m",
        HandleKind::CustomWingSectionTwist => "twist_deg",
        HandleKind::CustomFuselageSectionWidth => "width_m",
        HandleKind::CustomFuselageSectionHeight => "height_m",
        HandleKind::CustomFuselageSectionZ => "z_m",
        _ => return None,
    };
    let _ = component;
    values
        .pointer(pointer)?
        .as_array()?
        .get(index)?
        .get(key)?
        .as_f64()
}

fn generated_fuselage_section_scalar(state: &AppState, handle: &Handle) -> Option<f64> {
    let index = handle.section_index?;
    let key = match handle.kind {
        HandleKind::GeneratedFuselageSectionWidth => "width_m",
        HandleKind::GeneratedFuselageSectionHeight => "height_m",
        HandleKind::GeneratedFuselageSectionZ => "z_m",
        _ => return None,
    };
    let (config, design) = (state.typed_config()?, state.current_design()?);
    generated_fuselage_rows(&config, &design)
        .get(index)
        .map(|(section, _)| match key {
            "width_m" => section.width_m,
            "height_m" => section.height_m,
            _ => section.z_m,
        })
}

fn generated_fuselage_section_bounds(handle: &Handle) -> (f64, f64, usize) {
    match handle.kind {
        HandleKind::GeneratedFuselageSectionWidth | HandleKind::GeneratedFuselageSectionHeight => {
            (0.01, 100.0, 2)
        }
        HandleKind::GeneratedFuselageSectionZ => (-50.0, 50.0, 2),
        _ => (f64::NEG_INFINITY, f64::INFINITY, 2),
    }
}

fn write_generated_fuselage_section_scalar(
    state: &mut AppState,
    handle: &Handle,
    scalar: f64,
) -> bool {
    let Some(index) = handle.section_index else {
        return false;
    };
    let key = match handle.kind {
        HandleKind::GeneratedFuselageSectionWidth => "width_m",
        HandleKind::GeneratedFuselageSectionHeight => "height_m",
        HandleKind::GeneratedFuselageSectionZ => "z_m",
        _ => return false,
    };
    let (Some(config), Some(design)) = (state.typed_config(), state.current_design()) else {
        return false;
    };
    let mut sections: Vec<FuselageSection> = generated_fuselage_rows(&config, &design)
        .into_iter()
        .map(|(section, _)| section)
        .collect();
    let Some(section) = sections.get_mut(index) else {
        return false;
    };
    match key {
        "width_m" => section.width_m = scalar,
        "height_m" => section.height_m = scalar,
        _ => section.z_m = scalar,
    }
    let Some(fuselage) = state
        .config_values
        .pointer_mut("/geometry/fuselage")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let before = fuselage.get("generated_sections").cloned();
    let next = serde_json::to_value(sections).unwrap_or(Value::Array(Vec::new()));
    if before.as_ref() == Some(&next) {
        return false;
    }
    fuselage.insert("generated_sections".to_owned(), next);
    true
}

fn custom_section_bounds(state: &AppState, handle: &Handle) -> (f64, f64, usize) {
    let decimals = match handle.kind {
        HandleKind::CustomWingSectionSpan | HandleKind::CustomFuselageSectionStation => 3,
        HandleKind::CustomWingSectionTwist => 2,
        _ => 2,
    };
    let index = handle.section_index.unwrap_or(0);
    let mut bounds = match handle.kind {
        HandleKind::CustomWingSectionSpan | HandleKind::CustomFuselageSectionStation => {
            (0.001, 0.999)
        }
        HandleKind::CustomWingSectionChord
        | HandleKind::CustomFuselageSectionWidth
        | HandleKind::CustomFuselageSectionHeight => (0.01, 100.0),
        HandleKind::CustomWingSectionLeadingEdge => (-100.0, 100.0),
        HandleKind::CustomWingSectionHeight | HandleKind::CustomFuselageSectionZ => (-50.0, 50.0),
        HandleKind::CustomWingSectionTwist => (-30.0, 30.0),
        _ => (f64::NEG_INFINITY, f64::INFINITY),
    };
    if matches!(
        handle.kind,
        HandleKind::CustomWingSectionSpan | HandleKind::CustomFuselageSectionStation
    ) {
        let pointer = if handle.kind == HandleKind::CustomWingSectionSpan {
            "/geometry/wing/custom_sections"
        } else {
            "/geometry/fuselage/custom_sections"
        };
        if let Some(items) = state
            .config_values
            .pointer(pointer)
            .and_then(Value::as_array)
        {
            if let Some(previous) = items
                .get(index.saturating_sub(1))
                .and_then(|item| {
                    item.get(if handle.kind == HandleKind::CustomWingSectionSpan {
                        "span_fraction"
                    } else {
                        "x_fraction"
                    })
                })
                .and_then(Value::as_f64)
            {
                bounds.0 = bounds.0.max(previous + 0.001);
            }
            if let Some(next) = items
                .get(index + 1)
                .and_then(|item| {
                    item.get(if handle.kind == HandleKind::CustomWingSectionSpan {
                        "span_fraction"
                    } else {
                        "x_fraction"
                    })
                })
                .and_then(Value::as_f64)
            {
                bounds.1 = bounds.1.min(next - 0.001);
            }
        }
        let current = custom_section_scalar(&state.config_values, handle).unwrap_or(0.5);
        let generated = if handle.kind == HandleKind::CustomWingSectionSpan {
            state
                .typed_config()
                .and_then(|config| {
                    state
                        .current_design()
                        .and_then(|design| config.geometry.wing.transport_planform(&design).ok())
                })
                .map(|planform| {
                    planform
                        .stations()
                        .into_iter()
                        .map(|station| station.span_fraction)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            generated_fuselage_station_fractions(state)
        };
        for station in generated {
            if station < current {
                bounds.0 = bounds.0.max(station + 0.001);
            } else if station > current {
                bounds.1 = bounds.1.min(station - 0.001);
            } else {
                // A malformed or externally edited section can temporarily
                // coincide with a generated station.  Keep this handle
                // pinned instead of letting a drag create another collision.
                bounds.0 = bounds.0.max(current + 0.001);
                bounds.1 = bounds.1.min(current - 0.001);
            }
        }
    }
    if bounds.0 > bounds.1 {
        let midpoint = (bounds.0 + bounds.1) * 0.5;
        (midpoint, midpoint, decimals)
    } else {
        (bounds.0, bounds.1, decimals)
    }
}

fn generated_fuselage_station_fractions(state: &AppState) -> Vec<f64> {
    let (Some(config), Some(design)) = (state.typed_config(), state.current_design()) else {
        return Vec::new();
    };
    let fuselage = &config.geometry.fuselage;
    let length_m = design.fuselage_length_m;
    if !(length_m > 0.0) {
        return Vec::new();
    }
    let cabin_start = fuselage.cabin_start_x_m;
    let tailcone = fuselage.tailcone_length_m;
    let cabin_end = length_m - tailcone;
    if !(cabin_start >= 0.0 && cabin_end >= cabin_start && tailcone >= 0.0) {
        return Vec::new();
    }
    let mut fractions = Vec::with_capacity(20);
    for index in 0..9 {
        let angle = std::f64::consts::FRAC_PI_2 * index as f64 / 9.0;
        let xi = 1.0 - angle.cos();
        fractions.push(xi * cabin_start / length_m);
    }
    fractions.push(cabin_start / length_m);
    fractions.push(cabin_end / length_m);
    for index in 1..10 {
        let xi = index as f64 / 9.0;
        fractions.push((cabin_end + xi * tailcone) / length_m);
    }
    fractions
}

fn write_custom_section_scalar(state: &mut AppState, handle: &Handle, scalar: f64) -> bool {
    let index = match handle.section_index {
        Some(index) => index,
        None => return false,
    };
    let pointer = match handle.kind {
        HandleKind::CustomWingSectionSpan
        | HandleKind::CustomWingSectionLeadingEdge
        | HandleKind::CustomWingSectionChord
        | HandleKind::CustomWingSectionHeight
        | HandleKind::CustomWingSectionTwist => "/geometry/wing/custom_sections",
        HandleKind::CustomFuselageSectionStation
        | HandleKind::CustomFuselageSectionWidth
        | HandleKind::CustomFuselageSectionHeight
        | HandleKind::CustomFuselageSectionZ => "/geometry/fuselage/custom_sections",
        _ => return false,
    };
    let key = match handle.kind {
        HandleKind::CustomWingSectionSpan => "span_fraction",
        HandleKind::CustomWingSectionLeadingEdge => "leading_edge_x_m",
        HandleKind::CustomWingSectionChord => "chord_m",
        HandleKind::CustomWingSectionHeight => "z_m",
        HandleKind::CustomWingSectionTwist => "twist_deg",
        HandleKind::CustomFuselageSectionStation => "x_fraction",
        HandleKind::CustomFuselageSectionWidth => "width_m",
        HandleKind::CustomFuselageSectionHeight => "height_m",
        HandleKind::CustomFuselageSectionZ => "z_m",
        _ => return false,
    };
    let Some(item) = state
        .config_values
        .pointer_mut(pointer)
        .and_then(Value::as_array_mut)
        .and_then(|items| items.get_mut(index))
    else {
        return false;
    };
    let Some(slot) = item.get_mut(key) else {
        return false;
    };
    let before = slot.as_f64();
    *slot = Value::from(scalar);
    before != Some(scalar)
}

/// The field value with the handle's scalar replaced.
fn value_with_scalar(handle: &Handle, current: &Value, scalar: f64) -> Value {
    match handle.component {
        Some(index) => {
            let mut items: Vec<Value> = current.as_array().cloned().unwrap_or_default();
            if index < items.len() {
                items[index] = Value::from(scalar);
            }
            Value::Array(items)
        }
        None => match handle.kind {
            HandleKind::NacelleSpan => {
                // Engines stay mirrored: every positive position takes the
                // dragged value, every negative one its mirror, and a
                // centreline engine stays on the centreline.
                let items: Vec<Value> = current
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|y| {
                                Value::from(if y > 0.0 {
                                    scalar
                                } else if y < 0.0 {
                                    -scalar
                                } else {
                                    0.0
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Value::Array(items)
            }
            _ => Value::from(scalar),
        },
    }
}

impl AppState {
    /// The handles for the current aircraft and focus.
    pub fn sandbox_handles(&self) -> Vec<Handle> {
        let (Some(config), Some(design)) = (self.typed_config(), self.current_design()) else {
            return Vec::new();
        };
        handles(&config, &design, self.sandbox.focus())
    }

    fn sandbox_field(&self, id: &str) -> Option<SandboxField> {
        fields::inventory(&self.schema)
            .into_iter()
            .find(|f| f.id == id)
    }

    /// Start dragging `handle`. `points_per_canvas_unit` is the widget scale
    /// from scene canvas units to screen points.
    pub fn begin_handle_drag(
        &mut self,
        handle: &Handle,
        framing: &SceneFraming,
        points_per_canvas_unit: f32,
    ) -> bool {
        if !self.sandbox.active() || self.sandbox.drag.is_some() {
            return false;
        }
        let start_value = if is_custom_section_handle(handle.kind) {
            custom_section_scalar(&self.config_values, handle)
        } else if is_generated_fuselage_section_handle(handle.kind) {
            generated_fuselage_section_scalar(self, handle)
        } else {
            let Some(field) = self.sandbox_field(handle.field_id) else {
                return false;
            };
            let current = fields::read_value(&field, &self.config_values, &self.design_values);
            scalar_of(handle, &current)
        };
        let Some(start_value) = start_value else {
            return false;
        };
        let origin = framing.project(handle.point);
        let moved = framing.project([
            handle.point[0] + handle.axis[0],
            handle.point[1] + handle.axis[1],
            handle.point[2] + handle.axis[2],
        ]);
        let delta = egui::vec2((moved[0] - origin[0]) as f32, (moved[1] - origin[1]) as f32)
            * points_per_canvas_unit;
        let length = delta.length();
        if !length.is_finite() || length < 1e-3 {
            // The axis is edge-on to the camera; the drag cannot be resolved.
            return false;
        }
        let snapshot = self.edit_snapshot();
        self.sandbox.undo.begin_transaction(snapshot);
        self.sandbox.drag = Some(ActiveDrag {
            handle: handle.clone(),
            start_value,
            screen_axis: delta / length,
            per_point: handle.per_metre / f64::from(length),
            total: egui::Vec2::ZERO,
            changed: false,
        });
        true
    }

    /// Apply one frame of pointer motion to the active drag.
    pub fn update_handle_drag(&mut self, motion: egui::Vec2) {
        let (handle, start_value, per_point, along) = {
            let Some(drag) = self.sandbox.drag.as_mut() else {
                return;
            };
            if !motion.is_finite() {
                return;
            }
            drag.total += motion;
            (
                drag.handle.clone(),
                drag.start_value,
                drag.per_point,
                drag.total.dot(drag.screen_axis),
            )
        };
        let raw = start_value + f64::from(along) * per_point;
        let (rounded, changed) = if is_custom_section_handle(handle.kind) {
            let (min, max, decimals) = custom_section_bounds(self, &handle);
            let scale = 10f64.powi(decimals as i32);
            (((raw.clamp(min, max) * scale).round() / scale), true)
        } else if is_generated_fuselage_section_handle(handle.kind) {
            let (min, max, decimals) = generated_fuselage_section_bounds(&handle);
            let scale = 10f64.powi(decimals as i32);
            (((raw.clamp(min, max) * scale).round() / scale), true)
        } else {
            let Some(field) = self.sandbox_field(handle.field_id) else {
                return;
            };
            let current = fields::read_value(&field, &self.config_values, &self.design_values);
            let clamped = if matches!(
                field.kind,
                FieldKind::Float
                    | FieldKind::Int
                    | FieldKind::OptionalFloat
                    | FieldKind::Vec3
                    | FieldKind::FloatList
            ) {
                raw.clamp(field.min, field.max)
            } else {
                raw
            };
            let scale = 10f64.powi(field.decimals as i32);
            (
                (clamped * scale).round() / scale,
                scalar_of(&handle, &current) != Some(clamped),
            )
        };
        if !changed {
            return;
        }
        if is_custom_section_handle(handle.kind) {
            if !write_custom_section_scalar(self, &handle, rounded) {
                return;
            }
        } else if is_generated_fuselage_section_handle(handle.kind) {
            if !write_generated_fuselage_section_scalar(self, &handle, rounded) {
                return;
            }
        } else {
            let Some(field) = self.sandbox_field(handle.field_id) else {
                return;
            };
            let current = fields::read_value(&field, &self.config_values, &self.design_values);
            let next = value_with_scalar(&handle, &current, rounded);
            if !fields::value_in_domain(&field, &next) {
                return;
            }
            fields::write_value(
                &field,
                next,
                &mut self.config_values,
                &mut self.design_values,
            );
        }
        if let Some(drag) = self.sandbox.drag.as_mut() {
            drag.changed = rounded != start_value;
        }
        self.on_sandbox_model_changed();
    }

    /// Finish the active drag as one undo step.
    pub fn end_handle_drag(&mut self) {
        if let Some(drag) = self.sandbox.drag.take() {
            self.sandbox.undo.commit_transaction(drag.changed);
            if drag.changed {
                let label = drag.handle.label.to_owned();
                let value = if is_custom_section_handle(drag.handle.kind) {
                    custom_section_scalar(&self.config_values, &drag.handle)
                } else if is_generated_fuselage_section_handle(drag.handle.kind) {
                    generated_fuselage_section_scalar(self, &drag.handle)
                } else {
                    self.sandbox_field(drag.handle.field_id).and_then(|f| {
                        let value =
                            fields::read_value(&f, &self.config_values, &self.design_values);
                        scalar_of(&drag.handle, &value)
                    })
                }
                .map(|v| format!("{v:.3}"))
                .unwrap_or_default();
                self.note_parameter_modified(crate::views::tr(&label), value);
            }
        }
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{FuselageSection, WingSection};

    #[test]
    fn handles_sit_on_the_analytic_planform_and_follow_the_focus() {
        let state = AppState::default();
        let config = state.typed_config().expect("config");
        let design = state.current_design().expect("design");
        let all = handles(&config, &design, None);
        assert!(all.iter().any(|h| h.kind == HandleKind::Span));
        let span = all
            .iter()
            .find(|h| h.kind == HandleKind::Span)
            .expect("span handle");
        assert!((span.point[1] - design.span_m / 2.0).abs() < 1e-9);
        let fuselage = handles(&config, &design, Some(Discipline::Fuselage));
        assert!(fuselage
            .iter()
            .all(|h| h.discipline == Discipline::Fuselage));
        assert!(fuselage
            .iter()
            .any(|h| h.kind == HandleKind::FuselageDiameter));
    }

    #[test]
    fn mirrored_engine_positions_move_together_and_keep_centreline_engines() {
        let handle = handles(
            &AlasConfig::default(),
            &DesignVector::default(),
            Some(Discipline::Propulsion),
        )
        .into_iter()
        .find(|h| h.kind == HandleKind::NacelleSpan)
        .expect("nacelle span handle");
        let current = serde_json::json!([9.8, -9.8, 0.0]);
        assert_eq!(scalar_of(&handle, &current), Some(9.8));
        assert_eq!(
            value_with_scalar(&handle, &current, 11.0),
            serde_json::json!([11.0, -11.0, 0.0])
        );
    }

    #[test]
    fn custom_section_handles_route_each_point_back_to_the_section_list() {
        let mut config = AlasConfig::default();
        config.geometry.wing.custom_sections.push(WingSection {
            span_fraction: 0.56,
            leading_edge_x_m: 1.4,
            chord_m: 4.2,
            z_m: 0.3,
            twist_deg: -1.5,
            airfoil: "rae2822".to_owned(),
        });
        config
            .geometry
            .fuselage
            .custom_sections
            .push(FuselageSection {
                x_fraction: 0.52,
                width_m: 4.8,
                height_m: 5.1,
                z_m: 0.4,
                shape: 2.0,
            });
        let design = DesignVector::default();
        let wing = handles(&config, &design, Some(Discipline::Wing));
        let wing_custom: Vec<_> = wing
            .iter()
            .filter(|handle| is_custom_section_handle(handle.kind))
            .collect();
        assert_eq!(wing_custom.len(), 5);
        assert!(wing_custom
            .iter()
            .all(|handle| handle.section_index == Some(0)));

        let fuselage = handles(&config, &design, Some(Discipline::Fuselage));
        let fuselage_station = fuselage
            .iter()
            .find(|handle| handle.kind == HandleKind::CustomFuselageSectionStation)
            .expect("custom fuselage station handle");
        let mut state = AppState::default();
        state.config_values = serde_json::to_value(&config).expect("config JSON");
        assert_eq!(
            custom_section_scalar(&state.config_values, fuselage_station),
            Some(0.52)
        );
        assert!(write_custom_section_scalar(
            &mut state,
            fuselage_station,
            0.57
        ));
        assert_eq!(
            state.config_values["geometry"]["fuselage"]["custom_sections"][0]["x_fraction"],
            Value::from(0.57)
        );
    }

    #[test]
    fn generated_fuselage_preview_handles_cover_and_override_all_station_rows() {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let fuselage = handles(&config, &design, Some(Discipline::Fuselage));
        let z_handles: Vec<_> = fuselage
            .iter()
            .filter(|handle| handle.kind == HandleKind::GeneratedFuselageSectionZ)
            .collect();
        let width_handles: Vec<_> = fuselage
            .iter()
            .filter(|handle| handle.kind == HandleKind::GeneratedFuselageSectionWidth)
            .collect();
        let station_handles: Vec<_> = fuselage
            .iter()
            .filter(|handle| handle.kind == HandleKind::GeneratedFuselageSectionStation)
            .collect();
        assert_eq!(z_handles.len(), 20);
        assert_eq!(
            width_handles.len(),
            18,
            "the nose and tail tips have zero width"
        );
        assert_eq!(station_handles.len(), 19, "the nose tip has fixed X=0");
        assert!(z_handles
            .iter()
            .all(|handle| handle.section_index.is_some()));

        let mut state = AppState::default();
        state.config_values = serde_json::to_value(&config).expect("config JSON");
        let target = z_handles
            .iter()
            .find(|handle| handle.section_index == Some(10))
            .expect("cabin-end Z handle");
        assert_eq!(generated_fuselage_section_scalar(&state, target), Some(0.2));
        assert!(write_generated_fuselage_section_scalar(
            &mut state, target, 1.1
        ));
        assert_eq!(
            state.config_values["geometry"]["fuselage"]["generated_sections"]
                .as_array()
                .expect("generated override vector")
                .len(),
            20
        );
        assert_eq!(generated_fuselage_section_scalar(&state, target), Some(1.1));
    }

    #[test]
    fn custom_wing_height_drag_bounds_allow_negative_dihedral() {
        let mut config = AlasConfig::default();
        config.geometry.wing.custom_sections.push(WingSection {
            span_fraction: 0.56,
            leading_edge_x_m: 1.4,
            chord_m: 4.2,
            z_m: -1.0,
            twist_deg: 0.0,
            airfoil: "rae2822".to_owned(),
        });
        let handles = handles(&config, &DesignVector::default(), Some(Discipline::Wing));
        let handle = handles
            .into_iter()
            .find(|handle| handle.kind == HandleKind::CustomWingSectionHeight)
            .expect("custom wing height handle");
        let state = AppState {
            config_values: serde_json::to_value(config).expect("config JSON"),
            ..AppState::default()
        };
        let (min, max, _) = custom_section_bounds(&state, &handle);
        assert_eq!((min, max), (-50.0, 50.0));
    }
}
