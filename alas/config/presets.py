# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Aircraft preset registry for ALAS.

Each preset maps a real-world aircraft name to a complete set of configuration
objects: DesignVector defaults, GeometryConfig scaffold, DesignRequirements,
and engine name. This lets the user switch between aircraft families from a
single dropdown.

Dimensions come from publicly available specification sheets (Airbus, Boeing).
Root/tip chords for proprietary designs are estimated from wing area, aspect
ratio, taper ratio, and sweep using standard planform geometry relations.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple

from .design_variables import DesignVector
from .geometry_config import (
    EmpennageConfig,
    EngineConfig,
    FuselageConfig,
    GeometryConfig,
    WingConfig,
)
from .requirements import DesignRequirements
from .mass_config import MassModelConfig
from .performance_config import PerformanceConfig
from .performance_presets import get_performance_preset


@dataclass
class AircraftPreset:
    """A complete aircraft configuration preset."""

    name: str
    display_name: str
    description: str
    design_vector: DesignVector
    geometry: GeometryConfig
    requirements: DesignRequirements
    engine_name: str
    n_engines: int = 2
    # Optional per-aircraft mass-model calibration. The global Torenbeek
    # fractions in MassModelConfig are calibrated around the widebody
    # reference (e.g. the B787-9 lands within ~0.5 t of its real 128.8 t OEW
    # with the defaults), but smaller/older types deviate -- structural and
    # systems/furnishings mass does not scale linearly with MTOW, so a small
    # narrowbody like the A220-300 carries a higher OEW/MTOW fraction than a
    # widebody. When set, this MassModelConfig replaces the default for this
    # preset so its computed OEW matches the published figure. None = use the
    # global default.
    mass_model: Optional[MassModelConfig] = None
    # Optional per-aircraft field-performance / high-lift assumptions
    # (:class:`PerformanceConfig`). A modern widebody (triple-slotted flaps +
    # slats) reaches a much higher take-off CLmax than the narrowbody default,
    # which is what makes its take-off/landing V-speeds (VR/V2) come out right
    # instead of ~10 kt high. When set, this replaces the default performance
    # config. None = use the global default (standard narrowbody).
    performance: Optional[PerformanceConfig] = None

    def __post_init__(self):
        # Synchronise the engine name inside the geometry configuration
        if self.geometry and self.geometry.engine:
            self.geometry.engine.engine_name = self.engine_name

    def engine_spanwise_positions(self) -> Tuple[float, ...]:
        """Return the spanwise engine mount positions for this preset."""
        return self.geometry.engine.spanwise_positions_m


# ---------------------------------------------------------------------------
# Preset Registry
# ---------------------------------------------------------------------------

_PRESET_REGISTRY: Dict[str, AircraftPreset] = {}


def _register(preset: AircraftPreset) -> None:
    _PRESET_REGISTRY[preset.name] = preset


def get_preset(name: str) -> AircraftPreset:
    """Look up a preset by name. Raises KeyError if not found."""
    if name not in _PRESET_REGISTRY:
        raise KeyError(
            f"Unknown preset '{name}'. Available: {sorted(_PRESET_REGISTRY)}"
        )
    return _PRESET_REGISTRY[name]


def available_presets() -> List[str]:
    """Return list of all registered preset names in display order."""
    return list(_PRESET_REGISTRY.keys())


def preset_display_names() -> Dict[str, str]:
    """Return dict mapping preset name -> display name."""
    return {k: v.display_name for k, v in _PRESET_REGISTRY.items()}


# ===========================================================================
# AVE (reference long-range twin -- the original ALAS design)
# ===========================================================================
_register(
    AircraftPreset(
        name="AVE",
        display_name="AVE (Reference Twin)",
        description="Long-range widebody twin reference aircraft based on 777X-class geometry.",
        engine_name="GE9X",
        n_engines=2,
        design_vector=DesignVector(
            span_m=71.75,
            root_chord_m=16.50,
            break_chord_m=7.80,
            tip_chord_m=1.60,
            sweep_deg=34.0,
            tip_twist_deg=0.0,
            # wing_x_shift_m=0.5 keeps AVE's OEW/MZFW/MTOW CG compliant with the
            # min-nose-load (steering) gear limit at its auto-sized 525-passenger,
            # 65 t max_structural_payload_kg cabin. The compliant range for
            # wing_x_shift_m is approximately [-1.70, 2.64]; 0.5 is the range's
            # midpoint (OEW/MZFW/MTOW CG all land at ~25% MAC, static margin
            # 0.267 -- comfortably inside the envelope in both directions, not
            # just barely compliant).
            wing_x_shift_m=0.5,
            tail_scale=1.0,
            fuselage_length_m=76.72,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=25.14,
                root_z_m=-2.1,
                break_z_m=-0.3,
                tip_z_m=2.5,
                root_twist_deg=4.0,
                break_twist_deg=2.0,
                break_span_fraction=0.35,
                outboard_sweep_decrement_deg=2.0,
                root_airfoil="SC2-0714",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=10.7,
                hstab_z_m=1.2,
                hstab_root_chord_m=8.0,
                hstab_tip_chord_m=2.2,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(7.5, 11.0, 1.0),
                vstab_offset_from_tail_m=12.2,
                vstab_z_m=2.0,
                vstab_root_chord_m=9.5,
                vstab_tip_chord_m=3.2,
                vstab_tip_le_m=(9.0, 0.0, 9.8),
            ),
            fuselage=FuselageConfig(
                diameter_m=6.2,
                nose_z_m=-0.5,
                cabin_start_x_m=6.0,
                cabin_z_m=0.2,
                tailcone_length_m=14.0,
                tail_z_m=1.8,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(9.8, -9.8),
                # z_m is a direct offset below the local wing z (negative = below).
                # Calibrated to give ~0.35 m clearance below the wing lower surface.
                z_m=-3.13,
                inlet_x_offset_m=4.2,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.84,
            cruise_altitude_m=11887.2,
            mtow_kg=358_670,
            max_wing_area_m2=535.0,
            min_wing_loading_kg_m2=485.0,
            max_structural_payload_kg=65_000.0,  # notional widebody (~777-300ER class max payload)
        ),
        performance=get_performance_preset("advanced_highlift_widebody").settings,
    )
)

# ===========================================================================
# Airbus A340-300 (long-range quad)
# ===========================================================================
_register(
    AircraftPreset(
        name="A340-300",
        display_name="Airbus A340-300",
        description="Long-range quad-engine widebody with CFM56-5C engines.",
        engine_name="CFM56-5C",
        n_engines=4,
        design_vector=DesignVector(
            span_m=60.30,
            root_chord_m=12.00,
            break_chord_m=6.50,
            tip_chord_m=1.80,
            sweep_deg=30.0,
            tip_twist_deg=-2.0,
            wing_x_shift_m=-1.4,
            tail_scale=1.0,
            fuselage_length_m=63.69,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=22.0,
                root_z_m=-1.8,
                break_z_m=-0.3,
                tip_z_m=2.0,
                root_twist_deg=3.5,
                break_twist_deg=1.5,
                break_span_fraction=0.35,
                outboard_sweep_decrement_deg=2.0,
                root_airfoil="sc20612",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=9.0,
                hstab_z_m=1.0,
                hstab_root_chord_m=6.5,
                hstab_tip_chord_m=1.8,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(6.0, 9.0, 0.8),
                vstab_offset_from_tail_m=10.5,
                vstab_z_m=1.8,
                vstab_root_chord_m=8.0,
                vstab_tip_chord_m=2.8,
                vstab_tip_le_m=(7.5, 0.0, 8.5),
            ),
            fuselage=FuselageConfig(
                diameter_m=5.64,
                nose_z_m=-0.4,
                cabin_start_x_m=5.5,
                cabin_z_m=0.2,
                tailcone_length_m=12.0,
                tail_z_m=1.5,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(7.5, -7.5, 14.0, -14.0),
                # z_m calibrated for inboard engine pair (y=+/-7.5 m) at 0.35 m clearance.
                # Outboard pair (y=+/-14.0 m) clears by ~1.4 m with the same z_m.
                z_m=-1.94,
                inlet_x_offset_m=3.0,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.82,
            cruise_altitude_m=11887.2,
            mtow_kg=275_000,
            max_wing_area_m2=370.0,
            min_wing_loading_kg_m2=500.0,
            num_passengers=290,
            cargo_payload_kg=45_000.0,
            max_structural_payload_kg=48_600.0,  # A340-300: MZFW 178.0t - OEW ~129.4t
            dive_speed_m_s=200.0,
        ),
        performance=get_performance_preset("advanced_highlift_widebody").settings,
    )
)

# ===========================================================================
# Airbus A380-800 (super-jumbo quad)
# ===========================================================================
_register(
    AircraftPreset(
        name="A380-800",
        display_name="Airbus A380-800",
        description="Double-deck super-jumbo with Trent 900 engines.",
        engine_name="Trent 900",
        n_engines=4,
        design_vector=DesignVector(
            span_m=79.75,
            root_chord_m=23.00,
            break_chord_m=11.30,
            tip_chord_m=3.50,
            sweep_deg=33.5,
            tip_twist_deg=-2.5,
            wing_x_shift_m=-7.5,
            tail_scale=1.0,
            fuselage_length_m=72.72,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=26.70,
                root_z_m=-2.5,
                break_z_m=-0.4,
                tip_z_m=3.0,
                root_twist_deg=4.5,
                break_twist_deg=2.0,
                break_span_fraction=0.33,
                outboard_sweep_decrement_deg=2.5,
                root_airfoil="SC2-0714",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=11.0,
                hstab_z_m=1.5,
                hstab_root_chord_m=9.0,
                hstab_tip_chord_m=2.5,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(8.5, 12.5, 1.2),
                vstab_offset_from_tail_m=13.0,
                vstab_z_m=2.5,
                vstab_root_chord_m=11.0,
                vstab_tip_chord_m=3.5,
                vstab_tip_le_m=(10.0, 0.0, 11.0),
            ),
            fuselage=FuselageConfig(
                diameter_m=7.14,
                height_m=8.41,
                nose_z_m=-0.6,
                cabin_start_x_m=7.0,
                cabin_z_m=0.3,
                tailcone_length_m=15.0,
                tail_z_m=2.0,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(10.0, -10.0, 18.5, -18.5),
                # z_m calibrated for inboard pair (y=+/-10.0 m) at 0.35 m clearance.
                # Outboard pair (y=+/-18.5 m) clears by ~1.4 m with the same z_m.
                z_m=-3.14,
                inlet_x_offset_m=4.5,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.85,
            cruise_altitude_m=11887.2,
            mtow_kg=560_000,
            max_wing_area_m2=855.0,
            min_wing_loading_kg_m2=450.0,
            num_passengers=525,
            cargo_payload_kg=150_000.0,
            max_structural_payload_kg=84_000.0,  # A380-800: MZFW ~361t - OEW ~277t
            dive_speed_m_s=210.0,
        ),
        performance=get_performance_preset("advanced_highlift_widebody").settings,
    )
)

# ===========================================================================
# Boeing 787-9 Dreamliner (long-range twin)
# ===========================================================================
_register(
    AircraftPreset(
        name="B787-9",
        display_name="Boeing 787-9 Dreamliner",
        description="Long-range composite widebody twin with GEnx-1B engines.",
        engine_name="GEnx-1B",
        n_engines=2,
        design_vector=DesignVector(
            span_m=60.12,
            root_chord_m=12.60,
            break_chord_m=6.50,
            tip_chord_m=1.60,
            sweep_deg=32.2,
            tip_twist_deg=-2.0,
            wing_x_shift_m=-1.5,
            tail_scale=1.0,
            fuselage_length_m=62.81,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=21.00,
                root_z_m=-1.8,
                break_z_m=-0.2,
                tip_z_m=2.5,
                root_twist_deg=3.5,
                break_twist_deg=1.5,
                break_span_fraction=0.35,
                outboard_sweep_decrement_deg=2.0,
                root_airfoil="sc20614",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=9.5,
                hstab_z_m=1.0,
                hstab_root_chord_m=6.5,
                hstab_tip_chord_m=1.8,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(6.0, 9.5, 0.8),
                vstab_offset_from_tail_m=10.5,
                vstab_z_m=1.8,
                vstab_root_chord_m=8.0,
                vstab_tip_chord_m=2.8,
                vstab_tip_le_m=(7.5, 0.0, 8.5),
            ),
            fuselage=FuselageConfig(
                diameter_m=5.94,
                nose_z_m=-0.4,
                cabin_start_x_m=5.5,
                cabin_z_m=0.2,
                tailcone_length_m=12.0,
                tail_z_m=1.5,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(9.5, -9.5),
                # z_m calibrated to give ~0.35 m clearance below the wing lower surface.
                z_m=-2.55,
                inlet_x_offset_m=3.5,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.85,
            cruise_altitude_m=11887.2,
            mtow_kg=254_000,
            max_wing_area_m2=385.0,
            min_wing_loading_kg_m2=480.0,
            num_passengers=290,
            cargo_payload_kg=55_000.0,
            max_structural_payload_kg=52_600.0,  # B787-9: MZFW 181.4t - OEW 128.8t
            dive_speed_m_s=210.0,
        ),
        # Modern widebody high-lift (triple-slotted flaps + slats): higher take-off
        # CLmax so VR/V2 match the real 787-9 (~163-175 kt) instead of ~10 kt high.
        performance=get_performance_preset("advanced_highlift_widebody").settings,
    )
)

# ===========================================================================
# Airbus A320-200 (short/medium-range narrow-body)
# ===========================================================================
_register(
    AircraftPreset(
        name="A320-200",
        display_name="Airbus A320-200",
        description="Short/medium-range narrow-body twin with LEAP-1A engines.",
        engine_name="LEAP-1A",
        n_engines=2,
        design_vector=DesignVector(
            span_m=35.80,
            root_chord_m=6.10,
            break_chord_m=3.80,
            tip_chord_m=1.20,
            sweep_deg=25.0,
            tip_twist_deg=-1.5,
            wing_x_shift_m=0.0,
            tail_scale=1.0,
            fuselage_length_m=37.57,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=12.90,
                root_z_m=-1.2,
                break_z_m=-0.2,
                tip_z_m=1.5,
                root_twist_deg=3.0,
                break_twist_deg=1.0,
                break_span_fraction=0.37,
                outboard_sweep_decrement_deg=1.5,
                root_airfoil="sc20610",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=5.5,
                hstab_z_m=0.8,
                hstab_root_chord_m=4.0,
                hstab_tip_chord_m=1.2,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(3.5, 6.0, 0.5),
                vstab_offset_from_tail_m=6.5,
                vstab_z_m=1.2,
                vstab_root_chord_m=5.2,
                vstab_tip_chord_m=1.8,
                vstab_tip_le_m=(5.0, 0.0, 5.8),
            ),
            fuselage=FuselageConfig(
                diameter_m=3.95,
                nose_z_m=-0.3,
                cabin_start_x_m=3.5,
                cabin_z_m=0.1,
                tailcone_length_m=7.5,
                tail_z_m=1.0,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(5.5, -5.5),
                # z_m calibrated to give ~0.35 m clearance below the wing lower surface.
                z_m=-1.71,
                inlet_x_offset_m=2.5,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.78,
            cruise_altitude_m=11278.0,
            mtow_kg=78_000,
            max_wing_area_m2=130.0,
            min_wing_loading_kg_m2=500.0,
            num_passengers=150,
            cargo_payload_kg=18_000.0,
            max_structural_payload_kg=19_900.0,  # A320-200: MZFW 62.5t - OEW ~42.6t
            dive_speed_m_s=180.0,
        ),
        # Slats + single-slotted Fowler flaps give a real A320 a materially
        # higher CLmax than the generic "standard narrow-body" default -- without
        # this the computed V1/VR/V2 come out ~15-20 kt too high (e.g. VR=173 kt
        # vs a real ~148-153 kt), see performance_presets.py's "modern_narrowbody".
        performance=get_performance_preset("modern_narrowbody").settings,
    )
)

# ===========================================================================
# Airbus A220-300 (short/medium-range narrow-body)
# ===========================================================================
_register(
    AircraftPreset(
        name="A220-300",
        display_name="Airbus A220-300",
        description="Short/medium-range narrow-body twin with PW1500G geared turbofans.",
        engine_name="PW1500G",
        n_engines=2,
        design_vector=DesignVector(
            span_m=35.10,
            root_chord_m=5.80,
            break_chord_m=3.50,
            tip_chord_m=1.10,
            sweep_deg=25.0,
            tip_twist_deg=-1.5,
            wing_x_shift_m=0.0,
            tail_scale=1.0,
            fuselage_length_m=38.70,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=13.30,
                root_z_m=-1.0,
                break_z_m=-0.2,
                tip_z_m=1.5,
                root_twist_deg=3.0,
                break_twist_deg=1.0,
                break_span_fraction=0.37,
                outboard_sweep_decrement_deg=1.5,
                root_airfoil="SC2-0714",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=5.0,
                hstab_z_m=0.7,
                hstab_root_chord_m=3.8,
                hstab_tip_chord_m=1.1,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(3.2, 5.5, 0.5),
                vstab_offset_from_tail_m=6.0,
                vstab_z_m=1.0,
                vstab_root_chord_m=5.0,
                vstab_tip_chord_m=1.6,
                vstab_tip_le_m=(4.5, 0.0, 5.5),
            ),
            fuselage=FuselageConfig(
                diameter_m=3.50,
                nose_z_m=-0.2,
                cabin_start_x_m=3.2,
                cabin_z_m=0.1,
                tailcone_length_m=7.0,
                tail_z_m=0.8,
            ),
            engine=EngineConfig(
                spanwise_positions_m=(5.2, -5.2),
                # z_m calibrated to give ~0.35 m clearance below the wing lower surface.
                z_m=-1.71,
                inlet_x_offset_m=2.3,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.78,
            cruise_altitude_m=11278.0,
            mtow_kg=70_900,
            max_wing_area_m2=120.0,
            min_wing_loading_kg_m2=480.0,
            num_passengers=130,
            cargo_payload_kg=15_000.0,
            max_structural_payload_kg=18_700.0,  # A220-300: MZFW 55.8t - OEW 37.08t
            dive_speed_m_s=175.0,
        ),
        # A220-300 real OEW is 37.08 t; the global widebody-calibrated Torenbeek
        # fractions under-predict it (~34.3 t). A small modern narrowbody carries
        # a higher systems/furnishings fraction per MTOW, so bump both to match.
        mass_model=MassModelConfig(
            systems_mass_fraction=0.13, furnishings_mass_fraction=0.12
        ),
        # Full-span slats + double-slotted flaps -- same "modern narrowbody"
        # high-lift correction as the A320-200 (see its preset for the ~15-20 kt
        # V-speed overestimate this fixes).
        performance=get_performance_preset("modern_narrowbody").settings,
    )
)


# ===========================================================================
# McDonnell Douglas DC-10 (long-range trijet)
# ===========================================================================
_register(
    AircraftPreset(
        name="DC-10",
        display_name="McDonnell Douglas DC-10",
        description="Classic long-range trijet widebody with underwing and tail-mounted CF6-50 engines.",
        engine_name="CF6-50",
        n_engines=3,
        design_vector=DesignVector(
            span_m=50.41,
            root_chord_m=12.80,
            break_chord_m=7.80,
            tip_chord_m=1.80,
            sweep_deg=35.0,
            tip_twist_deg=-2.0,
            wing_x_shift_m=-3.25,
            tail_scale=1.0,
            fuselage_length_m=55.35,
            tail_x_shift_m=0.0,
            airfoil_thickness_scale=1.0,
            airfoil_camber_scale=1.0,
        ),
        geometry=GeometryConfig(
            wing=WingConfig(
                root_datum_x_m=21.40,
                root_z_m=-1.6,
                break_z_m=-0.3,
                tip_z_m=2.0,
                root_twist_deg=4.0,
                break_twist_deg=1.5,
                break_span_fraction=0.35,
                outboard_sweep_decrement_deg=2.0,
                root_airfoil="sc20612",
                tip_airfoil="sc20410",
            ),
            empennage=EmpennageConfig(
                tail_airfoil="naca0012",
                hstab_offset_from_tail_m=8.5,
                hstab_z_m=1.0,
                hstab_root_chord_m=7.2,
                hstab_tip_chord_m=2.0,
                hstab_root_twist_deg=-2.0,
                hstab_tip_twist_deg=-2.0,
                hstab_tip_le_m=(5.8, 8.5, 0.8),
                vstab_offset_from_tail_m=10.0,
                vstab_z_m=2.2,
                vstab_root_chord_m=10.5,
                vstab_tip_chord_m=3.8,
                vstab_tip_le_m=(7.5, 0.0, 9.5),
            ),
            fuselage=FuselageConfig(
                diameter_m=6.02,
                nose_z_m=-0.4,
                cabin_start_x_m=5.5,
                cabin_z_m=0.2,
                tailcone_length_m=11.5,
                tail_z_m=1.6,
            ),
            engine=EngineConfig(
                engine_name="CF6-50",
                spanwise_positions_m=(8.8, -8.8, 0.0),
                # z_m calibrated to give ~0.35 m clearance below the wing lower surface.
                # The centerline tail engine (y=0.0) is placed on the tailcone, not the wing.
                z_m=-2.12,
                inlet_x_offset_m=3.5,
            ),
        ),
        requirements=DesignRequirements(
            cruise_mach=0.82,
            cruise_altitude_m=10668.0,
            mtow_kg=259_450,
            max_wing_area_m2=338.8,
            min_wing_loading_kg_m2=500.0,
            num_passengers=250,
            cargo_payload_kg=65_000.0,
            max_structural_payload_kg=48_000.0,  # DC-10-30: MZFW ~182t - OEW ~121.2t
            dive_speed_m_s=210.0,
        ),
        performance=get_performance_preset("advanced_highlift_widebody").settings,
    )
)
