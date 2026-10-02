// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Construction and the full run of a [`FullAnalysis`].

use super::*;

impl FullAnalysis {
    /// Create a new analysis orchestrator for `config`.
    pub fn new(config: AlasConfig) -> Self {
        let reference_compatibility = config.mass_model.uses_reference_mass_methods();
        Self {
            config,
            reference_compatibility,
        }
    }

    /// Create a product analysis that preserves engine values supplied by an
    /// imported aircraft-data document.
    ///
    /// Kept as an explicit interchange-data entry point. Product constructors
    /// preserve live engine fields regardless of whether they came from CPACS,
    /// a preset, a saved configuration, or the engine designer.
    pub fn new_preserving_engine_config(config: AlasConfig) -> Self {
        Self::new(config)
    }

    /// Construct a full analysis that reproduces the frozen Python wing point.
    ///
    /// Product analyses use [`Self::new`]. This explicit compatibility path is
    /// reserved for the frozen reference fixtures, so the physical
    /// structural-centroid correction does not alter their expected values.
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        config.geometry.engine.apply_engine_spec();
        // The constructor is the explicit opt-in for the retained comparison
        // buildup.  Make that intent authoritative even when the supplied
        // configuration came from the pure-FLOPS product default; otherwise
        // the compatibility geometry path would still ask the FLOPS evaluator
        // for a complete transport contract.
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        config.mass_model.apply_architecture();
        config.analysis.restore_reference_mesh();
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
        // Premium economy is retained in saved cabins for compatibility, but
        // the product cabin and FLOPS contract have three classes. Fold that
        // slot before either pass builds a payload or prices the cabin;
        // otherwise a declared Premium count can be added to FLOPS tourist
        // while the row packer silently ignores it.
        let product_cabin = self.config.cabin.passenger.canonicalized_for_product();
        let mut product_cabin_config = self.config.cabin.clone();
        product_cabin_config.passenger = product_cabin.clone();
        // A cabin declared by count is the first pass's cabin too (`cabin_sync`).
        let (declared_requirements, analysis_mass_model) = cabin_sync::declared_cabin(
            &self.config.requirements,
            &self
                .config
                .analysis_mass_model(self.config.requirements.mtow_kg),
            &product_cabin,
        );
        let req = &declared_requirements;
        let mut plane = airplane;

        // The compatibility replay keeps the historical unfolded-area reference.
        if self.reference_compatibility {
            if let Some(main_wing) = plane.wings.first() {
                plane.s_ref = main_wing.unfolded_area();
            }
        }

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
                &product_cabin_config,
                &self.config.control_surfaces,
                Some(&analysis_mass_model),
                None,
                coordinate_model,
                &self.config.landing_gear,
            )
        };
        let (masses_init, coords_init, _cg_init) =
            initial_mass_result.map_err(|error| format!("mass-coordinate error: {error}"))?;
        let (coords_init, _) =
            self.station_coordinates(design, &plane, &masses_init, coords_init)?;

        // Second pass: build detailed interior layout and recompute mass breakdown and CG.
        let (oew, x_oew) = oew_and_cg(&masses_init, &coords_init);
        let first_pass = payload_pass::FirstPass {
            requirements: req,
            mass_model: &analysis_mass_model,
            cabin: &product_cabin,
            oew_kg: oew,
            x_oew_m: x_oew,
        };
        let payload_pass::PayloadPass {
            layout: payload_layout,
            structural_payload_limit_kg: effective_structural_payload_limit_kg,
            analysis: (masses, coords, _, flops_mass_buildup),
        } = self.payload_pass(design, &plane, &first_pass, coordinate_model)?;
        let (coords, cg) = self.station_coordinates(design, &plane, &masses, coords)?;
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
                AeroAnalysis::quarter_chord_sweep_deg(&plane, design.sweep_deg),
                Some(self.config.geometry.clone()),
                Some(self.config.drag_model.clone()),
                Some(fine_analysis.clone()),
            )
        };

        let spanwise = fine_analysis.spanwise_resolution.max(1) as usize;
        let chordwise = fine_analysis.chordwise_resolution.max(1) as usize;
        let fine_system = VlmSystem::assemble(&plane, spanwise, chordwise)
            .map_err(|e| format!("vlm assembly error: {e:?}"))?;

        let polar = aero
            .run_sweep_with_system(&fine_system, req.cruise_mach, req.cruise_altitude_m)
            .map_err(|e| format!("polar sweep error: {e:?}"))?;

        let component_masses = breakdown_to_map(&masses);
        let zero_fuel_mass_kg = crate::cruise_mass::zero_fuel_mass_kg(&component_masses);
        // The trim solution and everything that constrains the aircraft run at
        // the takeoff-mass lift coefficient; only the reported cruise CL, CD
        // and L/D move to the mid-cruise mass (`crate::cruise_mass`).
        let takeoff_cl = self.cruise_cl(&plane);
        let cruise_cl = self.reported_cruise_cl(&plane, zero_fuel_mass_kg);
        let design_point = self.compute_design_point(&polar, cruise_cl)?;
        let polar_fit = self.fit_polar(&plane, &polar);

        let (x_np, sm, _) = if self.reference_compatibility {
            neutral_point_reference_compatibility_with_system(&fine_system, &plane, &fine_analysis)
        } else {
            neutral_point_with_system(&fine_system, &plane, &fine_analysis)
        }
        .map_err(|e| format!("neutral point error: {e:?}"))?;

        let np_cond = self.np_conditions_for(&plane, &fine_analysis, req);

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
            let critical_x_np = np_cond.as_ref().map_or(x_np, |cond| cond.critical);
            let env = assess_model_cg_envelope(
                &plane,
                &masses,
                &coords,
                cg[0],
                x_np,
                critical_x_np,
                plane.c_ref,
                &self.config,
            )
            .map_err(|error| format!("model CG assessment error: {error}"))?;
            Some(env.hard_constraints_pass())
        };

        // Trimmed cruise operating point.
        let trimmed_at_takeoff_cl = self.compute_trimmed_design_point(
            &plane,
            takeoff_cl,
            &aero,
            &fine_analysis,
            &fine_system,
        );
        let (trimmed_design_point, trimmed_clamped) =
            trimmed_at_takeoff_cl.map_or((None, false), |trimmed| {
                let (point, clamped) =
                    self.trimmed_point_at_reported_cl(trimmed, &polar, cruise_cl);
                (Some(point), clamped)
            });

        let mut geometry_summary = self.geometry_summary(&plane, design);
        // Report note: the reported cruise CL lies outside the analysed polar,
        // so the reported cruise drag and L/D are the polar's end values rather
        // than interpolations. The key is present only when that happens.
        let polar_cl_range = polar
            .cl
            .iter()
            .filter(|cl| cl.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &cl| {
                (lo.min(cl), hi.max(cl))
            });
        if !self.reference_compatibility
            && (trimmed_clamped || cruise_cl < polar_cl_range.0 || cruise_cl > polar_cl_range.1)
        {
            geometry_summary.insert("reported_cruise_cl_outside_polar".to_owned(), 1.0);
        }
        if let Some(limit_kg) = effective_structural_payload_limit_kg {
            geometry_summary.insert("effective_structural_payload_limit_kg".to_owned(), limit_kg);
        }

        Ok(AnalysisReport {
            design: *design,
            airplane: plane,
            polar,
            design_point,
            polar_fit,
            static_margin: sm,
            x_neutral_point: x_np,
            geometry_summary,
            component_masses,
            flops_mass_buildup,
            mass_coordinates: coordinates_to_map(&coords),
            physical_cg: cg,
            payload_layout: Some(payload_layout),
            trimmed_design_point,
            cg_envelope_ok,
            neutral_point_conditions: np_cond,
            fuel: crate::fuel_model::ReportFuel::default(),
        })
    }

    /// Run the final report at the takeoff mass closed by the mission-sized
    /// optimizer: the declared limit stays provenance, every mass-coupled
    /// quantity is evaluated at the closed mass (never clamped), and a fixed
    /// aircraft keeps its declared design weights while a clean-sheet design
    /// couples (`AlasConfig::at_closure_mass`).
    pub fn run_at_sized_takeoff_mass(
        &self,
        design: &DesignVector,
        include_engines: bool,
        takeoff_mass_kg: f64,
    ) -> Result<AnalysisReport, String> {
        self.run_at_sized_design_weights(design, include_engines, takeoff_mass_kg, None)
    }

    /// [`Self::run_at_sized_takeoff_mass`] with the landing gear of the MTOW
    /// band and payload-adjusted modes designed at no less than
    /// `landing_floor_kg`, the finalist's reported design landing mass
    /// (`AlasConfig::at_sized_closure_mass_with_landing_floor`). Every other
    /// mode ignores the floor.
    pub fn run_at_sized_design_weights(
        &self,
        design: &DesignVector,
        include_engines: bool,
        takeoff_mass_kg: f64,
        landing_floor_kg: Option<f64>,
    ) -> Result<AnalysisReport, String> {
        self.run_sized(
            design,
            include_engines,
            takeoff_mass_kg,
            landing_floor_kg,
            None,
        )
    }

    /// The report of the sized candidate `sized`, built on its evaluated
    /// `design` (`alas_opt::ResolvedProductState::design`): the aircraft is
    /// rebuilt with the empennage scales it was sized with
    /// (`alas_opt::mdo::rebuild_airplane`), analysed at its design weights
    /// ([`Self::run_at_sized_design_weights`]), and carries the fuel
    /// artifacts its closure flew, so every fuel quantity the report prices
    /// is priced on the closure's own model.
    ///
    /// # Errors
    ///
    /// The geometry or analysis failure, as a description.
    pub fn run_sized_candidate(
        &self,
        design: &DesignVector,
        sized: &alas_opt::SizedCandidate,
    ) -> Result<AnalysisReport, String> {
        let mut report = self.run_sized(
            design,
            true,
            sized.takeoff_mass_kg,
            Some(sized.design_landing_mass_kg),
            Some(&sized.fuel_artifacts.tail_sizing),
        )?;
        report.fuel = crate::fuel_model::ReportFuel::sized(crate::fuel_model::SizedFuel::of(sized));
        Ok(report)
    }

    fn run_sized(
        &self,
        design: &DesignVector,
        include_engines: bool,
        takeoff_mass_kg: f64,
        landing_floor_kg: Option<f64>,
        tail_sizing: Option<&alas_config::TailSizing>,
    ) -> Result<AnalysisReport, String> {
        if !takeoff_mass_kg.is_finite() || takeoff_mass_kg <= 0.0 {
            return Err(format!(
                "sized takeoff mass must be finite and positive, got {takeoff_mass_kg} kg"
            ));
        }
        let mtow_limit_kg = self.config.requirements.mtow_kg;
        // The takeoff-mass sizing plan decides which structure the closure
        // belongs to: the MTOW band and payload-adjusted modes design it at
        // the closure (`AlasConfig::at_sized_closure_mass`).
        let sized_config = self
            .config
            .at_sized_closure_mass_with_landing_floor(takeoff_mass_kg, landing_floor_kg);
        let design_landing_mass_kg = sized_config
            .mass_model
            .flops_structure
            .design_landing_mass_kg;
        let design_gross_mass_kg =
            cabin_sync::sized_design_gross_mass_kg(&sized_config, takeoff_mass_kg);
        let sized_analysis = Self {
            config: sized_config,
            reference_compatibility: self.reference_compatibility,
        };
        let mut report = match tail_sizing {
            Some(sizing) => {
                let mut config = sized_analysis.config.clone();
                let mut design = *design;
                let plane = alas_opt::mdo::rebuild_airplane(&mut config, &mut design, sizing)?;
                Self {
                    config,
                    ..sized_analysis
                }
                .run_on_airplane(&design, plane)?
            }
            None => sized_analysis.run(design, include_engines)?,
        };
        // Provenance rides in the numeric geometry map the report schema
        // already has, so no consumer can read the limit as the flown mass.
        report
            .geometry_summary
            .insert("analysis_mass_basis_kg".to_owned(), takeoff_mass_kg);
        report
            .geometry_summary
            .insert("analysis_mtow_limit_kg".to_owned(), mtow_limit_kg);
        report
            .geometry_summary
            .insert("analysis_mass_basis_is_sized".to_owned(), 1.0);
        report.geometry_summary.insert(
            "analysis_design_gross_mass_kg".to_owned(),
            design_gross_mass_kg,
        );
        if let Some(landing_kg) = design_landing_mass_kg {
            report
                .geometry_summary
                .insert("analysis_design_landing_mass_kg".to_owned(), landing_kg);
        }
        Ok(report)
    }
}

/// Resolve the structural payload bound for an unchanged registered preset:
/// the smaller of the configured cap (normally the published `MZFW - OEW`)
/// and `MZFW - modeled OEW`, because the modeled OEW can be heavier than the
/// source OEW and the layout would otherwise respect the cap while producing
/// an overweight zero-fuel mass. Notional designs inherit no published MZFW.
pub(crate) fn effective_structural_payload_limit_kg(
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
    Some(
        if configured_limit_kg.is_finite() && configured_limit_kg > 0.0 {
            configured_limit_kg.min(available_payload_kg)
        } else {
            available_payload_kg
        },
    )
}
