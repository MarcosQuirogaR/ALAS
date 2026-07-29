# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Passenger cabin layout -- realistic interior generation.

Given the built fuselage geometry and a class mix, this engine generates a
physically plausible passenger cabin: seats packed by class (with seats-abreast
derived from the local cabin floor width), galleys and lavatories at class
boundaries, and **CS-25/FAR-25.807-compliant emergency exits** sized to the
passenger count. Checked baggage is routed to the lower-deck holds and balanced
to follow the seating distribution. Double-deck bodies (A380) lay out a main and
an upper deck.

Returns the common :class:`alas.physics.payload.PayloadLayout`.

Standards used (consolidated in ``payload_processed.md``, the project's
FAR/CS-25 cabin & payload regulatory reference):
* Exit capacity, minimum door width/height per type -- FAR/CS-25.807(g)
  (Matrix C): Type A 110, B 75, C 55, I 45, II 40, III 35, IV 9 per side.
* Main-aisle width -- FAR/CS-25.815 (Matrix A): >= 0.30 m for <= 19 pax,
  >= 0.51 m (20-in upper-body clearance) for >= 20 pax.
* Seats-abreast per aisle -- FAR/CS-25.817: no more than 3 seats between any
  passenger and an aisle (max 6 abreast single-aisle, 12 twin-aisle).
* Occupant masses -- EASA 2022 survey / FAA AC 120-27F (Matrix E): 84 kg
  seated occupant + carry-on, 16 kg checked bag routed to the holds.
* Checked bags & belly cargo are containerised into real lower-deck ULD
  positions (IATA ULD database + Matrix D envelope fit check) instead of
  abstract point masses, so the holds fill the way real airline load plans do.
* Seat geometry by class and provisioning ratios -- Stanford "Cabin Layout and
  Fuselage Geometry"; ~1 lavatory per 45 pax, ~1 galley per 100 pax.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Dict, List, Tuple

from .payload import CabinGeometry, DeckItem, PayloadLayout


@dataclass(frozen=True)
class ExitSpec:
    """One FAR/CS-25.807 emergency-exit class (capacity + minimum cutout)."""

    capacity_per_side: int  # max seats per exit, per fuselage side (25.807(g))
    width_m: float  # minimum door width (fuselage x-direction in plan)
    height_m: float  # minimum door height


# FAR/CS-25.807 exit classification (payload_processed.md Matrix C).
EXIT_TYPES: Dict[str, ExitSpec] = {
    "A": ExitSpec(110, 1.07, 1.83),
    "B": ExitSpec(75, 0.81, 1.83),
    "C": ExitSpec(55, 0.76, 1.22),
    "I": ExitSpec(45, 0.61, 1.22),
    "II": ExitSpec(40, 0.51, 1.12),
    "III": ExitSpec(35, 0.51, 0.91),
    "IV": ExitSpec(9, 0.48, 0.66),
}

# Maximum passenger seats permitted per exit of each type, per side of the
# fuselage (FAR/CS-25.807(g)). Kept as a plain dict for existing consumers.
EXIT_CAPACITY_PER_SIDE = {k: v.capacity_per_side for k, v in EXIT_TYPES.items()}

_MONUMENT_LEN = 0.95  # x-length consumed by a galley/lavatory bay [m]
_SEAT_BOX_H = 1.25  # drawn seat-row height [m]

# FAR/CS-25.815 minimum main-aisle widths (payload_processed.md Matrix A).
# The 20-in upper clearance governs at seat/armrest level, so it is the one
# used for the seats-abreast floor-width budget.
_AISLE_W_SMALL = 0.30  # <= 19 passengers
_AISLE_W_LARGE = 0.51  # >= 20 passengers


def required_aisle_width(n_pax: int) -> float:
    """FAR/CS-25.815 minimum main-aisle width for a given passenger count."""
    return _AISLE_W_SMALL if n_pax <= 19 else _AISLE_W_LARGE


def resolve_aisle_width(pax_cfg, n_pax: int) -> float:
    """The aisle width to lay out with: explicit config override, else 25.815."""
    w = float(getattr(pax_cfg, "aisle_width_m", 0.0) or 0.0)
    return w if w > 0.0 else required_aisle_width(n_pax)


def _select_exit_type(diameter_m: float) -> str:
    """Pick a representative exit type from the fuselage size."""
    if diameter_m >= 5.0:
        return "A"  # widebody main-deck door
    if diameter_m >= 3.6:
        return "C"  # narrow/widebody floor-level
    return "III"  # small narrowbody overwing


def _cabin_deck_segments(cg_geom: CabinGeometry):
    """Longitudinal (deck, x0, x1) segments available for seating/exits.

    Shared by :func:`build_passenger_layout` and
    :func:`alas.physics.payload.simulate_passenger_counts` so both use
    the identical notion of "available cabin length" per deck.
    """
    tail = cg_geom.tailcone_len
    segments = []
    for deck in cg_geom.passenger_decks:
        if deck.name == "upper":
            x0 = cg_geom.cabin_start_x + 3.0
            x1 = cg_geom.cabin_end_x
        else:
            x0 = cg_geom.cabin_start_x + 0.5
            x1 = cg_geom.cabin_end_x + 0.25 * tail
        segments.append((deck, x0, x1))
    return segments


def service_reserve_len(deck_length: float, mix: Dict[str, float]) -> float:
    """Longitudinal cabin floor [m] reserved for galleys/lavatories/closets/
    crew-rest, *beyond* the inter-class monument bays, for one deck segment.

    This is what makes a premium multi-class cabin genuinely less dense than an
    all-economy one, rather than economy simply floor-filling whatever the
    premium cabins don't use. It scales two ways:

    * with cabin length (a longer cabin carries proportionally more galley
      carts and lavatories -- roughly the CS-25 provisioning ratio), and
    * far more steeply with premium content: first/business cabins carry
      dedicated galleys, coat closets, and (on long-haul) crew-rest bunks that
      an economy-only cabin does not, which is the dominant reason a real
      787-9 3-class seats ~222 while its all-economy layout seats ~410 in the
      same shell.

    Constants calibrated so the shipped presets reproduce published seat
    counts (Ryanair ~410, Iberia 3-class ~222, Emirates premium-heavy ~250)
    on the B787-9.
    """
    premium_share = float(mix.get("First", 0.0)) + float(mix.get("Business", 0.0))
    return max(0.0, deck_length) * (0.045 + 0.45 * premium_share)


def max_certifiable_capacity(cg_geom: CabinGeometry, pax_cfg) -> Dict[str, int]:
    """Maximum CS-25/FAR-25.807-legal passenger count, per deck and total.

    Real aircraft don't scale exit count (and therefore certified capacity)
    with however much seating floor space happens to be available -- they
    have a small, fixed number of realistically-installable door-pair
    locations per deck (structural/practical spacing constraints), and that
    is what actually caps how many passengers can be carried, independent of
    whether more seats would physically fit. Without this cap, an
    unconstrained max-density layout would fill the entire cabin floor
    regardless of exits (e.g. an A380 single-class layout would seat 1400+,
    far past the real aircraft's 853-passenger certified maximum) -- this is
    the "Type A doors spammed left and right" symptom that appears if
    `n_pairs` is derived FROM an already-decided (unrealistic) passenger
    count, rather than the other way around.

    Each deck's exit-pair count is capped at
    ``floor(deck_length / pax_cfg.min_exit_pair_spacing_m)`` (minimum 1),
    further capped at 6 pairs (no real aircraft has more per deck). Returns
    ``{"main": n, "upper": n, ...}`` (per deck present) plus ``"total"``.
    """
    exit_type = _select_exit_type(cg_geom.diameter_m)
    cap_per_side = EXIT_CAPACITY_PER_SIDE[exit_type]
    if exit_type == "A":
        # Only the largest (widebody floor-level) exit type needs derating:
        # calibrated against real published exit-limit capacities, Type A's
        # nominal FAR/CS-25.807(g) rating (110/side) is far above what real
        # evacuation demonstrations achieve once several such doors interact
        # (cross-aisle congestion over long widebody aisles) -- e.g. the
        # 787-9 (4 Type-A pairs) has an exit limit of 420, not the naive
        # 4*110*2=880. Smaller exit types (fewer, smaller doors, shorter
        # aisles) were found to track close to their nominal rating in
        # practice (e.g. a 2-pair Type-C narrowbody), so are left undiscounted.
        cap_per_side *= max(pax_cfg.exit_capacity_realism_factor, 0.01)
    spacing = max(pax_cfg.min_exit_pair_spacing_m, 1.0)

    result: Dict[str, int] = {}
    total = 0
    for deck, x0, x1 in _cabin_deck_segments(cg_geom):
        deck_len = max(0.0, x1 - x0)
        n_pairs = max(1, min(6, int(deck_len // spacing)))
        deck_cap = int(n_pairs * cap_per_side * 2)
        result[deck.name] = deck_cap
        total += deck_cap
    result["total"] = total
    return result


def _abreast_and_aisles(cls, deck, cg_geom: CabinGeometry, aisle_w: float, x: float):
    """(seats-abreast, aisle count) for a class at station x.

    Applies FAR/CS-25.817: no more than three seats may separate any passenger
    from an aisle, so a single aisle allows at most 6 abreast (3-3) and two
    aisles at most 12 (3-6-3). The layout takes whichever legal option seats
    more passengers in the local usable floor width, preferring one aisle on
    ties (a second aisle spends floor width without adding seats).
    """
    seat_w = max(cls.width_m, 0.30)
    if cls.abreast > 0:
        return cls.abreast, (1 if cls.abreast <= 6 else 2)
    usable = cg_geom.usable_width(deck, x)
    n_single = min(int((usable - aisle_w) // seat_w), 6)
    n_twin = min(int((usable - 2.0 * aisle_w) // seat_w), 12)
    if n_twin > n_single:
        return max(1, n_twin), 2
    return max(1, n_single), 1


def _abreast(cls, deck, cg_geom: CabinGeometry, aisle_w: float, x: float) -> int:
    """Seats-abreast for a class at station x (auto from floor width if cls.abreast=0)."""
    return _abreast_and_aisles(cls, deck, cg_geom, aisle_w, x)[0]


def _seat_blocks(n: int, n_aisles: int) -> List[int]:
    """Split ``n`` seats abreast into lateral blocks separated by aisles.

    Single aisle: near-even split (3-3, 3-2, ...). Twin aisle: outboard blocks
    of up to 3 (25.817's window-side limit) with the remainder in the centre
    block (3-4-3, 2-3-2, ...).
    """
    if n <= 1:
        return [n]
    if n_aisles <= 1:
        return [math.ceil(n / 2), n // 2]
    out = 3 if n >= 8 else max(1, (n - 1) // 3)
    center = n - 2 * out
    if center < 1:
        return [math.ceil(n / 2), n // 2]
    return [out, center, out]


def _monument_fill_order(n_bays: int) -> List[int]:
    """Bay visiting order for round-robin galley/lav/exit placement.

    Real LOPAs anchor monuments and emergency exits at the very front and
    very back of the cabin first, and only add mid-cabin ones (at door
    stations) once more are needed than the two end bays provide -- e.g. a
    single-pair narrowbody has its exits at the front and rear only, while a
    widebody with several pairs adds mid-cabin ones spread between. A plain
    front-to-back fill instead piles every item into however many bays are
    needed starting from the front, leaving the rear of the cabin bare
    whenever the galley/lav/exit count is lower than the total bay count.
    """
    order: List[int] = []
    lo, hi = 0, n_bays - 1
    while lo < hi:
        order.append(lo)
        order.append(hi)
        lo += 1
        hi -= 1
    if lo == hi:
        order.append(lo)
    return order


def _spread_bay_indices(n_items: int, n_bays: int) -> List[int]:
    """``n_items`` bay indices, evenly spread front-to-rear across ``n_bays``.

    Used for emergency exits, where the exact count needed (``n_pairs``) is
    usually <= the number of monument bays and every one of them should be
    genuinely well spread (anchoring front and rear, and landing on the true
    middle bay for a middling count) -- unlike :func:`_monument_fill_order`'s
    fixed front/rear/near-front/... traversal, which is meant to be *cycled*
    for galley/lav overflow and so drifts toward the front for any partial
    prefix instead of spacing that prefix itself evenly.
    """
    if n_bays <= 0 or n_items <= 0:
        return []
    n_items = min(n_items, n_bays)
    if n_items == 1:
        return [0]
    out: List[int] = []
    for i in range(n_items):
        idx = round(i * (n_bays - 1) / (n_items - 1))
        while idx in out and idx < n_bays - 1:
            idx += 1
        out.append(idx)
    return out


def _stack_y(bay: Dict, side: str, item_width: float) -> Tuple[float, float]:
    """(y-offset magnitude, drawn width) for the next monument packed inward
    from the cabin wall on one side of ``bay``.

    Successive items of the same side pack back-to-back from the wall (the
    first item's outer edge flush with the usable width, matching the
    previous fixed-0.45 wall margin for a typical lav-width item). Once a bay
    holds more monuments than its half-width can fit at full size -- common
    once counts exceed the number of monument bays, see
    :func:`_monument_fill_order` -- the item is *narrowed* to whatever room
    is actually left rather than kept at full width and repositioned: full
    width with only a reposition would either cross the centreline into the
    opposite side's territory, or (if merely clamped at the centreline)
    overlap the previous item stacked on the same side.
    """
    half = bay["width"] / 2
    depth_key = f"_{side}_depth"
    depth = min(bay.get(depth_key, 0.0), half)  # never over-consume past the centreline
    available = half - depth
    drawn_width = max(0.0, min(item_width, available))
    bay[depth_key] = depth + drawn_width
    outer_edge = half - depth
    return outer_edge - drawn_width / 2, drawn_width


def build_passenger_layout(
    cg_geom: CabinGeometry, pax_cfg, req, oew: float = 0.0, x_oew: float = 0.0
) -> PayloadLayout:
    """Build the passenger :class:`PayloadLayout` for the given geometry/config."""
    # Class list -- fall back to a single economy cabin sized to num_passengers.
    classes = pax_cfg.classes()
    if not classes:
        econ = pax_cfg.economy
        single = type(econ)(
            count=int(req.num_passengers),
            abreast=econ.abreast,
            pitch_m=econ.pitch_m,
            width_m=econ.width_m,
            mass_per_pax_kg=econ.mass_per_pax_kg,
        )
        classes = [("Economy", single)]

    total_pax = sum(c.count for _n, c in classes)
    remaining = {name: c.count for name, c in classes}
    class_order = [name for name, _c in classes]
    class_map = {name: c for name, c in classes}

    # Every deck's pitch is stretched (never compressed below the configured/
    # regulatory minimum) so the seated block spans the *entire* available
    # floor length, guaranteeing close to 100% floor utilization regardless
    # of whether seating was limited by the CS-25.807 exit cap or by the
    # configured seat count -- matching how a real high-density LOPA is
    # actually built: seats spread evenly across the whole cabin up to
    # whichever limit binds, never bunched up front with a bare patch of
    # floor at the back. Applied uniformly across single- and multi-class
    # cabins alike -- a multi-class preset's lower seat count is not spare
    # cabin length to preserve, so maximizing floor utilization is the
    # explicit priority here and any resulting baseline CG shift is
    # recalibrated per-preset separately. Any leftover block position is
    # handled below rather than here, regardless of whether pitch-stretch
    # leaves any leftover to position.

    # FAR/CS-25.815 aisle width (auto from pax count unless overridden).
    aisle_w = resolve_aisle_width(pax_cfg, total_pax)

    items: List[DeckItem] = []
    bays: List[Dict] = []  # monument bay anchors: {x, deck, width}
    class_seat_count: Dict[str, int] = {n: 0 for n in class_order}
    deck_util: Dict[str, float] = {}  # per-deck floor utilization [0..1]
    max_abreast = 0
    max_aisles = 1

    # Maximum CS-25/FAR-25.807-legal capacity per deck -- caps seating even
    # when floor space remains (see max_certifiable_capacity's docstring for
    # why this matters: real aircraft are exit-limited, not floor-limited).
    deck_caps = max_certifiable_capacity(cg_geom, pax_cfg)
    deck_seated: Dict[str, int] = {d.name: 0 for d in cg_geom.passenger_decks}

    # Deck segments to fill, in order (main first, then upper on double-deckers).
    segments = _cabin_deck_segments(cg_geom)

    ci = 0  # index into class_order
    for deck, seg_x0, seg_x1 in segments:
        deck_cap = deck_caps.get(deck.name, deck_caps["total"])

        # Simulate to find the total seating length on this segment
        sim_remaining = remaining.copy()
        sim_ci = ci
        sim_x = seg_x0 + _MONUMENT_LEN
        sim_len = 0.0
        sim_seated = 0
        while (
            sim_ci < len(class_order)
            and sim_x < (seg_x1 - _MONUMENT_LEN)
            and sim_seated < deck_cap
        ):
            sim_name = class_order[sim_ci]
            sim_cls = class_map[sim_name]
            if sim_remaining[sim_name] <= 0:
                sim_ci += 1
                if sim_ci < len(class_order):
                    sim_x += _MONUMENT_LEN
                continue
            sim_ab = _abreast(sim_cls, deck, cg_geom, aisle_w, sim_x)
            sim_seats = min(sim_ab, sim_remaining[sim_name], deck_cap - sim_seated)
            sim_pitch = max(sim_cls.pitch_m, 0.30)
            sim_len += sim_pitch
            sim_remaining[sim_name] -= sim_seats
            sim_seated += sim_seats
            sim_x += sim_pitch

        # Mid-cabin monument bays: real LOPAs cluster galleys/lavs at the
        # door stations, not just the cabin ends. Estimate this deck's exit
        # pair count from the simulated seated count and reserve one bay
        # between each adjacent door pair; their floor length is charged
        # against seating (simulate_passenger_counts budgets the same bays,
        # keeping preset auto-sizing consistent with this layout).
        est_type = _select_exit_type(cg_geom.diameter_m)
        est_cap = EXIT_TYPES[est_type].capacity_per_side
        n_pairs_est = (
            max(2 if sim_seated > 110 else 1, math.ceil(sim_seated / est_cap))
            if sim_seated
            else 1
        )
        n_mid = max(0, n_pairs_est - 1)

        # Available seating length & floor utilization for this deck
        l_avail = seg_x1 - seg_x0 - 2 * _MONUMENT_LEN
        seating_room = max(0.0, l_avail - n_mid * _MONUMENT_LEN)
        # Stretch every row's pitch by the same factor so the seated block
        # exactly spans seating_room, never below 1.0 (never compress a row
        # tighter than its configured pitch -- if the configured count
        # already needs the whole floor or more, this is a no-op and the
        # existing exit-cap/floor-exhaustion truncation below still applies
        # unchanged). Applied per-class-mix-uniformly (see note above
        # pax_cfg.classes()), so a multi-class cabin's relative class-length
        # proportions are preserved -- only the shared pitch multiplier grows,
        # not any one class's share of the floor.
        pitch_stretch = 1.0
        if sim_len > 0:
            pitch_stretch = max(1.0, seating_room / sim_len)
        block_len = sim_len * pitch_stretch + n_mid * _MONUMENT_LEN
        deck_util[deck.name] = min(1.0, block_len / l_avail) if l_avail > 0 else 0.0
        # Seating always starts right after the front monument bay -- a real
        # LOPA never leaves a stretch of empty floor between the forward
        # bulkhead and the first row just to "center" a shorter block. Any
        # leftover cabin length (block_len < l_avail, common for a
        # lower-density multi-class layout that isn't floor- or exit-limited)
        # is left AFT of the last class instead, near the rear bulkhead/
        # tailcone -- physically what a real aircraft's spare cabin length
        # would be, not a gap ahead of business class. Centering the block
        # instead would put that leftover half-forward, dragging the payload
        # CG forward of where a real aircraft's spare cabin length sits.
        x = seg_x0 + _MONUMENT_LEN

        # Evenly spaced mid-bay stations across the seating block.
        block_start = x
        mid_stations = [
            block_start + block_len * (i / n_pairs_est) for i in range(1, n_mid + 1)
        ]

        # front-of-deck monument bay
        bays.append(
            {
                "x": seg_x0 + _MONUMENT_LEN / 2,
                "deck": deck.name,
                "width": cg_geom.usable_width(deck, seg_x0),
            }
        )

        while (
            ci < len(class_order)
            and x < (seg_x1 - _MONUMENT_LEN)
            and deck_seated[deck.name] < deck_cap
        ):
            # Insert the next mid-cabin monument bay once we reach its station.
            if mid_stations and x >= mid_stations[0] - 1e-6:
                bays.append(
                    {
                        "x": x + _MONUMENT_LEN / 2,
                        "deck": deck.name,
                        "width": cg_geom.usable_width(deck, x),
                    }
                )
                x += _MONUMENT_LEN
                mid_stations.pop(0)
                continue
            name = class_order[ci]
            cls = class_map[name]
            if remaining[name] <= 0:
                ci += 1
                # monument bay between classes
                if ci < len(class_order):
                    bays.append(
                        {
                            "x": x + _MONUMENT_LEN / 2,
                            "deck": deck.name,
                            "width": cg_geom.usable_width(deck, x),
                        }
                    )
                    x += _MONUMENT_LEN
                continue

            ab, n_aisles = _abreast_and_aisles(cls, deck, cg_geom, aisle_w, x)
            seats_row = min(ab, remaining[name], deck_cap - deck_seated[deck.name])
            usable = cg_geom.usable_width(deck, x)
            # guard against 0 pitch (infinite loop); stretched (single-class
            # only, see pitch_stretch above) so the block spans the full
            # available floor length instead of leaving it bare at the rear.
            pitch = max(cls.pitch_m, 0.30) * pitch_stretch
            seat_w = max(cls.width_m, 0.30)
            blocks = _seat_blocks(ab, n_aisles)
            max_abreast = max(max_abreast, ab)
            max_aisles = max(max_aisles, n_aisles)
            items.append(
                DeckItem(
                    kind="seat_row",
                    deck=deck.name,
                    x=x + pitch / 2,
                    y=0.0,
                    z=cg_geom.item_z(deck, x, _SEAT_BOX_H),
                    length=pitch,
                    width=usable,
                    height=cg_geom.clamp_height(deck, x, _SEAT_BOX_H),
                    mass=seats_row * cls.mass_per_pax_kg,
                    label=name,
                    meta={
                        "cls": name,
                        "abreast": ab,
                        "filled": seats_row,
                        "deck": deck.name,
                        "aisles": n_aisles,
                        "blocks": blocks,
                        "seat_w": seat_w,
                        "aisle_w": aisle_w,
                    },
                )
            )
            remaining[name] -= seats_row
            class_seat_count[name] += seats_row
            deck_seated[deck.name] += seats_row
            x += pitch

        # rear-of-deck monument bay at the back of the segment
        bays.append(
            {
                "x": seg_x1 - _MONUMENT_LEN / 2,
                "deck": deck.name,
                "width": cg_geom.usable_width(deck, seg_x1 - _MONUMENT_LEN),
            }
        )

    seated = sum(class_seat_count.values())

    # --- Galleys & lavatories (auto provisioning) ---
    lav_count = pax_cfg.lavatory_count or max(1, math.ceil(total_pax / 45))
    galley_count = pax_cfg.galley_count or max(1, math.ceil(total_pax / 100) + 1)
    if bays:
        # Monuments are distributed across ALL bays (cabin ends, class
        # boundaries, and the mid-cabin door-station bays) the way real
        # LOPAs do. A bay that receives more than one galley/lav stacks them
        # laterally inward from the wall (a galley complex), never at the
        # same coordinates -- galleys occupy the +y side, lavs the -y side.
        fill_order = _monument_fill_order(len(bays))
        for i in range(galley_count):
            b = bays[fill_order[i % len(fill_order)]]
            deck = next(d for d in cg_geom.passenger_decks if d.name == b["deck"])
            y, w = _stack_y(b, "galley", 0.85)
            items.append(
                DeckItem(
                    kind="galley",
                    deck=b["deck"],
                    x=b["x"],
                    y=y,
                    z=cg_geom.item_z(deck, b["x"], _SEAT_BOX_H),
                    length=_MONUMENT_LEN,
                    width=w,
                    height=cg_geom.clamp_height(deck, b["x"], _SEAT_BOX_H),
                    mass=0.0,
                    label="Galley",
                    meta={},
                )
            )
        for i in range(lav_count):
            b = bays[fill_order[i % len(fill_order)]]
            deck = next(d for d in cg_geom.passenger_decks if d.name == b["deck"])
            y, w = _stack_y(b, "lav", 0.90)
            items.append(
                DeckItem(
                    kind="lav",
                    deck=b["deck"],
                    x=b["x"],
                    y=-y,
                    z=cg_geom.item_z(deck, b["x"], _SEAT_BOX_H),
                    length=_MONUMENT_LEN,
                    width=w,
                    height=cg_geom.clamp_height(deck, b["x"], _SEAT_BOX_H),
                    mass=0.0,
                    label="Lav",
                    meta={},
                )
            )

    # --- Emergency exits (FAR/CS-25.807) ---
    # One exit-pair set per PASSENGER deck (not just main -- a double-decker's
    # upper deck needs its own evacuation exits too), sized from how many
    # passengers actually got seated on that deck (post capacity-cap above),
    # never from the raw/uncapped total_pax.
    exit_type = _select_exit_type(cg_geom.diameter_m)
    spec = EXIT_TYPES[exit_type]
    cap = spec.capacity_per_side
    total_exit_pairs = 0
    for deck, seg_x0, seg_x1 in segments:
        deck_pax = deck_seated.get(deck.name, 0)
        if deck_pax <= 0:
            continue
        min_pairs = 2 if deck_pax > 110 else 1
        n_pairs = max(min_pairs, math.ceil(deck_pax / cap))

        # Exit rows sit at this deck's monument-bay stations, evenly spread
        # front-to-rear (see _spread_bay_indices) so every exit lands on the
        # _MONUMENT_LEN gap already carved out of the seating there, giving
        # it real clearance, instead of a second, independently-computed
        # position list that would drift out of alignment with where the
        # seat rows actually leave room.
        deck_bays = sorted(
            (b for b in bays if b["deck"] == deck.name), key=lambda b: b["x"]
        )
        exit_xs = [
            deck_bays[i]["x"] for i in _spread_bay_indices(n_pairs, len(deck_bays))
        ]
        if n_pairs > len(exit_xs):
            extra = n_pairs - len(exit_xs)
            ex0 = seg_x0 - _MONUMENT_LEN + 1.0
            ex1 = seg_x1 + _MONUMENT_LEN - 1.0
            exit_xs += [ex0 + (ex1 - ex0) * (i + 0.5) / extra for i in range(extra)]

        for xe in exit_xs:
            half = cg_geom.usable_width(deck, xe) / 2 + cg_geom.wall
            for side in (-1, 1):
                # Door cutout drawn at its FAR/CS-25.807 minimum dimensions
                # (Matrix C): length = door width along x, height = door height.
                items.append(
                    DeckItem(
                        kind="exit",
                        deck=deck.name,
                        x=xe,
                        y=side * half,
                        z=cg_geom.item_z(deck, xe, spec.height_m),
                        length=spec.width_m,
                        width=0.25,
                        height=cg_geom.clamp_height(deck, xe, spec.height_m),
                        mass=0.0,
                        label=f"Type {exit_type}",
                        meta={
                            "type": exit_type,
                            "door_w": spec.width_m,
                            "door_h": spec.height_m,
                        },
                    )
                )
        total_exit_pairs += n_pairs

    # --- Mass & CG ---
    seat_items = [it for it in items if it.kind == "seat_row"]
    seat_mass = sum(it.mass for it in seat_items)
    seat_cg = (
        (sum(it.mass * it.x for it in seat_items) / seat_mass)
        if seat_mass > 0
        else 0.5 * (cg_geom.cabin_start_x + cg_geom.cabin_end_x)
    )

    # Checked baggage + belly cargo -> real lower-deck ULD positions.
    #
    # Bags are containerised into the same ULD slot grid the cargo loader uses
    # (LD3, degrading to LD3-45/bulk where the hold cross-section fails the
    # Matrix D fit check), and the load is trimmed toward the seating CG so
    # the payload CG stays driven by the seating distribution (airlines trim
    # bags). Any remaining hold capacity can carry revenue belly freight
    # (`belly_cargo_kg`) -- real passenger aircraft don't fly empty bellies.
    bag_mass = seated * pax_cfg.checked_bag_mass_kg
    belly_explicit = max(0.0, float(getattr(pax_cfg, "belly_cargo_kg", 0.0)))
    # Fill the belly with revenue freight up to the airframe's max STRUCTURAL
    # payload (MZFW - OEW) -- on top of passengers + checked bags -- so the
    # payload, and therefore the residual fuel (MTOW - OEW - payload), matches
    # the real aircraft's max-payload design point. The structural cap is
    # essential: a widebody belly can volumetrically hold far more than the
    # airframe may structurally carry, so filling to geometric ULD capacity
    # overshoots wildly (a 787 belly fits ~94 t of ULDs vs a ~53 t structural
    # max payload). An explicit belly_cargo_kg still wins if the user set one
    # larger than the auto-fill (e.g. a deliberate over-load study).
    max_struct_payload = float(getattr(req, "max_structural_payload_kg", 0.0) or 0.0)
    belly_to_max = (
        max(0.0, max_struct_payload - seat_mass - bag_mass)
        if max_struct_payload > 0
        else 0.0
    )
    belly_cargo = max(belly_explicit, belly_to_max)
    hold_capacity = 0.0
    hold_placed = 0.0
    n_hold_ulds = 0
    if bag_mass + belly_cargo > 0:
        from ..config.cabin_config import CargoDeckConfig
        from .cargo_loader import CargoLoadManager

        mgr = CargoLoadManager(cg_geom, CargoDeckConfig(use_main_deck=False))
        hold_capacity = mgr.total_capacity()
        belly_cargo = min(belly_cargo, max(0.0, hold_capacity - bag_mass))
        hold_mass = min(bag_mass + belly_cargo, hold_capacity)
        mgr.solve(hold_mass, seat_cg, lambda s: abs(s.x - seat_cg), fill_full=True)
        for s in mgr.slots:
            if s.payload <= 1.0:
                continue
            low = cg_geom.lower_deck
            items.append(
                DeckItem(
                    kind="bag",
                    deck="lower",
                    x=s.x,
                    y=s.y,
                    z=cg_geom.item_z(low, s.x, s.uld.height),
                    length=s.uld.length,
                    width=s.uld.width,
                    height=cg_geom.clamp_height(low, s.x, s.uld.height),
                    mass=s.total_weight,
                    label=f"{s.uld.code} {int(s.total_weight)} kg",
                    meta={
                        "bags": True,
                        "uld": s.uld.code,
                        "color": s.uld.color,
                        "fill": s.payload / s.max_net if s.max_net > 0 else 1.0,
                    },
                )
            )
            n_hold_ulds += 1
        hold_placed, _hold_cg, _n = mgr.mass_props()
        # Overflow guard: bag/cargo mass beyond the holds' ULD capacity is
        # still carried (it drives payload mass, fuel and CG) as a loose
        # bulk block at the aft hold, like the pre-ULD model did.
        leftover = (bag_mass + belly_cargo) - hold_placed
        if leftover > 1.0:
            low = cg_geom.lower_deck
            xx = cg_geom.cabin_end_x - 1.5
            items.append(
                DeckItem(
                    kind="bag",
                    deck="lower",
                    x=xx,
                    y=0.0,
                    z=cg_geom.item_z(low, xx, 1.4),
                    length=2.0,
                    width=cg_geom.usable_width(low, xx),
                    height=cg_geom.clamp_height(low, xx, 1.4),
                    mass=leftover,
                    label="Bulk overflow",
                    meta={"bags": True},
                )
            )
            hold_placed += leftover

    layout = PayloadLayout(mode="passenger", items=items)
    layout.recompute_cg()
    layout.summary = {
        "mode": "passenger",
        "total_pax": total_pax,
        "seated_pax": seated,
        "classes": {n: class_seat_count[n] for n in class_order},
        "lavatories": lav_count,
        "galleys": galley_count,
        "exit_type": exit_type,
        "exit_pairs": total_exit_pairs,
        "exit_capacity": total_exit_pairs * cap * 2,
        "max_certifiable_capacity": deck_caps["total"],
        "payload_t": layout.total_mass / 1000.0,
        "seat_mass_t": seat_mass / 1000.0,
        "bag_mass_t": bag_mass / 1000.0,
        "belly_cargo_t": belly_cargo / 1000.0,
        "hold_capacity_t": hold_capacity / 1000.0,
        "hold_used_t": hold_placed / 1000.0,
        "hold_ulds": n_hold_ulds,
        "aisle_width_m": aisle_w,
        "max_abreast": max_abreast,
        "n_aisles": max_aisles,
        "deck_utilization": {k: round(100.0 * v, 1) for k, v in deck_util.items()},
        "cg_pct_mac": cg_geom.x_to_pct_mac(layout.cg_x)
        if layout.total_mass > 0
        else 0.0,
        "double_deck": cg_geom.is_double_deck,
    }
    return layout
