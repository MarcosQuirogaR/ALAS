# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Design requirements -- the *user's* input to ALAS.

This is the high-level mission/target specification the user provides. The
optimizer then searches the design space (see
:mod:`alas.config.design_variables`) for the geometry that best satisfies
these requirements. Nothing here is hardcoded inside the solver: every value is
a field the user can set (in code or via a YAML config file).

The defaults below reproduce the "AVE long-range transport" reference case the
original scripts were tuned for, so the app runs out of the box.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class DesignRequirements:
    """Top-level mission targets and constraints supplied by the user."""

    # -- Cruise design point -------------------------------------------------
    cruise_mach: float = field(
        default=0.84,
        metadata={
            "label": "Cruise Mach number",
            "help": "Design cruise Mach number -- the primary speed target the optimizer sizes the aircraft around.",
        },
    )
    cruise_altitude_m: float = field(
        default=11887.2,
        metadata={
            "label": "Cruise altitude",
            "unit": "m",
            "help": "Design cruise altitude, used to compute air density/speed of sound for the cruise design point.",
        },
    )

    # -- Weights -------------------------------------------------------------
    mtow_kg: float = field(
        default=358_670.0,
        metadata={
            "label": "Max take-off weight (MTOW)",
            "unit": "kg",
            "help": "Target maximum take-off weight -- anchors the whole weight & balance / sizing pipeline.",
        },
    )

    # -- Aircraft Type & Payload ---------------------------------------------
    aircraft_type: str = field(
        default="passenger",
        metadata={
            "label": "Aircraft type",
            "help": "'passenger' or 'cargo' -- switches which cabin-preset list and payload model apply.",
        },
    )
    cabin_preset: str = field(
        default="Ryanair",
        metadata={
            "label": "Cabin preset",
            "help": "Named seating/payload layout preset ('Ryanair', 'Iberia', 'Emirates' for passenger; "
            "'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab.",
        },
    )
    num_passengers: int = field(
        default=350,
        metadata={
            "label": "Passenger count",
            "help": "Target passenger count (if aircraft_type is 'passenger'). Auto-recomputed when a cabin preset is "
            "active -- only editable with cabin_preset set to 'Custom'.",
            "readonly_unless": {"field": "cabin_preset", "value": "Custom"},
        },
    )
    cargo_payload_kg: float = field(
        default=102_100.0,
        metadata={
            "label": "Cargo payload capacity",
            "unit": "kg",
            "help": "Target cargo payload capacity (if aircraft_type is 'cargo'). Auto-recomputed when a cabin preset is "
            "active -- only editable with cabin_preset set to 'Custom'.",
            "readonly_unless": {"field": "cabin_preset", "value": "Custom"},
        },
    )
    max_structural_payload_kg: float = field(
        default=0.0,
        metadata={
            "label": "Max structural payload",
            "unit": "kg",
            "advanced": True,
            "help": "Maximum structural payload (= MZFW - OEW), i.e. the most the airframe may carry regardless of "
            "how much the belly could physically hold. In passenger mode the detailed layout fills the "
            "lower-deck belly with revenue freight (on top of passengers + checked bags) up to this "
            "structural limit, so the payload -- and therefore the residual fuel (MTOW - OEW - payload) -- "
            "matches the real aircraft's max-payload point. A widebody belly can volumetrically hold far "
            "more than this structural cap, so without it 'fill the belly' overshoots. 0 = disabled "
            "(use the explicit Cabin & Payload belly_cargo_kg instead).",
        },
    )

    # -- Structural design parameters ----------------------------------------
    ultimate_load_factor: float = field(
        default=3.75,
        metadata={
            "label": "Ultimate load factor (n_ult)",
            "advanced": True,
            "help": "Limit load factor times the 1.5 safety margin, fed into the Torenbeek structural mass formulas.",
        },
    )
    dive_speed_m_s: float = field(
        default=220.0,
        metadata={
            "label": "Design dive speed (V_dive)",
            "unit": "m/s",
            "advanced": True,
            "help": "Structural design dive speed, fed into the Torenbeek structural mass formulas. Also VD on the "
            "V-n diagram; design cruise speed VC is derived as VD/1.25 (CS-25.335(b) minimum margin) rather "
            "than a separate field.",
        },
    )
    limit_load_factor_neg: float = field(
        default=-1.0,
        metadata={
            "label": "Limit load factor, negative (n_lim,neg)",
            "advanced": True,
            "help": "CS-25.337(c) negative limit load factor for the V-n diagram. The positive limit load factor is "
            "derived as ultimate_load_factor / 1.5 (CS-25.303) rather than a separate field.",
        },
    )

    # -- Sizing constraints --------------------------------------------------
    max_wing_area_m2: float = field(
        default=535.0,
        metadata={
            "label": "Maximum wing area",
            "unit": "m^2",
            "advanced": True,
            "help": "Upper bound on wing planform area; the optimizer is penalised for exceeding it.",
        },
    )
    min_wing_loading_kg_m2: float = field(
        default=485.0,
        metadata={
            "label": "Minimum wing loading (MTOW/S)",
            "unit": "kg/m^2",
            "advanced": True,
            "help": "Lower bound on wing loading (MTOW / wing area) -- keeps the wing from being sized too large for the mass it carries.",
        },
    )

    # -- Aerodynamic targets -------------------------------------------------
    # (The cruise-alpha window the optimizer targets lives on
    # ObjectiveWeights.alpha_min_penalty_deg/alpha_max_penalty_deg -- Advanced
    # Settings -> Optimizer & weights -- not here. This class used to carry a
    # parallel target_cruise_alpha_deg/acceptable_alpha_band_deg pair, but no
    # code ever read them once the alpha penalty moved to ObjectiveWeights;
    # removed rather than left as dead fields on the primary Inputs tab,
    # where a user editing them would reasonably expect an effect.)
    max_cruise_cl: float = field(
        default=0.95,
        metadata={
            "label": "Maximum cruise CL (stall guard)",
            "advanced": True,
            "help": "Candidate designs whose required cruise CL exceeds this are rejected as infeasible (too close to stall).",
        },
    )

    # -- Stability target / Aft CG limit offset -------------------------------
    target_static_margin: float = field(
        default=0.10,
        metadata={
            "label": "Target static margin",
            "unit": "fraction of MAC",
            "advanced": True,
            "help": "Static margin at the Aft CG Limit: Aft CG Limit (%MAC) = Neutral Point (%MAC) - target_static_margin*100. "
            "A positive value ensures positive static stability when the CG is at the aft limit.",
        },
    )

    # -- CG envelope range ---------------------------------------------------
    cg_range_pct_mac: float = field(
        default=30.0,
        metadata={
            "label": "CG envelope width",
            "unit": "% MAC",
            "advanced": True,
            "help": "Width of the CG envelope. Forward CG Limit (%MAC) = Aft CG Limit (%MAC) - cg_range_pct_mac.",
        },
    )

    # -- Physical stability guard --------------------------------------------
    min_physical_static_margin: float = field(
        default=0.05,
        metadata={
            "label": "Minimum physical static margin",
            "unit": "fraction of MAC",
            "advanced": True,
            "help": "Minimum static margin measured using the actual mass-model (physical) CG, not the aerodynamic reference point. "
            "Designs below this are hard-rejected as inherently unstable. 0.0 = bare stability; 0.05 = 5% MAC buffer (recommended).",
        },
    )

    # -- Passenger weight assumption -----------------------------------------
    passenger_mass_kg: float = field(
        default=100.0,
        metadata={
            "label": "Mass per passenger",
            "unit": "kg",
            "advanced": True,
            "help": "Combined average mass per occupant (body + baggage). FAA AC 120-27E standard is 100 kg; airlines may use 90-105 kg.",
        },
    )

    # -- Physical constants --------------------------------------------------
    gravity_m_s2: float = field(
        default=9.81,
        metadata={
            "label": "Gravitational acceleration",
            "unit": "m/s^2",
            "advanced": True,
        },
    )

    def __post_init__(self) -> None:
        if self.aircraft_type not in ("passenger", "cargo"):
            raise ValueError(
                f"aircraft_type must be 'passenger' or 'cargo', "
                f"got '{self.aircraft_type}'."
            )
        # Safeguard cabin preset based on aircraft type
        if self.aircraft_type == "cargo":
            if self.cabin_preset not in ("Max payload", "Dense payload", "Custom"):
                self.cabin_preset = "Max payload"
        else:
            if self.cabin_preset not in ("Ryanair", "Iberia", "Emirates", "Custom"):
                self.cabin_preset = "Custom"

    @property
    def payload_kg(self) -> float:
        """Total payload mass based on passenger or cargo configuration."""
        if self.aircraft_type == "cargo":
            return self.cargo_payload_kg
        return self.num_passengers * self.passenger_mass_kg

    def required_cruise_cl(
        self, dynamic_pressure_pa: float, wing_area_m2: float
    ) -> float:
        """CL needed to sustain level cruise: CL = W / (q * S)."""
        weight_n = self.mtow_kg * self.gravity_m_s2
        return weight_n / (dynamic_pressure_pa * wing_area_m2)
