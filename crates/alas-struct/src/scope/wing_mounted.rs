// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-mounted point masses that relieve the bending case, and the ones
//! this crate cannot reach.

use alas_config::{ActiveEngineModel, DesignRequirements, EngineConfig, MassModelConfig};

use super::{NotAvailable, OmissionDirection, WING_MOUNTED_GEAR_RELIEF};

/// The near-zero threshold below which a spanwise engine station is treated as
/// centerline-mounted (loading neither semi-wing). Upstream's literal `1e-6`.
const CENTERLINE_Y_THRESHOLD_M: f64 = 1e-6;

/// What one semi-wing carries as discrete items, and what it carries that this
/// crate could not resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct WingMountedRelief {
    /// `(spanwise station m, mass kg)` for every resolved item on the modelled
    /// positive-`y` semi-wing. The datum is the aircraft centreline, the same
    /// one the sizing grid runs on.
    ///
    /// **The second element is a mass, not a force.**
    /// [`crate::loads::apply_point_mass_relief`] is what turns it into the
    /// newtons `n g m` the load case needs.
    pub point_masses_kg: Vec<(f64, f64)>,
    /// Wing-carried items that are real relief and are not in
    /// [`Self::point_masses_kg`].
    pub omitted: Vec<NotAvailable>,
}

impl WingMountedRelief {
    /// Total resolved wing-mounted mass on the modelled semi-wing, kg.
    pub fn total_mass_kg(&self) -> f64 {
        self.point_masses_kg.iter().map(|&(_, m)| m).sum()
    }
}

/// The propeller is wing-carried mass and is not reachable from this crate.
const PROPELLER_MASS: NotAvailable = NotAvailable {
    quantity: "propeller mass per installation, kg",
    reason: "the Hamilton Standard regression (NASA CR-152303 Vol. V eq. V.1.28-V.1.29) needs the \
             propeller diameter and the shaft-power rating, which alas_config's turboprop node \
             declares as blade count, activity factor and construction but which this crate is \
             not given the geometry or rating to evaluate.",
    resolved_by: "the resolved propulsion-group item ledger, passed in as wing-mounted point \
                  masses by a caller that holds it",
    direction: OmissionDirection::Heavier,
};

/// The nacelle is wing-carried mass and is not reachable from this crate.
const NACELLE_MASS: NotAvailable = NotAvailable {
    quantity: "nacelle mass per installation, kg",
    reason: "the declared nacelle area density (NASA CR-152303 Vol. V eq. V.1.6) has to be \
             multiplied by the nacelle wetted area, which is resolved from the aircraft geometry \
             and is not an input to this crate.",
    resolved_by: "the resolved propulsion-group item ledger, passed in as wing-mounted point \
                  masses by a caller that holds it",
    direction: OmissionDirection::Heavier,
};

/// A shaft-power installation with no declared certificated dry mass.
const TURBOPROP_DRY_MASS_NOT_DECLARED: NotAvailable = NotAvailable {
    quantity: "certificated dry mass of one shaft-power installation, kg",
    reason: "the configuration declares no turboprop engine dry mass, and a shaft-power \
             installation must not have one invented from a thrust it does not produce: \
             EngineConfig::thrust_kn is held at exactly zero for a turboprop by design. The \
             installation therefore relieves nothing.",
    resolved_by: "a declared certificated dry mass for the installed engine",
    direction: OmissionDirection::Heavier,
};

/// The engine model did not resolve at all.
const ENGINE_MODEL_UNRESOLVED: NotAvailable = NotAvailable {
    quantity: "wing-mounted powerplant mass and station, kg at m",
    reason: "EngineConfig::active_model did not resolve an engine, so no installed mass could be \
             read and the wing is relieved by no powerplant at all.",
    resolved_by: "an engine configuration that resolves to a turbofan or turboprop model",
    direction: OmissionDirection::Heavier,
};

/// Per-item wing-mounted relief for the modelled (positive-`y`, right)
/// semi-wing, with what could not be resolved alongside it.
///
/// `spanwise_positions_m` lists BOTH wings' engines for the full aircraft (a
/// symmetric twin is `(9.8, -9.8)`); since only one semi-wing is modelled, only
/// `y > 0` stations are returned, otherwise a symmetric pair would double-count
/// one engine's mass onto a single semi-wing. A `y == 0` entry is a
/// centreline or tail-mounted engine, which loads neither wing and is skipped
/// the same way; neither is an omission, because neither is wing-carried.
///
/// The turbofan branch reuses the same per-engine dry-mass formula as
/// `alas-mass::breakdown`'s `m_prop` (thrust / TWR / g, scaled by the
/// installation factor, whose declared scope is "pylon, cowling, fire
/// suppression and other installed accessories"). The two are kept as
/// deliberate duplicates rather than a shared helper: this one is per-engine
/// while `breakdown`'s is aggregated across the engine count, and the shared
/// quantity is the one-engine dry mass, which is short enough that factoring it
/// out would couple two crates for one multiplication. The intent that they stay
/// in sync is what this comment records.
pub fn wing_mounted_relief(
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    req: &DesignRequirements,
) -> WingMountedRelief {
    let mut omitted = vec![WING_MOUNTED_GEAR_RELIEF];
    let installed_kg = match engine_cfg.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => {
            let thrust_n = spec.rated_thrust_kn * 1000.0;
            (thrust_n / (mass_cfg.propulsion_twr_factor * req.gravity_m_s2))
                * mass_cfg.propulsion_installation_factor
        }
        Ok(ActiveEngineModel::Turboprop(_)) => {
            // Counted: the certificated dry engine mass - which for the PW100
            // series includes the reduction gearbox, per EASA TCDS IM.E.041
            // section III.2 - the declared engine-installation mass, shared equally
            // across the installed engines because it is declared for all of
            // them, and the declared propeller accessory mass. The propeller
            // and the nacelle are wing-carried too and are declared missing
            // rather than estimated.
            omitted.push(PROPELLER_MASS);
            omitted.push(NACELLE_MASS);
            match turboprop_installed_mass_each_kg(engine_cfg, mass_cfg) {
                Some(mass_kg) => mass_kg,
                None => {
                    omitted.push(TURBOPROP_DRY_MASS_NOT_DECLARED);
                    return WingMountedRelief {
                        point_masses_kg: Vec::new(),
                        omitted,
                    };
                }
            }
        }
        Err(_) => {
            omitted.push(ENGINE_MODEL_UNRESOLVED);
            return WingMountedRelief {
                point_masses_kg: Vec::new(),
                omitted,
            };
        }
    };
    if !installed_kg.is_finite() || installed_kg <= 0.0 {
        omitted.push(ENGINE_MODEL_UNRESOLVED);
        return WingMountedRelief {
            point_masses_kg: Vec::new(),
            omitted,
        };
    }
    WingMountedRelief {
        point_masses_kg: engine_cfg
            .spanwise_positions_m
            .iter()
            .filter(|&&y_pos| y_pos > CENTERLINE_Y_THRESHOLD_M)
            .map(|&y_pos| (y_pos, installed_kg))
            .collect(),
        omitted,
    }
}

/// The wing-carried mass of one shaft-power installation, kg, from the
/// aircraft's **declared** turboprop record only.
///
/// `None` when no certificated dry mass is declared. Nothing here is derived
/// from a thrust rating.
fn turboprop_installed_mass_each_kg(
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
) -> Option<f64> {
    let turboprop = &mass_cfg.flops_turboprop;
    let dry_each_kg = turboprop.engine_dry_mass_kg?;
    if !dry_each_kg.is_finite() || dry_each_kg <= 0.0 {
        return None;
    }
    let installed_count = engine_cfg.spanwise_positions_m.len().max(1) as f64;
    let installation_each_kg = if turboprop.engine_installation_mass_kg.is_finite() {
        turboprop.engine_installation_mass_kg.max(0.0) / installed_count
    } else {
        0.0
    };
    let accessories_each_kg = if turboprop.propeller_accessory_mass_kg.is_finite() {
        turboprop.propeller_accessory_mass_kg.max(0.0)
    } else {
        0.0
    };
    Some(dry_each_kg + installation_each_kg + accessories_each_kg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    fn preset_named(name: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name} is a registered preset: {error}"))
    }

    #[test]
    fn a_turboprop_declares_its_propeller_and_nacelle_missing_while_counting_its_engine() {
        // The ATR 72-600's PW127M dry mass is declared, so it relieves; the
        // propeller and the nacelle hang off the same wing and do not.
        let config = preset_named("ATR72-600");
        let relief = wing_mounted_relief(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
        );
        assert!(
            !relief.point_masses_kg.is_empty(),
            "a declared dry mass relieves the wing"
        );
        assert!(relief.total_mass_kg() > 0.0);
        let quantities: Vec<&str> = relief.omitted.iter().map(|gap| gap.quantity).collect();
        assert!(quantities.iter().any(|q| q.contains("propeller")));
        assert!(quantities.iter().any(|q| q.contains("nacelle")));
        // Every turboprop omission leaves the box heavier, never lighter.
        assert!(relief
            .omitted
            .iter()
            .all(|gap| gap.direction == OmissionDirection::Heavier));
    }

    #[test]
    fn a_turboprop_without_a_declared_dry_mass_relieves_nothing_and_says_why() {
        let mut config = preset_named("ATR72-600");
        config.mass_model.flops_turboprop.engine_dry_mass_kg = None;
        let relief = wing_mounted_relief(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
        );
        assert!(relief.point_masses_kg.is_empty());
        assert!(relief
            .omitted
            .iter()
            .any(|gap| gap.quantity.contains("certificated dry mass")));
    }

    #[test]
    fn only_the_modelled_semi_wing_is_relieved_by_a_symmetric_pair() {
        // Both wings' stations are declared; counting the mirrored station too
        // would relieve one semi-wing with both engines.
        let mut config = preset_named("A320-200");
        let station = config
            .geometry
            .engine
            .spanwise_positions_m
            .iter()
            .copied()
            .find(|&y| y > 0.0)
            .expect("a wing-mounted twin declares a positive station");
        config.geometry.engine.spanwise_positions_m = vec![station, -station, 0.0];
        let relief = wing_mounted_relief(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
        );
        assert_eq!(relief.point_masses_kg.len(), 1);
        assert_eq!(relief.point_masses_kg[0].0, station);
    }

    #[test]
    fn wing_mounted_gear_is_declared_missing_on_every_aircraft() {
        // The sizing entry points are not given the gear configuration, so this
        // omission is not a property of the powerplant and must be reported
        // whatever the engine resolves to.
        for name in ["A320-200", "ATR72-600"] {
            let config = preset_named(name);
            let relief = wing_mounted_relief(
                &config.geometry.engine,
                &config.mass_model,
                &config.requirements,
            );
            assert!(
                relief.omitted.contains(&WING_MOUNTED_GEAR_RELIEF),
                "{name} must declare the gear it does not relieve with"
            );
        }
    }
}
