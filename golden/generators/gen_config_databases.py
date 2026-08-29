# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""The three reference-data tables the configuration selects from by name.

Materials, engines and airports are databases that happen to be written as
Python: a few hundred lines each of constructor calls registering immutable
records. Nothing in them is computed, and nothing about them benefits from
being executable -- the reference implementation writes them that way because
a dataclass constructor is the shortest thing to hand in Python, not because
the data is code.

Ported, they become JSON: `crates/alas-config/data/{materials,engines,
airports}.json`, embedded in the crate and parsed once. That keeps a table of
published material properties reviewable as a table, keeps it out of the
700-line source limit it would otherwise dominate, and makes a correction to a
published figure a one-line data change rather than a code change.

This generator writes the parity copy under `golden/`. The crate's embedded
copy is deliberately separate -- a shipped binary must not need `golden/` on
disk -- and `tests/parity_databases.rs` is what stops the two drifting apart.
"""

from __future__ import annotations

import dataclasses

import _framework


def _records(items) -> list:
    """Dataclass instances as plain dicts, in the order the database lists."""
    return [dataclasses.asdict(item) for item in items]


def main() -> None:
    _framework.add_alas_to_path()

    from alas.config.airports import AIRPORTS
    from alas.config.engines import ENGINE_DATABASE
    from alas.config.materials import MATERIAL_DATABASE

    # Registration order is what `available_*` sorts, and what the reference
    # iterates when nothing has asked for a particular entry. Preserved so a
    # reader comparing the two sees the same table in the same order.
    _framework.write(
        "config",
        "materials",
        {
            "materials": _records(MATERIAL_DATABASE.values()),
            "available": sorted(MATERIAL_DATABASE),
        },
        description=(
            "alas.config.materials.MATERIAL_DATABASE: every registered "
            "structural material, in registration order, with the sorted name "
            "list `available_materials` returns"
        ),
    )

    _framework.write(
        "config",
        "engines",
        {
            "engines": _records(ENGINE_DATABASE.values()),
            "available": sorted(ENGINE_DATABASE),
            # The nacelle silhouette is derived from the engine's length
            # rather than stored, so it is captured per engine: it is what
            # the geometry builder actually draws, and a wrong station
            # fraction would produce a plausible nacelle of the wrong shape.
            "nacelle_profiles": {
                name: [list(point) for point in spec.nacelle_profile()]
                for name, spec in ENGINE_DATABASE.items()
            },
        },
        description=(
            "alas.config.engines.ENGINE_DATABASE: every registered turbofan, "
            "in registration order, with the sorted name list "
            "`available_engines` returns and each one's derived nacelle profile"
        ),
    )

    _framework.write(
        "config",
        "airports",
        {"airports": _records(AIRPORTS)},
        description=(
            "alas.config.airports.AIRPORTS: every curated aerodrome, in the "
            "order the database lists them"
        ),
    )

    from alas.config.design_variables import DESIGN_VARIABLE_SPECS, DesignVector

    # Spec order is the order of the flat vector the optimizer perturbs, so it
    # is captured rather than sorted: a table that agreed entry by entry but
    # not in order would move one variable while the geometry builder read
    # another, which is exactly the failure the two views can produce.
    _framework.write(
        "config",
        "design_variables",
        {
            "specs": _records(DESIGN_VARIABLE_SPECS),
            "default_vector": DesignVector.default().to_array().tolist(),
            "bounds": [list(pair) for pair in DesignVector.bounds()],
        },
        description=(
            "alas.config.design_variables: the ordered design space, the "
            "nominal design vector, and the bounds list handed to the optimizer"
        ),
    )


if __name__ == "__main__":
    main()
