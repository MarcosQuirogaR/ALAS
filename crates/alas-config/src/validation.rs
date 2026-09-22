// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/validation.py
// Reference: alas @ rust-port-baseline.

//! The checks a configuration has to pass that no single field can make.
//!
//! A field's own bounds keep it sensible on its own. What they cannot see is a
//! combination: every value can sit well inside its own range and still add up
//! to an aircraft that cruises past its structural dive speed, or a tailplane
//! whose tip is wider than its root. Each rule here reads several groups at
//! once, which is why it needs a whole [`AlasConfig`] and cannot live on a
//! field.
//!
//! What a rule produces is a path and a sentence. The path is what the
//! interface scrolls to and highlights, so a rule that fires correctly and
//! names the wrong field sends the user to edit something that was never the
//! problem; the sentence has to say which two values disagree and by how much,
//! because "invalid configuration" is not actionable.
//!
//! Severity is not decoration. An [`Severity::Error`] blocks the run and a
//! [`Severity::Warning`] only highlights, and the V-n rule uses both: past the
//! dive speed is the diagram's red zone, and between the cruise and dive
//! speeds is its caution band, which is a real design and not a good one.
//!
//! # Two differences from the reference
//!
//! Upstream wraps each rule in a `try/except` that swallows anything a rule
//! raises, because it runs on every keystroke of a debounced live preview and
//! a field caught mid-edit could break a unit conversion. A rule here reads
//! typed fields off a constructed configuration and has nothing to raise, so
//! there is no equivalent and none is needed.
//!
//! The cruise rule reproduces the reference's own two lines rather than
//! calling the V-n diagram builder, for the reason upstream gives: that
//! builder additionally needs a fully constructed aeroplane for its stall
//! terms, which is far too expensive to build on every validation tick. It
//! evaluates the atmosphere through the closed-form ISA where upstream uses
//! native aerodynamic model's fitted default; the two agree to about 1e-11 and every number
//! the rule prints is rounded to the nearest whole metre per second.

use serde::{Deserialize, Serialize};

use crate::{AlasConfig, PropulsionTechnology};

/// Sea-level density the equivalent airspeed is referred to, in kg/m^3.
///
/// The certification V-speeds are equivalent airspeeds, so the cruise point
/// has to be converted to one before it can be compared with them. This is the
/// standard sea-level value the conversion is defined against, held here
/// rather than read from the atmosphere model because it is the definition of
/// the airspeed reference and not a property of today's air.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// The margin CS-25.335(b) requires between the design cruise and dive speeds.
const DIVE_TO_CRUISE_SPEED_RATIO: f64 = 1.25;

/// Whether an issue blocks the run or only marks the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The configuration cannot be run as it stands.
    Error,
    /// The configuration is runnable, and something about it deserves a look.
    Warning,
}

/// One thing wrong with a configuration, and where to look for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Dotted path from the configuration root down to the offending field,
    /// such as `geometry.empennage.hstab_tip_chord_m`. The first segment names
    /// one of [`AlasConfig`]'s groups and the rest is a chain of field names.
    pub field_path: String,
    /// What is wrong, naming both values and their units.
    pub message: String,
    /// Whether this blocks the run.
    pub severity: Severity,
}

/// Every issue every rule finds, in the order the rules are registered.
///
/// All rules run: the first failure does not stop the rest, because a user
/// fixing one problem at a time and re-running is a worse experience than a
/// list, and because two issues are often one cause.
pub fn validate(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = cruise_point_inside_the_flight_envelope(config);
    issues.extend(atmosphere_domain_is_physical(config));
    issues.extend(fuel_properties_are_physical(config));
    issues.extend(landing_gear_inputs_are_physical(config));
    issues.extend(empennage_tapers_toward_its_tips(config));
    issues.extend(mses_timeouts_are_positive_and_finite(config));
    issues.extend(optimizer_tokens_are_supported(config));
    issues.extend(crate::optimizer::policy_review::policy_group_issues(config));
    issues.extend(active_mass_model_issues(config));
    issues.extend(passenger_and_mass_inputs_are_coherent(config));
    issues.extend(custom_geometry_is_physical(config));
    issues.extend(vlm_mesh_is_solvable(config));
    issues
}

/// Validate only the FLOPS nodes selected by the active mass architecture.
/// The legacy comparison path intentionally carries those nodes for saved-file
/// compatibility, but malformed inactive FLOPS/turboprop values must not
/// prevent a reference-compatibility comparison from running.
fn active_mass_model_issues(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mass = &config.mass_model;
    let mut issues = Vec::new();
    if !mass.architecture_is_coherent() {
        issues.push(ValidationIssue {
            field_path: "mass_model.mass_architecture".to_owned(),
            message: format!(
                "Mass architecture {:?} disagrees with one or more derived group selectors; reload or migrate the configuration before running.",
                mass.mass_architecture
            ),
            severity: Severity::Error,
        });
    }
    if !mass.mass_architecture.is_pure_flops() {
        return issues;
    }
    if let Err(error) = mass.flops_structure.validate() {
        issues.push(ValidationIssue {
            field_path: structure_validation_path(&error),
            message: error,
            severity: Severity::Error,
        });
    }
    if config.geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop {
        if let Err(field) = mass.flops_turboprop.validate() {
            issues.push(ValidationIssue {
                field_path: format!("mass_model.flops_turboprop.{field}"),
                message: format!(
                    "FLOPS turboprop input {field} is nonphysical or outside its supported range."
                ),
                severity: Severity::Error,
            });
        }
    }
    issues
}

fn structure_validation_path(error: &str) -> String {
    let field = error
        .strip_prefix("FLOPS ")
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("flops_structure");
    format!("mass_model.flops_structure.{field}")
}

/// Check the two cross-field contracts that are otherwise only enforced deep
/// in a mass evaluation: a passenger cabin must resolve to the three counts
/// FLOPS consumes, and structural `DG`/`WLDG` overrides must describe one
/// physically ordered fixed-aircraft basis. Passenger count/share edits are
/// warnings when the canonical cabin resolver can repair them; this keeps the
/// live validator from rejecting a valid preset before the first analysis
/// pass has materialized its cabin. Invalid modes and weights remain blocking
/// errors because no downstream caller can resolve them safely.
fn passenger_and_mass_inputs_are_coherent(config: &AlasConfig) -> Vec<ValidationIssue> {
    if !config.mass_model.mass_architecture.is_pure_flops() {
        return Vec::new();
    }
    let mut issues = sizing_basis_override_issues(config);

    let cabin = &config.cabin.passenger;
    if config.requirements.aircraft_type != "passenger" {
        for (field_path, value) in [
            (
                "mass_model.flops_transport.first_class_passenger_count",
                config
                    .mass_model
                    .flops_transport
                    .first_class_passenger_count,
            ),
            (
                "mass_model.flops_transport.business_class_passenger_count",
                config
                    .mass_model
                    .flops_transport
                    .business_class_passenger_count,
            ),
            (
                "mass_model.flops_transport.tourist_class_passenger_count",
                config
                    .mass_model
                    .flops_transport
                    .tourist_class_passenger_count,
            ),
        ] {
            if value.is_none() {
                issues.push(ValidationIssue {
                    field_path: field_path.to_owned(),
                    message: "FLOPS requires all three passenger class counts for a cargo mass evaluation; enter 0 explicitly for an absent class or select a passenger cabin preset.".to_owned(),
                    severity: Severity::Error,
                });
            }
        }
        return issues;
    }

    if cabin.class_mix_mode != "count" && cabin.class_mix_mode != "percent" {
        issues.push(ValidationIssue {
            field_path: "cabin.passenger.class_mix_mode".to_owned(),
            message: format!(
                "Passenger class mix mode {:?} is unsupported; choose 'count' or 'percent'.",
                cabin.class_mix_mode
            ),
            severity: Severity::Error,
        });
        return issues;
    }

    for (field_path, count) in [
        ("cabin.passenger.first.count", cabin.first.count),
        ("cabin.passenger.business.count", cabin.business.count),
        ("cabin.passenger.premium.count", cabin.premium.count),
        ("cabin.passenger.economy.count", cabin.economy.count),
    ] {
        if count < 0 {
            issues.push(ValidationIssue {
                field_path: field_path.to_owned(),
                message: format!(
                    "Passenger seat count {count} must be nonnegative; clear the stale serialized count or use count mode with a deliberate installed cabin."
                ),
                severity: Severity::Error,
            });
        }
    }

    for (field_path, share) in [
        ("cabin.passenger.first.share_pct", cabin.first.share_pct),
        (
            "cabin.passenger.business.share_pct",
            cabin.business.share_pct,
        ),
        ("cabin.passenger.economy.share_pct", cabin.economy.share_pct),
    ] {
        if !share.is_finite() || !(0.0..=100.0).contains(&share) {
            issues.push(ValidationIssue {
                field_path: field_path.to_owned(),
                message: format!(
                    "Passenger class share {share:?} must be finite and within [0, 100] percent."
                ),
                severity: Severity::Error,
            });
        }
    }

    let resolved = cabin.resolved_flops_counts(config.requirements.num_passengers);
    let expected = [resolved.first, resolved.business, resolved.tourist];
    let report_split_mismatch = cabin.class_mix_mode == "count" || cabin.total_seats() > 0;
    for (field_path, declared, expected_count, label) in [
        (
            "mass_model.flops_transport.first_class_passenger_count",
            config
                .mass_model
                .flops_transport
                .first_class_passenger_count,
            expected[0],
            "first",
        ),
        (
            "mass_model.flops_transport.business_class_passenger_count",
            config
                .mass_model
                .flops_transport
                .business_class_passenger_count,
            expected[1],
            "business",
        ),
        (
            "mass_model.flops_transport.tourist_class_passenger_count",
            config
                .mass_model
                .flops_transport
                .tourist_class_passenger_count,
            expected[2],
            "tourist/economy",
        ),
    ] {
        let declared_i64 = declared.and_then(|value| i64::try_from(value).ok());
        if declared_i64 != Some(expected_count) && (declared.is_none() || report_split_mismatch) {
            let actual = declared.map_or_else(|| "missing".to_owned(), |value| value.to_string());
            issues.push(ValidationIssue {
                field_path: field_path.to_owned(),
                message: format!(
                    "FLOPS {label} passenger count is {actual}; the canonical cabin resolver will use {expected_count} for this case. The value is normalizable before the mass pass."
                ),
                severity: Severity::Warning,
            });
        }
    }
    let resolved_total = resolved.total();
    if cabin.class_mix_mode == "count"
        && resolved.is_nonempty()
        && config.requirements.num_passengers != resolved_total
    {
        issues.push(ValidationIssue {
            field_path: "requirements.num_passengers".to_owned(),
            message: format!(
                "Passenger requirement is {}, while the declared count-mode cabin resolves to {resolved_total} installed seats; the mass pass will use the declared cabin total.",
                config.requirements.num_passengers
            ),
            severity: Severity::Warning,
        });
    }
    issues
}

fn sizing_basis_override_issues(config: &AlasConfig) -> Vec<ValidationIssue> {
    let structure = &config.mass_model.flops_structure;
    let mut issues = Vec::new();
    let mlw_fraction = config.mass_model.mlw_fraction_mtow;
    if !mlw_fraction.is_finite() || !(0.0 < mlw_fraction && mlw_fraction <= 1.0) {
        issues.push(ValidationIssue {
            field_path: "mass_model.mlw_fraction_mtow".to_owned(),
            message: format!(
                "Maximum landing-mass fraction ({mlw_fraction:?}) must be finite and within [0, 1] so the resolved WLDG cannot exceed DG."
            ),
            severity: Severity::Error,
        });
    }
    for (field_path, label, value) in [
        (
            "mass_model.flops_structure.design_gross_mass_kg",
            "DG",
            structure.design_gross_mass_kg,
        ),
        (
            "mass_model.flops_structure.design_landing_mass_kg",
            "WLDG",
            structure.design_landing_mass_kg,
        ),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value <= 0.0 {
                issues.push(ValidationIssue {
                    field_path: field_path.to_owned(),
                    message: format!(
                        "FLOPS {label} override ({value:?} kg) must be finite and greater than zero."
                    ),
                    severity: Severity::Error,
                });
            }
        }
    }
    match config.mass_sizing_basis() {
        crate::MassSizingBasis::FixedAircraft {
            design_gross_mass_kg: dg,
            design_landing_mass_kg: wldg,
        } if dg.is_finite() && wldg.is_finite() && wldg > dg => {
            issues.push(ValidationIssue {
                field_path: "mass_model.flops_structure.design_landing_mass_kg".to_owned(),
                message: format!(
                    "FLOPS WLDG ({wldg:.1} kg) cannot exceed DG ({dg:.1} kg, the resolved design gross mass); lower WLDG or raise the design gross mass."
                ),
                severity: Severity::Error,
            });
        }
        crate::MassSizingBasis::Coupled
            if structure.design_landing_mass_kg.is_some() => issues.push(ValidationIssue {
                field_path: "mass_model.flops_structure.design_landing_mass_kg".to_owned(),
                message: "FLOPS WLDG is inactive while clean-sheet sizing has no fixed DG override; leave it blank or declare DG to select a fixed-aircraft basis.".to_owned(),
                severity: Severity::Warning,
            }),
        _ => {}
    }
    issues
}

/// Keep user-defined geometry outside the undefined portions of the loft
/// model. The builder repeats its planform-dependent checks for the active
/// design vector; this boundary check catches malformed saved values before a
/// preview or run can consume them.
fn custom_geometry_is_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Err(error) = config.geometry.wing.validate_custom_sections() {
        issues.push(ValidationIssue {
            field_path: wing_section_path(&error),
            message: error.to_string(),
            severity: Severity::Error,
        });
    }
    if let Err(error) = config.geometry.fuselage.validate_custom_sections() {
        issues.push(ValidationIssue {
            field_path: fuselage_section_path(&error),
            message: error.to_string(),
            severity: Severity::Error,
        });
    }
    if let Err(error) = config.geometry.fuselage.validate_generated_sections() {
        issues.push(ValidationIssue {
            field_path: generated_fuselage_section_path(&error),
            message: error.to_string(),
            severity: Severity::Error,
        });
    }
    issues
}

fn wing_section_path(error: &crate::WingSectionError) -> String {
    let index = match error {
        crate::WingSectionError::NonFinite { index, .. }
        | crate::WingSectionError::NonPositiveChord { index, .. }
        | crate::WingSectionError::SpanOutOfRange { index, .. }
        | crate::WingSectionError::InvalidOrder { index, .. }
        | crate::WingSectionError::EmptyAirfoil(index) => Some(*index),
        crate::WingSectionError::DuplicatePlanformStation { .. }
        | crate::WingSectionError::NonMonotoneChord { .. } => None,
    };
    index.map_or_else(
        || "geometry.wing.custom_sections".to_owned(),
        |index| format!("geometry.wing.custom_sections[{index}]"),
    )
}

fn fuselage_section_path(error: &crate::FuselageSectionError) -> String {
    let index = match error {
        crate::FuselageSectionError::NonFinite { index, .. }
        | crate::FuselageSectionError::NonPositive { index, .. }
        | crate::FuselageSectionError::XOutOfRange { index, .. }
        | crate::FuselageSectionError::InvalidOrder { index, .. }
        | crate::FuselageSectionError::InvalidShape { index, .. } => Some(*index),
        crate::FuselageSectionError::DuplicateGeneratedStation { .. }
        | crate::FuselageSectionError::GeneratedSectionCount { .. } => None,
    };
    index.map_or_else(
        || "geometry.fuselage.custom_sections".to_owned(),
        |index| format!("geometry.fuselage.custom_sections[{index}]"),
    )
}

fn generated_fuselage_section_path(error: &crate::FuselageSectionError) -> String {
    let index = match error {
        crate::FuselageSectionError::NonFinite { index, .. }
        | crate::FuselageSectionError::NonPositive { index, .. }
        | crate::FuselageSectionError::XOutOfRange { index, .. }
        | crate::FuselageSectionError::InvalidShape { index, .. } => Some(*index),
        crate::FuselageSectionError::GeneratedSectionCount { .. }
        | crate::FuselageSectionError::InvalidOrder { .. }
        | crate::FuselageSectionError::DuplicateGeneratedStation { .. } => None,
    };
    index.map_or_else(
        || "geometry.fuselage.generated_sections".to_owned(),
        |index| format!("geometry.fuselage.generated_sections[{index}]"),
    )
}

/// Keep source-backed landing-gear dimensions and heterogeneous bogie lists
/// valid before any sizing or preview code consumes them. A reference
/// wheelbase is deliberately validated as metadata only: it cannot supply a
/// missing datum for the active model's absolute gear stations.
fn landing_gear_inputs_are_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    config
        .landing_gear
        .validation_errors()
        .into_iter()
        .map(|(field_path, message)| ValidationIssue {
            field_path,
            message,
            severity: Severity::Error,
        })
        .collect()
}

/// Fuel volume can only become a meaningful mass capacity when its conversion
/// inputs are finite and physical. Keep these invariants at the configuration
/// boundary so every downstream capacity consumer receives the same contract.
fn fuel_properties_are_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mass = &config.mass_model;
    let mut issues = Vec::new();

    if !mass.fuel_density_kg_m3.is_finite() || mass.fuel_density_kg_m3 <= 0.0 {
        issues.push(ValidationIssue {
            field_path: "mass_model.fuel_density_kg_m3".to_owned(),
            message: format!(
                "Fuel density ({:?} kg/m^3) must be finite and greater than zero.",
                mass.fuel_density_kg_m3
            ),
            severity: Severity::Error,
        });
    }

    if !mass.fuel_tank_usable_fraction.is_finite()
        || !(0.0..=1.0).contains(&mass.fuel_tank_usable_fraction)
    {
        issues.push(ValidationIssue {
            field_path: "mass_model.fuel_tank_usable_fraction".to_owned(),
            message: format!(
                "Usable fuel-tank volume fraction ({:?}) must be finite and within [0, 1].",
                mass.fuel_tank_usable_fraction
            ),
            severity: Severity::Error,
        });
    }

    issues
}

/// Reject finite-but-out-of-model cruise altitudes before any Mach, Reynolds,
/// or force calculation can consume a NaN atmosphere state.
fn atmosphere_domain_is_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    match alas_atmo::Atmosphere::try_new(config.requirements.cruise_altitude_m) {
        Ok(_) => Vec::new(),
        Err(error) => vec![ValidationIssue {
            field_path: "requirements.cruise_altitude_m".to_owned(),
            message: format!("Cruise atmosphere is outside its physical model domain: {error}"),
            severity: Severity::Error,
        }],
    }
}

/// Optimizer names are serialized strings for compatibility with the settings
/// file, but dispatch only has a finite set of implementations. Reject a typo
/// at the configuration boundary instead of silently running a different
/// algorithm while retaining the requested (and therefore misleading) token.
fn optimizer_tokens_are_supported(config: &AlasConfig) -> Vec<ValidationIssue> {
    let solver = &config.optimizer.solver;
    let mut issues = Vec::new();
    if !crate::SolverSettings::is_supported_method(&solver.method) {
        issues.push(ValidationIssue {
            field_path: "optimizer.solver.method".to_owned(),
            message: format!(
                "Unknown optimizer method {:?}; choose one of the registered optimizer methods.",
                solver.method
            ),
            severity: Severity::Error,
        });
    }
    if !crate::SolverSettings::is_supported_strategy(&solver.strategy) {
        issues.push(ValidationIssue {
            field_path: "optimizer.solver.strategy".to_owned(),
            message: format!(
                "Unknown differential-evolution strategy {:?}; choose one of the registered strategies.",
                solver.strategy
            ),
            severity: Severity::Error,
        });
    }
    issues
}

/// MSES uses these values to construct process deadlines. Rejecting invalid
/// values here keeps a malformed configuration from reaching
/// `Duration::try_from_secs_f64` (or a worker thread) and makes the error
/// visible at every run boundary, including when MSES is currently optional.
fn mses_timeouts_are_positive_and_finite(config: &AlasConfig) -> Vec<ValidationIssue> {
    [
        ("mses.timeout_mset_s", config.mses.timeout_mset_s, "MSET"),
        ("mses.timeout_mses_s", config.mses.timeout_mses_s, "MSES"),
    ]
    .into_iter()
    .filter(|(_, seconds, _)| !seconds.is_finite() || *seconds <= 0.0)
    .map(|(field_path, seconds, tool)| ValidationIssue {
        field_path: field_path.to_owned(),
        message: format!("{tool} timeout ({seconds:?} s) must be finite and greater than zero.",),
        severity: Severity::Error,
    })
    .collect()
}

/// The cruise design point must sit inside the V-n envelope.
///
/// The design dive speed is an equivalent airspeed, as every CS-25 V-speed is,
/// so the cruise point is converted to one through the square root of the
/// density ratio before the two are compared. Comparing a true airspeed
/// against an equivalent one instead rejects designs that are perfectly fine,
/// by a factor that grows with altitude.
fn cruise_point_inside_the_flight_envelope(config: &AlasConfig) -> Vec<ValidationIssue> {
    let requirements = &config.requirements;
    let atmosphere = alas_atmo::Atmosphere::new(requirements.cruise_altitude_m);
    let cruise_eas_m_s = requirements.cruise_mach
        * atmosphere.speed_of_sound()
        * (atmosphere.density() / SEA_LEVEL_DENSITY_KG_M3).sqrt();

    let dive_speed = requirements.dive_speed_m_s;
    let design_cruise_speed = dive_speed / DIVE_TO_CRUISE_SPEED_RATIO;

    if cruise_eas_m_s > dive_speed {
        return vec![ValidationIssue {
            field_path: "requirements.dive_speed_m_s".to_owned(),
            message: format!(
                "Cruise design point ({cruise_eas_m_s:.0} m/s EAS at Mach \
                 {:.2} / {} m) exceeds the design dive speed VD \
                 ({dive_speed:.0} m/s EAS): the aircraft would cruise \
                 outside its own structural flight envelope.",
                requirements.cruise_mach,
                grouped(requirements.cruise_altitude_m),
            ),
            severity: Severity::Error,
        }];
    }
    if cruise_eas_m_s > design_cruise_speed {
        return vec![ValidationIssue {
            field_path: "requirements.dive_speed_m_s".to_owned(),
            message: format!(
                "Cruise design point ({cruise_eas_m_s:.0} m/s EAS) is above VC \
                 ({design_cruise_speed:.0} m/s EAS = VD/1.25): the aircraft \
                 cruises in the V-n diagram's caution band, not normal \
                 operation."
            ),
            severity: Severity::Warning,
        }];
    }
    Vec::new()
}

/// Both stabilizers must be narrower at the tip than at the root.
///
/// The canonical "is this a shape at all" check. An inverted taper is almost
/// always a transposed pair of numbers, and it produces a surface the geometry
/// builder will happily loft and every downstream area and volume coefficient
/// will then be computed from.
fn empennage_tapers_toward_its_tips(config: &AlasConfig) -> Vec<ValidationIssue> {
    let empennage = &config.geometry.empennage;
    let mut issues = Vec::new();

    if empennage.hstab_tip_chord_m >= empennage.hstab_root_chord_m {
        issues.push(ValidationIssue {
            field_path: "geometry.empennage.hstab_tip_chord_m".to_owned(),
            message: format!(
                "H-stab tip chord ({:.2} m) must be smaller than its root \
                 chord ({:.2} m).",
                empennage.hstab_tip_chord_m, empennage.hstab_root_chord_m
            ),
            severity: Severity::Error,
        });
    }
    if empennage.vstab_tip_chord_m >= empennage.vstab_root_chord_m {
        issues.push(ValidationIssue {
            field_path: "geometry.empennage.vstab_tip_chord_m".to_owned(),
            message: format!(
                "V-stab tip chord ({:.2} m) must be smaller than its root \
                 chord ({:.2} m).",
                empennage.vstab_tip_chord_m, empennage.vstab_root_chord_m
            ),
            severity: Severity::Error,
        });
    }
    issues
}

/// The largest spanwise subdivision multiplier that still produces a usable
/// lattice. See [`vlm_mesh_is_solvable`] for the measurement behind it.
const MAX_SPANWISE_RESOLUTION: i64 = 2;

/// The fewest spanwise panels a surface can carry and still be a lifting
/// surface rather than a placeholder.
///
/// `geometry.wing.n_subdivisions` and its empennage twin are absolute panel
/// counts across a whole surface. Four is already far below anything usable:
/// the shipped wing uses 24, so this rejects nonsense rather than
/// arbitrating fidelity, which is what the convergence evidence in
/// `alas_config::analysis` is for.
const MIN_SPANWISE_PANELS: i64 = 4;

/// Keep the vortex-lattice mesh inside the range where its own induced drag
/// converges.
///
/// `spanwise_resolution` is a subdivision *multiplier*, and the geometry
/// builder has already subdivided every surface (`geometry.wing`/
/// `geometry.empennage.n_subdivisions`). Multiplying that again re-applies a
/// cosine spacing inside each existing strip, which leaves a two- to
/// three-fold width discontinuity at every original station and panels thin
/// enough that the near-field induced-drag integration stops converging.
///
/// Measured on the registered presets in an internal VLM
/// resolution-sensitivity study (2026-09-11): at a multiplier of 3 the
/// swept presets over-predict the induced-drag factor by 4-50 %, at 6 the
/// A320 trim solve diverges outright, and at 10 (AeroSandbox's own default,
/// and so a value a user may reasonably type) the influence matrix is
/// effectively singular while the solve still reports success, returning
/// L/D near 1 instead of 18. That last case is the reason this is an error
/// and not a warning: nothing downstream can detect it.
///
/// A multiplier of 2 is exactly a uniform halving (`cosspace` with three
/// points is `linspace`), so it introduces no discontinuity and stays
/// allowed. Refining the span further is a matter for `n_subdivisions`,
/// which distributes stations uniformly across the whole surface.
fn vlm_mesh_is_solvable(config: &AlasConfig) -> Vec<ValidationIssue> {
    let analysis = &config.analysis;
    let mut issues = Vec::new();

    for (field, value) in [
        ("spanwise_resolution", analysis.spanwise_resolution),
        (
            "fine_spanwise_resolution",
            analysis.fine_spanwise_resolution,
        ),
    ] {
        if value > MAX_SPANWISE_RESOLUTION {
            issues.push(ValidationIssue {
                field_path: format!("analysis.{field}"),
                message: format!(
                    "Spanwise panel resolution ({value}) must not exceed \
                     {MAX_SPANWISE_RESOLUTION}. It multiplies a surface the \
                     geometry builder has already subdivided, and above {} \
                     the vortex lattice stops converging: the induced drag \
                     grows without bound and the solver can return a \
                     converged-looking answer that is not physical. Refine \
                     the span with geometry.wing.n_subdivisions instead.",
                    MAX_SPANWISE_RESOLUTION
                ),
                severity: Severity::Error,
            });
        }
    }

    for (field, value) in [
        ("spanwise_resolution", analysis.spanwise_resolution),
        ("chordwise_resolution", analysis.chordwise_resolution),
        (
            "fine_spanwise_resolution",
            analysis.fine_spanwise_resolution,
        ),
        (
            "fine_chordwise_resolution",
            analysis.fine_chordwise_resolution,
        ),
    ] {
        if value < 1 {
            issues.push(ValidationIssue {
                field_path: format!("analysis.{field}"),
                message: format!(
                    "Panel resolution ({value}) must be at least 1; a mesh \
                     cannot have fewer than one panel per strip."
                ),
                severity: Severity::Error,
            });
        }
    }

    // The geometry panel counts are absolute counts across a surface, and
    // `Wing::mesh_spanwise` answers a count too small to honour with one
    // panel per section rather than by dropping a station. That is right for
    // library code and the wrong thing to discover from a result, so an
    // unusable count is reported here instead.
    for (field, value) in [
        (
            "geometry.wing.n_subdivisions",
            config.geometry.wing.n_subdivisions,
        ),
        (
            "geometry.empennage.n_subdivisions",
            config.geometry.empennage.n_subdivisions,
        ),
    ] {
        if value < MIN_SPANWISE_PANELS {
            issues.push(ValidationIssue {
                field_path: field.to_owned(),
                message: format!(
                    "Spanwise panel count ({value}) must be at least                      {MIN_SPANWISE_PANELS}. This is an absolute number of                      panels across the surface, not a count per section;                      below it the lattice cannot resolve a lifting surface."
                ),
                severity: Severity::Error,
            });
        }
    }

    issues
}

/// A whole number with thousands separators, as Python's `,.0f` renders it.
///
/// An altitude in metres is five digits, and five unbroken digits in the
/// middle of a sentence is the kind of number a reader mis-reads by a factor
/// of ten. The grouping is part of the message the reference emits, so it is
/// reproduced rather than left to the formatter.
fn grouped(value: f64) -> String {
    let rounded = format!("{value:.0}");
    let (sign, digits) = match rounded.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", rounded.as_str()),
    };

    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped}")
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_configuration_is_one_nothing_objects_to() {
        assert!(validate(&AlasConfig::default()).is_empty());
    }

    #[test]
    fn stale_percent_mode_floops_counts_are_reported_as_normalizable_warnings() {
        let mut config = AlasConfig::default();
        config.requirements.num_passengers = 100;
        config.cabin.passenger.first.share_pct = 10.0;
        config.cabin.passenger.business.share_pct = 20.0;
        config.cabin.passenger.economy.share_pct = 70.0;
        config.cabin.passenger.first.count = 10;
        config.cabin.passenger.business.count = 20;
        config.cabin.passenger.economy.count = 70;
        config
            .mass_model
            .flops_transport
            .first_class_passenger_count = Some(0);
        config
            .mass_model
            .flops_transport
            .business_class_passenger_count = Some(0);
        config
            .mass_model
            .flops_transport
            .tourist_class_passenger_count = Some(350);

        let issues = validate(&config);
        assert!(issues
            .iter()
            .all(|issue| issue.severity == Severity::Warning));
        assert!(issues.iter().any(|issue| {
            issue.field_path == "mass_model.flops_transport.first_class_passenger_count"
                && issue.message.contains("will use 10")
        }));
        assert!(issues.iter().any(|issue| {
            issue.field_path == "mass_model.flops_transport.tourist_class_passenger_count"
                && issue.message.contains("will use 70")
        }));
    }

    #[test]
    fn missing_passenger_class_counts_are_actionable_without_blocking_normalization() {
        let mut config = AlasConfig::default();
        config
            .mass_model
            .flops_transport
            .first_class_passenger_count = None;
        config
            .mass_model
            .flops_transport
            .business_class_passenger_count = None;
        config
            .mass_model
            .flops_transport
            .tourist_class_passenger_count = None;

        let issues = validate(&config);
        assert_eq!(issues.len(), 3);
        assert!(issues
            .iter()
            .all(|issue| issue.severity == Severity::Warning));
        assert!(issues.iter().all(|issue| {
            issue.field_path.starts_with("mass_model.flops_transport.")
                && issue.message.contains("canonical cabin resolver")
        }));
    }

    #[test]
    fn negative_declared_cabin_counts_are_blocking_errors() {
        let mut config = AlasConfig::default();
        config.cabin.passenger.class_mix_mode = "count".to_owned();
        config.cabin.passenger.business.count = -1;

        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "cabin.passenger.business.count"
                && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn landing_mass_override_must_have_a_gross_basis_and_stay_below_it() {
        let mut config = AlasConfig::default();
        config.mass_model.flops_structure.design_landing_mass_kg = Some(80_000.0);
        let missing_dg = validate(&config);
        assert!(missing_dg.iter().any(|issue| {
            issue.field_path == "mass_model.flops_structure.design_landing_mass_kg"
                && issue.severity == Severity::Warning
                && issue.message.contains("inactive")
        }));

        config.mass_model.flops_structure.design_gross_mass_kg = Some(70_000.0);
        let inverted = validate(&config);
        assert!(inverted.iter().any(|issue| {
            issue.field_path == "mass_model.flops_structure.design_landing_mass_kg"
                && issue.severity == Severity::Error
                && issue.message.contains("cannot exceed DG")
        }));
    }

    #[test]
    fn landing_mass_override_without_dg_is_valid_when_the_fixed_mode_resolves_dg() {
        let mut config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200"
        }))
        .unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        config.mass_model.flops_structure.design_landing_mass_kg = Some(60_000.0);

        assert!(!validate(&config).iter().any(|issue| {
            issue.field_path == "mass_model.flops_structure.design_landing_mass_kg"
                && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn resolved_reference_landing_mass_cannot_exceed_a_reduced_fixed_mtow() {
        let mut config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200"
        }))
        .unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        config.requirements.mtow_kg = 60_000.0;

        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "mass_model.flops_structure.design_landing_mass_kg"
                && issue.severity == Severity::Error
                && issue.message.contains("cannot exceed DG")
        }));
    }

    #[test]
    fn every_registered_aircraft_passes_its_own_validation() {
        // A preset that does not validate is a starting point the interface
        // offers and then refuses to run.
        for preset in crate::presets::registry() {
            let config = AlasConfig {
                geometry: preset.geometry.clone(),
                requirements: preset.requirements.clone(),
                ..Default::default()
            };
            assert_eq!(validate(&config), Vec::new(), "{}", preset.name);
        }
    }

    #[test]
    fn a_spanwise_resolution_past_the_convergent_range_blocks_the_run() {
        // 10 is AeroSandbox's own constructor default, so it is the value a
        // user is most likely to reach for, and it is the one that returns a
        // converged-looking non-physical solution rather than an error.
        for field in ["spanwise_resolution", "fine_spanwise_resolution"] {
            let mut config = AlasConfig::default();
            if field == "spanwise_resolution" {
                config.analysis.spanwise_resolution = 10;
                config.analysis.fine_spanwise_resolution = 10;
            } else {
                config.analysis.fine_spanwise_resolution = 10;
            }
            let issues = validate(&config);
            assert!(
                issues.iter().any(|issue| {
                    issue.field_path == format!("analysis.{field}")
                        && issue.severity == Severity::Error
                }),
                "{field}: {issues:?}"
            );
        }
    }

    #[test]
    fn a_spanwise_resolution_of_two_is_still_allowed() {
        // At a multiplier of two the cosine subdivision degenerates to a
        // uniform halving, so it introduces none of the width discontinuity
        // the rule exists to catch.
        let mut config = AlasConfig::default();
        config.analysis.spanwise_resolution = 2;
        config.analysis.fine_spanwise_resolution = 2;
        assert_eq!(validate(&config), Vec::new());
    }

    #[test]
    fn a_resolution_below_one_is_not_a_mesh() {
        let mut config = AlasConfig::default();
        config.analysis.chordwise_resolution = 0;
        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "analysis.chordwise_resolution" && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn malformed_heterogeneous_bogies_are_blocking_errors() {
        let mut config = AlasConfig::default();
        config.landing_gear.n_mlg_struts = 3;
        config.landing_gear.mlg_strut_bogie_wheels = Some(vec![4, 4]);
        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "landing_gear.mlg_strut_bogie_wheels"
                && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn cruising_past_the_dive_speed_blocks_the_run() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 100.0;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].field_path, "requirements.dive_speed_m_s");
    }

    #[test]
    fn cruising_in_the_caution_band_warns_without_blocking() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 130.0;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warning);
    }

    #[test]
    fn the_cruise_point_is_compared_as_an_equivalent_airspeed() {
        // The true airspeed at the default cruise point is about twice the
        // equivalent one, so a rule comparing the wrong reference would reject
        // the shipped configuration outright.
        let config = AlasConfig::default();
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let true_airspeed = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        assert!(true_airspeed > config.requirements.dive_speed_m_s);
        assert!(validate(&config).is_empty());
    }

    #[test]
    fn an_untapered_surface_is_rejected_as_well_as_an_inverted_one() {
        // The comparison is >= and not >, so equal chords fail too.
        let mut config = AlasConfig::default();
        config.geometry.empennage.hstab_tip_chord_m = config.geometry.empennage.hstab_root_chord_m;
        let issues = validate(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_path, "geometry.empennage.hstab_tip_chord_m");
    }

    #[test]
    fn every_rule_runs_rather_than_the_first_failure_stopping_the_rest() {
        let mut config = AlasConfig::default();
        config.requirements.dive_speed_m_s = 100.0;
        config.geometry.empennage.vstab_tip_chord_m = 10.0;
        assert_eq!(validate(&config).len(), 2);
    }

    #[test]
    fn invalid_mses_timeouts_are_blocking_configuration_errors() {
        let mut config = AlasConfig::default();
        config.mses.timeout_mset_s = 0.0;
        config.mses.timeout_mses_s = f64::NAN;
        let issues = validate(&config);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|issue| issue.severity == Severity::Error));
        assert!(issues
            .iter()
            .any(|issue| issue.field_path == "mses.timeout_mset_s"));
        assert!(issues
            .iter()
            .any(|issue| issue.field_path == "mses.timeout_mses_s"));
    }

    #[test]
    fn nonpositive_or_nonfinite_fuel_density_is_blocked() {
        for density_kg_m3 in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut config = AlasConfig::default();
            config.mass_model.fuel_density_kg_m3 = density_kg_m3;
            let issues = validate(&config);
            assert!(issues.iter().any(|issue| {
                issue.field_path == "mass_model.fuel_density_kg_m3"
                    && issue.severity == Severity::Error
            }));
        }
    }

    #[test]
    fn out_of_range_or_nonfinite_usable_tank_fraction_is_blocked() {
        for usable_fraction in [-0.01, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut config = AlasConfig::default();
            config.mass_model.fuel_tank_usable_fraction = usable_fraction;
            let issues = validate(&config);
            assert!(issues.iter().any(|issue| {
                issue.field_path == "mass_model.fuel_tank_usable_fraction"
                    && issue.severity == Severity::Error
            }));
        }
    }

    #[test]
    fn usable_tank_fraction_accepts_its_closed_interval_boundaries() {
        for usable_fraction in [0.0, 1.0] {
            let mut config = AlasConfig::default();
            config.mass_model.fuel_tank_usable_fraction = usable_fraction;
            assert!(!validate(&config)
                .iter()
                .any(|issue| { issue.field_path == "mass_model.fuel_tank_usable_fraction" }));
        }
    }

    #[test]
    fn nonphysical_cruise_altitude_is_a_blocking_atmosphere_error() {
        let mut config = AlasConfig::default();
        config.requirements.cruise_altitude_m = f64::NAN;
        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "requirements.cruise_altitude_m"
                && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn finite_cruise_altitudes_outside_the_atmosphere_table_are_blocked() {
        for altitude_m in [-2_001.0, 84_853.0] {
            let mut config = AlasConfig::default();
            config.requirements.cruise_altitude_m = altitude_m;
            let issues = validate(&config);
            assert!(
                issues.iter().any(|issue| {
                    issue.field_path == "requirements.cruise_altitude_m"
                        && issue.severity == Severity::Error
                }),
                "altitude {altitude_m} m must be rejected: {issues:?}"
            );
        }
    }

    #[test]
    fn unknown_optimizer_tokens_are_blocking_configuration_errors() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.method = "differential_evoluton".to_owned();
        config.optimizer.solver.strategy = "best1bni".to_owned();
        let issues = validate(&config);
        assert!(issues.iter().any(|issue| {
            issue.field_path == "optimizer.solver.method" && issue.severity == Severity::Error
        }));
        assert!(issues.iter().any(|issue| {
            issue.field_path == "optimizer.solver.strategy" && issue.severity == Severity::Error
        }));
    }

    #[test]
    fn a_severity_is_written_the_way_the_interface_reads_it() {
        assert_eq!(
            serde_json::to_value(Severity::Warning).unwrap(),
            serde_json::json!("warning")
        );
    }

    #[test]
    fn a_five_digit_altitude_is_grouped_the_way_the_message_expects() {
        assert_eq!(grouped(11_887.2), "11,887");
        assert_eq!(grouped(0.0), "0");
        assert_eq!(grouped(999.0), "999");
        assert_eq!(grouped(1_000.0), "1,000");
        assert_eq!(grouped(1_234_567.0), "1,234,567");
        assert_eq!(grouped(-2_500.0), "-2,500");
    }
}
