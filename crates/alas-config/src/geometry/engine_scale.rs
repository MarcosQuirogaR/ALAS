// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rubber-engine scaling: the selected catalogue engine at another rating.
//!
//! A clean-sheet design does not own an engine; it owns a thrust requirement.
//! The catalogue row named by [`EngineConfig::engine_name`] is therefore the
//! *baseline* (cycle, technology level, off-design shape), and the sizing
//! step asks for the same engine scaled by a factor `x = rating / baseline
//! rating`. Everything an engine carries is scaled once, here, so that every
//! discipline reads one consistent derivative (see the module documentation
//! of [`super::engine`] for why the configuration holds copies, not a name).
//!
//! # Scaling laws and where they are sourced
//!
//! * **Dimensions**: Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed.,
//!   Sec. 10.3, p. 283, Eqs. (10.1) and (10.2): `L = L_actual x^0.4` and
//!   `D = D_actual x^0.5` for the "typical jet engine" with `x` the ratio of
//!   required to actual thrust (verified on the page). Engine length scales
//!   the nacelle stations, diameter the nacelle radius and the fan diameter
//!   (thrust is proportional to the air mass flow, hence to the frontal area).
//!   Jenkinson, *Civil Jet Aircraft Design*, Sec. 9.10 "Rules for scaling an
//!   engine" gives geometric similarity, dimensions `x^0.5`; the two differ
//!   by under 2 percent at the validity limit below.
//! * **Fuel flow and thrust-specific fuel consumption**: Jenkinson Sec. 9.10:
//!   "fuel flow scales by the same ratio (i.e. SFC remains constant)". Raymer's
//!   cruise TSFC correlation, Eq. (10.9), has no thrust term either. The ICAO
//!   take-off fuel flow and the cruise reference thrust scale with `x`; every
//!   specific quantity (TSFC, part-power fuel-flow ratios, bypass ratio,
//!   pressure ratios, temperatures) is the baseline's. A real engine at a
//!   different size also shifts its component efficiencies (Reynolds number,
//!   tip clearances); that second-order change is not modelled and is the
//!   reason for the validity window.
//! * **Mass**: not scaled here. The FLOPS propulsion group already scales the
//!   engine mass from the rated thrust ratio with `(T/T0)^1.15` (NASA
//!   TM-2017-219627 Eq. 75), thrust reversers, controls, starters and pylon
//!   from the rating (Eqs. 86-89, LTH pylon relation) and the nacelle from the
//!   built nacelle area; Raymer's `W = W_actual x^1.1` (Eq. 10.3) corroborates
//!   the exponent. [`AlasConfig::with_engine_scaled`] pins the baseline rating
//!   those relations scale from.
//! * **Turboprop**: shaft power ratings and the fuel flow scale with `x`
//!   (constant power-specific fuel consumption: the same similarity argument
//!   as Jenkinson's jet rule; no turboprop-specific source for the size
//!   dependence was located, so this is an assumption, not a datum); the
//!   propeller diameter scales with `x^0.25`, Raymer Eq. (10.23)
//!   `D = K_p P^(1/4)` (p. 311, verified on the page); the governed propeller
//!   speed with `x^-0.25`, which holds the blade tip speed (Raymer
//!   Eqs. 10.21-10.22 limit it) and the advance ratio at the same flight
//!   speed (an engineering choice); the reduction ratio follows the propeller
//!   speed at constant power-turbine speed (the unit model does not read it).
//!   The propeller efficiency map is the unit model's own and responds to the
//!   changed power coefficient.
//!
//! # Validity
//!
//! Jenkinson Sec. 9.10: "It would be impractical and inaccurate to scale an
//! engine configuration from its baseline to 0.5 thrust scale ... A crude rule
//! of thumb suggests that thrust scaling should be limited to 20%."
//! [`ENGINE_SCALE_VALIDITY_FRACTION`] is that rule; a scale outside
//! `1 +- 0.20` of the catalogue rating is flagged by the optimizer as an
//! infeasible engine, not extrapolated silently.

use super::*;
use crate::AlasConfig;

/// Raymer Eq. (10.1): engine length exponent on the thrust scale factor.
pub const ENGINE_LENGTH_EXPONENT: f64 = 0.4;
/// Raymer Eq. (10.2): engine (and fan) diameter exponent on the scale factor.
pub const ENGINE_DIAMETER_EXPONENT: f64 = 0.5;
/// Raymer Eq. (10.23): propeller diameter exponent on the shaft power.
pub const PROPELLER_DIAMETER_POWER_EXPONENT: f64 = 0.25;
/// Jenkinson Sec. 9.10: the largest departure of the scale factor from one
/// that a baseline engine may be scaled by.
pub const ENGINE_SCALE_VALIDITY_FRACTION: f64 = 0.20;
/// Numerical bracket of the sizing search on the scale factor relative to
/// the engine as configured. Far wider than the validity window so that the
/// infeasibility of a brief that needs another engine class is measured
/// (and reported), not clamped away; not a physical statement.
pub const ENGINE_SCALE_SEARCH_BRACKET: (f64, f64) = (0.4, 2.5);

/// Why an engine could not be scaled.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineScaleError {
    /// The factor is not finite and positive.
    #[error("engine scale factor {0} must be finite and positive")]
    Factor(f64),
    /// The selection does not resolve to a coherent physics binding.
    #[error(transparent)]
    Binding(#[from] EngineBindingError),
}

fn scale_profile(profile: &mut [(f64, f64)], length_factor: f64) {
    for (station, _) in profile.iter_mut() {
        *station *= length_factor;
    }
}

impl EngineConfig {
    /// The rating the scale factor refers to: sea-level-static thrust, kN, of
    /// a turbofan, take-off shaft power, kW, of a turboprop.
    pub fn scaling_rating(&self) -> Option<f64> {
        match self.active_model().ok()? {
            ActiveEngineModel::Turbofan(spec) => Some(spec.rated_thrust_kn),
            ActiveEngineModel::Turboprop(spec) => Some(spec.takeoff_shaft_power_kw),
        }
    }

    /// The same rating of the catalogue row [`Self::engine_name`] selects: the
    /// baseline the validity window is measured from.
    pub fn catalogue_scaling_rating(&self) -> Option<f64> {
        let spec = crate::engines::get(&self.engine_name).ok()?;
        match spec.technology {
            PropulsionTechnology::Turbofan => spec.turbofan_spec().map(|t| t.rated_thrust_kn),
            PropulsionTechnology::Turboprop => {
                spec.turboprop.as_ref().map(|t| t.takeoff_shaft_power_kw)
            }
        }
    }

    /// The scale of this engine against its catalogue baseline.
    pub fn catalogue_scale(&self) -> Option<f64> {
        let (rating, baseline) = (self.scaling_rating()?, self.catalogue_scaling_rating()?);
        (baseline > 0.0).then(|| rating / baseline)
    }

    /// Rated thrust per engine, kN, that the thrust-scaled mass relations
    /// read (FLOPS equations 75-78, 86-89, 121-122 and the simple installed
    /// propulsion estimate); `None` for a turboprop or an incoherent binding.
    ///
    /// One rule for every turbofan: the mass belongs to the hardware, so the
    /// mass relations price the catalogue engine at its catalogue mass basis
    /// ([`crate::engines::EngineSpec::mass_basis_thrust_kn`], the rating of
    /// the record itself unless the record declares that its rating is a
    /// re-rating of hardware listed under another), scaled by this
    /// installation's [`Self::catalogue_scale`]. A rubber-scaled engine
    /// still scales; a re-rating of the same hardware does not. Without a
    /// declared mass basis this is exactly the rated thrust.
    pub fn mass_basis_thrust_kn(&self) -> Option<f64> {
        let ActiveEngineModel::Turbofan(spec) = self.active_model().ok()? else {
            return None;
        };
        let basis = crate::engines::get(&self.engine_name)
            .ok()
            .and_then(|catalogue| catalogue.mass_basis_thrust_kn);
        match basis {
            Some(basis_kn) => Some(basis_kn * self.catalogue_scale()?),
            None => Some(spec.rated_thrust_kn),
        }
    }

    /// The scale factor `x` at which this installation is the catalogue
    /// nacelle outline of its selected engine under the rubber-engine laws
    /// (length `x^0.4`, radius `x^0.5`, outline fractions unchanged); one for
    /// the catalogue installation itself. `None` when the outline or radius
    /// were edited in any other way, or the engine has no catalogue outline.
    ///
    /// The G2 nacelle revolve is the catalogue shape at the installation's
    /// own length and radius, so it stays valid exactly for these.
    pub fn catalogue_outline_scale(&self) -> Option<f64> {
        let spec = crate::engines::get(&self.engine_name).ok()?;
        let catalogue = spec.nacelle_profile();
        let catalogue_length = catalogue.last()?.0;
        let catalogue_radius = spec.nacelle_max_radius_m;
        if !(catalogue_length > 0.0 && catalogue_radius > 0.0)
            || self.nacelle_profile.len() != catalogue.len()
        {
            return None;
        }
        let near = |a: f64, b: f64| (a - b).abs() <= 1.0e-9 * b.abs().max(1.0);
        let length_scale = self.nacelle_length_m() / catalogue_length;
        let radius_scale = self.radius_scale_m / catalogue_radius;
        for (&(station, fraction), &(base_station, base_fraction)) in
            self.nacelle_profile.iter().zip(&catalogue)
        {
            if !near(station, base_station * length_scale) || !near(fraction, base_fraction) {
                return None;
            }
        }
        let from_length = length_scale.powf(1.0 / ENGINE_LENGTH_EXPONENT);
        let from_radius = radius_scale.powf(1.0 / ENGINE_DIAMETER_EXPONENT);
        (from_length.is_finite() && near(from_length, from_radius)).then_some(from_radius)
    }

    /// This engine scaled by `factor` (new rating over the present one).
    ///
    /// A factor of exactly one returns an unchanged copy, bit for bit.
    ///
    /// # Errors
    ///
    /// A non-positive or non-finite factor, or an incoherent engine binding.
    pub fn scaled_by(&self, factor: f64) -> Result<Self, EngineScaleError> {
        if !(factor.is_finite() && factor > 0.0) {
            return Err(EngineScaleError::Factor(factor));
        }
        self.active_model()?;
        let mut scaled = self.clone();
        if factor == 1.0 {
            return Ok(scaled);
        }
        let length = factor.powf(ENGINE_LENGTH_EXPONENT);
        let diameter = factor.powf(ENGINE_DIAMETER_EXPONENT);
        if let Some(spec) = scaled.turbofan.as_mut() {
            spec.rated_thrust_kn *= factor;
            spec.off_design.cruise_reference_thrust_n *= factor;
            spec.takeoff_fuel_flow_kg_s *= factor;
        }
        if let Some(spec) = scaled.turboprop.as_mut() {
            spec.takeoff_shaft_power_kw *= factor;
            spec.maximum_reserve_shaft_power_kw *= factor;
            spec.maximum_continuous_shaft_power_kw *= factor;
            spec.maximum_climb_shaft_power_kw *= factor;
            spec.maximum_cruise_shaft_power_kw *= factor;
            spec.maximum_cruise_fuel_flow_kg_h *= factor;
            spec.thermodynamic_shaft_power_kw *= factor;
            let propeller = factor.powf(PROPELLER_DIAMETER_POWER_EXPONENT);
            spec.propeller_diameter_m *= propeller;
            spec.governed_propeller_speed_rpm /= propeller;
            spec.reduction_ratio *= propeller;
        }
        scaled.fan_diameter_m *= diameter;
        scaled.radius_scale_m *= diameter;
        scale_profile(&mut scaled.nacelle_profile, length);
        if let Some(pod) = scaled.centre_engine.as_mut() {
            pod.width_m *= diameter;
            pod.height_m *= diameter;
            scale_profile(&mut pod.profile, length);
            for strut in &mut pod.struts {
                strut.station_m *= length;
            }
            if let Some(fairing) = pod.fin_aft_fairing.as_mut() {
                fairing.top_end_station_m *= length;
            }
        }
        scaled.active_model()?;
        Ok(scaled)
    }
}

impl AlasConfig {
    /// This configuration with its engine scaled by `factor` and the baseline
    /// rating the mass relations scale from pinned to the engine as it is
    /// now, unless the configuration already declares one.
    ///
    /// FLOPS equation 75 scales the engine mass by the ratio of the rated
    /// thrust to `baseline_engine_thrust_kn`, and blank means "the installed
    /// engine" (ratio one); the turboprop group does the same with
    /// `baseline_shaft_power_kw`. Without the pin a scaled engine would keep
    /// the baseline's mass. A factor of one returns the configuration
    /// unchanged.
    ///
    /// # Errors
    ///
    /// As [`EngineConfig::scaled_by`].
    pub fn with_engine_scaled(&self, factor: f64) -> Result<Self, EngineScaleError> {
        let engine = self.geometry.engine.scaled_by(factor)?;
        let mut scaled = self.clone();
        if factor == 1.0 {
            return Ok(scaled);
        }
        match self.geometry.engine.active_model()? {
            ActiveEngineModel::Turbofan(spec) => {
                // The mass relations read the mass basis thrust, so the
                // baseline they scale from is the mass basis of the engine
                // as it is now (the rated thrust unless one is declared).
                let basis_kn = self
                    .geometry
                    .engine
                    .mass_basis_thrust_kn()
                    .unwrap_or(spec.rated_thrust_kn);
                let structure = &mut scaled.mass_model.flops_structure;
                structure.baseline_engine_thrust_kn.get_or_insert(basis_kn);
            }
            ActiveEngineModel::Turboprop(spec) => {
                scaled
                    .mass_model
                    .flops_turboprop
                    .baseline_shaft_power_kw
                    .get_or_insert(spec.takeoff_shaft_power_kw);
            }
        }
        scaled.geometry.engine = engine;
        Ok(scaled)
    }
}

/// The engine a sizing step settled on, as a replayable record: the scaled
/// engine and the mass baselines it was scaled against. Writing it into a
/// configuration with [`AlasConfig::apply_sized_engine`] is idempotent (it
/// replaces, it does not compound), unlike repeating a scale factor.
#[derive(Debug, Clone, PartialEq)]
pub struct SizedEngine {
    /// The scaled engine installation.
    pub engine: EngineConfig,
    /// The FLOPS baseline thrust, kN, the engine mass scales from.
    pub baseline_engine_thrust_kn: Option<f64>,
    /// The turboprop baseline shaft power, kW, the engine mass scales from.
    pub baseline_shaft_power_kw: Option<f64>,
    /// Rating over the catalogue baseline's rating (one: the catalogue engine).
    pub catalogue_scale: f64,
}

impl SizedEngine {
    /// The sized engine `config` carries.
    pub fn of(config: &AlasConfig) -> Self {
        Self {
            engine: config.geometry.engine.clone(),
            baseline_engine_thrust_kn: config.mass_model.flops_structure.baseline_engine_thrust_kn,
            baseline_shaft_power_kw: config.mass_model.flops_turboprop.baseline_shaft_power_kw,
            catalogue_scale: config.geometry.engine.catalogue_scale().unwrap_or(f64::NAN),
        }
    }
}

impl AlasConfig {
    /// Replace the engine installation and its mass baselines with `sized`.
    pub fn apply_sized_engine(&mut self, sized: &SizedEngine) {
        self.geometry.engine = sized.engine.clone();
        self.mass_model.flops_structure.baseline_engine_thrust_kn = sized.baseline_engine_thrust_kn;
        self.mass_model.flops_turboprop.baseline_shaft_power_kw = sized.baseline_shaft_power_kw;
    }
}

#[cfg(test)]
#[path = "engine_scale_tests.rs"]
mod tests;
