// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The full multi-condition neutral-point set, carried alongside a report's
//! existing fixed-condition `x_neutral_point` rather than replacing it.

use alas_config::analysis::AnalysisConfig;
use alas_config::requirements::DesignRequirements;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::neutral_point::{
    neutral_point_conditions, HighLiftDevices, NeutralPointConditions, NpConditionsInput,
    PropellerSpec,
};

use super::FullAnalysis;

impl FullAnalysis {
    /// Clean low-speed/cruise, estimated high-lift and elastic-band
    /// neutral-point conditions, at the requirements' own cruise
    /// Mach/altitude. `None` on the frozen `reference_compatibility` path
    /// or a failed VLM probe. `polar` (the clean drag polar fit) sets the
    /// engine thrust of each condition for the jet-inlet term; `None` omits
    /// the term.
    pub(crate) fn np_conditions_for(
        &self,
        plane: &Airplane,
        fine_analysis: &AnalysisConfig,
        req: &DesignRequirements,
        design: &alas_config::design_variables::DesignVector,
        polar: Option<alas_opt::envelope::InletDragPolar>,
    ) -> Option<NeutralPointConditions> {
        if self.reference_compatibility {
            return None;
        }
        let np_input = NpConditionsInput {
            low_speed_altitude_m: 0.0,
            cruise_mach: req.cruise_mach,
            cruise_altitude_m: req.cruise_altitude_m,
            high_lift: HighLiftDevices::declared(
                self.config.performance.high_lift_chord_extension,
                &self.config.control_surfaces,
            ),
            propeller: propeller_spec(&self.config),
            jet_inlets: polar
                .and_then(|p| alas_opt::envelope::jet_inlet_spec(&self.config, p, plane.s_ref)),
            wing_stiffness: alas_mass::wing_reconciliation::wing_box_stiffness(
                &self.config,
                design,
                plane,
            )
            .ok()
            .map(|table| alas_stab::neutral_point::WingStiffness {
                y_m: table.y_m,
                ei_nm2: table.ei_nm2,
                gj_nm2: table.gj_nm2,
                box_axis_fraction: table.box_axis_fraction,
                inertia_relief: Some(alas_stab::neutral_point::InertiaRelief {
                    carried_kg_m: table.carried_kg_m,
                    point_masses_kg: table.point_masses_kg,
                    aircraft_mass_kg: table.aircraft_mass_kg,
                }),
            }),
            tail_flexibility: alas_mass::wing_reconciliation::tail_flexibility(
                &self.config,
                design,
                plane,
            )
            .ok()
            .map(|t| alas_stab::neutral_point::TailFlexibility {
                fuselage_x_m: t.fuselage_x_m,
                fuselage_ei_nm2: t.fuselage_ei_nm2,
                stabilizer: alas_stab::neutral_point::WingStiffness {
                    y_m: t.stabilizer_y_m,
                    ei_nm2: t.stabilizer_ei_nm2,
                    gj_nm2: t.stabilizer_gj_nm2,
                    box_axis_fraction: t.stabilizer_axis_fraction,
                    inertia_relief: None,
                },
            }),
            max_operating: self.config.max_operating_condition(),
            ..NpConditionsInput::default()
        };
        match neutral_point_conditions(plane, fine_analysis, &np_input) {
            Ok(conditions) => Some(conditions),
            Err(error) => {
                tracing::warn!(%error, "neutral-point conditions unavailable: VLM probe failed");
                None
            }
        }
    }
}

/// The tractor propeller of a turboprop configuration, when its blade count
/// is declared.
fn propeller_spec(config: &alas_config::AlasConfig) -> Option<PropellerSpec> {
    let spec = config.geometry.engine.turboprop.as_ref()?;
    Some(PropellerSpec {
        blades: spec.propeller_blade_count?,
        diameter_m: spec.propeller_diameter_m,
    })
}
