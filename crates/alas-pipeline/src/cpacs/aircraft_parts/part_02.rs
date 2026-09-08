// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl CpacsDocument {
    /// Reconstruct the native physics geometry from this CPACS aircraft.
    ///
    /// This path is intentionally limited to transformations and profile
    /// shapes that the existing `Airplane`/`Wing`/`Fuselage` types can retain
    /// without changing solver formulas. A future geometry backend can widen
    /// the accepted CPACS subset without changing the XML reader.
    pub fn to_airplane(&self) -> Result<Airplane, CpacsAircraftError> {
        if self.aircraft.wings.is_empty() {
            return Err(CpacsAircraftError::NoWings);
        }

        let fuselage_origins = self.fuselage_origins();
        let wings = self
            .aircraft
            .wings
            .iter()
            .map(|wing| self.convert_wing(wing, &fuselage_origins))
            .collect::<Result<Vec<_>, _>>()?;
        let fuselages = self
            .aircraft
            .fuselages
            .iter()
            .map(|fuselage| self.convert_fuselage(fuselage, &fuselage_origins))
            .collect::<Result<Vec<_>, _>>()?;

        let reference = self.aircraft.reference.as_ref();
        let reference_wing = wings
            .iter()
            .max_by(|left, right| left.projected_area().total_cmp(&right.projected_area()))
            .ok_or(CpacsAircraftError::NoWings)?;
        let s_ref = reference
            .and_then(|value| value.area)
            .unwrap_or_else(|| reference_wing.projected_area());
        let c_ref = reference
            .and_then(|value| value.length)
            .unwrap_or_else(|| reference_wing.mean_aerodynamic_chord());
        let b_ref = reference_wing.projected_span();
        validate_reference("area", s_ref)?;
        validate_reference("length", c_ref)?;
        validate_reference("span", b_ref)?;

        Ok(Airplane {
            name: self.aircraft.name.clone(),
            xyz_ref: reference
                .and_then(|value| value.point)
                .unwrap_or([0.0, 0.0, 0.0]),
            wings,
            fuselages,
            s_ref,
            c_ref,
            b_ref,
        })
    }

    /// Apply CPACS engine geometry and take-off cycle data to the existing
    /// non-geometric configuration inputs.
    ///
    /// CPACS geometry remains authoritative through [`Self::to_airplane`].
    /// These values only populate the propulsion quantities that the current
    /// mission and mass formulas already read; no formula or solver model is
    /// changed. Missing optional CPACS engine data leaves the caller's
    /// configuration value unchanged.
    pub fn apply_engine_data_to_config(
        &self,
        config: &mut AlasConfig,
        airplane: &Airplane,
    ) -> Result<(), CpacsAircraftError> {
        let engine_uid = self
            .aircraft
            .engine_positions
            .first()
            .map(|position| position.engine_uid.as_str());
        let engine = engine_uid
            .and_then(|uid| self.engines.iter().find(|engine| engine.uid == uid))
            .or_else(|| self.engines.first());
        let Some(engine) = engine else {
            return Ok(());
        };

        if !engine.name.is_empty() {
            config.geometry.engine.engine_name = engine.name.clone();
            if alas_config::engines::get(&engine.name).is_ok() {
                // A known identity intentionally resolves its complete engine
                // record once; CPACS values below remain authoritative and
                // overwrite any differing fields from that baseline.
                config.geometry.engine.apply_engine_spec();
            } else {
                // CPACS 3.5 does not provide a part-power fuel deck. Retaining
                // the caller/default schedule here would attach false engine
                // provenance, so product mission construction must stop until
                // a schedule is supplied explicitly.
                config.geometry.engine.part_power_fuel_flow_ratios.clear();
                config.geometry.engine.part_power_source = format!(
                    "unavailable: CPACS engine '{}' has no matched ICAO/EASA part-power schedule",
                    engine.name
                );
                config.geometry.engine.turbofan = None;
                config.geometry.engine.turboprop = None;
            }
        }
        if let Some(imported) = engine.turboprop.as_ref() {
            let payload = config.geometry.engine.turboprop.as_mut().ok_or(
                CpacsAircraftError::InvalidEngineValue {
                    field: "toolspecific/propulsion/technology",
                    value: f64::NAN,
                },
            )?;
            payload.takeoff_shaft_power_kw = imported.takeoff_shaft_power_kw;
            payload.maximum_reserve_shaft_power_kw = imported.maximum_reserve_shaft_power_kw;
            payload.maximum_continuous_shaft_power_kw = imported.maximum_continuous_shaft_power_kw;
            payload.maximum_climb_shaft_power_kw = imported.maximum_climb_shaft_power_kw;
            payload.maximum_cruise_shaft_power_kw = imported.maximum_cruise_shaft_power_kw;
            payload.maximum_cruise_fuel_flow_kg_h = imported.maximum_cruise_fuel_flow_kg_h;
            payload.propeller_model = imported.propeller_model.clone();
            payload.propeller_diameter_m = imported.propeller_diameter_m;
            payload.governed_propeller_speed_rpm = imported.governed_propeller_speed_rpm;
            payload.reduction_ratio = imported.reduction_ratio;
        }
        if let Some(value) = engine.thrust00_n {
            require_engine_value("thrust00", value, |value| value > 0.0)?;
            if let Some(payload) = config.geometry.engine.turbofan.as_mut() {
                payload.rated_thrust_kn = value / 1_000.0;
            }
        }
        if let Some(value) = engine.fpr00 {
            require_engine_value("fpr00", value, |value| value > 0.0)?;
            config.geometry.engine.fan_pressure_ratio = value;
        }
        if let Some(value) = engine.bpr00 {
            require_engine_value("bpr00", value, |value| value >= 0.0)?;
            config.geometry.engine.bypass_ratio = value;
        }
        if let Some(value) = engine.opr00 {
            require_engine_value("opr00", value, |value| value > 0.0)?;
            config.geometry.engine.overall_pressure_ratio = value;
        }
        if let Some(payload) = config.geometry.engine.turbofan.as_mut() {
            // CPACS scalars are authoritative for the imported derivative.
            // Keep the transitional flat mirror and typed mission payload in
            // lock-step until all remaining consumers are migrated.
            payload.fan_pressure_ratio = config.geometry.engine.fan_pressure_ratio;
            payload.bypass_ratio = config.geometry.engine.bypass_ratio;
            payload.overall_pressure_ratio = config.geometry.engine.overall_pressure_ratio;
        }
        if let Some(value) = engine.geometry_diameter_m {
            require_engine_value("geometry/diameter", value, |value| value > 0.0)?;
            config.geometry.engine.radius_scale_m = value / 2.0;
        }
        if let Some(value) = engine.geometry_length_m {
            require_engine_value("geometry/length", value, |value| value > 0.0)?;
            let current_length = config.geometry.engine.nacelle_length_m();
            if current_length > 0.0 {
                for (station, _) in &mut config.geometry.engine.nacelle_profile {
                    *station *= value / current_length;
                }
            }
        }

        let positions = airplane
            .fuselages
            .iter()
            .filter(|fuselage| fuselage.name.to_ascii_lowercase().contains("nacelle"))
            .filter_map(|fuselage| fuselage.xsecs.first().map(|section| section.xyz_c[1]))
            .collect::<Vec<_>>();
        if !positions.is_empty() {
            config.geometry.engine.spanwise_positions_m = positions;
        }
        Ok(())
    }

    fn convert_wing(
        &self,
        wing: &CpacsWing,
        fuselage_origins: &HashMap<String, [f64; 3]>,
    ) -> Result<Wing, CpacsAircraftError> {
        if wing.sections.is_empty() {
            return Err(CpacsAircraftError::NoSections {
                kind: "wing",
                uid: wing.uid.clone(),
            });
        }
        let symmetric = match wing.symmetry.as_deref() {
            None => false,
            Some("x-z-plane") => true,
            Some(symmetry) => {
                return Err(CpacsAircraftError::UnsupportedSymmetry {
                    wing: wing.name.clone(),
                    symmetry: symmetry.to_owned(),
                })
            }
        };
        validate_rigid_parent_transform(&wing.uid, wing.transformation.as_ref(), "wing")?;
        let parent_origin = wing
            .parent_uid
            .as_ref()
            .map(|uid| {
                fuselage_origins.get(uid).copied().ok_or_else(|| {
                    CpacsAircraftError::UnsupportedTransformation {
                        path: format!("wing/{}/parentUID", wing.uid),
                        reason: "only fuselage parent origins are representable".to_owned(),
                    }
                })
            })
            .transpose()?
            .unwrap_or([0.0, 0.0, 0.0]);
        let wing_origin = transform_translation(wing.transformation.as_ref(), parent_origin);

        let mut sections = Vec::with_capacity(wing.sections.len());
        for section in &wing.sections {
            validate_wing_section_transform(section.transformation.as_ref(), &section.uid)?;
            if section.elements.len() > 1 {
                return Err(CpacsAircraftError::MultipleElements {
                    kind: "wing",
                    uid: section.uid.clone(),
                    count: section.elements.len(),
                });
            }
            let element =
                section
                    .elements
                    .first()
                    .ok_or_else(|| CpacsAircraftError::NoElements {
                        kind: "wing",
                        uid: section.uid.clone(),
                    })?;
            let transform = element.transformation.as_ref();
            let scaling = transform
                .and_then(|value| value.scaling)
                .unwrap_or([1.0, 1.0, 1.0]);
            let rotation = transform
                .and_then(|value| value.rotation)
                .unwrap_or([0.0, 0.0, 0.0]);
            let translation = transform
                .and_then(|value| value.translation)
                .unwrap_or([0.0, 0.0, 0.0]);
            if !nearly(scaling[1], 1.0) || !nearly(scaling[2], scaling[0]) {
                return Err(CpacsAircraftError::UnsupportedTransformation {
                    path: format!("wing/{}/element/{}", wing.uid, element.uid),
                    reason: "airfoil scaling must be [chord, 1, chord]".to_owned(),
                });
            }
            if !is_zero_vector(translation) {
                return Err(CpacsAircraftError::UnsupportedTransformation {
                    path: format!("wing/{}/element/{}", wing.uid, element.uid),
                    reason: "element translation is not representable".to_owned(),
                });
            }
            if !nearly(rotation[0], 0.0) || !nearly(rotation[2], 0.0) {
                return Err(CpacsAircraftError::UnsupportedTransformation {
                    path: format!("wing/{}/element/{}", wing.uid, element.uid),
                    reason: "airfoil rotation must be a native twist about the Y axis".to_owned(),
                });
            }
            let profile = self
                .wing_airfoils
                .iter()
                .find(|profile| profile.uid == element.airfoil_uid)
                .ok_or_else(|| CpacsAircraftError::MissingProfile {
                    path: format!("wing/{}/element/{}", wing.uid, element.uid),
                    profile_uid: element.airfoil_uid.clone(),
                })?;
            if profile.points.len() < 3 {
                return Err(CpacsAircraftError::MissingProfile {
                    path: format!("wing/{}/element/{}", wing.uid, element.uid),
                    profile_uid: element.airfoil_uid.clone(),
                });
            }
            if profile.points.iter().any(|point| !nearly(point[1], 0.0)) {
                return Err(CpacsAircraftError::UnsupportedTransformation {
                    path: format!("wing/{}/profile/{}", wing.uid, profile.uid),
                    reason: "native airfoils must lie in the CPACS XZ plane".to_owned(),
                });
            }
            let coordinates = profile
                .points
                .iter()
                .rev()
                .map(|point| (point[0], point[2]))
                .collect();
            let section_origin =
                transform_translation(section.transformation.as_ref(), wing_origin);
            sections.push(WingXSec::new(
                section_origin,
                scaling[0],
                rotation[1],
                Airfoil::from_coordinates(profile.name.clone(), coordinates),
            ));
        }
        Ok(Wing::new(wing.name.clone(), sections, symmetric))
    }

    fn convert_fuselage(
        &self,
        fuselage: &CpacsFuselage,
        fuselage_origins: &HashMap<String, [f64; 3]>,
    ) -> Result<Fuselage, CpacsAircraftError> {
        if fuselage.sections.is_empty() {
            return Err(CpacsAircraftError::NoSections {
                kind: "fuselage",
                uid: fuselage.uid.clone(),
            });
        }
        validate_rigid_parent_transform(
            &fuselage.uid,
            fuselage.transformation.as_ref(),
            "fuselage",
        )?;
        if fuselage.parent_uid.is_some() {
            return Err(CpacsAircraftError::UnsupportedTransformation {
                path: format!("fuselage/{}/parentUID", fuselage.uid),
                reason: "nested fuselage parents are not representable".to_owned(),
            });
        }
        let parent_origin = fuselage
            .parent_uid
            .as_ref()
            .and_then(|uid| fuselage_origins.get(uid))
            .copied()
            .unwrap_or([0.0, 0.0, 0.0]);
        let fuselage_origin =
            transform_translation(fuselage.transformation.as_ref(), parent_origin);
        let mut sections = Vec::with_capacity(fuselage.sections.len());
        for section in &fuselage.sections {
            validate_fuselage_section_transform(section.transformation.as_ref(), &section.uid)?;
            if section.elements.len() > 1 {
                return Err(CpacsAircraftError::MultipleElements {
                    kind: "fuselage",
                    uid: section.uid.clone(),
                    count: section.elements.len(),
                });
            }
            let element =
                section
                    .elements
                    .first()
                    .ok_or_else(|| CpacsAircraftError::NoElements {
                        kind: "fuselage",
                        uid: section.uid.clone(),
                    })?;
            let profile = self
                .fuselage_profiles
                .iter()
                .find(|profile| profile.uid == element.profile_uid)
                .ok_or_else(|| CpacsAircraftError::MissingProfile {
                    path: format!("fuselage/{}/element/{}", fuselage.uid, element.uid),
                    profile_uid: element.profile_uid.clone(),
                })?;
            if !is_unit_ellipse(profile.points.as_slice()) {
                return Err(CpacsAircraftError::UnsupportedFuselageProfile {
                    section: section.uid.clone(),
                });
            }
            let transform = element.transformation.as_ref();
            let scaling = transform
                .and_then(|value| value.scaling)
                .unwrap_or([1.0, 1.0, 1.0]);
            let rotation = transform
                .and_then(|value| value.rotation)
                .unwrap_or([0.0, 0.0, 0.0]);
            let translation = transform
                .and_then(|value| value.translation)
                .unwrap_or([0.0, 0.0, 0.0]);
            if !is_zero_vector(rotation) || !is_zero_vector(translation) {
                return Err(CpacsAircraftError::UnsupportedTransformation {
                    path: format!("fuselage/{}/element/{}", fuselage.uid, element.uid),
                    reason: "profile rotation and translation are not representable".to_owned(),
                });
            }
            let section_origin =
                transform_translation(section.transformation.as_ref(), fuselage_origin);
            let native = FuselageXSec::new(
                section_origin,
                None,
                Some(2.0 * scaling[1]),
                Some(2.0 * scaling[2]),
                DEFAULT_SHAPE,
            )
            .map_err(|error| CpacsAircraftError::FuselageSection {
                section: section.uid.clone(),
                reason: error.to_string(),
            })?;
            sections.push(native);
        }
        Ok(Fuselage::new(fuselage.name.clone(), sections))
    }

    fn fuselage_origins(&self) -> HashMap<String, [f64; 3]> {
        self.aircraft
            .fuselages
            .iter()
            .map(|fuselage| {
                (
                    fuselage.uid.clone(),
                    transform_translation(fuselage.transformation.as_ref(), [0.0; 3]),
                )
            })
            .collect()
    }
}

fn transform_translation(
    transformation: Option<&CpacsTransformation>,
    parent_origin: [f64; 3],
) -> [f64; 3] {
    let translation = transformation
        .and_then(|value| value.translation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if transformation.and_then(|value| value.translation_reference.as_deref()) == Some("absGlobal")
    {
        translation
    } else {
        add(parent_origin, translation)
    }
}

fn validate_rigid_parent_transform(
    uid: &str,
    transformation: Option<&CpacsTransformation>,
    kind: &'static str,
) -> Result<(), CpacsAircraftError> {
    let scaling = transformation
        .and_then(|value| value.scaling)
        .unwrap_or([1.0, 1.0, 1.0]);
    let rotation = transformation
        .and_then(|value| value.rotation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if scaling != [1.0, 1.0, 1.0] || rotation != [0.0, 0.0, 0.0] {
        return Err(CpacsAircraftError::UnsupportedTransformation {
            path: format!("{kind}/{uid}/transformation"),
            reason: "parent scaling and rotation are not representable".to_owned(),
        });
    }
    Ok(())
}
