// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission the reported cruise point belongs to.
//!
//! The report's cruise lift coefficient, drag and lift-to-drag ratio
//! (`design_point`, `trimmed_design_point`) are level flight at one
//! mid-cruise mass in the state the mission flies that mass in
//! (`CruisePointBasis::cruise_altitude_m`, `cruise_tas_m_s`, `cruise_mach`:
//! the legal flight level its plan starts at, under both the configured level
//! and the aircraft's capability, plus the step climbs flown before
//! mid-cruise, at the speed its schedule flies there,
//! `alas_opt::mdo::cruise_mass::mid_cruise_point`). Only when no mission plan
//! is available does the point stand at the design cruise altitude and Mach,
//! and the feasibility assessment then says so
//! (`cruise_point_level_unplanned`). That mass is the
//! midpoint of one named mission, recorded with the point
//! ([`CruisePointBasis`]) and read by the level-cruise checks, so the report
//! and the checks describe the same flight state:
//!
//! - a report bound to a sized candidate takes the candidate's own mid-cruise
//!   mass (`alas_opt::SizedCandidate::mid_cruise_mass_kg`), the mass its
//!   in-loop cruise rows were evaluated at;
//! - a report analysed at a declared loading (a baseline) takes the same
//!   closed design mission of its vector under its configuration
//!   (`FullAnalysis::bind_design_cruise_point`), so a baseline and a
//!   search's reporting analysis of the same aircraft report one cruise
//!   point;
//! - a report whose design mission cannot be closed keeps the mean of the
//!   take-off and zero-fuel masses, says so, and the feasibility assessment
//!   reports it (`cruise_point_unpriced`).
//!
//! The dispatched route is another mission, flown from its own take-off
//! mass. Its cruise point is reported beside the headline
//! (`super::route_point::route_cruise_point`) and never replaces it.
//!
//! `design_point` is the lattice sweep at that lift coefficient: the airplane
//! with its stabilizer at the built incidence at every angle, an untrimmed
//! state (the GUI labels it so). Its induced drag at the cruise lift is 8 to
//! 12 counts above the trimmed airplane's (B747-400, CL 0.5, M 0.85: 139.2
//! against 126.9), and it stays the angle schedule the AVL, VSPAERO and MSES
//! comparisons read. The flown cruise point, `trimmed_design_point` and the
//! route point, takes the trimmed drag table's drag ([`trimmed_table_cd`]):
//! the stabilizer at the incidence that closes the pitching moment at each
//! lift coefficient, the build-up the mission is priced on and `cruise_polar`
//! exports. Without built fuel artifacts they fall back to the sweep.
//!
//! The polar is rebased on the drag table at `requirements.cruise_mach` and
//! the table's reference altitude (`rebase_polar_on_flown_drag`). A point
//! bound to a flown state reads the drag of `design_point`, of
//! `trimmed_design_point` and of the route point from the same table at the
//! flown Mach and level instead; `design_point`'s angle of attack stays the
//! sweep's at its lift coefficient.
//!
//! Units: masses kg, distances m, speeds m/s; coefficients on the report's
//! reference area; dynamic pressure `0.5 rho V^2 = 0.5 gamma p M^2` at the
//! level's standard pressure and the flown Mach.

use super::cruise_condition::{cruise_cl_of_mass, trimmed_table_cd, CruiseCondition};
use super::*;

const MISSION_KEY: &str = "cruise_point_mission";
const TAKEOFF_MASS_KEY: &str = "cruise_point_takeoff_mass_kg";
const LOADED_FUEL_KEY: &str = "cruise_point_loaded_fuel_kg";
const TRIP_FUEL_KEY: &str = "cruise_point_trip_fuel_kg";
const MID_MASS_KEY: &str = "cruise_point_mid_cruise_mass_kg";
const RESERVE_RANGE_KEY: &str = "cruise_point_reserve_plan_range_m";
const CRUISE_ALTITUDE_KEY: &str = "cruise_point_cruise_altitude_m";
const CRUISE_TAS_KEY: &str = "cruise_point_cruise_tas_m_s";
const CRUISE_MACH_KEY: &str = "cruise_point_cruise_mach";

/// Largest difference between two take-off masses that still name the same
/// loading, kg.
const SAME_LOADING_TOLERANCE_KG: f64 = 1.0;

/// The mission a reported cruise point is the midpoint of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CruisePointMission {
    /// The design mission of the analysed loading: all of its mission fuel
    /// flown, down to the reserves.
    FullMissionFuel,
    /// No mission was priced: every kilogram of loaded fuel is taken as
    /// burned in cruise.
    AllFuelBurned,
}

impl CruisePointMission {
    const fn code(self) -> f64 {
        match self {
            Self::AllFuelBurned => 0.0,
            Self::FullMissionFuel => 1.0,
        }
    }

    fn from_code(code: f64) -> Self {
        if code == Self::FullMissionFuel.code() {
            Self::FullMissionFuel
        } else {
            Self::AllFuelBurned
        }
    }

    /// Stable identifier for exports.
    pub const fn id(self) -> &'static str {
        match self {
            Self::FullMissionFuel => "design_mission_analysed_loading_full_mission_fuel",
            Self::AllFuelBurned => "all_loaded_fuel_burned",
        }
    }

    /// English label, the key of its catalog entry.
    pub const fn label(self) -> &'static str {
        match self {
            Self::FullMissionFuel => "Design mission: analysed loading, full mission fuel",
            Self::AllFuelBurned => "No mission priced: all loaded fuel burned",
        }
    }
}

/// The mission and masses behind a reported cruise point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruisePointBasis {
    /// The mission the point is the midpoint of.
    pub mission: CruisePointMission,
    /// Mass at brake release of that mission, kg.
    pub takeoff_mass_kg: f64,
    /// Usable fuel on board at brake release, kg.
    pub loaded_fuel_kg: f64,
    /// Trip fuel the mission burns, kg.
    pub trip_fuel_kg: f64,
    /// `takeoff_mass_kg - trip_fuel_kg / 2`, kg: the mass of the point.
    pub mid_cruise_mass_kg: f64,
    /// Still-air range, m, of the design mission whose plan priced the
    /// reserves, when one did. Context only: it is not the range flown.
    pub reserve_plan_range_m: Option<f64>,
    /// Pressure altitude the mission flies the point at, m: the level of its
    /// plan's cruise segment begun at or above `mid_cruise_mass_kg`
    /// (`alas_opt::mdo::cruise_mass::mid_cruise_point`). `None`: no mission
    /// plan was available and the point stands at the design cruise altitude.
    pub cruise_altitude_m: Option<f64>,
    /// True airspeed of that segment, m/s; `None` with `cruise_altitude_m`.
    pub cruise_tas_m_s: Option<f64>,
    /// Mach of that segment on the mission's day; `None` with
    /// `cruise_altitude_m` (the point then flies `requirements.cruise_mach`).
    pub cruise_mach: Option<f64>,
}

impl CruisePointBasis {
    /// The basis of a report no mission was priced for: the mean of the
    /// take-off mass and the zero-fuel mass (`crate::cruise_mass`), with
    /// `loaded_fuel_kg` of usable fuel on board.
    pub(super) fn all_fuel_burned(
        takeoff_mass_kg: f64,
        zero_fuel_mass_kg: f64,
        loaded_fuel_kg: f64,
    ) -> Self {
        let mid_cruise_mass_kg = crate::cruise_mass::mid_cruise_mass_kg(
            takeoff_mass_kg,
            takeoff_mass_kg - zero_fuel_mass_kg,
        );
        Self {
            mission: CruisePointMission::AllFuelBurned,
            takeoff_mass_kg,
            loaded_fuel_kg,
            trip_fuel_kg: 2.0 * (takeoff_mass_kg - mid_cruise_mass_kg),
            mid_cruise_mass_kg,
            reserve_plan_range_m: None,
            cruise_altitude_m: None,
            cruise_tas_m_s: None,
            cruise_mach: None,
        }
    }

    /// The basis of the sized candidate `sized` under `config`: its own
    /// mid-cruise mass, the same state of its take-off loading with the
    /// reserves of the dispatch it was closed on (under every MTOW mode), in
    /// the state its mission flies that mass in
    /// (`alas_opt::mdo::cruise_mass::mid_cruise_point`). A state that cannot
    /// be found leaves the point at the design altitude, which the
    /// feasibility assessment reports (`cruise_point_level_unplanned`).
    pub(super) fn of_sized(config: &AlasConfig, sized: &alas_opt::SizedCandidate) -> Self {
        let mid_cruise_mass_kg = sized.mid_cruise_mass_kg();
        let flown = match alas_opt::mdo::cruise_mass::mid_cruise_point(config, sized) {
            Ok(point) if point.altitude_m.is_finite() && point.altitude_m > 0.0 => Some(point),
            Ok(point) => {
                let level_m = point.altitude_m;
                tracing::warn!(level_m, "the mid-cruise level is not a usable altitude");
                None
            }
            Err(reason) => {
                tracing::warn!(%reason, "the cruise point stays at the design altitude");
                None
            }
        };
        Self {
            mission: CruisePointMission::FullMissionFuel,
            takeoff_mass_kg: sized.takeoff_mass_kg,
            loaded_fuel_kg: sized.takeoff_fuel_kg,
            trip_fuel_kg: 2.0 * (sized.takeoff_mass_kg - mid_cruise_mass_kg),
            mid_cruise_mass_kg,
            reserve_plan_range_m: (sized.design_range_m.is_finite() && sized.design_range_m > 0.0)
                .then_some(sized.design_range_m),
            cruise_altitude_m: flown.map(|point| point.altitude_m),
            cruise_tas_m_s: flown.map(|point| point.tas_m_s),
            cruise_mach: flown.map(|point| point.mach),
        }
    }

    /// The flown condition of the point, when its mission was planned.
    fn flown_condition(&self) -> Option<CruiseCondition> {
        Some(CruiseCondition {
            altitude_m: self.cruise_altitude_m?,
            mach: self.cruise_mach?,
        })
    }

    /// Whether a priced mission's point stands at the design altitude
    /// because no flown state of its mid-cruise mass was found.
    pub(crate) fn level_unplanned(&self) -> bool {
        self.mission == CruisePointMission::FullMissionFuel && self.flown_condition().is_none()
    }

    pub(super) fn write(&self, summary: &mut BTreeMap<String, f64>) {
        summary.insert(MISSION_KEY.to_owned(), self.mission.code());
        summary.insert(TAKEOFF_MASS_KEY.to_owned(), self.takeoff_mass_kg);
        summary.insert(LOADED_FUEL_KEY.to_owned(), self.loaded_fuel_kg);
        summary.insert(TRIP_FUEL_KEY.to_owned(), self.trip_fuel_kg);
        summary.insert(MID_MASS_KEY.to_owned(), self.mid_cruise_mass_kg);
        match self.reserve_plan_range_m {
            Some(range_m) => summary.insert(RESERVE_RANGE_KEY.to_owned(), range_m),
            None => summary.remove(RESERVE_RANGE_KEY),
        };
        for (key, value) in [
            (CRUISE_ALTITUDE_KEY, self.cruise_altitude_m),
            (CRUISE_TAS_KEY, self.cruise_tas_m_s),
            (CRUISE_MACH_KEY, self.cruise_mach),
        ] {
            match value {
                Some(value) => summary.insert(key.to_owned(), value),
                None => summary.remove(key),
            };
        }
    }

    /// The summary line naming the mission and its masses.
    fn summary_line(&self) -> String {
        let mission = match self.mission {
            CruisePointMission::FullMissionFuel => {
                "design mission: analysed loading, full mission fuel"
            }
            CruisePointMission::AllFuelBurned => "no mission priced, all loaded fuel burned",
        };
        let level = match (self.flown_condition(), self.cruise_tas_m_s) {
            (Some(flown), Some(tas_m_s)) => format!(
                " at {:.0} m, Mach {:.3} ({tas_m_s:.1} m/s TAS)",
                flown.altitude_m, flown.mach
            ),
            _ if self.level_unplanned() => " at the design cruise altitude, no flown level".into(),
            _ => String::new(),
        };
        format!(
            "  Cruise point     : {mission}; from {:.0} kg, mid-cruise {:.0} kg{level} \
             (trip fuel {:.0} kg)",
            self.takeoff_mass_kg, self.mid_cruise_mass_kg, self.trip_fuel_kg
        )
    }

    /// The exported record.
    pub(crate) fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "mission": self.mission.id(),
            "takeoff_mass_kg": self.takeoff_mass_kg,
            "loaded_fuel_kg": self.loaded_fuel_kg,
            "trip_fuel_kg": self.trip_fuel_kg,
            "mid_cruise_mass_kg": self.mid_cruise_mass_kg,
            "reserve_plan_range_m": self.reserve_plan_range_m,
            "cruise_altitude_m": self.cruise_altitude_m,
            "cruise_tas_m_s": self.cruise_tas_m_s,
            "cruise_mach": self.cruise_mach,
        })
    }
}

impl AnalysisReport {
    /// The mission and masses of the reported cruise point; `None` for a
    /// report that records none (the frozen reference path, whose cruise
    /// point stays at the take-off mass).
    pub fn cruise_point_basis(&self) -> Option<CruisePointBasis> {
        let value = |key: &str| self.geometry_summary.get(key).copied();
        Some(CruisePointBasis {
            mission: CruisePointMission::from_code(value(MISSION_KEY)?),
            takeoff_mass_kg: value(TAKEOFF_MASS_KEY)?,
            loaded_fuel_kg: value(LOADED_FUEL_KEY)?,
            trip_fuel_kg: value(TRIP_FUEL_KEY)?,
            mid_cruise_mass_kg: value(MID_MASS_KEY)?,
            reserve_plan_range_m: value(RESERVE_RANGE_KEY),
            cruise_altitude_m: value(CRUISE_ALTITUDE_KEY),
            cruise_tas_m_s: value(CRUISE_TAS_KEY),
            cruise_mach: value(CRUISE_MACH_KEY),
        })
    }

    /// Mid-cruise mass, kg, of the mission the reported cruise point belongs
    /// to, when that mission was priced and is flown from `takeoff_mass_kg`
    /// (within 1 kg): the mass a level-cruise check of the same loading is
    /// evaluated at.
    pub fn cruise_point_mass_of_loading(&self, takeoff_mass_kg: f64) -> Option<f64> {
        self.cruise_point_basis()
            .filter(|basis| {
                basis.mission != CruisePointMission::AllFuelBurned
                    && (basis.takeoff_mass_kg - takeoff_mass_kg).abs() <= SAME_LOADING_TOLERANCE_KG
                    && basis.mid_cruise_mass_kg.is_finite()
                    && basis.mid_cruise_mass_kg > 0.0
            })
            .map(|basis| basis.mid_cruise_mass_kg.min(takeoff_mass_kg))
    }
}

/// The lines the design summary carries on the reported cruise point: the
/// off-polar warning, when it applies, and the mission the point belongs to.
pub(crate) fn summary_lines(report: &AnalysisReport) -> Vec<String> {
    cruise_point_off_polar_note(report)
        .map(|note| format!("  WARNING          : {note}"))
        .into_iter()
        .chain(
            report
                .cruise_point_basis()
                .map(|basis| basis.summary_line()),
        )
        .collect()
}

/// Record whether the reported cruise point is the polar at the cruise lift
/// coefficient `cruise_cl`.
///
/// The keys are present only when it is not: the lift lies outside the
/// analysed sweep, the trimmed point was clamped (`trimmed_clamped`), or the
/// sweep could not be interpolated and its nearest sample `reported_cl`
/// stands in. The feasibility report and the summary read them.
pub(super) fn mark_polar_range(
    summary: &mut BTreeMap<String, f64>,
    polar: &PolarSweep,
    reported_cl: f64,
    cruise_cl: f64,
    trimmed_clamped: bool,
) {
    for key in [
        CRUISE_CL_OUTSIDE_POLAR_KEY,
        CRUISE_CL_REQUESTED_KEY,
        POLAR_CL_MIN_KEY,
        POLAR_CL_MAX_KEY,
    ] {
        summary.remove(key);
    }
    let (cl_min, cl_max) = polar_cl_range(polar);
    let sample_offset = (reported_cl - cruise_cl).abs();
    let nearest_sample_reported = sample_offset.is_nan() || sample_offset > f64::EPSILON;
    if trimmed_clamped || nearest_sample_reported || cruise_cl < cl_min || cruise_cl > cl_max {
        summary.insert(CRUISE_CL_OUTSIDE_POLAR_KEY.to_owned(), 1.0);
        summary.insert(CRUISE_CL_REQUESTED_KEY.to_owned(), cruise_cl);
        summary.insert(POLAR_CL_MIN_KEY.to_owned(), cl_min);
        summary.insert(POLAR_CL_MAX_KEY.to_owned(), cl_max);
    }
}

/// Lowest and highest finite lift coefficient of the analysed sweep.
fn polar_cl_range(polar: &PolarSweep) -> (f64, f64) {
    polar
        .cl
        .iter()
        .filter(|cl| cl.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &cl| {
            (lo.min(cl), hi.max(cl))
        })
}

impl FullAnalysis {
    /// Move the reported cruise point of `report` to the mid-cruise mass of
    /// `basis` and record the basis.
    ///
    /// `design_point` is read off the polar at the new lift coefficient; the
    /// trimmed point keeps its trim solution and trim-drag increment and
    /// moves along the polar (`Self::trimmed_point_at_reported_cl`: a point
    /// moved twice is the point moved once). The frozen reference path is
    /// left at its take-off mass.
    ///
    /// # Errors
    ///
    /// A mass without a positive finite lift coefficient, or a polar that
    /// cannot supply the point; the report is then unchanged.
    pub(crate) fn bind_cruise_point(
        &self,
        report: &mut AnalysisReport,
        basis: CruisePointBasis,
    ) -> Result<(), String> {
        if self.reference_compatibility {
            return Ok(());
        }
        let design = CruiseCondition::design(&self.config);
        let flown = basis.flown_condition();
        let cruise_cl = cruise_cl_of_mass(
            &self.config,
            report,
            basis.mid_cruise_mass_kg,
            flown.unwrap_or(design),
        );
        if !(cruise_cl.is_finite() && cruise_cl > 0.0) {
            return Err(format!(
                "the mid-cruise mass {} kg has no positive finite cruise lift coefficient",
                basis.mid_cruise_mass_kg
            ));
        }
        self.rebase_polar_on_flown_drag(report);
        let mut design_point = self.compute_design_point(&report.polar, cruise_cl)?;
        // The rebased polar is the table at the requirement's Mach and the
        // table's reference altitude; the flown state reads it at its own.
        if let Some(cd) = flown.and_then(|at| {
            trimmed_table_cd(&self.config, report, design_point.cl, Some(at))
        }) {
            design_point.cd = cd;
            design_point.l_over_d = design_point.cl / cd;
        }
        let mut trimmed_clamped = false;
        if let Some(trimmed) = report.trimmed_design_point {
            let (mut point, clamped) =
                self.trimmed_point_at_reported_cl(trimmed, &report.polar, cruise_cl);
            if let Some(cd) = trimmed_table_cd(&self.config, report, point.cl, flown) {
                point.cd = cd;
                point.l_over_d = point.cl / cd;
            }
            // The trim solution itself sits at the take-off lift coefficient
            // of the design cruise condition it was solved at.
            let takeoff_cl = cruise_cl_of_mass(&self.config, report, basis.takeoff_mass_kg, design);
            let (cl_min, cl_max) = polar_cl_range(&report.polar);
            trimmed_clamped = clamped || takeoff_cl < cl_min || takeoff_cl > cl_max;
            report.trimmed_design_point = Some(point);
        }
        report.design_point = design_point;
        mark_polar_range(
            &mut report.geometry_summary,
            &report.polar,
            design_point.cl,
            cruise_cl,
            trimmed_clamped,
        );
        basis.write(&mut report.geometry_summary);
        Ok(())
    }

    /// Bind the reported cruise point of a report analysed at a declared
    /// loading to the design mission the aircraft closes on: the closed
    /// design mission of the same vector under the same configuration
    /// (`alas_opt::assess_product_candidate`, [`CruisePointBasis::of_sized`]),
    /// the one source a search's reporting analysis of the same aircraft
    /// reads, so a baseline and a sandbox replay of one aircraft report the
    /// same cruise point. The rest of the report keeps its declared mass
    /// basis.
    ///
    /// A report bound to a sized candidate is left as it is. When the design
    /// mission cannot be closed the report keeps saying that no mission was
    /// priced, and the feasibility assessment reports it
    /// (`FindingCode::CruisePointUnpriced`).
    pub(crate) fn bind_design_cruise_point(&self, report: &mut AnalysisReport) {
        if self.reference_compatibility || report.fuel.sized_fuel().is_some() {
            return;
        }
        if report.cruise_point_basis().is_none() {
            return;
        }
        let config = self.config.at_band_mass(report.design.mtow_kg);
        let bound = alas_opt::assess_product_candidate(&config, &report.design).and_then(|closed| {
            self.bind_cruise_point(report, CruisePointBasis::of_sized(&config, &closed.sized))
        });
        if let Err(reason) = bound {
            tracing::warn!(%reason, "the reported cruise point is not bound to its design mission");
        }
    }
}

// A failed expectation or unwrap here is a failed test assertion.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A report that is not bound to a mission says so, and binding it moves
    /// the reported point to the mission's mid-cruise mass: the lift
    /// coefficient is level flight at that mass, the drag is the polar there,
    /// the trim solution is untouched, and a level-cruise check of the same
    /// loading reads the same mass.
    #[test]
    fn binding_moves_the_reported_point_to_the_mission_mid_cruise_mass() {
        let analysis = FullAnalysis::new(AlasConfig::default());
        let mut report = analysis.run(&DesignVector::default(), true).unwrap();
        let unpriced = report.cruise_point_basis().unwrap();
        assert_eq!(unpriced.mission, CruisePointMission::AllFuelBurned);
        assert_eq!(
            report.cruise_point_mass_of_loading(unpriced.takeoff_mass_kg),
            None
        );
        let cl_unpriced = report.design_point.cl;
        let design = CruiseCondition::design(&analysis.config);
        assert!(
            (cl_unpriced
                - cruise_cl_of_mass(
                    &analysis.config,
                    &report,
                    unpriced.mid_cruise_mass_kg,
                    design
                ))
            .abs()
                < 1.0e-12
        );
        let trim_before = report.trimmed_design_point.unwrap();

        // A mission that lands with 10 % of the loaded fuel as reserves.
        let trip_fuel_kg = 0.9 * unpriced.loaded_fuel_kg;
        let basis = CruisePointBasis {
            mission: CruisePointMission::FullMissionFuel,
            trip_fuel_kg,
            mid_cruise_mass_kg: unpriced.takeoff_mass_kg - 0.5 * trip_fuel_kg,
            reserve_plan_range_m: Some(5.0e6),
            ..unpriced
        };
        analysis.bind_cruise_point(&mut report, basis).unwrap();

        assert_eq!(report.cruise_point_basis(), Some(basis));
        let cl = cruise_cl_of_mass(&analysis.config, &report, basis.mid_cruise_mass_kg, design);
        assert!(cl > cl_unpriced, "reserves not burned: heavier mid-cruise state");
        assert!((report.design_point.cl - cl).abs() < 1.0e-12);
        let polar_cd = crate::cruise_mass::polar_cd_at_cl(&report.polar, cl, cl).unwrap();
        assert!((report.design_point.cd - polar_cd.value).abs() < 1.0e-12);
        let trim = report.trimmed_design_point.unwrap();
        assert!((trim.cl - cl).abs() < 1.0e-12);
        assert_eq!(trim.trim_ih_deg, trim_before.trim_ih_deg);
        assert_eq!(
            trim.geometric_body_alpha_deg,
            trim_before.geometric_body_alpha_deg
        );
        // The flown point's drag is the trimmed drag table's where the report
        // carries one; without a table the trim-drag increment over the
        // polar is carried unchanged.
        match trimmed_table_cd(&analysis.config, &report, trim.cl, None) {
            Some(cd) => {
                assert!((trim.cd - cd).abs() < 1.0e-12);
                assert!((trim.l_over_d - trim.cl / cd).abs() < 1.0e-9);
            }
            None => {
                let increment = |point: &TrimmedDesignPoint| {
                    let on_polar =
                        crate::cruise_mass::polar_cd_at_cl(&report.polar, point.cl, point.cl)
                            .unwrap();
                    point.cd - on_polar.value
                };
                assert!((increment(&trim) - increment(&trim_before)).abs() < 1.0e-9);
            }
        }

        assert_eq!(
            report.cruise_point_mass_of_loading(basis.takeoff_mass_kg),
            Some(basis.mid_cruise_mass_kg)
        );
        // Another loading is another mission: the report's mass is not its.
        assert_eq!(
            report.cruise_point_mass_of_loading(basis.takeoff_mass_kg - 5_000.0),
            None
        );
        let lines = summary_lines(&report);
        let label = "design mission: analysed loading, full mission fuel";
        assert!(lines.iter().any(|line| line.contains(label)), "{lines:?}");
        // A priced mission with no flown state stands at the design
        // condition, and says so (`cruise_point_level_unplanned`).
        assert!(basis.level_unplanned());
        assert!(lines.iter().any(|line| line.contains("no flown level")), "{lines:?}");
    }

    /// A point bound to a flown state is level flight at that state's level
    /// and Mach (the dynamic pressure `0.5 gamma p M^2` there), not at the
    /// requirement's Mach read at the level: a turboprop holding its
    /// equivalent airspeed below its reference level flies a lower Mach and
    /// a higher lift coefficient (ATR72-600 at FL150: about 9 %).
    #[test]
    fn a_flown_state_sets_the_dynamic_pressure_of_the_reported_point() {
        let analysis = FullAnalysis::new(AlasConfig::default());
        let mut report = analysis.run(&DesignVector::default(), true).unwrap();
        let unpriced = report.cruise_point_basis().unwrap();
        let req = &analysis.config.requirements;
        let altitude_m = req.cruise_altitude_m - 1_000.0;
        let mach = 0.9 * req.cruise_mach;
        let sound = Atmosphere::new(altitude_m).speed_of_sound();
        let basis = CruisePointBasis {
            mission: CruisePointMission::FullMissionFuel,
            cruise_altitude_m: Some(altitude_m),
            cruise_tas_m_s: Some(mach * sound),
            cruise_mach: Some(mach),
            ..unpriced
        };
        assert!(!basis.level_unplanned());
        analysis.bind_cruise_point(&mut report, basis).unwrap();
        assert_eq!(report.cruise_point_basis(), Some(basis));
        let flown = CruiseCondition { altitude_m, mach };
        let cl = cruise_cl_of_mass(&analysis.config, &report, basis.mid_cruise_mass_kg, flown);
        assert!((report.design_point.cl - cl).abs() < 1.0e-12);
        // The requirement's Mach at the same level understates it by 1/0.9^2.
        let at_requirement = CruiseCondition {
            altitude_m,
            mach: req.cruise_mach,
        };
        let cl_requirement =
            cruise_cl_of_mass(&analysis.config, &report, basis.mid_cruise_mass_kg, at_requirement);
        assert!((cl / cl_requirement - 1.0 / 0.81).abs() < 1.0e-9);
        // The headline drag is the trimmed table's at the flown Mach and
        // level, as the trimmed point's is, where the report carries a table.
        if let Some(cd) = trimmed_table_cd(&analysis.config, &report, cl, Some(flown)) {
            assert!((report.design_point.cd - cd).abs() < 1.0e-12);
            assert!((report.design_point.l_over_d - cl / cd).abs() < 1.0e-9);
        }
        let lines = summary_lines(&report);
        assert!(lines.iter().any(|line| line.contains("Mach")), "{lines:?}");
    }
}
