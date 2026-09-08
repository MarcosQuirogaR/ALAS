// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Technology-independent interface consumed by aircraft analyses.
pub trait PropulsionSystemModel: Send + Sync {
    /// Evaluate one system operating point.
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError>;
    /// Solve the technology controls needed to deliver a requested body force.
    /// Implementations that lack an inverse solver return
    /// [`PropulsionError::UnsupportedDemand`].
    fn solve_for_force(
        &self,
        mut request: PropulsionRequest,
        required_body_force_n: [f64; 3],
    ) -> Result<PropulsionResult, PropulsionError> {
        request.demand = PropulsionDemand::RequiredBodyForceN(required_body_force_n);
        self.evaluate(&request)
    }
    /// Return available minimum and maximum output at one flight condition.
    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError>;
    /// Return the explicit installed propulsion mass and CG inventory.
    fn mass_inventory(&self) -> &[PropulsionMassItem];
    /// Return geometry and axes of installed propulsion units.
    fn installation(&self) -> &PropulsionInstallation;
    /// Return model, dataset, calibration, and evidence traceability.
    fn provenance(&self) -> &ModelProvenance;
    /// Return model capabilities, validity, or provenance without evaluating
    /// an operating point.
    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics;
}

/// Lossless system adapter around the current mission turbofan evaluator.
pub struct LegacyTurbofanModel {
    inputs: TurbofanInputs,
    params: VehicleBuilderParams,
    compressor_nondimensional_massflow: f64,
    provenance: ModelProvenance,
    mass_inventory: Vec<PropulsionMassItem>,
    installation: PropulsionInstallation,
}

const LEGACY_VALIDITY_REASON: &str = "legacy mission turbofan compatibility model is preserved for migration parity and has not been validated as a physical engine deck";

impl LegacyTurbofanModel {
    /// Construct a lossless adapter around an already sized mission turbofan.
    ///
    /// The preserved evaluator returns one scalar all-engine force and no
    /// per-unit forces. Consequently, this compatibility adapter accepts only
    /// finite, co-axial `+X` installations whose lateral and vertical offsets
    /// sum to zero. That symmetry is what makes its reported zero installed
    /// moment physically representable. New technology implementations should
    /// calculate each unit's force and moment instead of using this constraint.
    pub fn new(
        inputs: TurbofanInputs,
        params: VehicleBuilderParams,
        compressor_nondimensional_massflow: f64,
        provenance: ModelProvenance,
        mass_inventory: Vec<PropulsionMassItem>,
        installation: PropulsionInstallation,
    ) -> Result<Self, PropulsionError> {
        let engine_count = inputs.number_of_engines;
        if !engine_count.is_finite()
            || engine_count < 1.0
            || engine_count.fract().abs() > f64::EPSILON
        {
            return Err(PropulsionError::InvalidInput {
                field: "number of engines",
                value: engine_count,
            });
        }
        let engine_count = engine_count as usize;
        if installation.unit_positions_m.len() != engine_count
            || installation.thrust_axes_body.len() != engine_count
        {
            return Err(PropulsionError::InvalidInstallation(format!(
                "expected {engine_count} positions and thrust axes"
            )));
        }
        if installation
            .unit_positions_m
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(PropulsionError::InvalidInstallation(
                "unit positions must be finite".to_owned(),
            ));
        }
        let axis_tolerance = 1.0e-12;
        if installation.thrust_axes_body.iter().any(|axis| {
            axis.iter().any(|value| !value.is_finite())
                || (axis[0] - 1.0).abs() > axis_tolerance
                || axis[1].abs() > axis_tolerance
                || axis[2].abs() > axis_tolerance
        }) {
            return Err(PropulsionError::InvalidInstallation(
                "legacy scalar adapter requires finite unit thrust axes aligned with +X".to_owned(),
            ));
        }
        let summed_y_m: f64 = installation.unit_positions_m.iter().map(|p| p[1]).sum();
        let summed_z_m: f64 = installation.unit_positions_m.iter().map(|p| p[2]).sum();
        let position_scale_m = installation
            .unit_positions_m
            .iter()
            .flat_map(|position| position.iter())
            .fold(1.0_f64, |scale, value| scale.max(value.abs()));
        let symmetry_tolerance_m = position_scale_m * 1.0e-12;
        if summed_y_m.abs() > symmetry_tolerance_m || summed_z_m.abs() > symmetry_tolerance_m {
            return Err(PropulsionError::InvalidInstallation(
                "legacy scalar adapter requires symmetric offsets producing zero net moment"
                    .to_owned(),
            ));
        }

        Ok(Self {
            inputs,
            params,
            compressor_nondimensional_massflow,
            provenance,
            mass_inventory,
            installation,
        })
    }

    fn demand_fraction(&self, demand: PropulsionDemand) -> Result<f64, PropulsionError> {
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                Ok(value)
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized force demand",
                value,
            }),
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no inverse force solver",
            )),
            PropulsionDemand::Rating(_) => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no named rating schedules",
            )),
            PropulsionDemand::RatedFraction { .. } => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no named rating schedules",
            )),
        }
    }

    fn raw_evaluate(
        &self,
        flight: FlightCondition,
        demand: f64,
    ) -> Result<ThrustOutput, PropulsionError> {
        let freestream = Freestream::from(flight);
        let raw = evaluate_thrust(
            &freestream,
            &self.inputs,
            &self.params,
            self.compressor_nondimensional_massflow,
            demand,
        );
        for (name, value) in [
            ("thrust", raw.thrust_n),
            ("fuel flow", raw.fuel_flow_rate_kg_s),
            ("power", raw.power_w),
        ] {
            if !value.is_finite() {
                return Err(PropulsionError::NonFiniteOutput(name));
            }
        }
        Ok(raw)
    }
}

impl PropulsionSystemModel for LegacyTurbofanModel {
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError> {
        if request.mode != OperatingMode::Normal {
            return Err(PropulsionError::UnsupportedMode(request.mode));
        }
        if request.failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        if request.loads != PropulsionLoads::default() {
            return Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no accessory, bleed, or electrical load model",
            ));
        }
        let demand = self.demand_fraction(request.demand)?;
        let raw = self.raw_evaluate(request.flight, demand)?;
        Ok(PropulsionResult {
            body_force_n: [raw.thrust_n, 0.0, 0.0],
            body_moment_nm: [0.0; 3],
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(raw.fuel_flow_rate_kg_s),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: -raw.fuel_flow_rate_kg_s,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: None,
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: None,
            rotational_speed_rpm: None,
            achieved_demand: PropulsionDemand::NormalizedForce(demand),
            active_limits: Vec::new(),
            residuals: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned(),
            },
            provenance: self.provenance.clone(),
            trace: Some(TechnologyTrace::LegacyTurbofan(raw)),
        })
    }

    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        if failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        let maximum = self.raw_evaluate(flight, 1.0)?;
        let minimum = self.raw_evaluate(flight, 0.0)?;
        Ok(PropulsionCapability {
            maximum_body_force_n: [maximum.thrust_n, 0.0, 0.0],
            minimum_body_force_n: [minimum.thrust_n, 0.0, 0.0],
            maximum_shaft_power_w: None,
            active_limits: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned(),
            },
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
        let message = match query {
            DiagnosticsQuery::Provenance => "Legacy mission turbofan adapter; outputs retain their original equations and selected dataset.",
            DiagnosticsQuery::SupportedSemantics => "Supports normal mode, all units available, normalized force demand, and zero aircraft-service extraction only.",
            DiagnosticsQuery::ValidityDomain => LEGACY_VALIDITY_REASON,
        };
        PropulsionDiagnostics {
            query,
            items: vec![DiagnosticItem {
                code: "legacy-turbofan-adapter".to_owned(),
                message: message.to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}

/// Initial orchestrator facade. It owns the selected system implementation and
/// is the stable injection point for mission, performance, GUI and reporting.
pub struct PropulsionOrchestrator {
    model: Box<dyn PropulsionSystemModel>,
}

impl PropulsionOrchestrator {
    /// Select the system-level technology implementation used by consumers.
    pub fn new(model: impl PropulsionSystemModel + 'static) -> Self {
        Self {
            model: Box::new(model),
        }
    }

    /// Evaluate one propulsion-system operating point.
    pub fn evaluate(
        &self,
        request: &PropulsionRequest,
    ) -> Result<PropulsionResult, PropulsionError> {
        self.model.evaluate(request)
    }

    /// Query available system output at a flight condition and failure state.
    pub fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        self.model.capability(flight, failure)
    }

    /// Delegate an inverse force request to the selected technology model.
    pub fn solve_for_force(
        &self,
        request: PropulsionRequest,
        required_body_force_n: [f64; 3],
    ) -> Result<PropulsionResult, PropulsionError> {
        self.model.solve_for_force(request, required_body_force_n)
    }

    /// Return the selected model's installed mass inventory.
    pub fn mass_inventory(&self) -> &[PropulsionMassItem] {
        self.model.mass_inventory()
    }

    /// Return the selected model's installation geometry and thrust axes.
    pub fn installation(&self) -> &PropulsionInstallation {
        self.model.installation()
    }

    /// Return model and evidence provenance.
    pub fn provenance(&self) -> &ModelProvenance {
        self.model.provenance()
    }

    /// Query the selected technology model's capabilities and provenance.
    pub fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        self.model.diagnostics(query)
    }
}

