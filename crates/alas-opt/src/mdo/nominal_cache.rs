// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cache for the registered aircraft's modelled reference quantities
//! that a reference adaptation compares every candidate with: the balance
//! nominal, the buffet reference wing and the tank capacity.
//!
//! Each is a pure function of the configuration, so the key is the complete
//! configuration: the preset name and a hash of its Debug rendering, which
//! prints every field. Two configurations that differ in any input (geometry,
//! mass model, structures, tanks, analysis mesh, requirements) never share a
//! value; the only staleness is a 64-bit hash collision, and a change that
//! does not move the quantity (a solver seed) costs one extra resolution, not
//! a wrong value. Each key resolves exactly once, also under concurrent
//! candidate evaluation: the map lock only hands out the key's cell, and the
//! cell's own once-initialisation serialises the resolution of that key
//! without blocking other keys.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use alas_config::AlasConfig;

/// The cache key of `config`: its preset and a hash of every field.
pub(crate) fn config_key(config: &AlasConfig) -> (String, u64) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{config:?}").hash(&mut hasher);
    (config.preset.clone(), hasher.finish())
}

/// `config` with no candidate main-gear translation: the configuration of
/// the registered aircraft, which has its published gear.
fn without_placement(config: &AlasConfig) -> std::borrow::Cow<'_, AlasConfig> {
    if config.landing_gear.derived_main_gear.is_none() {
        return std::borrow::Cow::Borrowed(config);
    }
    let mut cleared = config.clone();
    cleared.landing_gear.derived_main_gear = None;
    std::borrow::Cow::Owned(cleared)
}

/// One key's value, resolved at most once.
type Cell<T> = Arc<OnceLock<Option<T>>>;
/// A lazily created, locked map from the configuration key.
type KeyMap<V> = OnceLock<Mutex<HashMap<(String, u64), V>>>;

/// A process-wide map from a complete configuration key to one resolved
/// reference quantity, `None` when it could not be resolved.
pub(crate) struct NominalCache<T> {
    cells: KeyMap<Cell<T>>,
    #[cfg(test)]
    resolutions: KeyMap<usize>,
}

impl<T: Copy> NominalCache<T> {
    pub(crate) const fn new() -> Self {
        Self {
            cells: OnceLock::new(),
            #[cfg(test)]
            resolutions: OnceLock::new(),
        }
    }

    /// The value cached for `config`, resolving it with `resolve` the first
    /// time that configuration is seen.
    ///
    /// The registered aircraft has its published gear: a candidate's solved
    /// main-gear translation (`LandingGearConfig::derived_main_gear`) is
    /// cleared before the configuration is keyed and before `resolve` sees
    /// it, so every placed candidate shares the one nominal of its
    /// configuration and none is compared with a nominal carrying its gear.
    pub(crate) fn get_or_resolve(
        &self,
        config: &AlasConfig,
        resolve: impl FnOnce(&AlasConfig) -> Option<T>,
    ) -> Option<T> {
        let config = without_placement(config);
        let config = config.as_ref();
        let key = config_key(config);
        let cell = Arc::clone(
            self.cells
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(key.clone())
                .or_default(),
        );
        *cell.get_or_init(|| {
            #[cfg(test)]
            {
                *self
                    .resolutions
                    .get_or_init(Mutex::default)
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(key)
                    .or_default() += 1;
            }
            resolve(config)
        })
    }

    /// How many times the value of `config` was resolved.
    #[cfg(test)]
    pub(crate) fn resolutions(&self, config: &AlasConfig) -> usize {
        self.resolutions
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&config_key(without_placement(config).as_ref()))
            .copied()
            .unwrap_or(0)
    }
}
