// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mass statement: component masses plus stations plus loadable items,
//! turned into an item-level ledger and named mass states.
//!
//! [`crate::breakdown`] answers "how much does the wing weigh"; this module
//! answers "what is the aircraft's mass, centre of gravity and inertia
//! tensor at takeoff". [`MassStatement::build`] places every product
//! [`crate::breakdown::MassBreakdown`] component at its
//! [`crate::stations::ComponentStations`] station as a [`MassItem`], with a
//! centroidal tensor from [`crate::inertia`]'s closed-form solids ([`build`]
//! is that physical detail), then [`MassStatement::state`] combines the
//! right subset for each named load case. Fuel is kept out of the built
//! ledger: [`MassStatement::state`] and [`MassStatement::with_fuel_items`]
//! add whichever usable-fuel items the caller supplies on top of the
//! zero-fuel ledger, so a CG-vs-fuel sweep never has to rebuild it.

mod build;
mod flops_items;

use alas_config::MassModelConfig;

use crate::breakdown::{FlopsMassBuildup, MassBreakdown};
use crate::flops_transport::FlopsTransportBreakdown;
use crate::inertia::RadiiOfGyration;
use crate::ledger::{LedgerError, MassGroup, MassItem, MassLedger, MassMethod, MassProperties};
use crate::stations::ComponentStations;

/// Which mass method actually produced each replaceable ledger group.
///
/// The ledger labels every row with a [`MassMethod`] so an audit can tell a
/// Torenbeek correlation from a FLOPS equation. Those labels are derived from
/// the authoritative [`MassModelConfig::mass_architecture`], not from the
/// compatibility selector mirrors. A single architecture keeps all three
/// replaceable groups on one method family, so a stale selector cannot create
/// a hybrid ledger label.
///
/// [`Self::default`] is the explicit reference-compatible labelling for a
/// lumped statement. A statement carrying a verified FLOPS systems buildup
/// uses [`Self::pure_flops`] automatically through [`MassStatement::build`].
/// Callers holding an [`alas_config::MassModelConfig`] should use
/// [`Self::from_mass_model`] with [`MassStatement::build_with_methods`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerMethods {
    /// Wing, both tails and the fuselage.
    pub structure: MassMethod,
    /// Nose and main landing gear.
    pub landing_gear: MassMethod,
    /// Installed propulsion, including nacelles.
    pub propulsion: MassMethod,
}

impl Default for LedgerMethods {
    fn default() -> Self {
        Self {
            structure: MassMethod::Correlation("Torenbeek"),
            landing_gear: MassMethod::TakeoffMassFraction,
            propulsion: MassMethod::Correlation("thrust-to-weight"),
        }
    }
}

impl LedgerMethods {
    /// Labels for a complete pure-FLOPS component statement.
    ///
    /// A FLOPS structural result owns the wing, tails, fuselage and landing
    /// gear together, while its propulsion result owns the installed engines
    /// and nacelles. Keeping this as one constructor prevents a caller that
    /// supplies the FLOPS subsystem buildup from accidentally retaining the
    /// legacy labels on the other groups.
    pub const fn pure_flops() -> Self {
        Self {
            structure: MassMethod::Correlation("FLOPS"),
            landing_gear: MassMethod::Correlation("FLOPS"),
            propulsion: MassMethod::Correlation("FLOPS"),
        }
    }

    /// The labels the selected mass methods imply.
    ///
    /// The FLOPS structural group covers the wing, both tails, the fuselage
    /// and the landing gear (equation 136), so selecting it relabels all
    /// five. The propulsion selection relabels the installed propulsion
    /// group alone.
    pub fn from_mass_model(model: &MassModelConfig) -> Self {
        let reference = Self::default();
        let flops_structure = model.mass_architecture.is_pure_flops();
        let flops_propulsion = model.mass_architecture.is_pure_flops();
        Self {
            structure: if flops_structure {
                MassMethod::Correlation("FLOPS")
            } else {
                reference.structure
            },
            landing_gear: if flops_structure {
                MassMethod::Correlation("FLOPS")
            } else {
                reference.landing_gear
            },
            propulsion: if flops_propulsion {
                match model.flops_structure.pylon_mass_method {
                    alas_config::PylonMassMethod::None => MassMethod::Correlation("FLOPS"),
                    alas_config::PylonMassMethod::LthBoxBeamV1 => {
                        MassMethod::Correlation("FLOPS + LTH pylons")
                    }
                }
            } else {
                reference.propulsion
            },
        }
    }

    /// Methods from the completed evaluation rather than a potentially stale
    /// configuration. A shaft-power installation is not a FLOPS jet engine.
    pub fn from_buildup(buildup: &FlopsMassBuildup) -> Self {
        let mut methods = Self::pure_flops();
        methods.propulsion = if let Some(group) = &buildup.airframe.turboprop_propulsion {
            if group.engine_mass_source == "declared_certificated_dry_mass" {
                MassMethod::Correlation("declared engine + GASP/TM-83458 + FLOPS fuel system")
            } else {
                MassMethod::Correlation("GASP/TM-83458 + FLOPS fuel system")
            }
        } else if buildup.airframe.propulsion_inputs.pylon_mass_method
            == alas_config::PylonMassMethod::LthBoxBeamV1
        {
            MassMethod::Correlation("FLOPS + LTH pylons")
        } else {
            MassMethod::Correlation("FLOPS")
        };
        methods
    }
}

/// A payload item's mass, position and extent, independent of
/// `alas-payload`'s `DeckItem` so this crate does not depend on it.
///
/// The pipeline fills these from `PayloadLayout`'s `DeckItem`s; an empty
/// slice of these falls back to [`ComponentStations::payload_fallback`].
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadItemSummary {
    /// A human-readable label, folded into the ledger item's id.
    pub label: String,
    /// Item mass, kg.
    pub mass_kg: f64,
    /// Item centroid in the geometry frame, m.
    pub position_m: [f64; 3],
    /// Item bounding extent `[length_x, width_y, height_z]`, m.
    pub extent_m: [f64; 3],
}

/// A named aircraft mass state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadState {
    /// Fixed items, operating items and unusable fuel: no payload, no usable fuel.
    OperatingEmpty,
    /// Operating empty plus payload, no usable fuel.
    ZeroFuel,
    /// Zero fuel plus the statement's takeoff fuel items.
    Takeoff,
    /// Zero fuel plus the statement's landing fuel items.
    Landing,
}

/// Everything [`MassStatement::build`] needs to construct a ledger.
pub struct MassStatementInputs<'a> {
    /// The ten-group component masses from the selected architecture.
    pub masses: &'a MassBreakdown,
    /// Geometry-derived placement and extent for every component.
    pub stations: &'a ComponentStations,
    /// Per-item payload, or empty to use the lumped fallback.
    pub payload_items: &'a [PayloadItemSummary],
    /// Usable-fuel items at takeoff, from the tanks/fuel-plan modules.
    pub takeoff_fuel_items: Vec<MassItem>,
    /// Usable-fuel items at landing.
    pub landing_fuel_items: Vec<MassItem>,
    /// Fuel that cannot be delivered to the engines; part of the OEW.
    pub unusable_fuel_items: Vec<MassItem>,
    /// The verified FLOPS systems/operating-items buildup, when the
    /// selected systems-mass method produced one.
    pub flops: Option<&'a FlopsTransportBreakdown>,
}

/// The item-level mass ledger plus the fuel loads it is combined with.
///
/// The ledger itself never holds usable fuel: [`Self::state`] and
/// [`Self::with_fuel_items`] add [`Self::takeoff_fuel`]/
/// [`Self::landing_fuel`] (or an arbitrary set) on top of it.
pub struct MassStatement {
    ledger: MassLedger,
    takeoff_fuel: Vec<MassItem>,
    landing_fuel: Vec<MassItem>,
}

/// A ledger's radii of gyration next to Raymer's jet-transport reference row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiiComparison {
    /// Roll, pitch and yaw radii of gyration from the ledger, m.
    pub ledger_radii_m: [f64; 3],
    /// The same three radii Raymer's jet-transport fractions imply, m.
    pub reference_radii_m: [f64; 3],
    /// `ledger / reference` per axis; near 1 is plausible, not a pin.
    pub ratio: [f64; 3],
}

impl MassStatement {
    /// Build and validate the ledger, keeping the fuel items separate.
    ///
    /// # Errors
    ///
    /// [`LedgerError`] if any item (including a caller-supplied payload or
    /// fuel item) has an invalid mass, position, inertia tensor, or a
    /// duplicate id, or if supplied unusable-fuel rows disagree with the
    /// selected method's own unusable-fuel allocation, or if itemized payload
    /// disagrees with the component payload mass.
    pub fn build(inputs: MassStatementInputs<'_>) -> Result<Self, LedgerError> {
        let methods = if let Some(flops) = inputs.flops {
            let mut methods = LedgerMethods::pure_flops();
            if matches!(
                flops.propulsion_sizing,
                crate::flops_transport::PropulsionSizing::ShaftPower { .. }
            ) {
                methods.propulsion = MassMethod::Correlation(
                    "shaft-power installation (resolved sources in buildup)",
                );
            }
            methods
        } else {
            LedgerMethods::default()
        };
        Self::build_with_methods(inputs, methods)
    }

    /// [`Self::build`] with explicit per-group method labels.
    ///
    /// Only the [`MassMethod`] tag on the structural, landing-gear and
    /// propulsion rows changes; no mass moves. Use this wherever the
    /// selected [`alas_config::MassModelConfig`] is in scope, so the ledger
    /// does not describe a FLOPS structural group as a Torenbeek one.
    ///
    /// # Errors
    ///
    /// As [`Self::build`].
    pub fn build_with_methods(
        inputs: MassStatementInputs<'_>,
        methods: LedgerMethods,
    ) -> Result<Self, LedgerError> {
        let ledger = build::build_ledger(
            inputs.masses,
            inputs.stations,
            inputs.payload_items,
            inputs.unusable_fuel_items,
            inputs.flops,
            methods,
        )?;
        ledger.validate()?;
        Ok(Self {
            ledger,
            takeoff_fuel: inputs.takeoff_fuel_items,
            landing_fuel: inputs.landing_fuel_items,
        })
    }

    /// The built ledger, excluding usable fuel (see the struct doc).
    pub fn ledger(&self) -> &MassLedger {
        &self.ledger
    }

    /// Every ledger item combined: operating empty plus payload, no usable fuel.
    fn zero_fuel(&self) -> MassProperties {
        self.ledger.properties_where(|_| true)
    }

    /// Mass, centre of gravity and inertia tensor for a named load state.
    pub fn state(&self, state: LoadState) -> MassProperties {
        match state {
            LoadState::OperatingEmpty => self.ledger.operating_empty(),
            LoadState::ZeroFuel => self.zero_fuel(),
            LoadState::Takeoff => self.with_fuel_items(&self.takeoff_fuel),
            LoadState::Landing => self.with_fuel_items(&self.landing_fuel),
        }
    }

    /// Zero-fuel properties plus an arbitrary set of fuel items.
    ///
    /// This is what a CG-vs-fuel curve sweeps over: each candidate fuel
    /// loading is combined with the same zero-fuel ledger rather than
    /// rebuilding it.
    pub fn with_fuel_items(&self, fuel_items: &[MassItem]) -> MassProperties {
        let zero_fuel = self.zero_fuel();
        let fuel_properties: Vec<MassProperties> =
            fuel_items.iter().map(MassItem::properties).collect();
        let mut parts = Vec::with_capacity(fuel_properties.len() + 1);
        parts.push(zero_fuel);
        parts.extend(fuel_properties);
        MassProperties::combine(parts.iter())
    }

    /// `props`'s longitudinal centre of gravity as a percentage of the mean
    /// aerodynamic chord, aft of `mac_leading_edge_x_m`.
    pub fn cg_pct_mac(&self, props: &MassProperties, mac_leading_edge_x_m: f64, mac_m: f64) -> f64 {
        100.0 * (props.cg_m[0] - mac_leading_edge_x_m) / mac_m
    }

    /// Compare `state`'s ledger radii of gyration with Raymer's
    /// jet-transport reference row at the given span and fuselage length.
    ///
    /// This is a plausibility check, not a pin: [`RadiiComparison::ratio`]
    /// near 1 says the ledger's mass distribution is of the right order for
    /// a jet transport, not that either value is correct to more precision
    /// than a conceptual-design correlation supports.
    pub fn radii_of_gyration_check(
        &self,
        state: LoadState,
        span_m: f64,
        fuselage_length_m: f64,
    ) -> RadiiComparison {
        let props = self.state(state);
        let ledger_radii_m = props.radii_of_gyration();
        let reference_radii_m = RadiiOfGyration::JET_TRANSPORT.radii_m(span_m, fuselage_length_m);
        let ratio = [
            safe_ratio(ledger_radii_m[0], reference_radii_m[0]),
            safe_ratio(ledger_radii_m[1], reference_radii_m[1]),
            safe_ratio(ledger_radii_m[2], reference_radii_m[2]),
        ];
        RadiiComparison {
            ledger_radii_m,
            reference_radii_m,
            ratio,
        }
    }

    /// Mass of each ledger group present in `state`, in first-appearance order.
    pub fn group_totals(&self, state: LoadState) -> Vec<(MassGroup, f64)> {
        let mut totals = match state {
            LoadState::OperatingEmpty => {
                group_totals_where(&self.ledger, |item| item.role.is_operating_empty())
            }
            LoadState::ZeroFuel | LoadState::Takeoff | LoadState::Landing => {
                self.ledger.group_totals()
            }
        };
        let fuel_items: &[MassItem] = match state {
            LoadState::Takeoff => &self.takeoff_fuel,
            LoadState::Landing => &self.landing_fuel,
            LoadState::OperatingEmpty | LoadState::ZeroFuel => &[],
        };
        if !fuel_items.is_empty() {
            totals.push((
                MassGroup::Fuel,
                fuel_items.iter().map(|item| item.mass_kg).sum(),
            ));
        }
        totals
    }
}

/// `numerator / denominator`, or `0.0` rather than `NaN`/`inf` when the
/// reference radius is degenerate (zero span or fuselage length).
fn safe_ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator > 0.0 {
        numerator / denominator
    } else {
        0.0
    }
}

/// Mass of each group among the items `include` selects, first-appearance order.
fn group_totals_where(
    ledger: &MassLedger,
    include: impl Fn(&MassItem) -> bool,
) -> Vec<(MassGroup, f64)> {
    let mut totals: Vec<(MassGroup, f64)> = Vec::new();
    for item in ledger.items().iter().filter(|item| include(item)) {
        match totals.iter_mut().find(|(group, _)| *group == item.group) {
            Some((_, total)) => *total += item.mass_kg,
            None => totals.push((item.group, item.mass_kg)),
        }
    }
    totals
}

#[cfg(test)]
#[path = "statement_tests.rs"]
mod tests;
