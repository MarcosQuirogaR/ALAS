// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Construction and the full run of a [`FullAnalysis`].

use super::mass_pass::MassPass;
use super::*;
use alas_stab::neutral_point::TUNNEL_AC_SUMMARY_KEY;

impl FullAnalysis {
    /// This analysis with its configuration resolved at the band takeoff mass
    /// `design` carries (`AlasConfig::at_band_mass`): the fixed-requirement
    /// problem at that mass. `None` when the mode is not the MTOW band, the
    /// configuration is a sized closure's, or it is resolved already.
    pub(super) fn at_band_design(&self, design: &DesignVector) -> Option<Self> {
        if self.closure_bound
            || self.config.optimizer.objective.mtow_sizing != alas_config::MtowSizing::MtowBand
        {
            return None;
        }
        let config = self.config.at_band_mass(design.mtow_kg);
        (config != self.config).then_some(Self {
            config,
            reference_compatibility: self.reference_compatibility,
            reference_masses: self.reference_masses,
            closure_bound: self.closure_bound,
            load_plan: self.load_plan,
            operator_cg_target_pct_mac: self.operator_cg_target_pct_mac,
            sized_fuel: self.sized_fuel.clone(),
        })
    }

    /// Create a new analysis orchestrator for `config`.
    pub fn new(config: AlasConfig) -> Self {
        let reference_masses = config.mass_model.uses_reference_mass_methods();
        Self {
            config,
            reference_compatibility: false,
            reference_masses,
            closure_bound: false,
            load_plan: LoadPlanPolicy::default(),
            operator_cg_target_pct_mac: None,
            sized_fuel: None,
        }
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
            reference_masses: true,
            closure_bound: false,
            load_plan: LoadPlanPolicy::Off,
            operator_cg_target_pct_mac: None,
            sized_fuel: None,
        }
    }

    /// Execute the full analysis workflow on `design`.
    pub fn run(
        &self,
        design: &DesignVector,
        include_engines: bool,
    ) -> Result<AnalysisReport, String> {
        let plane = self.build_airplane(design, include_engines)?;
        self.run_on_airplane(design, plane)
    }

    /// The aircraft [`Self::run`] analyses for `design`.
    ///
    /// # Errors
    ///
    /// The geometry build failure, as a description.
    pub fn build_airplane(
        &self,
        design: &DesignVector,
        include_engines: bool,
    ) -> Result<Airplane, String> {
        let builder = if self.reference_compatibility {
            AircraftBuilder::new_reference_compatibility(Some(self.config.geometry.clone()))
        } else {
            AircraftBuilder::new(Some(self.config.geometry.clone()))
        };
        builder
            .build(Some(design), include_engines)
            .map_err(|e| format!("geometry build error: {e}"))
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
        if self.load_plan == LoadPlanPolicy::Off
            || self.reference_masses
            || self.reference_compatibility
        {
            return self.run_steered(design, airplane, None);
        }
        load_plan::run_planned(self, design, airplane)
    }

    /// [`Self::run_on_airplane`] with the payload centroid steered to
    /// `steer_payload_cg_x_m` (`load_plan`).
    pub(super) fn run_steered(
        &self,
        design: &DesignVector,
        airplane: Airplane,
        steer_payload_cg_x_m: Option<f64>,
    ) -> Result<AnalysisReport, String> {
        if let Some(resolved) = self.at_band_design(design) {
            return resolved.run_steered(design, airplane, steer_payload_cg_x_m);
        }
        let MassPass {
            mut plane,
            declared_requirements,
            tail_eta,
            loading,
            masses,
            coords,
            cg,
            payload_layout,
            effective_structural_payload_limit_kg,
            offloaded_payload_kg,
            flops_mass_buildup,
        } = self.mass_pass(design, airplane, steer_payload_cg_x_m)?;
        let req = &declared_requirements;
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
                Some(self.config.resolved_drag_model()),
                Some(fine_analysis.clone()),
            )
        } else {
            AeroAnalysis::new(
                &plane,
                AeroAnalysis::quarter_chord_sweep_deg(&plane, design.sweep_deg),
                Some(self.config.geometry.clone()),
                Some(self.config.resolved_drag_model()),
                Some(fine_analysis.clone()),
            )
        };

        let spanwise = fine_analysis.spanwise_resolution.max(1) as usize;
        let chordwise = fine_analysis.chordwise_resolution.max(1) as usize;
        for near in alas_aero::vlm::lattice_grouping_diagnostics(&plane.wings) {
            tracing::warn!(
                surface = %near.surface,
                other = %near.other,
                gap_m = near.gap_m,
                end_chord_m = near.end_chord_m,
                "a surface end misses another planform and stays in its own lattice component"
            );
        }
        let fine_system = if self.reference_compatibility {
            VlmSystem::assemble_reference_compatibility(&plane, spanwise, chordwise)
        } else {
            VlmSystem::assemble(&plane, spanwise, chordwise)
        }
        .map_err(|e| format!("vlm assembly error: {e}"))?;

        let polar = super::run_polar(
            &aero,
            &plane,
            &fine_analysis,
            req.cruise_mach,
            req.cruise_altitude_m,
            !self.reference_compatibility && fine_analysis.compressible_trim,
            Some(&fine_system),
        )?;

        let component_masses = breakdown_to_map(&masses);
        let zero_fuel_mass_kg = crate::cruise_mass::zero_fuel_mass_kg(&component_masses);
        // The trim solution and everything that constrains the aircraft run at
        // the takeoff-mass lift coefficient; only the reported cruise CL, CD
        // and L/D move to the mid-cruise mass (`crate::cruise_mass`).
        let loaded_analysis = loading.map(|loading| {
            let mut config = self.config.clone();
            config.requirements.mtow_kg = loading.takeoff_mass_kg;
            Self {
                config,
                sized_fuel: self.sized_fuel.clone(),
                reference_compatibility: self.reference_compatibility,
                reference_masses: self.reference_masses,
                closure_bound: self.closure_bound,
                load_plan: self.load_plan,
                operator_cg_target_pct_mac: self.operator_cg_target_pct_mac,
            }
        });
        let operating = loaded_analysis.as_ref().unwrap_or(self);
        let takeoff_cl = operating.cruise_cl(&plane);
        let cruise_cl = operating.reported_cruise_cl(&plane, zero_fuel_mass_kg);
        let design_point = self.compute_design_point(&polar, cruise_cl)?;
        let polar_fit = self.fit_polar(&plane, &polar);

        let (x_np, sm, _) = if self.reference_compatibility {
            neutral_point_reference_compatibility_with_system(&fine_system, &plane, &fine_analysis)
        } else {
            neutral_point_with_system(&fine_system, &plane, &fine_analysis)
        }
        .map_err(|e| format!("neutral point error: {e}"))?;

        let inlet_polar = alas_opt::envelope::InletDragPolar {
            cd0: polar_fit.cd0,
            k: polar_fit.k,
        };
        let np_cond =
            self.np_conditions_for(&plane, &fine_analysis, req, design, Some(inlet_polar));

        let cg_envelope_ok = if self.reference_masses {
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
        if let Some(d) = np_cond.as_ref().map(|c| &c.diagnostics) {
            let shift = d.wing_lift_fraction.clamp(0.0, 1.0) * d.fuselage.tunnel_correction_shift_m;
            geometry_summary.insert(
                TUNNEL_AC_SUMMARY_KEY.to_owned(),
                100.0 * shift / plane.c_ref,
            );
        }
        if let Some(eta) = &tail_eta {
            for (key, value) in eta.summary_entries() {
                geometry_summary.insert(key.to_owned(), value);
            }
        }
        if let Some(loading) = loading {
            takeoff_loading::annotate(&mut geometry_summary, loading);
            geometry_summary.insert(
                "analysis_mtow_limit_kg".to_owned(),
                self.config.requirements.mtow_kg,
            );
        }
        // No mission is priced here: the point stands at the mean of the
        // take-off and zero-fuel masses until the report is bound to its
        // mission (`cruise_point`).
        if !self.reference_compatibility {
            let takeoff_mass_kg = operating.config.requirements.mtow_kg;
            cruise_point::CruisePointBasis::all_fuel_burned(
                takeoff_mass_kg,
                zero_fuel_mass_kg,
                loading.map_or(takeoff_mass_kg - zero_fuel_mass_kg, |loading| {
                    loading.carried_usable_fuel_kg
                }),
            )
            .write(&mut geometry_summary);
            cruise_point::mark_polar_range(
                &mut geometry_summary,
                &polar,
                design_point.cl,
                cruise_cl,
                trimmed_clamped,
            );
        }
        if let Some(limit_kg) = effective_structural_payload_limit_kg {
            geometry_summary.insert("effective_structural_payload_limit_kg".to_owned(), limit_kg);
        }
        if offloaded_payload_kg > 0.0 {
            geometry_summary.insert(ROUTE_PAYLOAD_OFFLOADED_KEY.to_owned(), offloaded_payload_kg);
        }
        if let Some(derived) = self.config.landing_gear.derived_main_gear {
            geometry_summary.insert(
                crate::gear_stations::MAIN_GEAR_TRANSLATION_KEY.to_owned(),
                derived.translation_m,
            );
        }

        let mut report = AnalysisReport {
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
            fuel: self
                .sized_fuel
                .clone()
                .map_or_else(crate::fuel_model::ReportFuel::default, crate::fuel_model::ReportFuel::sized),
        };
        if let Some((aligned, aligned_cg)) =
            crate::feasibility::ledger_aligned_coordinates(&self.config, &report)
        {
            report.mass_coordinates = coordinates_to_map(&aligned);
            report.physical_cg = aligned_cg;
        }
        Ok(report)
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
            Some(crate::fuel_model::SizedFuel::of(sized)),
        )?;
        report.fuel = crate::fuel_model::ReportFuel::sized(crate::fuel_model::SizedFuel::of(sized));
        // The reported cruise point is the candidate's own mid-cruise mass,
        // in the state its mission flies it in (`cruise_mass::mid_cruise_point`).
        if let Err(reason) = self.bind_cruise_point(
            &mut report,
            cruise_point::CruisePointBasis::of_sized(&self.config, sized),
        ) {
            tracing::warn!(%reason, "the reported cruise point is not bound to the sized candidate");
        }
        if let Some(loading) = sized.takeoff_loading {
            takeoff_loading::annotate(&mut report.geometry_summary, loading);
        }
        Ok(report)
    }

    fn run_sized(
        &self,
        design: &DesignVector,
        include_engines: bool,
        takeoff_mass_kg: f64,
        landing_floor_kg: Option<f64>,
        tail_sizing: Option<&alas_config::TailSizing>,
        sized_fuel: Option<crate::fuel_model::SizedFuel>,
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
            reference_masses: self.reference_masses,
            closure_bound: true,
            load_plan: self.load_plan,
            operator_cg_target_pct_mac: self.operator_cg_target_pct_mac,
            sized_fuel,
        };
        let mut report = match tail_sizing {
            Some(sizing) => {
                let mut config = sized_analysis.config.clone();
                let mut drawn = *design;
                let plane = alas_opt::mdo::rebuild_airplane(&mut config, &mut drawn, sizing)?;
                let mut report = Self {
                    config,
                    ..sized_analysis
                }
                .run_on_airplane(&drawn, plane)?;
                // The derived wing setting rotates the drawn wing and
                // rides in `TailSizing::wing_incidence_deg`; it is never written
                // into the published design vector. A search delivers this vector
                // and feeds it back, and the delivered configuration keeps the
                // unrotated root and break incidences, so a rotated tip here
                // would change the washout of every rebuild from the pair.
                report.design.tip_twist_deg = design.tip_twist_deg;
                report
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
