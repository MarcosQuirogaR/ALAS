// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clean-sheet engine sizing: a clean-sheet evaluation scales the
//! selected catalogue engine to the thrust requirements of its own closed
//! design, instead of flying the catalogue rating whatever the design needs.
//!
//! The evidence is physical consistency, not a fit to a reference: the sized
//! rating meets every thrust row on its nominal value with at most 1 percent
//! to spare, the engine mass, nacelle and fuel flow follow the rating, the
//! specific consumption stays the baseline's, and a registered aircraft in a
//! reference mode keeps its declared engine untouched.

// A test asserts on values it built from a brief it controls, so a failed
// unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, ActiveEngineModel, AlasConfig, DesignVector};
use alas_opt::{assess_product_candidate, canonicalize_design, CandidateAssessment};
use serde_json::json;

/// The rows a thrust requirement is written on (nominal value: available at
/// least the required, `actual >= limit`).
const THRUST_ROWS: [&str; 5] = [
    "oei_second_segment",
    "oei_final_segment",
    "oei_approach_climb",
    "landing_climb",
    "takeoff_field",
];

fn preset_clean_sheet(name: &str) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
    let preset = presets::registry().iter().find(|p| p.name == name).unwrap();
    let design = canonicalize_design(&config, preset.design_vector).unwrap();
    (config, design)
}

fn assess(config: &AlasConfig, design: &DesignVector) -> CandidateAssessment {
    assess_product_candidate(config, design).unwrap_or_else(|reason| panic!("assessment: {reason}"))
}

fn held(config: &AlasConfig) -> AlasConfig {
    let mut held = config.clone();
    held.optimizer.design_space.engine_sized_by_requirements = false;
    held
}

fn row<'a>(a: &'a CandidateAssessment, id: &str) -> &'a alas_opt::ConstraintResidual {
    a.residuals
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("row {id} missing"))
}

fn rating_kn(engine: &alas_config::EngineConfig) -> f64 {
    engine.scaling_rating().unwrap()
}

/// What sizing must deliver on a converged clean-sheet evaluation: the
/// installed rating meets the requirement and exceeds it by at most the
/// match tolerance, inside the validity window, with every thrust row met.
fn assert_sized(label: &str, config: &AlasConfig, a: &CandidateAssessment) {
    let matched = row(a, "engine_thrust_match");
    assert_eq!(matched.normalized_violation, 0.0, "{label}: match row");
    // Sign contract: positive when violated, so not positive when met.
    assert!(matched.raw_residual <= 0.0, "{label}: {matched:?}");
    let installed = matched.actual;
    let required = matched.limit;
    assert!(
        required <= installed * (1.0 + 1.0e-9) && installed <= required * 1.011,
        "{label}: installed {installed} against required {required}"
    );
    let sized = a
        .resolved
        .sized_engine
        .as_ref()
        .unwrap_or_else(|| panic!("{label}: sized engine"));
    assert!((rating_kn(&sized.engine) - installed).abs() < 1.0e-9 * installed);
    let window = config.optimizer.design_space.engine_rating_window;
    let scale = row(a, "engine_scale_validity").actual;
    assert!(
        (scale - 1.0).abs() <= window,
        "{label}: catalogue scale {scale}"
    );
    for id in THRUST_ROWS {
        if let Some(r) = a.residuals.iter().find(|r| r.id == id) {
            assert!(
                r.limit.is_finite() && r.actual >= r.limit * (1.0 - 2.0e-5),
                "{label}: {id} actual {} limit {}",
                r.actual,
                r.limit
            );
        }
    }
}

/// Engine mass group over the thrust ratio: the FLOPS group scales the
/// engine with `(T/T0)^1.15`, the nacelle with its area, the reversers
/// linearly, the controls with the square root and the pylon with
/// `T^0.65`: the group follows the rating with an exponent between 0.8
/// and 1.2.
fn assert_mass_follows_the_rating(
    label: &str,
    sized: &CandidateAssessment,
    fixed: &CandidateAssessment,
    thrust_ratio: f64,
) {
    let ratio = sized.resolved.masses.propulsion / fixed.resolved.masses.propulsion;
    let exponent = ratio.ln() / thrust_ratio.ln();
    assert!(
        (0.8..1.2).contains(&exponent),
        "{label}: propulsion mass ratio {ratio} for thrust ratio {thrust_ratio} (exponent {exponent})"
    );
}

#[test]
fn an_a320_class_clean_sheet_engine_is_sized_just_above_its_catalogue_rating() {
    let (config, design) = preset_clean_sheet("A320-200");
    let sized = assess(&config, &design);
    let fixed = assess(&held(&config), &design);
    assert_sized("A320-200", &config, &sized);
    let sized_engine = sized.resolved.sized_engine.as_ref().unwrap();
    let catalogue = config.geometry.engine.clone();
    let thrust_ratio = rating_kn(&sized_engine.engine) / rating_kn(&catalogue);
    // The model's design-level cruise requirement governs the A320 brief on
    // a 4,100 m runway: 1.028 of the CFM56-5B4 rating, about 0.957 without the
    // afterbody and installation parasite items. Cause, isolated by switching
    // those items off in place: the
    // upsweep (Raymer Eq. 12.36, 10.0 counts) and base (Torenbeek 1982 Eq.
    // F-42, 1.8 counts) readings plus pylons and fairing (4.2 counts). About
    // 3 % is left unexplained: the Airbus FCOM M.78 ISA cruise table (Getting
    // to Grips with Aircraft Performance, p. 148, Fig. F27) holds FL370 on max
    // cruise thrust up to 70 t and the requirement point is 70.4 t, so the real
    // engine just meets it.
    assert!((1.00..1.10).contains(&thrust_ratio), "{thrust_ratio}");
    assert_mass_follows_the_rating("A320-200", &sized, &fixed, thrust_ratio);
    // Specific consumption is the baseline's; the engine is just smaller.
    assert_eq!(
        sized_engine.engine.cruise_tsfc_kg_kgf_hr,
        catalogue.cruise_tsfc_kg_kgf_hr
    );
    assert!(sized.sized.operating_empty_mass_kg > fixed.sized.operating_empty_mass_kg);
    // Held engine: the catalogue rating, with the requirement reported (120.1
    // against 122.87 kN, see above).
    assert!(fixed.resolved.sized_engine.is_none());
    let margin = row(&fixed, "engine_thrust_margin");
    assert_eq!(margin.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert!(margin.actual < margin.limit, "{margin:?}");
    // A held engine within the window of its requirement is matched.
    let oversize = row(&fixed, "engine_oversize");
    assert_eq!(oversize.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert_eq!(oversize.normalized_violation, 0.0);
    assert!(fixed
        .residuals
        .iter()
        .all(|r| r.id != "engine_thrust_match"));
}

#[test]
fn a_widebody_clean_sheet_engine_is_sized_above_its_catalogue_rating() {
    let (config, design) = preset_clean_sheet("B787-9");
    let sized = assess(&config, &design);
    let fixed = assess(&held(&config), &design);
    assert_sized("B787-9", &config, &sized);
    let sized_engine = sized.resolved.sized_engine.as_ref().unwrap();
    let thrust_ratio = rating_kn(&sized_engine.engine) / rating_kn(&config.geometry.engine);
    assert!((1.02..1.20).contains(&thrust_ratio), "{thrust_ratio}");
    assert_mass_follows_the_rating("B787-9", &sized, &fixed, thrust_ratio);
    assert!(sized.sized.operating_empty_mass_kg > fixed.sized.operating_empty_mass_kg);
    // The nacelle grows with the rating (Raymer 10.1, 10.2).
    let (b, s) = (&config.geometry.engine, &sized_engine.engine);
    assert!((s.fan_diameter_m / b.fan_diameter_m - thrust_ratio.sqrt()).abs() < 1e-9);
    assert!((s.nacelle_length_m() / b.nacelle_length_m() - thrust_ratio.powf(0.4)).abs() < 1e-9);
}

#[test]
fn a_clean_sheet_brief_without_a_preset_sizes_its_engine_to_the_brief() {
    // 90 t, 180 seats, no preset: only the brief's numbers and the engine.
    // The planned profile of this brief is a step in the
    // engine factor: below about 1.05 of the LEAP-1A rating the plan climbs
    // through six cruise levels to FL350, above it collapses to one level near
    // 6.7 km, and the thrust requirement steps by about 2 percent across it.
    // The sizing fixed point has no solution inside the 1 percent match band
    // then (closures alternate 1.009 / 0.976), so every brief from 80 t to 86 t
    // ends unmatched (probed); 88 t and 90 t sit clear of the step and size.
    // The step itself is a mission-planner behaviour (a larger engine planning a
    // lower, single cruise level), not sizing behaviour.
    let config = AlasConfig::from_value(&json!({
        "requirements": {"mtow_kg": 90_000.0, "num_passengers": 180, "cruise_mach": 0.78, "cruise_altitude_m": 10_668.0},
        "geometry": {"fuselage": {"diameter_m": 3.96}, "engine": {"engine_name": "LEAP-1A"}},
        "optimizer": {"objective": {"design_range_nmi": 3000.0}}
    }))
    .unwrap();
    assert!(config.derives_clean_sheet_start());
    let design = config.configured_nominal_design();
    let sized = assess(&config, &design);
    assert_sized("brief 90 t", &config, &sized);
}

#[test]
fn a_clean_sheet_turboprop_is_sized_in_shaft_power_with_its_propeller() {
    let (config, design) = preset_clean_sheet("ATR72-600");
    let sized = assess(&config, &design);
    assert_sized("ATR72-600", &config, &sized);
    let engine = &sized.resolved.sized_engine.as_ref().unwrap().engine;
    let (Ok(ActiveEngineModel::Turboprop(base)), Ok(ActiveEngineModel::Turboprop(new))) =
        (config.geometry.engine.active_model(), engine.active_model())
    else {
        panic!("turboprop binding");
    };
    let x = new.takeoff_shaft_power_kw / base.takeoff_shaft_power_kw;
    // Raymer Eq. 10.23, at the tip speed of the baseline propeller.
    assert!((new.propeller_diameter_m / base.propeller_diameter_m - x.powf(0.25)).abs() < 1e-9);
    assert!(
        (new.propeller_diameter_m * new.governed_propeller_speed_rpm
            / (base.propeller_diameter_m * base.governed_propeller_speed_rpm)
            - 1.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn an_engine_that_needs_more_than_the_window_is_infeasible_not_extrapolated() {
    // The E195-E2 brief needs 0.797 of the PW1900G at the viscous margin 1.00 and
    // 0.819 at the default 1.035 (probed with the margin set back):
    // outside the 15 percent window either way. With the product drag items it is
    // 0.861, inside 15 percent, so the window is 10 percent. The row says so, and widening the window clears it.
    let (mut config, design) = preset_clean_sheet("E195-E2");
    config.optimizer.design_space.engine_rating_window = 0.10;
    let strict = assess(&config, &design);
    let validity = row(&strict, "engine_scale_validity");
    assert!(validity.normalized_violation > 0.0, "{validity:?}");
    assert!(strict
        .violated_hard_ids()
        .contains(&"engine_scale_validity"));
    // The need itself is pinned, because the margin moves with any
    // drag change, so a drift of the need shows here before it flips the
    // default-window outcome. The need is 0.8249 without the product drag items
    // (pylons, wing-body fairing, the added drag areas) and 0.8607 with them;
    // 0.8756 with the cambered-surface boundary
    // condition and the tail dynamic-pressure ratio of the lattice; still outside
    // the 10 percent window, so the infeasible verdict is unchanged.
    assert!(
        (validity.actual - 0.8756).abs() < 0.003,
        "E195-E2 engine need {} (0.8756 after vlm-physics; 0.8607 before it)",
        validity.actual
    );
    let mut wide = config.clone();
    wide.optimizer.design_space.engine_rating_window = 0.30;
    let relaxed = assess(&wide, &design);
    assert_eq!(
        row(&relaxed, "engine_scale_validity").normalized_violation,
        0.0
    );
    assert_sized("E195-E2 widened", &wide, &relaxed);
}

#[test]
fn a_registered_aircraft_in_a_reference_mode_keeps_its_declared_engine() {
    for mode in [DesignMode::ReferenceAdaptation, DesignMode::BaselineSandbox] {
        let mut config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
        config.optimizer.design_space.mode = mode;
        let preset = presets::registry()
            .iter()
            .find(|p| p.name == "A320-200")
            .unwrap();
        let design = canonicalize_design(&config, preset.design_vector).unwrap();
        let a = assess(&config, &design);
        assert!(a.resolved.sized_engine.is_none(), "{mode:?}");
        for id in [
            "engine_thrust_match",
            "engine_scale_validity",
            "engine_thrust_margin",
        ] {
            assert!(a.residuals.iter().all(|r| r.id != id), "{mode:?}: {id}");
        }
    }
}

#[test]
fn replaying_the_sized_engine_as_a_held_engine_finds_it_matched() {
    let (config, design) = preset_clean_sheet("C919");
    let sized = assess(&config, &design);
    assert_sized("C919", &config, &sized);
    let mut replay = held(&config);
    replay.apply_sized_engine(sized.resolved.sized_engine.as_ref().unwrap());
    let again = assess(&replay, &design);
    let margin = row(&again, "engine_thrust_margin");
    // The held engine is the sized one: required/installed within the match
    // band (the replay closes from a cold start, so allow its tolerance).
    let ratio = margin.limit / margin.actual;
    assert!((0.985..=1.003).contains(&ratio), "{ratio}");
}

#[test]
fn a_brief_far_from_the_catalogue_engine_is_reported_not_extrapolated() {
    // A 70 t narrow-body brief on the 467 kN GE9X needs a fraction of it. The
    // search stops at the end of its bracket (never an unbounded extrapolation),
    // the window row carries the mismatch and the design is infeasible.
    let config = AlasConfig::from_value(&json!({
        "requirements": {"mtow_kg": 70_000.0, "num_passengers": 168, "cruise_mach": 0.78, "cruise_altitude_m": 10_668.0},
        "geometry": {"fuselage": {"diameter_m": 3.96}, "engine": {"engine_name": "GE9X"}},
        "optimizer": {"objective": {"design_range_nmi": 2000.0}}
    }))
    .unwrap();
    let design = config.configured_nominal_design();
    let a = assess(&config, &design);
    let scale = row(&a, "engine_scale_validity");
    assert!(scale.normalized_violation > 0.0, "{scale:?}");
    assert!(scale.actual >= alas_config::ENGINE_SCALE_SEARCH_BRACKET.0 * (1.0 - 1.0e-9));
    assert!(a.violated_hard_ids().contains(&"engine_scale_validity"));
    assert!(!a.hard_feasible);
    let matched = row(&a, "engine_thrust_match");
    assert!(
        matched.normalized_violation > 0.0 && matched.raw_residual > 0.0,
        "{matched:?}"
    );
}

fn a320_with_field(stated_m: Option<f64>) -> CandidateAssessment {
    let (mut config, design) = preset_clean_sheet("A320-200");
    config.toda_override_m = stated_m;
    assess(&config, &design)
}

#[test]
fn a_shorter_stated_field_length_drives_the_sizing_and_a_longer_one_is_ignored_with_a_notice() {
    let route = a320_with_field(None);
    let route_rating = rating_kn(&route.resolved.sized_engine.as_ref().unwrap().engine);
    assert!(route
        .residuals
        .iter()
        .all(|r| r.id != "takeoff_field_override_ignored"));
    let toda = row(&route, "takeoff_field").actual;

    // 2,300 m is far shorter than the 4,100 m of the route (E7: 2,600 m no
    // longer binds past the 2 percent margin once the field is flown in ground
    // effect): the field row
    // governs and the engine grows past what the route needed. The
    // route rating is 123.48 kN (cruise-governed, see the
    // A320 test) and the field needs 123.77 kN, +0.24 %
    // (2,200 m gives 123.80 and 2,100 m cannot be met: the heavier sized engine
    // and fuel feed back into the field), no longer past the
    // former 2 % (isolated with the items off in place).
    let shorter = a320_with_field(Some(2_300.0));
    let field = row(&shorter, "takeoff_field");
    assert_eq!(field.actual, 2_300.0);
    assert!(field.limit <= 2_300.0 * (1.0 + 1.0e-5), "{field:?}");
    let shorter_rating = rating_kn(&shorter.resolved.sized_engine.as_ref().unwrap().engine);
    assert!(
        shorter_rating > route_rating,
        "{shorter_rating} vs {route_rating}"
    );
    assert_eq!(
        row(&shorter, "engine_thrust_match").normalized_violation,
        0.0
    );
    assert!(shorter
        .residuals
        .iter()
        .all(|r| r.id != "takeoff_field_override_ignored"));

    // A value longer than the route's is never used to lengthen the runway.
    // The stated value must stay inside the validity range (600-5000 m) to be noticed.
    assert!(toda + 300.0 <= 5000.0, "route TODA {toda} m");
    let longer = a320_with_field(Some(toda + 300.0));
    assert_eq!(row(&longer, "takeoff_field").actual, toda);
    assert_eq!(
        rating_kn(&longer.resolved.sized_engine.as_ref().unwrap().engine).to_bits(),
        route_rating.to_bits()
    );
    let notice = row(&longer, "takeoff_field_override_ignored");
    assert_eq!(notice.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert_eq!((notice.actual, notice.limit), (toda + 300.0, toda));
}

#[test]
fn a_held_engine_more_than_the_window_above_its_requirement_is_a_warning_only_diagnostic() {
    // The E195-E2 brief needs about 0.85 of the PW1900G rating: a 2 percent window flags the
    // held engine, through the held path (`hold_as_drawn`, as a promoted
    // sandbox does) that the polar path shares. The A320's brief needs
    // 1.028 of the CFM56, so its held engine is not oversize.
    let (mut config, design) = preset_clean_sheet("E195-E2");
    config.optimizer.design_space.hold_as_drawn();
    config.optimizer.design_space.engine_rating_window = 0.02;
    let a = assess(&config, &design);
    let oversize = row(&a, "engine_oversize");
    assert!(oversize.normalized_violation > 0.0, "{oversize:?}");
    // Sign contract (types.rs): positive when violated, actual above limit.
    assert!(oversize.raw_residual > 0.0 && oversize.actual > oversize.limit);
    // Warning only: a Diagnostic never refuses the
    // design; the plausibility rule ENGINE-OVERSIZE reports it.
    assert_eq!(oversize.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert!(!a.violated_hard_ids().contains(&"engine_oversize"));
    // The held E195-E2: an undetermined obstacle verdict at 36L is a Diagnostic warning, not a
    // rejection, and a run held to the registered aircraft reports the absolute tail-scrape row
    // as a diagnostic (the registered aircraft is certified in that state). What remains hard is
    // the relative usable-CG guard that the held run keeps. Any other hard row must fail here.
    let mut hard = a.violated_hard_ids();
    hard.sort_unstable();
    assert_eq!(hard, ["usable_cg_range_vs_nominal"]);
    assert!(oversize
        .detail
        .as_deref()
        .unwrap()
        .contains("rating window"));

    // Inside the default window the row is met: raw residual not positive and
    // no violation detail.
    let mut inside = config.clone();
    inside.optimizer.design_space.engine_rating_window = 0.20;
    let b = assess(&inside, &design);
    let met = row(&b, "engine_oversize");
    assert!(
        met.raw_residual <= 0.0 && met.actual <= met.limit,
        "{met:?}"
    );
    assert_eq!(met.normalized_violation, 0.0);
    assert!(met.detail.is_none());
    // The window never changes the hard verdict (the held E195-E2 has its own
    // geometry rows, e.g. root_to_kink_te_angle, in both).
    assert_eq!(a.violated_hard_ids(), b.violated_hard_ids());
    assert_eq!(a.hard_feasible, b.hard_feasible);
}

#[test]
fn the_a400m_needs_an_engine_outside_the_window_and_is_infeasible_not_extrapolated() {
    let (config, design) = preset_clean_sheet("A400M");
    let a = assess(&config, &design);
    let validity = row(&a, "engine_scale_validity");
    assert!(validity.actual < 0.8, "{validity:?}");
    assert!(a.violated_hard_ids().contains(&"engine_scale_validity"));
}

#[test]
fn a_longer_stated_landing_field_length_is_ignored_with_a_notice_and_a_shorter_one_governs() {
    let landing = |stated_m: Option<f64>| {
        let (mut config, design) = preset_clean_sheet("A320-200");
        config.lda_override_m = stated_m;
        assess(&config, &design)
    };
    let route = landing(None);
    assert!(route
        .residuals
        .iter()
        .all(|r| r.id != "landing_field_override_ignored"));
    // The landing row is a wing-loading limit that fills the usable LDA share.
    let (reference, _) = preset_clean_sheet("A320-200");
    let arrival = reference.arrival_airport_record().unwrap();
    let lda = arrival.lda_m;
    let limit_at = |stated_lda_m: f64| {
        alas_perf::performance::ws_landing_limit_for(
            &reference.performance,
            false,
            stated_lda_m,
            alas_perf::performance::landing_runway_share(
                alas_perf::performance::JET_DRY_LANDING_SHARE,
                reference.performance.wet_landing_runway,
            ),
            alas_perf::performance::density_ratio(arrival.elevation_m, arrival.isa_deviation_c),
        )
    };
    let route_limit = row(&route, "landing_field").limit;

    let shorter = landing(Some(lda - 1_000.0));
    let shortest = landing(Some(lda - 1_500.0));
    // A shorter stated LDA strictly lowers the limit, and a shorter one lowers it more.
    assert_eq!(route_limit.to_bits(), limit_at(lda).to_bits());
    let limit_short = row(&shorter, "landing_field").limit;
    assert_eq!(limit_short.to_bits(), limit_at(lda - 1_000.0).to_bits());
    assert_eq!(
        row(&shortest, "landing_field").limit.to_bits(),
        limit_at(lda - 1_500.0).to_bits()
    );
    assert!(shorter
        .residuals
        .iter()
        .all(|r| r.id != "landing_field_override_ignored"));

    let longer = landing(Some(lda + 900.0));
    assert_eq!(
        row(&longer, "landing_field").limit.to_bits(),
        route_limit.to_bits()
    );
    let notice = row(&longer, "landing_field_override_ignored");
    assert_eq!(notice.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert_eq!((notice.actual, notice.limit), (lda + 900.0, lda));
}
