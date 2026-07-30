# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Parameterized SUAVE configs/analyses/mission builder.

A generalization of ``suave_example.py``'s ``configs_setup`` / ``analyses_setup``
/ ``base_analysis`` (vehicle-agnostic, ported essentially unchanged) and
``mission_setup`` (was hardcoded to the Madrid-Nairobi route's fixed
1000/1200/1350 NM cruise legs and FL310/FL350/FL390 step-climb ladder; now
scaled to the requested route distance and cruise altitude so it works for any
origin/destination/aircraft).
"""

from __future__ import annotations

import _compat  # noqa: F401
import SUAVE
from SUAVE.Core import Units


def configs_setup(vehicle):
    configs = SUAVE.Components.Configs.Config.Container()
    base_config = SUAVE.Components.Configs.Config(vehicle)
    base_config.tag = "base"
    configs.append(base_config)

    config = SUAVE.Components.Configs.Config(base_config)
    config.tag = "cruise"
    configs.append(config)

    config = SUAVE.Components.Configs.Config(base_config)
    config.tag = "takeoff"
    config.wings["main_wing"].control_surfaces.flap.deflection = 20.0 * Units.deg
    config.wings["main_wing"].control_surfaces.slat.deflection = 25.0 * Units.deg
    config.max_lift_coefficient_factor = 1.0
    configs.append(config)

    config = SUAVE.Components.Configs.Config(base_config)
    config.tag = "cutback"
    config.wings["main_wing"].control_surfaces.flap.deflection = 15.0 * Units.deg
    config.wings["main_wing"].control_surfaces.slat.deflection = 20.0 * Units.deg
    config.max_lift_coefficient_factor = 1.0
    configs.append(config)

    config = SUAVE.Components.Configs.Config(base_config)
    config.tag = "landing"
    config.wings["main_wing"].control_surfaces.flap.deflection = 30.0 * Units.deg
    config.wings["main_wing"].control_surfaces.slat.deflection = 25.0 * Units.deg
    config.max_lift_coefficient_factor = 1.0
    configs.append(config)

    config = SUAVE.Components.Configs.Config(base_config)
    config.tag = "short_field_takeoff"
    config.wings["main_wing"].control_surfaces.flap.deflection = 30.0 * Units.deg
    config.wings["main_wing"].control_surfaces.slat.deflection = 25.0 * Units.deg
    config.max_lift_coefficient_factor = 1.0
    configs.append(config)
    return configs


def simple_sizing(configs):
    base = configs.base
    base.pull_base()
    base.mass_properties.max_zero_fuel = 0.73 * base.mass_properties.max_takeoff
    for wing in base.wings:
        wing.areas.wetted = 2.05 * wing.areas.reference
        wing.areas.exposed = 0.85 * wing.areas.wetted
        wing.areas.affected = 0.60 * wing.areas.wetted
    base.store_diff()

    landing = configs.landing
    landing.pull_base()
    landing.mass_properties.landing = 0.75 * base.mass_properties.takeoff
    landing.store_diff()


def base_analysis(vehicle):
    analyses = SUAVE.Analyses.Vehicle()

    weights = SUAVE.Analyses.Weights.Weights_Transport()
    weights.vehicle = vehicle
    analyses.append(weights)

    aerodynamics = SUAVE.Analyses.Aerodynamics.Fidelity_Zero()
    aerodynamics.geometry = vehicle
    analyses.append(aerodynamics)

    stability = SUAVE.Analyses.Stability.Fidelity_Zero()
    stability.geometry = vehicle
    analyses.append(stability)

    energy = SUAVE.Analyses.Energy.Energy()
    energy.network = vehicle.networks
    analyses.append(energy)

    planet = SUAVE.Analyses.Planets.Planet()
    analyses.append(planet)

    atmosphere = SUAVE.Analyses.Atmospheric.US_Standard_1976()
    atmosphere.features.planet = planet.features
    analyses.append(atmosphere)
    return analyses


def analyses_setup(configs):
    analyses = SUAVE.Analyses.Analysis.Container()
    for tag, config in configs.items():
        analyses[tag] = base_analysis(config)
    return analyses


def mission_setup(analyses, request: dict):
    """Build a climb/step-cruise/descent mission scaled to the requested route.

    ``request`` carries: ``cruise_altitude_m``, ``departure_elevation_m``,
    ``arrival_elevation_m``, ``route_distance_m``, ``departure_isa_deviation_c``,
    and ``profile`` (a serialized
    ``alas.config.mission_config.MissionProfileConfig`` -- every speed,
    rate, and altitude fraction below comes from there, not a literal here,
    so the whole flight profile is editable from Advanced Settings ->
    Mission Analysis without touching this file). Cruise Mach is not used
    here: every cruise segment's air speed is an explicit TAS from
    ``profile``, not derived from Mach -- engine sizing (which does need
    Mach) reads it from the separate vehicle request instead.
    """
    cruise_alt = request["cruise_altitude_m"]
    dep_elev = request["departure_elevation_m"]
    arr_elev = request["arrival_elevation_m"]
    route_distance_nm = request["route_distance_m"] / Units.nautical_miles
    p = request["profile"]

    mission = SUAVE.Analyses.Mission.Sequential_Segments()
    mission.tag = request.get("mission_tag", "alas_mission")

    airport = SUAVE.Attributes.Airports.Airport()
    airport.altitude = dep_elev * Units.m
    airport.delta_isa = request.get("departure_isa_deviation_c", 0.0)
    airport.atmosphere = SUAVE.Attributes.Atmospheres.Earth.US_Standard_1976()
    mission.airport = airport

    Segments = SUAVE.Analyses.Mission.Segments
    base_segment = Segments.Segment()

    # Cruise-step flight levels scaled proportionally to the requested cruise
    # altitude via the configured fractions.
    fl_1 = max(cruise_alt * p["initial_climb_altitude_fraction"], dep_elev + 3000.0 * Units.m)
    fl_2 = max(cruise_alt * p["step_climb_1_altitude_fraction"], fl_1 + 300.0)

    # Cruise leg distances scaled to the actual route by the configured fractions.
    leg_1_nm = route_distance_nm * p["cruise_1_distance_fraction"]
    leg_2_nm = route_distance_nm * p["cruise_2_distance_fraction"]
    leg_3_nm = route_distance_nm * p["cruise_3_distance_fraction"]

    # -- Takeoff --
    takeoff_segment = Segments.Climb.Constant_Speed_Constant_Rate(base_segment)
    takeoff_segment.tag = "takeoff"
    takeoff_segment.analyses.extend(analyses.takeoff)
    takeoff_segment.altitude_start = dep_elev * Units.m
    takeoff_segment.altitude_end = dep_elev * Units.m + p["takeoff_altitude_gain_m"] * Units.m
    takeoff_segment.air_speed = p["takeoff_air_speed_m_s"] * Units["m/s"]
    takeoff_segment.climb_rate = p["takeoff_climb_rate_m_s"] * Units["m/s"]
    mission.append_segment(takeoff_segment)

    # -- Initial climb --
    initial_climb = Segments.Climb.Constant_Speed_Constant_Rate(base_segment)
    initial_climb.tag = "initial_climb"
    initial_climb.analyses.extend(analyses.cruise)
    initial_climb.altitude_end = fl_1
    initial_climb.air_speed = p["initial_climb_air_speed_m_s"] * Units["m/s"]
    initial_climb.climb_rate = p["initial_climb_rate_m_s"] * Units["m/s"]
    mission.append_segment(initial_climb)

    # -- Cruise step 1 --
    cruise_1 = Segments.Cruise.Constant_Speed_Constant_Altitude(base_segment)
    cruise_1.tag = "cruise_step_1"
    cruise_1.analyses.extend(analyses.cruise)
    cruise_1.air_speed = p["cruise_1_air_speed_m_s"] * Units["m/s"]
    cruise_1.distance = max(leg_1_nm, 1.0) * Units.nautical_miles
    mission.append_segment(cruise_1)

    # -- Step climb 1 --
    step_climb_1 = Segments.Climb.Constant_Speed_Constant_Rate(base_segment)
    step_climb_1.tag = "step_climb_1"
    step_climb_1.analyses.extend(analyses.cruise)
    step_climb_1.altitude_end = fl_2
    step_climb_1.air_speed = p["step_climb_1_air_speed_m_s"] * Units["m/s"]
    step_climb_1.climb_rate = p["step_climb_1_rate_m_s"] * Units["m/s"]
    mission.append_segment(step_climb_1)

    # -- Cruise step 2 --
    cruise_2 = Segments.Cruise.Constant_Speed_Constant_Altitude(base_segment)
    cruise_2.tag = "cruise_step_2"
    cruise_2.analyses.extend(analyses.cruise)
    cruise_2.air_speed = p["cruise_2_air_speed_m_s"] * Units["m/s"]
    cruise_2.distance = max(leg_2_nm, 1.0) * Units.nautical_miles
    mission.append_segment(cruise_2)

    # -- Step climb 2 --
    step_climb_2 = Segments.Climb.Constant_Speed_Constant_Rate(base_segment)
    step_climb_2.tag = "step_climb_2"
    step_climb_2.analyses.extend(analyses.cruise)
    step_climb_2.altitude_end = cruise_alt
    step_climb_2.air_speed = p["step_climb_2_air_speed_m_s"] * Units["m/s"]
    step_climb_2.climb_rate = p["step_climb_2_rate_m_s"] * Units["m/s"]
    mission.append_segment(step_climb_2)

    # -- Cruise step 3 (remaining distance) --
    cruise_3 = Segments.Cruise.Constant_Speed_Constant_Altitude(base_segment)
    cruise_3.tag = "cruise_step_3"
    cruise_3.analyses.extend(analyses.cruise)
    cruise_3.air_speed = p["cruise_3_air_speed_m_s"] * Units["m/s"]
    cruise_3.distance = max(leg_3_nm, 1.0) * Units.nautical_miles
    mission.append_segment(cruise_3)

    # -- Descent ladder down to arrival field elevation --
    descent_steps = [
        (p["descent_1_altitude_ft"], p["descent_1_air_speed_m_s"], p["descent_1_rate_m_s"]),
        (p["descent_2_altitude_ft"], p["descent_2_air_speed_m_s"], p["descent_2_rate_m_s"]),
        (p["descent_3_altitude_ft"], p["descent_3_air_speed_m_s"], p["descent_3_rate_m_s"]),
        (p["descent_4_altitude_ft"], p["descent_4_air_speed_m_s"], p["descent_4_rate_m_s"]),
    ]
    arr_elev_ft = arr_elev / Units.ft
    for i, (alt_ft, spd, rate) in enumerate(descent_steps):
        if alt_ft <= arr_elev_ft:
            continue
        seg = Segments.Descent.Constant_Speed_Constant_Rate(base_segment)
        seg.tag = f"descent_{i + 1}"
        seg.analyses.extend(analyses.landing)
        seg.altitude_end = alt_ft * Units.ft
        seg.air_speed = spd * Units["m/s"]
        seg.descent_rate = rate * Units["m/s"]
        mission.append_segment(seg)

    # -- Final landing segment down to the arrival field --
    landing = Segments.Descent.Constant_Speed_Constant_Rate(base_segment)
    landing.tag = "final_landing"
    landing.analyses.extend(analyses.landing)
    landing.altitude_end = arr_elev * Units.m
    landing.air_speed = p["landing_air_speed_m_s"] * Units["m/s"]
    landing.descent_rate = p["landing_descent_rate_m_s"] * Units["m/s"]
    mission.append_segment(landing)

    return mission


def missions_setup(base_mission):
    missions = SUAVE.Analyses.Mission.Mission.Container()
    missions.base = base_mission
    return missions
