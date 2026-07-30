# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Structural material database for the wingbox FEM.

Mirrors :mod:`alas.config.engines`'s pattern exactly: a small, named,
immutable database of real material properties that
:class:`alas.config.structures_config.StructuresConfig` selects from by
name (``skin_material``, ``spar_web_material``, ``spar_cap_material``,
``rib_material``) -- the wingbox sizing/mesh code reads the resolved
:class:`MaterialSpec`, never the raw name.

``f_allow_pa`` is a single design allowable stress (not a separate
tension/compression pair) -- the same simplification the reference scripts
used (``SIG_ADM``/``SIG_CFRP`` constants), appropriate for a preliminary
strength-based sizing pass rather than a certified stress analysis.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List


@dataclass(frozen=True)
class MaterialSpec:
    """Specification for a single structural material."""

    name: str  # Display name, e.g. "Al 7075-T6"
    category: str  # "metallic" or "composite" -- informational only
    e_pa: float  # Young's modulus (Pa)
    nu: float  # Poisson's ratio (-)
    rho_kg_m3: float  # Density (kg/m^3)
    f_allow_pa: float  # Design allowable stress (Pa), FS already excluded
    # (StructuresConfig.additional_safety_factor applies on top)

    @property
    def g_pa(self) -> float:
        """Shear modulus, from isotropic E/nu (not used standalone for CFRP's
        true orthotropic behaviour, but consistent with the reference
        scripts' own isotropic MAT1 treatment of the UD cap material)."""
        return self.e_pa / (2.0 * (1.0 + self.nu))


MATERIAL_DATABASE: Dict[str, MaterialSpec] = {}


def _register(spec: MaterialSpec) -> None:
    MATERIAL_DATABASE[spec.name] = spec


# --- Al 7075-T6 (spar webs / ribs / skin -- high-strength aluminum) --------
_register(
    MaterialSpec(
        name="Al 7075-T6",
        category="metallic",
        e_pa=71.0e9,
        nu=0.33,
        rho_kg_m3=2810.0,
        f_allow_pa=380.0e6,  # matches the original reference sizing's validated SIG_ADM
    )
)

# --- Al 2024-T3 (skin / rib alternative -- higher fatigue/damage tolerance) -
_register(
    MaterialSpec(
        name="Al 2024-T3",
        category="metallic",
        e_pa=73.1e9,
        nu=0.33,
        rho_kg_m3=2780.0,
        f_allow_pa=324.0e6,
    )
)

# --- CFRP UD (spar caps -- unidirectional carbon-fiber, A350/B777X class) --
_register(
    MaterialSpec(
        name="CFRP UD",
        category="composite",
        e_pa=120.0e9,
        nu=0.30,
        rho_kg_m3=1600.0,
        f_allow_pa=900.0e6,  # matches the original reference's validated SIG_CFRP
    )
)

# --- Steel 4130 (root reinforcement / high point-load regions) ------------
_register(
    MaterialSpec(
        name="Steel 4130",
        category="metallic",
        e_pa=210.0e9,
        nu=0.30,
        rho_kg_m3=7850.0,
        f_allow_pa=460.0e6,
    )
)

# --- Ti-6Al-4V (high-temperature / high-strength fittings) -----------------
_register(
    MaterialSpec(
        name="Ti-6Al-4V",
        category="metallic",
        e_pa=113.8e9,
        nu=0.34,
        rho_kg_m3=4430.0,
        f_allow_pa=880.0e6,
    )
)

# The additions below broaden the selectable palette to the alloys/laminates a
# preliminary wingbox trade study actually reaches for. Allowables follow the
# same convention as the entries above: a single design allowable in the
# yield-strength class (MMPDS/handbook typical values, rounded conservatively),
# with StructuresConfig.additional_safety_factor applied on top. None of these
# is a default anywhere -- purely additive options.

# --- Al 7050-T7451 (thick-plate spar/rib stock; better stress-corrosion
#     resistance than 7075 at near-identical stiffness/strength) -------------
_register(
    MaterialSpec(
        name="Al 7050-T7451",
        category="metallic",
        e_pa=71.7e9,
        nu=0.33,
        rho_kg_m3=2830.0,
        f_allow_pa=440.0e6,
    )
)

# --- Al 6061-T6 (GA/light-aircraft class; weldable, inexpensive) ------------
_register(
    MaterialSpec(
        name="Al 6061-T6",
        category="metallic",
        e_pa=68.9e9,
        nu=0.33,
        rho_kg_m3=2700.0,
        f_allow_pa=240.0e6,
    )
)

# --- Al-Li 2195-T8 (aluminum-lithium; ~5% lighter and stiffer than 2xxx/7xxx,
#     used on Falcon/Shuttle ET tankage and modern lower-wing skins) ---------
_register(
    MaterialSpec(
        name="Al-Li 2195-T8",
        category="metallic",
        e_pa=76.0e9,
        nu=0.33,
        rho_kg_m3=2700.0,
        f_allow_pa=480.0e6,
    )
)

# --- CFRP quasi-isotropic laminate (skins/webs; strain-limited allowable,
#     far below UD caps -- selecting this for skin_material vs CFRP UD for
#     spar_cap_material mirrors real composite-wing practice) ----------------
_register(
    MaterialSpec(
        name="CFRP QI",
        category="composite",
        e_pa=55.0e9,
        nu=0.31,
        rho_kg_m3=1580.0,
        f_allow_pa=450.0e6,
    )
)

# --- GLARE 3 (glass-fiber/aluminum fiber-metal laminate, A380 upper fuselage
#     class; excellent fatigue/impact, modest static strength) ---------------
_register(
    MaterialSpec(
        name="GLARE 3",
        category="composite",
        e_pa=58.0e9,
        nu=0.33,
        rho_kg_m3=2520.0,
        f_allow_pa=280.0e6,
    )
)

# --- Steel 300M (ultra-high-strength landing-gear/fitting steel) ------------
_register(
    MaterialSpec(
        name="Steel 300M",
        category="metallic",
        e_pa=205.0e9,
        nu=0.29,
        rho_kg_m3=7830.0,
        f_allow_pa=1400.0e6,
    )
)


def get_material(name: str) -> MaterialSpec:
    """Look up a material by name. Raises KeyError if not found."""
    if name not in MATERIAL_DATABASE:
        raise KeyError(
            f"Unknown material '{name}'. Available: {sorted(MATERIAL_DATABASE)}"
        )
    return MATERIAL_DATABASE[name]


def available_materials() -> List[str]:
    """Return sorted list of all registered material names."""
    return sorted(MATERIAL_DATABASE.keys())
