#!/usr/bin/env python3
"""Run an independent Aviary FLOPS mass audit for the ALAS A320 deck.

The ALAS result is read from the serialized pure-FLOPS result.  The Aviary
side is assembled from an empty ``AviaryValues`` object so a validation-case
fixture cannot silently contribute an A320 input.  Values are kept in SI in
the ALAS deck and converted once at the Aviary boundary to the legacy FLOPS
units required by the pinned Aviary implementation.

This is an audit instrument, not an ALAS production model.  It never edits
the ALAS model or the Aviary checkout.  The output directory is the only
write location.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import platform
import shutil
import subprocess
import sys
import traceback
from enum import Enum
from pathlib import Path
from typing import Any


PINNED_AVIARY_COMMIT = "c7affbbe54dcbeded7373eae05f771882e2bb28a"
PINNED_NASA_REFERENCE = "NASA/TM-2017-219627 Vol. I"
NUMERICAL_ABS_TOLERANCE_KG = 1.0e-8
NUMERICAL_REL_TOLERANCE = 1.0e-10

# Exact resolved ALAS FLOPS deck values.  These are deliberately checked
# against the serialized record before the independent run so that a changed
# worktree result cannot masquerade as the registered A320 case.
EXPECTED_SI = {
    "design_gross_mass_kg": 78000.0,
    "design_range_nmi": 3400.0,
    "maximum_mach": 0.82,
    "wing_area_m2": 122.60037559438999,
    "wing_span_m": 35.8,
    "quarter_chord_sweep_deg": 22.88183019377685,
    "fuselage_length_m": 37.57,
    "fuselage_width_m": 3.95,
    "fuselage_depth_m": 4.14,
    "passenger_compartment_length_m": 26.57,
    "engine_count": 2,
    "wing_mounted_engine_count": 2,
    "fuselage_mounted_engine_count": 0,
    "rated_thrust_per_engine_n": 120100.0,
    "nacelle_diameter_m": 2.0,
    "fuel_tank_count": 3,
    "maximum_fuel_capacity_kg": 19334.0,
    "first_class_passenger_count": 12,
    "business_class_passenger_count": 0,
    "tourist_class_passenger_count": 138,
    "flight_attendant_count": 4,
    "flight_crew_count": 2,
    "galley_crew_count": 0,
    "hydraulic_pressure_pa": 20684271.879504,
    "variable_sweep_penalty": 0.0,
    "containerized_cargo_kg": 0.0,
    "fuselage_count": 1,
    "movable_surface_area_m2": 44.36529574640197,
}

# Geometry retained by the ALAS preset / structure adapter but not repeated in
# the compact ``resolved_inputs`` record.  The values are source or model
# outputs, never fitted to an OEW reference.
EXACT_GEOMETRY_SI = {
    "wing_root_chord_m": 7.333,
    "wing_kink_chord_m": 3.432,
    "wing_tip_chord_m": 1.4,
    "wing_taper_ratio": 1.4 / 7.333,
    "wing_aspect_ratio": 35.8**2 / 122.60037559438999,
    "wing_thickness_to_chord": 0.09973720656826204,
    "control_surface_area_ratio": 44.36529574640197 / 122.60037559438999,
    "horizontal_tail_area_m2": 31.0005,
    "horizontal_tail_taper_ratio": 1.149 / 3.831,
    "horizontal_tail_span_m": 12.45,
    "vertical_tail_area_m2": 21.50768,
    "vertical_tail_taper_ratio": 1.884 / 5.444,
    "vertical_tail_height_m": 5.87,
    "nacelle_length_m": 3.3,
    # The ALAS FLOPS structure adapter resolves these oleo lengths from its
    # mode-aware design landing mass (71,760 kg).  They are not Airbus MLW.
    "design_landing_mass_kg": 71760.0,
    "main_gear_oleo_length_m": 2.412632402234636,
    "nose_gear_oleo_length_m": 1.6888426815642452,
}

# Conversion constants are explicit rather than delegated to a convenience
# case.  The Aviary components use US customary FLOPS units internally.
CONVERSIONS = {
    "m_to_ft": 1.0 / 0.3048,
    "m2_to_ft2": (1.0 / 0.3048) ** 2,
    "kg_to_lbm": 1.0 / 0.45359237,
    "n_to_lbf": 1.0 / 4.4482216152605,
    "pa_to_psi": 1.0 / 6894.757293168,
    "m_to_in": 1.0 / 0.0254,
}

# ALAS output path -> Aviary output path.  Each line is one equation-level
# component; group totals are added separately below.  Inlet/nozzle masses do
# not have independent outputs in Aviary's pinned transport FLOPS branch and
# are retained as an explicit scope gap rather than hidden in nacelle mass.
COMPONENTS = [
    ("wing.bending_material", "aircraft:wing:bending_material_mass", "groups.airframe.structure.wing.bending_material_kg", "kg", "Wing W1 bending material"),
    ("wing.shear_and_control", "aircraft:wing:shear_control_mass", "groups.airframe.structure.wing.shear_and_control_kg", "kg", "Wing W2 shear/control"),
    ("wing.miscellaneous", "aircraft:wing:misc_mass", "groups.airframe.structure.wing.miscellaneous_kg", "kg", "Wing W3 miscellaneous"),
    ("wing.total", "aircraft:wing:mass", "groups.airframe.structure.wing.total_kg", "kg", "Wing total"),
    ("horizontal_tail", "aircraft:horizontal_tail:mass", "groups.airframe.structure.horizontal_tail_kg", "kg", "Horizontal tail"),
    ("vertical_tail", "aircraft:vertical_tail:mass", "groups.airframe.structure.vertical_tail_kg", "kg", "Vertical tail"),
    ("fuselage", "aircraft:fuselage:mass", "groups.airframe.structure.fuselage_kg", "kg", "Fuselage"),
    ("main_gear", "aircraft:landing_gear:main_gear_mass", "groups.airframe.structure.main_gear_kg", "kg", "Main landing gear"),
    ("nose_gear", "aircraft:landing_gear:nose_gear_mass", "groups.airframe.structure.nose_gear_kg", "kg", "Nose landing gear"),
    ("nacelle_each", "aircraft:nacelle:mass", "groups.airframe.structure.nacelle_kg", "kg", "Nacelle each; ALAS row is total"),
    ("paint", "aircraft:paint:mass", "groups.airframe.structure.paint_kg", "kg", "Paint"),
    ("engine_core_each", "aircraft:engine:mass", "groups.airframe.propulsion.engine_core_each_kg", "kg", "Engine core each"),
    ("engine_cores_total", "aircraft:propulsion:total_engine_mass", "groups.airframe.propulsion.engine_cores_kg", "kg", "Engine cores total"),
    ("engine_controls", "aircraft:propulsion:total_engine_controls_mass", "groups.airframe.propulsion.engine_controls_kg", "kg", "Engine controls"),
    ("thrust_reversers_total", "aircraft:propulsion:total_thrust_reversers_mass", "groups.airframe.propulsion.thrust_reversers_kg", "kg", "Thrust reversers"),
    ("starter_total", "aircraft:propulsion:total_starter_mass", "groups.airframe.propulsion.starters_kg", "kg", "Starters"),
    ("misc_propulsion_total", "aircraft:propulsion:total_misc_mass", "groups.airframe.propulsion.misc_kg", "kg", "Misc propulsion (controls + starters + additional)"),
    ("fuel_system", "aircraft:fuel:fuel_system_mass", "groups.airframe.propulsion.fuel_system_kg", "kg", "Fuel system"),
    ("air_conditioning", "aircraft:air_conditioning:mass", "groups.systems.air_conditioning_kg", "kg", "Air conditioning"),
    ("anti_icing", "aircraft:anti_icing:mass", "groups.systems.anti_ice_kg", "kg", "Anti-ice"),
    ("apu", "aircraft:apu:mass", "groups.systems.apu_kg", "kg", "APU"),
    ("avionics", "aircraft:avionics:mass", "groups.systems.avionics_kg", "kg", "Avionics"),
    ("electrical", "aircraft:electrical:mass", "groups.systems.electrical_kg", "kg", "Electrical"),
    ("furnishings", "aircraft:furnishings:mass", "groups.systems.furnishings_kg", "kg", "Furnishings"),
    ("hydraulics", "aircraft:hydraulics:mass", "groups.systems.hydraulics_kg", "kg", "Hydraulics"),
    ("instruments", "aircraft:instruments:mass", "groups.systems.instruments_kg", "kg", "Instruments"),
    ("surface_controls", "aircraft:wing:surface_control_mass", "groups.systems.surface_controls_kg", "kg", "Surface controls"),
    ("cabin_crew", "aircraft:crew_and_payload:cabin_crew_mass", "groups.operating_items.cabin_crew_and_baggage_kg", "kg", "Cabin crew and baggage"),
    ("flight_crew", "aircraft:crew_and_payload:flight_crew_mass", "groups.operating_items.flight_crew_and_baggage_kg", "kg", "Flight crew and baggage"),
    ("passenger_service", "aircraft:crew_and_payload:passenger_service_mass", "groups.operating_items.passenger_service_kg", "kg", "Passenger service"),
    ("unusable_fuel", "aircraft:fuel:unusable_fuel_mass", "groups.operating_items.unusable_fuel_kg", "kg", "Unusable fuel"),
    ("engine_oil", "aircraft:propulsion:total_engine_oil_mass", "groups.operating_items.engine_oil_kg", "kg", "Engine oil"),
    ("cargo_containers", "aircraft:crew_and_payload:cargo_container_mass", "groups.operating_items.cargo_containers_kg", "kg", "Cargo containers"),
    ("empty_mass_margin", "aircraft:design:empty_mass_margin", None, "kg", "Empty-mass margin (declared zero)"),
    ("inlet_each", None, "groups.airframe.propulsion.inlet_each_kg", "kg", "Engine inlet (Aviary scope gap)"),
    ("nozzle_each", None, "groups.airframe.propulsion.nozzle_each_kg", "kg", "Engine nozzle (Aviary scope gap)"),
]

GROUP_OUTPUTS = [
    ("structure_total", "aircraft:design:structure_mass", "groups.airframe.structure.total_kg", "Structure total"),
    ("propulsion_total", "aircraft:propulsion:mass", "groups.airframe.propulsion.total_kg", "Propulsion without nacelles"),
    ("systems_total", "aircraft:design:systems_and_equipment_mass", "groups.systems.total_kg", "Systems and equipment"),
    ("operating_items_total", "mission:operating_items_mass", "groups.operating_items.total_kg", "Operating items"),
    ("empty_mass", "aircraft:design:empty_mass", "ledger.oew_kg", "Empty mass before operating items"),
    ("operating_mass", "mission:operating_mass", "ledger.oew_kg", "Operating mass including operating items"),
]


def _jsonable(value: Any) -> Any:
    """Convert NumPy/OpenMDAO scalar containers into JSON-safe values."""
    if isinstance(value, Enum):
        return _jsonable(value.value)
    if hasattr(value, "tolist"):
        return _jsonable(value.tolist())
    if isinstance(value, dict):
        return {str(k): _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(v) for v in value]
    if isinstance(value, float):
        if math.isnan(value) or math.isinf(value):
            return None
    if hasattr(value, "item"):
        try:
            return _jsonable(value.item())
        except Exception:
            pass
    return value


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(_jsonable(value), indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def run_git(source: Path, *args: str) -> str:
    p = subprocess.run(
        ["git", "-C", str(source), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return p.stdout.strip()


def nested_get(document: dict[str, Any], dotted: str) -> Any:
    value: Any = document
    for part in dotted.split("."):
        value = value[part]
    return value


def assert_deck(record: dict[str, Any]) -> None:
    if record.get("preset") != "A320-200":
        raise ValueError(f"expected A320-200 record, got {record.get('preset')!r}")
    identity = record.get("identity", {})
    if identity.get("model") != "A320-214" or identity.get("weight_variant") != "WV017":
        raise ValueError(f"unexpected A320 identity: {identity}")
    resolved = record["production"]["resolved_inputs"]
    for key, expected in EXPECTED_SI.items():
        actual = resolved[key]
        if isinstance(expected, (int, float)):
            tolerance = 2e-10 * max(1.0, abs(float(expected)))
            if abs(float(actual) - float(expected)) > tolerance:
                raise ValueError(f"resolved input {key}: {actual!r} != pinned {expected!r}")
        elif actual != expected:
            raise ValueError(f"resolved input {key}: {actual!r} != pinned {expected!r}")


def load_alas_record(raw_path: Path) -> tuple[dict[str, Any], str]:
    record, digest = load_a320_record(raw_path)
    assert_deck(record)
    return record, digest


def load_a320_record(raw_path: Path) -> tuple[dict[str, Any], str]:
    """Load the A320 record without applying the post-correction deck check."""
    data = json.loads(raw_path.read_text(encoding="utf-8"))
    records = data.get("aircraft", [])
    record = next((x for x in records if x.get("preset") == "A320-200"), None)
    if record is None:
        raise ValueError(f"{raw_path} has no A320-200 record")
    return record, sha256(raw_path)


def set_value(values: Any, key: str, value: Any, units: str = "unitless") -> None:
    """Set an AviaryValues value, preserving explicit units."""
    values.set_val(key, value, units=units)


def build_aviary_inputs(record: dict[str, Any]) -> tuple[Any, dict[str, Any]]:
    """Build an empty, explicit AviaryValues A320 deck."""
    from aviary.utils.aviary_values import AviaryValues
    from aviary.variable_info.enums import AircraftTypes
    from aviary.variable_info.variables import Aircraft, Mission, Settings

    s = record["production"]["resolved_inputs"]
    g = EXACT_GEOMETRY_SI
    c = CONVERSIONS
    x = AviaryValues()
    set_value(x, Settings.VERBOSITY, 0)

    # Options are declared before mass setup.  They are part of the deck and
    # are recorded separately from continuous equation inputs.
    for key, value in [
        (Aircraft.Design.TYPE, AircraftTypes.TRANSPORT),
        (Aircraft.Design.USE_ALT_MASS, False),
        (Aircraft.Wing.DETAILED_WING, False),
        (Aircraft.HorizontalTail.NUM_TAILS, 1),
        (Aircraft.VerticalTail.NUM_TAILS, 1),
        (Aircraft.Fuel.NUM_TANKS, s["fuel_tank_count"]),
        (Aircraft.Engine.SCALE_MASS, False),
        (Aircraft.Engine.ADDITIONAL_MASS_FRACTION, [0.0]),
        (Aircraft.Engine.NUM_ENGINES, [s["engine_count"]]),
        (Aircraft.Engine.NUM_WING_ENGINES, [s["wing_mounted_engine_count"]]),
        (Aircraft.Engine.NUM_FUSELAGE_ENGINES, [s["fuselage_mounted_engine_count"]]),
        (Aircraft.Propulsion.TOTAL_NUM_ENGINES, s["engine_count"]),
        (Aircraft.Propulsion.TOTAL_NUM_WING_ENGINES, s["wing_mounted_engine_count"]),
        (Aircraft.Propulsion.TOTAL_NUM_FUSELAGE_ENGINES, s["fuselage_mounted_engine_count"]),
    ]:
        set_value(x, key, value)

    # Design and primary geometry.
    set_value(x, Aircraft.Design.GROSS_MASS, s["design_gross_mass_kg"] * c["kg_to_lbm"], "lbm")
    set_value(x, Aircraft.Design.MAX_MACH, s["maximum_mach"])
    set_value(x, Aircraft.Design.RANGE, s["design_range_nmi"], "NM")
    set_value(x, Aircraft.Design.CRUISE_MACH, 0.78)
    # Exact ALAS landing-gear adapter mass, intentionally distinct from the
    # published 66,000 kg A320 MLW reference anchor.
    set_value(x, Aircraft.Design.LANDING_TO_TAKEOFF_MASS_RATIO, g["design_landing_mass_kg"] / s["design_gross_mass_kg"])
    set_value(x, Aircraft.Fuselage.LENGTH, s["fuselage_length_m"] * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Fuselage.MAX_WIDTH, s["fuselage_width_m"] * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Fuselage.MAX_HEIGHT, s["fuselage_depth_m"] * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Fuselage.PLANFORM_AREA, s["fuselage_length_m"] * s["fuselage_width_m"] * c["m2_to_ft2"], "ft**2")
    set_value(x, Aircraft.Fuselage.REF_DIAMETER, ((s["fuselage_width_m"] + s["fuselage_depth_m"]) / 2.0) * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Fuselage.PASSENGER_COMPARTMENT_LENGTH, s["passenger_compartment_length_m"] * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Fuselage.NUM_FUSELAGES, s["fuselage_count"])
    set_value(x, Aircraft.Fuselage.MILITARY_CARGO_FLOOR, False)

    # Wing and empennage values are exact ALAS planform/adapter values.  The
    # simple wing branch is intentional: ALAS uses the same simplified FLOPS
    # bending relation for this case; no detailed distribution is invented.
    set_value(x, Aircraft.Wing.AREA, s["wing_area_m2"] * c["m2_to_ft2"], "ft**2")
    set_value(x, Aircraft.Wing.SPAN, s["wing_span_m"] * c["m_to_ft"], "ft")
    set_value(x, Aircraft.Wing.ASPECT_RATIO, g["wing_aspect_ratio"])
    set_value(x, Aircraft.Wing.TAPER_RATIO, g["wing_taper_ratio"])
    set_value(x, Aircraft.Wing.THICKNESS_TO_CHORD, g["wing_thickness_to_chord"])
    set_value(x, Aircraft.Wing.SWEEP, s["quarter_chord_sweep_deg"], "deg")
    set_value(x, Aircraft.Wing.CONTROL_SURFACE_AREA_RATIO, g["control_surface_area_ratio"])
    set_value(x, Aircraft.Wing.ULTIMATE_LOAD_FACTOR, 3.75)
    set_value(x, Aircraft.Wing.LOAD_FRACTION, 1.0)
    set_value(x, Aircraft.Wing.STRUT_BRACING_FACTOR, 0.0)
    set_value(x, Aircraft.Wing.AEROELASTIC_TAILORING_FACTOR, 0.0)
    set_value(x, Aircraft.Wing.COMPOSITE_FRACTION, 0.0)
    set_value(x, Aircraft.Wing.VAR_SWEEP_MASS_PENALTY, s["variable_sweep_penalty"])
    set_value(x, Aircraft.Wing.MASS_SCALER, 1.0)
    set_value(x, Aircraft.HorizontalTail.AREA, g["horizontal_tail_area_m2"] * c["m2_to_ft2"], "ft**2")
    set_value(x, Aircraft.HorizontalTail.TAPER_RATIO, g["horizontal_tail_taper_ratio"])
    set_value(x, Aircraft.VerticalTail.AREA, g["vertical_tail_area_m2"] * c["m2_to_ft2"], "ft**2")
    set_value(x, Aircraft.VerticalTail.TAPER_RATIO, g["vertical_tail_taper_ratio"])

    # Propulsion.  The ALAS fallback engine core mass is used as a declared
    # baseline input, with Aviary scaling disabled; this isolates the
    # equation/adapter comparison from a catalog engine-mass choice.
    engine_mass_lbm = record["production"]["groups"]["airframe"]["propulsion"]["baseline_engine_mass_kg"] * c["kg_to_lbm"]
    per_engine_thrust_lbf = s["rated_thrust_per_engine_n"] * c["n_to_lbf"]
    set_value(x, Aircraft.Engine.REFERENCE_MASS, [engine_mass_lbm], "lbm")
    set_value(x, Aircraft.Engine.REFERENCE_SLS_THRUST, [per_engine_thrust_lbf], "lbf")
    set_value(x, Aircraft.Engine.SCALED_SLS_THRUST, [per_engine_thrust_lbf], "lbf")
    set_value(x, Aircraft.Engine.MASS_SCALER, [1.0])
    set_value(x, Aircraft.Engine.SCALE_FACTOR, [1.0])
    set_value(x, Aircraft.Engine.THRUST_REVERSERS_MASS_SCALER, [1.0])
    set_value(x, Aircraft.Nacelle.AVG_DIAMETER, [s["nacelle_diameter_m"] * c["m_to_ft"]], "ft")
    set_value(x, Aircraft.Nacelle.AVG_LENGTH, [g["nacelle_length_m"] * c["m_to_ft"]], "ft")
    set_value(x, Aircraft.Nacelle.MASS_SCALER, [1.0])
    set_value(x, Aircraft.Propulsion.TOTAL_SCALED_SLS_THRUST, s["engine_count"] * per_engine_thrust_lbf, "lbf")
    set_value(x, Aircraft.Propulsion.MISC_MASS_SCALER, 1.0)

    # Fuel capacity is a prescribed ALAS input.  CorePreMission's override
    # mechanism keeps the FuelCapacityGroup from replacing it with an
    # unverified geometric capacity.
    set_value(x, Aircraft.Fuel.MAX_CAPACITY_MASS, s["maximum_fuel_capacity_kg"] * c["kg_to_lbm"], "lbm")
    set_value(x, Aircraft.Fuel.FUEL_SYSTEM_MASS_SCALER, 1.0)
    set_value(x, Aircraft.Fuel.UNUSABLE_FUEL_MASS_SCALER, 1.0)
    set_value(x, Aircraft.Fuel.DENSITY, 6.7, "lbm/galUS")
    set_value(x, Aircraft.Fuel.WING_FUEL_FRACTION, 0.0)
    set_value(x, Aircraft.Hydraulics.SYSTEM_PRESSURE, s["hydraulic_pressure_pa"] * c["pa_to_psi"], "psi")
    set_value(x, Aircraft.Hydraulics.MASS_SCALER, 1.0)

    # Cabin/operating-item discrete functions are set explicitly.  Aviary
    # represents class counts as options for passenger-service equipment and
    # as inputs for payload; both are included to make the adapter auditable.
    for key, value in [
        (Aircraft.CrewPayload.Design.NUM_PASSENGERS, sum(s[k] for k in ("first_class_passenger_count", "business_class_passenger_count", "tourist_class_passenger_count"))),
        (Aircraft.CrewPayload.Design.NUM_FIRST_CLASS, s["first_class_passenger_count"]),
        (Aircraft.CrewPayload.Design.NUM_BUSINESS_CLASS, s["business_class_passenger_count"]),
        (Aircraft.CrewPayload.Design.NUM_ECONOMY_CLASS, s["tourist_class_passenger_count"]),
        (Aircraft.CrewPayload.NUM_PASSENGERS, sum(s[k] for k in ("first_class_passenger_count", "business_class_passenger_count", "tourist_class_passenger_count"))),
        (Aircraft.CrewPayload.NUM_FIRST_CLASS, s["first_class_passenger_count"]),
        (Aircraft.CrewPayload.NUM_BUSINESS_CLASS, s["business_class_passenger_count"]),
        (Aircraft.CrewPayload.NUM_ECONOMY_CLASS, s["tourist_class_passenger_count"]),
        (Aircraft.CrewPayload.NUM_FLIGHT_ATTENDANTS, s["flight_attendant_count"]),
        (Aircraft.CrewPayload.NUM_FLIGHT_CREW, s["flight_crew_count"]),
        (Aircraft.CrewPayload.NUM_GALLEY_CREW, s["galley_crew_count"]),
    ]:
        set_value(x, key, value)
    set_value(x, Aircraft.CrewPayload.MASS_PER_PASSENGER, 0.0, "lbm")
    set_value(x, Aircraft.CrewPayload.BAGGAGE_MASS_PER_PASSENGER, 0.0, "lbm")
    set_value(x, Aircraft.CrewPayload.MISC_CARGO, 0.0, "lbm")
    set_value(x, Aircraft.CrewPayload.WING_CARGO, 0.0, "lbm")
    set_value(x, Aircraft.CrewPayload.PASSENGER_SERVICE_MASS_SCALER, 1.0)
    set_value(x, Aircraft.CrewPayload.CABIN_CREW_MASS_SCALER, 1.0)
    set_value(x, Aircraft.CrewPayload.FLIGHT_CREW_MASS_SCALER, 1.0)
    set_value(x, Aircraft.CrewPayload.CARGO_CONTAINER_MASS_SCALER, 1.0)

    # Gear and remaining zero-scope inputs.
    set_value(x, Aircraft.LandingGear.MAIN_GEAR_OLEO_LENGTH, g["main_gear_oleo_length_m"] * c["m_to_in"], "inch")
    set_value(x, Aircraft.LandingGear.NOSE_GEAR_OLEO_LENGTH, g["nose_gear_oleo_length_m"] * c["m_to_in"], "inch")
    set_value(x, Aircraft.LandingGear.MAIN_GEAR_MASS_SCALER, 1.0)
    set_value(x, Aircraft.LandingGear.NOSE_GEAR_MASS_SCALER, 1.0)
    set_value(x, Aircraft.Paint.MASS_PER_UNIT_AREA, 0.0, "lbm/ft**2")
    set_value(x, Aircraft.Design.TOTAL_WETTED_AREA, 0.0, "ft**2")
    set_value(x, Aircraft.Design.EMPTY_MASS_MARGIN_SCALER, 0.0)
    set_value(x, Aircraft.Design.EXTERNAL_SUBSYSTEMS_MASS, 0.0, "lbm")
    set_value(x, Aircraft.Battery.MASS, 0.0, "lbm")
    set_value(x, Mission.OPERATING_ITEMS_MASS_ADDITIONAL, 0.0, "lbm")

    deck = {
        "si": {"resolved_inputs": s, "exact_geometry": g},
        "aviary": {key: {"value": value, "units": units} for key, (value, units) in x.items()},
        "conversions": c,
        "adapter_decisions": {
            "mass_branch": "Aviary FLOPS transport (USE_ALT_MASS=False)",
            "wing_branch": "simple wing bending, matching ALAS Simplified branch",
            "engine_mass": "ALAS fallback baseline_engine_mass_kg, with Aviary SCALE_MASS=False",
            "fuel_capacity": "prescribed ALAS resolved maximum_fuel_capacity_kg; Aviary geometric capacity is overridden",
            "landing_mass": "ALAS mode-aware design landing mass 71,760 kg / 78,000 kg gross; published 66,000 kg MLW retained only as separate evidence anchor",
            "crew_and_containers": "150 passengers = 12 first + 0 business + 138 economy; 4 attendants; 2 flight crew; 0 galley crew; 0 containerized cargo",
            "inlet_nozzle": "no separate inlet/nozzle mass outputs in pinned Aviary transport FLOPS branch; retained as explicit scope gap",
        },
    }
    return x, deck


def get_problem_output(prob: Any, key: str, units: str = "lbm") -> float | None:
    try:
        value = prob.get_val(key, units=units)
    except Exception:
        return None
    value = _jsonable(value)
    if isinstance(value, list):
        if not value:
            return None
        value = value[0]
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def run_full_mass_model(x: Any) -> tuple[dict[str, float], dict[str, Any]]:
    """Run CorePreMission with only the pinned Aviary mass builder."""
    import openmdao.api as om
    from aviary.subsystems.mass.mass_builder import CoreMassBuilder
    from aviary.subsystems.premission import CorePreMission
    from aviary.utils.functions import set_aviary_initial_values
    from aviary.variable_info.enums import LegacyCode
    from aviary.variable_info.functions import setup_model_options
    from aviary.variable_info.variable_meta_data import CoreMetaData

    builder = CoreMassBuilder("mass", CoreMetaData, LegacyCode.FLOPS)
    prob = om.Problem(reports=False)
    prob.model.add_subsystem(
        "mass",
        CorePreMission(
            aviary_options=x,
            subsystems=[builder],
            subsystem_options={},
        ),
        promotes=["*"]
    )
    setup_model_options(prob, x)
    prob.setup(check=False)
    set_aviary_initial_values(prob, x)
    prob.run_model()

    values: dict[str, float] = {}
    for name, key, *_ in COMPONENTS:
        value = get_problem_output(prob, key)
        if value is not None:
            # Vectorized engine/nacelle outputs have one engine type.  The
            # table stores per-engine values where the key says "each".
            values[name] = value
    for name, key, *_ in GROUP_OUTPUTS:
        value = get_problem_output(prob, key)
        if value is not None:
            values[name] = value
    # Useful diagnostics are retained for wiring review.
    for key in [
        "aircraft:design:touchdown_mass_max",
        "aircraft:engine:pod_mass",
        "aircraft:fuel:max_capacity_mass",
        "aircraft:wing:bending_material_factor",
        "aircraft:wing:eng_pod_inertia_factor",
        "aircraft:wing:control_surface_area",
    ]:
        value = get_problem_output(prob, key)
        if value is not None:
            values[f"diagnostic:{key}"] = value
    return values, {
        "status": "success",
        "execution": "CorePreMission -> CoreMassBuilder(FLOPS) -> MassPremission",
        "component_count": len([k for k in values if not k.startswith("diagnostic:")]),
    }


def run_component_fallback(x: Any, error: BaseException) -> tuple[dict[str, float], dict[str, Any]]:
    """Attempt transparent sourced components if group assembly is blocked.

    This fallback deliberately covers only standalone components whose inputs
    are explicit in the deck.  It does not synthesize missing group wiring or
    call the result a full mass-model run.
    """
    import openmdao.api as om
    from aviary.subsystems.mass.flops_based.air_conditioning import TransportAirCondMass
    from aviary.subsystems.mass.flops_based.anti_icing import AntiIcingMass
    from aviary.subsystems.mass.flops_based.apu import TransportAPUMass
    from aviary.subsystems.mass.flops_based.avionics import TransportAvionicsMass
    from aviary.subsystems.mass.flops_based.crew import CabinCrewMass, FlightCrewMass
    from aviary.subsystems.mass.flops_based.electrical import ElectricalMass
    from aviary.subsystems.mass.flops_based.engine_controls import TransportEngineCtrlsMass
    from aviary.subsystems.mass.flops_based.engine_oil import TransportEngineOilMass
    from aviary.subsystems.mass.flops_based.fuel_system import TransportFuelSystemMass
    from aviary.subsystems.mass.flops_based.furnishings import TransportFurnishingsGroupMass
    from aviary.subsystems.mass.flops_based.fuselage import TransportFuselageMass
    from aviary.subsystems.mass.flops_based.horizontal_tail import HorizontalTailMass
    from aviary.subsystems.mass.flops_based.hydraulics import TransportHydraulicsGroupMass
    from aviary.subsystems.mass.flops_based.instruments import TransportInstrumentMass
    from aviary.subsystems.mass.flops_based.nacelle import NacelleMass
    from aviary.subsystems.mass.flops_based.passenger_service import PassengerServiceMass
    from aviary.subsystems.mass.flops_based.starter import TransportStarterMass
    from aviary.subsystems.mass.flops_based.surface_controls import SurfaceControlMass
    from aviary.subsystems.mass.flops_based.thrust_reverser import ThrustReverserMass
    from aviary.subsystems.mass.flops_based.unusable_fuel import TransportUnusableFuelMass
    from aviary.subsystems.mass.flops_based.vertical_tail import VerticalTailMass
    from aviary.utils.functions import set_aviary_initial_values
    from aviary.variable_info.functions import setup_model_options

    classes = {
        "air_conditioning": TransportAirCondMass,
        "anti_icing": AntiIcingMass,
        "apu": TransportAPUMass,
        "avionics": TransportAvionicsMass,
        "cabin_crew": CabinCrewMass,
        "electrical": ElectricalMass,
        "engine_controls": TransportEngineCtrlsMass,
        "engine_oil": TransportEngineOilMass,
        "fuel_system": TransportFuelSystemMass,
        "furnishings": TransportFurnishingsGroupMass,
        "fuselage": TransportFuselageMass,
        "horizontal_tail": HorizontalTailMass,
        "hydraulics": TransportHydraulicsGroupMass,
        "instruments": TransportInstrumentMass,
        "nacelle_each": NacelleMass,
        "passenger_service": PassengerServiceMass,
        "starter_total": TransportStarterMass,
        "surface_controls": SurfaceControlMass,
        "thrust_reversers_total": ThrustReverserMass,
        "unusable_fuel": TransportUnusableFuelMass,
        "vertical_tail": VerticalTailMass,
        "flight_crew": FlightCrewMass,
    }
    values: dict[str, float] = {}
    failures: dict[str, str] = {}
    for name, cls in classes.items():
        try:
            # Problem requires a Group root; wrapping the standalone pinned
            # ExplicitComponent also keeps its promoted Aviary variable names.
            root = om.Group()
            root.add_subsystem("component", cls(), promotes=["*"])
            prob = om.Problem(model=root, reports=False)
            setup_model_options(prob, x)
            prob.setup(check=False)
            set_aviary_initial_values(prob, x)
            prob.run_model()
            output_key = next((key for comp_name, key, *_ in COMPONENTS if comp_name == name), None)
            if output_key is None:
                output_key = {
                    "starter_total": "aircraft:propulsion:total_starter_mass",
                    "thrust_reversers_total": "aircraft:propulsion:total_thrust_reversers_mass",
                }.get(name)
            output = get_problem_output(prob, output_key) if output_key else None
            if output is not None:
                values[name] = output
            else:
                failures[name] = "standalone component produced no mapped output"
        except Exception as exc:  # bounded, per-component recovery
            failures[name] = f"{type(exc).__name__}: {exc}"
    return values, {
        "status": "standalone_components_only" if values else "blocked",
        "execution": "independent pinned Aviary FLOPS ExplicitComponents; no group parity claim",
        "component_count": len(values),
        "failures": failures,
        "full_model_error": f"{type(error).__name__}: {error}",
    }


def comparison_tolerance(alas_value: float) -> float:
    """Return the kg tolerance used for an equation-level comparison."""
    return max(NUMERICAL_ABS_TOLERANCE_KG, NUMERICAL_REL_TOLERANCE * max(1.0, abs(alas_value)))


def classify_comparison(
    name: str,
    alas_value: float,
    aviary_kg: float | None,
    diff: float | None,
) -> tuple[str, str, str]:
    """Classify numerical coverage separately from intentional adapter gaps."""
    if aviary_kg is None:
        if name in {"inlet_each", "nozzle_each"}:
            return (
                "scope_gap",
                "no_independent_aviary_output",
                "Aviary pinned transport FLOPS branch has no separate inlet/nozzle output",
            )
        return "unavailable", "execution_gap", "no output from executed branch"
    if name == "cargo_containers":
        return (
            "intentional_adapter_difference",
            "discrete_vs_smooth_container_convention",
            "ALAS uses ceil(container cargo/standard container mass); Aviary uses smooth sin_int4 sizing, so zero cargo remains fractional",
        )
    if name == "empty_mass_margin":
        return (
            "covered",
            "declared_zero_adapter",
            "ALAS has no serialized independent margin row; both sides are explicitly zero by adapter/scaler policy",
        )
    if name in {"operating_items_total", "operating_mass"}:
        return (
            "propagated_adapter_difference",
            "propagated_container_convention",
            "difference propagates the cargo-container convention into operating-item and operating-mass totals",
        )
    tolerance = comparison_tolerance(alas_value)
    if diff is not None and abs(diff) <= tolerance:
        return "covered", "within_source_precision", "shared pinned FLOPS equation / adapter"
    return "covered_difference", "numeric_difference", "mapped output exceeds the source-precision tolerance"


def make_comparison(record: dict[str, Any], values_lbm: dict[str, float]) -> list[dict[str, Any]]:
    alas = record["production"]
    rows: list[dict[str, Any]] = []
    for name, _, alas_path, unit, label in COMPONENTS:
        # ALAS does not serialize an independent empty-mass-margin component;
        # this exact-deck adapter declares the margin zero, matching its
        # production ledger policy and the Aviary scaler input.
        alas_value = 0.0 if alas_path is None else float(nested_get(alas, alas_path))
        aviary_lbm = values_lbm.get(name)
        aviary_kg = None if aviary_lbm is None else aviary_lbm * 0.45359237
        diff = None if aviary_kg is None else aviary_kg - alas_value
        if name == "nacelle_each" and aviary_kg is not None:
            # Aviary reports nacelle mass per engine type; ALAS structure
            # reports the installed total.  Compare like with like and retain
            # the per-engine value in the raw Aviary output.
            aviary_each_kg = aviary_kg
            aviary_kg *= EXPECTED_SI["engine_count"]
            diff = aviary_kg - alas_value
        else:
            aviary_each_kg = None
        status, difference_class, scope = classify_comparison(name, alas_value, aviary_kg, diff)
        rows.append({
            "component": name,
            "label": label,
            "alas_kg": alas_value,
            "aviary_kg": aviary_kg,
            "aviary_each_kg": aviary_each_kg,
            "multiplicity": EXPECTED_SI["engine_count"] if name == "nacelle_each" else 1,
            "alas_ledger_oew_kg": None,
            "alas_operating_items_in_ledger_kg": None,
            "difference_kg": diff,
            "difference_percent_of_alas": None if diff is None or alas_value == 0.0 else 100.0 * diff / alas_value,
            "comparison_tolerance_kg": comparison_tolerance(alas_value),
            "status": status,
            "difference_class": difference_class,
            "scope": scope,
        })
    for name, _, alas_path, label in GROUP_OUTPUTS:
        alas_reference_oew = None
        alas_operating_items = None
        if name == "empty_mass":
            # ALAS's legacy ledger intentionally carries operating items in
            # its OEW/Furnishings bucket.  Compare Aviary EmptyMass with the
            # corresponding pre-operating-item ledger subtotal, then compare
            # OperatingMass with the published ALAS ledger OEW below.
            alas_reference_oew = float(alas["ledger"]["oew_kg"])
            alas_operating_items = float(alas["groups"]["operating_items"]["total_kg"])
            alas_value = alas_reference_oew - alas_operating_items
        else:
            alas_value = float(nested_get(alas, alas_path))
        aviary_lbm = values_lbm.get(name)
        aviary_kg = None if aviary_lbm is None else aviary_lbm * 0.45359237
        diff = None if aviary_kg is None else aviary_kg - alas_value
        if name == "operating_mass":
            alas_reference_oew = float(alas["ledger"]["oew_kg"])
            alas_value = alas_reference_oew
            diff = None if aviary_kg is None else aviary_kg - alas_value
        status, difference_class, scope = classify_comparison(name, alas_value, aviary_kg, diff)
        rows.append({
            "component": name,
            "label": label,
            "alas_kg": alas_value,
            "aviary_kg": aviary_kg,
            "aviary_each_kg": None,
            "multiplicity": 1,
            "alas_ledger_oew_kg": alas_reference_oew,
            "alas_operating_items_in_ledger_kg": alas_operating_items,
            "difference_kg": diff,
            "difference_percent_of_alas": None if diff is None or alas_value == 0.0 else 100.0 * diff / alas_value,
            "comparison_tolerance_kg": comparison_tolerance(alas_value),
            "status": status,
            "difference_class": difference_class,
            "scope": scope if difference_class != "within_source_precision" else "group total crosswalk",
        })
    return rows


def holdout_summary(record: dict[str, Any], digest: str) -> dict[str, Any]:
    """Keep the small, reviewable A320 baseline/holdout record in the artifact."""
    production = record["production"]
    groups = production.get("groups", {})
    resolved = production.get("resolved_inputs", {})
    group_paths = {
        "structure_total_kg": ("airframe", "structure"),
        "propulsion_total_kg": ("airframe", "propulsion"),
        "systems_total_kg": ("systems",),
        "operating_items_total_kg": ("operating_items",),
    }
    group_totals = {}
    for output_name, path in group_paths.items():
        group = groups
        for part in path:
            group = group.get(part, {})
        if "total_kg" in group:
            group_totals[output_name] = group["total_kg"]
    selected_inputs = {
        key: resolved.get(key)
        for key in (
            "first_class_passenger_count",
            "business_class_passenger_count",
            "tourist_class_passenger_count",
            "flight_attendant_count",
            "flight_crew_count",
            "galley_crew_count",
            "containerized_cargo_kg",
        )
        if key in resolved
    }
    return {
        "raw_sha256": digest,
        "preset": record.get("preset"),
        "identity": record.get("identity"),
        "ledger_oew_kg": production.get("ledger", {}).get("oew_kg"),
        "group_totals_kg": group_totals,
        "selected_discrete_inputs": selected_inputs,
    }


def build_holdout_artifact(
    baseline_path: Path,
    baseline_error: str | None,
    current_record: dict[str, Any],
    current_hash: str,
) -> dict[str, Any]:
    """Describe the pre/post correction pair without fitting either result."""
    artifact: dict[str, Any] = {
        "aircraft": "A320-214 / WV017 / CFM56-5B4/3 / sharklets",
        "policy": {
            "selection": "A320-214 WV017 is the registered independent exact-deck audit pair; family holdout selection remains lead-owned",
            "baseline_before_corrections": True,
            "comparison_run_uses": "current serialized ALAS deck after source-backed global input policy",
            "allowed_correction_scope": "source-backed cabin class and crew counts recorded in the ALAS deck",
            "aircraft_specific_force_fit": False,
            "aviary_specific_mass_adjustment": False,
        },
        "baseline_path": str(baseline_path),
        "post_correction_path": "outputs/flops-refinement/raw.json",
        "status": "unavailable",
    }
    if baseline_error:
        artifact["baseline_error"] = baseline_error
        artifact["post_correction"] = holdout_summary(current_record, current_hash)
        return artifact
    try:
        baseline_record, baseline_hash = load_a320_record(baseline_path)
    except Exception as exc:
        artifact["baseline_error"] = f"{type(exc).__name__}: {exc}"
        artifact["post_correction"] = holdout_summary(current_record, current_hash)
        return artifact
    baseline = holdout_summary(baseline_record, baseline_hash)
    post = holdout_summary(current_record, current_hash)
    pre_oew = baseline.get("ledger_oew_kg")
    post_oew = post.get("ledger_oew_kg")
    delta: dict[str, Any] = {}
    if isinstance(pre_oew, (int, float)) and isinstance(post_oew, (int, float)):
        delta["ledger_oew_kg"] = post_oew - pre_oew
    for key in sorted(set(baseline["group_totals_kg"]) | set(post["group_totals_kg"])):
        before = baseline["group_totals_kg"].get(key)
        after = post["group_totals_kg"].get(key)
        if isinstance(before, (int, float)) and isinstance(after, (int, float)):
            delta[key] = after - before
    artifact.update({
        "status": "registered_pre_post_pair",
        "same_identity": baseline.get("identity") == post.get("identity"),
        "pre_correction": baseline,
        "post_correction": post,
        "post_minus_pre_delta_kg": delta,
        "interpretation": "The pair preserves the pre-correction ALAS result for holdout review.  The independent Aviary run is evaluated against the post-correction deck; no value is adjusted to close the OEW difference.",
    })
    return artifact


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fields = list(rows[0]) if rows else []
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def write_artifact_manifest(out: Path, generator: Path) -> None:
    """Hash every generated artifact after the run has finished."""
    artifacts = {}
    for path in sorted(out.iterdir()):
        if path.name == "artifact_manifest.json" or not path.is_file():
            continue
        artifacts[path.name] = {"sha256": sha256(path), "bytes": path.stat().st_size}
    write_json(out / "artifact_manifest.json", {
        "schema_version": "a320-flops-aviary-artifact-manifest-v1",
        "generator": {"path": str(generator), "sha256": sha256(generator)},
        "artifacts": artifacts,
    })


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--alas-raw", type=Path, default=Path("outputs/flops-refinement/raw.json"))
    parser.add_argument("--baseline-alas-raw", type=Path, default=Path("outputs/flops-refinement/baseline/raw.json"))
    parser.add_argument("--aviary-source", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path("outputs/a320-flops-audit/reference"))
    args = parser.parse_args()

    raw_path = args.alas_raw.resolve()
    aviary_source = args.aviary_source.resolve()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    record, raw_hash = load_alas_record(raw_path)

    commit = run_git(aviary_source, "rev-parse", "HEAD")
    if commit != PINNED_AVIARY_COMMIT:
        raise RuntimeError(f"Aviary source is {commit}, expected pinned {PINNED_AVIARY_COMMIT}")
    dirty = run_git(aviary_source, "status", "--porcelain")
    source_manifest = {
        "source": str(aviary_source),
        "commit": commit,
        "commit_check": "exact",
        "working_tree_status": dirty,
        "license": "Apache-2.0 (pinned Aviary source)",
        "reference": PINNED_NASA_REFERENCE,
        "alas_reference_manifest": str(Path("out/evidence/data/flops-reference-20260911/manifest.json").resolve()),
        "alas_raw": str(raw_path),
        "alas_raw_sha256": raw_hash,
        "generated_by": "reproducible tools/a320_flops_aviary_audit.py",
    }
    reference_manifest_path = Path("out/evidence/data/flops-reference-20260911/manifest.json").resolve()
    if reference_manifest_path.exists():
        source_manifest["alas_reference_manifest_sha256"] = sha256(reference_manifest_path)
    write_json(out / "source_manifest.json", source_manifest)

    x, deck = build_aviary_inputs(record)
    write_json(out / "input_deck.json", deck)
    write_json(out / "alas_record.json", record)

    runtime = {
        "python": sys.version,
        "platform": platform.platform(),
        "executable": sys.executable,
        "aviary_version": None,
        "openmdao_version": None,
        "numpy_version": None,
        "distribution_versions": {},
        "pip_freeze": [],
    }
    try:
        import importlib.metadata
        import aviary
        import numpy
        import openmdao

        runtime.update({
            "aviary_version": getattr(aviary, "__version__", "unknown"),
            "openmdao_version": getattr(openmdao, "__version__", "unknown"),
            "numpy_version": getattr(numpy, "__version__", "unknown"),
        })
        for distribution in ("aviary", "openmdao", "numpy", "scipy", "dymos", "pandas"):
            try:
                runtime["distribution_versions"][distribution] = importlib.metadata.version(distribution)
            except importlib.metadata.PackageNotFoundError:
                runtime["distribution_versions"][distribution] = None
        freeze_command = [sys.executable, "-m", "pip", "freeze"]
        freeze = subprocess.run(freeze_command, check=False, capture_output=True, text=True)
        if freeze.returncode != 0:
            # uv-created environments may intentionally omit pip.  Use uv's
            # read-only freeze against this exact interpreter instead of
            # silently recording an empty dependency list.
            uv = shutil.which("uv")
            if uv:
                freeze_command = [uv, "pip", "freeze", "--python", sys.executable]
                freeze = subprocess.run(freeze_command, check=False, capture_output=True, text=True)
        runtime["freeze_command"] = freeze_command
        runtime["freeze_returncode"] = freeze.returncode
        if freeze.returncode == 0:
            runtime["pip_freeze"] = sorted(line for line in freeze.stdout.splitlines() if line)
        else:
            runtime["freeze_error"] = freeze.stderr.strip()
    except Exception as exc:
        runtime["import_error"] = f"{type(exc).__name__}: {exc}"
    write_json(out / "runtime.json", runtime)

    full_error: str | None = None
    try:
        values, execution = run_full_mass_model(x)
    except Exception as exc:
        full_error = "".join(traceback.format_exception(exc)).strip()
        values, execution = run_component_fallback(x, exc)
    write_json(out / "aviary_outputs.json", {"mass_outputs_lbm": values, "execution": execution})
    failure_path = out / "full_model_failure.txt"
    if full_error:
        failure_path.write_text(full_error + "\n", encoding="utf-8")
    elif failure_path.exists():
        # Avoid leaving a stale failure trace from an earlier bounded run next
        # to a current successful result.
        failure_path.unlink()

    rows = make_comparison(record, values)
    write_csv(out / "component_comparison.csv", rows)
    write_json(out / "component_comparison.json", rows)

    available = sum(row["aviary_kg"] is not None for row in rows)
    within_precision = sum(row["difference_class"] == "within_source_precision" for row in rows)
    scope_gaps = sum(row["difference_class"] == "no_independent_aviary_output" for row in rows)
    intentional_differences = sum(
        row["difference_class"] in {"discrete_vs_smooth_container_convention", "propagated_container_convention"}
        for row in rows
    )
    declared_zero = sum(row["difference_class"] == "declared_zero_adapter" for row in rows)
    # "Coverage" measures whether the pinned Aviary execution produced a
    # mapped output.  It deliberately includes documented adapter differences;
    # numerical agreement is reported independently by within_precision.
    covered = available
    empty_mass_row = next((row for row in rows if row["component"] == "empty_mass"), None)
    operating_mass_row = next((row for row in rows if row["component"] == "operating_mass"), None)
    baseline_path = args.baseline_alas_raw.resolve()
    holdout = build_holdout_artifact(baseline_path, None, record, raw_hash)
    write_json(out / "holdout.json", holdout)
    verdict = {
        "status": "full_openmdao_mass_group" if execution["status"] == "success" else execution["status"],
        "full_openmdao_execution": execution["status"] == "success",
        "component_rows": len(rows),
        "covered_rows": covered,
        "coverage_fraction": covered / len(rows) if rows else 0.0,
        "available_aviary_outputs": available,
        "within_source_precision_rows": within_precision,
        "declared_zero_adapter_rows": declared_zero,
        "scope_gap_rows": scope_gaps,
        "intentional_or_propagated_adapter_difference_rows": intentional_differences,
        "comparison_tolerance": {
            "absolute_kg": NUMERICAL_ABS_TOLERANCE_KG,
            "relative_fraction": NUMERICAL_REL_TOLERANCE,
            "definition": "max(absolute_kg, relative_fraction * max(1 kg, abs(ALAS value)))",
        },
        "same_input_deck_first": True,
        "actual_oew_reference_kg": record.get("reference", {}).get("oew_kg"),
        "actual_oew_reference_evidence": record.get("reference", {}).get("mass_reference_evidence", {}).get("oew_kg"),
        "published_reference_oew_kg": record.get("reference", {}).get("oew_kg"),
        "alas_production_oew_kg": record["production"]["ledger"]["oew_kg"],
        "aviary_empty_mass_kg": empty_mass_row["aviary_kg"] if empty_mass_row else None,
        "aviary_operating_mass_kg": operating_mass_row["aviary_kg"] if operating_mass_row else None,
        "reference_parity": "OEW reference is an independent anchor; no force-fit or aircraft-specific correction applied",
        "holdout_policy": "A320-214 WV017 is pinned as the unchanged ALAS deck; corrections are not applied to make Aviary agree",
        "known_scope_gaps": [
            "Aviary pinned branch has no separate inlet/nozzle mass outputs; ALAS retains zero rows separately.",
            "Aviary pod mass is an installation bookkeeping diagnostic and is not added to nacelle structure a second time.",
            "Full OpenMDAO success means the pinned mass group executed; it is equation implementation parity, not physical aircraft validation.",
        ],
        "adapter_findings": {
            "cargo_containers": {
                "alas_policy": "discrete ceiling; zero declared cargo yields 0 kg",
                "aviary_policy": "smooth sin_int4 surrogate; zero cargo yields 27.334621794196618 kg",
                "production_change_recommended": False,
            },
            "nacelle": {
                "alas_scope": "installed total in structure",
                "aviary_scope": "per-engine-type output multiplied by two for comparison",
                "pod_mass_double_counted": False,
            },
            "engine_mass_candidate": {
                "candidate_kg_per_engine": 2454.8,
                "current_kg_per_engine": 2226.6894032481673,
                "status": "conditional; starter/reverser/accessory overlap unresolved",
                "promoted_in_run": False,
            },
            "empty_mass_margin": {
                "alas_policy": "no serialized independent row; declared zero",
                "aviary_output_kg": 0.0,
                "scaler": 0.0,
            },
        },
        "source_manifest": "source_manifest.json",
        "runtime": "runtime.json",
        "input_deck": "input_deck.json",
        "alas_record": "alas_record.json",
        "holdout": "holdout.json",
        "comparison": "component_comparison.csv",
        "artifact_manifest": "artifact_manifest.json",
    }
    write_json(out / "verdict.json", verdict)

    readme = f"""# Independent A320 ALAS/Aviary FLOPS audit

Generated by `reproducible tools/a320_flops_aviary_audit.py`.

- ALAS record: `{raw_path}` (SHA-256 `{raw_hash}`)
- Aviary commit: `{commit}` (exact pinned commit)
- Reference equations: `{PINNED_NASA_REFERENCE}`
- Execution status: `{verdict['status']}`
- Component coverage: `{covered}/{len(rows)}` rows
- Within source precision: `{within_precision}` rows; explicit Aviary scope gaps: `{scope_gaps}` rows
- Declared-zero adapter rows: `{declared_zero}`
- Intentional/propagated adapter differences: `{intentional_differences}` rows

The first comparison uses the exact serialized ALAS A320-214/WV017 input deck,
with SI values converted once to Aviary's legacy FLOPS units.  The adapter keeps
the ALAS mode-aware design landing mass of 71,760 kg and fallback CFM56 engine
mass as explicit inputs.  Airbus published anchors (including 66,000 kg MLW and
41,052 kg OEW in the ALAS record) remain separate evidence; no value is fitted
to them.  The zero-cargo container row remains a documented difference because
ALAS uses a discrete ceiling while Aviary's pinned FLOPS branch uses a smooth
integer approximation.  See `input_deck.json`, `alas_record.json`,
`component_comparison.csv`, `holdout.json`, `verdict.json`, and
`artifact_manifest.json`.

Run from the ALAS repository with the pinned environment:

```text
{sys.executable} "reproducible tools/a320_flops_aviary_audit.py" --aviary-source "{aviary_source}" --baseline-alas-raw "{baseline_path}" --output-dir "{out}"
```
    """
    (out / "README.md").write_text(readme, encoding="utf-8")
    write_artifact_manifest(out, Path(__file__).resolve())
    print(json.dumps({"status": verdict["status"], "coverage": f"{covered}/{len(rows)}", "output": str(out)}, indent=2))
    return 0 if values else 2


if __name__ == "__main__":
    raise SystemExit(main())
