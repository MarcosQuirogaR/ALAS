# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-payload``: the detailed interior, end to end.

The three modules of the payload cluster -- ``alas/physics/payload.py``,
``alas/physics/cabin_layout.py`` and ``alas/physics/cargo_loader.py`` -- are
circularly interdependent (``payload`` owns the shared geometry and the
dispatcher, the other two own the two layout engines and both import back into
``payload`` for its item types), so they are checked by one fixture rather than
three. Everything below runs on real built aircraft rather than synthetic
probes: each case names an ``ALASConfig`` overlay, and the plane comes from
``AircraftBuilder(config.geometry).build(preset.design_vector)`` the way every
caller of ``build_payload_layout`` gets one.

Four sections, and what each is here to pin down:

``geometry``
    ``CabinGeometry``'s samplers on four fuselages, probed *past both ends* of
    the station list so the clamping branches of ``np.interp`` are exercised
    rather than assumed. A380-800 is the one preset whose ``height_m`` clears
    ``diameter_m * 1.15``, so it is the only case that reaches the double-deck
    branch and its two passenger ``DeckSpec``s.

``layouts``
    ``build_payload_layout`` -- the dispatcher, so both engines are reached
    through the same entry point every consumer uses. Every item is recorded in
    the order the engine appended it, with its ``meta``: the layout is a
    *sequence*, and two implementations that place the same items in a
    different order have not agreed. Cases cover single- and multi-class
    cabins, the double deck, monument counts past the bay count (the
    ``_stack_y`` narrowing), the structural-payload belly fill, the
    bulk-overflow guard, all four cargo loading strategies, and a narrowbody
    freighter whose holds fail the LD3 fit check and degrade through
    ``LOWER_HOLD_FALLBACKS``.

``simulations``
    ``simulate_passenger_counts`` on its own, which is what drives
    ``requirements.num_passengers`` before any layout is built, across the
    three shipped preset mixes and the all-economy one.

``cabin_presets``
    ``apply_cabin_preset``: every branch of it, including both ``Custom``
    branches (the percent-mode re-solve and the already-configured no-op) and
    the two freighter presets.

``oew``
    ``oew_and_cg`` over a real ``run_mass_analysis`` breakdown, plus a case
    whose masses are all zero, since the zero-mass guard is what stops the
    optimizer's first evaluation dividing by nothing.
"""

from __future__ import annotations

import copy

import _framework

_framework.add_alas_to_path()

from alas.config.presets import get_preset  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.physics.mass import run_mass_analysis  # noqa: E402
from alas.physics.payload import (  # noqa: E402
    CabinGeometry,
    apply_cabin_preset,
    build_payload_layout,
    oew_and_cg,
    simulate_passenger_counts,
)

# A three-class cabin given as explicit seat counts, reused by several cases so
# that what differs between them is the fuselage and not the cabin.
_THREE_CLASS = {
    "business": {"count": 30, "abreast": 4, "pitch_m": 1.55, "width_m": 0.70},
    "premium": {"count": 21, "abreast": 7, "pitch_m": 0.97, "width_m": 0.52},
    "economy": {"count": 180, "pitch_m": 0.79, "width_m": 0.46},
}

# (case name, ALASConfig overlay, oew, x_oew, why this case is here).
GEOMETRY_CASES = [
    ("default", {}, "the shipped configuration's own fuselage"),
    ("a320", {"preset": "A320-200"}, "a narrowbody: the smallest usable width"),
    ("b787", {"preset": "B787-9"}, "a twin-aisle widebody"),
    (
        "a380",
        {"preset": "A380-800"},
        "the one preset whose height clears 1.15 diameters, so the only one "
        "that reaches the double-deck branch",
    ),
    (
        "thick_wall",
        {"preset": "A320-200", "cabin": {"passenger": {"wall_thickness_m": 1.4}}},
        "a wall thick enough to drive internal_half_height onto its 0.1 m "
        "floor and usable_width onto its zero floor",
    ),
]

LAYOUT_CASES = [
    (
        "default_single_class",
        {},
        0.0,
        0.0,
        "no class carries a seat count, so classes() is empty and the engine "
        "falls back to one economy cabin sized to num_passengers",
    ),
    (
        "a320_high_density",
        {
            "preset": "A320-200",
            "cabin": {
                "passenger": {
                    "economy": {"count": 180, "pitch_m": 0.7112, "width_m": 0.46}
                }
            },
        },
        0.0,
        0.0,
        "a floor-limited narrowbody at the regulatory minimum pitch: the "
        "pitch-stretch multiplier stays at 1.0 and the exit cap does not bind",
    ),
    (
        "b787_three_class",
        {"preset": "B787-9", "cabin": {"passenger": _THREE_CLASS}},
        0.0,
        0.0,
        "three classes on one deck, so the inter-class monument bays and the "
        "class-boundary handoff are both reached",
    ),
    (
        "b787_exit_limited",
        {
            "preset": "B787-9",
            "cabin": {"passenger": {"economy": {"count": 900, "pitch_m": 0.7112}}},
        },
        0.0,
        0.0,
        "far more seats requested than CS-25.807 will certify, so the layout "
        "truncates on max_certifiable_capacity rather than on floor length",
    ),
    (
        "a380_double_deck",
        {
            "preset": "A380-800",
            "cabin": {
                "passenger": {
                    "business": {"count": 76, "abreast": 4, "pitch_m": 1.55},
                    "economy": {"count": 450, "pitch_m": 0.81},
                }
            },
        },
        0.0,
        0.0,
        "two passenger decks: the class index carries across the deck "
        "boundary, and each deck gets its own exit set",
    ),
    (
        "crowded_monuments",
        {
            "preset": "B787-9",
            "cabin": {
                "passenger": {
                    "economy": {"count": 250},
                    "galley_count": 11,
                    "lavatory_count": 13,
                }
            },
        },
        0.0,
        0.0,
        "more monuments than bays, so _monument_fill_order cycles and _stack_y "
        "narrows the later items instead of overlapping them",
    ),
    (
        "belly_freight",
        {
            "preset": "B787-9",
            "cabin": {
                "passenger": {"economy": {"count": 290}, "belly_cargo_kg": 12_000.0}
            },
        },
        0.0,
        0.0,
        "an explicit belly load competing with the structural-payload "
        "auto-fill, and the hold-capacity clamp above both",
    ),
    (
        "belly_overflow",
        {
            "preset": "A320-200",
            "cabin": {
                "passenger": {"economy": {"count": 180}, "belly_cargo_kg": 80_000.0}
            },
        },
        0.0,
        0.0,
        "a belly load past what the holds can hold, so the freight is clamped "
        "to the capacity the checked bags leave rather than overflowing",
    ),
    (
        "bag_overflow",
        {
            "preset": "A320-200",
            "cabin": {
                "passenger": {"economy": {"count": 180}, "checked_bag_mass_kg": 120.0}
            },
        },
        0.0,
        0.0,
        "checked baggage alone past the holds' capacity, which is the only way "
        "to reach the bulk-overflow guard: the belly freight is clamped to what "
        "the bags leave, and the bags themselves are not",
    ),
    (
        "no_structural_cap",
        {
            "preset": "B787-9",
            "requirements": {"max_structural_payload_kg": 0.0},
            "cabin": {"passenger": {"economy": {"count": 290}}},
        },
        0.0,
        0.0,
        "with no structural cap the belly auto-fill is off entirely, so only "
        "the checked bags reach the holds",
    ),
    (
        "cargo_target_cg",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 90_000.0},
        },
        130_000.0,
        30.0,
        "a freighter with a real operating-empty mass and CG, so the required "
        "payload CG is solved backwards from the target aircraft CG",
    ),
    (
        "cargo_no_oew",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 60_000.0},
        },
        0.0,
        0.0,
        "the same freighter with no operating-empty mass supplied, which is "
        "the branch every caller that only wants the payload's own CG takes",
    ),
    (
        "cargo_door_proximity",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 70_000.0},
            "cabin": {
                "cargo": {
                    "loading_strategy": "door_proximity",
                    "main_door_x_m": 22.0,
                    "fwd_door_x_m": 9.0,
                    "aft_door_x_m": 45.0,
                }
            },
        },
        120_000.0,
        29.0,
        "the door-proximity priority with all three doors placed explicitly, "
        "so none of the three 'or' fallbacks fires",
    ),
    (
        "cargo_uniform",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 70_000.0},
            "cabin": {"cargo": {"loading_strategy": "uniform"}},
        },
        0.0,
        0.0,
        "the one strategy that spreads the load rather than filling positions "
        "in priority order (fill_full=False)",
    ),
    (
        "cargo_min_pallets",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 40_000.0},
            "cabin": {
                "cargo": {"loading_strategy": "min_pallets", "target_cg_pct_mac": 0.0}
            },
        },
        0.0,
        0.0,
        "the fall-through strategy branch, with a target of zero so the 25% "
        "MAC default is what the loader trims to",
    ),
    (
        "cargo_narrowbody",
        {
            "preset": "A320-200",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 16_000.0},
        },
        45_000.0,
        16.0,
        "a narrowbody hold too shallow for an LD3, so the loader degrades "
        "through LOWER_HOLD_FALLBACKS to the reduced-height container",
    ),
    (
        "cargo_lower_deck_only",
        {
            "preset": "B787-9",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 30_000.0},
            "cabin": {"cargo": {"use_main_deck": False, "cg_trim_step_kg": 500.0}},
        },
        0.0,
        0.0,
        "no main deck at all, and a coarse trim step, which is the "
        "configuration the passenger engine builds its bag loader with",
    ),
    (
        "cargo_over_capacity",
        {
            "preset": "A320-200",
            "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 400_000.0},
        },
        0.0,
        0.0,
        "more payload asked for than there are positions to hold it, so the "
        "solver's total-capacity clamp is what decides the load",
    ),
]

# (case name, ALASConfig overlay, mix, why).
SIMULATION_CASES = [
    ("a320_economy", {"preset": "A320-200"}, {"Economy": 1.0}, "the all-economy mix"),
    ("b787_economy", {"preset": "B787-9"}, {"Economy": 1.0}, "the same on a widebody"),
    (
        "b787_iberia",
        {"preset": "B787-9"},
        {"Business": 0.40, "Premium": 0.09, "Economy": 0.51},
        "the shipped three-class long-haul mix, where the premium share drives "
        "service_reserve_len hard",
    ),
    (
        "b787_emirates",
        {"preset": "B787-9"},
        {"First": 0.09, "Business": 0.30, "Economy": 0.61},
        "a mix with a first cabin, so all four class branches are reached "
        "across this section",
    ),
    (
        "a380_iberia",
        {"preset": "A380-800"},
        {"Business": 0.40, "Premium": 0.09, "Economy": 0.51},
        "the multi-class mix across two decks, where the per-deck cap rather "
        "than the total is what binds",
    ),
    (
        "a320_tiny",
        {"preset": "A320-200", "cabin": {"passenger": {"first": {"pitch_m": 2.5}}}},
        {"First": 1.0},
        "one very long-pitch class, which is where the last-class 'absorb "
        "whatever is left' budget branch shows",
    ),
]

# (case name, ALASConfig overlay, why).
CABIN_PRESET_CASES = [
    ("ryanair", {"preset": "A320-200", "requirements": {"cabin_preset": "Ryanair"}},
     "the single-class high-density preset, which rewrites economy's geometry"),
    ("iberia", {"preset": "B787-9", "requirements": {"cabin_preset": "Iberia"}},
     "the three-class preset, which writes four classes and a length mix"),
    ("emirates", {"preset": "B787-9", "requirements": {"cabin_preset": "Emirates"}},
     "the premium-heavy preset, the only one that fills the first cabin"),
    (
        "custom_percent",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Custom"},
            "cabin": {"passenger": {"class_mix_mode": "percent"}},
        },
        "Custom in percent mode re-solves the counts from the shares on every "
        "call rather than returning early",
    ),
    (
        "custom_count_already_seated",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Custom"},
            "cabin": {
                "passenger": {
                    "class_mix_mode": "count",
                    "economy": {"count": 111},
                }
            },
        },
        "Custom in count mode with seats already configured is the no-op "
        "return the optimizer loop takes on every evaluation",
    ),
    (
        "custom_count_empty",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Custom"},
            "cabin": {"passenger": {"class_mix_mode": "count"}},
        },
        "Custom in count mode with nothing configured yet floor-fills economy "
        "so the cabin is never silently empty",
    ),
    (
        "custom_cargo_empty",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Custom", "aircraft_type": "cargo",
                             "cargo_payload_kg": 0.0},
        },
        "the freighter side of the same first-time fill: 70% of geometric "
        "capacity",
    ),
    (
        "max_payload",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Max payload", "aircraft_type": "cargo"},
        },
        "the freighter preset that fills every position, and forces the four "
        "cargo defaults on the way",
    ),
    (
        "dense_payload",
        {
            "preset": "B787-9",
            "requirements": {"cabin_preset": "Dense payload", "aircraft_type": "cargo"},
        },
        "the same at 70% of capacity",
    ),
    (
        "unknown_preset",
        {"preset": "A320-200", "requirements": {"cabin_preset": "Nonesuch"}},
        "a name no branch matches returns without touching anything, which is "
        "what keeps a stale saved file from emptying a cabin",
    ),
]

_PROBE_HEIGHT = 1.25  # the drawn seat-row height, so clamp_height is exercised
_PCT_MAC_PROBES = [-20.0, 0.0, 12.5, 25.0, 40.0, 100.0]


def _config_and_plane(overlay: dict):
    """The configuration an overlay describes, and the aircraft it builds."""
    config = ALASConfig.from_dict(overlay)
    preset_name = overlay.get("preset", "")
    dv = get_preset(preset_name).design_vector if preset_name else None
    plane = AircraftBuilder(config.geometry).build(dv, include_engines=False)
    return config, plane, dv


def _deck_record(deck) -> dict:
    return {
        "name": deck.name,
        "floor_frac": deck.floor_frac,
        "ceil_frac": deck.ceil_frac,
        "width_factor": deck.width_factor,
        "is_passenger": deck.is_passenger,
    }


def _stations(g: CabinGeometry) -> list[float]:
    """Probe stations, deliberately running past both ends of the section list."""
    span = g.x_max - g.x_min
    return [g.x_min - 2.0, g.x_min] + [
        g.x_min + span * i / 12.0 for i in range(1, 12)
    ] + [g.x_max, g.x_max + 3.0]


def _geometry_record(g: CabinGeometry) -> dict:
    decks = list(g.passenger_decks) + [g.lower_deck]
    stations = _stations(g)
    return {
        "wall": g.wall,
        "x_min": g.x_min,
        "x_max": g.x_max,
        "fus_len": g.fus_len,
        "diameter_m": g.diameter_m,
        "cabin_start_x": g.cabin_start_x,
        "tailcone_len": g.tailcone_len,
        "cabin_end_x": g.cabin_end_x,
        "is_double_deck": g.is_double_deck,
        "mac": g.mac,
        "x_wing_ac": g.x_wing_ac,
        "x_lemac": g.x_lemac,
        "x_wing_le": g.x_wing_le,
        "wing_root_chord": g.wing_root_chord,
        "passenger_decks": [_deck_record(d) for d in g.passenger_decks],
        "lower_deck": _deck_record(g.lower_deck),
        "wing_box_x_range": list(g.wing_box_x_range()),
        "stations": stations,
        "width_at": [g.width_at(x) for x in stations],
        "height_at": [g.height_at(x) for x in stations],
        "zc_at": [g.zc_at(x) for x in stations],
        "internal_half_height": [g.internal_half_height(x) for x in stations],
        "x_to_pct_mac": [g.x_to_pct_mac(x) for x in stations],
        "pct_mac_to_x": [g.pct_mac_to_x(p) for p in _PCT_MAC_PROBES],
        "per_deck": [
            {
                "name": d.name,
                "floor_z": [g.floor_z(d, x) for x in stations],
                "ceil_z": [g.ceil_z(d, x) for x in stations],
                "deck_height": [g.deck_height(d, x) for x in stations],
                "clamp_height": [g.clamp_height(d, x, _PROBE_HEIGHT) for x in stations],
                "item_z": [g.item_z(d, x, _PROBE_HEIGHT) for x in stations],
                "usable_width": [g.usable_width(d, x) for x in stations],
            }
            for d in decks
        ],
    }


def _item_record(item) -> dict:
    return {
        "kind": item.kind,
        "deck": item.deck,
        "x": item.x,
        "y": item.y,
        "z": item.z,
        "length": item.length,
        "width": item.width,
        "height": item.height,
        "mass": item.mass,
        "label": item.label,
        "meta": item.meta,
    }


def _layout_record(layout) -> dict:
    return {
        "mode": layout.mode,
        "total_mass": layout.total_mass,
        "cg_x": layout.cg_x,
        "cg_y": layout.cg_y,
        "decks": layout.decks(),
        "items": [_item_record(it) for it in layout.items],
        "summary": layout.summary,
    }


def _class_record(cls) -> dict:
    return {
        "share_pct": cls.share_pct,
        "count": cls.count,
        "abreast": cls.abreast,
        "pitch_m": cls.pitch_m,
        "width_m": cls.width_m,
        "mass_per_pax_kg": cls.mass_per_pax_kg,
    }


def _cabin_record(config) -> dict:
    pax = config.cabin.passenger
    cargo = config.cabin.cargo
    return {
        "class_mix_mode": pax.class_mix_mode,
        "first": _class_record(pax.first),
        "business": _class_record(pax.business),
        "premium": _class_record(pax.premium),
        "economy": _class_record(pax.economy),
        "num_passengers": config.requirements.num_passengers,
        "cargo_payload_kg": config.requirements.cargo_payload_kg,
        "use_main_deck": cargo.use_main_deck,
        "main_deck_uld": cargo.main_deck_uld,
        "lower_deck_uld": cargo.lower_deck_uld,
        "loading_strategy": cargo.loading_strategy,
    }


def _geometry_section() -> list[dict]:
    out = []
    for name, overlay, why in GEOMETRY_CASES:
        config, plane, _dv = _config_and_plane(overlay)
        g = CabinGeometry(plane, config.geometry, config.cabin.passenger.wall_thickness_m)
        out.append(
            {"name": name, "input": overlay, "why": why, "geometry": _geometry_record(g)}
        )
    return out


def _layout_section() -> list[dict]:
    out = []
    for name, overlay, oew, x_oew, why in LAYOUT_CASES:
        config, plane, _dv = _config_and_plane(overlay)
        layout = build_payload_layout(plane, config, oew, x_oew)
        out.append(
            {
                "name": name,
                "input": overlay,
                "oew": oew,
                "x_oew": x_oew,
                "why": why,
                "layout": _layout_record(layout),
            }
        )
    return out


def _simulation_section() -> list[dict]:
    out = []
    for name, overlay, mix, why in SIMULATION_CASES:
        config, plane, _dv = _config_and_plane(overlay)
        g = CabinGeometry(plane, config.geometry, config.cabin.passenger.wall_thickness_m)
        counts = simulate_passenger_counts(g, config.cabin.passenger, mix)
        out.append(
            {
                "name": name,
                "input": overlay,
                "mix": mix,
                "why": why,
                "counts": counts,
            }
        )
    return out


def _cabin_preset_section() -> list[dict]:
    out = []
    for name, overlay, why in CABIN_PRESET_CASES:
        config, _plane, dv = _config_and_plane(overlay)
        before = _cabin_record(config)
        apply_cabin_preset(config, dv)
        out.append(
            {
                "name": name,
                "input": overlay,
                "why": why,
                "before": before,
                "after": _cabin_record(config),
            }
        )
    return out


def _oew_section() -> list[dict]:
    """`oew_and_cg` over a real breakdown, and over one with nothing in it."""
    config, plane, _dv = _config_and_plane({"preset": "B787-9"})
    masses, coords, _cg = run_mass_analysis(
        plane, config.requirements, config.geometry, config.mass_model
    )
    masses = {k: float(v) for k, v in masses.items()}
    coords = {k: [float(c) for c in v] for k, v in coords.items()}

    empty = {k: 0.0 for k in masses}
    negative = copy.deepcopy(masses)
    negative["Wing"] = -1000.0

    # Upstream's "component has a mass and no coordinate" branch is not a case
    # here: every caller passes `calculate_component_masses` and
    # `define_mass_coordinates` together, and those always populate the same
    # ten names, so no input this program can produce reaches it. The port's
    # typed signature makes that unreachability structural rather than
    # incidental -- see `alas-payload::oew`'s module doc.
    cases = [
        ("b787", masses, coords, "a real component breakdown"),
        ("all_zero", empty, coords, "no mass at all: the divide-by-zero guard"),
        (
            "negative_component",
            negative,
            coords,
            "a negative mass is floored at zero rather than subtracting moment",
        ),
    ]
    out = []
    for name, m, c, why in cases:
        total, cg = oew_and_cg(m, c)
        out.append(
            {
                "name": name,
                "why": why,
                "masses": m,
                "coords": c,
                "oew": total,
                "x_oew": cg,
            }
        )
    return out


def main() -> None:
    payload = {
        "geometry": _geometry_section(),
        "layouts": _layout_section(),
        "simulations": _simulation_section(),
        "cabin_presets": _cabin_preset_section(),
        "oew": _oew_section(),
    }

    double_deck = [c for c in payload["geometry"] if c["geometry"]["is_double_deck"]]
    if not double_deck:
        raise SystemExit(
            "no geometry case reached the double-deck branch; the two-deck "
            "DeckSpec pair would go unchecked"
        )

    overflowed = [
        c
        for c in payload["layouts"]
        for item in c["layout"]["items"]
        if item["label"] == "Bulk overflow"
    ]
    if not overflowed:
        raise SystemExit(
            "no layout case placed a loose bulk block; the overflow guard "
            "would go unchecked, and it is only reachable when the checked "
            "baggage alone exceeds the holds"
        )

    _framework.write(
        "payload",
        "layout",
        payload,
        description=(
            "alas.physics.payload / cabin_layout / cargo_loader: CabinGeometry's "
            "samplers, build_payload_layout's full item sequence and summary for "
            "passenger and freighter cabins, simulate_passenger_counts, "
            "apply_cabin_preset and oew_and_cg, on aircraft built from the "
            "shipped presets"
        ),
    )


if __name__ == "__main__":
    main()
