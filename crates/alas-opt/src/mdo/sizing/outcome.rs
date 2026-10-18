// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sized-candidate record and the trim that a re-sizing may reuse.

use alas_config::{airport_dataset, airports, AlasConfig, MtowPlan};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};

use crate::mdo::mda::{retrim_needed, retrim_needed_within};
use crate::mdo::trim::TrimmedPolar;
use crate::mdo::types::{HistoryFields, PayloadCapacity, SizedCandidate};

/// Everything a mission-sized residual table is computed from, beyond the
/// scalar summary in [`SizedCandidate`].
pub(crate) struct SizingOutcome {
    pub plane: Airplane,
    pub masses: MassBreakdown,
    pub coords: MassCoordinates,
    pub cg_x: f64,
    pub x_np: f64,
    pub mac: f64,
    /// Geometric aircraft-body angle at the cruise trim used by the mission
    /// model, before the presentation-only compressibility correction.
    pub geometric_body_alpha_deg: f64,
    pub n_engines: i64,
    pub static_thrust_kn: f64,
    pub departure: Option<&'static airports::Airport>,
    pub arrival: Option<&'static airports::Airport>,
    /// Source-resolved records used for routing/elevation. A record may be
    /// present while its runway values remain physical-only and therefore
    /// unusable by field-performance constraints.
    #[expect(
        dead_code,
        reason = "retained for finalist airport provenance reporting"
    )]
    pub departure_record: Option<airport_dataset::ProvenancedAirport>,
    #[expect(
        dead_code,
        reason = "retained for finalist airport provenance reporting"
    )]
    pub arrival_record: Option<airport_dataset::ProvenancedAirport>,
    /// Whether both configured aerodrome identifiers resolved to records.
    pub airport_records_resolved: bool,
    /// Whether both records carry declared operational runway distances.
    pub declared_airport_data_complete: bool,
    /// Whether the mission distance was explicit or could be computed from
    /// two finite source-resolved coordinates.
    pub mission_distance_known: bool,
    /// Minimum still-air distance for the configured climb/descent profile.
    pub minimum_profile_range_m: f64,
    /// Upper takeoff-mass limit of the plan, or the declared MTOW when the
    /// plan has none.
    pub mtow_ceiling: f64,
    /// The resolved takeoff-mass sizing plan of the candidate.
    pub plan: MtowPlan,
    pub sized: SizedCandidate,
    pub history: HistoryFields,
    #[expect(dead_code, reason = "retained for finalist load-case reporting")]
    pub capacity: PayloadCapacity,
    /// Whether the wing total represents a complete primary plus secondary
    /// inventory. Clean-sheet movable correlations remain partial.
    pub structural_inventory_complete: bool,
    /// The closure's native trim, offered to a re-sizing of this airframe;
    /// `None` when an external polar replaced the native trim.
    pub trim_reuse: Option<TrimReuse>,
    /// The geometry-scaled maximum lift coefficients this candidate was sized
    /// with, `None` when it keeps the configuration's declared values.
    pub clmax: Option<alas_perf::performance::clmax::ClmaxSet>,
    /// The registered aircraft's declared coefficients, present exactly when
    /// [`Self::clmax`] is: what the nominal references are keyed on and
    /// resolved under (lever A12).
    pub declared_clmax: Option<alas_perf::performance::clmax::ClmaxSet>,
}

impl SizingOutcome {
    /// `config` as the registered-aircraft references must read it: the
    /// candidate's solved gear and everything else, but the declared CLmax
    /// of the registered aircraft instead of this candidate's scaled ones.
    /// The reference nominal is a property of the registered aircraft, not
    /// of the candidate being compared with it.
    pub(crate) fn registered_config<'a>(
        &self,
        config: &'a AlasConfig,
    ) -> std::borrow::Cow<'a, AlasConfig> {
        match &self.declared_clmax {
            Some(set) if crate::mdo::speedups::enabled(crate::mdo::speedups::Lever::NominalKey) => {
                std::borrow::Cow::Owned(super::clmax_geometry::with_clmax(config, set))
            }
            _ => std::borrow::Cow::Borrowed(config),
        }
    }

    /// `config` as the residuals and the cost must read it: with this
    /// candidate's scaled maximum lift coefficients when it has them.
    pub(crate) fn config_with_clmax<'a>(
        &self,
        config: &'a AlasConfig,
    ) -> std::borrow::Cow<'a, AlasConfig> {
        match &self.clmax {
            Some(set) => std::borrow::Cow::Owned(super::clmax_geometry::with_clmax(config, set)),
            None => std::borrow::Cow::Borrowed(config),
        }
    }
}

/// One closure's native trim and lattice work, offered to a re-sizing of the
/// same candidate whose configuration differs only in its main-gear
/// translation (`residuals::gear_placement`).
///
/// Moving the gear changes no aerodynamic input: the airframe is built from
/// the geometry configuration and the design vector alone. It moves the
/// centre of gravity, and the closure's own rule applies to that move
/// (`mda::retrim_needed`): within `retrim_cg_tolerance_pct_mac` the polar
/// trimmed at the old centre of gravity stands, as it does between the
/// passes of one closure, and beyond it the re-sizing trims afresh. The
/// lattice and wake cache is exact for any centre of gravity.
#[derive(Clone)]
pub(crate) struct TrimReuse {
    /// The configuration the trim was solved for, without its gear
    /// translation.
    pub(super) config: AlasConfig,
    /// The design vector and the airframe as built, before any trim.
    pub(super) dv: alas_config::DesignVector,
    pub(super) untrimmed: Airplane,
    /// The airframe as the closure's last trim left it.
    pub(super) trimmed: Airplane,
    pub(super) polar: TrimmedPolar,
    pub(super) trim_cg_x_m: f64,
    pub(super) screening_drag_table: bool,
    pub(super) vlm_cache: crate::mdo::trim::CandidateVlmCache,
    /// Re-trim tolerance this trim may stand under, percent MAC (lever A13);
    /// `None` is the configured `retrim_cg_tolerance_pct_mac`.
    pub(super) cg_tolerance_pct_mac: Option<f64>,
}

impl TrimReuse {
    /// The trim (polar and centre of gravity) a closure left on `plane`, for a candidate configured as
    /// `config` and built as `untrimmed` from `dv`.
    pub(super) fn of_closure(
        config: &AlasConfig,
        dv: alas_config::DesignVector,
        untrimmed: Airplane,
        plane: &Airplane,
        trim: (&TrimmedPolar, f64),
        screening_drag_table: bool,
        vlm_cache: crate::mdo::trim::CandidateVlmCache,
    ) -> Self {
        let mut without_gear = config.clone();
        without_gear.landing_gear.derived_main_gear = None;
        Self {
            config: without_gear,
            dv,
            untrimmed,
            trimmed: plane.clone(),
            polar: trim.0.clone(),
            trim_cg_x_m: trim.1,
            screening_drag_table,
            vlm_cache,
            cg_tolerance_pct_mac: None,
        }
    }

    /// This trim, standing for a centre-of-gravity shift of up to
    /// `pct_mac` percent of the chord instead of the configured tolerance.
    pub(crate) fn with_cg_tolerance(mut self, pct_mac: f64) -> Self {
        self.cg_tolerance_pct_mac = Some(pct_mac);
        self
    }

    /// Whether this trim stands for `config`, `dv` and the freshly built
    /// `plane` with its centre of gravity at `cg_x_m`.
    pub(super) fn stands_for(
        &self,
        config: &AlasConfig,
        dv: &alas_config::DesignVector,
        plane: &Airplane,
        cg_x_m: f64,
        screening_drag_table: bool,
    ) -> bool {
        let mut without_gear = config.clone();
        without_gear.landing_gear.derived_main_gear = None;
        self.screening_drag_table == screening_drag_table
            && cg_x_m.is_finite()
            && self.dv == *dv
            && self.untrimmed == *plane
            && self.config == without_gear
            && !self.cg_tolerance_pct_mac.map_or_else(
                || retrim_needed(config, plane, cg_x_m, self.trim_cg_x_m),
                |pct| retrim_needed_within(pct, plane, cg_x_m, self.trim_cg_x_m),
            )
    }
}
