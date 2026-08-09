# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""The cross-field checks a configuration has to pass before it is run.

A field's own bounds keep it sensible on its own. What they cannot see is a
combination: a dive speed slower than the cruise point it is supposed to
bracket, a tapered surface whose tip is wider than its root. Each rule reads
several groups at once and reports a dotted path down to the field the user
should look at, and both halves of that -- the path and the wording -- are what
this fixture pins down. A rule that fired correctly and named the wrong field
sends the user to edit something that was never the problem.

Severity is recorded too. It is not decoration: an error blocks the run and a
warning only highlights, so a rule that downgraded itself would let a design
that cruises outside its own structural envelope through to the solver.

The cases are described as saved-file overlays rather than as constructed
objects, so the same input builds the same configuration on both sides through
the loading path `gen_config_settings.py` already covers.

One difference in how the two implementations reach the cruise dynamic
pressure, worth stating because it is invisible in the output: the reference's
rule builds `aerosandbox.Atmosphere` at its default, which is a fitted
approximation rather than the closed-form ISA this port evaluates. The two
agree to about 1e-11, and every number the rule produces is rendered to the
nearest whole metre per second, so the messages are identical -- but the cases
below are deliberately chosen well away from the VC and VD thresholds, since a
case sitting on one of them would be decided by that last digit rather than by
the rule.
"""

from __future__ import annotations

import dataclasses

import _framework

# (case name, what a saved file holds, why this case is here).
CASES = [
    ("defaults", {}, "the shipped configuration is one nothing objects to"),
    (
        "cruise_past_dive_speed",
        {"requirements": {"dive_speed_m_s": 100.0}},
        "the cruise design point sits outside the V-n envelope entirely: an "
        "error, which blocks the run",
    ),
    (
        "cruise_in_the_caution_band",
        {"requirements": {"dive_speed_m_s": 130.0}},
        "the cruise point is past VC but short of VD -- the diagram's yellow "
        "band, which is a warning and not a block",
    ),
    (
        "untapered_empennage",
        {
            "geometry": {
                "empennage": {
                    "hstab_tip_chord_m": 9.0,
                    "vstab_tip_chord_m": 10.0,
                }
            }
        },
        "both stabilisers wider at the tip than at the root: two errors from "
        "one rule, in the order the rule appends them",
    ),
    (
        "tip_chord_equal_to_root",
        {"geometry": {"empennage": {"hstab_tip_chord_m": 8.0}}},
        "an untapered surface is rejected as well as an inverted one, since "
        "the comparison is >= and not >",
    ),
    (
        "both_rules_at_once",
        {
            "requirements": {"dive_speed_m_s": 100.0},
            "geometry": {"empennage": {"vstab_tip_chord_m": 10.0}},
        },
        "two rules firing on one configuration, collected in registration "
        "order rather than the first one stopping the rest",
    ),
    (
        "preset_a380",
        {"preset": "A380-800"},
        "a real aircraft through the loading path: a preset that changes both "
        "the cruise point and the empennage must still pass",
    ),
    (
        "preset_a320",
        {"preset": "A320-200"},
        "the same for a narrow-body, whose cruise altitude and dive speed are "
        "both lower",
    ),
]


def main() -> None:
    _framework.add_alas_to_path()

    from alas.config.settings import ALASConfig
    from alas.validation import validate

    cases = []
    for name, data, why in CASES:
        config = ALASConfig.from_dict(data)
        cases.append(
            {
                "name": name,
                "input": data,
                "why": why,
                "issues": [dataclasses.asdict(issue) for issue in validate(config)],
            }
        )

    _framework.write(
        "config",
        "validation",
        {"cases": cases},
        description=(
            "alas.validation.validate: the issues each configuration produces, "
            "with the field each one points at and the severity that decides "
            "whether it blocks the run"
        ),
    )


if __name__ == "__main__":
    main()
