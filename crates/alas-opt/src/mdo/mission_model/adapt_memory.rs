// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the last adaptations of a model learned, shared by its clones.

use std::sync::{Arc, Mutex};

use super::profile::LegKind;

/// What the last adaptations learned, shared by clones: the last recovered
/// flown altitude per leg (grid index) and the last revised scale of each
/// climb rung per leg. Both only change the cost of a search, never its
/// answer.
#[derive(Debug, Clone, Default)]
pub(crate) struct AdaptationMemory(Arc<Mutex<MemorySlots>>);

#[derive(Debug, Default)]
struct MemorySlots {
    levels: [Option<i64>; 2],
    rung_scales: [[Option<f64>; 3]; 2],
}

impl PartialEq for AdaptationMemory {
    fn eq(&self, _other: &Self) -> bool {
        // A warm start changes cost, never the answer.
        true
    }
}

impl AdaptationMemory {
    fn slot(leg: LegKind) -> usize {
        match leg {
            LegKind::Trip => 0,
            LegKind::Diversion => 1,
        }
    }

    pub(super) fn get(&self, leg: LegKind) -> Option<i64> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).levels[Self::slot(leg)]
    }

    pub(super) fn set(&self, leg: LegKind, index: i64) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).levels[Self::slot(leg)] = Some(index);
    }

    pub(super) fn rung_scale(&self, leg: LegKind, rung: usize) -> Option<f64> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).rung_scales[Self::slot(leg)][rung - 1]
    }

    pub(super) fn set_rung_scale(&self, leg: LegKind, rung: usize, scale: f64) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).rung_scales[Self::slot(leg)][rung - 1] =
            Some(scale);
    }
}
