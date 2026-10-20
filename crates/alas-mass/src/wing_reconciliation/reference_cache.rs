// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact reuse of prepared primary-wing-box solves.
//!
//! Candidate boxes retain their [`CANDIDATE_ENTRIES`] most recently used exact
//! cases per evaluation thread (the gear ledger, structural feasibility and
//! balance ledger of one evaluation ask for a handful of distinct inputs in
//! turn, and a later pass repeats them); the frozen reference uses a separate
//! process-wide cache and cannot evict those candidate entries. The reference key
//! retains the complete prepared geometry (including private airfoil and spar
//! arrays), structures, design-load requirements, engines, mass model,
//! materials, declared/geometric fuel state, mounted masses and root X datum.
//! Derived `Debug` representations round-trip finite floating-point values and
//! preserve signed zero; non-finite representations bypass reuse. The whole
//! representation is compared, so hash collisions cannot reuse another case.
//! A map lock only obtains a per-key cell; structural sizing runs outside it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::sizing::WingboxSizing;

use crate::wingbox_feedback::SizedWingboxMass;

use super::{DesignWingBox, WingReconciliationError};

type SizingResult = Result<(SizedWingboxMass, WingboxSizing), WingReconciliationError>;
type Cell<T> = Arc<OnceLock<Arc<T>>>;
type ReferenceCells<T> = OnceLock<Mutex<HashMap<String, Cell<T>>>>;
/// Most recently used exact candidate cases kept per thread. Hits need the
/// whole key to match, so the count changes only how often a solve repeats.
const CANDIDATE_ENTRIES: usize = 8;

/// Most recently used first.
type CandidateEntries = Vec<(String, Arc<SizingResult>)>;

thread_local! {
    static CANDIDATE: RefCell<CandidateEntries> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub(super) struct PreparedSizing<'a> {
    pub(super) geometry: WingStructureGeometry,
    pub(super) structures: &'a StructuresConfig,
    pub(super) requirements: &'a DesignRequirements,
    pub(super) engine: &'a EngineConfig,
    pub(super) mass_model: &'a MassModelConfig,
    pub(super) materials: [&'static MaterialSpec; 4],
    pub(super) declared_fuel: bool,
    pub(super) fuel: Vec<f64>,
    pub(super) wing_mounted: Vec<(f64, f64)>,
    pub(super) torque: alas_struct::torque_sources::TorqueSources,
    pub(super) main_gear_wing_fraction: f64,
    pub(super) root_x_m: f64,
}

impl PreparedSizing<'_> {
    fn cache_key(&self) -> Option<String> {
        let representation = format!("{self:?}");
        (!has_non_finite_token(&representation)).then_some(representation)
    }

    pub(super) fn solve(&self) -> SizingResult {
        let [skin, web, cap, rib] = self.materials;
        let sizing = alas_struct::sizing::size_wingbox_with_torque_sources(
            &self.geometry,
            self.structures,
            self.requirements,
            skin,
            web,
            cap,
            rib,
            self.declared_fuel.then_some(self.fuel.as_slice()),
            &self.wing_mounted,
            &self.torque,
            alas_struct::sizing::CaseSet::ALL,
        );
        let final_section = alas_struct::sizing::size_for_linear_model(
            &self.geometry,
            sizing,
            self.structures,
            self.requirements,
            self.engine,
            self.mass_model,
            skin,
            web,
            cap,
            &self.fuel,
            &self.wing_mounted,
            alas_struct::feasibility::LinearModelLimits {
                max_curvature_relative_error: self.structures.max_linear_curvature_relative_error,
            },
        );
        let sizing = final_section.sizing;
        // The shared arithmetic zero band makes the mass and downstream
        // structural gates apply one predicate to the same sized margins.
        let strength_margins_ok = sizing
            .spars
            .iter()
            .flat_map(|spar| spar.margin_of_safety.iter())
            .all(|margin| alas_struct::sizing::margin_is_structurally_non_negative(*margin));
        if !sizing.total_mass_kg.is_finite()
            || sizing.total_mass_kg <= 0.0
            || !strength_margins_ok
            || !sizing.rib_spacing_pass()
            || [
                sizing.mass_breakdown_kg.spar_caps,
                sizing.mass_breakdown_kg.spar_webs,
                sizing.mass_breakdown_kg.skin,
                sizing.mass_breakdown_kg.ribs,
            ]
            .iter()
            .any(|mass| !mass.is_finite() || *mass < 0.0)
        {
            return Err(WingReconciliationError::StructuralSizing);
        }
        let centroid =
            crate::wing_centroid::sized_wingbox_centroid(&self.geometry, &sizing, self.root_x_m)
                .map_err(|_| WingReconciliationError::StructuralSizing)?;
        let quantities = alas_struct::non_optimum::BoxQuantities::from_sizing(
            &self.geometry,
            &sizing,
            cap.rho_kg_m3,
        );
        let primary = SizedWingboxMass::symmetric_semiwing(sizing.total_mass_kg, centroid.xyz_m)
            .with_box_quantities(quantities)
            .with_main_gear_wing_fraction(self.main_gear_wing_fraction);
        Ok((primary, sizing))
    }
}

struct ReferenceCache<T> {
    cells: ReferenceCells<T>,
}

impl<T> ReferenceCache<T> {
    const fn new() -> Self {
        Self {
            cells: OnceLock::new(),
        }
    }

    fn get_or_resolve(&self, key: String, resolve: impl FnOnce() -> T) -> Arc<T> {
        let cell = Arc::clone(
            self.cells
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(key)
                .or_default(),
        );
        Arc::clone(cell.get_or_init(|| Arc::new(resolve())))
    }
}

/// Whether a derived `Debug` representation prints a non-finite float.
///
/// `f64`'s `Debug` writes `NaN`, `inf` and `-inf`. Only whole identifier
/// tokens match, so a field or material name that merely contains the
/// letters "inf" (for example `info`) does not silently disable reuse.
fn has_non_finite_token(representation: &str) -> bool {
    representation
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|token| token == "NaN" || token == "inf")
}

pub(super) fn reference_primary(
    prepared: PreparedSizing<'_>,
) -> Result<SizedWingboxMass, WingReconciliationError> {
    static CACHE: ReferenceCache<SizingResult> = ReferenceCache::new();
    let Some(key) = prepared.cache_key() else {
        return prepared.solve().map(|(primary, _)| primary);
    };
    match CACHE.get_or_resolve(key, || prepared.solve()).as_ref() {
        Ok((primary, _)) => Ok(*primary),
        Err(error) => Err(*error),
    }
}

fn candidate_result(prepared: &PreparedSizing<'_>) -> Arc<SizingResult> {
    let Some(key) = prepared.cache_key() else {
        return Arc::new(prepared.solve());
    };
    let cached = CANDIDATE.with(|entries| {
        let mut entries = entries.borrow_mut();
        let position = entries
            .iter()
            .position(|(cached_key, _)| *cached_key == key)?;
        // Move the hit to the front so the least recently used entry leaves.
        entries[..=position].rotate_right(1);
        Some(Arc::clone(&entries[0].1))
    });
    if let Some(result) = cached {
        return result;
    }
    let result = Arc::new(prepared.solve());
    CANDIDATE.with(|entries| {
        let mut entries = entries.borrow_mut();
        entries.insert(0, (key, Arc::clone(&result)));
        entries.truncate(CANDIDATE_ENTRIES);
    });
    result
}

pub(super) fn candidate_sizing(prepared: PreparedSizing<'_>) -> SizingResult {
    candidate_result(&prepared).as_ref().clone()
}

pub(super) fn candidate_design_box(
    prepared: PreparedSizing<'_>,
) -> Result<DesignWingBox, WingReconciliationError> {
    match candidate_result(&prepared).as_ref() {
        Ok((primary, sizing)) => Ok(DesignWingBox {
            primary: *primary,
            declaration: sizing.composite_declaration,
        }),
        Err(error) => Err(*error),
    }
}

#[cfg(test)]
// Fixture construction failures are test assertions, not library panics.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    #[test]
    fn cached_reference_sizing_is_bit_identical_to_direct_sizing_for_every_preset() {
        let cache = ReferenceCache::new();
        for name in [
            "A320-200",
            "A220-300",
            "A340-300",
            "A380-800",
            "B787-9",
            "DC-10",
            "ATR72-600",
            "AVE",
        ] {
            let preset = presets::get(name).expect("registered preset resolves");
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("registered configuration resolves");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .expect("registered nominal aircraft builds");
            let requirements = super::super::design_requirements(&config);
            let prepared = super::super::prepare_primary_wing(
                &config,
                &preset.design_vector,
                &plane,
                &requirements,
            )
            .expect("registered structural preparation succeeds");
            let key = prepared.cache_key().expect("registered inputs are finite");
            let direct = prepared
                .solve()
                .expect("registered direct primary sizing succeeds");
            let cached = cache.get_or_resolve(key.clone(), || prepared.solve());
            let repeated = cache.get_or_resolve(key, || {
                panic!("identical reference inputs must reuse their sized box")
            });
            assert!(Arc::ptr_eq(&cached, &repeated), "{name}");
            let resolved = cached.as_ref().as_ref().expect("cached sizing succeeds");
            assert_eq!(&direct, resolved, "{name}: sizing values");
            assert_eq!(sizing_bits(&direct), sizing_bits(resolved), "{name}: bits");
            let candidate = candidate_result(&prepared);
            let repeated_candidate = candidate_result(&prepared);
            assert!(Arc::ptr_eq(&candidate, &repeated_candidate), "{name}");
            assert_eq!(candidate.as_ref(), &Ok(direct.clone()), "{name}");
            assert_eq!(
                sizing_bits(
                    candidate
                        .as_ref()
                        .as_ref()
                        .expect("candidate sizing succeeds")
                ),
                sizing_bits(&direct),
                "{name}: candidate bits"
            );
            let public_sizing = super::super::sized_primary_wing(
                &config,
                &preset.design_vector,
                &plane,
                &requirements,
            )
            .expect("public sizing succeeds");
            assert_eq!(sizing_bits(&public_sizing), sizing_bits(&direct), "{name}");
            assert_eq!(
                super::super::size_design_wing_box(&config, &preset.design_vector, &plane),
                Ok(DesignWingBox {
                    primary: direct.0,
                    declaration: direct.1.composite_declaration,
                }),
                "{name}: shared box"
            );
            assert_eq!(reference_primary(prepared), Ok(direct.0), "{name}");
        }
    }

    #[test]
    fn candidate_cache_is_thread_local_and_keeps_several_recent_entries() {
        let preset = presets::get("A320-200").expect("registered preset resolves");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("registered configuration resolves");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .expect("registered nominal aircraft builds");
        let requirements = super::super::design_requirements(&config);
        let mut prepared = super::super::prepare_primary_wing(
            &config,
            &preset.design_vector,
            &plane,
            &requirements,
        )
        .expect("registered structural preparation succeeds");
        let original_x = prepared.root_x_m;
        let original = candidate_result(&prepared);
        assert!(original.is_ok());
        let concurrent = std::thread::scope(|scope| {
            scope
                .spawn(|| candidate_result(&prepared))
                .join()
                .unwrap_or_else(|error| std::panic::resume_unwind(error))
        });
        assert!(!Arc::ptr_eq(&original, &concurrent));
        assert_eq!(original.as_ref(), concurrent.as_ref());
        reference_primary(
            super::super::prepare_primary_wing(
                &config,
                &preset.design_vector,
                &plane,
                &requirements,
            )
            .expect("registered structural preparation succeeds"),
        )
        .expect("reference sizing succeeds");
        assert!(Arc::ptr_eq(&original, &candidate_result(&prepared)));
        prepared.root_x_m += 1.0;
        let shifted = candidate_result(&prepared);
        assert!(!Arc::ptr_eq(&original, &shifted));
        assert_eq!(shifted.as_ref(), &prepared.solve());
        prepared.root_x_m = original_x;
        // The earlier case is still held: restoring the input reuses it.
        let restored = candidate_result(&prepared);
        assert!(Arc::ptr_eq(&original, &restored));
        // The cache is bounded: more distinct cases than it holds evict the
        // least recently used one, and a re-solve is bit-identical.
        for step in 1..=CANDIDATE_ENTRIES {
            prepared.root_x_m = original_x + step as f64;
            let _ = candidate_result(&prepared);
        }
        prepared.root_x_m = original_x;
        let evicted = candidate_result(&prepared);
        assert!(!Arc::ptr_eq(&original, &evicted));
        assert_eq!(original.as_ref(), evicted.as_ref());
        assert_eq!(
            sizing_bits(original.as_ref().as_ref().expect("sized")),
            sizing_bits(evicted.as_ref().as_ref().expect("sized"))
        );
    }

    fn sizing_bits((primary, sizing): &(SizedWingboxMass, WingboxSizing)) -> Vec<u64> {
        let mut values = vec![
            primary.mass_kg,
            sizing.t_skin,
            sizing.rib_spacing_m,
            sizing.total_mass_kg,
            sizing.mass_breakdown_kg.spar_caps,
            sizing.mass_breakdown_kg.spar_webs,
            sizing.mass_breakdown_kg.skin,
            sizing.mass_breakdown_kg.ribs,
        ];
        values.extend(primary.centroid_m);
        for array in [
            &sizing.y_stations,
            &sizing.eta_stations,
            &sizing.chord,
            &sizing.spar_fracs,
        ] {
            values.extend(array);
        }
        for spar in &sizing.spars {
            values.push(spar.chord_fraction);
            for array in [
                &spar.h,
                &spar.w_cap,
                &spar.t_cap,
                &spar.a_cap,
                &spar.t_web,
                &spar.lower_extra_area,
                &spar.frac_moment,
                &spar.margin_of_safety,
            ] {
                values.extend(array);
            }
        }
        if let Some(relative_uncertainty) = sizing
            .composite_declaration
            .and_then(|declaration| declaration.relative_uncertainty)
        {
            values.push(relative_uncertainty);
        }
        values.into_iter().map(f64::to_bits).collect()
    }

    #[test]
    fn every_prepared_input_group_invalidates_the_reference_key() {
        let preset = presets::get("A320-200").expect("registered preset resolves");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("registered configuration resolves");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .expect("registered nominal aircraft builds");
        let requirements = super::super::design_requirements(&config);
        let prepare = || {
            super::super::prepare_primary_wing(
                &config,
                &preset.design_vector,
                &plane,
                &requirements,
            )
            .expect("registered structural preparation succeeds")
        };
        let key = prepare().cache_key().expect("finite nominal inputs");
        let mut changed = prepare();
        changed.geometry.c_root = f64::from_bits(changed.geometry.c_root.to_bits() + 1);
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut changed = prepare();
        changed.root_x_m = f64::from_bits(changed.root_x_m.to_bits() + 1);
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut changed = prepare();
        changed.fuel[0] = f64::from_bits(changed.fuel[0].to_bits() + 1);
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut changed = prepare();
        changed.declared_fuel = !changed.declared_fuel;
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut changed = prepare();
        changed.wing_mounted.push((1.0, 1.0));
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut structures = config.structures.clone();
        structures.t_skin_min_m = f64::from_bits(structures.t_skin_min_m.to_bits() + 1);
        let mut changed = prepare();
        changed.structures = &structures;
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut loads = requirements.clone();
        loads.mtow_kg = f64::from_bits(loads.mtow_kg.to_bits() + 1);
        let mut changed = prepare();
        changed.requirements = &loads;
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut engine = config.geometry.engine.clone();
        engine.z_m = f64::from_bits(engine.z_m.to_bits() + 1);
        let mut changed = prepare();
        changed.engine = &engine;
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut mass_model = config.mass_model.clone();
        mass_model.suspended_mass_fraction =
            f64::from_bits(mass_model.suspended_mass_fraction.to_bits() + 1);
        let mut changed = prepare();
        changed.mass_model = &mass_model;
        assert_ne!(changed.cache_key(), Some(key.clone()));
        let mut changed = prepare();
        changed.materials[2] =
            alas_config::materials::get("CFRP QI").expect("alternate material resolves");
        assert_ne!(changed.cache_key(), Some(key));
        let mut changed = prepare();
        changed.root_x_m = 0.0;
        let positive_zero = changed.cache_key();
        changed.root_x_m = -0.0;
        assert_ne!(changed.cache_key(), positive_zero);
        for non_finite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            changed.root_x_m = non_finite;
            assert_eq!(changed.cache_key(), None);
        }
    }

    #[test]
    fn concurrent_reference_requests_resolve_once_and_share_the_value() {
        let cache = ReferenceCache::new();
        let calls = AtomicUsize::new(0);
        let barrier = Barrier::new(8);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        cache.get_or_resolve("same prepared inputs".to_owned(), || {
                            calls.fetch_add(1, Ordering::SeqCst)
                        })
                    })
                })
                .collect();
            let results: Vec<_> = handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|error| std::panic::resume_unwind(error))
                })
                .collect();
            assert!(results
                .iter()
                .all(|result| Arc::ptr_eq(&results[0], result)));
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let first = cache.get_or_resolve("same prepared inputs".to_owned(), || usize::MAX);
        let other = cache.get_or_resolve("different prepared inputs".to_owned(), || {
            calls.fetch_add(1, Ordering::SeqCst)
        });
        assert!(!Arc::ptr_eq(&first, &other));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn independent_reference_keys_do_not_hold_the_map_lock_during_resolution() {
        // Both resolvers must be inside `resolve` at once. If the map lock were
        // held during resolution the second would never enter, so the
        // rendezvous is bounded and fails instead of hanging the suite.
        let cache = ReferenceCache::new();
        let inside = AtomicUsize::new(0);
        let rendezvous_reached = AtomicUsize::new(0);
        let results: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = ["one", "two"]
                .into_iter()
                .map(|key| {
                    let (cache, inside, reached) = (&cache, &inside, &rendezvous_reached);
                    scope.spawn(move || {
                        cache.get_or_resolve(key.to_owned(), || {
                            inside.fetch_add(1, Ordering::SeqCst);
                            let deadline = Instant::now() + Duration::from_secs(10);
                            while inside.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
                                std::thread::yield_now();
                            }
                            if inside.load(Ordering::SeqCst) == 2 {
                                reached.fetch_add(1, Ordering::SeqCst);
                            }
                            key.len()
                        })
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|error| std::panic::resume_unwind(error))
                })
                .collect()
        });
        assert_eq!(
            rendezvous_reached.load(Ordering::SeqCst),
            2,
            "resolutions of independent keys must overlap"
        );
        assert_eq!(
            results.iter().map(|value| **value).collect::<Vec<_>>(),
            [3, 3]
        );
    }

    #[test]
    fn only_whole_non_finite_float_tokens_bypass_the_cache() {
        for text in ["x: NaN,", "x: inf}", "[1.0, -inf]", "(NaN)"] {
            assert!(has_non_finite_token(text), "{text}");
        }
        for text in [
            "info: 1.0",
            "infill_factor: 2.0",
            "Nano",
            "t: 1e-7, 1.5e300",
        ] {
            assert!(!has_non_finite_token(text), "{text}");
        }
    }

    #[test]
    fn nominal_prepared_inputs_have_a_reuse_key() {
        let preset = presets::get("A320-200").expect("registered preset resolves");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("registered configuration resolves");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .expect("registered nominal aircraft builds");
        let requirements = super::super::design_requirements(&config);
        let prepared = super::super::prepare_primary_wing(
            &config,
            &preset.design_vector,
            &plane,
            &requirements,
        )
        .expect("registered structural preparation succeeds");
        assert!(prepared.cache_key().is_some());
    }
}
