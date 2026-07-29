# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Control-surface geometry configuration -- chord/span-fraction bounds for the sizing diagram."""

from __future__ import annotations
from dataclasses import dataclass, field


@dataclass
class ControlSurfacesConfig:
    """Wing (slat/flap/aileron/spoiler) and tail (elevator/rudder) control-surface
    chord-fraction and span-fraction (of the local semi-span) bounds.

    Purely a *representation* input for the control-surface sizing diagram
    (``reporting/visualization.py::figure_control_surfaces``) -- ALAS
    models the wing/tail as plain lifting surfaces without deflectable
    control-surface sub-geometry, so these fractions don't feed back into
    the aerodynamic or mass models, only what that one diagram draws.
    Defaults are typical transport-aircraft proportions.
    """

    slat_chord_fraction: float = field(
        default=0.15,
        metadata={
            "label": "Slat chord fraction",
            "help": "Leading-edge slat chord as a fraction of local wing chord.",
        },
    )
    slat_span_start_frac: float = field(
        default=0.08,
        metadata={
            "label": "Slat span start",
            "unit": "fraction of semi-span",
            "help": "Inboard end of the slat run, as a fraction of wing semi-span from the root.",
        },
    )
    slat_span_end_frac: float = field(
        default=0.95,
        metadata={
            "label": "Slat span end",
            "unit": "fraction of semi-span",
            "help": "Outboard end of the slat run, as a fraction of wing semi-span from the root.",
        },
    )

    flap_chord_fraction: float = field(
        default=0.25,
        metadata={
            "label": "Flap chord fraction",
            "help": "Trailing-edge flap chord as a fraction of local wing chord.",
        },
    )
    flap_span_start_frac: float = field(
        default=0.10,
        metadata={
            "label": "Flap span start",
            "unit": "fraction of semi-span",
            "help": "Inboard end of the flap run (just outside the fuselage), as a fraction of wing semi-span.",
        },
    )
    flap_span_end_frac: float = field(
        default=0.62,
        metadata={
            "label": "Flap span end",
            "unit": "fraction of semi-span",
            "help": "Outboard end of the flap run, as a fraction of wing semi-span.",
        },
    )

    aileron_chord_fraction: float = field(
        default=0.20,
        metadata={
            "label": "Aileron chord fraction",
            "help": "Aileron chord as a fraction of local wing chord.",
        },
    )
    aileron_span_start_frac: float = field(
        default=0.66,
        metadata={
            "label": "Aileron span start",
            "unit": "fraction of semi-span",
            "help": "Inboard end of the aileron run, as a fraction of wing semi-span.",
        },
    )
    aileron_span_end_frac: float = field(
        default=0.95,
        metadata={
            "label": "Aileron span end",
            "unit": "fraction of semi-span",
            "help": "Outboard end of the aileron run, as a fraction of wing semi-span.",
        },
    )

    spoiler_chord_fraction: float = field(
        default=0.10,
        metadata={
            "label": "Spoiler chord fraction",
            "help": "Spoiler/speedbrake chord as a fraction of local wing chord (ahead of the flaps).",
        },
    )
    spoiler_span_start_frac: float = field(
        default=0.10,
        metadata={
            "label": "Spoiler span start",
            "unit": "fraction of semi-span",
            "help": "Inboard end of the spoiler run, as a fraction of wing semi-span.",
        },
    )
    spoiler_span_end_frac: float = field(
        default=0.64,
        metadata={
            "label": "Spoiler span end",
            "unit": "fraction of semi-span",
            "help": "Outboard end of the spoiler run (typically spanning the flap run), as a fraction of wing semi-span.",
        },
    )

    elevator_chord_fraction: float = field(
        default=0.35,
        metadata={
            "label": "Elevator chord fraction",
            "help": "Elevator chord as a fraction of local horizontal-stabilizer chord.",
        },
    )
    elevator_span_start_frac: float = field(
        default=0.05,
        metadata={
            "label": "Elevator span start",
            "unit": "fraction of semi-span",
            "help": "Inboard end of the elevator run, as a fraction of h-stab semi-span.",
        },
    )
    elevator_span_end_frac: float = field(
        default=0.95,
        metadata={
            "label": "Elevator span end",
            "unit": "fraction of semi-span",
            "help": "Outboard end of the elevator run, as a fraction of h-stab semi-span.",
        },
    )

    rudder_chord_fraction: float = field(
        default=0.35,
        metadata={
            "label": "Rudder chord fraction",
            "help": "Rudder chord as a fraction of local vertical-stabilizer chord.",
        },
    )
    rudder_span_start_frac: float = field(
        default=0.10,
        metadata={
            "label": "Rudder span start",
            "unit": "fraction of semi-span",
            "help": "Root-ward end of the rudder run, as a fraction of v-stab span.",
        },
    )
    rudder_span_end_frac: float = field(
        default=0.90,
        metadata={
            "label": "Rudder span end",
            "unit": "fraction of semi-span",
            "help": "Tip-ward end of the rudder run, as a fraction of v-stab span.",
        },
    )
