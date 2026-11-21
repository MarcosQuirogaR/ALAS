// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optional display-language hook for text the figure code wraps itself.
//!
//! The report crate stays language-neutral: exports are English unless a host
//! installs a localizer. A footnote wrapped into one text element per line
//! cannot be translated afterwards (the wrap depends on the language), so the
//! wrapping helpers translate the whole sentence first through this hook.

use std::borrow::Cow;
use std::sync::OnceLock;

static LOCALIZER: OnceLock<fn(&str) -> String> = OnceLock::new();

/// Install the sentence localizer used before wrapping. The first call wins.
pub fn set_text_localizer(localizer: fn(&str) -> String) {
    let _ = LOCALIZER.set(localizer);
}

/// `text` through the installed localizer, or unchanged when there is none.
pub(crate) fn localize(text: &str) -> Cow<'_, str> {
    match LOCALIZER.get() {
        Some(localizer) => Cow::Owned(localizer(text)),
        None => Cow::Borrowed(text),
    }
}
