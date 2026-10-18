// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cache of drag handles shown in the viewport.
//!
//! Building the handles needs the typed configuration and design vector and
//! walks every discipline, so it is done once per aircraft state rather than
//! once per frame.

use std::cell::RefCell;
use std::rc::Rc;

use crate::state::state_memo::{config_fingerprint, design_values_fingerprint};
use crate::state::AppState;

use super::drag::{handles, Handle};
use super::fields::Discipline;

/// What the cached handles depend on: the sandbox revision, the focused
/// discipline, and content fingerprints of the configuration and the design
/// vector (direct buffer writes need not bump the revision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HandleCacheKey {
    revision: u64,
    focus: Option<Discipline>,
    config: u64,
    design: u64,
}

/// The handles last built and the state they were built for.
pub(super) type HandleCache = RefCell<Option<(HandleCacheKey, Rc<Vec<Handle>>)>>;

impl AppState {
    /// The handles for the current aircraft and focus, rebuilt only when the
    /// key above changed.
    pub fn sandbox_handles_shared(&self) -> Rc<Vec<Handle>> {
        let key = HandleCacheKey {
            revision: self.sandbox.revision,
            focus: self.sandbox.focus(),
            config: config_fingerprint(&self.config_values),
            design: design_values_fingerprint(&self.design_values),
        };
        if let Some((cached_key, cached)) = self.sandbox.handle_cache.borrow().as_ref() {
            if *cached_key == key {
                return Rc::clone(cached);
            }
        }
        let built = match (self.typed_config(), self.current_design()) {
            (Some(config), Some(design)) => handles(&config, &design, self.sandbox.focus()),
            _ => Vec::new(),
        };
        let built = Rc::new(built);
        *self.sandbox.handle_cache.borrow_mut() = Some((key, Rc::clone(&built)));
        built
    }
}
