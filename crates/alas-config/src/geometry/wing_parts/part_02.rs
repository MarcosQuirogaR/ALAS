// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl WingConfig {
    /// Resolve the side-of-body/root/kink/tip planform for `design`.
    ///
    /// Existing configurations leave all optional transport fields unset and
    /// therefore produce the historical root/kink/tip geometry exactly. New
    /// configurations can make the side-of-body and outboard sweep explicit
    /// without consumers duplicating sweep or trailing-edge calculations.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when station order, dimensions,
    /// chords, or sweep angles cannot describe a finite physical wing.
    pub fn transport_planform(
        &self,
        design: &DesignVector,
    ) -> Result<TransportPlanform, TransportPlanformError> {
        validate_finite("span", design.span_m)?;
        validate_positive("span", design.span_m)?;
        validate_finite("root chord", design.root_chord_m)?;
        validate_positive("root chord", design.root_chord_m)?;
        validate_finite("kink chord", design.break_chord_m)?;
        validate_positive("kink chord", design.break_chord_m)?;
        validate_finite("tip chord", design.tip_chord_m)?;
        validate_positive("tip chord", design.tip_chord_m)?;
        validate_finite("inboard leading-edge sweep", design.sweep_deg)?;

        let semi_span_m = design.span_m / 2.0;
        let side_of_body_span_fraction = self.side_of_body_span_fraction.unwrap_or(0.0);
        let kink_span_fraction = self.kink_span_fraction.unwrap_or(self.break_span_fraction);
        let transport_extension_active =
            self.side_of_body_span_fraction.is_some() || self.kink_span_fraction.is_some();
        let outboard_le_sweep_deg = if transport_extension_active {
            design.sweep_deg
        } else {
            self.outboard_le_sweep_deg
                .unwrap_or(design.sweep_deg - self.outboard_sweep_decrement_deg)
        };

        validate_finite("side-of-body span fraction", side_of_body_span_fraction)?;
        validate_finite("kink span fraction", kink_span_fraction)?;
        if let Some(side_of_body_chord_ratio) = self.side_of_body_chord_ratio {
            validate_finite("side-of-body chord ratio", side_of_body_chord_ratio)?;
            validate_positive("side-of-body chord ratio", side_of_body_chord_ratio)?;
        }
        validate_finite("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        validate_sweep("inboard leading-edge sweep", design.sweep_deg)?;
        validate_sweep("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        if side_of_body_span_fraction < 0.0
            || side_of_body_span_fraction >= kink_span_fraction
            || kink_span_fraction >= 1.0
        {
            return Err(TransportPlanformError::InvalidStationOrder {
                side_of_body: side_of_body_span_fraction,
                kink: kink_span_fraction,
            });
        }
        let root = MainWingStation {
            kind: MainWingStationKind::Root,
            span_fraction: 0.0,
            y_m: 0.0,
            leading_edge_x_m: 0.0,
            chord_m: design.root_chord_m,
        };
        let kink_y_m = kink_span_fraction * semi_span_m;
        let kink = MainWingStation {
            kind: MainWingStationKind::Kink,
            span_fraction: kink_span_fraction,
            y_m: kink_y_m,
            leading_edge_x_m: kink_y_m * design.sweep_deg.to_radians().tan(),
            chord_m: design.break_chord_m,
        };
        let side_of_body_y_m = side_of_body_span_fraction * semi_span_m;
        let side_of_body = (side_of_body_span_fraction > 0.0).then(|| {
            let root_to_kink_fraction = side_of_body_span_fraction / kink_span_fraction;
            let interpolated_chord_m = design.root_chord_m
                + root_to_kink_fraction * (design.break_chord_m - design.root_chord_m);
            let leading_edge_x_m = side_of_body_y_m * design.sweep_deg.to_radians().tan();
            let kink_trailing_edge_x_m = kink.leading_edge_x_m + kink.chord_m;
            // The centreline root is a carry-through datum inside the body,
            // not an exposed aerodynamic chord. When no explicit body chord
            // is supplied, retain the linear panel unless that would make the
            // exposed body-to-kink trailing edge run forward; in that case
            // the side-of-body chord ends at the kink trailing-edge station.
            let derived_chord_m =
                interpolated_chord_m.min(kink_trailing_edge_x_m - leading_edge_x_m);
            MainWingStation {
                kind: MainWingStationKind::SideOfBody,
                span_fraction: side_of_body_span_fraction,
                y_m: side_of_body_y_m,
                leading_edge_x_m,
                chord_m: self
                    .side_of_body_chord_ratio
                    .map_or(derived_chord_m, |ratio| design.root_chord_m * ratio),
            }
        });
        let tip_y_m = semi_span_m;
        let tip = MainWingStation {
            kind: MainWingStationKind::Tip,
            span_fraction: 1.0,
            y_m: tip_y_m,
            leading_edge_x_m: kink.leading_edge_x_m
                + (tip_y_m - kink_y_m) * outboard_le_sweep_deg.to_radians().tan(),
            chord_m: design.tip_chord_m,
        };

        let planform = TransportPlanform {
            root,
            side_of_body,
            kink,
            tip,
            inboard_le_sweep_deg: design.sweep_deg,
            outboard_le_sweep_deg,
        };
        if let Some(side_of_body) = planform.side_of_body {
            validate_positive("side-of-body chord", side_of_body.chord_m)?;
        }
        Ok(planform)
    }

    /// Return the exposed inboard section for a 2-D aerodynamic calculation.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when the parent planform is not
    /// physically constructible.
    pub fn inboard_aerodynamic_station(
        &self,
        design: &DesignVector,
    ) -> Result<InboardAerodynamicStation, TransportPlanformError> {
        let planform = self.transport_planform(design)?;
        if let Some(side_of_body) = planform.side_of_body {
            let root_to_kink_fraction = side_of_body.y_m / planform.kink.y_m;
            return Ok(InboardAerodynamicStation {
                y_m: side_of_body.y_m,
                leading_edge_x_m: side_of_body.leading_edge_x_m,
                chord_m: side_of_body.chord_m,
                twist_deg: self.root_twist_deg
                    + root_to_kink_fraction * (self.break_twist_deg - self.root_twist_deg),
            });
        }
        Ok(InboardAerodynamicStation {
            y_m: planform.root.y_m,
            leading_edge_x_m: planform.root.leading_edge_x_m,
            chord_m: planform.root.chord_m,
            twist_deg: self.root_twist_deg,
        })
    }
}

fn validate_finite(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(TransportPlanformError::NonFinite { field, value })
    }
}

fn validate_positive(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value > 0.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::NonPositive { field, value })
    }
}

fn validate_sweep(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value.abs() < 89.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::InvalidSweep { field, value })
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, OptionSource};

    #[test]
    fn the_default_wing_washes_out_from_root_to_tip() {
        // Washout is what keeps the tip from stalling before the root, which
        // is what keeps the ailerons working through the stall.
        let wing = WingConfig::default();
        assert!(wing.root_twist_deg > wing.break_twist_deg);
    }

    #[test]
    fn the_default_wing_has_positive_dihedral() {
        let wing = WingConfig::default();
        assert!(wing.tip_z_m > wing.root_z_m);
    }

    #[test]
    fn the_break_sits_strictly_between_the_root_and_the_tip() {
        // At 0 or 1 the crank collapses onto a defining section and the
        // outboard sweep decrement has nothing to apply to.
        let wing = WingConfig::default();
        assert!(wing.break_span_fraction > 0.0);
        assert!(wing.break_span_fraction < 1.0);
    }

    #[test]
    fn the_product_defaults_derive_a_collinear_side_of_body_station() {
        let wing = WingConfig::default();
        let design = DesignVector::default();
        let planform = wing
            .transport_planform(&design)
            .expect("the default planform is physically valid");

        assert!(planform.side_of_body.is_some());
        assert_eq!(planform.kink.span_fraction, 0.37);
        assert_eq!(planform.stations().len(), 4);
        assert_eq!(planform.panels().len(), 3);
        assert_eq!(planform.inboard_le_sweep_deg, design.sweep_deg);
        assert_eq!(planform.outboard_le_sweep_deg, design.sweep_deg);
        let side_of_body = planform.side_of_body.expect("the station is active");
        let fraction = side_of_body.y_m / planform.kink.y_m;
        let expected_chord =
            planform.root.chord_m + fraction * (planform.kink.chord_m - planform.root.chord_m);
        assert!((side_of_body.chord_m - expected_chord).abs() < 1e-12);
    }

    #[test]
    fn legacy_saved_wing_configuration_uses_the_pre_transport_planform() {
        let mut value = serde_json::to_value(WingConfig::default()).unwrap();
        let object = value
            .as_object_mut()
            .expect("a wing configuration serializes as an object");
        for field in [
            "side_of_body_span_fraction",
            "side_of_body_chord_ratio",
            "kink_span_fraction",
            "outboard_le_sweep_deg",
        ] {
            object.remove(field);
        }
        let legacy: WingConfig = serde_json::from_value(value)
            .expect("new optional transport fields do not invalidate saved configurations");
        let design = DesignVector::default();
        let planform = legacy
            .transport_planform(&design)
            .expect("the legacy planform remains valid");

        assert!(planform.side_of_body.is_none());
        assert_eq!(planform.kink.span_fraction, legacy.break_span_fraction);
        assert_eq!(
            planform.outboard_le_sweep_deg,
            design.sweep_deg - legacy.outboard_sweep_decrement_deg
        );
    }

    #[test]
    fn explicit_transport_stations_cannot_create_a_second_leading_edge_sweep() {
        let wing = WingConfig {
            side_of_body_span_fraction: Some(0.10),
            side_of_body_chord_ratio: Some(0.90),
            kink_span_fraction: Some(0.40),
            outboard_le_sweep_deg: Some(28.0),
            ..WingConfig::default()
        };
        let design = DesignVector::default();

        let planform = wing
            .transport_planform(&design)
            .expect("the configured transport planform is valid");
        let panels = planform.panels();

        assert_eq!(planform.stations().len(), 4);
        assert_eq!(panels.len(), 3);
        assert_eq!(planform.kink.span_fraction, 0.40);
        assert_eq!(planform.kink.y_m, 0.40 * design.span_m / 2.0);
        assert!((panels[0].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
        assert!((panels[2].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
        assert!(panels[1].trailing_edge_sweep_deg < panels[1].leading_edge_sweep_deg);
        assert!(panels[2].trailing_edge_sweep_deg < panels[2].leading_edge_sweep_deg);
    }

    #[test]
    fn inboard_aerodynamic_station_uses_side_of_body_chord_and_interpolated_twist() {
        let wing = WingConfig::default();
        let design = DesignVector::default();
        let planform = wing
            .transport_planform(&design)
            .expect("the default transport planform is valid");
        let side_of_body = planform
            .side_of_body
            .expect("product default has a side-of-body station");
        let station = wing
            .inboard_aerodynamic_station(&design)
            .expect("the default inboard station is valid");
        let expected_twist_deg = wing.root_twist_deg
            + (side_of_body.y_m / planform.kink.y_m) * (wing.break_twist_deg - wing.root_twist_deg);

        assert_eq!(station.y_m, side_of_body.y_m);
        assert_eq!(station.chord_m, side_of_body.chord_m);
        assert!((station.twist_deg - expected_twist_deg).abs() < 1e-12);
    }

    #[test]
    fn legacy_inboard_aerodynamic_station_remains_at_the_centerline_root() {
        let wing = WingConfig {
            side_of_body_span_fraction: None,
            side_of_body_chord_ratio: None,
            ..WingConfig::default()
        };
        let design = DesignVector::default();
        let station = wing
            .inboard_aerodynamic_station(&design)
            .expect("the legacy inboard station is valid");

        assert_eq!(station.y_m, 0.0);
        assert_eq!(station.chord_m, design.root_chord_m);
        assert_eq!(station.twist_deg, wing.root_twist_deg);
    }

    #[test]
    fn unconventional_reverse_taper_is_not_mislabeled_as_nonphysical() {
        let wing = WingConfig::default();
        let design = DesignVector {
            break_chord_m: 17.0,
            sweep_deg: 40.0,
            ..DesignVector::default()
        };

        assert!(wing.transport_planform(&design).is_ok());
    }

    #[test]
    fn a_forward_swept_trailing_edge_is_a_valid_tapered_wing() {
        let wing = WingConfig {
            side_of_body_chord_ratio: Some(1.0),
            ..WingConfig::default()
        };

        assert!(wing.transport_planform(&DesignVector::default()).is_ok());
    }

    #[test]
    fn a_planform_query_outboard_of_the_tip_is_a_typed_error() {
        let planform = WingConfig::default()
            .transport_planform(&DesignVector::default())
            .expect("the default planform is valid");

        assert!(matches!(
            planform.leading_edge_x_at(planform.tip.y_m + 0.01),
            Err(TransportPlanformError::SpanwisePositionOutsidePlanform { .. })
        ));
    }

    #[test]
    fn both_section_fields_offer_the_airfoil_library_and_still_accept_a_naca_code() {
        // The geometry layer resolves any NACA 4-digit code without the
        // library carrying it, so a strict list would reject valid input.
        for name in ["root_airfoil", "tip_airfoil"] {
            let schema = WingConfig::default().schema();
            let Entry::Leaf(leaf) = &schema.field(name).unwrap().entry else {
                panic!("{name} is not a group");
            };
            assert_eq!(leaf.options, Some(OptionSource::Airfoil));
        }
        assert!(OptionSource::Airfoil.editable());
    }
}

