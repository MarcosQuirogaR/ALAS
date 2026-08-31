// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

impl FullAnalysis {
    /// Create a new analysis orchestrator for `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self {
            config,
            reference_compatibility: false,
        }
    }

    /// Create a product analysis that preserves engine values supplied by an
    /// imported aircraft-data document.
    ///
    /// Kept as an explicit interchange-data entry point. Product constructors
    /// preserve live engine fields regardless of whether they came from CPACS,
    /// a preset, a saved configuration, or the engine designer.
    pub fn new_preserving_engine_config(config: AlasConfig) -> Self {
        Self {
            config,
            reference_compatibility: false,
        }
    }

    /// Construct a full analysis that reproduces the frozen Python wing point.
    ///
    /// Product analyses use [`Self::new`]. This explicit compatibility path is
    /// reserved for the port's reference fixture and historical sensitivity
    /// artifacts, so the physical structural-centroid correction does not
    /// erase evidence of what the translated implementation did.
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        config.geometry.engine.apply_engine_spec();
        Self {
            config,
            reference_compatibility: true,
        }
    }

    /// Execute the full analysis workflow on `design`.
    pub fn run(
        &self,
        design: &DesignVector,
        include_engines: bool,
    ) -> Result<AnalysisReport, String> {
        let builder = if self.reference_compatibility {
            AircraftBuilder::new_reference_compatibility(Some(self.config.geometry.clone()))
        } else {
            AircraftBuilder::new(Some(self.config.geometry.clone()))
        };
        let plane = builder
            .build(Some(design), include_engines)
            .map_err(|e| format!("geometry build error: {e:?}"))?;
        self.run_on_airplane(design, plane)
    }

    /// Execute the same analysis workflow on an aircraft supplied by a CPACS
    /// interchange document.
    ///
    /// The aerodynamic, mass, stability, and feasibility formulas remain the
    /// same as [`Self::run`]. Only geometry ownership changes: the caller has
    /// already decoded and validated the aircraft data.
    pub fn run_on_airplane(
        &self,
        design: &DesignVector,
        airplane: Airplane,
    ) -> Result<AnalysisReport, String> {
        let req = &self.config.requirements;
        let mut plane = airplane;

        // The explicit compatibility constructor replays the historical
        // unfolded wing normalization.  `AircraftBuilder` now publishes the
        // projected XY reference by default, so restore the old normalization
        // at this pipeline boundary instead of changing the authoritative
        // geometry API (or silently masking the product convention).
        if self.reference_compatibility {
            if let Some(main_wing) = plane.wings.first() {
                plane.s_ref = main_wing.unfolded_area();
            }
        }

        // First pass with lumped payload to determine OEW and approximate CG.
        let coordinate_model = if self.reference_compatibility {
            MassCoordinateModel::ReferenceCompatibility
        } else {
            MassCoordinateModel::StructuralWingbox(&self.config.structures)
        };
        let initial_mass_result = if self.reference_compatibility {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                None,
                coordinate_model,
                &self.config.landing_gear,
            )
        } else {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                None,
                coordinate_model,
                &self.config.landing_gear,
            )
        };
        let (masses_init, coords_init, _cg_init) =
            initial_mass_result.map_err(|error| format!("mass-coordinate error: {error}"))?;

        // Second pass: build detailed interior layout and recompute mass breakdown and CG.
        let (oew, x_oew) = oew_and_cg(&masses_init, &coords_init);
        let effective_structural_payload_limit_kg =
            effective_structural_payload_limit_kg(&self.config, design, oew);
        let mut payload_config = self.config.clone();
        if let Some(limit_kg) = effective_structural_payload_limit_kg {
            payload_config.requirements.max_structural_payload_kg = limit_kg;
        }
        let payload_layout = if self.reference_compatibility {
            build_payload_layout_reference_compatibility(&plane, &self.config, oew, x_oew)
        } else {
            build_payload_layout(&plane, &payload_config, oew, x_oew)
        }
        .map_err(|error| format!("payload layout error: {error}"))?;
        let layout_summary = Some(alas_mass::breakdown::PayloadLayoutSummary {
            total_mass: payload_layout.total_mass,
            cg_x: payload_layout.cg_x,
            cg_y: payload_layout.cg_y,
        });

        let detailed_mass_result = if self.reference_compatibility {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                layout_summary.as_ref(),
                coordinate_model,
                &self.config.landing_gear,
            )
        } else {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                layout_summary.as_ref(),
                coordinate_model,
                &self.config.landing_gear,
            )
        };
        let (masses, coords, cg) =
            detailed_mass_result.map_err(|error| format!("mass-coordinate error: {error}"))?;

        // Anchor the aerodynamic moment reference to the actual physical CG.
        plane.xyz_ref[0] = cg[0];

        // Fine resolution configuration for reporting.
        let mut fine_analysis = self.config.analysis.clone();
        fine_analysis.spanwise_resolution = fine_analysis.fine_spanwise_resolution;
        fine_analysis.chordwise_resolution = fine_analysis.fine_chordwise_resolution;

        let aero = if self.reference_compatibility {
            AeroAnalysis::new_reference_compatibility(
                &plane,
                design.sweep_deg,
                Some(self.config.geometry.clone()),
                Some(self.config.drag_model.clone()),
                Some(fine_analysis.clone()),
            )
        } else {
            AeroAnalysis::new(
                &plane,
                design.sweep_deg,
                Some(self.config.geometry.clone()),
                Some(self.config.drag_model.clone()),
                Some(fine_analysis.clone()),
            )
        };

        let polar = aero
            .run_sweep(req.cruise_mach, req.cruise_altitude_m)
            .map_err(|e| format!("polar sweep error: {e:?}"))?;

        let design_point = self.compute_design_point(&plane, &polar);
        let polar_fit = self.fit_polar(&plane, &polar);

        let (x_np, sm, _) = if self.reference_compatibility {
            neutral_point_reference_compatibility(&plane, &fine_analysis)
        } else {
            neutral_point(&plane, &fine_analysis)
        }
        .map_err(|e| format!("neutral point error: {e:?}"))?;

        let cg_envelope_ok = if self.reference_compatibility {
            let env = check_cg_envelope(
                &plane,
                &masses,
                &coords,
                cg[0],
                x_np,
                plane.c_ref,
                &self.config,
            );
            Some(!env.violation)
        } else {
            let env = assess_model_cg_envelope(
                &plane,
                &masses,
                &coords,
                cg[0],
                x_np,
                plane.c_ref,
                &self.config,
            )
            .map_err(|error| format!("model CG assessment error: {error}"))?;
            Some(env.hard_constraints_pass())
        };

        // Trimmed cruise operating point.
        let trimmed_design_point = self.compute_trimmed_design_point(&plane, &aero, &fine_analysis);

        let mut geometry_summary = self.geometry_summary(&plane, design);
        if let Some(limit_kg) = effective_structural_payload_limit_kg {
            geometry_summary.insert(
                "effective_structural_payload_limit_kg".to_owned(),
                limit_kg,
            );
        }

        Ok(AnalysisReport {
            design: *design,
            airplane: plane.clone(),
            polar,
            design_point,
            polar_fit,
            static_margin: sm,
            x_neutral_point: x_np,
            geometry_summary,
            component_masses: breakdown_to_map(&masses),
            mass_coordinates: coordinates_to_map(&coords),
            physical_cg: cg,
            payload_layout: Some(payload_layout),
            trimmed_design_point,
            cg_envelope_ok,
        })
    }

}

/// Resolve the structural payload bound for an unchanged registered preset.
///
/// The configuration cap is normally the published `MZFW - OEW` value. The
/// product mass method is an estimate, however, and its modeled OEW can be
/// heavier than the source OEW. In that case the safe payload bound is the
/// smaller of the configured cap and `MZFW - modeled OEW`; otherwise the
/// detailed layout would appear to respect the payload cap while still
/// producing an overweight zero-fuel mass. Modified/notional designs do not
/// inherit a published MZFW from a preset.
fn effective_structural_payload_limit_kg(
    config: &AlasConfig,
    design: &DesignVector,
    modeled_oew_kg: f64,
) -> Option<f64> {
    let preset = presets::get(&config.preset).ok()?;
    if *design != preset.design_vector {
        return None;
    }
    let mzfw_kg = preset.reference.mzfw_kg?;
    let available_payload_kg = mzfw_kg - modeled_oew_kg;
    if !mzfw_kg.is_finite() || !modeled_oew_kg.is_finite() || available_payload_kg <= 0.0 {
        return None;
    }
    let configured_limit_kg = config.requirements.max_structural_payload_kg;
    Some(if configured_limit_kg.is_finite() && configured_limit_kg > 0.0 {
        configured_limit_kg.min(available_payload_kg)
    } else {
        available_payload_kg
    })
}
