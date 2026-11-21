// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The design vector the evaluator builds: the nominal a run starts from,
//! the cabin-sized fuselage and the tail sizing of a candidate.

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::{build, wing_position};

/// Resolve a deterministic nominal vector for a product design space.
///
/// Clean-sheet passenger studies with cabin-derived fuselage length need the
/// same one-dimensional sizing solve before bounds are handed to the search;
/// otherwise the search would evaluate one body length and return another in
/// its best-design vector. Other modes return the supplied nominal intact.
///
/// A derived clean-sheet start places its wing on the estimated fuselage
/// (see [`alas_config::clean_sheet`]); once the cabin has sized the actual
/// body, that start's wing is placed on it at the same class station.
pub(crate) fn canonical_nominal_design(
    config: &AlasConfig,
    nominal: DesignVector,
) -> Result<DesignVector, String> {
    let mut design = canonicalize_design(config, nominal)?;
    let explicit_shift = config
        .optimizer
        .design_space
        .initial_design
        .contains_key("wing_x_shift_m");
    if config.derives_clean_sheet_start()
        && !explicit_shift
        && nominal.same_geometry(&config.configured_nominal_design())
    {
        if let Some(shift_m) = config.clean_sheet_wing_x_shift_m(&design) {
            // An explicit wing-shift bound still holds the re-placed wing.
            let bounds = config.optimizer.design_space.bounds.get("wing_x_shift_m");
            design.wing_x_shift_m = bounds.map_or(shift_m, |&(lower, upper)| {
                shift_m.clamp(lower, upper.max(lower))
            });
        }
    }
    Ok(config.band_design(design))
}

/// The vector a run that supplies no design starts from, materialized as the
/// evaluator builds it: [`AlasConfig::configured_nominal_design`] through
/// [`canonical_nominal_design`].
///
/// # Errors
///
/// When the cabin-derived fuselage cannot be sized.
pub fn configured_nominal_design(config: &AlasConfig) -> Result<DesignVector, String> {
    canonical_nominal_design(config, config.configured_nominal_design())
}

/// Materialize the design vector that the production evaluator actually
/// builds for a candidate.
///
/// In a clean-sheet passenger study the fuselage length is derived from the
/// cabin load case.  Keeping this operation at the public pipeline boundary
/// prevents a no-optimization run (or a downstream export) from publishing
/// the unsized default vector while the evaluator silently builds a different
/// fuselage.  Other design modes preserve the caller's vector verbatim.
pub fn canonicalize_design(
    config: &AlasConfig,
    mut design: DesignVector,
) -> Result<DesignVector, String> {
    if !config.optimizer.design_space.sizes_fuselage_from_cabin()
        || config.requirements.aircraft_type == "cargo"
    {
        return Ok(design);
    }
    let mut materialized = config.clone();
    crate::objective::apply_candidate_payload_load_case(&mut materialized, &design)
        .map_err(|error| format!("cabin load case cannot be materialized: {error}"))?;
    build::size_fuselage_from_cabin(&materialized, &mut design)
        .map_err(|failure| format!("fuselage cannot be sized from cabin: {failure}"))?;
    Ok(design)
}

/// The design vector and tail sizing the evaluator builds a candidate with,
/// without evaluating it: the candidate payload load case, the cabin-derived
/// fuselage unless `preserve_explicit_fuselage_length`, and a reference
/// adaptation's tail sizing, through the very function the evaluator's
/// geometry build calls. An external aerodynamic model can then draw the
/// tail the coupled assessment will size, mass and trim.
///
/// # Errors
///
/// The candidate failure reason when those steps reject the vector.
pub fn resolve_tail_sizing(
    config: &AlasConfig,
    design: &DesignVector,
    preserve_explicit_fuselage_length: bool,
) -> Result<(DesignVector, alas_config::TailSizing), String> {
    let x = design.to_array();
    // A redesigned registered aircraft is drawn with its wing where it
    // balances, as the evaluator builds it (`wing_position`).
    if let Some((balanced, resolved, _)) =
        wing_position::balanced(config, &x, preserve_explicit_fuselage_length)
            .map_err(|failure| failure.to_string())?
    {
        let sizing = alas_config::TailSizing::of(&balanced.geometry.empennage, &resolved);
        return Ok((resolved, sizing));
    }
    let (_, resolved, sizing) =
        build::resolve_before_build(config, &x, preserve_explicit_fuselage_length)
            .map_err(|failure| failure.to_string())?;
    Ok((resolved, sizing))
}
