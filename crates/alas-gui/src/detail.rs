// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The global Basic / Advanced detail level.
//!
//! One switch in the top bar drives every page. The source of truth is the
//! schema's per-field `advanced` level: Basic hides those fields (and each
//! page's Advanced sections), Advanced shows them. The shell publishes the
//! current level into the egui context once per frame so the form renderers
//! need no extra parameter; a context that never received a level (unit
//! tests, embedders) keeps the legacy collapsed "Advanced (n)" group.

use egui::{Context, Id};

/// How much of each page is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Detail {
    /// Routine fields only.
    #[default]
    Basic,
    /// Routine and advanced fields, plus each page's Advanced sections.
    Advanced,
}

impl Detail {
    /// The switch label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Advanced => "Advanced",
        }
    }

    /// Whether advanced rows are shown.
    pub fn is_advanced(self) -> bool {
        self == Self::Advanced
    }
}

fn key() -> Id {
    Id::new("alas_detail_level")
}

/// Publish the level the shell is rendering with.
pub fn publish(ctx: &Context, detail: Detail) {
    ctx.data_mut(|data| data.insert_temp(key(), detail));
}

/// The published level, or `None` when the shell never published one.
pub fn published(ctx: &Context) -> Option<Detail> {
    ctx.data(|data| data.get_temp::<Detail>(key()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unpublished_context_has_no_level() {
        assert_eq!(published(&Context::default()), None);
    }

    #[test]
    fn the_published_level_round_trips() {
        let ctx = Context::default();
        publish(&ctx, Detail::Advanced);
        assert_eq!(published(&ctx), Some(Detail::Advanced));
        publish(&ctx, Detail::Basic);
        assert_eq!(published(&ctx), Some(Detail::Basic));
    }
}
