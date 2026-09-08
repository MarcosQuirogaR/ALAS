// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl Atr72TurbopropSystem {
    /// Construct a two-unit ATR propulsion adapter from explicit installation data.
    ///
    /// Exactly two positions and two thrust axes are required. Installation mass
    /// remains caller-owned evidence because a defensible common definition for
    /// dry engine, propeller, nacelle, fluids, and mounting mass is not yet fixed.
    pub fn new(
        unit_model: Pw127m568fModel,
        mass_inventory: Vec<PropulsionMassItem>,
        installation: PropulsionInstallation,
    ) -> Result<Self, PropulsionError> {
        if installation.unit_positions_m.len() != 2 || installation.thrust_axes_body.len() != 2 {
            return Err(PropulsionError::OutsideModelDomain(
                "ATR 72 adapter requires exactly two positions and two thrust axes".to_owned(),
            ));
        }
        for axis in &installation.thrust_axes_body {
            let magnitude = (axis[0].powi(2) + axis[1].powi(2) + axis[2].powi(2)).sqrt();
            if !magnitude.is_finite() || (magnitude - 1.0).abs() > 1.0e-9 {
                return Err(PropulsionError::OutsideModelDomain(
                    "ATR 72 thrust axes must be finite unit vectors".to_owned(),
                ));
            }
        }
        Ok(Self {
            unit_model,
            provenance: ModelProvenance {
                model: ModelIdentity {
                    family: "pw127m-568f-turboprop-surrogate".to_owned(),
                    version: "1".to_owned(),
                },
                dataset: Some("ATR 72-212A / PW127M / 568F-1 preliminary".to_owned()),
                sources: vec![
                    "EASA TCDS E.041: PW127M certified shaft-power ratings".to_owned(),
                    "EASA TCDS A.084 and ATR public data: ATR 72 installation and 3.93 m 568F-1 propeller".to_owned(),
                    "ATR 72-600 factsheet: 762 kg/h two-engine fuel flow at maximum cruise; single-point PSFC calibration only".to_owned(),
                    "Generic six-blade CT/CP/J surrogate: no public OEM 568F map; unvalidated".to_owned(),
                ],
            },
            mass_inventory,
            installation,
        })
    }

    fn active_units(&self, failure: &FailureState) -> Result<Vec<usize>, PropulsionError> {
        match failure {
            FailureState::None => Ok(vec![0, 1]),
            FailureState::UnitsUnavailable(indices) => {
                if indices.iter().any(|index| *index > 1)
                    || (indices.len() == 2 && indices[0] == indices[1])
                    || indices.len() > 2
                {
                    return Err(PropulsionError::UnsupportedFailureState);
                }
                Ok((0..2).filter(|index| !indices.contains(index)).collect())
            }
        }
    }

    fn rating_and_fraction(
        &self,
        demand: PropulsionDemand,
        active_count: usize,
    ) -> Result<(Pw127mRating, f64), PropulsionError> {
        let takeoff = if active_count == 1 {
            Pw127mRating::MaximumTakeoffReserve
        } else {
            Pw127mRating::NormalTakeoff
        };
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                Ok((takeoff, value))
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized propulsion demand",
                value,
            }),
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround) => Ok((takeoff, 1.0)),
            PropulsionDemand::Rating(PropulsionRating::MaximumContinuous) => {
                Ok((Pw127mRating::MaximumContinuous, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::MaximumClimb) => {
                Ok((Pw127mRating::MaximumClimb, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::Cruise) => {
                Ok((Pw127mRating::MaximumCruise, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::FlightIdle) => {
                Ok((Pw127mRating::FlightIdleSurrogate, 1.0))
            }
            PropulsionDemand::RatedFraction { rating, fraction }
                if fraction.is_finite() && (0.0..=1.0).contains(&fraction) =>
            {
                let (rating, _) =
                    self.rating_and_fraction(PropulsionDemand::Rating(rating), active_count)?;
                Ok((rating, fraction))
            }
            PropulsionDemand::RatedFraction { fraction, .. } => {
                Err(PropulsionError::InvalidInput {
                    field: "rated force fraction",
                    value: fraction,
                })
            }
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "ATR turboprop surrogate has no inverse force solver",
            )),
        }
    }

    fn solve_normalized_force_fraction(
        unit_model: Pw127m568fModel,
        condition: TurbopropCondition,
        rating: Pw127mRating,
        mode: TurbopropMode,
        requested_force_fraction: f64,
    ) -> Result<f64, PropulsionError> {
        let output_at = |power_fraction| {
            unit_model.evaluate(
                condition,
                TurbopropCommand {
                    rating,
                    power_fraction,
                    mode,
                    propeller_speed_rpm: unit_model.governed_propeller_speed_rpm,
                },
            )
        };
        let maximum = output_at(1.0).map_err(Self::map_error)?.total_thrust_n;
        let target = requested_force_fraction * maximum;
        let mut lower = None;
        for step in 1..=1_000 {
            let fraction = f64::from(step) / 1_000.0;
            if let Ok(output) = output_at(fraction) {
                lower = Some((fraction, output.total_thrust_n));
                break;
            }
        }
        let (mut low, minimum) = lower.ok_or_else(|| {
            PropulsionError::OutsideModelDomain(
                "no governed operating point exists below full rating".to_owned(),
            )
        })?;
        if target < minimum {
            return Err(PropulsionError::OutsideModelDomain(
                "requested force is below the lowest governed point; no PW127M flight-idle schedule is available"
                    .to_owned(),
            ));
        }
        let mut high = 1.0;
        for _ in 0..60 {
            let middle = 0.5 * (low + high);
            let thrust = output_at(middle).map_err(Self::map_error)?.total_thrust_n;
            if thrust < target {
                low = middle;
            } else {
                high = middle;
            }
        }
        Ok(0.5 * (low + high))
    }

    fn map_error(error: TurbopropError) -> PropulsionError {
        match error {
            TurbopropError::NonFinite(field) => PropulsionError::InvalidInput {
                field,
                value: f64::NAN,
            },
            TurbopropError::OutsideDomain { field, value } => {
                PropulsionError::InvalidInput { field, value }
            }
            TurbopropError::UnsupportedMode(_) => PropulsionError::OutsideModelDomain(
                "requested PW127M/568F operating mode lacks a public calibrated map".to_owned(),
            ),
            TurbopropError::GovernorNoSolution { required_power_w } => {
                PropulsionError::OutsideModelDomain(format!(
                    "generic propeller governor cannot absorb {required_power_w} W"
                ))
            }
            TurbopropError::NonPhysicalResult(field) => PropulsionError::NonFiniteOutput(field),
        }
    }

    fn extrapolated() -> ValidityStatus {
        ValidityStatus::Extrapolated {
            reason: "generic unvalidated six-blade propeller surrogate, family-level PSFC prior, and no PW127M altitude-lapse deck".to_owned(),
        }
    }

    fn zero_result(&self, limit_name: &str) -> PropulsionResult {
        PropulsionResult {
            body_force_n: [0.0; 3],
            body_moment_nm: [0.0; 3],
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(0.0),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: 0.0,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: Some(0.0),
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: Some(0.0),
            rotational_speed_rpm: Some(0.0),
            achieved_demand: PropulsionDemand::NormalizedForce(0.0),
            active_limits: vec![ActiveLimit {
                name: limit_name.to_owned(),
                utilization: 1.0,
            }],
            residuals: vec![Residual {
                name: "shaft power balance".to_owned(),
                value: 0.0,
                unit: "W".to_owned(),
            }],
            validity: Self::extrapolated(),
            provenance: self.provenance.clone(),
            trace: None,
        }
    }
}

impl PropulsionSystemModel for Atr72TurbopropSystem {
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError> {
        if request.loads.bleed_mass_flow_kg_s != 0.0 || request.loads.electrical_power_w != 0.0 {
            return Err(PropulsionError::UnsupportedDemand(
                "PW127M surrogate supports mechanical accessory extraction only",
            ));
        }
        if !request.state.values.is_empty() {
            return Err(PropulsionError::UnsupportedDemand(
                "PW127M surrogate has no dynamic state contract",
            ));
        }
        let active = self.active_units(&request.failure)?;
        let mut mode = match request.mode {
            OperatingMode::Normal => TurbopropMode::Governed,
            OperatingMode::Shutdown => {
                if request.demand != PropulsionDemand::NormalizedForce(0.0) {
                    return Err(PropulsionError::UnsupportedDemand(
                        "shutdown requires exactly zero normalized demand",
                    ));
                }
                return Ok(self.zero_result("propulsion shutdown commanded"));
            }
            OperatingMode::FlightIdle => TurbopropMode::FlightIdle,
            other => return Err(PropulsionError::UnsupportedMode(other)),
        };
        let (rating, requested_fraction) =
            self.rating_and_fraction(request.demand, active.len())?;
        if rating == Pw127mRating::FlightIdleSurrogate {
            mode = TurbopropMode::FlightIdle;
        }
        if active.is_empty() {
            return Ok(
                self.zero_result("zero capability: both installed propulsion units unavailable")
            );
        }
        let extra_accessory_per_unit = request.loads.accessory_power_w / active.len() as f64;
        if !extra_accessory_per_unit.is_finite() || extra_accessory_per_unit < 0.0 {
            return Err(PropulsionError::InvalidInput {
                field: "accessory power",
                value: request.loads.accessory_power_w,
            });
        }
        let mut unit_model = self.unit_model;
        unit_model.accessory_power_w += extra_accessory_per_unit;
        let force_fraction = match request.demand {
            PropulsionDemand::NormalizedForce(value) => Some(value),
            _ => None,
        };
        let solved_fraction = if let Some(force_fraction) = force_fraction {
            match Self::solve_normalized_force_fraction(
                unit_model,
                TurbopropCondition {
                    density_kg_m3: request.flight.density_kg_m3,
                    true_airspeed_m_s: request.flight.velocity_m_s,
                },
                rating,
                mode,
                force_fraction,
            ) {
                Ok(value) => value,
                Err(PropulsionError::OutsideModelDomain(reason))
                    if reason.contains("below the lowest governed point") =>
                {
                    // A normalized mission command can legitimately ask for
                    // less than the lowest forward-thrust point represented by
                    // the generic governor. There is no public PW127M idle or
                    // windmilling map, so use the declared neutral flight-idle
                    // surrogate rather than turning a missing operating point
                    // into a panic or an invented propeller force.
                    let mut idle_request = request.clone();
                    idle_request.demand = PropulsionDemand::Rating(PropulsionRating::FlightIdle);
                    idle_request.mode = OperatingMode::FlightIdle;
                    let mut idle_result = self.evaluate(&idle_request)?;
                    idle_result.achieved_demand = PropulsionDemand::NormalizedForce(0.0);
                    idle_result.active_limits.push(ActiveLimit {
                        name: "flight-idle-thrust".to_owned(),
                        utilization: 1.0,
                    });
                    return Ok(idle_result);
                }
                Err(error) => return Err(error),
            }
        } else {
            requested_fraction
        };
        let command_power_fraction = match request.demand {
            PropulsionDemand::RatedFraction { .. }
                if rating != Pw127mRating::FlightIdleSurrogate =>
            {
                let rated_power_w = unit_model.rated_shaft_power_w(rating);
                let idle_power_w =
                    unit_model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
                (idle_power_w + solved_fraction * (rated_power_w - idle_power_w)) / rated_power_w
            }
            _ => solved_fraction,
        };
        let reported_utilization = match request.demand {
            PropulsionDemand::RatedFraction { fraction, .. } => fraction,
            _ => solved_fraction,
        };
        let command = TurbopropCommand {
            rating,
            power_fraction: command_power_fraction,
            mode,
            propeller_speed_rpm: unit_model.governed_propeller_speed_rpm,
        };
        let condition = TurbopropCondition {
            density_kg_m3: request.flight.density_kg_m3,
            true_airspeed_m_s: request.flight.velocity_m_s,
        };
        let mut force = [0.0; 3];
        let mut moment = [0.0; 3];
        let mut fuel_flow = 0.0;
        let mut shaft_power = 0.0;
        let mut torque = 0.0;
        let mut power_residual = 0.0;
        for index in active {
            let output = unit_model
                .evaluate(condition, command)
                .map_err(Self::map_error)?;
            let axis = self.installation.thrust_axes_body[index];
            let unit_force = [
                axis[0] * output.total_thrust_n,
                axis[1] * output.total_thrust_n,
                axis[2] * output.total_thrust_n,
            ];
            for component in 0..3 {
                force[component] += unit_force[component];
            }
            let position = self.installation.unit_positions_m[index];
            moment[0] += position[1] * unit_force[2] - position[2] * unit_force[1];
            moment[1] += position[2] * unit_force[0] - position[0] * unit_force[2];
            moment[2] += position[0] * unit_force[1] - position[1] * unit_force[0];
            fuel_flow += output.fuel_flow_kg_s;
            shaft_power += output.propeller_power_w;
            torque += output.propeller_torque_n_m;
            power_residual += output.power_balance_residual_w;
        }
        Ok(PropulsionResult {
            body_force_n: force,
            body_moment_nm: moment,
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(fuel_flow),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: -fuel_flow,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: Some(shaft_power),
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: Some(torque),
            rotational_speed_rpm: Some(unit_model.governed_propeller_speed_rpm),
            achieved_demand: request.demand,
            active_limits: vec![ActiveLimit {
                name: format!("PW127M {rating:?}"),
                utilization: reported_utilization,
            }],
            residuals: vec![Residual {
                name: "shaft power balance".to_owned(),
                value: power_residual,
                unit: "W".to_owned(),
            }],
            validity: Self::extrapolated(),
            provenance: self.provenance.clone(),
            trace: None,
        })
    }

    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        let result = self.evaluate(&PropulsionRequest {
            flight,
            demand: PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
            mode: OperatingMode::Normal,
            failure,
            loads: Default::default(),
            state: Default::default(),
            time_step_s: None,
        })?;
        Ok(PropulsionCapability {
            maximum_body_force_n: result.body_force_n,
            minimum_body_force_n: [0.0; 3],
            maximum_shaft_power_w: result.shaft_power_w,
            active_limits: result.active_limits,
            validity: result.validity,
            provenance: self.provenance.clone(),
        })
    }

    fn mass_inventory(&self) -> &[PropulsionMassItem] {
        &self.mass_inventory
    }

    fn installation(&self) -> &PropulsionInstallation {
        &self.installation
    }

    fn provenance(&self) -> &ModelProvenance {
        &self.provenance
    }

    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        let (code, message) = match query {
            DiagnosticsQuery::Provenance => (
                "atr72-turboprop-surrogate-provenance",
                PROVENANCE,
            ),
            DiagnosticsQuery::SupportedSemantics => (
                "atr72-turboprop-surrogate-semantics",
                "Two installed engines; normalized-force inverse control; takeoff, maximum-continuous, maximum-climb, maximum-cruise and unvalidated flight-idle ratings; normal, flight-idle and shutdown modes; OEI automatic-reserve rating; mechanical accessory load.",
            ),
            DiagnosticsQuery::ValidityDomain => (
                "atr72-turboprop-surrogate-validity",
                "All points are Extrapolated: public PW127M lapse/fuel decks and Hamilton 568F-1 CT/CP maps are unavailable. Flight idle is a declared surrogate; feather/windmill and reverse are rejected.",
            ),
        };
        PropulsionDiagnostics {
            query,
            items: vec![DiagnosticItem {
                code: code.to_owned(),
                message: message.to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}
