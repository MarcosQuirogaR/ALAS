# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Config/schema/preset endpoints.

Thin wrappers over :class:`~alas.config.settings.ALASConfig` and
:mod:`alas.config.presets` -- the same objects ``cli.py`` already uses,
so a config JSON produced here round-trips through
``ALASConfig.from_dict``/``to_dict`` exactly like a YAML file does today.
"""

from __future__ import annotations

import dataclasses
from typing import Any, Dict

from fastapi import APIRouter, HTTPException

from ..config.airports import AIRPORT_NAMES
from ..config.design_variables import DESIGN_VARIABLE_SPECS, DesignVector
from ..config.fidelity_presets import (
    available_fidelity_presets,
    fidelity_preset_display_names,
    get_fidelity_preset,
)
from ..config.performance_presets import (
    available_performance_presets,
    get_performance_preset,
    performance_preset_display_names,
)
from ..config.presets import available_presets, get_preset, preset_display_names
from ..config.settings import ALASConfig
from ..config.solver_presets import (
    available_solver_presets,
    get_solver_preset,
    solver_preset_display_names,
)
from .schema import _humanize, dataclass_schema

router = APIRouter()

# Aux-preset registries (analysis fidelity / solver / field-performance
# assumptions) -- unlike the aircraft preset above, each of these only fills
# in one config *group* (or, for "solver", one nested sub-group) rather than
# the whole ALASConfig, so they're exposed generically by kind instead of
# each getting their own dedicated route.
_AUX_PRESET_REGISTRIES: Dict[str, Dict[str, Any]] = {
    "fidelity": {
        "available": available_fidelity_presets,
        "display_names": fidelity_preset_display_names,
        "get": get_fidelity_preset,
        "group": "analysis",
        "attr": "analysis",
    },
    "solver": {
        "available": available_solver_presets,
        "display_names": solver_preset_display_names,
        "get": get_solver_preset,
        "group": "optimizer",
        "subpath": "solver",
        "attr": "settings",
    },
    "performance": {
        "available": available_performance_presets,
        "display_names": performance_preset_display_names,
        "get": get_performance_preset,
        "group": "performance",
        "attr": "settings",
    },
}


@router.get("/engines")
def get_engines() -> dict:
    """Engine names (config/engines.py's ENGINE_DATABASE) plus headline specs
    for tooltips. The Inputs screen's Engine dropdown used to hardcode a copy
    of this list ("the sidecar has no dedicated list endpoint") -- which would
    silently drift the moment an engine was added to the database.
    """
    from ..config.engines import ENGINE_DATABASE, available_engines

    specs = {}
    for name in available_engines():
        e = ENGINE_DATABASE[name]
        specs[name] = {
            "manufacturer": e.manufacturer,
            "thrust_kn": float(e.thrust_kn),
            "bypass_ratio": float(e.bypass_ratio),
            "fan_diameter_m": float(e.fan_diameter_m),
        }
    return {"names": available_engines(), "specs": specs}


@router.get("/airports")
def get_airports() -> dict:
    """The 20 curated airports (``config/airports.py``) the Route card's
    departure/arrival dropdowns are built from.

    Free-text ``departure_airport``/``arrival_airport`` fields would raise a
    ``KeyError`` at run time rather than at the input, since
    ``pipeline.py``'s ``get_airport()`` lookup only recognizes an exact
    match against this list -- constraining the dropdown to real entries is
    a correctness fix, not just a UX one. SimBrief (when configured) still
    takes priority over whichever of these is selected -- see
    ``routing/simbrief_route.py`` and ``pipeline.py``'s mission-routing stage.
    """
    return {"names": AIRPORT_NAMES}


@router.get("/config/default")
def get_default_config() -> dict:
    """The default config plus the nominal (AVE) design vector.

    Both are always returned together (see ``/config/preset/{name}`` below):
    a config alone can't drive ``DesignSpaceTable`` (Phase 3), which needs the
    matching initial design vector the way the desktop UI's preset
    handling and ``DesignPipeline.run``'s own ``initial_design`` fallback
    both do.
    """
    return {
        "config": ALASConfig().to_dict(),
        "design_vector": dataclasses.asdict(DesignVector.default()),
    }


@router.get("/config/presets")
def list_presets() -> dict:
    names = available_presets()
    display = preset_display_names()
    return {"names": names, "display_names": display}


@router.get("/config/preset/{name}")
def get_preset_config(name: str) -> dict:
    try:
        preset = get_preset(name)
    except KeyError:
        raise HTTPException(status_code=404, detail=f"Unknown preset '{name}'")
    config = ALASConfig.from_dict({"preset": name}).to_dict()
    return {"config": config, "design_vector": dataclasses.asdict(preset.design_vector)}


@router.get("/config/aux-presets/{kind}")
def list_aux_presets(kind: str) -> dict:
    """List presets for one of the orphaned Py6-era preset registries:
    ``fidelity`` (Analysis fidelity page), ``solver`` (Optimizer & weights
    page) or ``performance`` (Performance page). Each only fills in a single
    config group/sub-group rather than a whole ``ALASConfig``, unlike
    the aircraft preset above -- see ``/config/aux-preset/{kind}/{name}``.
    """
    reg = _AUX_PRESET_REGISTRIES.get(kind)
    if reg is None:
        raise HTTPException(status_code=404, detail=f"Unknown preset kind '{kind}'")
    return {
        "names": reg["available"](),
        "display_names": reg["display_names"](),
        "group": reg["group"],
        "subpath": reg.get("subpath"),
    }


@router.get("/config/aux-preset/{kind}/{name}")
def get_aux_preset(kind: str, name: str) -> dict:
    reg = _AUX_PRESET_REGISTRIES.get(kind)
    if reg is None:
        raise HTTPException(status_code=404, detail=f"Unknown preset kind '{kind}'")
    try:
        preset = reg["get"](name)
    except KeyError:
        raise HTTPException(status_code=404, detail=f"Unknown {kind} preset '{name}'")
    values = dataclasses.asdict(getattr(preset, reg["attr"]))
    return {"group": reg["group"], "subpath": reg.get("subpath"), "values": values}


@router.get("/schema")
def get_schema() -> dict:
    """Full recursive schema for every top-level config group.

    One entry per :class:`ALASConfig` dataclass field (``requirements``,
    ``geometry``, ``optimizer``, ...) -- mirrors the set of Advanced Settings
    pages the frontend builds one ``DynamicForm`` per (see
    ``docs/architecture.md`` §2a).
    """
    return dataclass_schema(ALASConfig())


@router.get("/design-space/specs")
def get_design_space_specs() -> dict:
    """The searchable design variables, in optimizer-vector order.

    Source for the Design Space table: one entry
    per :data:`DESIGN_VARIABLE_SPECS` item (name, label, unit, description,
    default/lower/upper), enough for a React ``DesignSpaceTable`` to render
    the same Variable/Unit/Initial/Lower/Upper/Description columns.
    """
    specs = []
    for spec in DESIGN_VARIABLE_SPECS:
        label, _unit = _humanize(spec.name)
        specs.append(
            {
                "name": spec.name,
                "label": label,
                "unit": spec.unit,
                "description": spec.description,
                "default": spec.default,
                "lower": spec.lower,
                "upper": spec.upper,
                "decimals": spec.decimals,
            }
        )
    return {"specs": specs}
