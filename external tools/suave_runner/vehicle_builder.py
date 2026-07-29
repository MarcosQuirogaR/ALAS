# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Parameterized SUAVE vehicle builder.

A generalization of ``suave_example.py``'s ``vehicle_setup()`` -- that function
hardcoded every dimension to the AVE/GE9X reference aircraft. This module
builds the same kind of SUAVE ``Vehicle`` (simple trapezoidal main wing +
h-stab + v-stab, circular fuselage, podded turbofan nacelles) but reads every
dimension from a flat ``vehicle_request`` dict supplied by
``alas.integration.suave_vehicle.build_vehicle_request`` -- so it works
for any ALAS preset/optimized design, not just AVE.

Where ALAS doesn't model a parameter SUAVE needs (e.g. high-lift device
span fractions, individual compressor pressure ratios), this keeps the same
generic textbook-level defaults ``suave_example.py`` used, scaled by the one
or two figures ALAS *does* know (e.g. HPC pressure ratio is backed out
from the engine's published overall pressure ratio).
"""

from __future__ import annotations

import numpy as np

import _compat  # noqa: F401  (must run before `import SUAVE`)
import SUAVE
from SUAVE.Core import Units


def build_vehicle(request: dict) -> SUAVE.Vehicle:
    dv = request["design_vector"]
    geo = request["geometry_summary"]
    gcfg = request["geometry_config"]
    masses = request["component_masses_kg"]
    engine = request["engine"]
    req = request["requirements"]
    n_engines = int(engine["n_engines"])

    vehicle = SUAVE.Vehicle()
    vehicle.tag = request.get("name", "ALAS_Design")

    # -- vehicle-level mass properties ------------------------------------
    mtow = float(request["mtow_kg"])
    oew = sum(masses.get(k, 0.0) for k in
              ("Wing", "H-Stab", "V-Stab", "Fuselage", "Gear", "Propulsion", "Systems"))
    payload = masses.get("Payload", 0.0)
    vehicle.mass_properties.max_takeoff = mtow * Units.kilogram
    vehicle.mass_properties.takeoff = mtow * Units.kilogram
    vehicle.mass_properties.operating_empty = oew * Units.kilogram
    vehicle.mass_properties.max_zero_fuel = (oew + payload) * Units.kilogram
    vehicle.mass_properties.cargo = payload * Units.kilogram if req.get("aircraft_type") == "cargo" else 0.0
    vehicle.envelope.ultimate_load = req.get("ultimate_load_factor", 3.75)
    vehicle.envelope.limit_load = req.get("ultimate_load_factor", 3.75) / 1.5

    vehicle.reference_area = geo["wing_area_m2"] * Units["meters**2"]
    vehicle.passengers = int(req.get("num_passengers", 0))
    vehicle.systems.control = "fully powered"
    vehicle.systems.accessories = "long range"

    # -- landing gear (generic scaling, ALAS doesn't model gear geometry) --
    landing_gear = SUAVE.Components.Landing_Gear.Landing_Gear()
    landing_gear.tag = "main_landing_gear"
    landing_gear.main_tire_diameter = 1.35 * Units.m
    landing_gear.nose_tire_diameter = 1.10 * Units.m
    landing_gear.main_strut_length = 4.5 * Units.m
    landing_gear.nose_strut_length = 3.8 * Units.m
    landing_gear.main_units = 2
    landing_gear.nose_units = 1
    landing_gear.main_wheels = 6 if mtow > 150_000 else 4
    landing_gear.nose_wheels = 2
    vehicle.landing_gear = landing_gear

    # -- main wing ----------------------------------------------------------
    wcfg = gcfg["wing"]
    wing = SUAVE.Components.Wings.Main_Wing()
    wing.tag = "main_wing"
    wing.aspect_ratio = geo["aspect_ratio"]
    wing.sweeps.quarter_chord = dv["sweep_deg"] * Units.deg
    wing.taper = geo["taper_ratio"]
    wing.spans.projected = dv["span_m"] * Units.meter
    wing.chords.root = dv["root_chord_m"] * Units.meter
    wing.chords.tip = dv["tip_chord_m"] * Units.meter
    wing.areas.reference = geo["wing_area_m2"] * Units["meters**2"]
    wing.twists.root = wcfg["root_twist_deg"] * Units.degrees
    wing.twists.tip = dv["tip_twist_deg"] * Units.degrees
    wing.thickness_to_chord = 0.12 * dv.get("airfoil_thickness_scale", 1.0)
    wing.chords.mean_aerodynamic = geo["mean_aerodynamic_chord_m"] * Units.meter
    wing_x = wcfg["root_datum_x_m"] + dv.get("wing_x_shift_m", 0.0)
    wing.origin = [[wing_x, 0, wcfg["root_z_m"]]] * Units.meter
    wing.vertical = False
    wing.symmetric = True
    wing.high_lift = True
    wing.dynamic_pressure_ratio = 1.0

    flap = SUAVE.Components.Wings.Control_Surfaces.Flap()
    flap.tag = "flap"
    flap.span_fraction_start = 0.08
    flap.span_fraction_end = 0.65
    flap.deflection = 0.0 * Units.degrees
    flap.configuration_type = "double_slotted"
    flap.chord_fraction = 0.25
    wing.append_control_surface(flap)

    slat = SUAVE.Components.Wings.Control_Surfaces.Slat()
    slat.tag = "slat"
    slat.span_fraction_start = 0.06
    slat.span_fraction_end = 0.95
    slat.deflection = 0.0 * Units.degrees
    slat.chord_fraction = 0.15
    wing.append_control_surface(slat)

    aileron = SUAVE.Components.Wings.Control_Surfaces.Aileron()
    aileron.tag = "aileron"
    aileron.span_fraction_start = 0.67
    aileron.span_fraction_end = 0.95
    aileron.deflection = 0.0 * Units.degrees
    aileron.chord_fraction = 0.20
    wing.append_control_surface(aileron)
    vehicle.append_component(wing)

    # -- horizontal stabilizer ------------------------------------------
    ecfg = gcfg["empennage"]
    hstab_area = geo.get("h_stab_area_m2", 0.20 * geo["wing_area_m2"])
    hstab_root_c = ecfg["hstab_root_chord_m"]
    hstab_tip_c = ecfg["hstab_tip_chord_m"]
    hstab_span = 2.0 * hstab_area / (hstab_root_c + hstab_tip_c)
    tip_le = ecfg["hstab_tip_le_m"]  # (x, y, z) of tip LE relative to root LE
    hstab_sweep_rad = np.arctan2(tip_le[0], tip_le[1]) if tip_le[1] else 0.0

    wing = SUAVE.Components.Wings.Horizontal_Tail()
    wing.tag = "horizontal_stabilizer"
    wing.areas.reference = hstab_area * Units["meters**2"]
    wing.aspect_ratio = hstab_span ** 2 / hstab_area
    wing.sweeps.quarter_chord = float(np.degrees(hstab_sweep_rad)) * Units.deg
    wing.thickness_to_chord = 0.10
    wing.taper = hstab_tip_c / hstab_root_c
    wing.spans.projected = hstab_span * Units.meter
    wing.chords.root = hstab_root_c * Units.meter
    wing.chords.tip = hstab_tip_c * Units.meter
    wing.chords.mean_aerodynamic = (hstab_root_c + hstab_tip_c) / 2.0 * Units.meter
    wing.twists.root = ecfg["hstab_root_twist_deg"] * Units.degrees
    wing.twists.tip = ecfg["hstab_tip_twist_deg"] * Units.degrees
    tail_x = gcfg["fuselage"]["nose_z_m"]  # placeholder if no better anchor
    hstab_x = wing_x + ecfg["hstab_offset_from_tail_m"] + dv.get("tail_x_shift_m", 0.0)
    wing.origin = [[hstab_x, 0, ecfg["hstab_z_m"]]] * Units.meter
    wing.vertical = False
    wing.symmetric = True
    wing.dynamic_pressure_ratio = 0.9
    vehicle.append_component(wing)

    # -- vertical stabilizer ------------------------------------------
    vstab_area = geo.get("v_stab_area_m2", 0.10 * geo["wing_area_m2"])
    vstab_root_c = ecfg["vstab_root_chord_m"]
    vstab_tip_c = ecfg["vstab_tip_chord_m"]
    vstab_span = 2.0 * vstab_area / (vstab_root_c + vstab_tip_c)
    vtip_le = ecfg["vstab_tip_le_m"]
    vstab_sweep_rad = np.arctan2(vtip_le[0], vtip_le[2]) if vtip_le[2] else 0.0

    wing = SUAVE.Components.Wings.Vertical_Tail()
    wing.tag = "vertical_stabilizer"
    wing.areas.reference = vstab_area * Units["meters**2"]
    wing.aspect_ratio = vstab_span ** 2 / vstab_area
    wing.sweeps.quarter_chord = float(np.degrees(vstab_sweep_rad)) * Units.deg
    wing.thickness_to_chord = 0.08
    wing.taper = vstab_tip_c / vstab_root_c
    wing.spans.projected = vstab_span * Units.meter
    wing.chords.root = vstab_root_c * Units.meter
    wing.chords.tip = vstab_tip_c * Units.meter
    wing.chords.mean_aerodynamic = (vstab_root_c + vstab_tip_c) / 2.0 * Units.meter
    wing.twists.root = 0.0 * Units.degrees
    wing.twists.tip = 0.0 * Units.degrees
    vstab_x = wing_x + ecfg["vstab_offset_from_tail_m"] + dv.get("tail_x_shift_m", 0.0)
    wing.origin = [[vstab_x, 0, ecfg["vstab_z_m"]]] * Units.meter
    wing.vertical = True
    wing.symmetric = False
    wing.t_tail = False
    wing.dynamic_pressure_ratio = 1.0
    vehicle.append_component(wing)

    # -- fuselage -------------------------------------------------------
    fcfg = gcfg["fuselage"]
    fus_len = dv["fuselage_length_m"]
    diameter = fcfg["diameter_m"]
    height = fcfg.get("height_m") or diameter

    fuselage = SUAVE.Components.Fuselages.Fuselage()
    fuselage.tag = "fuselage"
    fuselage.number_coach_seats = vehicle.passengers
    fuselage.seats_abreast = int(np.clip(round(diameter / 0.55), 2, 10))
    fuselage.seat_pitch = 1.0 * Units.meter
    fuselage.fineness.nose = 1.6
    fuselage.fineness.tail = 2.0
    fuselage.lengths.nose = 0.156 * fus_len * Units.meter
    fuselage.lengths.tail = 0.235 * fus_len * Units.meter
    fuselage.lengths.total = fus_len * Units.meter
    fuselage.lengths.fore_space = 6.0 * Units.meter
    fuselage.lengths.aft_space = 5.0 * Units.meter
    fuselage.width = diameter * Units.meter
    fuselage.heights.maximum = height * Units.meter
    fuselage.effective_diameter = diameter * Units.meter
    fuselage.areas.side_projected = 0.9 * fus_len * height * Units["meters**2"]
    fuselage.areas.wetted = np.pi * diameter * fus_len * Units["meters**2"]
    fuselage.areas.front_projected = (np.pi / 4.0) * diameter ** 2 * Units["meters**2"]
    fuselage.differential_pressure = 5.8e4 * Units.pascal
    fuselage.heights.at_quarter_length = height * Units.meter
    fuselage.heights.at_three_quarters_length = 0.93 * height * Units.meter
    fuselage.heights.at_wing_root_quarter_chord = height * Units.meter
    vehicle.append_component(fuselage)

    # -- nacelles + turbofan network -------------------------------------
    ecfg_eng = gcfg["engine"]
    sweep_rad = np.radians(dv["sweep_deg"])
    z_eng = ecfg_eng["z_m"]
    inlet_x_offset = ecfg_eng["inlet_x_offset_m"]
    nacelle_diameter = 2.0 * engine["nacelle_max_radius_m"]

    nacelles = []
    origins = []
    for i, y_pos in enumerate(ecfg_eng["spanwise_positions_m"]):
        x_le_local_eng = abs(y_pos) * np.tan(sweep_rad)
        x_inlet = (wing_x + x_le_local_eng) - inlet_x_offset
        nacelle = SUAVE.Components.Nacelles.Nacelle()
        nacelle.tag = f"nacelle_{i + 1}"
        nacelle.length = engine["nacelle_length_m"] * Units.meter
        nacelle.diameter = nacelle_diameter * Units.meter
        nacelle.inlet_diameter = 0.85 * nacelle_diameter * Units.meter
        nacelle.areas.wetted = 1.1 * np.pi * nacelle.diameter * nacelle.length
        nacelle.origin = [[x_inlet, y_pos, z_eng]] * Units.meter
        nacelle.flow_through = True
        nacelle_airfoil = SUAVE.Components.Airfoils.Airfoil()
        nacelle_airfoil.naca_4_series_airfoil = "2410"
        nacelle.append_airfoil(nacelle_airfoil)
        vehicle.append_component(nacelle)
        nacelles.append(nacelle)
        origins.append([x_inlet, y_pos, z_eng])

    turbofan = SUAVE.Components.Energy.Networks.Turbofan()
    turbofan.tag = "turbofan"
    turbofan.number_of_engines = n_engines
    turbofan.bypass_ratio = engine["bypass_ratio"]
    turbofan.origin = origins * Units.meter
    turbofan.working_fluid = SUAVE.Attributes.Gases.Air()

    ram = SUAVE.Components.Energy.Converters.Ram()
    ram.tag = "ram"
    turbofan.append(ram)

    inlet_nozzle = SUAVE.Components.Energy.Converters.Compression_Nozzle()
    inlet_nozzle.tag = "inlet_nozzle"
    inlet_nozzle.polytropic_efficiency = 0.98
    inlet_nozzle.pressure_ratio = 0.98
    turbofan.append(inlet_nozzle)

    # Overall pressure ratio is published per engine; split it between a fixed
    # low-pressure-compressor ratio (generic, unmeasured) and a high-pressure
    # compressor ratio backed out to hit the published OPR -- the same split
    # structure suave_example.py used for GE9X (LPC 1.20 fixed, HPC solved).
    lpc_pr = 1.20
    opr = engine["overall_pressure_ratio"]
    hpc_pr = max(opr / lpc_pr, 1.0)

    compressor = SUAVE.Components.Energy.Converters.Compressor()
    compressor.tag = "low_pressure_compressor"
    compressor.polytropic_efficiency = 0.91
    compressor.pressure_ratio = lpc_pr
    turbofan.append(compressor)

    compressor = SUAVE.Components.Energy.Converters.Compressor()
    compressor.tag = "high_pressure_compressor"
    compressor.polytropic_efficiency = 0.93
    compressor.pressure_ratio = hpc_pr
    turbofan.append(compressor)

    turbine = SUAVE.Components.Energy.Converters.Turbine()
    turbine.tag = "low_pressure_turbine"
    turbine.mechanical_efficiency = 0.99
    turbine.polytropic_efficiency = 0.95
    turbofan.append(turbine)

    turbine = SUAVE.Components.Energy.Converters.Turbine()
    turbine.tag = "high_pressure_turbine"
    turbine.mechanical_efficiency = 0.99
    turbine.polytropic_efficiency = 0.95
    turbofan.append(turbine)

    combustor = SUAVE.Components.Energy.Converters.Combustor()
    combustor.tag = "combustor"
    combustor.efficiency = 0.99
    combustor.alphac = 1.0
    combustor.turbine_inlet_temperature = engine.get("turbine_inlet_temp_k", 1650.0)
    combustor.pressure_ratio = 0.95
    combustor.fuel_data = SUAVE.Attributes.Propellants.Jet_A()
    turbofan.append(combustor)

    nozzle = SUAVE.Components.Energy.Converters.Expansion_Nozzle()
    nozzle.tag = "core_nozzle"
    nozzle.polytropic_efficiency = 0.95
    nozzle.pressure_ratio = 0.99
    turbofan.append(nozzle)

    nozzle = SUAVE.Components.Energy.Converters.Expansion_Nozzle()
    nozzle.tag = "fan_nozzle"
    nozzle.polytropic_efficiency = 0.95
    nozzle.pressure_ratio = 0.99
    turbofan.append(nozzle)

    fan = SUAVE.Components.Energy.Converters.Fan()
    fan.tag = "fan"
    fan.polytropic_efficiency = 0.93
    fan.pressure_ratio = engine.get("fan_pressure_ratio", 1.5)
    turbofan.append(fan)

    thrust = SUAVE.Components.Energy.Processes.Thrust()
    thrust.tag = "compute_thrust"
    # turbofan_sizing() below is called AT the cruise Mach/altitude, so
    # total_design must be the thrust the engine produces AT THAT point, not
    # its sea-level-static rating -- confirmed against SUAVE's own Boeing_737
    # regression vehicle, which sizes total_design to the cruise-thrust-
    # required at 35,000ft/M0.78, not the CFM56's much larger SLS rating.
    # Using the static rating here instead (a past bug) told SUAVE the
    # engine could produce its full static thrust AT CRUISE ALTITUDE too,
    # oversizing it and making every mission segment's solved throttle read
    # far too low (e.g. ~0.2 at cruise). cruise_thrust_kn is precomputed in
    # alas/integration/suave_vehicle.py from the same on-design cycle
    # model the Propulsion Analysis tab uses, so this stays consistent with
    # what that tab reports as "Per-engine thrust, this cruise pt". Falls
    # back to the old (wrong but non-crashing) static-thrust value only if
    # that cycle was infeasible for this design.
    cruise_thrust_kn = engine.get("cruise_thrust_kn")
    if cruise_thrust_kn is None:
        cruise_thrust_kn = engine["thrust_kn"]
    thrust.total_design = n_engines * cruise_thrust_kn * 1000.0 * Units.N
    turbofan.thrust = thrust

    from SUAVE.Methods.Propulsion.turbofan_sizing import turbofan_sizing
    cruise_altitude = req["cruise_altitude_m"] * Units.m
    cruise_mach = req["cruise_mach"]
    turbofan_sizing(turbofan, cruise_mach, cruise_altitude)
    vehicle.append_component(turbofan)

    return vehicle
