# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Detailed payload layout -- the detailed interior model.

This module turns a built :class:`asb.Airplane` plus the cabin/payload
configuration into a concrete :class:`PayloadLayout`: a list of physical items
(seat rows, ULD pallets, galleys, lavatories, exits, baggage) distributed across
the **main deck**, the **lower-deck holds**, and (for double-deck aircraft such
as the A380) the **upper deck**, each with a mass and a position. From those it
derives the true payload mass and longitudinal centre of gravity.

It IS called inside the optimizer loop, not just the baseline pass, the final
analysis, and the Cabin/Payload preview: the CG-envelope two-pass fix in
``optimization/objective.py`` (see ``docs/architecture.md`` §1) has every
candidate evaluation call :func:`build_payload_layout` to override the payload
CG before checking envelope compliance, since the lumped
``cabin_payload_density_kg_m`` model in :mod:`alas.physics.mass` alone can
differ from this module's true mass-weighted CG by several percent MAC. The
lumped model is now only ever a cheap first-pass estimate (to seed OEW/x_oew
for this module's own inputs), never the final payload CG any caller keeps.

The same :class:`PayloadLayout` feeds three consumers: the mass analysis
(payload CG override), the visualization (deck plans / 3D preview), and the
report.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Tuple

import aerosandbox as asb
import numpy as np

from .mass import OEW_KEYS
from .stability import _xsec_height, _xsec_width


@dataclass
class DeckItem:
    """One physical payload element placed in the aircraft.

    Coordinates are absolute aircraft coordinates [m]; ``length`` is along x
    (longitudinal), ``width`` along y (lateral), ``height`` along z. ``mass`` is
    in kg (0 for non-mass monuments such as exits). ``meta`` carries kind-specific
    extras (class name, ULD code, exit type, fill fraction, ...).
    """

    kind: str  # 'seat_row' | 'uld' | 'galley' | 'lav' | 'exit' | 'bag'
    deck: str  # 'main' | 'lower' | 'upper'
    x: float
    y: float
    z: float
    length: float
    width: float
    mass: float = 0.0
    height: float = 0.0
    label: str = ""
    meta: Dict = field(default_factory=dict)


@dataclass
class PayloadLayout:
    """The complete interior layout and its mass properties."""

    mode: str  # 'passenger' | 'cargo'
    items: List[DeckItem] = field(default_factory=list)
    total_mass: float = 0.0
    cg_x: float = 0.0
    cg_y: float = 0.0
    summary: Dict = field(default_factory=dict)

    def by_deck(self, deck: str) -> List[DeckItem]:
        return [it for it in self.items if it.deck == deck]

    def by_kind(self, kind: str) -> List[DeckItem]:
        return [it for it in self.items if it.kind == kind]

    def decks(self) -> List[str]:
        seen = []
        for it in self.items:
            if it.deck not in seen:
                seen.append(it.deck)
        return seen

    def recompute_cg(self) -> None:
        """Recompute total mass and CG from the mass-bearing items."""
        m = mx = my = 0.0
        for it in self.items:
            if it.mass > 0:
                m += it.mass
                mx += it.mass * it.x
                my += it.mass * it.y
        self.total_mass = m
        self.cg_x = mx / m if m > 0 else 0.0
        self.cg_y = my / m if m > 0 else 0.0


# ---------------------------------------------------------------------------
# Deck geometry extraction
# ---------------------------------------------------------------------------
@dataclass
class DeckSpec:
    """A horizontal deck: its floor, its ceiling, and its usable width.

    ``floor_frac`` and ``ceil_frac`` are fractions of the internal half-height b
    (relative to the section centre, +up), so the deck occupies the vertical band
    ``[zc + floor_frac·b, zc + ceil_frac·b]``. Items sit *on* the floor and their
    height is clamped to the deck height, so decks never overlap vertically.
    ``width_factor`` scales the internal width to the usable floor width
    (accounting for a deck sitting off the section centre, where the cross-section
    is narrower).
    """

    name: str
    floor_frac: float
    ceil_frac: float
    width_factor: float
    is_passenger: bool


class CabinGeometry:
    """Samples the built fuselage to expose cabin/deck geometry.

    All the layout engines read from this so they share one consistent notion of
    where the cabin is, how wide the floor is at each station, and where each
    deck floor sits -- including the double-deck (A380) case.
    """

    def __init__(
        self, plane: asb.Airplane, geometry_config, wall_thickness_m: float = 0.15
    ):
        fus = plane.fuselages[0]
        self.wall = float(wall_thickness_m)

        xs = np.array([float(s.xyz_c[0]) for s in fus.xsecs])
        order = np.argsort(xs)
        self._x = xs[order]
        self._w = np.array([_xsec_width(s) for s in fus.xsecs])[order]
        self._h = np.array([_xsec_height(s) for s in fus.xsecs])[order]
        self._zc = np.array([float(s.xyz_c[2]) for s in fus.xsecs])[order]

        self.x_min = float(self._x[0])
        self.x_max = float(self._x[-1])
        self.fus_len = self.x_max - self.x_min

        fg = geometry_config.fuselage
        self.diameter_m = float(fg.diameter_m)
        self.cabin_start_x = float(fg.cabin_start_x_m)
        self.tailcone_len = float(fg.tailcone_length_m)
        self.cabin_end_x = self.x_max - self.tailcone_len

        # Double-deck detection: a markedly taller-than-wide ovoid body (A380).
        height_m = getattr(fg, "height_m", None)
        self.is_double_deck = (
            height_m is not None and float(height_m) >= self.diameter_m * 1.15
        )

        # MAC frame for %MAC <-> metre conversions.
        wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
        self.mac = float(plane.c_ref)
        self.x_wing_ac = float(wing.aerodynamic_center()[0])
        self.x_lemac = self.x_wing_ac - 0.25 * self.mac
        # Centre wing box (for lower-hold exclusion).
        self.x_wing_le = float(wing.xsecs[0].xyz_le[0])
        self.wing_root_chord = float(wing.xsecs[0].chord)

        if self.is_double_deck:
            self.passenger_decks = [
                DeckSpec(
                    "main",
                    floor_frac=-0.48,
                    ceil_frac=-0.02,
                    width_factor=0.95,
                    is_passenger=True,
                ),
                DeckSpec(
                    "upper",
                    floor_frac=+0.04,
                    ceil_frac=+0.55,
                    width_factor=0.80,
                    is_passenger=True,
                ),
            ]
            self.lower_deck = DeckSpec(
                "lower",
                floor_frac=-0.95,
                ceil_frac=-0.50,
                width_factor=0.55,
                is_passenger=False,
            )
        else:
            self.passenger_decks = [
                DeckSpec(
                    "main",
                    floor_frac=-0.18,
                    ceil_frac=+0.95,
                    width_factor=0.97,
                    is_passenger=True,
                ),
            ]
            self.lower_deck = DeckSpec(
                "lower",
                floor_frac=-0.95,
                ceil_frac=-0.20,
                width_factor=0.60,
                is_passenger=False,
            )

    # -- samplers ------------------------------------------------------------
    def width_at(self, x: float) -> float:
        return float(np.interp(x, self._x, self._w))

    def height_at(self, x: float) -> float:
        return float(np.interp(x, self._x, self._h))

    def zc_at(self, x: float) -> float:
        return float(np.interp(x, self._x, self._zc))

    def internal_half_height(self, x: float) -> float:
        return max(0.1, self.height_at(x) / 2.0 - self.wall)

    def floor_z(self, deck: DeckSpec, x: float) -> float:
        return self.zc_at(x) + deck.floor_frac * self.internal_half_height(x)

    def ceil_z(self, deck: DeckSpec, x: float) -> float:
        return self.zc_at(x) + deck.ceil_frac * self.internal_half_height(x)

    def deck_height(self, deck: DeckSpec, x: float) -> float:
        return max(
            0.3, (deck.ceil_frac - deck.floor_frac) * self.internal_half_height(x)
        )

    def clamp_height(self, deck: DeckSpec, x: float, h: float) -> float:
        """Clamp an item height to fit within the deck."""
        return min(h, self.deck_height(deck, x))

    def item_z(self, deck: DeckSpec, x: float, h: float) -> float:
        """Vertical centre of an item of height ``h`` resting on the deck floor."""
        return self.floor_z(deck, x) + self.clamp_height(deck, x, h) / 2.0

    def usable_width(self, deck: DeckSpec, x: float) -> float:
        """Usable floor width [m] available for seats/ULDs at station x on deck."""
        internal = max(0.0, self.width_at(x) - 2.0 * self.wall)
        return max(0.0, internal * deck.width_factor)

    # -- frames --------------------------------------------------------------
    def x_to_pct_mac(self, x: float) -> float:
        return (x - self.x_lemac) / max(self.mac, 1e-6) * 100.0

    def pct_mac_to_x(self, pct: float) -> float:
        return self.x_lemac + (pct / 100.0) * self.mac

    def wing_box_x_range(self) -> Tuple[float, float]:
        return self.x_wing_le, self.x_wing_le + self.wing_root_chord


# ---------------------------------------------------------------------------
# OEW helper (shared by the pipeline orchestration)
# ---------------------------------------------------------------------------
def oew_and_cg(
    masses: Dict[str, float], coords: Dict[str, List[float]]
) -> Tuple[float, float]:
    """Return (OEW mass, OEW longitudinal CG x) from a component mass/coord dict."""
    m_tot = mom = 0.0
    for k in OEW_KEYS:
        m = max(0.0, masses.get(k, 0.0))
        if m <= 0 or k not in coords:
            continue
        m_tot += m
        mom += m * coords[k][0]
    return m_tot, (mom / m_tot if m_tot > 0 else 0.0)


# ---------------------------------------------------------------------------
# Dispatcher
# ---------------------------------------------------------------------------
def build_payload_layout(
    plane: asb.Airplane, config, oew: float = 0.0, x_oew: float = 0.0
) -> PayloadLayout:
    """Build the detailed payload layout for ``plane`` under ``config``.

    Dispatches on ``config.requirements.aircraft_type``. ``oew`` and ``x_oew``
    (the operating-empty CG) let the cargo solver place load so the *aircraft* CG
    hits its target; they may be 0 when only the payload's own CG is needed.
    """
    req = config.requirements
    geom = config.geometry
    cabin = config.cabin
    cg_geom = CabinGeometry(plane, geom, cabin.passenger.wall_thickness_m)

    if req.aircraft_type == "cargo":
        from .cargo_loader import build_cargo_layout

        return build_cargo_layout(cg_geom, cabin.cargo, req, oew, x_oew)
    else:
        from .cabin_layout import build_passenger_layout

        return build_passenger_layout(cg_geom, cabin.passenger, req, oew, x_oew)


def simulate_passenger_counts(
    cg_geom: CabinGeometry, pax_cfg, mix: Dict[str, float]
) -> Dict[str, int]:
    """Simulate seat placement across all passenger decks to maximize seating space
    and return the seat count for each class based on the given length mix percentages.

    Capped per deck at :func:`alas.physics.cabin_layout.max_certifiable_capacity`
    (CS-25/FAR-25.807 exit-derived maximum) so this fast auto-sizer -- which drives
    ``requirements.num_passengers`` (and, through it, the FIRST-pass lumped payload
    mass ``mass.calculate_component_masses`` sees before every caller, including the
    optimizer loop, rebuilds the real cabin layout below) -- agrees with what
    :func:`alas.physics.cabin_layout.build_passenger_layout` (the detailed
    layout engine, called every optimizer evaluation as well as at baseline/final,
    not run-once) would actually seat. Without this cap, a preset like "Ryanair" on
    a large-diameter/long fuselage would report a passenger count limited only by
    floor space (e.g. 1400+ on an A380-sized body), far past what real exit
    regulations would ever certify (A380 real-world max: 853).
    """
    from .cabin_layout import (
        EXIT_TYPES,
        _abreast,
        _cabin_deck_segments,
        _select_exit_type,
        max_certifiable_capacity,
        resolve_aisle_width,
        service_reserve_len,
    )
    import math as _math

    counts = {"First": 0, "Business": 0, "Premium": 0, "Economy": 0}
    _MONUMENT_LEN = 0.95
    # Transport-category auto-sizing always targets >= 20 seats, so the
    # FAR/CS-25.815 auto aisle width resolves to the 0.51 m (20 in) clearance.
    aisle_w = resolve_aisle_width(pax_cfg, 20)

    # We do a deck-by-deck pass, matching build_passenger_layout
    segments = _cabin_deck_segments(cg_geom)
    deck_caps = max_certifiable_capacity(cg_geom, pax_cfg)
    exit_cap = EXIT_TYPES[_select_exit_type(cg_geom.diameter_m)].capacity_per_side

    for deck, seg_x0, seg_x1 in segments:
        total_length = seg_x1 - seg_x0
        deck_cap = deck_caps.get(deck.name, deck_caps["total"])

        # We process classes from forward to aft
        classes_to_layout = []
        for name in ["First", "Business", "Premium", "Economy"]:
            p = mix.get(name, 0.0)
            if p > 0:
                classes_to_layout.append((name, p))

        # Total monuments: front, aft, and between classes
        num_classes = len(classes_to_layout)
        if num_classes == 0:
            continue

        def _count_deck(n_mid_bays: int):
            """Seat the deck with the given number of extra mid-cabin
            (door-station) monument bays charged against seating length."""
            local = {"First": 0, "Business": 0, "Premium": 0, "Economy": 0}
            seated = 0
            # Reserve inter-class monument bays PLUS the galley/lav/closet/
            # crew-rest floor that scales with premium content -- the latter is
            # what keeps a premium 3-class cabin from floor-filling economy into
            # every metre the premium cabins leave free (see service_reserve_len).
            l_seating = (
                total_length
                - (num_classes + 1 + n_mid_bays) * _MONUMENT_LEN
                - service_reserve_len(total_length, mix)
            )
            if l_seating <= 0:
                return local, seated
            x = seg_x0 + _MONUMENT_LEN
            # `remaining` is the seat-row length budget still unspent, across
            # the WHOLE deck -- not each class's own nominal length share.
            # Splitting `l_seating` into per-class lengths and then floor-
            # truncating each independently (the previous `x_end_section`
            # cutoff) throws away up to one row's worth of floor *in every
            # class section* (a partial pitch left over below a class's own
            # boundary was never reclaimed by the next class) -- compounding
            # to several wasted rows across a 3-4-class cabin. Earlier
            # classes now get their share rounded to the nearest whole row
            # (not floored, so small rounding errors average out rather than
            # always undershooting); the LAST class absorbs whatever's left
            # of the shared budget after that, the same way a real airline
            # sets an exact business/first row count and lets economy fill
            # the rest of the floor -- eliminating the compounding waste
            # while leaving `service_reserve_len`'s deliberate multi-class
            # spare-floor reservation (above) completely untouched.
            remaining = l_seating
            for i, (name, pct) in enumerate(classes_to_layout):
                if name == "First":
                    cls_cfg = pax_cfg.first
                elif name == "Business":
                    cls_cfg = pax_cfg.business
                elif name == "Premium":
                    cls_cfg = pax_cfg.premium
                else:
                    cls_cfg = pax_cfg.economy
                pitch = max(cls_cfg.pitch_m, 0.30)
                is_last = i == num_classes - 1
                budget = (
                    remaining
                    if is_last
                    else min(remaining, max(0, round(pct * l_seating / pitch)) * pitch)
                )
                n_rows = 0
                while n_rows * pitch + pitch <= budget + 1e-9 and seated < deck_cap:
                    ab = _abreast(cls_cfg, deck, cg_geom, aisle_w, x)
                    ab = min(ab, deck_cap - seated)
                    local[name] += ab
                    seated += ab
                    x += pitch
                    n_rows += 1
                remaining -= n_rows * pitch
                if i < num_classes - 1:
                    x += _MONUMENT_LEN
            return local, seated

        # Two passes: first without mid-cabin bays to estimate the exit-pair
        # count, then again charging one monument bay per door interval --
        # the same floor-length budget build_passenger_layout reserves, so
        # preset counts match what the detailed layout can actually seat.
        _local0, seated0 = _count_deck(0)
        n_pairs = (
            max(2 if seated0 > 110 else 1, _math.ceil(seated0 / exit_cap))
            if seated0
            else 1
        )
        local, _seated = _count_deck(max(0, n_pairs - 1))
        for k, v in local.items():
            counts[k] += v

    return counts


def apply_cabin_preset(config, design_vector) -> None:
    """Compute and apply cabin layout preset counts and update requirements payload."""
    req = config.requirements
    geom = config.geometry
    cabin = config.cabin

    preset = req.cabin_preset

    if preset == "Custom":
        # num_passengers/cargo_payload_kg are read-only, floor-fill-derived
        # totals; the UI shows them as read-only derived fields, with no way
        # for the user to type a starting value directly. "Custom" means
        # hand-editing PER-CLASS counts / cargo settings on the Cabin &
        # Payload tab, which this still leaves entirely alone; this only
        # auto-fills a sensible non-zero STARTING POINT (maximise floor usage
        # with the economy class's own current pitch/width, or a 70% dense
        # cargo load) the very first time Custom is selected with nothing
        # configured yet, exactly like a named preset would, so switching to
        # Custom never leaves the cabin silently empty with no way to change
        # it. Never overwrites once the user has set anything, and (the
        # common repeated-candidate case, e.g. every optimizer evaluation)
        # skips building the temporary airplane entirely once a Custom cabin
        # is already configured -- an immediate no-op return that must stay
        # cheap for every one of those calls.
        # In percent mode the per-class SHARES are the user's input and the
        # counts are a derived output, so a Custom cabin must be re-solved
        # whenever those shares change -- returning early on "already has
        # seats" (correct for count mode, where counts ARE the input) would
        # freeze the layout at whatever the first solve produced and silently
        # ignore every subsequent share edit.
        if req.aircraft_type != "cargo" and cabin.passenger.class_mix_mode == "percent":
            mix = cabin.passenger.length_share_mix()
            if not mix:
                return
            from ..geometry.aircraft_builder import AircraftBuilder

            plane = AircraftBuilder(geom).build(design_vector, include_engines=False)
            cg_geom = CabinGeometry(plane, geom, cabin.passenger.wall_thickness_m)
            counts = simulate_passenger_counts(cg_geom, cabin.passenger, mix)
            cabin.passenger.first.count = counts.get("First", 0)
            cabin.passenger.business.count = counts.get("Business", 0)
            cabin.passenger.premium.count = counts.get("Premium", 0)
            cabin.passenger.economy.count = counts.get("Economy", 0)
            req.num_passengers = int(sum(counts.values()))
            return

        already_configured = (
            req.cargo_payload_kg > 0.0
            if req.aircraft_type == "cargo"
            else cabin.passenger.total_seats() > 0
        )
        if already_configured:
            return
        from ..geometry.aircraft_builder import AircraftBuilder

        plane = AircraftBuilder(geom).build(design_vector, include_engines=False)
        cg_geom = CabinGeometry(plane, geom, cabin.passenger.wall_thickness_m)
        if req.aircraft_type == "cargo":
            from .cargo_loader import CargoLoadManager

            mgr = CargoLoadManager(cg_geom, cabin.cargo)
            req.cargo_payload_kg = float(0.70 * mgr.total_capacity())
        else:
            counts = simulate_passenger_counts(
                cg_geom, cabin.passenger, {"Economy": 1.0}
            )
            cabin.passenger.economy.count = counts.get("Economy", 0)
            req.num_passengers = int(sum(counts.values()))
        return

    # Build a temporary airplane to get the cabin geometry
    from ..geometry.aircraft_builder import AircraftBuilder

    builder = AircraftBuilder(geom)
    plane = builder.build(design_vector, include_engines=False)
    cg_geom = CabinGeometry(plane, geom, cabin.passenger.wall_thickness_m)

    if req.aircraft_type == "cargo":
        # Ensure default cargo settings
        cabin.cargo.use_main_deck = True
        cabin.cargo.main_deck_uld = "PMC"
        cabin.cargo.lower_deck_uld = "LD3"
        cabin.cargo.loading_strategy = "target_cg"

        from .cargo_loader import CargoLoadManager

        mgr = CargoLoadManager(cg_geom, cabin.cargo)
        total_capacity = mgr.total_capacity()

        if preset == "Max payload":
            req.cargo_payload_kg = float(total_capacity)
        elif preset == "Dense payload":
            req.cargo_payload_kg = float(0.70 * total_capacity)

    else:
        # Define class mix percentages of available seating length
        if preset == "Ryanair":
            # 100% economy, high density seat dimensions
            cabin.passenger.first.count = 0
            cabin.passenger.business.count = 0
            cabin.passenger.premium.count = 0

            # Set Ryanair standard economy seat geometry. High density comes
            # from the TIGHT PITCH (0.7112 m / 28 in -- payload_processed.md
            # Matrix B's regulatory/industry floor for economy pitch), NOT
            # from an extra seat abreast: real high-density economy (incl.
            # Ryanair) keeps the aircraft's normal abreast (e.g. 787 = 9,
            # A320 = 6, A220 = 5). A 0.43 m footprint let the auto-abreast
            # squeeze a 10th seat into a 787 the real fuselage can't fit;
            # 0.46 m (incl. armrest) is the realistic minimum and yields the
            # correct abreast per fuselage. This 28 in pitch is what makes a
            # single-class A320 baseline reach its real ~180-seat certified
            # capacity at 100% floor utilization (see cabin_layout.
            # build_passenger_layout's single-class floor-stretch): a looser
            # 30 in pitch physically fits only ~168 rows in the A320's cabin
            # length, well short of both the floor and the exit-derived cap.
            # The wider-body types (787-9, A380-800) are exit-limited, not
            # floor-limited, at either pitch, so this change doesn't affect
            # their seat counts (see min_exit_pair_spacing_m's note).
            cabin.passenger.economy.pitch_m = 0.7112
            cabin.passenger.economy.width_m = 0.46
            cabin.passenger.economy.mass_per_pax_kg = 80.0

            mix = {"Economy": 1.0}
        elif preset == "Iberia":
            # Realistic long-haul 3-class. Business is full lie-flat 1-2-1
            # (abreast=4) at an effective staggered-suite pitch (~1.55 m), not
            # a 2-2-2 recliner -- the auto-abreast would otherwise pack 6-abreast
            # business and roughly double the real seat count. Premium economy
            # is 2-3-2 (abreast=7). The LENGTH mix reflects that lie-flat
            # business consumes far more floor per seat than economy, so the
            # blended density is ~half the all-economy max (real 787-9 3-class
            # ~222 vs ~410 all-economy). See _service_reserve_len for the
            # premium galley/crew-rest floor that further limits economy.
            cabin.passenger.first.count = 0

            cabin.passenger.business.abreast = 4
            cabin.passenger.business.pitch_m = 1.55
            cabin.passenger.business.width_m = 0.70
            cabin.passenger.business.mass_per_pax_kg = 90.0

            cabin.passenger.premium.abreast = 7
            cabin.passenger.premium.pitch_m = 0.97
            cabin.passenger.premium.width_m = 0.52
            cabin.passenger.premium.mass_per_pax_kg = 86.0

            cabin.passenger.economy.abreast = 0  # auto (9 on a 787, 6 on A320)
            cabin.passenger.economy.pitch_m = 0.79
            cabin.passenger.economy.width_m = 0.46
            cabin.passenger.economy.mass_per_pax_kg = 84.0

            mix = {"Business": 0.40, "Premium": 0.09, "Economy": 0.51}
        elif preset == "Emirates":
            # Premium-heavy 3-class. First is enclosed suites 1-2-1 (abreast=4)
            # at a long ~2.0 m suite pitch; business is lie-flat 1-2-1
            # (abreast=4) at ~1.55 m. Both consume far more floor per seat than
            # economy, so the length mix is weighted toward the premium cabins
            # even though economy still holds the most seats.
            cabin.passenger.first.abreast = 4
            cabin.passenger.first.pitch_m = 2.00
            cabin.passenger.first.width_m = 0.95
            cabin.passenger.first.mass_per_pax_kg = 96.0

            cabin.passenger.business.abreast = 4
            cabin.passenger.business.pitch_m = 1.55
            cabin.passenger.business.width_m = 0.70
            cabin.passenger.business.mass_per_pax_kg = 90.0

            cabin.passenger.premium.count = 0

            cabin.passenger.economy.abreast = 0  # auto
            cabin.passenger.economy.pitch_m = 0.81
            cabin.passenger.economy.width_m = 0.46
            cabin.passenger.economy.mass_per_pax_kg = 84.0

            mix = {"First": 0.09, "Business": 0.30, "Economy": 0.61}
        else:
            return

        # Mirror the preset's own length mix into the per-class share_pct
        # fields, so the Cabin & Payload page shows the shares that actually
        # produced this layout instead of stale/default ones.
        cabin.passenger.set_length_share_mix(mix)

        # Simulate seating layout to find the counts
        counts = simulate_passenger_counts(cg_geom, cabin.passenger, mix)

        # Set counts in config
        cabin.passenger.first.count = counts.get("First", 0)
        cabin.passenger.business.count = counts.get("Business", 0)
        cabin.passenger.premium.count = counts.get("Premium", 0)
        cabin.passenger.economy.count = counts.get("Economy", 0)

        # Update requirements passenger count
        req.num_passengers = int(sum(counts.values()))
