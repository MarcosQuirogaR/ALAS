# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Configuration layer for ALAS.

Everything the user can tune -- mission requirements, the geometry scaffold,
optimizer behaviour, analysis fidelity, and the searchable design space -- is
defined here as typed, documented dataclasses. The computational core reads
only from these objects, so there are no hardcoded design values buried in the
solver code.
"""

from .analysis_config import AnalysisConfig
from .cabin_config import (
    CabinConfig,
    CargoDeckConfig,
    PassengerCabinConfig,
    SeatClassConfig,
)
from .control_surfaces_config import ControlSurfacesConfig
from .design_variables import (
    DESIGN_VARIABLE_SPECS,
    DesignVariableSpec,
    DesignVector,
)
from .geometry_config import (
    EmpennageConfig,
    EngineConfig,
    FuselageConfig,
    GeometryConfig,
    WingConfig,
)
from .landing_gear_config import LandingGearConfig
from .mass_config import MassModelConfig
from .mission_config import MissionConfig, MissionProfileConfig
from .mses_config import MSESConfig
from .optimizer_config import ObjectiveWeights, OptimizerConfig, SolverSettings
from .performance_config import PerformanceConfig
from .physics_config import DragModelConfig
from .requirements import DesignRequirements
from .settings import ALASConfig

__all__ = [
    "ALASConfig",
    "DesignRequirements",
    "GeometryConfig",
    "WingConfig",
    "EmpennageConfig",
    "FuselageConfig",
    "EngineConfig",
    "OptimizerConfig",
    "ObjectiveWeights",
    "SolverSettings",
    "AnalysisConfig",
    "DragModelConfig",
    "MassModelConfig",
    "LandingGearConfig",
    "PerformanceConfig",
    "CabinConfig",
    "PassengerCabinConfig",
    "CargoDeckConfig",
    "SeatClassConfig",
    "MissionConfig",
    "MissionProfileConfig",
    "MSESConfig",
    "ControlSurfacesConfig",
    "DesignVector",
    "DesignVariableSpec",
    "DESIGN_VARIABLE_SPECS",
]
