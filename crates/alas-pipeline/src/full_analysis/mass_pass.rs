// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The geometry and mass part of a full run, before any aerodynamics,
//! shared by the full run and `alas --only mass`.

use super::*;

impl FullAnalysis {
    /// The geometry and mass part of [`Self::run_steered`]: the declared
    /// cabin, the tail efficiency, both mass passes with the payload layout,
    /// the takeoff loading and the station coordinates and CG, exactly as the
    /// full run computes them before its aerodynamics.
    pub(super) fn mass_pass(
        &self,
        design: &DesignVector,
        airplane: Airplane,
        steer_payload_cg_x_m: Option<f64>,
    ) -> Result<MassPass, String> {
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
        let tail_eta = if self.reference_compatibility {
            plane.set_horizontal_tail_dynamic_pressure_ratio(1.0);
            None
        } else {
            Some(alas_aero::analysis::apply_tail_efficiency(
                &mut plane,
                &self.config,
            ))
        };

        // The compatibility replay keeps the historical unfolded-area reference.
        if self.reference_compatibility {
            if let Some(main_wing) = plane.wings.first() {
                plane.s_ref = main_wing.unfolded_area();
            }
        }

        let coordinate_model = if self.reference_masses {
            MassCoordinateModel::ReferenceCompatibility
        } else {
            MassCoordinateModel::StructuralWingbox(&self.config.structures)
        };
        let initial_mass_result = if self.reference_masses {
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
            steer_payload_cg_x_m,
        };
        let payload_pass::PayloadPass {
            layout: payload_layout,
            structural_payload_limit_kg: effective_structural_payload_limit_kg,
            offloaded_payload_kg,
            analysis: (mut masses, mut coords, _, flops_mass_buildup),
        } = self.payload_pass(design, &plane, &first_pass, coordinate_model)?;
        // An unsized analysis at the declared MTOW, in every sizing mode, and
        // a Hard-MTOW closure both carry the maximum loadable fuel; another
        // mode's closure carries its own mission fuel.
        let loading = if self.reference_masses
            || (self.closure_bound
                && !self
                    .config
                    .optimizer
                    .objective
                    .mtow_sizing
                    .sizes_at_declared_mass())
        {
            None
        } else {
            takeoff_loading::apply(&self.config, design, &plane, &mut masses, &mut coords)?
        };
        let (coords, cg) = self.station_coordinates(design, &plane, &masses, coords)?;
        Ok(MassPass {
            plane,
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
        })
    }

    /// The mass part of the report of `design` on `airplane`: exactly what the
    /// full run's report carries before any aerodynamics. The same two mass
    /// passes with the payload layout, the takeoff fuel loading capped by the
    /// tanks and the mass-basis summary entries that go with it, the
    /// ledger-aligned station coordinates and the grouped FLOPS evaluation,
    /// with the aerodynamic parts left empty (a polar that was never flown, a
    /// neutral point not a number).
    ///
    /// The desktop draws its pre-run Load & trim sheet from this report, so a
    /// sheet built from it and the sheet of a completed run assess one
    /// loading: the CG gate reads the item-level mass statement of the same
    /// report contents.
    ///
    /// # Errors
    ///
    /// The mass or station-coordinate failure, as a description.
    pub fn mass_report(
        &self,
        design: &DesignVector,
        airplane: Airplane,
    ) -> Result<AnalysisReport, String> {
        if let Some(resolved) = self.at_band_design(design) {
            return resolved.mass_report(design, airplane);
        }
        let MassPass {
            plane,
            tail_eta,
            loading,
            masses,
            coords,
            cg,
            payload_layout,
            effective_structural_payload_limit_kg,
            offloaded_payload_kg,
            flops_mass_buildup,
            ..
        } = self.mass_pass(design, airplane, None)?;
        let mut geometry_summary = self.geometry_summary(&plane, design);
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
            polar: PolarSweep {
                alpha_deg: Vec::new(),
                geometric_alpha_deg: Vec::new(),
                compressible_mach: None,
                cl: Vec::new(),
                cd: Vec::new(),
                cd_induced: Vec::new(),
                cd_wave: Vec::new(),
                cd_parasite: Vec::new(),
                cm: Vec::new(),
                l_over_d: Vec::new(),
            },
            design_point: DesignPoint {
                alpha_deg: 0.0,
                cl: 0.0,
                cd: 0.0,
                l_over_d: 0.0,
            },
            polar_fit: PolarFit {
                cd0: f64::NAN,
                c1: f64::NAN,
                k: f64::NAN,
                oswald_e: f64::NAN,
                aspect_ratio: 0.0,
                // No aerodynamic analysis is run here, so an empty polar
                // cannot be reported as a successful fit.
                status: PolarFitStatus::FallbackInsufficientPoints,
            },
            static_margin: f64::NAN,
            x_neutral_point: f64::NAN,
            geometry_summary,
            component_masses: breakdown_to_map(&masses),
            flops_mass_buildup,
            mass_coordinates: coordinates_to_map(&coords),
            physical_cg: cg,
            payload_layout: Some(payload_layout),
            trimmed_design_point: None,
            cg_envelope_ok: None,
            neutral_point_conditions: None,
            fuel: crate::fuel_model::ReportFuel::default(),
        };
        if let Some((aligned, aligned_cg)) =
            crate::feasibility::ledger_aligned_coordinates(&self.config, &report)
        {
            report.mass_coordinates = coordinates_to_map(&aligned);
            report.physical_cg = aligned_cg;
        }
        Ok(report)
    }

    /// The mass breakdown and centre of gravity of `design` on `airplane` as
    /// the full run computes them before any aerodynamics (the first,
    /// unsteered pass: a load plan, which needs the neutral point, is not
    /// applied), for `alas --only mass`.
    ///
    /// # Errors
    ///
    /// The mass or station-coordinate failure, as a description.
    pub fn mass_stage(
        &self,
        design: &DesignVector,
        airplane: Airplane,
    ) -> Result<serde_json::Value, String> {
        if let Some(resolved) = self.at_band_design(design) {
            return resolved.mass_stage(design, airplane);
        }
        let pass = self.mass_pass(design, airplane, None)?;
        let component_masses = breakdown_to_map(&pass.masses);
        Ok(serde_json::json!({
            "component_masses_kg": component_masses,
            "total_mass_kg": component_masses.values().sum::<f64>(),
            "physical_cg_m": pass.cg,
            "mass_coordinates_m": coordinates_to_map(&pass.coords),
            "takeoff_loading": format!("{:?}", pass.loading),
            "effective_structural_payload_limit_kg": pass.effective_structural_payload_limit_kg,
            "offloaded_payload_kg": pass.offloaded_payload_kg,
            "load_plan_applied": false,
        }))
    }
}

/// What [`FullAnalysis::mass_pass`] hands the rest of the full run.
pub(super) struct MassPass {
    pub plane: Airplane,
    pub declared_requirements: alas_config::DesignRequirements,
    pub tail_eta: Option<alas_aero::analysis::TailEfficiency>,
    pub loading: Option<alas_mass::loading::MtowFuelLoading>,
    pub masses: alas_mass::breakdown::MassBreakdown,
    pub coords: alas_mass::breakdown::MassCoordinates,
    pub cg: [f64; 3],
    pub payload_layout: PayloadLayout,
    pub effective_structural_payload_limit_kg: Option<f64>,
    pub offloaded_payload_kg: f64,
    pub flops_mass_buildup: Option<Box<FlopsMassBuildup>>,
}
