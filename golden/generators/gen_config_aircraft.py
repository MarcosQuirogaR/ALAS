# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""The aircraft preset registry: seven real types, each as a whole design.

Unlike the three named-preset registries, an entry here is not a handful of
overrides on one dataclass -- it is a complete aircraft: a design vector, a
geometry scaffold, a set of requirements, an engine, and optionally its own
mass-model and field-performance calibration. Several hundred dimensions read
off published specification sheets, and a transposed digit in any of them
produces an aircraft that is plausible and wrong, which is the failure this
project is built to catch.

So the fixture records every field of every preset rather than the ones a
reader would think to check. It also records the two derived things upstream
exposes: the display-name mapping the dropdown is built from, and the
registration order, which decides what a user who does not choose ends up
with.

One behaviour worth capturing explicitly is `AircraftPreset.__post_init__`,
which copies the preset's engine name down into its geometry. Two of the seven
state that name twice -- once on the preset and once inside `EngineConfig` --
and the rest state it only on the preset, so the synchronised value is the one
every consumer reads and the one this fixture holds.
"""

from __future__ import annotations

import dataclasses

import _framework


def _serializable(obj):
    """Tuples become lists, as ``ALASConfig.to_dict`` does before saving."""
    if isinstance(obj, dict):
        return {k: _serializable(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_serializable(v) for v in obj]
    return obj


def main() -> None:
    _framework.add_alas_to_path()

    from alas.config.presets import (
        _PRESET_REGISTRY,
        available_presets,
        get_preset,
        preset_display_names,
    )

    presets = []
    for name in available_presets():
        preset = get_preset(name)
        entry = _serializable(dataclasses.asdict(preset))
        # Read back through the accessor rather than off the geometry, so a
        # port that reproduces the fields and not the accessor is caught.
        entry["engine_spanwise_positions"] = _serializable(
            preset.engine_spanwise_positions()
        )
        presets.append(entry)

    _framework.write(
        "config",
        "aircraft_presets",
        {
            "presets": presets,
            "available": available_presets(),
            "display_names": preset_display_names(),
            "registered": list(_PRESET_REGISTRY),
        },
        description=(
            "alas.config.presets: every registered aircraft preset in "
            "registration order, as a complete design -- design vector, "
            "geometry scaffold, requirements, engine, and any per-aircraft "
            "mass-model and performance calibration"
        ),
    )


if __name__ == "__main__":
    main()
