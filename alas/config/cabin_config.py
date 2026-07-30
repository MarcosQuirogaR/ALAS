# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Cabin & payload configuration -- passenger seating classes and cargo deck loading.

These objects drive the detailed payload layout (:mod:`alas.physics.payload`),
used by the baseline pass, the final analysis, the Cabin/Payload preview, AND
every optimizer-loop candidate evaluation (see ``optimization/objective.py``
and ``docs/architecture.md`` Sec 1): each call builds the real cabin/cargo
layout from these fields to override the lumped payload CG before checking
CG-envelope compliance. The one place these
settings do NOT reach is :mod:`alas.physics.mass`'s very first,
cheap-estimate pass (``cabin_payload_density_kg_m``, which sizes the
*occupied cabin length* used for a first-pass payload/systems CG guess before
the detailed layout above overrides it -- the lumped mass there is
``requirements.payload_kg``, itself derived from the seat counts these
objects hold).

Design note: passenger classes are modelled as four *fixed slots* (first /
business / premium / economy) rather than a variable-length list, so the
auto-generated settings form renders them with no
per-field UI code. A class with ``count == 0`` is simply absent. If every class
count is 0, the layout engine falls back to a single economy cabin sized to
``requirements.num_passengers`` (the "auto from total pax" behaviour).
"""

from __future__ import annotations

from dataclasses import dataclass, field


# Typical seat geometry per class (Stanford "Cabin Layout and Fuselage Geometry";
# Airline-seat survey). Pitch/width in metres; mass is occupant + carry-on, with
# checked baggage handled separately and routed to the lower-deck holds.
@dataclass
class SeatClassConfig:
    """One passenger cabin class block."""

    # Share of the usable cabin FLOOR LENGTH given to this class, in percent.
    # This is the primary way to describe a class mix: airlines think in "how
    # much of the cabin is business vs economy", and seat COUNT is an outcome
    # of that share combined with the class's own pitch/abreast and the
    # fuselage's real geometry -- not something you can pick independently. The
    # named presets already worked this way internally (a length-mix dict); this
    # exposes the same quantity. Shares are normalised across classes, so they
    # need not sum to exactly 100. Ignored when
    # ``PassengerCabinConfig.class_mix_mode == "count"``.
    share_pct: float = field(
        default=0.0,
        metadata={
            "min": 0.0,
            "max": 100.0,
            "label": "Share of cabin length [%]",
            "help": "Percentage of usable cabin floor length allocated to this class. Seat count is "
            "derived from it using this class's pitch/abreast and the real fuselage geometry. "
            "Shares are normalised, so they need not add up to exactly 100. Set the class to 0 "
            "to remove it. Only used when Class mix mode is 'percent'.",
        },
    )
    # Explicit seat count -- the fallback when you need an exact number rather
    # than a share (class_mix_mode == "count"). Read-only in percent mode,
    # where it is a derived output of the layout solve.
    count: int = field(
        default=0,
        metadata={
            "min": 0,
            "help": "Exact number of seats in this class (0 = class absent). Only editable when Class "
            "mix mode is 'count'; in 'percent' mode this is computed from the share above.",
            # Grey the field out in percent mode, where it's a derived output. The
            # sibling lookup is by field NAME within the same form level, and
            # class_mix_mode lives one level up on PassengerCabinConfig, so the
            # frontend resolves it against the nearest enclosing form values --
            # see DynamicForm's readonly_unless handling.
            "readonly_unless": {"field": "class_mix_mode", "value": "count"},
        },
    )
    abreast: int = field(
        default=0, metadata={"min": 0}
    )  # seats per row (0 = auto from cabin width)
    # Bounds are the union across every class in payload_processed.md's Matrix B
    # (seat pitch/width by cabin class) -- shared across First/Business/Premium/
    # Economy since they're all instances of this one dataclass, so the range
    # has to cover the tightest realistic case (economy's 28 in / 16 in floor)
    # up to the loosest (a business flat-bed suite's >= 70 in pitch / up to
    # ~28 in width, plus headroom for width_m's *footprint* semantics -- see
    # PassengerCabinConfig's note below -- which already exceeds the raw
    # cushion width for First/Business by design). This still stops a wildly
    # uncertifiable value (e.g. a 0.2 m pitch) from being enterable at all.
    pitch_m: float = field(
        default=0.79,
        metadata={
            "min": 0.7112,
            "max": 2.5,
            "help": "Longitudinal seat spacing. Regulatory/industry floor is economy's 28 in (0.7112 m); "
            "premium/business/first cabins use larger values (Matrix B, payload_processed.md).",
        },
    )
    width_m: float = field(
        default=0.45,
        metadata={
            "min": 0.4064,
            "max": 1.2,
            "help": "Lateral seat footprint (incl. shell/armrests -- wider than the raw cushion for "
            "premium classes). Floor is economy's 16 in (0.4064 m) cushion width (Matrix B).",
        },
    )
    mass_per_pax_kg: float = 100.0  # occupant + carry-on


@dataclass
class PassengerCabinConfig:
    """Passenger cabin: class mix + monuments + baggage assumptions.

    Counts of 0 for galley/lavatory mean "auto" (derived from passenger count via
    the standard provisioning ratios). ``abreast`` of 0 on a class means "auto"
    (derived from the local cabin floor width).
    """

    # width_m is the lateral *footprint* per seat (incl. shell/armrests), so the
    # auto seats-abreast comes out realistic for premium classes (e.g. business
    # suites are ~0.7 m wide, first ~0.95 m), not just the cushion width.
    # mass_per_pax_kg is the *seated* mass (occupant + carry-on only); the checked
    # bag is added separately below and routed to the lower-deck holds, so the
    # per-occupant total (~ seat + checked_bag) stays consistent with the lumped
    # model's requirements.passenger_mass_kg (100 kg, body + all baggage).
    # How the class mix is specified. "percent" (default) reads each class's
    # share_pct and derives seat counts from the real cabin geometry -- the way
    # the named presets have always worked internally, and the way a cabin is
    # actually specified in practice. "count" is the fallback for pinning exact
    # per-class seat numbers.
    class_mix_mode: str = field(
        default="percent",
        metadata={
            "label": "Class mix mode",
            "options": ["percent", "count"],
            "help": "'percent': give each class a share of cabin length and let the layout solve the "
            "seat counts (recommended -- counts depend on pitch, abreast and fuselage shape). "
            "'count': type exact per-class seat numbers instead.",
        },
    )

    # Per-class seat geometry. pitch_m is seat-row spacing; width_m is the
    # lateral FOOTPRINT per seat (shell/armrests included, so it exceeds the
    # advertised cushion width for premium classes -- that footprint is what
    # sets a realistic seats-abreast). Values are the mid-range of current
    # in-service transport cabins (payload_processed.md Matrix B):
    #   first    1.93 m / 76 in pitch, 0.95 m footprint -- suite, 1-2-1
    #   business 1.55 m / 61 in pitch, 0.70 m footprint -- lie-flat, 1-2-1
    #     (was 1.27 m / 50 in: that is an ANGLED/recliner business pitch, and
    #      it disagreed with every shipped preset, which all already set 1.55 m
    #      for a real flat-bed cabin -- the default now matches them)
    #   premium  0.97 m / 38 in pitch, 0.52 m footprint -- 2-3-2 / 2-4-2
    #   economy  0.79 m / 31 in pitch, 0.46 m footprint -- 3-3 / 3-3-3
    # Default shares describe a conventional two-class short/medium-haul cabin
    # (a small business section up front, the rest economy); a preset overwrites
    # them, and "Custom" starts from these.
    first: SeatClassConfig = field(
        default_factory=lambda: SeatClassConfig(
            share_pct=0.0, pitch_m=1.93, width_m=0.95, mass_per_pax_kg=96.0
        )
    )
    business: SeatClassConfig = field(
        default_factory=lambda: SeatClassConfig(
            share_pct=15.0, pitch_m=1.55, width_m=0.70, mass_per_pax_kg=90.0
        )
    )
    premium: SeatClassConfig = field(
        default_factory=lambda: SeatClassConfig(
            share_pct=0.0, pitch_m=0.97, width_m=0.52, mass_per_pax_kg=86.0
        )
    )
    economy: SeatClassConfig = field(
        default_factory=lambda: SeatClassConfig(
            share_pct=85.0, pitch_m=0.79, width_m=0.46, mass_per_pax_kg=84.0
        )
    )

    aisle_width_m: float = 0.0  # 0 = auto per FAR/CS 25.815: 0.30 m for <= 19 pax,
    # 0.51 m (the 20-in upper-body clearance, which governs at seat/armrest level)
    # for >= 20 pax. Set explicitly to override (e.g. a wider premium aisle).
    galley_count: int = 0  # 0 = auto (~1 per 100 pax + 1)
    lavatory_count: int = 0  # 0 = auto (~1 per 45 pax)
    checked_bag_mass_kg: float = 16.0  # per passenger (EASA 2022 survey); containerised
    # into real lower-deck ULD positions (LD3 / LD3-45 / bulk, after the Matrix D fit check)
    # and trimmed toward the seating CG (airlines trim bags).
    belly_cargo_kg: float = 0.0  # revenue belly freight [kg] loaded into whatever
    # lower-deck ULD capacity remains after checked bags (0 = none). Real passenger
    # aircraft rarely fly with empty bellies -- set this to fill the holds. Capped at the
    # holds' remaining capacity; note it counts toward payload, so it trades against fuel
    # in the MTOW budget on the run-once detailed passes.
    wall_thickness_m: float = 0.15  # mould-line inset from outer skin to usable cabin
    # wall, per side: frames/stringers + >= 1 in insulation blankets + standoff drain gap
    # + trim panels (payload_processed.md Sec 9). 0.15 m is a typical narrowbody value.

    min_exit_pair_spacing_m: float = (
        11.0  # minimum realistic longitudinal spacing between
    )
    # adjacent emergency-exit-pair locations, per deck [m]. Caps how many exit pairs (and
    # therefore how many CS-25.807-legal passengers) a deck can actually support -- without
    # this, auto-sizing/layout would fill the entire cabin floor with seats regardless of
    # whether enough exits could realistically be installed to evacuate them (e.g. an
    # unconstrained max-density A380 single-class layout would seat ~1400+, far past the
    # real type's certified maximum). Calibrated so the shipped presets' single-class
    # ("Ryanair") max-density baseline lands on each type's real-world certified exit
    # limit at exactly 100% floor utilization (see cabin_layout.build_passenger_layout's
    # single-class floor-stretch): B787-9 = 420, A320-200 = 220 (floor-limited well below this,
    # not exit-limited -- see the Ryanair economy pitch note in physics.payload.apply_cabin_
    # preset), A380-800 = 840 (main 420 + upper 420). At 10 m spacing the A380-800 count comes
    # out unrealistically high (945: the main deck gets a 5th exit pair its real LOPA doesn't
    # have); 11 m removes that 5th pair while leaving the smaller types' pair counts (and
    # therefore their caps) unchanged.

    exit_capacity_realism_factor: float = (
        0.478  # derates ONLY Type-A exits' FAR/CS-25.807(g)
    )
    # THEORETICAL maximum rating (110/side) for the purposes of computing the aircraft's
    # overall realistic capacity CEILING only (see cabin_layout.max_certifiable_capacity) --
    # real certified capacities come from a full 90-second evacuation demonstration, which for
    # widebodies with several large Type-A doors typically comes in well below the naive sum of
    # each door's individual maximum rating (aisle/cross-aisle throughput becomes the limiting
    # factor, not door count). Calibrated so the 787-9's 4 Type-A door pairs give its real
    # 420-passenger exit limit exactly (4*110*0.478*2 = 420); also within tolerance for the
    # A340-300 (~375) and A380-800 (~853). Smaller exit types (B/C/I/II/III, narrowbodies with
    # fewer/smaller doors and shorter aisles) were found to track close to their nominal rating
    # in practice (calibrated against A320-200: ~180, A220-300: ~150) and are left undiscounted.
    # Does NOT affect the number of exit pairs actually installed for a given seated passenger
    # count (build_passenger_layout still sizes those from the full, undiscounted regulatory
    # table -- that part of FAR/CS-25.807(g) is applied literally).

    def classes(self) -> list:
        """Return ``[(name, SeatClassConfig), ...]`` for present classes, in
        forward-to-aft order (first -> business -> premium -> economy)."""
        ordered = [
            ("First", self.first),
            ("Business", self.business),
            ("Premium", self.premium),
            ("Economy", self.economy),
        ]
        return [(name, c) for name, c in ordered if c.count > 0]

    def total_seats(self) -> int:
        return sum(c.count for _name, c in self.classes())

    def length_share_mix(self) -> dict:
        """Normalised ``{class name: fraction of cabin length}`` from the
        per-class ``share_pct`` values -- the input
        :func:`alas.physics.cabin_layout.simulate_passenger_counts`
        expects, and the same "length mix" the named presets build internally.

        Classes with a zero/negative share are omitted (absent). Shares are
        normalised to sum to 1, so the user never has to make them add to
        exactly 100. Returns ``{}`` if no class has a positive share, which
        callers treat as "nothing to lay out" rather than dividing by zero."""
        raw = [
            ("First", self.first.share_pct),
            ("Business", self.business.share_pct),
            ("Premium", self.premium.share_pct),
            ("Economy", self.economy.share_pct),
        ]
        positive = [(name, float(pct)) for name, pct in raw if pct and pct > 0.0]
        total = sum(pct for _name, pct in positive)
        if total <= 0.0:
            return {}
        return {name: pct / total for name, pct in positive}

    def set_length_share_mix(self, mix: dict) -> None:
        """Write a ``{class name: fraction}`` mix back into ``share_pct`` (in
        percent), zeroing classes absent from ``mix``.

        Keeps the percent-mode UI honest after a named preset runs: the preset
        picks the mix internally, and without this the Cabin page would still
        show the previous (or default) shares while the layout reflected the
        preset's."""
        by_name = {
            "First": self.first,
            "Business": self.business,
            "Premium": self.premium,
            "Economy": self.economy,
        }
        for name, cls in by_name.items():
            cls.share_pct = float(mix.get(name, 0.0)) * 100.0


@dataclass
class CargoDeckConfig:
    """Cargo loading: which decks, which ULD types, and how to distribute load."""

    use_main_deck: bool = True  # True = main-deck freighter (PMC pallets)
    main_deck_uld: str = "PMC"  # ULD code for the main deck (see ULD_DATABASE:
    # PMC/PAG pallets, M1 20-ft box, LD6/LD8/LD11 wide containers, ...)
    lower_deck_uld: str = "LD3"  # ULD code for the lower holds (LD1/LD2/LD3/...);
    # automatically degrades to LD3-45 then bulk where the hold cross-section
    # fails the Matrix D envelope fit check (narrowbodies).
    # Loading strategy:
    #   target_cg      -- spread load, trim to the target CG
    #   min_pallets    -- concentrate full ULDs near the required CG (fewest units)
    #   door_proximity -- prioritise slots nearest the cargo doors (fast turnaround)
    #   uniform        -- even distribution across all slots
    loading_strategy: str = "target_cg"
    target_cg_pct_mac: float = 25.0  # <= 0 -> auto: centre of the CG envelope
    main_door_x_m: float = 0.0  # 0 -> auto (mid-cabin, main deck)
    fwd_door_x_m: float = 0.0  # 0 -> auto (forward lower hold)
    aft_door_x_m: float = 0.0  # 0 -> auto (aft lower hold)

    # CG-trim solver tuning (physics.cargo_loader.CargoLoadManager.solve):
    # how much payload [kg] to shift between slots per trim iteration, and
    # the iteration cap. A smaller step converges more precisely but may need
    # more iterations for a large aircraft; raise cg_trim_max_iterations if a
    # very large aircraft's load solver isn't converging within the cap.
    cg_trim_step_kg: float = 50.0
    cg_trim_max_iterations: int = 2000


@dataclass
class CabinConfig:
    """Composed cabin/payload configuration (passenger + cargo)."""

    passenger: PassengerCabinConfig = field(default_factory=PassengerCabinConfig)
    cargo: CargoDeckConfig = field(default_factory=CargoDeckConfig)
