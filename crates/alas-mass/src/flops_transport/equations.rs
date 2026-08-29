// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closed-form FLOPS transport equations in their published imperial units.

use alas_units::{FOOT, POUND_FORCE, POUND_MASS, PSI};

use super::{
    FlopsOperatingItemsBreakdown, FlopsSystemsBreakdown, FlopsTransportBreakdown,
    FlopsTransportInputError, FlopsTransportInputs,
};

fn pounds_to_kg(value: f64) -> f64 {
    value * POUND_MASS
}

pub(super) fn surface_controls_kg(maximum_mach: f64, area_m2: f64, gross_kg: f64) -> f64 {
    let area_ft2 = area_m2 / FOOT.powi(2);
    let gross_lb = gross_kg / POUND_MASS;
    pounds_to_kg(1.1 * maximum_mach.powf(0.52) * area_ft2.powf(0.6) * gross_lb.powf(0.32))
}

pub(super) fn apu_kg(fuselage_planform_area_m2: f64, passengers: usize) -> f64 {
    let area_ft2 = fuselage_planform_area_m2 / FOOT.powi(2);
    pounds_to_kg(54.0 * area_ft2.powf(0.3) + 5.4 * (passengers as f64).powf(0.9))
}

pub(super) fn instruments_kg(
    fuselage_planform_area_m2: f64,
    maximum_mach: f64,
    flight_crew: usize,
    wing_engines: usize,
    fuselage_engines: usize,
) -> f64 {
    let area_ft2 = fuselage_planform_area_m2 / FOOT.powi(2);
    pounds_to_kg(
        0.48 * area_ft2.powf(0.57)
            * maximum_mach.sqrt()
            * (10.0
                + 2.5 * flight_crew as f64
                + wing_engines as f64
                + 1.5 * fuselage_engines as f64),
    )
}

pub(super) fn hydraulics_kg(
    fuselage_planform_area_m2: f64,
    wing_area_m2: f64,
    wing_engines: usize,
    fuselage_engines: usize,
    hydraulic_pressure_pa: f64,
    variable_sweep_penalty: f64,
    maximum_mach: f64,
) -> f64 {
    let fuselage_area_ft2 = fuselage_planform_area_m2 / FOOT.powi(2);
    let wing_area_ft2 = wing_area_m2 / FOOT.powi(2);
    let pressure_psi = hydraulic_pressure_pa / PSI;
    pounds_to_kg(
        0.57 * (fuselage_area_ft2 + 0.27 * wing_area_ft2)
            * (1.0 + 0.03 * wing_engines as f64 + 0.05 * fuselage_engines as f64)
            * (3000.0 / pressure_psi).powf(0.35)
            * (1.0 + 0.04 * variable_sweep_penalty)
            * maximum_mach.powf(0.33),
    )
}

pub(super) fn electrical_kg(
    fuselage_length_m: f64,
    fuselage_width_m: f64,
    fuselage_count: usize,
    engine_count: usize,
    flight_crew: usize,
    passengers: usize,
) -> f64 {
    pounds_to_kg(
        92.0 * (fuselage_length_m / FOOT).powf(0.4)
            * (fuselage_width_m / FOOT).powf(0.14)
            * (fuselage_count as f64).powf(0.27)
            * (engine_count as f64).powf(0.69)
            * (1.0 + 0.044 * flight_crew as f64 + 0.0015 * passengers as f64),
    )
}

pub(super) fn avionics_kg(
    design_range_nmi: f64,
    flight_crew: usize,
    fuselage_planform_area_m2: f64,
) -> f64 {
    let area_ft2 = fuselage_planform_area_m2 / FOOT.powi(2);
    pounds_to_kg(
        15.8 * design_range_nmi.powf(0.1) * (flight_crew as f64).powf(0.7) * area_ft2.powf(0.43),
    )
}

// FLOPS Eq. 110 uses each cabin class and both fuselage dimensions explicitly;
// bundling them would hide the source equation's one-to-one inputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn furnishings_kg(
    flight_crew: usize,
    first: usize,
    business: usize,
    tourist: usize,
    passenger_compartment_length_m: f64,
    fuselage_width_m: f64,
    fuselage_depth_m: f64,
    fuselage_count: usize,
) -> f64 {
    pounds_to_kg(
        127.0 * flight_crew as f64
            + 112.0 * first as f64
            + 78.0 * business as f64
            + 44.0 * tourist as f64
            + 2.6
                * (passenger_compartment_length_m / FOOT)
                * ((fuselage_width_m + fuselage_depth_m) / FOOT)
                * fuselage_count as f64,
    )
}

pub(super) fn air_conditioning_kg(
    fuselage_planform_area_m2: f64,
    fuselage_depth_m: f64,
    passengers: usize,
    maximum_mach: f64,
    avionics_kg_value: f64,
) -> f64 {
    let area_ft2 = fuselage_planform_area_m2 / FOOT.powi(2);
    let depth_ft = fuselage_depth_m / FOOT;
    let avionics_lb = avionics_kg_value / POUND_MASS;
    pounds_to_kg(
        (3.2 * (area_ft2 * depth_ft).powf(0.6) + 9.0 * (passengers as f64).powf(0.83))
            * maximum_mach
            + 0.075 * avionics_lb,
    )
}

pub(super) fn anti_ice_kg(
    span_m: f64,
    sweep_deg: f64,
    nacelle_diameter_m: f64,
    engine_count: usize,
    fuselage_width_m: f64,
) -> f64 {
    pounds_to_kg(
        (span_m / FOOT) / sweep_deg.to_radians().cos()
            + 3.8 * (nacelle_diameter_m / FOOT) * engine_count as f64
            + 1.5 * (fuselage_width_m / FOOT),
    )
}

pub(super) fn cabin_crew_and_baggage_kg(attendants: usize, galley_crew: usize) -> f64 {
    pounds_to_kg(155.0 * attendants as f64 + 200.0 * galley_crew as f64)
}

pub(super) fn flight_crew_and_baggage_kg(flight_crew: usize) -> f64 {
    pounds_to_kg(225.0 * flight_crew as f64)
}

pub(super) fn unusable_fuel_kg(
    engine_count: usize,
    thrust_n: f64,
    wing_area_m2: f64,
    tank_count: usize,
    maximum_fuel_capacity_kg: f64,
) -> f64 {
    pounds_to_kg(
        11.5 * engine_count as f64 * (thrust_n / POUND_FORCE).powf(0.2)
            + 0.07 * (wing_area_m2 / FOOT.powi(2))
            + 1.6 * tank_count as f64 * (maximum_fuel_capacity_kg / POUND_MASS).powf(0.28),
    )
}

pub(super) fn engine_oil_kg(engine_count: usize, thrust_n: f64) -> f64 {
    pounds_to_kg(0.082 * engine_count as f64 * (thrust_n / POUND_FORCE).powf(0.65))
}

pub(super) fn passenger_service_kg(
    first: usize,
    business: usize,
    tourist: usize,
    design_range_nmi: f64,
    maximum_mach: f64,
) -> f64 {
    pounds_to_kg(
        (5.164 * first as f64 + 3.846 * business as f64 + 2.529 * tourist as f64)
            * (design_range_nmi / maximum_mach).powf(0.225),
    )
}

pub(super) fn cargo_containers_kg(containerized_cargo_kg: f64) -> f64 {
    let containers = ((containerized_cargo_kg / POUND_MASS) / 950.0).ceil();
    pounds_to_kg(175.0 * containers)
}

fn validate_positive(value: f64, field: &'static str) -> Result<(), FlopsTransportInputError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(FlopsTransportInputError { field })
    }
}

fn validate(inputs: &FlopsTransportInputs) -> Result<(), FlopsTransportInputError> {
    for (value, field) in [
        (inputs.maximum_mach, "maximum_mach"),
        (inputs.design_range_nmi, "design_range_nmi"),
        (inputs.design_gross_mass_kg, "design_gross_mass_kg"),
        (inputs.wing_area_m2, "wing_area_m2"),
        (inputs.movable_surface_area_m2, "movable_surface_area_m2"),
        (inputs.wing_span_m, "wing_span_m"),
        (inputs.fuselage_length_m, "fuselage_length_m"),
        (inputs.fuselage_width_m, "fuselage_width_m"),
        (inputs.fuselage_depth_m, "fuselage_depth_m"),
        (
            inputs.passenger_compartment_length_m,
            "passenger_compartment_length_m",
        ),
        (
            inputs.rated_thrust_per_engine_n,
            "rated_thrust_per_engine_n",
        ),
        (inputs.nacelle_diameter_m, "nacelle_diameter_m"),
        (inputs.hydraulic_pressure_pa, "hydraulic_pressure_pa"),
        (inputs.maximum_fuel_capacity_kg, "maximum_fuel_capacity_kg"),
    ] {
        validate_positive(value, field)?;
    }
    if !inputs.quarter_chord_sweep_deg.is_finite() || inputs.quarter_chord_sweep_deg.abs() >= 90.0 {
        return Err(FlopsTransportInputError {
            field: "quarter_chord_sweep_deg",
        });
    }
    if !(0.0..=1.0).contains(&inputs.variable_sweep_penalty) {
        return Err(FlopsTransportInputError {
            field: "variable_sweep_penalty",
        });
    }
    if inputs.fuselage_count == 0 {
        return Err(FlopsTransportInputError {
            field: "fuselage_count",
        });
    }
    if inputs.flight_crew_count == 0 {
        return Err(FlopsTransportInputError {
            field: "flight_crew_count",
        });
    }
    if inputs.engine_count == 0
        || inputs.wing_mounted_engine_count + inputs.fuselage_mounted_engine_count
            != inputs.engine_count
    {
        return Err(FlopsTransportInputError {
            field: "engine_mounting",
        });
    }
    if inputs.fuel_tank_count == 0 {
        return Err(FlopsTransportInputError {
            field: "fuel_tank_count",
        });
    }
    if !inputs.containerized_cargo_kg.is_finite() || inputs.containerized_cargo_kg < 0.0 {
        return Err(FlopsTransportInputError {
            field: "containerized_cargo_kg",
        });
    }
    Ok(())
}

/// Evaluate NASA/TM-2017-219627 Vol. I transport equations 97, 101-106, 108,
/// 110, 113, and 115-126 from complete SI inputs.
///
/// # Errors
///
/// Returns [`FlopsTransportInputError`] before evaluating a power law when an
/// input is nonfinite, outside its physical range, or internally inconsistent.
pub fn estimate_flops_transport(
    inputs: &FlopsTransportInputs,
) -> Result<FlopsTransportBreakdown, FlopsTransportInputError> {
    validate(inputs)?;

    let fuselage_planform_area_m2 =
        inputs.fuselage_count as f64 * inputs.fuselage_length_m * inputs.fuselage_width_m;
    let passengers = inputs.passenger_count();
    let surface_controls = surface_controls_kg(
        inputs.maximum_mach,
        inputs.movable_surface_area_m2,
        inputs.design_gross_mass_kg,
    );
    let apu = apu_kg(fuselage_planform_area_m2, passengers);
    let instruments = instruments_kg(
        fuselage_planform_area_m2,
        inputs.maximum_mach,
        inputs.flight_crew_count,
        inputs.wing_mounted_engine_count,
        inputs.fuselage_mounted_engine_count,
    );
    let hydraulics = hydraulics_kg(
        fuselage_planform_area_m2,
        inputs.wing_area_m2,
        inputs.wing_mounted_engine_count,
        inputs.fuselage_mounted_engine_count,
        inputs.hydraulic_pressure_pa,
        inputs.variable_sweep_penalty,
        inputs.maximum_mach,
    );
    let electrical = electrical_kg(
        inputs.fuselage_length_m,
        inputs.fuselage_width_m,
        inputs.fuselage_count,
        inputs.engine_count,
        inputs.flight_crew_count,
        passengers,
    );
    let avionics = avionics_kg(
        inputs.design_range_nmi,
        inputs.flight_crew_count,
        fuselage_planform_area_m2,
    );
    let furnishings = furnishings_kg(
        inputs.flight_crew_count,
        inputs.first_class_passenger_count,
        inputs.business_class_passenger_count,
        inputs.tourist_class_passenger_count,
        inputs.passenger_compartment_length_m,
        inputs.fuselage_width_m,
        inputs.fuselage_depth_m,
        inputs.fuselage_count,
    );
    let air_conditioning = air_conditioning_kg(
        fuselage_planform_area_m2,
        inputs.fuselage_depth_m,
        passengers,
        inputs.maximum_mach,
        avionics,
    );
    let anti_ice = anti_ice_kg(
        inputs.wing_span_m,
        inputs.quarter_chord_sweep_deg,
        inputs.nacelle_diameter_m,
        inputs.engine_count,
        inputs.fuselage_width_m,
    );

    let systems = FlopsSystemsBreakdown {
        surface_controls_kg: surface_controls,
        apu_kg: apu,
        instruments_kg: instruments,
        hydraulics_kg: hydraulics,
        electrical_kg: electrical,
        avionics_kg: avionics,
        furnishings_kg: furnishings,
        air_conditioning_kg: air_conditioning,
        anti_ice_kg: anti_ice,
        total_kg: surface_controls
            + apu
            + instruments
            + hydraulics
            + electrical
            + avionics
            + furnishings
            + air_conditioning
            + anti_ice,
    };

    // Equations 119-126. Counts are declared inputs rather than equation
    // 116-118 defaults because the product contract represents an installed
    // cabin and crew, not an unspecified FLOPS study.
    let cabin_crew_and_baggage =
        cabin_crew_and_baggage_kg(inputs.flight_attendant_count, inputs.galley_crew_count);
    let flight_crew_and_baggage = flight_crew_and_baggage_kg(inputs.flight_crew_count);
    let unusable_fuel = unusable_fuel_kg(
        inputs.engine_count,
        inputs.rated_thrust_per_engine_n,
        inputs.wing_area_m2,
        inputs.fuel_tank_count,
        inputs.maximum_fuel_capacity_kg,
    );
    let engine_oil = engine_oil_kg(inputs.engine_count, inputs.rated_thrust_per_engine_n);
    let passenger_service = passenger_service_kg(
        inputs.first_class_passenger_count,
        inputs.business_class_passenger_count,
        inputs.tourist_class_passenger_count,
        inputs.design_range_nmi,
        inputs.maximum_mach,
    );
    let cargo_containers = cargo_containers_kg(inputs.containerized_cargo_kg);
    let operating_items = FlopsOperatingItemsBreakdown {
        cabin_crew_and_baggage_kg: cabin_crew_and_baggage,
        flight_crew_and_baggage_kg: flight_crew_and_baggage,
        unusable_fuel_kg: unusable_fuel,
        engine_oil_kg: engine_oil,
        passenger_service_kg: passenger_service,
        cargo_containers_kg: cargo_containers,
        total_kg: cabin_crew_and_baggage
            + flight_crew_and_baggage
            + unusable_fuel
            + engine_oil
            + passenger_service
            + cargo_containers,
    };

    Ok(FlopsTransportBreakdown {
        systems,
        operating_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn representative_inputs() -> FlopsTransportInputs {
        FlopsTransportInputs {
            maximum_mach: 0.84,
            design_range_nmi: 3_000.0,
            design_gross_mass_kg: 250_000.0,
            wing_area_m2: 360.0,
            movable_surface_area_m2: 75.0,
            wing_span_m: 60.0,
            quarter_chord_sweep_deg: 25.0,
            fuselage_length_m: 60.0,
            fuselage_width_m: 5.8,
            fuselage_depth_m: 6.0,
            fuselage_count: 1,
            passenger_compartment_length_m: 35.0,
            first_class_passenger_count: 0,
            business_class_passenger_count: 30,
            tourist_class_passenger_count: 260,
            flight_crew_count: 2,
            flight_attendant_count: 8,
            galley_crew_count: 1,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            engine_count: 2,
            rated_thrust_per_engine_n: 250_000.0,
            nacelle_diameter_m: 3.2,
            hydraulic_pressure_pa: 20_684_271.879_504,
            variable_sweep_penalty: 0.0,
            maximum_fuel_capacity_kg: 100_000.0,
            fuel_tank_count: 4,
            containerized_cargo_kg: 0.0,
        }
    }

    #[test]
    fn avionics_and_electrical_masses_follow_declared_architecture_at_fixed_mtow() {
        let baseline = representative_inputs();
        let mut long_range_quad = baseline;
        long_range_quad.design_range_nmi = 6_000.0;
        long_range_quad.wing_mounted_engine_count = 4;
        long_range_quad.engine_count = 4;
        long_range_quad.rated_thrust_per_engine_n = 180_000.0;

        let first = estimate_flops_transport(&baseline).expect("representative FLOPS inputs");
        let second =
            estimate_flops_transport(&long_range_quad).expect("altered FLOPS inputs remain valid");

        assert_eq!(
            baseline.design_gross_mass_kg,
            long_range_quad.design_gross_mass_kg
        );
        assert_ne!(first.systems.avionics_kg, second.systems.avionics_kg);
        assert_ne!(first.systems.electrical_kg, second.systems.electrical_kg);
    }

    #[test]
    fn a_widebody_long_range_architecture_has_more_avionics_than_a_narrowbody() {
        // Holding MTOW and flight-deck crew fixed isolates the architecture
        // inputs in FLOPS Eq. 108. The fuselage dimensions mirror the
        // registered A220-300 and B787-9 geometry presets; the range contrast
        // is deliberately synthetic because the B787 design-mission record is
        // unverified and must not be filled from a marketing value.
        let mut narrowbody = representative_inputs();
        narrowbody.design_range_nmi = 3_400.0;
        narrowbody.fuselage_length_m = 38.7;
        narrowbody.fuselage_width_m = 3.5;

        let mut widebody = narrowbody;
        widebody.design_range_nmi *= 2.0;
        widebody.fuselage_length_m = 62.81;
        widebody.fuselage_width_m = 5.94;

        let narrowbody_mass = estimate_flops_transport(&narrowbody)
            .expect("A220-like FLOPS inputs are valid")
            .systems
            .avionics_kg;
        let widebody_mass = estimate_flops_transport(&widebody)
            .expect("787-like FLOPS inputs are valid")
            .systems
            .avionics_kg;

        assert_eq!(
            narrowbody.design_gross_mass_kg,
            widebody.design_gross_mass_kg
        );
        assert!(
            widebody_mass > narrowbody_mass,
            "widebody avionics {widebody_mass} kg must exceed narrowbody {narrowbody_mass} kg"
        );
    }
}
