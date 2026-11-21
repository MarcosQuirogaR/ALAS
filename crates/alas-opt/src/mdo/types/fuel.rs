// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a sized candidate carries so its missions can be flown again on the
//! same aircraft, and the controls and work counters of its closure.

use std::sync::Arc;

use alas_config::TailSizing;

use crate::mdo::drag_table::TrimmedDragTable;
use crate::mdo::mission_model::{CruiseDrag, FrozenMissionPlan, ParabolicPolar, SizingBudget};

/// The clean cruise drag one candidate's missions are flown on.
///
/// Coefficients are dimensionless on the candidate's `Airplane::s_ref`;
/// altitudes m (ISA geometric).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum CandidateDrag {
    /// The trimmed `CD(CL, M, h)` table of a natively trimmed candidate.
    Table(Arc<TrimmedDragTable>),
    /// The parabolic polar an external solver measured at the cruise point
    /// (see [`ExternalPolar`](super::ExternalPolar)); it carries no build-up
    /// to tabulate.
    External(Arc<ParabolicPolar>),
}

impl CandidateDrag {
    /// The model the mission integrator reads.
    pub fn cruise_drag(&self) -> Arc<dyn CruiseDrag> {
        match self {
            Self::Table(table) => table.clone(),
            Self::External(polar) => polar.clone(),
        }
    }

    /// Clean trimmed drag coefficient at `cl`, `mach` and `altitude_m`.
    pub fn cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        match self {
            Self::Table(table) => table.cd(cl, mach, altitude_m),
            Self::External(polar) => polar.cd(cl, mach, altitude_m),
        }
    }

    /// Drag at the actual US1976 atmosphere of an airport (take-off or
    /// landing) condition before the configuration increment: as
    /// [`Self::cd_at_atmosphere`] without the lift-dependent viscous term,
    /// which belongs to the clean polar only (Torenbeek 2013, p. 96), and with
    /// the upsweep at the attitude of a configuration whose devices add
    /// `flap_lift_cl` at constant angle of attack (zero for a clean one).
    pub fn cd_airport_base_at_atmosphere(
        &self,
        cl: f64,
        mach: f64,
        altitude_m: f64,
        isa_deviation_c: f64,
        flap_lift_cl: f64,
    ) -> f64 {
        match self {
            Self::Table(table) => {
                let atmosphere = alas_atmo::us1976_compute_values(altitude_m, isa_deviation_c);
                let reynolds_per_m =
                    atmosphere.density_kg_m3 * mach * atmosphere.speed_of_sound_m_s
                        / atmosphere.dynamic_viscosity_pa_s;
                table.cd_without_lift_viscous_at_reynolds_per_m(
                    cl,
                    mach,
                    reynolds_per_m,
                    flap_lift_cl,
                )
            }
            Self::External(polar) => polar.cd(cl, mach, altitude_m),
        }
    }

    /// Clean trimmed drag at the actual US1976 atmosphere, including an ISA
    /// temperature deviation (K or deg C). Native parasite drag uses
    /// `rho V / mu` per metre with each component's own Reynolds length.
    /// An external measured polar has no Reynolds-dependent buildup.
    pub fn cd_at_atmosphere(
        &self,
        cl: f64,
        mach: f64,
        altitude_m: f64,
        isa_deviation_c: f64,
    ) -> f64 {
        match self {
            Self::Table(table) => {
                let atmosphere = alas_atmo::us1976_compute_values(altitude_m, isa_deviation_c);
                let reynolds_per_m =
                    atmosphere.density_kg_m3 * mach * atmosphere.speed_of_sound_m_s
                        / atmosphere.dynamic_viscosity_pa_s;
                table.cd_at_reynolds_per_m(cl, mach, reynolds_per_m)
            }
            Self::External(polar) => polar.cd(cl, mach, altitude_m),
        }
    }

    /// Wave drag coefficient at `cl` and `mach`.
    pub fn wave_cd(&self, cl: f64, mach: f64) -> f64 {
        match self {
            Self::Table(table) => table.wave_cd(cl, mach),
            Self::External(polar) => polar.wave_cd_at(mach),
        }
    }

    /// The `(cd0, k)` of the parabola `cd0 + k CL^2` tangent to the drag at
    /// the design lift coefficient and Mach `mach`, wave drag included
    /// ([`TrimmedDragTable::parabolic_equivalent`]). An external polar's
    /// wave term does not depend on CL, so its tangent is exact.
    pub fn parabolic_equivalent(&self, mach: f64) -> (f64, f64) {
        match self {
            Self::Table(table) => table.parabolic_equivalent(mach),
            Self::External(polar) => (polar.cd0 + polar.wave_cd_at(mach), polar.induced_factor_k),
        }
    }

    /// The trimmed drag table, when the candidate was trimmed natively.
    pub fn table(&self) -> Option<&Arc<TrimmedDragTable>> {
        match self {
            Self::Table(table) => Some(table),
            Self::External(_) => None,
        }
    }
}

/// What rebuilds a candidate's propulsion deck:
/// `mdo::propulsion::PropulsionDeck::from_engine` on the candidate's
/// `geometry.engine` at this reference point.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct DeckKey {
    /// Engine name and model identity (`PropulsionDeck::identity`).
    pub identity: String,
    /// Reference (sizing cruise) Mach of the deck.
    pub reference_mach: f64,
    /// Reference (sizing cruise) ISA altitude of the deck, m.
    pub reference_altitude_m: f64,
    /// Maximum-climb rate the climb rating is anchored to, ft/min.
    pub max_climb_rate_ft_min: Option<f64>,
}

/// Everything a downstream consumer needs to fly the candidate's missions
/// on the same aircraft the sizing closure flew, without re-trimming or
/// re-sizing it.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct CandidateFuelArtifacts {
    /// The cruise drag of the converged trim.
    pub drag: CandidateDrag,
    /// Reference wing area the drag coefficients are on (`Airplane::s_ref`),
    /// m^2.
    pub reference_area_m2: f64,
    /// The propulsion deck the closure flew.
    pub deck: DeckKey,
    /// The trip plan in force on the closure's last pass (after any
    /// re-freeze), or `None` when no plan could be made at that mass.
    pub frozen_plan: Option<FrozenMissionPlan>,
    /// The empennage scales the geometry was built with.
    pub tail_sizing: TailSizing,
    /// The station of the centre of gravity, m (geometry axes), `drag` was
    /// trimmed at: where the tail load, and with it the trimmed induced drag,
    /// is the table's own. A leg flown with the centre of gravity elsewhere
    /// pays the difference ([`crate::mdo::CgTrimDrag`]).
    pub trim_cg_x_m: f64,
    /// The geometry-scaled maximum lift coefficients the closure flew with,
    /// None when the candidate keeps the configuration's declared values
    /// (the registered geometry, or the model switched off). Every consumer
    /// that re-flies or reports the candidate reads its coefficients through
    /// [Self::config_with_clmax] so the report and the sizing agree (AERL-01).
    pub clmax: Option<alas_perf::performance::clmax::ClmaxSet>,
    /// The jump of the takeoff-mass map the closure met, and whether the
    /// held mission plan settled it, for the report to state.
    pub sizing_map_jump: Option<crate::mdo::SizingMapJump>,
    /// The wing's buffet-onset lift coefficient at the cruise Mach the
    /// closure's level selection was bounded by
    /// ([`crate::mdo::SegmentMissionModel::with_buffet_onset_cl`]); `None`
    /// for no buffet bound.
    pub buffet_onset_cl: Option<f64>,
}

impl CandidateFuelArtifacts {
    /// config with the coefficients this candidate was sized with, or the
    /// borrowed original when it kept the declared ones.
    pub fn config_with_clmax<'a>(
        &self,
        config: &'a alas_config::AlasConfig,
    ) -> std::borrow::Cow<'a, alas_config::AlasConfig> {
        match &self.clmax {
            Some(set) => {
                std::borrow::Cow::Owned(crate::mdo::sizing::clmax_geometry::with_clmax(config, set))
            }
            None => std::borrow::Cow::Borrowed(config),
        }
    }
}

/// Work one candidate's sizing closure spent, for cost reports and budgets.
/// Counters only; nothing here enters a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SizingWork {
    /// Trip plans frozen (one per outer pass that could make one).
    pub plan_freezes: u32,
    /// Complete profile integrations of the closure: trips, diversions and
    /// level-search flights.
    pub trip_flights: u64,
    /// Propulsion-deck evaluations of the whole candidate, off-design route
    /// included.
    pub deck_evals: u64,
}

/// Caller controls of one candidate's sizing closure. The default sizes
/// exactly as the configuration says.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SizingControls {
    /// Work limits; an exhausted budget rejects the candidate as
    /// `mission_model::SIZING_BUDGET_EXHAUSTED`.
    pub budget: Option<SizingBudget>,
    /// Takeoff mass the outer loop starts from, kg, instead of the plan's
    /// seed: a warm start from a neighbouring candidate's closure.
    pub initial_takeoff_mass_kg: Option<f64>,
    /// Integration steps per planned segment the plan refinement starts
    /// from, instead of the model default; the frozen count still meets
    /// `mission_model::frozen_plan::RICHARDSON_TOLERANCE`.
    pub steps_per_segment: Option<usize>,
    /// Screening-only drag tables check the fitted induced quadratic at the
    /// clean-CL endpoints and cruise check before refining failed cells.
    /// Reported candidates use the full set of independent interior checks.
    pub screening_drag_table: bool,
    /// Factor on the selected engine a clean-sheet engine sizing starts
    /// from instead of one: the search's deterministic start, the median
    /// required factor of its first calibration (`SearchDiagnostics::
    /// engine_start`), which its later analyses and the replays of its
    /// finalists take (`engine::sizing`). `None` starts from the selected
    /// engine, as the delivery guard's replay does.
    pub initial_engine_factor: Option<f64>,
    /// Relative distance from its aim within which a clean-sheet engine
    /// sizing stops (`engine::sizing`); `None`, as on every replayed and
    /// reported analysis, keeps the tight default that makes the sized
    /// engine independent of the start. A search model may stop wider.
    pub engine_aim_band: Option<f64>,
    /// Slope of a clean-sheet engine sizing's map from the installed to the
    /// aimed factor (`engine::sizing`) that its first update takes as known:
    /// a Newton step on a measured slope in place of the plain fixed-point
    /// update. The search's start, the median of the slopes its first
    /// calibration measured (`SearchDiagnostics::engine_slope`). It moves no
    /// root: the sizing still stops only inside its aim band. `None` takes
    /// the plain update.
    pub initial_engine_slope: Option<f64>,
    /// A search model holds an engine its run sizes (the exploration
    /// model's held engine, `ScreeningFidelity::held_engine`): its closure
    /// reports `engine_scale_validity` as a hard row on the scale the sizing
    /// would aim at on this airframe (the held catalogue scale times the
    /// requirement ratio, raised by half the match tolerance), so a candidate
    /// whose sized engine falls outside the rating window is refused by the
    /// model that ranks it, not first by the refinement model.
    pub predicts_engine_scale: bool,
}
