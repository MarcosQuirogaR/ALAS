# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Spanish UI-string catalog, read out of ``alas.translations.es``.

``CATALOG`` is built as three plain ``dict.update()`` calls over ``_LABELS``
(config field labels), ``_HELP`` (config field help text) and ``_FIGURES``
(figure titles, axis labels and legend entries), in that order. A later
dict's value silently wins if any key repeats -- that is what ``update``
does -- so this fixture also records whether the three ever disagree about a
key they share, which the ``t()`` lookup would otherwise hide: a wrong catalog
entry it never applies to is invisible in the running program, but if this
port only reproduces ``CATALOG`` and not the merge order, the two could
diverge silently the next time a key is added upstream.

Importing ``alas.translations.es`` runs ``alas/__init__.py`` first, since
Python always initialises a package before a submodule inside it. That
``__init__`` module eagerly imports ``alas.config.settings`` (for
``ALASConfig``) and everything it composes, but stops there: the heavy
AeroSandbox/CasADi/SciPy pipeline import is deferred behind ``__getattr__``
(docs/PORTING.md calls this "a lazy-export shim"). Measured at under 0.2s
under the reference venv, which is cheap enough that no
``importlib.util.spec_from_file_location`` bypass is needed.
"""

from __future__ import annotations

import _framework


def main() -> None:
    _framework.add_alas_to_path()
    import alas.translations.es as es

    labels = es._LABELS  # noqa: SLF001 - reading the pre-merge source dicts is the point
    help_text = es._HELP  # noqa: SLF001
    figures = es._FIGURES  # noqa: SLF001
    catalog = es.CATALOG

    if len(catalog) != len(labels | help_text | figures):
        raise SystemExit(
            "CATALOG's size does not match the union of _LABELS/_HELP/_FIGURES; "
            "the merge order in this generator's docstring may be stale"
        )

    # Walk the three source dicts in CATALOG.update()'s own order, recording
    # every key that repeats and whether the later dict's value actually
    # differs from the earlier one -- an update() that silently overrides a
    # key with a *different* value would be exactly the kind of upstream
    # surprise worth failing loudly on, rather than absorbing into the fixture.
    ordered = [("_LABELS", labels), ("_HELP", help_text), ("_FIGURES", figures)]
    first_seen: dict[str, tuple[str, str]] = {}
    silent_overrides = []
    for source_name, source_dict in ordered:
        for key, value in source_dict.items():
            if key in first_seen:
                prior_source, prior_value = first_seen[key]
                if prior_value != value:
                    silent_overrides.append((key, prior_source, prior_value, source_name, value))
            else:
                first_seen[key] = (source_name, value)

    if silent_overrides:
        raise SystemExit(
            "_LABELS/_HELP/_FIGURES disagree about a shared key's translation: "
            f"{silent_overrides!r}"
        )

    # One key is intentionally shared with an identical value: "Fuselage"
    # appears both as a config label (_LABELS) and as a figure legend entry
    # (_FIGURES), both translating to "Fuselaje". CATALOG.update() applies
    # _FIGURES last, so that repetition is a harmless no-op rather than a
    # collision -- the loop above would have raised if the values had ever
    # disagreed.
    repeated_keys = [key for key in figures if key in labels or key in help_text]
    repeated_keys += [key for key in help_text if key in labels]
    if repeated_keys != ["Fuselage"]:
        raise SystemExit(
            f"the set of keys shared across _LABELS/_HELP/_FIGURES changed: {repeated_keys!r} "
            "-- update this generator's comment to describe the new overlap"
        )

    _framework.write(
        "i18n",
        "es_catalog",
        {"catalog": catalog},
        description=(
            "alas.translations.es.CATALOG: every English source string mapped "
            "to its Spanish translation, dumped verbatim"
        ),
    )


if __name__ == "__main__":
    main()
