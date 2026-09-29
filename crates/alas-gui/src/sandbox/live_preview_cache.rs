// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keep the lofted exterior across camera updates in both guided previews.

use std::{cell::RefCell, rc::Rc};

use alas_config::{DesignVector, GeometryConfig};
use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::SandboxSceneModel;

#[derive(PartialEq)]
struct ModelKey {
    geometry: GeometryConfig,
    design: DesignVector,
    airfoil_generation: u64,
}

#[derive(Default)]
struct ModelCache {
    entry: Option<(ModelKey, Rc<SandboxSceneModel>)>,
}

impl ModelCache {
    fn get(&mut self, key: ModelKey) -> Option<Rc<SandboxSceneModel>> {
        if let Some((previous, model)) = &self.entry {
            if *previous == key {
                return Some(Rc::clone(model));
            }
        }
        let plane = AircraftBuilder::new(Some(key.geometry.clone()))
            .build(Some(&key.design), true)
            .ok()?;
        let model = Rc::new(super::build_sandbox_model(&plane));
        self.entry = Some((key, Rc::clone(&model)));
        Some(model)
    }
}

thread_local! {
    // A single bounded entry is sufficient for the dock and its detached
    // window, which share one aircraft. Exact input equality also makes reuse
    // safe across separate AppStates on this UI thread, without relying on a
    // revision increment at every public configuration mutation site.
    static CACHE: RefCell<ModelCache> = RefCell::new(ModelCache::default());
}

pub(super) fn model(
    geometry: GeometryConfig,
    design: DesignVector,
) -> Option<Rc<SandboxSceneModel>> {
    let key = ModelKey {
        geometry,
        design,
        // Imported airfoils can change the loft without changing their names.
        airfoil_generation: alas_geom::airfoil_io::generation(),
    };
    CACHE.with(|cache| cache.borrow_mut().get(key))
}

#[cfg(test)]
// Tests assert on known-good fixtures, where a panic is the failure report.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn key() -> ModelKey {
        ModelKey {
            geometry: GeometryConfig::default(),
            design: DesignVector::default(),
            airfoil_generation: 0,
        }
    }

    #[test]
    fn camera_reprojections_reuse_the_lofted_model() {
        let mut cache = ModelCache::default();
        let first = cache.get(key()).expect("aircraft builds");
        let second = cache.get(key()).expect("aircraft builds");
        assert!(Rc::ptr_eq(&first, &second));
        let options = super::super::live_preview_options();
        let original = first.render(None, None, &options).0;
        let rotated = second
            .render(
                Some(alas_report::scene::Camera3D {
                    azim_deg: 120.0,
                    ..Default::default()
                }),
                None,
                &options,
            )
            .0;
        assert_ne!(original, rotated);
    }

    #[test]
    fn design_geometry_and_airfoil_edits_each_rebuild_the_model() {
        let mut cache = ModelCache::default();
        let first = cache.get(key()).expect("aircraft builds");
        let mut design_edit = key();
        design_edit.design.span_m += 1.0;
        let edited = cache.get(design_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));

        let first = cache.get(key()).expect("aircraft builds");
        let mut geometry_edit = key();
        geometry_edit.geometry.wing.n_subdivisions += 1;
        let edited = cache.get(geometry_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));

        let first = cache.get(key()).expect("aircraft builds");
        let mut import_edit = key();
        import_edit.airfoil_generation += 1;
        let edited = cache.get(import_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));
    }
}
