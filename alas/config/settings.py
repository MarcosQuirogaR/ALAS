# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Top-level ALAS configuration aggregator.

:class:`ALASConfig` bundles the four configuration groups (requirements,
geometry scaffold, optimizer, analysis) into a single object that the pipeline
consumes. It can be built from defaults, overlaid with a partial dictionary, or
loaded from / saved to a YAML file -- which is how a user (or, later, a GUI
front-end) supplies their inputs without touching code.
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field, fields, is_dataclass
from pathlib import Path
from typing import Any, Dict

from .analysis_config import AnalysisConfig
from .cabin_config import CabinConfig
from .control_surfaces_config import ControlSurfacesConfig
from .geometry_config import GeometryConfig
from .landing_gear_config import LandingGearConfig
from .mass_config import MassModelConfig
from .mission_config import MissionConfig
from .mses_config import MSESConfig
from .optimizer_config import OptimizerConfig
from .performance_config import PerformanceConfig
from .physics_config import DragModelConfig
from .propulsion_config import PropulsionCycleConfig
from .requirements import DesignRequirements
from .structures_config import StructuresConfig


@dataclass
class ALASConfig:
    """The single object that fully specifies an ALAS run."""

    preset: str = ""
    requirements: DesignRequirements = field(default_factory=DesignRequirements)
    geometry: GeometryConfig = field(default_factory=GeometryConfig)
    optimizer: OptimizerConfig = field(default_factory=OptimizerConfig)
    analysis: AnalysisConfig = field(default_factory=AnalysisConfig)
    drag_model: DragModelConfig = field(default_factory=DragModelConfig)
    performance: PerformanceConfig = field(default_factory=PerformanceConfig)
    mass_model: MassModelConfig = field(default_factory=MassModelConfig)
    landing_gear: LandingGearConfig = field(default_factory=LandingGearConfig)
    cabin: CabinConfig = field(default_factory=CabinConfig)
    mission: MissionConfig = field(default_factory=MissionConfig)
    mses: MSESConfig = field(default_factory=MSESConfig)
    control_surfaces: ControlSurfacesConfig = field(
        default_factory=ControlSurfacesConfig
    )
    propulsion_cycle: PropulsionCycleConfig = field(
        default_factory=PropulsionCycleConfig
    )
    structures: StructuresConfig = field(default_factory=StructuresConfig)

    # Departure and arrival airports persisted with the config so saved YAML
    # files reproduce the same route without the user having to re-select.
    departure_airport: str = "London Heathrow (EGLL)"
    arrival_airport: str = "Dubai (OMDB)"

    # -- serialisation -------------------------------------------------------
    def to_dict(self) -> Dict[str, Any]:
        """Recursively convert to a plain dict (YAML/JSON friendly).

        Tuples are converted to lists so PyYAML's safe dumper (which cannot
        represent tuples) can serialise the result.
        """
        return _to_serializable(dataclasses.asdict(self))

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ALASConfig":
        """Build a config, overlaying ``data`` onto the defaults.

        If 'preset' is specified in data, we first load that preset's
        geometry and requirements defaults, then overlay the rest of the data.
        """
        instance = cls()
        preset_name = data.get("preset", "")
        if preset_name:
            from .presets import get_preset

            try:
                preset = get_preset(preset_name)
                instance.preset = preset_name
                import copy

                instance.geometry = copy.deepcopy(preset.geometry)
                instance.requirements = copy.deepcopy(preset.requirements)
                # Per-preset calibration overrides. The GUI applies these on
                # preset selection (main_window._on_preset_changed); this
                # YAML/CLI path must apply the same ones, or a headless run
                # of e.g. the A220-300 silently reverts to the global
                # widebody-calibrated mass fractions (~2.8 t OEW error) and
                # the generic narrowbody CLmax (~15-20 kt V-speed error) its
                # preset was specifically calibrated away from. None = the
                # preset uses the global defaults, which instance already has.
                if preset.mass_model is not None:
                    instance.mass_model = copy.deepcopy(preset.mass_model)
                if preset.performance is not None:
                    instance.performance = copy.deepcopy(preset.performance)
            except KeyError:
                pass

        return _overlay_dataclass(instance, data)

    # -- YAML I/O ------------------------------------------------------------
    @classmethod
    def from_yaml(cls, path: str | Path) -> "ALASConfig":
        import yaml  # imported lazily so the core has no hard YAML dependency

        path = Path(path)
        with path.open("r", encoding="utf-8") as f:
            data = yaml.safe_load(f) or {}
        return cls.from_dict(data)

    def to_yaml(self, path: str | Path) -> None:
        import yaml

        path = Path(path)
        with path.open("w", encoding="utf-8") as f:
            yaml.safe_dump(self.to_dict(), f, sort_keys=False, default_flow_style=False)

    # -- JSON I/O --------------------------------------------------------------
    # Thin codecs over the exact same to_dict()/from_dict() the YAML path
    # uses -- JSON and YAML configs are interchangeable since they share one
    # dict representation, not two independently-maintained serializers.
    @classmethod
    def from_json(cls, path: str | Path) -> "ALASConfig":
        import json

        path = Path(path)
        with path.open("r", encoding="utf-8") as f:
            data = json.load(f)
        return cls.from_dict(data)

    def to_json(self, path: str | Path) -> None:
        import json

        path = Path(path)
        with path.open("w", encoding="utf-8") as f:
            json.dump(self.to_dict(), f, indent=2)


def _to_serializable(obj: Any) -> Any:
    """Recursively turn tuples into lists for safe YAML/JSON serialisation."""
    if isinstance(obj, dict):
        return {k: _to_serializable(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_to_serializable(v) for v in obj]
    return obj


def _overlay_dataclass(instance: Any, data: Dict[str, Any]) -> Any:
    """Recursively overlay a (possibly partial) dict onto a dataclass instance."""
    if not is_dataclass(instance):
        return data

    field_map = {f.name: f for f in fields(instance)}
    for key, value in data.items():
        if key not in field_map:
            raise KeyError(
                f"Unknown config key '{key}' for {type(instance).__name__}. "
                f"Valid keys: {sorted(field_map)}"
            )
        current = getattr(instance, key)
        if is_dataclass(current) and isinstance(value, dict):
            setattr(instance, key, _overlay_dataclass(current, value))
        else:
            # Re-tuple values that the dataclass declares as tuples (YAML gives lists).
            if isinstance(current, tuple) and isinstance(value, list):
                value = tuple(value)
            setattr(instance, key, value)
    return instance
