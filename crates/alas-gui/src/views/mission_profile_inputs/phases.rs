// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission phases and the solver fields each one edits.

#[derive(Clone, Copy)]
pub(crate) struct Phase {
    pub(crate) id: &'static str,
    pub(super) title: &'static str,
    pub(crate) fields: &'static [&'static str],
}

pub(crate) const PHASES: &[Phase] = &[
    Phase {
        id: "takeoff",
        title: "Take-off",
        fields: &[
            "takeoff_altitude_gain_m",
            "takeoff_air_speed_m_s",
            "takeoff_climb_rate_m_s",
        ],
    },
    Phase {
        id: "initial_climb",
        title: "Initial climb",
        fields: &[
            "initial_climb_air_speed_m_s",
            "initial_climb_rate_m_s",
            "initial_climb_altitude_fraction",
        ],
    },
    Phase {
        id: "cruise_1",
        title: "Cruise leg 1",
        fields: &["cruise_1_air_speed_m_s", "cruise_1_distance_fraction"],
    },
    Phase {
        id: "step_climb_1",
        title: "Step climb 1",
        fields: &[
            "step_climb_1_air_speed_m_s",
            "step_climb_1_rate_m_s",
            "step_climb_1_altitude_fraction",
        ],
    },
    Phase {
        id: "cruise_2",
        title: "Cruise leg 2",
        fields: &["cruise_2_air_speed_m_s", "cruise_2_distance_fraction"],
    },
    Phase {
        id: "step_climb_2",
        title: "Step climb 2",
        fields: &["step_climb_2_air_speed_m_s", "step_climb_2_rate_m_s"],
    },
    Phase {
        id: "cruise_3",
        title: "Cruise leg 3",
        fields: &["cruise_3_air_speed_m_s", "cruise_3_distance_fraction"],
    },
    Phase {
        id: "descent_1",
        title: "Descent rung 1",
        fields: &[
            "descent_1_altitude_ft",
            "descent_1_air_speed_m_s",
            "descent_1_rate_m_s",
        ],
    },
    Phase {
        id: "descent_2",
        title: "Descent rung 2",
        fields: &[
            "descent_2_altitude_ft",
            "descent_2_air_speed_m_s",
            "descent_2_rate_m_s",
        ],
    },
    Phase {
        id: "descent_3",
        title: "Descent rung 3",
        fields: &[
            "descent_3_altitude_ft",
            "descent_3_air_speed_m_s",
            "descent_3_rate_m_s",
        ],
    },
    Phase {
        id: "descent_4",
        title: "Descent rung 4",
        fields: &[
            "descent_4_altitude_ft",
            "descent_4_air_speed_m_s",
            "descent_4_rate_m_s",
        ],
    },
    Phase {
        id: "final_approach",
        title: "Final approach",
        fields: &["landing_air_speed_m_s", "landing_descent_rate_m_s"],
    },
];

/// Ask the native window for the size of its content, once per change of that
/// size: a window the user then resizes by hand is left alone until the
/// content itself changes.
pub(super) fn fit_to_content(ctx: &egui::Context, phase_id: &str, wanted: egui::Vec2) {
    let id = egui::Id::new(("mission_profile_phase_fit", phase_id));
    let wanted = wanted.round();
    let last = ctx.data(|data| data.get_temp::<egui::Vec2>(id));
    if last.is_some_and(|last| (last - wanted).abs().max_elem() < 2.0) {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, wanted));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(wanted));
}
