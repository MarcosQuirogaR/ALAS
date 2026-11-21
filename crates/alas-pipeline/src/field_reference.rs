// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! ISA sea-level field performance at the certification reference masses.
//!
//! Published aircraft-characteristics charts quote take-off field length at
//! maximum take-off mass and landing field length and reference speed at
//! maximum landing mass, on an ISA sea-level runway. The feasibility gate
//! evaluates the *departure and arrival airports* at the analysed masses, so
//! neither number exists anywhere in the pipeline. This module evaluates the
//! same `alas_perf` field-performance model on a synthetic ISA sea-level
//! aerodrome so the two can be compared, without touching the gate.
//!
//! Jet distances use the Raymer matching-chart correlations; propeller
//! distances use Torenbeek's force/energy and balanced-field method with the
//! installed speed-dependent thrust deck. These are conceptual estimates.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig, TurbopropEngineSpec};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_opt::mdo::{second_segment_evidence, EngineOutModel, V2ClimbOutPolar, YawAirframe};
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, decompose_gradient, far25_oei_gradient,
    oei_cl_at_v2, FieldPolar, GradientInputs, OeiClimbStatus,
};
use alas_prop::turboprop::Pw127mRating;

use crate::full_analysis::AnalysisReport;

/// ISA sea-level air density, kg/m^3 (ISO 2533 / ICAO Doc 7488).
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Landing reference speed over the 1 g landing-configuration stall speed:
/// 14 CFR 25.125(b)(2)(i), `V_REF >= 1.23 V_SR`, with `V_SR` taken as the 1 g
/// stall speed `V_S1g` the model computes. This is deliberately not the
/// approach-speed factor in `PerformanceConfig`, which is a calibration knob.
/// 14 CFR 25.103 defines `V_SR >= V_S1g`, so taking `V_SR = V_S1g` slightly
/// understates `V_REF`.
pub const VREF_OVER_VS1G: f64 = 1.23;

/// Share of the landing field length the actual landing distance from 50 ft
/// may use on a dry runway: 14 CFR 121.195(b) for turbine transports (EU Air
/// Ops CAT.POL.A.230 uses 60 % for jets and 70 % for turboprops). This
/// constant describes the jet and historical replay convention; corrected
/// propeller callers use their configured share. Airport-planning landing field
/// lengths are the actual distance divided by this share (Raymer, *Aircraft
/// Design: A Conceptual Approach*, 5th ed., 2012, Sec. 5.4 after eq. 5.11,
/// takes the FAR 25 field length as 1.667 times the distance).
pub const DRY_LANDING_DISTANCE_SHARE: f64 = 0.6;

/// Second-segment climb evidence the model supports: the required T/W and,
/// when the evidence is complete, the gradient the installed thrust delivers
/// (the same balance written for the available side,
/// `alas_perf::performance::decompose_gradient`).
#[derive(Debug, Clone, PartialEq)]
pub struct OeiSecondSegmentReference {
    /// Required minimum gross gradient, 14 CFR 25.121(b), when the engine
    /// count is on the Part 25 table.
    pub required_gradient: Option<f64>,
    /// Required in-flight-equivalent T/W to hold that gradient, when defined.
    pub required_inflight_tw: Option<f64>,
    /// Required installed sea-level T/W, only when the condition-specific
    /// evidence the assessor demands is configured.
    pub required_sls_tw: Option<f64>,
    /// The assessor's evidence status.
    pub status: OeiClimbStatus,
    /// What the numbers are, in words.
    pub basis: &'static str,
    /// V2 / VSR the second segment is assessed at.
    pub v2_over_vsr: f64,
    /// Whether V2 was raised above the CS 25.107 schedule to meet the
    /// gradient (improved climb); the jet field length is then charged by
    /// improved_climb_field_factor.
    pub improved_climb: bool,
    /// Factor on the jet take-off field length for the improved-climb V2.
    pub improved_climb_field_factor: f64,
    /// Gross gradient (rise over distance, dimensionless; 0.024 is 2.4 %) the
    /// installed all-engine static thrust delivers with one engine out at V2:
    /// `(N-1)/N r T/W_installed - CD/CL`, with `r` the operating-engine
    /// thrust at V2 over the rated static thrust and `CD` the drag of the
    /// assessed point (clean, take-off flap increment, failed engine, trim).
    /// `None` unless `status` is `SlsEquivalent` (the same evidence gate as
    /// the required T/W). For a propeller the installed T/W is the
    /// ground-roll mean thrust basis of `static_thrust_basis`.
    pub available_gradient: Option<f64>,
}

/// Gross-gradient residual allowed when an improved-climb `V2` is solved to the
/// CS 25.121(b) minimum (1e-4 percentage points).
const GRADIENT_SOLVER_TOLERANCE: f64 = 1.0e-6;

impl OeiSecondSegmentReference {
    /// Whether the available gross gradient holds the CS 25.121(b) minimum.
    /// Without both numbers (no evidence, or an engine count off the Part 25
    /// table) the gradient is not shown to be met. An improved-climb `V2` is
    /// solved to the minimum itself, so the comparison allows the solver's
    /// residual (`GRADIENT_SOLVER_TOLERANCE`).
    pub fn meets_required_gradient(&self) -> bool {
        matches!(
            (self.available_gradient, self.required_gradient),
            (Some(available), Some(required)) if available >= required - GRADIENT_SOLVER_TOLERANCE
        )
    }
}

/// Field performance and reference speeds on an ISA sea-level runway.
#[derive(Debug, Clone, PartialEq)]
pub struct IsaSeaLevelFieldReference {
    /// Maximum take-off mass used, kg.
    pub takeoff_mass_kg: f64,
    /// Maximum landing mass used, kg.
    pub landing_mass_kg: f64,
    /// All-engine installed sea-level thrust the T/W rests on, N.
    pub static_thrust_n: f64,
    /// Where that thrust comes from.
    pub static_thrust_basis: &'static str,
    /// Static T/W at the take-off mass (thrust over `m g`).
    pub static_tw: f64,
    /// Take-off flap deflection the field is flown at, degrees, when declared.
    pub takeoff_flap_deg: Option<f64>,
    /// How the take-off flap was selected among several sourced settings, with
    /// the field and gradient of each; `None` when only one is offered.
    pub takeoff_flap_selection: Option<String>,
    /// Conceptual take-off field length, m. For jets, the take-off-parameter
    /// correlation `37.7 (W/S)/(sigma CL_max,TO T/W)` [ft, psf] itself.
    ///
    /// That is Raymer's take-off correlation (*Aircraft Design: A Conceptual
    /// Approach*, 5th ed., 2012, Sec. 5.4, Fig. 5.4), which he reads directly
    /// as the FAR 25 take-off field length: already the 14 CFR 25.113 /
    /// balanced-field length, so it needs no further factor to compare with an
    /// airport-planning take-off field length (Loftin, NASA RP-1060, 1980,
    /// gives the same form with 37.5). Propellers instead use the larger of
    /// the Torenbeek BFL approximation and the once-factored AEO estimate.
    pub takeoff_field_length_m: f64,
    /// Definition of `takeoff_field_length_m`.
    pub takeoff_field_length_basis: &'static str,
    /// Conceptual balanced field length, m: the jet field correlation itself
    /// or Torenbeek's propeller balanced-field approximation. Historical
    /// replay alone applies the configurable legacy multiplier.
    pub model_balanced_field_length_m: f64,
    /// Definition of `model_balanced_field_length_m`.
    pub model_balanced_field_length_basis: &'static str,
    /// Factored dry landing field length, m: actual landing distance divided
    /// by the declared planning share (60% jet, 70% propeller by default).
    pub landing_field_length_m: f64,
    /// Definition of `landing_field_length_m`.
    pub landing_field_length_basis: &'static str,
    /// The CS 25.113 integration behind the jet take-off distances (all-engine
    /// distance, balanced `V1`, `VEF`, selected `VR`); `None` for the
    /// propeller method and the replay correlations.
    pub takeoff_field: Option<alas_perf::performance::TakeoffField>,
    /// Take-off safety speed `V2` at the take-off mass, m/s (CAS equals TAS
    /// at ISA sea level to within the compressibility correction the speed
    /// model applies).
    pub v2_m_s: f64,
    /// The speed schedule behind the reference (`VMC`, the schedule `VR`, the
    /// decision-speed proxy `V1` and the approach speed `VAPP`, m/s), at the
    /// take-off and landing masses of this reference. The integrated jet
    /// `V1` and `VR` are in [`Self::takeoff_field`]; the propeller method
    /// integrates no `V1`, so its decision and rotation speeds are these.
    pub speeds: alas_perf::performance::VSpeeds,
    /// Take-off-configuration reference stall speed `VSR` at the take-off
    /// mass, m/s, from which `V2 / VSR` is formed.
    pub v_stall_takeoff_m_s: f64,
    /// Unfactored landing distance from 50 ft, m: the Torenbeek (2013) Eqs.
    /// 9.21-9.24 air, flare and braked-run segments at Vref = 1.23 VS1g, with
    /// the mean deceleration of the declared main-gear truck (jets, p. 274) or
    /// the propeller method's sourced values; no reverse thrust. The
    /// `k_land` correlation remains only in `legacy_field_correlations` replay.
    /// The field length above is this divided by the dry share
    /// (`JET_DRY_LANDING_SHARE`), the share the optimizer residual, matching
    /// chart and delivery gate apply to the LDA.
    pub landing_distance_m: f64,
    /// 1 g landing-configuration stall speed at the landing mass, m/s.
    pub vs1g_landing_m_s: f64,
    /// `V_REF = 1.23 V_S1g`, m/s.
    pub vref_m_s: f64,
    /// Engine-out second-segment climb evidence.
    pub oei: OeiSecondSegmentReference,
}

/// All-engine installed sea-level thrust, N, and its basis.
///
/// Mirrors the gate's resolution (`feasibility::static_thrust`, which is
/// private to that module): the catalogue rating for a turbofan, the
/// propeller model's ground-roll mean thrust at `V_LOF/sqrt(2)` for a
/// turboprop. That mean remains a reported legacy/OEI diagnostic; the corrected
/// propeller field distance integrates the actual thrust curve separately.
/// At ISA sea level the lift-off equivalent airspeed is the lift-off speed itself.
pub fn sea_level_static_thrust_n(
    config: &AlasConfig,
    lift_off_speed_m_s: f64,
) -> Result<(f64, &'static str), String> {
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    if n_engines == 0 {
        return Err("no engine is installed on this aircraft".to_owned());
    }
    match engine.active_model().map_err(|error| error.to_string())? {
        ActiveEngineModel::Turbofan(_) => Ok((
            n_engines as f64 * engine.thrust_kn() * 1_000.0,
            "certificated static jet rating",
        )),
        ActiveEngineModel::Turboprop(spec) => Ok((
            propeller_roll_mean_thrust_n(spec, n_engines, lift_off_speed_m_s)?,
            "propeller model ground-roll mean thrust at V_LOF/sqrt(2)",
        )),
    }
}

fn propeller_roll_mean_thrust_n(
    spec: &TurbopropEngineSpec,
    n_engines: usize,
    lift_off_speed_m_s: f64,
) -> Result<f64, String> {
    let field = turboprop_unit_model(spec)
        .field_performance(
            ISA_SEA_LEVEL_DENSITY_KG_M3,
            Pw127mRating::NormalTakeoff,
            lift_off_speed_m_s,
        )
        .map_err(|error| format!("propeller ground-roll thrust unavailable: {error}"))?;
    let thrust_n = n_engines as f64 * field.mean_ground_roll_thrust_per_engine_n;
    if thrust_n.is_finite() && thrust_n > 0.0 {
        Ok(thrust_n)
    } else {
        Err(format!(
            "propeller ground-roll mean thrust {thrust_n} N is unusable"
        ))
    }
}

/// The `V2` climb-out drag locus of the report's airplane (Torenbeek 1982, Eq. 5-29).
pub fn v2_climb_out_polar(config: &AlasConfig, report: &AnalysisReport) -> V2ClimbOutPolar {
    let plane = &report.airplane;
    V2ClimbOutPolar::of(
        config,
        alas_opt::mdo::climb_out_aspect_ratio(config, plane.b_ref, plane.s_ref),
    )
}

/// Engine-out drag coefficient at the `V2` operating point, gear retracted,
/// on the locus the engine-out climb rows read ([`v2_climb_out_polar`]): the
/// take-off flaps are in it, so no flap increment is added.
///
/// The engine-out assessor evaluates `cd0 + k CL^2` at one selected CL only:
/// `(CD_at_CL, 0)` preserves that drag. Not a polar over other lift
/// coefficients; a CL above the take-off maximum is an error, not a value.
pub fn candidate_field_polar(
    config: &AlasConfig,
    report: &AnalysisReport,
    cl: f64,
    mach: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<(f64, f64), String> {
    if !cl.is_finite()
        || cl <= 0.0
        || !mach.is_finite()
        || mach <= 0.0
        || !altitude_m.is_finite()
        || !isa_deviation_c.is_finite()
    {
        return Err("field drag needs positive CL/Mach and finite atmosphere inputs".to_owned());
    }
    let cd = v2_climb_out_polar(config, report).cd(cl, config.performance.cl_max_to, true)?;
    if !cd.is_finite() || cd <= 0.0 {
        return Err("V2 climb-out drag is not finite and positive".to_owned());
    }
    Ok((cd, 0.0))
}

/// ISA sea-level field reference using shared candidate drag at the V2 point.
///
/// The engine-out assessment uses the V2 climb-out locus. Propeller
/// distances use a separate low-speed quadratic from the same candidate
/// model, with field high-lift and failed-engine increments added separately.
///
/// When the flight manual offers several sourced take-off flap settings and the
/// reference names none (`alas_config::presets::takeoff_flap_settings`), the
/// reference is the shortest take-off field over the settings whose
/// second-segment gradient at the take-off mass meets CS 25.121(b): ICAO
/// Annex 14 defines the aeroplane reference field length as the minimum field
/// length at maximum take-off mass, sea level, ISA, still air, and the flight
/// manual schedules a setting only where it meets the climb requirement. The
/// selection is recorded in `takeoff_flap_selection`.
pub fn report_isa_sea_level_field_reference(
    config: &AlasConfig,
    report: &AnalysisReport,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<IsaSeaLevelFieldReference, String> {
    let settings = alas_config::presets::takeoff_flap_settings(&config.preset, &config.performance);
    if settings.is_empty() {
        return reference_at_flap(
            config,
            None,
            report,
            wing_area_m2,
            takeoff_mass_kg,
            landing_mass_kg,
        );
    }
    let mut best: Option<IsaSeaLevelFieldReference> = None;
    let mut first: Option<IsaSeaLevelFieldReference> = None;
    let mut log = Vec::new();
    for setting in &settings {
        let reference = match reference_at_flap(
            config,
            Some(setting),
            report,
            wing_area_m2,
            takeoff_mass_kg,
            landing_mass_kg,
        ) {
            Ok(reference) => reference,
            Err(error) => {
                log.push(format!("{:.0} deg: {error}", setting.deflection_deg));
                continue;
            }
        };
        let meets = reference.oei.meets_required_gradient();
        log.push(format!(
            "{:.0} deg: CLmax_TO {:.3}, TOFL {:.0} m, OEI gradient {} vs {}",
            setting.deflection_deg,
            setting.cl_max_to,
            reference.takeoff_field_length_m,
            reference
                .oei
                .available_gradient
                .map_or("n/a".to_owned(), |g| format!("{:.2} %", 100.0 * g)),
            reference
                .oei
                .required_gradient
                .map_or("n/a".to_owned(), |g| format!("{:.1} %", 100.0 * g)),
        ));
        if meets
            && best.as_ref().is_none_or(|current| {
                reference.takeoff_field_length_m < current.takeoff_field_length_m
            })
        {
            best = Some(reference.clone());
        }
        first.get_or_insert(reference);
    }
    let (mut chosen, rule) = match (best, first) {
        (Some(best), _) => (best, "shortest field meeting CS 25.121(b)"),
        (None, Some(first)) => (
            first,
            "no setting meets CS 25.121(b); declared setting kept",
        ),
        (None, None) => {
            return Err(format!(
                "no take-off flap setting evaluates: {}",
                log.join("; ")
            ))
        }
    };
    chosen.takeoff_flap_selection = Some(format!(
        "{} deg take-off flap, {rule} (ICAO Annex 14 reference field length is the minimum at MTOW, SL, ISA); {}",
        chosen.takeoff_flap_deg.map_or("?".to_owned(), |d| format!("{d:.0}")),
        log.join("; ")
    ));
    Ok(chosen)
}

/// [`report_isa_sea_level_field_reference`] at take-off flap `flap`, else the
/// configured one.
fn reference_at_flap(
    config: &AlasConfig,
    flap: Option<&alas_config::presets::TakeoffFlapSetting>,
    report: &AnalysisReport,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<IsaSeaLevelFieldReference, String> {
    // The V speeds of the reference use the computed minimum control speed,
    // like the feasibility gate and the report figures, not the declared one.
    // The improved-climb V2 is the one this aerodrome needs, not the
    // departure's.
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let resolved = crate::report_config::resolve_report_at_flap(
        config,
        report,
        Some(&airport),
        flap,
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
    )
    .config;
    let config = &*resolved;
    let perf = &config.performance;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let cl = oei_cl_at_v2(perf.cl_max_to, speeds.v2_ms / speeds.v_stall_to_ms)
        .ok_or_else(|| "field V2 lift coefficient is invalid".to_owned())?;
    let atmosphere = alas_atmo::us1976_compute_values(0.0, 0.0);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let (cd0, k) = candidate_field_polar(config, report, cl, mach, 0.0, 0.0)?;
    let field_polar = if !perf.legacy_field_correlations {
        Some(crate::field_performance::report_field_polar(
            config, report, mach, &airport,
        )?)
    } else {
        None
    };
    reference_with_field_polar(
        config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        wing_area_m2,
        cd0,
        k,
        takeoff_mass_kg,
        landing_mass_kg,
        field_polar,
    )
}

/// Evaluate [`IsaSeaLevelFieldReference`] for `config`.
///
/// `takeoff_mass_kg` is MTOW and `landing_mass_kg` is MLW. `cd0` and `k` are
/// the parabolic-polar fit used for the engine-out assessment.
pub fn isa_sea_level_field_reference(
    config: &AlasConfig,
    wing_area_m2: f64,
    cd0: f64,
    k: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<IsaSeaLevelFieldReference, String> {
    reference_with_field_polar(
        config,
        None,
        wing_area_m2,
        cd0,
        k,
        takeoff_mass_kg,
        landing_mass_kg,
        None,
    )
}

// OEI point drag and the optional propeller field polar serve distinct conditions.
#[allow(clippy::too_many_arguments)]
fn reference_with_field_polar(
    config: &AlasConfig,
    airframe: Option<YawAirframe<'_>>,
    wing_area_m2: f64,
    cd0: f64,
    k: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    field_polar: Option<FieldPolar>,
) -> Result<IsaSeaLevelFieldReference, String> {
    let usable = |value: f64| value.is_finite() && value > 0.0;
    if !(usable(wing_area_m2) && usable(takeoff_mass_kg) && usable(landing_mass_kg)) {
        return Err("wing area and reference masses must be finite and positive".to_owned());
    }
    // ISA sea level, standard day. The runway length is irrelevant to the
    // required distances; it only has to exist to build the aerodrome.
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let perf = &config.performance;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let (static_thrust_n, static_thrust_basis) = sea_level_static_thrust_n(config, speeds.v_r_ms)?;
    let static_tw = static_thrust_n / (takeoff_mass_kg * config.requirements.gravity_m_s2);
    let reference_cd0 = field_polar.as_ref().map_or(cd0, |polar| polar.cd0);
    let polar = field_polar.unwrap_or_else(|| FieldPolar::parabola(cd0, k));
    let field = crate::field_performance::calculate_with_airframe(
        config,
        airframe,
        &airport,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        &polar,
    )?;
    let propeller = matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !perf.legacy_field_correlations;

    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let required_gradient = far25_oei_gradient(n_engines);
    // The same evidence builder as the optimizer residual, the feasibility
    // finding and the matching chart; the reference departs from ISA sea
    // level.
    let static_per_engine_n = static_thrust_n / n_engines.max(1) as f64;
    let model = EngineOutModel::new(config, airframe, wing_area_m2, Some(static_per_engine_n)).ok();
    let oei_condition =
        second_segment_evidence(model.as_ref(), perf, &airport, &speeds, &|_, _, _| {
            reference_cd0
        })
        .condition;
    // With an airframe `cd0` is the V2 climb-out locus (the flaps are in it, and
    // the engine-out asymmetry where its source says so); without one, the caller's fit.
    let v2_polar = airframe.map(|a| {
        V2ClimbOutPolar::of(
            config,
            alas_opt::mdo::climb_out_aspect_ratio(config, a.plane.b_ref, a.plane.s_ref),
        )
    });
    let (flap_delta_cd, oei_condition) = match v2_polar {
        Some(polar) => (0.0, polar.engine_out_condition(oei_condition)),
        None => (perf.oei_climb_delta_cd, oei_condition),
    };
    let cl_v2 = oei_cl_at_v2(perf.cl_max_to, oei_condition.v2_over_vstall)
        .ok_or_else(|| "field V2 lift coefficient is invalid".to_owned())?;
    let assessment = assess_oei_climb(
        cd0,
        k,
        n_engines,
        required_gradient.unwrap_or(perf.oei_gradient),
        cl_v2,
        flap_delta_cd,
        perf.cl_max_to,
        Some(oei_condition),
    );
    // The available side of the same balance, on the same evidence and gate.
    let available_gradient = (assessment.status == OeiClimbStatus::SlsEquivalent)
        .then(|| {
            decompose_gradient(&GradientInputs {
                n_engines,
                installed_tw_sls: static_tw,
                thrust_ratio: oei_condition.condition_to_sls_thrust_ratio?,
                cl: cl_v2,
                cd0_clean: cd0,
                induced_factor: k,
                flap_delta_cd,
                windmilling_cd: oei_condition.windmilling_cd?,
                trim_cd: oei_condition.asymmetric_trim_cd?,
            })
        })
        .flatten()
        .map(|decomposition| decomposition.gradient);
    Ok(IsaSeaLevelFieldReference {
        takeoff_mass_kg,
        landing_mass_kg,
        static_thrust_n,
        static_thrust_basis,
        static_tw,
        takeoff_flap_deg: perf.takeoff_flap_deflection_deg,
        takeoff_flap_selection: None,
        takeoff_field_length_m: field.todr_m,
        takeoff_field_length_basis: if perf.legacy_field_correlations {
            "translated replay: Raymer take-off-parameter correlation 37.7 (W/S)/(sigma CLmax_TO T/W) [ft, psf] (Aircraft Design, Sec. 5.4, Fig. 5.4)"
        } else if propeller {
            "Torenbeek ch.5/App.K propeller field estimate with speed-dependent installed thrust; \
             max of 115% AEO distance and eq.5-89 balanced-field approximation. Dry, level, still \
             air; normal AEO rating and installed automatic takeoff reserve on the remaining engines; failed-engine drag (feathered \
             propeller) and asymmetric trim drag from the shared engine-out evidence builder. Conceptual, not certificated."
        } else {
            "CS 25.113(a) take-off field length by integration: max(1.15 x all-engine distance to 35 ft, balanced field length), dry level hard runway, still air, ISA sea level. Ground roll with the deck take-off thrust at every speed (Raymer Eqs. 17.100-17.101, mu 0.02 Torenbeek p. 168), 3 s rotation (Raymer p. 673), airborne distance the larger of the energy (Torenbeek Eq. 5-85, integrated with the drag at each height) and transition-arc (Raymer Eqs. 17.105-17.112) distances, the lift-dependent drag reduced in ground effect (Torenbeek 1982 App. G-7.2 Eq. G-77; CS 25.111(d)) and the drag table read at the flown lift coefficient, gear down to 35 ft (AMC 25.111(b)(2); Torenbeek 1982 p. 551); one-engine continued take-off with the failure at VEF and the failed-engine windmilling plus trim drag; VR searched between the schedule minimum and V2 for the shortest field. Conceptual, not certificated"
        },
        model_balanced_field_length_m: field.bfl_m,
        model_balanced_field_length_basis: if perf.legacy_field_correlations {
            "historical bfl_factor times the jet field correlation; duplicate margin retained for replay"
        } else if propeller {
            "Torenbeek eq.5-89 conceptual balance with mean acceleration integrated from rest to the eq.5-88 V1, \
             V2 thrust lapse and configured stop deceleration; no certified V1/ASDA claim"
        } else {
            "balanced field: CS 25.109(a)(1) accelerate-stop (all engines to VEF, one engine to V1, braking at 0.37 g mean (Torenbeek Eq. 5-87, 15 jet transports, no reverse thrust), plus 3.5 s at V1: the CS 25.109 2 s increment and the pilot-action transition that completes the 4.5 s inertia time the 0.37 g was fitted with) equal to the CS 25.113(a)(1) continued take-off distance at the balanced V1"
        },
        landing_field_length_m: field.ldr_m
            / crate::field_performance::dry_landing_distance_share(config),
        takeoff_field: field.takeoff_field,
        landing_field_length_basis: if propeller {
            "FAA AC 25-7D 4.11 air distance, 2 s transition and Torenbeek eq.5-94 braking, no reverse credit, at MLW; \
             divided by configured dry planning share (CAT.POL.A.230(a)(2): 0.70 default, \
             FAR 121.195(b): select 0.60); 25.125 actual distance starts at 50 ft"
        } else {
            "FAR 25 factored dry landing field length: unfactored 25.125 distance from 50 ft \
             (FAA AC 25-7D 4.11 air distance, 2 s transition, truck-class braking) at MLW, ISA sea level, divided \
             by 0.6 (14 CFR 121.195(b)); k_land (W/S)/(sigma CLmax_land) only in the legacy replay"
        },
        landing_distance_m: field.ldr_m,
        v2_m_s: speeds.v2_ms,
        speeds,
        v_stall_takeoff_m_s: speeds.v_stall_to_ms,
        vs1g_landing_m_s: speeds.v_stall_land_ms,
        vref_m_s: VREF_OVER_VS1G * speeds.v_stall_land_ms,
        oei: OeiSecondSegmentReference {
            required_gradient,
            required_inflight_tw: assessment.required_inflight_tw,
            required_sls_tw: assessment.required_sls_tw,
            status: assessment.status,
            v2_over_vsr: speeds.v2_ms / speeds.v_stall_to_ms,
            improved_climb: perf.improved_climb_v2_over_vsr.is_some(),
            improved_climb_field_factor: perf.improved_climb_field_factor(),
            available_gradient,
            basis: "required T/W to hold the CS 25.121(b) gross gradient at V2 = max(selected \
                 factor, 1.13 VSR, 1.10 VMC) with VMC computed from the engine-out yaw balance of \
                 the built fin and rudder when the report supports it (the declared ratio, the \
                 CS 25.149(c) ceiling, is the fallback); thrust ratio from the \
                 engine deck at V2; drag on the V2 climb-out locus CD = C0 + CL^2 / (pi A E) \
                 (Torenbeek 1982, Eq. 5-29; pair by slat state; E lowered 4 % when the critical \
                 engine is wing-mounted and 2 % when it is on the rear fuselage, read as the \
                 asymmetric-trim drag (Torenbeek G-84..89 is added instead where no reduction \
                 applies); failed-engine drag (Roskam VI 4.67) added; the available gradient is the same \
                 balance at the installed static T/W (alas_perf decompose_gradient)",
        },
    })
}

// The tests build curated presets, so a failed expect is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;
    use serde_json::json;

    /// The V2 point projection is the climb-out locus of the configuration
    /// (Torenbeek 1982, Eq. 5-29), not the report's polar fit and not the
    /// clean table: `C0 + CL^2 / (pi A E)` with the pair of the slat state and
    /// the engine-failure factor of the engine position.
    #[test]
    fn field_drag_is_the_v2_climb_out_locus() {
        // (preset, C0, E, engine-failure factor): the ATR 72 has no slats and
        // wing-mounted engines; the A320 has slats and wing-mounted engines.
        for (name, c0, e, factor) in [
            ("ATR72-600", 0.005, 0.61, 0.96),
            ("A320-200", 0.018, 0.70, 0.96),
        ] {
            let config = AlasConfig::from_value(&json!({ "preset": name })).expect("preset");
            let design = alas_config::presets::get(name)
                .expect("preset")
                .design_vector;
            let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
                .run(&design, true)
                .expect("full analysis");
            // The report fit is not a field drag source.
            report.polar_fit.cd0 = f64::NAN;
            report.polar_fit.c1 = f64::NAN;
            report.polar_fit.k = f64::NAN;
            let aspect_ratio = alas_opt::mdo::climb_out_aspect_ratio(
                &config,
                report.airplane.b_ref,
                report.airplane.s_ref,
            );
            let (cl, mach, altitude_m) = (1.1, 0.22, 2_100.0);
            let expected = c0 + cl * cl / (std::f64::consts::PI * aspect_ratio * e * factor);
            for deviation_c in [-20.0, 0.0, 17.0] {
                let (cd0, k) =
                    candidate_field_polar(&config, &report, cl, mach, altitude_m, deviation_c)
                        .expect("field drag");
                assert_eq!(k, 0.0);
                assert!(
                    (cd0 - expected).abs() < 1e-12,
                    "{name}: {cd0} vs {expected}"
                );
            }
            assert!(v2_climb_out_polar(&config, &report).notes().is_empty());
            // Above the take-off maximum lift the locus is not extrapolated.
            let above = config.performance.cl_max_to + 0.1;
            assert!(candidate_field_polar(&config, &report, above, mach, altitude_m, 0.0).is_err());
        }
    }

    #[test]
    fn the_sea_level_field_is_continuous_and_monotone_in_take_off_mass_across_the_second_segment_limit(
    ) {
        // A350-900: at MTOW the second segment is met only by a raised V2. A 0.5 %
        // mass step (about the same relative change in thrust-to-weight) must not move
        // by a jump, whether or not some V2 can meet the gradient. The response is steep
        // (about 10 % per step) because the required thrust is flat near the best-climb
        // speed, but it is continuous: the earlier fall-back to the schedule speed when
        // no V2 met the gradient was a 45 % step.
        let config = AlasConfig::from_value(&json!({ "preset": "A350-900" })).expect("preset");
        let design = alas_config::presets::get("A350-900")
            .expect("preset")
            .design_vector;
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("full analysis");
        let mtow = config.requirements.mtow_kg;
        let field = |scale: f64| {
            report_isa_sea_level_field_reference(
                &config,
                &report,
                report.airplane.s_ref,
                mtow * scale,
                config.landing_mass_limit_kg(mtow),
            )
            .expect("reference")
            .takeoff_field_length_m
        };
        let lengths: Vec<f64> = [0.985, 0.99, 0.995, 1.0, 1.005, 1.01]
            .iter()
            .map(|scale| field(*scale))
            .collect();
        for pair in lengths.windows(2) {
            assert!(pair[1] >= pair[0] * 0.999, "not monotone: {lengths:?}");
            assert!(pair[1] <= pair[0] * 1.15, "jump: {lengths:?}");
        }
    }

    #[test]
    fn a320_reference_is_finite_and_ordered() {
        let mut config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).expect("preset");
        // No fin is built on this path: state the asymmetric trim drag.
        config.performance.oei_asymmetric_trim_cd = Some(0.001);
        let mtow = config.requirements.mtow_kg;
        let reference =
            isa_sea_level_field_reference(&config, 122.6, 0.02, 0.04, mtow, 0.92 * mtow)
                .expect("field reference");
        for value in [
            reference.static_tw,
            reference.takeoff_field_length_m,
            reference.model_balanced_field_length_m,
            reference.landing_field_length_m,
            reference.landing_distance_m,
            reference.vref_m_s,
        ] {
            assert!(value.is_finite() && value > 0.0, "{reference:?}");
        }
        // CS 25.113(a): the field length is the greater of the balanced field
        // and 1.15 times the all-engine distance; both legs are reported.
        let solved = reference.takeoff_field.expect("integrated jet field");
        assert_eq!(reference.takeoff_field_length_m, solved.tofl_m);
        assert_eq!(reference.model_balanced_field_length_m, solved.balanced_m);
        assert!((solved.tofl_m - solved.balanced_m.max(1.15 * solved.all_engine_m)).abs() < 1e-9);
        assert!(solved.balanced_m <= reference.takeoff_field_length_m + 1e-9);
        assert!(solved.v1_m_s <= solved.vr_m_s && solved.vr_m_s <= reference.v2_m_s + 1e-9);
        // The factored landing field length is the actual distance over 0.6.
        assert!(
            (reference.landing_field_length_m / reference.landing_distance_m - 1.0 / 0.6).abs()
                < 1e-12
        );
        assert!((reference.vref_m_s / reference.vs1g_landing_m_s - VREF_OVER_VS1G).abs() < 1e-12);
    }

    #[test]
    fn the_turboprop_uses_the_propeller_roll_mean_thrust() {
        let config = AlasConfig::from_value(&json!({ "preset": "ATR72-600" })).expect("preset");
        let mtow = config.requirements.mtow_kg;
        let design = alas_config::presets::get("ATR72-600")
            .expect("preset")
            .design_vector;
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("analysis");
        let reference = report_isa_sea_level_field_reference(
            &config,
            &report,
            report.airplane.s_ref,
            mtow,
            0.95 * mtow,
        )
        .expect("turboprop field reference");
        assert!(reference.static_thrust_basis.contains("propeller"));
        assert!(reference.static_tw > 0.0 && reference.takeoff_field_length_m.is_finite());
    }
}
