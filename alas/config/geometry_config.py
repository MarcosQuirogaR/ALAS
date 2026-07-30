# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Geometry configuration -- every value the original scripts hardcoded inside
``define_ave_geometry`` / ``gen_geometry`` now lives here as a named, overridable
field.

The split is deliberate:

* :mod:`alas.config.design_variables` holds the degrees of freedom the
  *optimizer* is allowed to vary.
* This module holds the structural "scaffold" -- vertical placements, section
  twists, the empennage layout, the fuselage stations, the engine nacelle
  profile -- that defines the *family* of aircraft. These are fixed during an
  optimization run but remain user-configurable between runs, so there are no
  buried magic numbers.

Lengths are in metres, angles in degrees, unless noted otherwise.

Fields carry ``metadata={"label": ..., "unit": ..., "help": ...}`` so the
auto-generated settings form can show a
descriptive name, a bracketed unit, and a hover tooltip instead of a bare
mechanical transform of the field name.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Tuple


@dataclass
class WingConfig:
    """Main-wing scaffold not covered by the design vector."""

    root_datum_x_m: float = field(
        default=26.24,
        metadata={
            "label": "Wing root X position",
            "unit": "m",
            "help": "Fuselage-station X of the wing-root leading-edge datum -- how far aft of the nose the wing sits.",
        },
    )
    root_z_m: float = field(
        default=-2.1,
        metadata={
            "label": "Wing root vertical offset",
            "unit": "m",
            "help": "Vertical (Z) placement of the wing-root leading edge relative to the fuselage centerline.",
        },
    )
    break_z_m: float = field(
        default=-0.3,
        metadata={
            "label": "Wing break vertical offset",
            "unit": "m",
            "help": "Vertical placement of the mid-span 'break' section leading edge, where the taper/dihedral rate changes.",
        },
    )
    tip_z_m: float = field(
        default=2.5,
        metadata={
            "label": "Wing tip vertical offset",
            "unit": "m",
            "help": "Vertical placement of the wing-tip leading edge. Tip above root gives positive dihedral.",
        },
    )
    root_twist_deg: float = field(
        default=4.0,
        metadata={
            "label": "Wing root twist",
            "unit": "deg",
            "help": "Geometric twist (incidence) of the root section, positive = leading-edge-up (washin).",
        },
    )
    break_twist_deg: float = field(
        default=2.0,
        metadata={
            "label": "Wing break twist",
            "unit": "deg",
            "help": "Geometric twist of the mid-span break section.",
        },
    )
    break_span_fraction: float = field(
        default=0.35,
        metadata={
            "label": "Wing break span location",
            "unit": "0-1 of semispan",
            "help": "Spanwise position of the trailing-edge break, as a fraction of the semispan (0 = root, 1 = tip).",
        },
    )
    outboard_sweep_decrement_deg: float = field(
        default=2.0,
        metadata={
            "label": "Outboard sweep reduction",
            "unit": "deg",
            "help": "How many degrees less swept the outboard panel is than the inboard panel (a common yehudi/crank shape).",
        },
    )
    root_airfoil: str = field(
        default="SC2-0714",
        metadata={
            "label": "Root airfoil section",
            "help": "Reference airfoil at the wing root, morphed by the design vector's thickness/camber scale factors.",
        },
    )
    tip_airfoil: str = field(
        default="naca2410",
        metadata={
            "label": "Tip airfoil section",
            "help": "Reference airfoil at the wing tip.",
        },
    )
    n_subdivisions: int = field(
        default=8,
        metadata={
            "label": "Wing VLM panel count",
            "help": "Spanwise panel refinement per wing section for the vortex-lattice solver. Higher = more accurate, slower.",
        },
    )


@dataclass
class EmpennageConfig:
    """Horizontal and vertical stabiliser scaffold.

    In-plane dimensions are multiplied by the ``tail_scale`` design variable at
    build time; the values here are the unit-scale (tail_scale = 1.0) geometry.
    """

    tail_airfoil: str = field(
        default="naca0012",
        metadata={
            "label": "Tail airfoil section",
            "help": "Reference airfoil shared by both the horizontal and vertical stabilisers (usually symmetric, e.g. NACA 00xx).",
        },
    )
    n_subdivisions: int = field(
        default=6,
        metadata={
            "label": "Tail VLM panel count",
            "help": "Spanwise panel refinement per tail surface for the vortex-lattice solver.",
        },
    )

    # Horizontal stabiliser
    hstab_offset_from_tail_m: float = field(
        default=10.7,
        metadata={
            "label": "H-stab offset forward of tail tip",
            "unit": "m",
            "help": "How far forward of the fuselage tail tip the horizontal-stabiliser root leading edge sits.",
        },
    )
    hstab_z_m: float = field(
        default=1.2,
        metadata={
            "label": "H-stab vertical offset",
            "unit": "m",
            "help": "Vertical placement of the horizontal stabiliser relative to the fuselage centerline.",
        },
    )
    hstab_root_chord_m: float = field(
        default=8.0,
        metadata={
            "label": "H-stab root chord",
            "unit": "m",
        },
    )
    hstab_tip_chord_m: float = field(
        default=2.2,
        metadata={
            "label": "H-stab tip chord",
            "unit": "m",
        },
    )
    hstab_root_twist_deg: float = field(
        default=-2.0,
        metadata={
            "label": "H-stab root twist",
            "unit": "deg",
            "help": "Incidence of the horizontal stabiliser root -- usually slightly negative to trim the wing's nose-down pitching moment.",
        },
    )
    hstab_tip_twist_deg: float = field(
        default=-2.0,
        metadata={
            "label": "H-stab tip twist",
            "unit": "deg",
        },
    )
    hstab_tip_le_m: Tuple[float, float, float] = field(
        default=(7.5, 11.0, 1.0),
        metadata={
            "label": "H-stab tip leading edge (x, y, z)",
            "unit": "m",
            "help": "Position of the horizontal-stabiliser tip leading edge relative to its root, as (x, y, z).",
        },
    )

    # Vertical stabiliser
    vstab_offset_from_tail_m: float = field(
        default=12.2,
        metadata={
            "label": "V-stab offset forward of tail tip",
            "unit": "m",
            "help": "How far forward of the fuselage tail tip the vertical-stabiliser root leading edge sits.",
        },
    )
    vstab_z_m: float = field(
        default=2.0,
        metadata={
            "label": "V-stab vertical offset",
            "unit": "m",
        },
    )
    vstab_root_chord_m: float = field(
        default=9.5,
        metadata={
            "label": "V-stab root chord",
            "unit": "m",
        },
    )
    vstab_tip_chord_m: float = field(
        default=3.2,
        metadata={
            "label": "V-stab tip chord",
            "unit": "m",
        },
    )
    vstab_tip_le_m: Tuple[float, float, float] = field(
        default=(9.0, 0.0, 9.8),
        metadata={
            "label": "V-stab tip leading edge (x, y, z)",
            "unit": "m",
            "help": "Position of the vertical-stabiliser tip leading edge relative to its root, as (x, y, z).",
        },
    )


@dataclass
class FuselageConfig:
    """Fuselage body-of-revolution or ovoid stations."""

    diameter_m: float = field(
        default=6.2,
        metadata={
            "label": "Fuselage diameter (width)",
            "unit": "m",
            "help": "Maximum fuselage cross-sectional diameter -- the main driver of cabin width and wetted area. "
            "Best judged on the 3-view preview's front/isometric panels, not the top view.",
        },
    )
    height_m: Optional[float] = field(
        default=None,
        metadata={
            "label": "Fuselage height (if non-circular)",
            "unit": "m",
            "help": "Vertical cross-section dimension. Leave blank/none for a circular fuselage where height equals diameter.",
        },
    )
    nose_z_m: float = field(
        default=-0.5,
        metadata={
            "label": "Nose vertical offset",
            "unit": "m",
            "help": "Vertical offset of the nose tip relative to the fuselage centerline.",
        },
    )
    cabin_start_x_m: float = field(
        default=6.0,
        metadata={
            "label": "Cabin start X position",
            "unit": "m",
            "help": "X-station where the fuselage first reaches full diameter, i.e. the end of the nose taper.",
        },
    )
    cabin_z_m: float = field(
        default=0.2,
        metadata={
            "label": "Cabin vertical offset",
            "unit": "m",
            "help": "Vertical offset of the cylindrical cabin section relative to the fuselage centerline.",
        },
    )
    tailcone_length_m: float = field(
        default=14.0,
        metadata={
            "label": "Tailcone length",
            "unit": "m",
            "help": "Length of the aft taper, from the end of the cylindrical cabin section to the tail tip.",
        },
    )
    tail_z_m: float = field(
        default=1.8,
        metadata={
            "label": "Tail vertical offset",
            "unit": "m",
            "help": "Vertical offset of the upswept tail tip (positive = tail rises above centerline, typical for ground clearance/rotation).",
        },
    )
    n_subdivisions: int = field(
        default=12,
        metadata={
            "label": "Fuselage cross-section count",
            "help": "Number of longitudinal stations used to loft the fuselage body. Higher = smoother surface, slower to draw.",
        },
    )


@dataclass
class EngineConfig:
    """Podded engine / nacelle scaffold AND the live, editable engine design
    parameters (thrust, cycle data) -- the single source of truth every
    consumer (mass estimation, SUAVE mission analysis, the Matching Chart/LTO
    T/W lookup, the payload-range TSFC, and the Propulsion Analysis tab) reads
    from, rather than each independently re-looking up the immutable
    :data:`alas.config.engines.ENGINE_DATABASE` by name. ``engine_name``
    is only a preset SELECTOR: choosing one (or calling
    :meth:`apply_engine_spec`) copies that entry's values in here once: after
    that, editing any field below (e.g. from the "Engine Designer" Advanced
    Settings tab) changes the design's actual engine everywhere at once, with
    no risk of the edit only reaching some consumers.

    Edited only here, never via the Geometry Scaffold tab's auto-generated
    form (see ``hide_in_form`` on :class:`GeometryConfig`'s ``engine`` field)
    -- the "Engine Designer" tab is this dataclass's one dedicated editor.
    """

    engine_name: str = field(
        default="GE9X",
        metadata={
            "label": "Engine model",
            "help": "Name from the built-in engine database (see the Engine selector on the Inputs tab) -- drives thrust, "
            "mass, and the default nacelle profile.",
        },
    )

    # Nacelle profile as (x_station_m, radius_fraction) pairs, scaled by radius_scale.
    nacelle_profile: List[Tuple[float, float]] = field(
        default_factory=lambda: [
            (0.0, 0.40),
            (0.6, 0.92),
            (1.2, 1.0),
            (4.3, 1.0),
            (5.9, 0.82),
            (7.8, 0.45),
        ],
        metadata={
            "label": "Nacelle profile points",
            "advanced": True,
            "columns": ["x-station [m]", "radius fraction [0-1]"],
            "help": "List of (x-station [m], radius-fraction [0-1 of radius_scale_m below]) pairs tracing the "
            "nacelle's longitudinal silhouette from inlet (x=0) to exit -- see the Engine Designer tab's "
            "live nacelle-silhouette preview for a picture of the shape these points draw. "
            "Auto-filled from the engine database when engine_name is recognised.",
        },
    )
    radius_scale_m: float = field(
        default=2.1,
        metadata={
            "label": "Nacelle max radius",
            "unit": "m",
            "advanced": True,
            "help": "Physical radius the nacelle_profile's fraction-of-1.0 points are scaled by.",
        },
    )
    spanwise_positions_m: Tuple[float, ...] = field(
        default=(9.8, -9.8),
        metadata={
            "label": "Engine spanwise (Y) positions",
            "unit": "m",
            "advanced": True,
            "help": "Y-coordinate of each engine's mount position (negative = left/port side); one entry per engine.",
        },
    )
    z_m: float = field(
        default=-2.9,
        metadata={
            "label": "Engine vertical offset",
            "unit": "m",
            "advanced": True,
            "help": "Vertical placement of the engine centerline relative to the wing reference plane (negative = below the wing).",
        },
    )
    inlet_x_offset_m: float = field(
        default=4.2,
        metadata={
            "label": "Inlet offset ahead of wing LE",
            "unit": "m",
            "advanced": True,
            "help": "How far forward of the (swept) wing leading edge the nacelle inlet face sits.",
        },
    )

    # -- Live engine design parameters (thrust + cycle data). Defaults match
    # the GE9X registry entry -- the same "fallback = default engine" pattern
    # nacelle_profile/radius_scale_m above already use for a bare EngineConfig()
    # constructed before apply_engine_spec() has run. -----------------------
    thrust_kn: float = field(
        default=467.0,
        metadata={
            "label": "Rated thrust per engine",
            "unit": "kN",
            "decimals": 2,
            "help": "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, "
            "the Matching Chart T/W lookup, and the SUAVE turbofan sizing target.",
        },
    )
    bypass_ratio: float = field(
        default=10.0,
        metadata={
            "label": "Bypass ratio (BPR)",
            "unit": "-",
            "help": "Ratio of bypass (fan duct) to core mass flow. Feeds the SUAVE turbofan network and the "
            "Propulsion Analysis on-design cycle.",
        },
    )
    overall_pressure_ratio: float = field(
        default=60.0,
        metadata={
            "label": "Overall (core) pressure ratio (OPR)",
            "unit": "-",
            "advanced": True,
            "help": "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the "
            "fan). Feeds SUAVE's compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) "
            "and the Propulsion Analysis cycle's compressor_pressure_ratio.",
        },
    )
    fan_pressure_ratio: float = field(
        default=1.45,
        metadata={
            "label": "Fan pressure ratio (FPR)",
            "unit": "-",
            "advanced": True,
            "help": "Pressure ratio across the fan (bypass stream), separate from the core OPR above.",
        },
    )
    turbine_inlet_temp_k: float = field(
        default=1670.0,
        metadata={
            "label": "Turbine inlet temperature (T4t)",
            "unit": "K",
            "advanced": True,
            "help": "Combustor-exit stagnation temperature -- the primary driver of specific thrust and thermal "
            "efficiency in the on-design cycle.",
        },
    )
    cruise_tsfc_kg_kgf_hr: float = field(
        default=0.50,
        metadata={
            "label": "Cruise TSFC (reference)",
            "unit": "kg/(kgf.hr)",
            "help": "Reference cruise thrust-specific fuel consumption, used by the Breguet payload-range "
            "diagram. Not necessarily identical to the Propulsion Analysis tab's on-design-cycle-"
            "computed TSFC (a fast conceptual cycle model with generic component efficiencies vs. this "
            "field's real/published in-service figure) -- see that tab's caption.",
        },
    )
    fan_diameter_m: float = field(
        default=3.40,
        metadata={
            "label": "Fan diameter",
            "unit": "m",
            "advanced": True,
            "help": "Fan face diameter -- informational/reference only (does not currently size the nacelle "
            "profile, which comes from radius_scale_m/nacelle_profile above).",
        },
    )

    def apply_engine_spec(self) -> None:
        """Overwrite nacelle profile, radius, and every engine design field
        (thrust, cycle data) from the registry entry named by ``engine_name``.

        This is the ONLY place :data:`~alas.config.engines.ENGINE_DATABASE`
        is read once a design is running -- every other consumer reads the
        fields this method just set, not the registry directly, so editing
        those fields afterwards (e.g. in the Engine Designer tab) actually
        changes what mass estimation/SUAVE/the Matching Chart/Propulsion
        Analysis all use.
        """
        from .engines import get_engine

        try:
            spec = get_engine(self.engine_name)
            self.nacelle_profile = spec.nacelle_profile()
            self.radius_scale_m = spec.nacelle_max_radius_m
            self.thrust_kn = spec.thrust_kn
            self.bypass_ratio = spec.bypass_ratio
            self.overall_pressure_ratio = spec.overall_pressure_ratio
            self.fan_pressure_ratio = spec.fan_pressure_ratio
            self.turbine_inlet_temp_k = spec.turbine_inlet_temp_k
            self.cruise_tsfc_kg_kgf_hr = spec.cruise_tsfc_kg_kgf_hr
            self.fan_diameter_m = spec.fan_diameter_m
        except KeyError:
            pass  # keep manual fallback values

    def nacelle_length_m(self) -> float:
        """Nacelle length [m], derived from the last (aft-most) nacelle-profile
        x-station rather than stored as a separate field -- ``nacelle_profile``
        already fixes it exactly (see ``EngineSpec.nacelle_profile()``)."""
        return float(self.nacelle_profile[-1][0]) if self.nacelle_profile else 0.0


@dataclass
class GeometryConfig:
    """Composed geometry scaffold for the whole aircraft."""

    wing: WingConfig = field(
        default_factory=WingConfig,
        metadata={
            "help": "Main-wing scaffold parameters not already covered by the optimizer's design vector.",
        },
    )
    empennage: EmpennageConfig = field(
        default_factory=EmpennageConfig,
        metadata={
            "help": "Horizontal and vertical stabiliser scaffold.",
        },
    )
    fuselage: FuselageConfig = field(
        default_factory=FuselageConfig,
        metadata={
            "help": "Fuselage body-of-revolution stations, incl. diameter (width) and length breakdown.",
        },
    )
    engine: EngineConfig = field(
        default_factory=EngineConfig,
        metadata={
            "help": "Podded engine / nacelle placement, shape, and design parameters -- edited on the "
            "dedicated 'Engine Designer' Advanced Settings tab, not here.",
            "hide_in_form": True,
        },
    )

    # Wetted-area / form-factor reference data used by the drag buildup that are
    # geometric in nature (see alas.physics.aerodynamics for usage).
    wing_wetted_area_factor: float = field(
        default=2.05,
        metadata={
            "label": "Wing wetted-area factor",
            "help": "Ratio of wetted (exposed, both sides) surface area to planform area for a thin wing. Used by the drag buildup.",
        },
    )
    fuselage_wetted_factor: float = field(
        default=0.9,
        metadata={
            "label": "Fuselage wetted-area factor",
            "help": "Correction factor on pi*diameter*length for a non-cylindrical (tapered nose/tail) fuselage body.",
        },
    )
