// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! View components for every navigable page and shell panel.

pub mod airport_window;
pub mod analyses_view;
pub mod cfd_view;
pub mod control_bar;
pub mod design_space_view;
pub mod form;
pub mod form_page;
pub mod guide_data;
pub(crate) mod inputs_custom;
pub(crate) mod inputs_relaxation;
pub mod inputs_view;
pub mod notices;
pub mod overlays;
pub mod preview_dock;
mod result_3d;
pub mod results_view;
pub mod run_log;
pub mod screening_view;
pub(crate) mod screening_window;
pub mod tools_view;
pub mod tour_data;
pub mod uav_view;
pub mod wing_analysis_view;

pub use analyses_view::show_analyses_view;
pub use control_bar::show_control_bar;
pub use design_space_view::show_design_space_view;
pub use form_page::show_form_page;
pub use inputs_view::show_inputs_view;
pub use preview_dock::show_preview_dock;
pub use results_view::show_results_view;
pub use run_log::show_run_log;
pub use screening_view::show_screening_view;
pub use tools_view::show_tools_view;
pub use uav_view::show_uav_view;

pub(crate) fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

pub(crate) fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    fields.iter().fold(tr(template), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}
