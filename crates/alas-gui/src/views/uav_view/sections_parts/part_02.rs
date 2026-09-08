// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn manual_propulsion_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr("Advanced fallback: enter measured or solver-derived points only when the automatic source-bounded model is not the required evidence. A retail static-thrust value is not accepted for cruise."),
    );
    ui.add_space(8.0);
    card(ui, "Propulsion data source or test ID", None, |ui| {
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            TextEdit::singleline(&mut state.propulsion_evidence)
                .hint_text(tr("Example: dyno-2026-03 or solver-case-17")),
        )
        .on_hover_text(tr(
            "Enter a test ID, report name, solver case, or source URL that supports both operating points.",
        ));
        ui.add_space(6.0);
        form_grid(ui, "uav_propulsion_manual_grid", |ui| {
            integer(
                ui,
                "Battery series cells",
                &mut state.propulsion_series_cells,
            );
        });
    });
    ui.add_space(8.0);
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            operating_point_card(state, &mut columns[0], true);
            operating_point_card(state, &mut columns[1], false);
        });
    } else {
        operating_point_card(state, ui, true);
        ui.add_space(8.0);
        operating_point_card(state, ui, false);
    }
    ui.add_space(6.0);
    ui.weak(tr(
        "The manual fallback retains the historic two-point energy surrogate; use the automatic catalogue solver for the multi-phase electrical simulation.",
    ));
}

fn operating_point_card(state: &mut UavWorkflowState, ui: &mut Ui, low_speed: bool) {
    let (title, description) = if low_speed {
        (
            "Low-speed operating point",
            "Use the speed entered as Maximum stall speed in Mission; static thrust alone is not enough.",
        )
    } else {
        (
            "Cruise operating point",
            "Use the cruise speed entered in Mission.",
        )
    };
    card(ui, title, Some(description), |ui| {
        form_grid(
            ui,
            if low_speed {
                "uav_low_speed_propulsion_grid"
            } else {
                "uav_cruise_propulsion_grid"
            },
            |ui| {
                if low_speed {
                    value(ui, "Low-speed thrust", &mut state.stall_thrust_n, "N");
                    value(ui, "Low-speed current", &mut state.stall_current_a, "A");
                    value(ui, "Low-speed power", &mut state.stall_power_w, "W");
                } else {
                    value(ui, "Cruise thrust", &mut state.cruise_thrust_n, "N");
                    value(ui, "Cruise current", &mut state.cruise_current_a, "A");
                    value(ui, "Cruise power", &mut state.cruise_power_w, "W");
                }
            },
        );
    });
}

fn form_grid(ui: &mut Ui, id: impl std::hash::Hash, contents: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 8.0])
        .show(ui, contents);
}

