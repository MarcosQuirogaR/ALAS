# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Cargo loading -- ULD-based main/lower-deck load distribution with a CG solver.

Ported and generalised from ``cargo_manager_example.py``: it builds cargo slots
on the main deck and the forward/aft lower-deck holds from the *actual* fuselage
geometry, fills them with ULDs from a small database, and distributes the
payload to trim the aircraft toward a target centre of gravity using a selectable
priority (target-CG, fewest pallets, door proximity, or uniform).

Returns the common :class:`alas.physics.payload.PayloadLayout` consumed by
the mass analysis, the preview, and the report.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List

from .payload import CabinGeometry, DeckItem, PayloadLayout


# ---------------------------------------------------------------------------
# ULD database (Unit Load Devices)
#
# Dimensions, max gross weights, tare weights and internal volumes follow the
# IATA ULD tables consolidated in payload_processed.md §6.2 (Matrix D). Base
# footprints: half-width containers (LD1/LD2/LD3 family) share the 60.4x61.5 in
# AKE base; 3.18 m entries are the double-width / pallet class (LD6/LD8/LD11,
# P1P/P6P pallets); M-1 is the 20-ft-class main-deck freighter box.
# ---------------------------------------------------------------------------
@dataclass
class ULDType:
    code: str
    name: str
    length: float  # along x [m]
    width: float  # along y [m]
    height: float  # along z [m]
    max_gross_weight: float
    tare_weight: float
    color: str
    volume_m3: float = 0.0  # internal volume (IATA nominal)

    @property
    def max_net(self) -> float:
        return self.max_gross_weight - self.tare_weight


ULD_DATABASE = {
    "LD1": ULDType("AKC", "LD1 Container", 1.56, 1.53, 1.63, 1588, 120, "#c0392b", 5.0),
    "LD2": ULDType("DPE", "LD2 Container", 1.56, 1.19, 1.63, 1225, 92, "#d35400", 3.5),
    "LD3": ULDType("AKE", "LD3 Container", 1.56, 1.53, 1.63, 1588, 82, "#e74c3c", 4.5),
    # LD3-45 (AKH): the reduced-height LD3 used in narrowbody (A320-family)
    # lower holds -- supplemental to the payload_processed.md table, included
    # as the fit-check fallback so narrowbodies still containerise bags.
    "LD3-45": ULDType(
        "AKH", "LD3-45 Container", 1.56, 1.53, 1.14, 1134, 82, "#e57373", 3.6
    ),
    "LD6": ULDType("ALF", "LD6 Container", 3.18, 1.53, 1.63, 3175, 230, "#e67e22", 9.1),
    "LD8": ULDType("DQF", "LD8 Container", 3.18, 1.53, 1.63, 2450, 127, "#f39c12", 7.1),
    "LD11": ULDType(
        "ALP", "LD11 Container", 3.18, 1.53, 1.63, 3175, 185, "#f1c40f", 7.4
    ),
    "PAG": ULDType(
        "P1P", "LD7 Pallet (88x125)", 3.18, 2.24, 1.63, 4626, 110, "#2980b9", 10.5
    ),
    "PMC": ULDType(
        "P6P", "PMC Pallet (96x125)", 3.18, 2.44, 1.63, 6804, 120, "#3498db", 11.5
    ),
    "M1": ULDType(
        "AMA", "M-1 Main-Deck Box", 6.06, 2.44, 2.44, 11340, 1000, "#8e44ad", 33.7
    ),
    "BLK": ULDType("BLK", "Bulk Cargo", 1.50, 2.00, 1.50, 2000, 0, "#95a5a6", 3.0),
}

# Fit-check fallback chain for the lower holds (Matrix D interference check):
# if the configured container's rigid envelope can't fit the hold cross-section
# the loader degrades to the next smaller profile instead of silently clamping
# the container into the structure.
LOWER_HOLD_FALLBACKS = ("LD3-45", "BLK")


def _uld(code: str, fallback: str) -> ULDType:
    return ULD_DATABASE.get(code, ULD_DATABASE[fallback])


@dataclass
class CargoSlot:
    sid: str
    deck: str
    x: float
    y: float
    uld: ULDType
    payload: float = 0.0  # net cargo currently loaded [kg]

    @property
    def max_net(self) -> float:
        return self.uld.max_net

    @property
    def total_weight(self) -> float:
        return (self.payload + self.uld.tare_weight) if self.payload > 1.0 else 0.0


# ---------------------------------------------------------------------------
# Load manager
# ---------------------------------------------------------------------------
class CargoLoadManager:
    """Builds cargo slots from the fuselage geometry and solves the load."""

    def __init__(self, cg_geom: CabinGeometry, cfg):
        self.g = cg_geom
        self.cfg = cfg
        self.slots: List[CargoSlot] = []
        self._build_slots()

    # -- slot construction ---------------------------------------------------
    def _row(self, sid_prefix: str, deck, x: float, uld: ULDType) -> int:
        """Place a transverse row of ULDs across the deck at station x.

        Applies the Matrix D interference check (payload_processed.md §6.2):
        the ULD's rigid envelope must fit the local hold cross-section (width
        AND height); a station too narrow or too shallow gets no slots rather
        than a container clamped into the structure. Returns slots placed.
        """
        usable = self.g.usable_width(deck, x)
        if self.g.deck_height(deck, x) < uld.height:
            return 0
        n_across = int(usable // uld.width)
        if n_across <= 0:
            return 0
        n_across = min(n_across, 3)
        if n_across == 1:
            ys = [0.0]
        elif n_across == 2:
            ys = [-(uld.width / 2 + 0.05), (uld.width / 2 + 0.05)]
        else:
            ys = [-(uld.width + 0.05), 0.0, (uld.width + 0.05)]
        for i, y in enumerate(ys):
            self.slots.append(CargoSlot(f"{sid_prefix}{i + 1}", deck.name, x, y, uld))
        return len(ys)

    def _fill_lower_holds(self, uld: ULDType) -> int:
        """Generate slot rows in the fwd/aft lower holds; returns slots placed."""
        g = self.g
        low = g.lower_deck
        pitch = uld.length + 0.08
        wb_start, wb_end = g.wing_box_x_range()
        placed = 0

        # Forward hold
        x = g.cabin_start_x + 0.8 + uld.length / 2
        i = 0
        while x <= wb_start - uld.length / 2 and i < 40:
            placed += self._row(f"FWD-{i + 1}-", low, x, uld)
            x += pitch
            i += 1

        # Aft hold
        x = wb_end + uld.length / 2 + 0.2
        x_end = g.cabin_end_x - uld.length / 2
        i = 0
        while x <= x_end and i < 40:
            placed += self._row(f"AFT-{i + 1}-", low, x, uld)
            x += pitch
            i += 1
        return placed

    def _build_slots(self) -> None:
        g, cfg = self.g, self.cfg

        # --- MAIN DECK (freighter) ---
        if cfg.use_main_deck:
            md = next(d for d in g.passenger_decks if d.name == "main")
            uld = _uld(cfg.main_deck_uld, "PMC")
            pitch = uld.length + 0.20
            x = g.cabin_start_x + 1.5 + uld.length / 2
            x_end = g.cabin_end_x - uld.length / 2
            i = 0
            while x <= x_end and i < 60:
                self._row(f"MD-{i + 1}-", md, x, uld)
                x += pitch
                i += 1

        # --- LOWER-DECK HOLDS (fwd / aft of the wing box) ---
        # Try the configured container first; if its envelope fails the fit
        # check at every station (e.g. an LD3 in a narrowbody hold), degrade
        # through the fallback chain (LD3-45, then loose bulk).
        self.lower_uld = _uld(cfg.lower_deck_uld, "LD3")
        candidates = [self.lower_uld] + [
            ULD_DATABASE[c]
            for c in LOWER_HOLD_FALLBACKS
            if ULD_DATABASE[c].code != self.lower_uld.code
        ]
        for uld in candidates:
            if self._fill_lower_holds(uld) > 0:
                self.lower_uld = uld
                break

        # --- BULK ---
        low = g.lower_deck
        blk = ULD_DATABASE["BLK"]
        self.slots.append(CargoSlot("BULK", low.name, g.cabin_end_x - 0.8, 0.0, blk))

    # -- mass properties -----------------------------------------------------
    def mass_props(self):
        m_tot = mom = 0.0
        used = 0
        for s in self.slots:
            w = s.total_weight
            if w > 0:
                m_tot += w
                mom += w * s.x
                used += 1
        cg = mom / m_tot if m_tot > 0 else 0.0
        return m_tot, cg, used

    def total_capacity(self) -> float:
        return sum(s.max_net for s in self.slots)

    def clear(self) -> None:
        for s in self.slots:
            s.payload = 0.0

    # -- solver --------------------------------------------------------------
    def solve(
        self,
        target_mass: float,
        target_cg: float,
        priority_func: Callable[[CargoSlot], float],
        fill_full: bool = True,
    ) -> None:
        """Distribute ``target_mass`` of cargo, trimming toward ``target_cg``.

        ``priority_func`` ranks slots (smaller = loaded first). Mirrors the
        ``solve_optimized`` routine from ``cargo_manager_example.py``.
        """
        self.clear()
        if not self.slots:
            return
        sorted_slots = sorted(self.slots, key=priority_func)
        target_mass = min(target_mass, self.total_capacity())

        # 1. Initial fill
        if fill_full:
            remaining = target_mass
            for s in sorted_slots:
                if remaining <= 0:
                    break
                add = min(remaining, s.max_net)
                s.payload = add
                remaining -= add
        else:
            per_slot = target_mass / len(self.slots)
            for s in self.slots:
                s.payload = min(per_slot, s.max_net)

        # 2. CG trim loop (mass conservation + CG balancing)
        step = self.cfg.cg_trim_step_kg
        for _ in range(self.cfg.cg_trim_max_iterations):
            curr_m, curr_cg, _ = self.mass_props()
            # Keep total mass on target -- directionally: remove excess from
            # the heavy side / add shortfall on the light side (relative to
            # the CG error). The old priority-ordered removal could strip
            # exactly the slots the CG-shift step below had just filled
            # (whenever the far hold is *farther* from the target than the
            # near hold, e.g. a fwd hold across the wing box), cancelling
            # every shift and stalling the loop at a large CG error.
            diff = target_mass - curr_m
            if abs(diff) > 5.0:
                err_now = curr_cg - target_cg
                if diff > 0:
                    # add on the light side first (pulls CG toward target)
                    candidates = sorted(
                        self.slots, key=lambda s: s.x, reverse=(err_now < 0)
                    )
                else:
                    # remove from the heavy side first
                    candidates = sorted(
                        self.slots, key=lambda s: s.x, reverse=(err_now > 0)
                    )
                for s in candidates:
                    if abs(diff) < 1.0:
                        break
                    if diff > 0:
                        space = s.max_net - s.payload
                        if space > 0:
                            add = min(diff, space)
                            s.payload += add
                            diff -= add
                    elif s.payload > 0:
                        rem = min(-diff, s.payload)
                        s.payload -= rem
                        diff += rem

            err = curr_cg - target_cg
            if abs(err) < 0.05 and abs(target_mass - curr_m) < 10.0:
                break
            # shift load from the heavy side toward the light side
            if err > 0:
                src = [s for s in self.slots if s.x > curr_cg and s.payload > 0]
                dst = [s for s in self.slots if s.x < curr_cg and s.payload < s.max_net]
            else:
                src = [s for s in self.slots if s.x < curr_cg and s.payload > 0]
                dst = [s for s in self.slots if s.x > curr_cg and s.payload < s.max_net]
            if src and dst:
                s_src = sorted(src, key=priority_func, reverse=True)[0]
                s_dst = sorted(dst, key=priority_func)[0]
                amt = min(step, s_src.payload, s_dst.max_net - s_dst.payload)
                s_src.payload -= amt
                s_dst.payload += amt
            else:
                break


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------
def build_cargo_layout(
    cg_geom: CabinGeometry, cargo_cfg, req, oew: float = 0.0, x_oew: float = 0.0
) -> PayloadLayout:
    """Build the cargo :class:`PayloadLayout` for the given geometry and config."""
    mgr = CargoLoadManager(cg_geom, cargo_cfg)

    payload_target = float(req.cargo_payload_kg)

    # Target aircraft CG (x) from %MAC; auto = a conservative 25 % MAC.
    target_pct = (
        cargo_cfg.target_cg_pct_mac if cargo_cfg.target_cg_pct_mac > 0 else 25.0
    )
    target_cg_aircraft = cg_geom.pct_mac_to_x(target_pct)

    # Required *payload* CG so the whole aircraft sits at the target CG.
    if oew > 0 and payload_target > 0:
        req_pay_cg = (
            (oew + payload_target) * target_cg_aircraft - oew * x_oew
        ) / payload_target
    else:
        req_pay_cg = target_cg_aircraft

    # Door positions (auto unless overridden).
    main_door = cargo_cfg.main_door_x_m or (
        cg_geom.cabin_start_x + 0.55 * (cg_geom.cabin_end_x - cg_geom.cabin_start_x)
    )
    fwd_door = cargo_cfg.fwd_door_x_m or (cg_geom.cabin_start_x + 2.0)
    aft_door = cargo_cfg.aft_door_x_m or (cg_geom.cabin_end_x - 2.0)

    def door_priority(s: CargoSlot) -> float:
        if s.deck == "main":
            return abs(s.x - main_door)
        return min(abs(s.x - fwd_door), abs(s.x - aft_door))

    strategy = cargo_cfg.loading_strategy
    if strategy == "door_proximity":
        mgr.solve(payload_target, req_pay_cg, door_priority, fill_full=True)
    elif strategy == "uniform":
        mgr.solve(
            payload_target, req_pay_cg, lambda s: abs(s.x - req_pay_cg), fill_full=False
        )
    else:  # target_cg | min_pallets -- concentrate full ULDs near the required CG
        mgr.solve(
            payload_target, req_pay_cg, lambda s: abs(s.x - req_pay_cg), fill_full=True
        )

    # Convert loaded slots into DeckItems.
    items: List[DeckItem] = []
    n_main = n_lower = 0
    for s in mgr.slots:
        if s.payload <= 1.0:
            continue
        deck_spec = (
            next((d for d in cg_geom.passenger_decks if d.name == s.deck), None)
            if s.deck == "main"
            else cg_geom.lower_deck
        )
        if deck_spec is not None:
            z_item = cg_geom.item_z(deck_spec, s.x, s.uld.height)
            h_item = cg_geom.clamp_height(deck_spec, s.x, s.uld.height)
        else:
            z_item, h_item = s.uld.height / 2, s.uld.height
        fill = s.payload / s.max_net if s.max_net > 0 else 0.0
        items.append(
            DeckItem(
                kind="uld",
                deck=s.deck,
                x=s.x,
                y=s.y,
                z=z_item,
                length=s.uld.length,
                width=s.uld.width,
                height=h_item,
                mass=s.total_weight,
                label=f"{s.uld.code} {int(s.total_weight)}kg",
                meta={
                    "uld": s.uld.code,
                    "fill": fill,
                    "color": s.uld.color,
                    "net": s.payload,
                },
            )
        )
        if s.deck == "main":
            n_main += 1
        else:
            n_lower += 1

    capacity = mgr.total_capacity()
    layout = PayloadLayout(mode="cargo", items=items)
    layout.recompute_cg()
    layout.summary = {
        "mode": "cargo",
        "payload_t": layout.total_mass / 1000.0,
        "n_ulds": n_main + n_lower,
        "n_main_deck": n_main,
        "n_lower_deck": n_lower,
        "n_slots": len(mgr.slots),
        "capacity_t": capacity / 1000.0,
        "fill_pct": 100.0 * payload_target / capacity if capacity > 0 else 0.0,
        "volume_m3": sum(s.uld.volume_m3 for s in mgr.slots),
        "lower_uld": mgr.lower_uld.code if hasattr(mgr, "lower_uld") else "",
        "target_cg_pct_mac": target_pct,
        "achieved_cg_pct_mac": cg_geom.x_to_pct_mac(layout.cg_x)
        if layout.total_mass > 0
        else 0.0,
        "strategy": strategy,
    }
    return layout
