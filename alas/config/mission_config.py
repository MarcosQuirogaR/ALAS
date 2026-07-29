# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Mission analysis configuration -- all values exposed in Advanced Settings.

``MissionConfig`` controls whether ``DesignPipeline.run()`` includes a SUAVE
mission analysis as part of the normal Run flow (it does, by default), where
the isolated SUAVE venv / downloadable assets (navdata, earth texture) live,
and the full climb/cruise/descent speed profile SUAVE flies -- every figure
lives here rather than hardcoded inside
``external tools/suave_runner/mission_builder.py`` (a generalisation of
``suave_example.py``'s own hardcoded Madrid-Nairobi profile), consistent
with the rest of the config layer's "no hardcoded design values" rule.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class MissionProfileConfig:
    """SUAVE mission segment speeds/rates/altitudes.

    Defaults reproduce ``suave_example.py``'s original Madrid-Nairobi
    profile (takeoff -> initial climb -> 2 step-climbs -> 3 cruise legs ->
    4-step descent ladder -> final landing). Cruise leg fractions split the
    *actual* route distance (not a fixed NM figure) across the three cruise
    legs in the same 1000:1200:1350 NM proportions the original script used.
    """

    # Takeoff
    takeoff_altitude_gain_m: float = 3048.0  # climb to this height AGL after liftoff
    takeoff_air_speed_m_s: float = 128.6
    takeoff_climb_rate_m_s: float = 10.0

    # Initial climb (to the first cruise step)
    initial_climb_air_speed_m_s: float = 170.0
    initial_climb_rate_m_s: float = 12.0
    initial_climb_altitude_fraction: float = 0.795  # fraction of cruise altitude

    # Step climb 1 (to the second cruise step)
    step_climb_1_air_speed_m_s: float = 250.0
    step_climb_1_rate_m_s: float = 3.0
    step_climb_1_altitude_fraction: float = 0.897  # fraction of cruise altitude

    # Step climb 2 (to final cruise altitude)
    step_climb_2_air_speed_m_s: float = 248.0
    step_climb_2_rate_m_s: float = 2.5

    # Cruise legs (speed each; distance is the route's total split by fraction)
    cruise_1_air_speed_m_s: float = 253.5
    cruise_1_distance_fraction: float = 0.28169  # 1000 / 3550 NM
    cruise_2_air_speed_m_s: float = 249.4
    cruise_2_distance_fraction: float = 0.33803  # 1200 / 3550 NM
    cruise_3_air_speed_m_s: float = 247.8
    cruise_3_distance_fraction: float = 0.38028  # 1350 / 3550 NM

    # Descent ladder (each step skipped if below the arrival field elevation)
    descent_1_altitude_ft: float = 30000.0
    descent_1_air_speed_m_s: float = 220.0
    descent_1_rate_m_s: float = 4.5
    descent_2_altitude_ft: float = 17000.0
    descent_2_air_speed_m_s: float = 195.0
    descent_2_rate_m_s: float = 5.0
    descent_3_altitude_ft: float = 10000.0
    descent_3_air_speed_m_s: float = 170.0
    descent_3_rate_m_s: float = 5.0
    descent_4_altitude_ft: float = 6500.0
    descent_4_air_speed_m_s: float = 150.0
    descent_4_rate_m_s: float = 5.0

    # Final landing segment down to the arrival field elevation
    landing_air_speed_m_s: float = 83.6
    landing_descent_rate_m_s: float = 3.0


@dataclass
class MissionConfig:
    """Top-level SUAVE mission-analysis settings.

    ``enabled`` controls whether mission analysis runs automatically as part
    of ``DesignPipeline.run()`` (default on). Asset paths are repo-root-
    relative so a saved YAML config stays portable across machines.
    """

    enabled: bool = True
    timeout_s: float = 900.0
    suave_venv_dir: str = ".suave-venv"
    # Where "external tools/suave_runner/run_mission.py" (the script actually
    # invoked inside the isolated venv) lives. These repo-root-relative
    # defaults are resolved by pipeline.py's _run_mission_analysis, which
    # tries (in order): something already on disk at this path next to the
    # app (a user's own provisioned copy always wins), then a SUAVE runtime
    # bundled into a packaged ALAS.exe (scripts/build_suave_env.py,
    # extracted automatically at first launch -- see desktop/suave_runtime.go),
    # then this default as a dev-checkout fallback. Set to an absolute path
    # here (Setup > External Tools > SUAVE) to pin either one to a specific
    # location instead.
    suave_runner_dir: str = "external tools/suave_runner"
    navdata_dir: str = "alas/data/navdata"
    texture_path: str = "alas/data/textures/earth_blue_marble.jpg"
    routes_dir: str = "alas/data/routes"
    great_circle_points: int = 50
    # SimBrief username or numeric Pilot ID (Setup > External Tools). When
    # set, Route.for_airports() tries fetching this user's most recently
    # generated SimBrief OFP first (real, current-AIRAC SID/STAR/airway
    # routing computed by SimBrief itself -- see routing/simbrief_route.py)
    # before falling through to manual KML / the open navdata airway graph /
    # great-circle. Leave blank to skip this tier entirely. Requires
    # generating that exact origin/destination OFP on SimBrief yourself first
    # -- there is no free API to request a *new* dispatch on demand, only to
    # fetch your last one. hide_in_form: consolidated onto Setup > External
    # Tools instead of shown on this Advanced Settings page.
    simbrief_username: str = field(default="", metadata={"hide_in_form": True})
    simbrief_timeout_s: float = field(default=15.0, metadata={"hide_in_form": True})
    # When a fetched SimBrief OFP is for a different city pair than the
    # departure/arrival airports selected on the Inputs page, use the OFP's
    # pair anyway (a real dispatched flight plan is the highest-fidelity route
    # available, so it outranks a manually-picked pair). Set False to instead
    # ignore a mismatched OFP and fall through to the other routing tiers.
    simbrief_overrides_airports: bool = field(
        default=True,
        metadata={
            "label": "SimBrief overrides route airports",
            "help": "When your most recent SimBrief OFP is for a different city pair than the "
            "departure/arrival airports selected above, fly the OFP's pair instead. A real "
            "dispatched OFP (real SID/STAR/airways, current AIRAC) is the most accurate route "
            "ALAS can get, so it takes precedence. Turn off to keep the manually-selected "
            "airports and ignore a mismatched OFP.",
        },
    )
    profile: MissionProfileConfig = field(default_factory=MissionProfileConfig)
