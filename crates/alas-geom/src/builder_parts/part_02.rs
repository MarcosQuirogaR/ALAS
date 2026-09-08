// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// `n_subdivisions` clamped to `usize`, so a negative or overflowing
/// configuration value becomes `0` -- which [`Wing::subdivide_sections`]
/// rejects with [`SubdivideSectionsError::RatioTooSmall`], the same outcome
/// Python's `ratio < 2` guard produces for a negative `ratio` -- rather than
/// wrapping to a huge unsigned value on the `as` cast.
fn n_subdivisions_usize(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// Evenly spaced points from `start` to `stop`, inclusive -- NumPy's
/// `linspace(start, stop, num, endpoint=True)`. Duplicated from
/// `aircraft::spacing::linspace`; see the module doc.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// Sine-spaced points from `start` to `stop`, bunched near `start` --
/// `native aerodynamic model.numpy.spacing.sinspace` at its default `reverse_spacing =
/// False`: `start + (stop - start) * (1 - cos(linspace(0, pi/2, num)))`, with
/// both endpoints then forced exact to correct the trigonometric round trip,
/// exactly as upstream's own endpoint fixup does.
fn sinspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    let mut spaced: Vec<f64> = linspace(0.0, std::f64::consts::FRAC_PI_2, num)
        .into_iter()
        .map(|t| start + (stop - start) * (1.0 - t.cos()))
        .collect();
    spaced[0] = start;
    let last = spaced.len() - 1;
    spaced[last] = stop;
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluding_engines_leaves_exactly_one_fuselage() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let airplane = builder
            .build(None, false)
            .expect("the default aircraft builds cleanly");
        assert_eq!(airplane.fuselages.len(), 1);
        assert_eq!(airplane.fuselages[0].name, "Fuselage");
    }

    #[test]
    fn including_engines_appends_one_nacelle_per_spanwise_position() {
        let geometry = GeometryConfig::default();
        let expected_engine_count = geometry.engine.spanwise_positions_m.len();
        let builder = AircraftBuilder::new(Some(geometry));
        let airplane = builder
            .build(None, true)
            .expect("the default aircraft builds cleanly");
        assert_eq!(airplane.fuselages.len(), 1 + expected_engine_count);
    }

    #[test]
    fn a_centerline_engine_is_named_and_placed_on_the_tailcone() {
        // No default configuration reaches `y_pos == 0.0` -- see the module
        // doc -- so this is exercised directly on a synthetic geometry.
        let mut geometry = GeometryConfig::default();
        geometry.engine.spanwise_positions_m = vec![0.0];
        let builder = AircraftBuilder::new(Some(geometry));
        let dv = DesignVector::default();

        let nacelles = builder
            .build_engines(&dv)
            .expect("a single centerline engine builds cleanly");
        assert_eq!(nacelles.len(), 1);
        assert_eq!(nacelles[0].name, "Nacelle Center");

        let expected_z =
            builder.geometry.fuselage.tail_z_m + builder.geometry.engine.radius_scale_m + 0.2;
        // The nacelle's own xsecs are all at local x/y/z == 0 before
        // translation, so its first xsec's absolute Z is the translation.
        assert!((nacelles[0].xsecs[0].xyz_c[2] - expected_z).abs() < 1e-9);
    }

    #[test]
    fn wing_mounted_engines_are_named_by_which_side_they_are_on() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let dv = DesignVector::default();
        let nacelles = builder
            .build_engines(&dv)
            .expect("the default two wing-mounted engines build cleanly");
        assert_eq!(nacelles.len(), 2);
        assert_eq!(nacelles[0].name, "Nacelle R");
        assert_eq!(nacelles[1].name, "Nacelle L");
    }

    #[test]
    fn aircraft_reference_axes_use_the_projected_main_wing_convention() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let airplane = builder
            .build(None, false)
            .expect("the default product aircraft builds");
        let main_wing = &airplane.wings[0];

        assert_eq!(airplane.s_ref, main_wing.reference_area());
        assert_eq!(airplane.b_ref, main_wing.reference_span());
        assert!((airplane.s_ref - main_wing.area()).abs() > 1.0e-6);
        assert!((airplane.b_ref - main_wing.span()).abs() > 1.0e-6);
    }

    #[test]
    fn an_outboard_nacelle_follows_the_continuous_leading_edge_sweep() {
        let mut geometry = GeometryConfig::default();
        let dv = DesignVector::default();
        let semi_span = dv.span_m / 2.0;
        let y_outboard = 0.8 * semi_span;
        geometry.engine.spanwise_positions_m = vec![y_outboard];
        let builder = AircraftBuilder::new(Some(geometry));
        let planform = builder
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the default planform is valid");

        let nacelles = builder
            .build_engines(&dv)
            .expect("outboard nacelle placement is valid");
        let expected = builder.geometry.wing.root_datum_x_m
            + planform
                .leading_edge_x_at(y_outboard)
                .expect("the nacelle is inside the planform")
            - builder.geometry.engine.inlet_x_offset_m;
        let inboard_extrapolation = builder.geometry.wing.root_datum_x_m
            + y_outboard * dv.sweep_deg.to_radians().tan()
            - builder.geometry.engine.inlet_x_offset_m;

        assert!(y_outboard > planform.kink.y_m);
        assert!((nacelles[0].xsecs[0].xyz_c[0] - expected).abs() < 1e-12);
        assert!((expected - inboard_extrapolation).abs() < 1e-12);
    }

    #[test]
    fn explicit_side_of_body_station_is_lofted_into_the_main_wing() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.side_of_body_span_fraction = Some(0.10);
        geometry.wing.side_of_body_chord_ratio = Some(0.90);
        geometry.wing.kink_span_fraction = Some(0.40);
        geometry.wing.outboard_le_sweep_deg = Some(28.0);
        geometry.wing.n_subdivisions = 2;
        let builder = AircraftBuilder::new(Some(geometry));
        let dv = DesignVector::default();
        let planform = builder
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the transport planform is valid");

        let airplane = builder
            .build(Some(&dv), false)
            .expect("the explicit transport planform builds");
        let main_wing = &airplane.wings[0];

        // Three original lofted panels at a subdivision ratio of two yield
        // six sections plus the unchanged tip. The side-of-body station is
        // the first section of the second panel.
        assert_eq!(main_wing.xsecs.len(), 7);
        assert!((main_wing.xsecs[2].xyz_le[1] - planform.side_of_body.unwrap().y_m).abs() < 1e-12);
        assert!((main_wing.xsecs[2].chord - planform.side_of_body.unwrap().chord_m).abs() < 1e-12);
        assert!((main_wing.xsecs[4].xyz_le[1] - planform.kink.y_m).abs() < 1e-12);
        assert!(
            (main_wing.xsecs[6].xyz_le[0]
                - (builder.geometry.wing.root_datum_x_m + planform.tip.leading_edge_x_m))
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn a_collinear_side_of_body_station_does_not_add_a_vlm_subdivision_block() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.n_subdivisions = 2;
        let builder = AircraftBuilder::new(Some(geometry));

        let airplane = builder
            .build(Some(&DesignVector::default()), false)
            .expect("the default transport planform builds");

        // Root/kink/tip is two physical panels. The derived side-of-body
        // station lies on the inboard panel and therefore adds no mesh block.
        assert_eq!(airplane.wings[0].xsecs.len(), 5);
    }

    #[test]
    fn reference_compatibility_retains_the_frozen_nacelle_station() {
        let dv = DesignVector {
            span_m: 35.8,
            root_chord_m: 6.5,
            break_chord_m: 4.2,
            tip_chord_m: 1.8,
            sweep_deg: 25.0,
            fuselage_length_m: 37.5,
            ..DesignVector::default()
        };
        let reference =
            AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
        let y_outboard = reference.geometry.engine.spanwise_positions_m[0].abs();
        let reference_inlet = reference
            .build_engines(&dv)
            .expect("reference nacelles build")[0]
            .xsecs[0]
            .xyz_c[0];
        let frozen_inlet = reference.geometry.wing.root_datum_x_m
            + y_outboard * dv.sweep_deg.to_radians().tan()
            - reference.geometry.engine.inlet_x_offset_m;

        assert!((reference_inlet - frozen_inlet).abs() < 1e-12);
    }

    #[test]
    fn product_and_reference_builders_keep_distinct_planform_contracts() {
        let dv = DesignVector::default();
        let product = AircraftBuilder::new(Some(GeometryConfig::default()));
        let reference =
            AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));

        let product_planform = product
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the product planform is valid");
        let reference_planform = reference
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the frozen reference planform is valid");
        assert!(
            (product_planform.kink.span_fraction - reference_planform.kink.span_fraction).abs()
                > 1e-12,
            "product and frozen reference kink stations must not be conflated"
        );
        assert!(
            (product_planform.outboard_le_sweep_deg - reference_planform.outboard_le_sweep_deg)
                .abs()
                > 1e-12,
            "product and frozen reference outboard sweep must remain distinct"
        );

        let product_airplane = product
            .build(Some(&dv), false)
            .expect("the product aircraft builds");
        let reference_airplane = reference
            .build(Some(&dv), false)
            .expect("the frozen reference aircraft builds");
        let product_wing = &product_airplane.wings[0];
        let reference_wing = &reference_airplane.wings[0];
        assert!(
            (product_wing.mean_aerodynamic_chord() - reference_wing.mean_aerodynamic_chord()).abs()
                > 1e-12,
            "built product geometry must not silently replay the frozen reference geometry"
        );
    }

    #[test]
    fn sinspace_bunches_points_near_the_start() {
        let values = sinspace(0.0, 1.0, 11);
        assert_eq!(values[0], 0.0);
        assert_eq!(values[10], 1.0);
        let first_gap = values[1] - values[0];
        let last_gap = values[10] - values[9];
        assert!(first_gap < last_gap, "{first_gap} >= {last_gap}");
    }

    #[test]
    fn product_builder_preserves_every_live_engine_and_nacelle_field() {
        let mut geometry = GeometryConfig::default();
        let engine = &mut geometry.engine;
        engine.engine_name = "Trent 900".to_owned();
        engine.nacelle_profile = vec![(0.0, 0.31), (2.3, 0.97), (6.4, 0.42)];
        engine.radius_scale_m = 1.93;
        engine.spanwise_positions_m = vec![-8.7, 8.7];
        engine.z_m = -2.41;
        engine.inlet_x_offset_m = 3.67;
        engine.turbofan.as_mut().unwrap().rated_thrust_kn = 401.2;
        engine.bypass_ratio = 9.31;
        engine.overall_pressure_ratio = 42.7;
        engine.fan_pressure_ratio = 1.61;
        engine.turbine_inlet_temp_k = 1734.0;
        engine.cruise_tsfc_kg_kgf_hr = 0.487;
        engine.fan_diameter_m = 3.08;
        let expected = engine.clone();

        let builder = AircraftBuilder::new(Some(geometry));

        assert_eq!(builder.geometry.engine, expected);
        let airplane = builder
            .build(Some(&DesignVector::default()), true)
            .expect("edited live engine geometry builds");
        let nacelle = &airplane.fuselages[1];
        assert_eq!(nacelle.xsecs.len(), expected.nacelle_profile.len());
        let built_length = nacelle.xsecs.last().unwrap().xyz_c[0] - nacelle.xsecs[0].xyz_c[0];
        assert!((built_length - expected.nacelle_length_m()).abs() < 1.0e-12);
    }

    #[test]
    fn reference_builder_still_resolves_the_named_database_engine() {
        let mut geometry = GeometryConfig::default();
        geometry.engine.engine_name = "Trent 900".to_owned();
        geometry.engine.turbofan.as_mut().unwrap().rated_thrust_kn = 1.0;

        let builder = AircraftBuilder::new_reference_compatibility(Some(geometry));
        let spec = alas_config::engines::get("Trent 900").unwrap();

        assert_eq!(builder.geometry.engine.thrust_kn(), spec.thrust_kn);
        assert_eq!(
            builder.geometry.engine.nacelle_profile,
            spec.nacelle_profile()
        );
    }

    #[test]
    fn n_subdivisions_usize_clamps_a_negative_configuration_value_to_zero() {
        assert_eq!(n_subdivisions_usize(-1), 0);
        assert_eq!(n_subdivisions_usize(8), 8);
    }

    #[test]
    fn a_ratio_below_two_from_configuration_is_a_build_error_not_a_panic() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.n_subdivisions = 1;
        let builder = AircraftBuilder::new(Some(geometry));
        let error = builder
            .build(None, false)
            .expect_err("n_subdivisions=1 must be rejected, not silently truncated");
        assert!(matches!(error, BuildError::Subdivide(_)));
    }
}

