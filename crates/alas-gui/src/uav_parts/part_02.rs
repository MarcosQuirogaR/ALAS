// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl UavWorkflowState {
    /// Snapshot inputs and start one deterministic search in a background worker.
    ///
    /// Returns `false` when a worker is already active or the snapshot is
    /// invalid. Invalid input is exposed through [`Self::execution`].
    pub fn start(&mut self) -> bool {
        if self.is_running() {
            return false;
        }
        if let TopologyAvailability::Unavailable(reason) = self
            .topology
            .availability(UavAnalysisPath::PreliminaryOptimization)
        {
            self.execution = UavExecutionStatus::Failed(format!(
                "{} cannot start: {}",
                self.topology.label(),
                reason.description()
            ));
            return false;
        }
        let request = match self.run_request() {
            Ok(request) => request,
            Err(message) => {
                self.execution = UavExecutionStatus::Failed(message);
                return false;
            }
        };
        let total_candidates = request.evaluations;
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.execution = UavExecutionStatus::Running(OptimizationProgress {
            evaluated_candidates: 0,
            total_candidates,
            verified_candidates: 0,
            best_score: None,
        });
        let (sender, receiver) = channel();
        self.worker_rx = Some(receiver);
        let cancel_flag = self.cancel_flag.clone();
        match thread::Builder::new()
            .name("alas-uav-optimizer".to_owned())
            .spawn(move || run_worker(request, sender, cancel_flag))
        {
            Ok(_) => true,
            Err(error) => {
                self.worker_rx = None;
                self.execution = UavExecutionStatus::Failed(format!(
                    "Could not start the UAV optimization worker: {error}"
                ));
                false
            }
        }
    }

    /// Request cooperative cancellation at the next candidate boundary.
    pub fn cancel(&mut self) {
        let UavExecutionStatus::Running(progress) = self.execution else {
            return;
        };
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.execution = UavExecutionStatus::CancelRequested(progress);
    }

    /// Drain worker messages without blocking the egui frame.
    pub fn poll(&mut self) {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.worker_rx {
            loop {
                match receiver.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for message in messages {
            match message {
                UavWorkerMessage::Progress(progress) => {
                    self.execution = match self.execution {
                        UavExecutionStatus::CancelRequested(_) => {
                            UavExecutionStatus::CancelRequested(progress)
                        }
                        _ => UavExecutionStatus::Running(progress),
                    };
                }
                UavWorkerMessage::Finished(finished) => {
                    self.worker_rx = None;
                    self.last_electrical_mission = finished.electrical_mission;
                    match finished.result {
                        Ok(accepted) => {
                            self.outcome = UavWorkflowOutcome::PreliminaryFeasible {
                                optimized: Box::new(accepted.optimized),
                                shared_core: accepted.shared_core,
                            };
                            self.last_completed_topology = Some(self.topology);
                            self.active_section = UavSection::Results;
                            self.execution = UavExecutionStatus::Completed;
                        }
                        Err(OptimizationError::NoFeasibleDesign(summary)) => {
                            self.outcome = UavWorkflowOutcome::NoFeasibleDesign(summary);
                            self.last_completed_topology = Some(self.topology);
                            self.active_section = UavSection::Results;
                            self.execution = UavExecutionStatus::Completed;
                        }
                        Err(OptimizationError::Cancelled {
                            evaluated_candidates,
                        }) => {
                            self.execution = UavExecutionStatus::Cancelled {
                                evaluated_candidates,
                            };
                        }
                        Err(OptimizationError::InvalidProblem(message)) => {
                            self.execution = UavExecutionStatus::Failed(message);
                        }
                    }
                }
            }
        }
        if disconnected && self.is_running() {
            self.worker_rx = None;
            self.execution = UavExecutionStatus::Failed(
                "The UAV optimization worker stopped without returning a result.".to_owned(),
            );
        }
    }

    /// Whether a worker is active or cancellation is pending.
    pub fn is_running(&self) -> bool {
        matches!(
            self.execution,
            UavExecutionStatus::Running(_) | UavExecutionStatus::CancelRequested(_)
        )
    }

    /// All source-reviewed records compatible with a required role.
    ///
    /// This uses the complete embedded catalogue, including manufacturer and
    /// specialist-source records outside the original RC Innovations seed.
    /// Optional physical fields remain optional in each record; this method
    /// only filters by role and never fills missing evidence.
    pub fn records_for(&self, role: ComponentRole) -> Vec<&'static ComponentRecord> {
        optimization_catalog()
            .map(|catalog| {
                catalog
                    .records
                    .iter()
                    .filter(|record| role_matches(role, &record.kind))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The currently selected source-reviewed record, if the catalogue contains it.
    pub fn selected_record(&self, role: ComponentRole) -> Option<&'static ComponentRecord> {
        selected_record(&self.selections, role)
    }

    /// Return selected catalogue fields that the optimizer must have before it
    /// can build a physically reviewable airframe.
    ///
    /// A selected record is not the same as complete engineering evidence.
    /// The page uses this list for section status and the run summary, while
    /// the optimizer remains the authority that rejects missing values.
    pub fn selected_evidence_gaps(&self) -> Vec<String> {
        let mut gaps = Vec::new();
        for role in ComponentRole::ALL {
            let Some(record) = self.selected_record(role) else {
                gaps.push(format!("{}: selected record", role.label()));
                continue;
            };
            match &record.kind {
                ComponentKind::Battery(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                    missing_field(
                        &mut gaps,
                        role,
                        record,
                        spec.nominal_voltage_v,
                        "nominal voltage",
                    );
                    missing_field(
                        &mut gaps,
                        role,
                        record,
                        spec.series_cells,
                        "series-cell count",
                    );
                }
                ComponentKind::Motor(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                }
                ComponentKind::Esc(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                }
                ComponentKind::Propeller(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.diameter_m, "diameter");
                }
                ComponentKind::Servo(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                    if spec
                        .stall_current_at_voltage(self.systems.control_bus_voltage_v)
                        .is_none()
                    {
                        gaps.push(format!(
                            "{} {}: stall current at {:.1} V",
                            role.label(),
                            record.id,
                            self.systems.control_bus_voltage_v
                        ));
                    }
                }
                ComponentKind::MaterialStock(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "stock mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "stock dimensions");
                    missing_field(
                        &mut gaps,
                        role,
                        record,
                        spec.youngs_modulus_pa,
                        "Young's modulus",
                    );
                    missing_field(
                        &mut gaps,
                        role,
                        record,
                        spec.allowable_stress_pa,
                        "allowable stress",
                    );
                }
                ComponentKind::Receiver(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                    if spec.min_voltage_v.is_none() || spec.max_voltage_v.is_none() {
                        gaps.push(format!(
                            "{} {}: published supply-voltage range",
                            role.label(),
                            record.id
                        ));
                    }
                }
                ComponentKind::Electronics(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                }
                ComponentKind::LandingGear(spec) => {
                    missing_field(&mut gaps, role, record, spec.mass_kg, "mass");
                    missing_field(&mut gaps, role, record, spec.dimensions, "dimensions");
                    missing_field(
                        &mut gaps,
                        role,
                        record,
                        spec.max_aircraft_mass_kg,
                        "maximum supported aircraft mass",
                    );
                }
            }
        }
        gaps
    }

    /// Build the currently selected full-aircraft BOM before running a search.
    pub fn selected_aircraft_bom(&self) -> AircraftBillOfMaterials {
        optimized_aircraft_bom(&SelectedComponents {
            battery_id: self.selections.battery.clone(),
            motor_id: self.selections.motor.clone(),
            motor_count: self.propulsion_motor_count,
            esc_id: self.selections.esc.clone(),
            propeller_id: self.selections.propeller.clone(),
            servo_id: self.selections.servo.clone(),
            servo_count: 3,
            material_id: self.selections.material.clone(),
            receiver_id: self.selections.receiver.clone(),
            electronics_id: self.selections.electronics.clone(),
            landing_gear_id: self.selections.landing_gear.clone(),
            propulsion_evidence: self.propulsion_evidence.clone(),
        })
    }
}

