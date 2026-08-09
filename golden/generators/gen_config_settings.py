# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""What `ALASConfig` does with a partial configuration, rather than what it holds.

`gen_config.py` already records the aggregate as freshly constructed. What it
cannot see is the loading path, which is where a saved file becomes a run:
selecting a preset copies that aircraft's geometry and requirements in and,
for the two presets that carry one, its mass-model and field-performance
calibration as well -- and then the file's own keys are laid over the result.
A port that applied those in the other order, or skipped the calibrations
because the graphical front end also applies them, would produce a different
aircraft from the same file with nothing to say so.

The cases below therefore record the whole resulting configuration, not the
fields each input names. Four of them exercise the parts most likely to be
ported wrong: laying one field of one nested group over a preset, a list-valued
field arriving from YAML as a list where the dataclass declares a tuple, a
preset name the registry does not carry, and a key that does not exist.

The failing cases are recorded as failures on purpose. A configuration file
with a misspelled key that loads successfully is a run whose settings are not
the ones its author wrote down, and nothing later can detect that; the port has
to reject exactly what the reference rejects.
"""

from __future__ import annotations

import _framework


def _serializable(obj):
    """Tuples become lists, as ``ALASConfig.to_dict`` does before saving."""
    if isinstance(obj, dict):
        return {k: _serializable(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_serializable(v) for v in obj]
    return obj


# (case name, what a saved file holds, why this case is here).
CASES = [
    ("empty", {}, "an empty file is the defaults, not an error"),
    (
        "preset_only",
        {"preset": "A220-300"},
        "the one preset carrying both a mass-model and a performance "
        "calibration; naming it must apply both, as the graphical front end "
        "does on selection",
    ),
    (
        "preset_then_field",
        {"preset": "B787-9", "requirements": {"cruise_mach": 0.80}},
        "one field of one nested group laid over a preset: everything else in "
        "that group stays at the preset's value rather than reverting",
    ),
    (
        "tuple_field_from_a_list",
        {"geometry": {"empennage": {"hstab_tip_le_m": [6.1, 9.7, 0.9]}}},
        "YAML has no tuples, so a tuple-valued field arrives as a list and is "
        "re-tupled against the declared value",
    ),
    (
        "airports",
        {"departure_airport": "Madrid (LEMD)", "arrival_airport": "Quito (SEQM)"},
        "the route endpoints are saved with the configuration so a reloaded "
        "file reproduces the same route",
    ),
    (
        "unknown_preset",
        {"preset": "Concorde"},
        "an unregistered preset is swallowed rather than raised -- and the "
        "overlay then writes the unknown name into the field anyway, which is "
        "reproduced rather than tidied up",
    ),
    (
        "deep_partial",
        {
            "optimizer": {"weights": {"ld_weight": 2.0}},
            "structures": {"spanwise_stations": 120},
        },
        "two unrelated groups patched at different depths in one file",
    ),
    (
        "unknown_key",
        {"requirements": {"cruise_match": 0.8}},
        "a misspelled key is an error, not a value silently dropped",
    ),
    (
        "unknown_top_level_key",
        {"aerodynamics": {}},
        "the same at the top level, where the misspelling is a whole group",
    ),
]


def main() -> None:
    _framework.add_alas_to_path()

    from alas.config.presets import available_presets
    from alas.config.settings import ALASConfig

    cases = []
    for name, data, why in CASES:
        entry = {"name": name, "input": data, "why": why}
        try:
            entry["result"] = _serializable(ALASConfig.from_dict(data).to_dict())
        except KeyError as error:
            # `KeyError.args[0]` is the message `_overlay_dataclass` built,
            # which names the offending key and lists the valid ones.
            entry["error"] = str(error.args[0])
        cases.append(entry)

    _framework.write(
        "config",
        "settings",
        {
            "cases": cases,
            # Every preset through the same path, since this is how a headless
            # run selects an aircraft and there is no other check that all
            # seven survive it.
            "from_preset": {
                name: _serializable(ALASConfig.from_dict({"preset": name}).to_dict())
                for name in available_presets()
            },
        },
        description=(
            "alas.config.settings.ALASConfig.from_dict: the configuration each "
            "saved-file case produces, including the ones that are rejected, "
            "and every aircraft preset applied through the same path"
        ),
    )


if __name__ == "__main__":
    main()
