# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""The three named-preset registries the settings screens select from.

Solver, performance and fidelity presets each bundle one configuration
dataclass under a short name, so the choice between "quick draft" and
"exhaustive", or between one high-lift technology level and another, is a
dropdown rather than eight fields typed in by hand.

What matters about a preset is exactly what it overrides and what it leaves
alone: the performance registry deliberately does not touch the matching
chart's plotting resolution, and the fidelity registry deliberately touches
only three of `AnalysisConfig`'s fields. A preset that quietly wrote a fourth
would reset an assumption the user had tuned, and would do it silently. So
this fixture records each preset's whole resulting configuration, not just the
fields its constructor named -- a preset that overreaches shows up as a
disagreement in a field it should never have written.

Registration order is preserved: it is the order the dropdown lists them in,
and the first entry is what a user who does not choose ends up running.
"""

from __future__ import annotations

import dataclasses

import _framework


def _registry(presets, payload_field: str) -> list:
    """Each preset as name, display name, description and full settings."""
    return [
        {
            "name": preset.name,
            "display_name": preset.display_name,
            "description": preset.description,
            "settings": dataclasses.asdict(getattr(preset, payload_field)),
        }
        for preset in presets
    ]


def main() -> None:
    _framework.add_alas_to_path()

    from alas.config.fidelity_presets import _FIDELITY_PRESET_REGISTRY
    from alas.config.performance_presets import _PERFORMANCE_PRESET_REGISTRY
    from alas.config.solver_presets import _SOLVER_PRESET_REGISTRY

    _framework.write(
        "config",
        "presets",
        {
            # The payload field is named `settings` on two of the three
            # registries and `analysis` on the fidelity one. The fixture
            # spells it `settings` throughout, because what is being compared
            # is the configuration a preset produces and not what its own
            # dataclass called the field holding it.
            "solver": _registry(_SOLVER_PRESET_REGISTRY.values(), "settings"),
            "performance": _registry(
                _PERFORMANCE_PRESET_REGISTRY.values(), "settings"
            ),
            "fidelity": _registry(_FIDELITY_PRESET_REGISTRY.values(), "analysis"),
        },
        description=(
            "alas.config.{solver,performance,fidelity}_presets: every "
            "registered preset in registration order, with the complete "
            "configuration each one produces"
        ),
    )


if __name__ == "__main__":
    main()
