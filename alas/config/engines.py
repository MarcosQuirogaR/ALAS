# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Engine database for ALAS.

Stores specifications for real turbofan engines used across the aircraft
presets. Each engine provides nacelle sizing data (fan diameter -> nacelle
radius, nacelle length) so the geometry builder can auto-generate
correctly-scaled nacelle bodies of revolution.

Each engine also carries gas-turbine cycle data (overall pressure ratio,
turbine inlet temperature, fan pressure ratio) consumed by the SUAVE mission
bridge (``alas.integration.suave_vehicle``) to size a turbofan energy
network. Overall pressure ratio is manufacturer-published for most engines
(see per-field comments); fan pressure ratio and turbine inlet temperature
are not publicly published for any of these engines and are engineering
estimates following the well-known inverse correlation between bypass ratio
and fan pressure ratio -- appropriate for conceptual-design-level mission
analysis, not claimed as certified data.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Tuple


@dataclass(frozen=True)
class EngineSpec:
    """Specification for a single turbofan engine model."""

    name: str  # Display name, e.g. "GE9X"
    manufacturer: str  # "GE Aviation", "Rolls-Royce", etc.
    thrust_kn: float  # Max rated take-off thrust per engine (kN)
    fan_diameter_m: float  # Fan diameter (m)
    bypass_ratio: float  # Bypass ratio (-)
    nacelle_length_m: float  # Approximate nacelle length (m)
    nacelle_max_radius_m: float  # Max nacelle external radius (m)
    overall_pressure_ratio: float = 35.0  # Overall pressure ratio (-)
    turbine_inlet_temp_k: float = (
        1650.0  # Combustor exit / turbine inlet temp (K), estimated
    )
    fan_pressure_ratio: float = 1.5  # Fan pressure ratio (-), estimated
    cruise_tsfc_kg_kgf_hr: float = 0.55  # Cruise thrust-specific fuel consumption
    # (kg fuel / (kgf thrust . hr)), consumed by the Breguet payload-range diagram
    # (reporting/visualization.py::figure_payload_range). Not publicly published
    # per-engine; estimated from each family's typical cruise-TSFC class and
    # publicized relative-efficiency claims -- same disclaimer as the OPR/FPR/
    # turbine-inlet-temp estimates above, appropriate for conceptual design.

    def nacelle_profile(self) -> List[Tuple[float, float]]:
        """Generate a realistic nacelle body-of-revolution profile.

        Returns a list of (x_station_m, radius_fraction) pairs where
        radius_fraction is relative to ``nacelle_max_radius_m``.
        The profile approximates a real high-bypass nacelle:
          - Inlet lip (blunt leading edge)
          - Fan cowl (max diameter)
          - Core cowl taper
          - Nozzle exit
        """
        L = self.nacelle_length_m
        return [
            (0.0, 0.40),  # inlet lip (blunt, not a sharp point)
            (0.08 * L, 0.92),  # rapid expansion to fan cowl
            (0.15 * L, 1.00),  # max diameter at fan cowl
            (0.55 * L, 1.00),  # constant-section core cowl
            (0.75 * L, 0.82),  # begin taper
            (L, 0.45),  # nozzle exit
        ]


# ---------------------------------------------------------------------------
# Engine Database
# ---------------------------------------------------------------------------

ENGINE_DATABASE: Dict[str, EngineSpec] = {}


def _register(spec: EngineSpec) -> None:
    ENGINE_DATABASE[spec.name] = spec


# --- GE9X (Boeing 777X) ---------------------------------------------------
_register(
    EngineSpec(
        name="GE9X",
        manufacturer="GE Aerospace",
        thrust_kn=467.0,  # 105,000 lbf
        fan_diameter_m=3.40,  # 134 in
        bypass_ratio=10.0,
        nacelle_length_m=7.80,
        nacelle_max_radius_m=2.10,
        overall_pressure_ratio=60.0,  # public (GE/Boeing press materials)
        turbine_inlet_temp_k=1670.0,  # estimated, modern high-OPR class
        fan_pressure_ratio=1.45,  # estimated, very low FPR / high BPR
        cruise_tsfc_kg_kgf_hr=0.50,  # estimated, best-in-class (~10% better than GE90-115B)
    )
)

# --- CFM56-5C (Airbus A340) -----------------------------------------------
_register(
    EngineSpec(
        name="CFM56-5C",
        manufacturer="CFM International",
        thrust_kn=151.0,  # 34,000 lbf
        fan_diameter_m=1.836,  # 72.3 in
        bypass_ratio=6.6,
        nacelle_length_m=4.00,
        nacelle_max_radius_m=1.10,
        overall_pressure_ratio=38.0,  # public (EASA TCDS E.003, max climb)
        turbine_inlet_temp_k=1600.0,  # estimated, 1990s-era technology
        fan_pressure_ratio=1.65,  # estimated, moderate BPR
        cruise_tsfc_kg_kgf_hr=0.58,  # estimated, typical CFM56-family class
    )
)

# --- Rolls-Royce Trent 900 (Airbus A380) ----------------------------------
_register(
    EngineSpec(
        name="Trent 900",
        manufacturer="Rolls-Royce",
        thrust_kn=374.0,  # 84,000 lbf
        fan_diameter_m=2.946,  # 116 in
        bypass_ratio=8.5,
        nacelle_length_m=6.80,
        nacelle_max_radius_m=1.80,
        overall_pressure_ratio=39.0,  # public (Rolls-Royce)
        turbine_inlet_temp_k=1650.0,  # estimated
        fan_pressure_ratio=1.55,  # estimated
        cruise_tsfc_kg_kgf_hr=0.54,  # estimated, typical Trent-family class
    )
)

# --- GEnx-1B (Boeing 787) -------------------------------------------------
_register(
    EngineSpec(
        name="GEnx-1B",
        manufacturer="GE Aerospace",
        thrust_kn=339.0,  # 76,100 lbf
        fan_diameter_m=2.822,  # 111.1 in
        bypass_ratio=9.6,
        nacelle_length_m=5.80,
        nacelle_max_radius_m=1.70,
        overall_pressure_ratio=45.0,  # public components (HPC 23:1 x LPC 1.3 x fan 1.5)
        turbine_inlet_temp_k=1650.0,  # estimated
        fan_pressure_ratio=1.50,  # public (GEnx-1B70)
        cruise_tsfc_kg_kgf_hr=0.52,  # estimated (GE claims ~15% better burn than CF6)
    )
)

# --- CFM LEAP-1A (Airbus A320neo) -----------------------------------------
_register(
    EngineSpec(
        name="LEAP-1A",
        manufacturer="CFM International",
        thrust_kn=120.64,  # 27,120 lbf (LEAP-1A26 standard A320neo rating)
        fan_diameter_m=1.981,  # 78 in
        bypass_ratio=11.0,
        nacelle_length_m=3.60,
        nacelle_max_radius_m=1.15,
        overall_pressure_ratio=40.0,  # public (CFM)
        turbine_inlet_temp_k=1650.0,  # estimated, modern technology
        fan_pressure_ratio=1.40,  # estimated, low FPR / high BPR
        cruise_tsfc_kg_kgf_hr=0.52,  # estimated (CFM claims ~15% better burn than CFM56)
    )
)

# --- Pratt & Whitney PW1500G (Airbus A220) ---------------------------------
_register(
    EngineSpec(
        name="PW1500G",
        manufacturer="Pratt & Whitney",
        thrust_kn=104.5,  # 23,300 lbf (mid-range)
        fan_diameter_m=1.854,  # 73 in
        bypass_ratio=12.0,
        nacelle_length_m=3.40,
        nacelle_max_radius_m=1.08,
        overall_pressure_ratio=35.0,  # ESTIMATED -- not publicly published for this
        # geared-turbofan variant; in line with the
        # PW1000G family's typical 35-40:1 class
        turbine_inlet_temp_k=1650.0,  # estimated
        fan_pressure_ratio=1.35,  # estimated, geared low-FPR architecture
        cruise_tsfc_kg_kgf_hr=0.50,  # estimated (P&W claims ~16% better burn, geared architecture)
    )
)


# --- GE CF6-50 (McDonnell Douglas DC-10) -----------------------------------
_register(
    EngineSpec(
        name="CF6-50",
        manufacturer="GE Aerospace",
        thrust_kn=227.0,  # 51,000 lbf
        fan_diameter_m=2.19,  # 86.4 in
        bypass_ratio=4.4,
        nacelle_length_m=4.50,
        nacelle_max_radius_m=1.30,
        overall_pressure_ratio=29.3,  # public (GE)
        turbine_inlet_temp_k=1500.0,  # estimated, 1970s-era technology
        fan_pressure_ratio=1.70,  # estimated, low BPR / higher FPR
        cruise_tsfc_kg_kgf_hr=0.63,  # estimated, older/lower-BPR 1970s-era class
    )
)


def get_engine(name: str) -> EngineSpec:
    """Look up an engine by name. Raises KeyError if not found."""
    if name not in ENGINE_DATABASE:
        raise KeyError(f"Unknown engine '{name}'. Available: {sorted(ENGINE_DATABASE)}")
    return ENGINE_DATABASE[name]


def available_engines() -> List[str]:
    """Return sorted list of all registered engine names."""
    return sorted(ENGINE_DATABASE.keys())
