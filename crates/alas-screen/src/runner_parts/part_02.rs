// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


// The private dispatcher carries the explicit parity/product mode seam.
#[allow(clippy::too_many_arguments)]
fn run_airfoil_screening_with_mass_model(
    config: &AlasConfig,
    dv: Option<&DesignVector>,
    options: &AirfoilScreeningOptions,
    mses_dir: Option<&Path>,
    mut progress_callback: Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&dyn Fn() -> bool>,
    mass_model: ScreeningMassModel,
) -> Result<AirfoilScreeningResult, String> {
    if !options.alpha_min_deg.is_finite()
        || !options.alpha_max_deg.is_finite()
        || !options.alpha_step_deg.is_finite()
        || options.alpha_step_deg <= 0.0
        || options.alpha_max_deg < options.alpha_min_deg
    {
        return Err("screening alpha sweep requires finite ordered bounds and a positive step".to_owned());
    }
    let geometry = match mass_model {
        ScreeningMassModel::ReferenceCompatibility => ScreeningGeometry::ReferenceCompatibility,
        ScreeningMassModel::StructuralWingbox => ScreeningGeometry::Product,
    };
    let dv_val = dv.copied().unwrap_or_default();
    let (mach, reynolds, level_flight_cl, altitude) =
        cruise_condition_with_geometry(config, &dv_val, geometry)?;
    let section_mach = mach * dv_val.sweep_deg.to_radians().cos();
    let cl_target = options.target_cl.unwrap_or(level_flight_cl);
    if !cl_target.is_finite() || cl_target <= 0.0 {
        return Err("screening target CL must be finite and positive".to_owned());
    }
    let flow_regime = ScreeningFlowRegime::from_section_mach(section_mach);
    let (ld_weight, fuel_weight, robustness_weight) = options.objective.weights((
        options.ld_weight,
        options.fuel_weight,
        options.robustness_weight,
    ));

    let all_names = AirfoilLibrary::get_available_airfoils();
    let names = filter_names(&all_names, &options.name_filter);
    let n_total = names.len();

    let n_alpha = (((options.alpha_max_deg - options.alpha_min_deg)
        / options.alpha_step_deg.max(0.1))
    .round() as usize)
        + 1;
    let alphas_deg = linspace(options.alpha_min_deg, options.alpha_max_deg, n_alpha);

    let model_size = ModelSize::from_name(&options.model_size).unwrap_or(ModelSize::Large);

    let mut results: Vec<AirfoilCandidateResult> = Vec::new();
    let mut errors: Vec<HashMap<String, String>> = Vec::new();
    let mut cancelled = false;

    // Stage 1: 2-D screening
    for (i, name) in names.iter().enumerate() {
        if let Some(cancel_fn) = should_cancel {
            if cancel_fn() {
                cancelled = true;
                break;
            }
        }

        let cand = score_candidate_with_geometry(
            name,
            config,
            &dv_val,
            section_mach,
            reynolds,
            cl_target,
            config.mass_model.fuel_tank_usable_fraction,
            &alphas_deg,
            model_size,
            options.min_tc,
            options.max_tc,
            options.cl_band,
            geometry,
        );

        if cand.status == "ok" {
            results.push(cand);
        } else {
            let mut err_map = HashMap::new();
            err_map.insert("name".to_string(), name.clone());
            err_map.insert(
                "error".to_string(),
                cand.error.unwrap_or_else(|| "unknown error".to_string()),
            );
            errors.push(err_map);
        }

        if let Some(ref mut cb) = progress_callback {
            if (i + 1) % 50 == 0 || i + 1 == n_total {
                let msg = format!(
                    "Stage 1 (2-D): {}/{} evaluated -- {} ok, {} errors",
                    i + 1,
                    n_total,
                    results.len(),
                    errors.len()
                );
                cb(&msg);
            }
        }
    }

    let n_ok_stage1 = results.len();

    if !results.is_empty() {
        blend_scores(
            &mut results,
            ld_weight,
            fuel_weight,
            robustness_weight,
            |r| r.l_over_d.unwrap_or(0.0),
            false,
        );
        results.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
    }

    let reference_set: HashSet<&str> = REFERENCE_AIRFOILS.iter().copied().collect();
    for r in &mut results {
        if reference_set.contains(r.name.as_str()) {
            r.is_reference = true;
        }
    }

    // Stage 2: 3-D refinement
    let mut n_refined = 0;
    if options.refine_3d && !results.is_empty() && options.refine_top_n > 0 && !cancelled {
        let top_n_limit = options.refine_top_n.min(results.len());
        let mut shortlist_indices: Vec<usize> = (0..top_n_limit).collect();
        for (idx, r) in results.iter().enumerate().skip(top_n_limit) {
            if r.is_reference && !shortlist_indices.contains(&idx) {
                shortlist_indices.push(idx);
            }
        }

        for (step, &idx) in shortlist_indices.iter().enumerate() {
            if let Some(cancel_fn) = should_cancel {
                if cancel_fn() {
                    cancelled = true;
                    break;
                }
            }

            refine_candidate_3d_with_mass_model(
                &mut results[idx],
                config,
                &dv_val,
                mach,
                altitude,
                cl_target,
                options.min_static_margin,
                mass_model,
                geometry,
            );

            if results[idx].refined {
                n_refined += 1;
            }

            if let Some(ref mut cb) = progress_callback {
                let msg = format!(
                    "Stage 2 (3-D wing): {}/{} re-simulated -- {} ok",
                    step + 1,
                    shortlist_indices.len(),
                    n_refined
                );
                cb(&msg);
            }
        }

        let mut refined_candidates: Vec<AirfoilCandidateResult> =
            results.iter().filter(|r| r.refined).cloned().collect();

        if !refined_candidates.is_empty() {
            blend_scores(
                &mut refined_candidates,
                ld_weight,
                fuel_weight,
                robustness_weight,
                |r| r.l_over_d_3d.unwrap_or(0.0),
                true,
            );

            let refined_map: HashMap<String, f64> = refined_candidates
                .into_iter()
                .filter_map(|r| r.score_3d.map(|s| (r.name, s)))
                .collect();

            for r in &mut results {
                if let Some(&s3d) = refined_map.get(&r.name) {
                    r.score_3d = Some(s3d);
                }
            }
        }

        results.sort_by(|a, b| {
            let key_a = (
                a.refined,
                a.score_3d.unwrap_or(-1.0),
                a.score.unwrap_or(0.0),
            );
            let key_b = (
                b.refined,
                b.score_3d.unwrap_or(-1.0),
                b.score.unwrap_or(0.0),
            );
            key_b
                .partial_cmp(&key_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Stage 3: MSES verification
    let mut n_mses_verified = 0;
    if options.verify_mses && !results.is_empty() && options.mses_top_n > 0 && !cancelled {
        if let Some(dir) = mses_dir {
            let refined_indices: Vec<usize> = results
                .iter()
                .enumerate()
                .filter(|(_, r)| r.refined)
                .map(|(i, _)| i)
                .collect();

            let mses_limit = options.mses_top_n.min(refined_indices.len());
            let mut mses_indices: Vec<usize> =
                refined_indices.iter().take(mses_limit).copied().collect();

            for &idx in &refined_indices[mses_limit..] {
                if results[idx].is_reference && !mses_indices.contains(&idx) {
                    mses_indices.push(idx);
                }
            }

            let cancellation = Arc::new(AtomicBool::new(false));
            if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
                cancellation.store(true, Ordering::Relaxed);
                cancelled = true;
            }
            let mut completed_mses = 0;
            let mut ordered_verified = vec![None; mses_indices.len()];
            let completed = run_bounded_indexed(
                mses_indices.len(),
                MAX_MSES_WORKERS,
                cancellation.clone(),
                |job_index| {
                    let idx = mses_indices[job_index];
                    let mut candidate = results[idx].clone();
                    let worker_cancellation = cancellation.clone();
                    let worker_cancel = || worker_cancellation.load(Ordering::Relaxed);
                    verify_candidate_mses(
                        &mut candidate,
                        config,
                        &dv_val,
                        mach,
                        altitude,
                        cl_target,
                        dir,
                        Some(&worker_cancel),
                    );
                    candidate
                },
                |job_index, candidate| {
                    ordered_verified[job_index] = Some(candidate.mses_verified);
                    while completed_mses < ordered_verified.len() {
                        let Some(verified) = ordered_verified[completed_mses].take() else {
                            break;
                        };
                        completed_mses += 1;
                        if verified {
                            n_mses_verified += 1;
                        }
                        if let Some(ref mut cb) = progress_callback {
                            let msg = format!(
                                "Stage 3 (MSES): {}/{} verified -- {} ok",
                                completed_mses,
                                mses_indices.len(),
                                n_mses_verified
                            );
                            cb(&msg);
                        }
                    }
                    if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
                        cancellation.store(true, Ordering::Relaxed);
                    }
                },
            );
            for (job_index, candidate) in completed {
                results[mses_indices[job_index]] = candidate;
            }
            n_mses_verified = mses_indices
                .iter()
                .filter(|&&index| results[index].mses_verified)
                .count();
            if cancellation.load(Ordering::Relaxed) {
                cancelled = true;
            }

            let mut verified: Vec<AirfoilCandidateResult> = results
                .iter()
                .filter(|r| r.mses_verified)
                .cloned()
                .collect();

            if !verified.is_empty() {
                verified.sort_by(|a, b| {
                    b.l_over_d_mses
                        .unwrap_or(0.0)
                        .total_cmp(&a.l_over_d_mses.unwrap_or(0.0))
                });
                let verified_names: HashSet<String> =
                    verified.iter().map(|r| r.name.clone()).collect();
                let rest: Vec<AirfoilCandidateResult> = results
                    .into_iter()
                    .filter(|r| !verified_names.contains(&r.name))
                    .collect();

                results = verified;
                results.extend(rest);
            }
        } else {
            mark_mses_not_configured(&mut results);
        }
    }

    let top_candidates: Vec<AirfoilCandidateResult> =
        results.into_iter().take(options.top_n).collect();

    Ok(AirfoilScreeningResult {
        baseline_airfoil: config.geometry.wing.root_airfoil.clone(),
        cruise_mach: mach,
        cruise_reynolds: reynolds,
        cruise_altitude_m: altitude,
        cl_target,
        uses_explicit_target_cl: options.target_cl.is_some(),
        section_mach,
        flow_regime,
        transonic_caveat: section_mach >= TRANSONIC_MACH_CAVEAT,
        refined_3d: options.refine_3d && n_refined > 0,
        n_total,
        n_ok: n_ok_stage1,
        n_error: errors.len(),
        n_refined,
        n_mses_verified,
        cancelled,
        candidates: top_candidates,
        errors: errors.into_iter().take(100).collect(),
    })
}

/// Run indexed jobs with a bounded worker count and return results by index.
///
/// Workers claim indices atomically and only the coordinator invokes the
/// progress callback. This keeps cancellation and callback ordering stable,
/// while sorting the collected results prevents completion timing from
/// changing the ranked candidate order.
fn run_bounded_indexed<T, F, P>(
    job_count: usize,
    worker_limit: usize,
    cancellation: Arc<AtomicBool>,
    job: F,
    mut on_result: P,
) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
    P: FnMut(usize, &T),
{
    if job_count == 0 || cancellation.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let workers = job_count.min(worker_limit.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = channel();
    let mut output = Vec::with_capacity(job_count);
    thread::scope(|scope| {
        let job_ref = &job;
        for _ in 0..workers {
            let sender = sender.clone();
            let next = next.clone();
            let cancellation = cancellation.clone();
            scope.spawn(move || loop {
                if cancellation.load(Ordering::Relaxed) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= job_count {
                    break;
                }
                let value = job_ref(index);
                if sender.send((index, value)).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        for (index, value) in receiver {
            on_result(index, &value);
            output.push((index, value));
        }
    });
    output.sort_by_key(|(index, _)| *index);
    output
}

/// Preserve the ranking while making a selected-but-unresolved Stage 3 visible.
fn mark_mses_not_configured(candidates: &mut [AirfoilCandidateResult]) {
    for candidate in candidates.iter_mut().filter(|candidate| candidate.refined) {
        candidate.mses_status = Some("not_configured".to_owned());
        candidate.mses_error = Some(
            "MSES verification was selected, but no MSES installation was resolved. Configure Setup > External Tools."
                .to_owned(),
        );
    }
}
