# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
ALAS -- a modular aircraft preliminary-design application.

Workflow:  user requirements  ->  design optimization  ->  full AeroSandbox
analysis  ->  reporting/export.

Public entry points:
    >>> from alas import ALASConfig, DesignPipeline
    >>> config = ALASConfig()                 # or ALASConfig.from_yaml(...)
    >>> result = DesignPipeline(config).run()

See ``docs/architecture.md`` for the module map and ``docs/methods.md``
for the physics.
"""

from typing import TYPE_CHECKING

from .config.settings import ALASConfig

if TYPE_CHECKING:  # real imports for type checkers only
    from .pipeline import DesignPipeline, PipelineResult

__version__ = "0.1.0"

__all__ = [
    "ALASConfig",
    "DesignPipeline",
    "PipelineResult",
    "__version__",
]


def __getattr__(name: str):
    # PEP 562 lazy exports: `from .pipeline import ...` here used to make
    # importing *anything* under `alas` pay pipeline's transitive
    # aerosandbox/casadi/scipy import (~11s cold, the bulk of the packaged
    # sidecar's startup) even for consumers that only need config/schema.
    # `from alas import DesignPipeline` still works exactly as before;
    # it just triggers the heavy import at that moment instead of at package
    # import.
    if name in ("DesignPipeline", "PipelineResult"):
        from . import pipeline

        return getattr(pipeline, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
