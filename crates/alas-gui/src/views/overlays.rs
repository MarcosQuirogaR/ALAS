// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The window-level overlays: the boot splash, the first-run walkthrough,
//! the storage dialog, and the About window.

mod about;
mod splash;
mod storage;
#[cfg(test)]
mod tests;
mod walkthrough;

pub use about::show_about;
pub use splash::show_splash;
pub use storage::show_storage_dialog;
pub use walkthrough::show_walkthrough;

fn tr(text: &str) -> String {
    crate::views::tr(text)
}

fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    alas_i18n::fill_fields(&tr(template), fields)
}
