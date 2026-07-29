# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Weight & balance (component mass buildup and center of gravity estimation).

Estimates individual component masses (structure, systems, propulsion, payload,
and fuel) using Torenbeek empirical formulas and determines the physical
location of the center of gravity (CG).
"""

from __future__ import annotations

from typing import Dict, List, Tuple
import aerosandbox as asb
import aerosandbox.numpy as np
import aerosandbox.library.weights.torenbeek_weights as torenbeek

from ..config.requirements import DesignRequirements
from ..config.geometry_config import GeometryConfig
from ..config.mass_config import MassModelConfig


def calculate_component_masses(
    plane: asb.Airplane,
    requirements: DesignRequirements,
    geometry_config: GeometryConfig,
    mass_model: MassModelConfig | None = None,
) -> Dict[str, float]:
    """Calculate the masses of all primary aircraft components in kg.

    Uses Torenbeek empirical methods calibrated for CS-25/FAR-25 class transports.
    All empirical fractions come from ``mass_model`` so the user can tune them
    from Advanced Settings -> Mass model.
    """
    mm = mass_model or MassModelConfig()
    mtow_target = requirements.mtow_kg
    n_ult = requirements.ultimate_load_factor
    v_dive = requirements.dive_speed_m_s

    # Extract lifting surfaces
    wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    hstab = next((w for w in plane.wings if w.name == "Horizontal Stabilizer"), None)
    vstab = next((w for w in plane.wings if w.name == "Vertical Stabilizer"), None)

    if hstab is None:
        hstab = plane.wings[1] if len(plane.wings) > 1 else wing
    if vstab is None:
        vstab = plane.wings[2] if len(plane.wings) > 2 else wing

    # Extract fuselage
    fus = plane.fuselages[0]

    # Wing mass: Torenbeek Appendix C
    m_wing = torenbeek.mass_wing(
        wing=wing,
        design_mass_TOGW=mtow_target,
        ultimate_load_factor=n_ult,
        suspended_mass=mtow_target * mm.suspended_mass_fraction,
        never_exceed_airspeed=v_dive,
        max_airspeed_for_flaps=mm.max_airspeed_for_flaps_ms,
        main_gear_mounted_to_wing=False,
        flap_deflection_angle=mm.flap_deflection_angle_deg,
    )

    # Stabilizers
    m_hstab = torenbeek.mass_wing(
        wing=hstab,
        design_mass_TOGW=mtow_target,
        ultimate_load_factor=n_ult,
        suspended_mass=0.0,
        never_exceed_airspeed=v_dive,
        max_airspeed_for_flaps=0.0,
        main_gear_mounted_to_wing=False,
        flap_deflection_angle=0.0,
    )

    m_vstab = torenbeek.mass_wing(
        wing=vstab,
        design_mass_TOGW=mtow_target,
        ultimate_load_factor=n_ult,
        suspended_mass=0.0,
        never_exceed_airspeed=v_dive,
        max_airspeed_for_flaps=0.0,
        main_gear_mounted_to_wing=False,
        flap_deflection_angle=0.0,
    )

    # Fuselage mass: Torenbeek simple method
    l_tail = max(1.0, hstab.aerodynamic_center()[0] - wing.aerodynamic_center()[0])
    m_fus = torenbeek.mass_fuselage_simple(
        fuselage=fus,
        never_exceed_airspeed=v_dive,
        wing_to_tail_distance=l_tail,
    )

    # Landing gear
    m_gear = mm.landing_gear_mass_fraction * mtow_target

    # Propulsion mass: dry engine weight + pylons + accessories. Reads the
    # engine's live, editable design thrust (EngineConfig.thrust_kn) -- kept
    # in sync with the preset registry by apply_engine_spec(), and directly
    # editable from the Engine Designer tab -- rather than re-looking the
    # engine up by name, so a hand-tuned thrust value is reflected here too.
    thrust_n = geometry_config.engine.thrust_kn * 1000.0
    if thrust_n > 0:
        n_engines = len(geometry_config.engine.spanwise_positions_m)
        m_prop = (
            n_engines
            * (thrust_n / (mm.propulsion_twr_factor * requirements.gravity_m_s2))
            * mm.propulsion_installation_factor
        )
    else:
        m_prop = mm.propulsion_mass_fallback_fraction * mtow_target

    # Systems & equipment
    m_sys = mm.systems_mass_fraction * mtow_target

    # Furnishings & operational items
    m_furn = mm.furnishings_mass_fraction * mtow_target

    # Payload
    m_payload = requirements.payload_kg

    # OEW, MZFW, fuel
    m_str = m_wing + m_hstab + m_vstab + m_fus + m_gear
    m_oew = m_str + m_prop + m_sys + m_furn
    m_mzfw = m_oew + m_payload
    m_fuel = mtow_target - m_mzfw

    return {
        "Wing": m_wing,
        "H-Stab": m_hstab,
        "V-Stab": m_vstab,
        "Fuselage": m_fus,
        "Gear": m_gear,
        "Propulsion": m_prop,
        "Systems": m_sys,
        "Furnishings": m_furn,
        "Payload": m_payload,
        "Fuel": m_fuel,
    }


def define_mass_coordinates(
    plane: asb.Airplane,
    geometry_config: GeometryConfig,
    requirements: DesignRequirements | None = None,
    mass_model: MassModelConfig | None = None,
) -> Dict[str, List[float]]:
    """Determine the X, Y, Z physical locations of the centroid of each component.

    Payload is placed at the centre of the *occupied* cabin length (payload
    mass / ``mass_model.cabin_payload_density_kg_m``, capped at the full
    available cabin) rather than always the full available cabin. This means
    that stretching the fuselage beyond what the required payload physically
    needs does NOT shift the payload CG aft for free -- the optimizer must pay
    a CG-mismatch penalty for unrealistic stretch. See methods.md's
    "Component CG coordinates" section for the L_occ derivation.
    """
    mm = mass_model or MassModelConfig()
    req = requirements or DesignRequirements()
    wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    fus = plane.fuselages[0]

    fus_len = fus.xsecs[-1].xyz_c[0] - fus.xsecs[0].xyz_c[0]
    fus_z = fus.xsecs[0].xyz_c[2]

    w_ac = wing.aerodynamic_center()
    w_root_z = wing.xsecs[0].xyz_le[2]

    cabin_start = geometry_config.fuselage.cabin_start_x_m
    tailcone_len = geometry_config.fuselage.tailcone_length_m
    cabin_len = max(1.0, fus_len - cabin_start - tailcone_len)

    # Occupied cabin length: how much of the available cabin the PAYLOAD
    # actually needs at the configured linear density, capped at what's
    # physically available. A fuselage stretched beyond that need does not
    # move the payload centroid (and therefore the CG) aft "for free". This
    # must stay scoped to Payload only -- Systems and Furnishings are OEW
    # (installed-equipment) components below and use the full cabin_len
    # instead: they are physically present over the whole installed cabin
    # regardless of how many of those seats a particular run happens to book,
    # so their position must NOT move when only num_passengers/payload_kg
    # changes (e.g. switching a cabin preset from all-economy to a lower-
    # density 3-class at the SAME fuselage length). Tying OEW-component
    # position to occupied_len instead would drag the entire OEW-component CG
    # forward whenever a preset books fewer passengers, purely as an artifact
    # of the shorter occupied length rather than any real change to where the
    # installed equipment sits, corrupting CG-envelope compliance for an
    # otherwise correct lower-density 3-class config.
    occupied_len = min(
        cabin_len, req.payload_kg / max(mm.cabin_payload_density_kg_m, 1e-6)
    )

    # Systems (avionics, ECS, APU) are concentrated in the forward equipment bay
    # and central cabin zone, including APU. Scaled with the full installed
    # cabin length (NOT occupied_len -- see note above).
    x_systems = cabin_start + 0.45 * cabin_len
    # Furnishings (seats, galleys, etc.) and operational items -- also
    # installed over the full cabin, not the currently-booked payload.
    x_furn = cabin_start + 0.50 * cabin_len
    # Payload CG at the centre of the occupied cabin section (the one place
    # occupied_len is the physically correct choice).
    x_payload = cabin_start + 0.50 * occupied_len

    coords = {
        "Fuselage": [fus_len * 0.46, 0.0, fus_z],
        "Wing": [w_ac[0] + (wing.xsecs[0].chord * 0.2), 0.0, w_root_z],
        "H-Stab": [
            fus_len - geometry_config.empennage.hstab_offset_from_tail_m / 2.0,
            0.0,
            fus_z + geometry_config.empennage.hstab_z_m,
        ],
        "V-Stab": [
            fus_len - geometry_config.empennage.vstab_offset_from_tail_m / 2.0,
            0.0,
            fus_z + geometry_config.empennage.vstab_z_m,
        ],
        "Gear": [w_ac[0], 0.0, w_root_z - 2.5],
        "Propulsion": [w_ac[0], 0.0, w_root_z - 1.0],
        "Systems": [x_systems, 0.0, fus_z],
        "Furnishings": [x_furn, 0.0, fus_z],
        "Payload": [x_payload, 0.0, fus_z],
        "Fuel": [w_ac[0], 0.0, w_root_z],
    }

    try:
        nacelles = [f for f in plane.fuselages if "Nacelle" in f.name]
        if nacelles:
            x_engines = []
            y_engines = []
            z_engines = []
            for nac in nacelles:
                x_start = nac.xsecs[0].xyz_c[0]
                length = nac.xsecs[-1].xyz_c[0] - x_start
                x_engines.append(x_start + length * 0.5)
                y_engines.append(nac.xsecs[0].xyz_c[1])
                z_engines.append(nac.xsecs[0].xyz_c[2])
            coords["Propulsion"] = [
                float(np.mean(x_engines)),
                float(np.mean(y_engines)),
                float(np.mean(z_engines)),
            ]
    except Exception:
        pass

    return coords


def calculate_physical_cg(
    masses: Dict[str, float],
    coords: Dict[str, List[float]],
) -> List[float]:
    """Calculate the global center of gravity location [X, Y, Z] in meters."""
    mom_x = mom_y = mom_z = m_tot = 0.0
    for k, m in masses.items():
        m_calc = max(0.0, m)
        x, y, z = coords[k]
        mom_x += m_calc * x
        mom_y += m_calc * y
        mom_z += m_calc * z
        m_tot += m_calc
    if m_tot == 0.0:
        return [0.0, 0.0, 0.0]
    return [mom_x / m_tot, mom_y / m_tot, mom_z / m_tot]


# Components that make up the Operating Empty Weight (everything except payload
# and fuel). This is the single canonical definition -- every other module that
# needs the OEW component set (physics.payload, optimization.objective,
# reporting.design_report/visualization, gui.widgets.results_view) imports it
# from here rather than redefining its own copy.
OEW_KEYS = (
    "Wing",
    "H-Stab",
    "V-Stab",
    "Fuselage",
    "Gear",
    "Propulsion",
    "Systems",
    "Furnishings",
)


def run_mass_analysis(
    plane: asb.Airplane,
    requirements: DesignRequirements,
    geometry_config: GeometryConfig,
    mass_model: MassModelConfig | None = None,
    payload_layout=None,
) -> Tuple[Dict[str, float], Dict[str, List[float]], List[float]]:
    """Execute the full weight and balance analysis.

    Parameters
    ----------
    payload_layout : PayloadLayout, optional
        Detailed interior layout (cargo ULDs or passenger seating). When given,
        the lumped Payload mass/coordinate are replaced by the layout's true mass
        and centre of gravity, and the fuel remainder is recomputed. Every caller
        (the optimizer loop included -- see `optimization/objective.py` and
        `docs/architecture.md` Sec 1) calls this function TWICE per evaluation:
        once without `payload_layout` to get a cheap OEW/x_oew estimate, then
        again with the detailed layout built from that estimate, so the CG this
        function returns always reflects the real cabin/cargo layout rather than
        the lumped `cabin_payload_density_kg_m` estimate, which only ever seeds
        that first pass.

    Returns
    -------
    Tuple[Dict[str, float], Dict[str, List[float]], List[float]]
        Component masses, component coordinates, and global CG.
    """
    masses = calculate_component_masses(
        plane, requirements, geometry_config, mass_model
    )
    coords = define_mass_coordinates(plane, geometry_config, requirements, mass_model)

    if payload_layout is not None and payload_layout.total_mass > 0:
        m_oew = sum(masses.get(k, 0.0) for k in OEW_KEYS)
        masses["Payload"] = float(payload_layout.total_mass)
        masses["Fuel"] = requirements.mtow_kg - (m_oew + masses["Payload"])
        z_payload = coords["Payload"][2] if "Payload" in coords else 0.0
        coords["Payload"] = [
            float(payload_layout.cg_x),
            float(payload_layout.cg_y),
            z_payload,
        ]

    cg = calculate_physical_cg(masses, coords)
    return masses, coords, cg
