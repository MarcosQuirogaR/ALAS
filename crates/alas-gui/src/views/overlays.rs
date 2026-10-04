// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The window-level overlays: the boot splash, the first-run walkthrough, the
//! advanced walkthrough guide, the storage dialog, and the About window.

mod about;
mod guide;
mod splash;
mod storage;
#[cfg(test)]
mod tests;
mod walkthrough;

pub use about::show_about;
pub use guide::show_advanced_guide;
pub use splash::show_splash;
pub use storage::show_storage_dialog;
pub use walkthrough::show_walkthrough;

fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    fields.iter().fold(tr(template), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}
