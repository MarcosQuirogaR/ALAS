# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Dataclass -> JSON-schema introspection.

Walks a config dataclass and emits a plain JSON description a React
``DynamicForm`` component can render generically, using field-metadata
conventions (``label``/``unit``/``help``/``min``/``max``/``decimals``/
``columns``/``advanced``/``hide_in_form``/``readonly_unless``) declared on
each dataclass field.

Deliberately independent of any GUI toolkit -- this module (and the rest of
``alas.sidecar``) must import cleanly headless.
"""

from __future__ import annotations

from dataclasses import fields, is_dataclass
from typing import Any, Dict, List, Tuple

from ..i18n import t

# Kept in sync with the frontend's own field-type table -- see DynamicForm if this
# one ever needs to change (e.g. a new unit suffix).
_UNIT_SUFFIXES: List[Tuple[str, str]] = [
    ("_kg_m2", "kg/m²"),
    ("_m_s2", "m/s²"),
    ("_m_s", "m/s"),
    ("_m2", "m²"),
    ("_deg", "deg"),
    ("_kg", "kg"),
    ("_pa", "Pa"),
    ("_m", "m"),
    ("_s", "s"),
]

_WEIGHT_SUFFIXES = ("_scale", "_per_m", "_weight", "_floor", "_cost", "_floor_m")


def _humanize(name: str) -> Tuple[str, str]:
    unit = ""
    label_src = name
    for suffix, u in _UNIT_SUFFIXES:
        if name.endswith(suffix):
            unit, label_src = u, name[: -len(suffix)]
            break
    label = label_src.replace("_", " ").strip().capitalize()
    return label, unit


def _is_number(v: Any) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _is_number_tuple(v: Any) -> bool:
    return isinstance(v, (list, tuple)) and len(v) > 0 and all(_is_number(x) for x in v)


def _string_field_extras(name: str, instance: Any) -> Dict[str, Any]:
    """Mirrors the frontend's string-field branch: which
    string fields get a dropdown, with what options, and whether free text
    is still allowed alongside the suggestions (Qt's ``setEditable(True)``).
    """
    if "airfoil" in name.lower():
        from ..geometry.airfoils import AirfoilLibrary

        return {"editable": True, "options": AirfoilLibrary.get_available_airfoils()}
    if name == "engine_name":
        # Same registry the Inputs screen's Engine dropdown is served from
        # (GET /engines) -- without this, the Advanced Settings geometry form
        # rendered engine_name as bare free text with no discoverability and
        # no protection against a typo that get_engine() then KeyErrors on.
        from ..config.engines import available_engines

        return {"editable": False, "options": available_engines()}
    # editable=False renders a strict <select> (the option list IS the valid
    # set -- free text could only produce a KeyError later); editable=True
    # renders a searchable combo box that still allows free text (airfoils:
    # AeroSandbox can also resolve names not in the library, e.g. any NACA
    # 4-digit code).
    if name.endswith("_material") and name != "strut_material":
        from ..config.materials import available_materials

        return {"editable": False, "options": available_materials()}
    if name == "strut_material":
        # Validated against physics.landing_gear.STRUT_MATERIALS (plus the
        # "auto" sentinel), not config.materials.MATERIAL_DATABASE -- its own
        # option list, not the generic `_material` one above.
        from ..physics.landing_gear import STRUT_MATERIALS

        options = ["auto"] + sorted(set(STRUT_MATERIALS.values()))
        return {"editable": False, "options": options}
    if name == "tire_class":
        from ..physics.landing_gear import TIRE_DATABASE, _TIRE_ORDER

        return {
            "editable": False,
            "options": ["auto"] + [k for k in _TIRE_ORDER if k in TIRE_DATABASE],
        }
    if name == "te_rib_mode":
        return {
            "editable": False,
            "options": ["all", "none", "alternate", "inboard", "outboard"],
        }
    if name == "strategy":
        # scipy.optimize.differential_evolution's built-in strategy names.
        return {
            "editable": False,
            "options": [
                "best1bin",
                "best1exp",
                "rand1bin",
                "rand1exp",
                "best2bin",
                "best2exp",
                "rand2bin",
                "rand2exp",
                "randtobest1bin",
                "randtobest1exp",
                "currenttobest1bin",
                "currenttobest1exp",
            ],
        }
    if name == "aircraft_type":
        return {"editable": False, "options": ["passenger", "cargo"]}
    if name == "cabin_preset":
        options_by_aircraft_type = {
            "passenger": ["Ryanair", "Iberia", "Emirates", "Custom"],
            "cargo": ["Max payload", "Dense payload", "Custom"],
        }
        aircraft_type = getattr(instance, "aircraft_type", "passenger")
        return {
            "editable": False,
            "options": options_by_aircraft_type.get(
                aircraft_type, options_by_aircraft_type["passenger"]
            ),
            "options_by_aircraft_type": options_by_aircraft_type,
        }
    return {}


def _field_kind(value: Any, name: str) -> str:
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return (
            "weight_slider"
            if any(name.endswith(s) for s in _WEIGHT_SUFFIXES) and value > 0
            else "float"
        )
    if isinstance(value, str):
        return "str"
    if value is None:
        return "optional"
    if isinstance(value, (list, tuple)) and value and all(_is_number(v) for v in value):
        return "number_list"
    if (
        isinstance(value, (list, tuple))
        and value
        and all(_is_number_tuple(v) for v in value)
    ):
        return "tuple_list"
    return "unsupported"


def dataclass_schema(instance: Any) -> Dict[str, Any]:
    """Return a JSON-safe schema dict for one (possibly nested) dataclass instance.

    Shape (one entry per field, in declaration order)::

        {
          "type": "<DataclassName>",
          "fields": [
            {"name", "kind", "label", "unit", "help", "advanced",
             "value", "min", "max", "decimals", "columns",
             ["fields": [...]] }   # present only when kind == "dataclass"
          ]
        }
    """
    if not is_dataclass(instance):
        raise TypeError("dataclass_schema requires a dataclass instance")

    out_fields: List[Dict[str, Any]] = []
    for f in fields(instance):
        meta = dict(f.metadata or {})
        if meta.get("hide_in_form"):
            continue
        value = getattr(instance, f.name)
        auto_label, auto_unit = _humanize(f.name)
        # Single translation boundary for every auto-generated form field: the
        # dataclasses keep their canonical English prose, and the active
        # language (set per request from ?lang=) is applied here on the way out.
        # An untranslated string falls through as English -- see alas/i18n.
        entry: Dict[str, Any] = {
            "name": f.name,
            "label": t(meta.get("label", auto_label)),
            "unit": meta.get("unit", auto_unit),
            "help": t(meta.get("help", "")),
            "advanced": bool(meta.get("advanced", False)),
        }
        if is_dataclass(value):
            entry["kind"] = "dataclass"
            entry["fields"] = dataclass_schema(value)["fields"]
        else:
            entry["kind"] = _field_kind(value, f.name)
            entry["value"] = value if entry["kind"] != "optional" else None
            if "min" in meta:
                entry["min"] = meta["min"]
            if "max" in meta:
                entry["max"] = meta["max"]
            if "decimals" in meta:
                entry["decimals"] = meta["decimals"]
            if "columns" in meta:
                entry["columns"] = meta["columns"]
            if "readonly_unless" in meta:
                entry["readonly_unless"] = meta["readonly_unless"]
            if entry["kind"] == "str":
                entry.update(_string_field_extras(f.name, instance))
        out_fields.append(entry)

    return {"type": type(instance).__name__, "fields": out_fields}
