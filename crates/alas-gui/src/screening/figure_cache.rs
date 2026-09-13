// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Completed screening scenes depend on result revision, theme and language only.

use std::{collections::HashMap, sync::Arc};

use alas_report::{families::screening::*, scene::Scene};
use alas_screen::types::AirfoilScreeningResult;

#[derive(Default)]
pub(crate) struct ScreeningFigureCache {
    identity: Option<(u64, String, String)>,
    scenes: HashMap<&'static str, Option<Arc<Scene>>>,
    generation: u64,
}

impl ScreeningFigureCache {
    pub(crate) fn clear(&mut self) {
        self.identity = None;
        self.scenes.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn revision(&self) -> u64 {
        self.generation
    }

    pub(crate) fn get(
        &mut self,
        result: &AirfoilScreeningResult,
        revision: u64,
        theme: &str,
        language: &str,
        id: &'static str,
    ) -> Option<Arc<Scene>> {
        let matches = self
            .identity
            .as_ref()
            .is_some_and(|key| key.0 == revision && key.1 == theme && key.2 == language);
        if !matches {
            self.clear();
            self.identity = Some((revision, theme.to_owned(), language.to_owned()));
        }
        self.scenes
            .entry(id)
            .or_insert_with(|| {
                let scene = match id {
                    "trade_map" => fig_trade_map(result, Some(theme)),
                    "rerank_2d_3d" => fig_rerank_2d_3d(result, Some(theme)),
                    "ranking_bars" => fig_ranking_bars(result, Some(theme)),
                    "mses_verification" => fig_mses_verification(result, Some(theme)),
                    "section_shapes" => fig_section_shapes(result, Some(theme)),
                    _ => None,
                };
                scene
                    .map(crate::scene::localize_scene_for_display)
                    .map(Arc::new)
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_result() -> AirfoilScreeningResult {
        AirfoilScreeningResult {
            n_total: 1,
            n_ok: 1,
            candidates: vec![alas_screen::types::AirfoilCandidateResult {
                name: "naca0012".to_owned(),
                status: "ok".to_owned(),
                l_over_d: Some(42.0),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn completed_figures_reuse_scenes_and_invalidate_every_display_dependency() {
        let result = completed_result();
        let mut cache = ScreeningFigureCache::default();
        let mut previous = None;
        for (revision, theme, language) in [
            (1, "light", "en"),
            (2, "light", "en"),
            (2, "dark", "en"),
            (2, "grey", "en"),
            (2, "grey", "es"),
        ] {
            alas_i18n::set_language(Some(language));
            let first = cache.get(&result, revision, theme, language, "ranking_bars");
            assert!(first.is_some());
            let generation = cache.revision();
            if let Some(old) = previous {
                assert_ne!(generation, old);
            }
            for _ in 0..100 {
                let next = cache.get(&result, revision, theme, language, "ranking_bars");
                assert_eq!(generation, cache.revision());
                match (&first, &next) {
                    (Some(a), Some(b)) => assert!(Arc::ptr_eq(a, b)),
                    (None, None) => {}
                    _ => panic!("cached availability changed"),
                }
            }
            assert_eq!(cache.scenes.len(), 1);
            previous = Some(generation);
        }
        cache.clear();
        assert!(cache.scenes.is_empty());
        assert!(cache.identity.is_none());
        assert_ne!(Some(cache.revision()), previous);
    }

    #[test]
    fn completion_invalidates_cached_scenes_and_unavailable_figures_are_cached() {
        use crate::screening::{ScreeningMessage, ScreeningState};
        let mut state = ScreeningState::default();
        let result = completed_result();
        let first = state
            .figure_cache
            .get(&result, 0, "light", "en", "ranking_bars");
        let revision = state.figure_cache.revision();
        assert!(state
            .figure_cache
            .get(&result, 0, "light", "en", "mses_verification")
            .is_none());
        assert_eq!(state.figure_cache.scenes.len(), 2);
        let (tx, rx) = std::sync::mpsc::channel();
        state.rx = Some(rx);
        assert!(tx
            .send(ScreeningMessage::Finished(Box::new(Ok(result.clone()))))
            .is_ok());
        state.poll();
        assert_eq!(state.result_revision, 1);
        assert!(state.figure_cache.scenes.is_empty());
        let next = state.figure_cache.get(
            &result,
            state.result_revision,
            "light",
            "en",
            "ranking_bars",
        );
        assert_ne!(state.figure_cache.revision(), revision);
        if let (Some(a), Some(b)) = (first, next) {
            assert!(!Arc::ptr_eq(&a, &b));
        } else {
            panic!("completed ranking figure must be available");
        }
    }
}
