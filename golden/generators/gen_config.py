# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Default values and form descriptions for every configuration dataclass.

A configuration module is not interesting arithmetic, which is exactly why it
needs a fixture: several hundred defaults copied by hand is several hundred
chances to transpose a digit, and a wrong default produces a plausible
aircraft rather than a failure. The two things worth capturing are what a
freshly constructed configuration holds, and what the settings interface is
told about each field -- the second because a wrong unit or a missing bound is
just as capable of producing a wrong number, by way of the person filling the
form in.

``declared`` records whether the dataclass field stated its own ``help``, as
opposed to leaving ``schema.py`` to emit an empty one. The port supplies an
explanation for every field, including the ones upstream left blank
(CONTRIBUTING.md requires it), so the parity test compares ``help`` only
where upstream declared one. ``label`` is compared always, derived ones
included: a derived label that disagrees means the name-to-label rule was
ported wrong, and every label is a key into the translation catalog.

Adding a configuration module to this fixture is one entry in ``TYPES``.
"""

from __future__ import annotations

import dataclasses

import _framework

# (fixture key, module under alas.config, class name). Order is the order the
# fixture lists them in, which is the order they are ported in.
TYPES = [
    ("DragModelConfig", "physics_config", "DragModelConfig"),
    ("MissionConfig", "mission_config", "MissionConfig"),
    ("MSESConfig", "mses_config", "MSESConfig"),
    ("AnalysisConfig", "analysis_config", "AnalysisConfig"),
    ("MassModelConfig", "mass_config", "MassModelConfig"),
    ("LandingGearConfig", "landing_gear_config", "LandingGearConfig"),
    ("ControlSurfacesConfig", "control_surfaces_config", "ControlSurfacesConfig"),
    ("PerformanceConfig", "performance_config", "PerformanceConfig"),
    ("PropulsionCycleConfig", "propulsion_config", "PropulsionCycleConfig"),
    ("DesignRequirements", "requirements", "DesignRequirements"),
    ("StructuresConfig", "structures_config", "StructuresConfig"),
    ("CabinConfig", "cabin_config", "CabinConfig"),
    ("OptimizerConfig", "optimizer_config", "OptimizerConfig"),
    ("GeometryConfig", "geometry_config", "GeometryConfig"),
    # GeometryConfig hides its engine group from the form, so nothing above
    # describes those thirteen fields. Captured separately rather than left
    # unverified: it has its own editor upstream, and a label or unit ported
    # wrong there is as capable of producing a wrong engine as one anywhere
    # else.
    ("EngineConfig", "geometry_config", "EngineConfig"),
    # The aggregate the pipeline consumes. Its own three fields are a preset
    # name and the two airports, and everything else is one of the groups
    # above nested inside it -- so what this entry actually pins down is the
    # composition: which groups a run is made of, in which order the settings
    # screen lists them, and that each one arrives at its own defaults.
    ("ALASConfig", "settings", "ALASConfig"),
]


def _declared(instance, prefix: str, out: dict) -> dict:
    """Which prose each field stated for itself, rather than inheriting.

    Keyed by the field's dotted path from the configuration's root rather
    than by its type's name: a path is the same on both sides of the port,
    while a type name is subject to each language's naming conventions
    (``MSESConfig`` here is ``MsesConfig`` in Rust).
    """
    for field in dataclasses.fields(instance):
        metadata = dict(field.metadata or {})
        path = f"{prefix}.{field.name}"
        out[path] = {"help": "help" in metadata}
        value = getattr(instance, field.name)
        if dataclasses.is_dataclass(value):
            _declared(value, path, out)
    return out


def main() -> None:
    _framework.add_alas_to_path()

    from alas.sidecar.schema import dataclass_schema

    types = {}
    for key, module_name, class_name in TYPES:
        module = __import__(f"alas.config.{module_name}", fromlist=[class_name])
        instance = getattr(module, class_name)()

        types[key] = {
            "defaults": _framework_serializable(dataclasses.asdict(instance)),
            "schema": dataclass_schema(instance),
            "declared": _declared(instance, "", {}),
        }

    _framework.write(
        "config",
        "defaults",
        {"types": types},
        description=(
            "every configuration dataclass as freshly constructed: its default "
            "values, the form description alas.sidecar.schema derives from it, "
            "and which fields stated their own label and help"
        ),
    )


def _framework_serializable(obj):
    """Tuples become lists, as ``ALASConfig.to_dict`` does before saving."""
    if isinstance(obj, dict):
        return {k: _framework_serializable(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_framework_serializable(v) for v in obj]
    return obj


if __name__ == "__main__":
    main()
