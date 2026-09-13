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

use alas_config::{AlasConfig, DesignVector};
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
    }
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
        let Some(field) = self.sandbox_field(handle.field_id) else {
            return false;
        };
        let current = fields::read_value(&field, &self.config_values, &self.design_values);
        let Some(start_value) = scalar_of(handle, &current) else {
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
        let Some(drag) = self.sandbox.drag.as_mut() else {
            return;
        };
        if !motion.is_finite() {
            return;
        }
        drag.total += motion;
        let along = drag.total.dot(drag.screen_axis);
        let raw = drag.start_value + f64::from(along) * drag.per_point;
        let handle = drag.handle.clone();
        let start_value = drag.start_value;
        let Some(field) = self.sandbox_field(handle.field_id) else {
            return;
        };
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
        let rounded = {
            let scale = 10f64.powi(field.decimals as i32);
            (clamped * scale).round() / scale
        };
        let current = fields::read_value(&field, &self.config_values, &self.design_values);
        if scalar_of(&handle, &current) == Some(rounded) {
            return;
        }
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
                let field = self.sandbox_field(drag.handle.field_id);
                let value = field
                    .map(|f| fields::read_value(&f, &self.config_values, &self.design_values))
                    .and_then(|v| scalar_of(&drag.handle, &v))
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
}
