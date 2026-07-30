# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Airport database for ALAS performance analysis.

Provides curated entries for 10 major international hubs and 10 challenging
high-altitude or short-field airports, plus a helper for custom entries.
Data sourced from official AIP publications and ICAO aerodrome charts.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List


@dataclass
class Airport:
    """Single aerodrome entry used for performance calculations."""

    name: str  # Display name, e.g. "London Heathrow (EGLL)"
    icao: str  # 4-letter ICAO code
    elevation_m: float  # Field elevation above MSL (m)
    toda_m: float  # Take-Off Distance Available (longest runway, m)
    lda_m: float  # Landing Distance Available (longest runway, m)
    isa_deviation_c: float = 0.0  # Hot-day ISA temperature offset (°C)
    notes: str = ""  # Brief description
    latitude_deg: float = 0.0  # Reference-point latitude (+N / -S), used for routing
    longitude_deg: float = 0.0  # Reference-point longitude (+E / -W), used for routing

    @property
    def display(self) -> str:
        return self.name


# ---------------------------------------------------------------------------
# Airport Database  (10 major + 10 challenging)
# ---------------------------------------------------------------------------

AIRPORTS: List[Airport] = [
    # --- Major international hubs ---
    Airport(
        "London Heathrow (EGLL)",
        "EGLL",
        elevation_m=25,
        toda_m=3902,
        lda_m=3902,
        notes="Busiest airport in Europe",
        latitude_deg=51.4700,
        longitude_deg=-0.4543,
    ),
    Airport(
        "Paris CDG (LFPG)",
        "LFPG",
        elevation_m=119,
        toda_m=4215,
        lda_m=4215,
        notes="Longest runway: 09L/27R, 4215 m",
        latitude_deg=49.0097,
        longitude_deg=2.5479,
    ),
    Airport(
        "Frankfurt (EDDF)",
        "EDDF",
        elevation_m=364,
        toda_m=4000,
        lda_m=4000,
        notes="Central European hub, moderate elevation",
        latitude_deg=50.0379,
        longitude_deg=8.5622,
    ),
    Airport(
        "Madrid Barajas (LEMD)",
        "LEMD",
        elevation_m=610,
        toda_m=4100,
        lda_m=4100,
        notes="Significant elevation — density penalty",
        latitude_deg=40.4719,
        longitude_deg=-3.5626,
    ),
    Airport(
        "Dubai (OMDB)",
        "OMDB",
        elevation_m=19,
        toda_m=4000,
        lda_m=4000,
        isa_deviation_c=15,
        notes="Hot-day ops standard; ISA+15 typical summer",
        latitude_deg=25.2532,
        longitude_deg=55.3657,
    ),
    Airport(
        "Singapore Changi (WSSS)",
        "WSSS",
        elevation_m=7,
        toda_m=4000,
        lda_m=4000,
        isa_deviation_c=8,
        notes="Tropical, near sea level",
        latitude_deg=1.3644,
        longitude_deg=103.9915,
    ),
    Airport(
        "Denver International (KDEN)",
        "KDEN",
        elevation_m=1655,
        toda_m=4877,
        lda_m=4877,
        notes="High-elevation hub in the US; longest available runway",
        latitude_deg=39.8561,
        longitude_deg=-104.6737,
    ),
    Airport(
        "Mexico City (MMMX)",
        "MMMX",
        elevation_m=2230,
        toda_m=3985,
        lda_m=3985,
        isa_deviation_c=5,
        notes="High-and-hot; severe density penalty",
        latitude_deg=19.4363,
        longitude_deg=-99.0721,
    ),
    Airport(
        "Johannesburg OR Tambo (FAOR)",
        "FAOR",
        elevation_m=1700,
        toda_m=4418,
        lda_m=4418,
        isa_deviation_c=10,
        notes="High altitude + hot-day: classic challenging hub",
        latitude_deg=-26.1392,
        longitude_deg=28.2460,
    ),
    Airport(
        "Tokyo Narita (RJAA)",
        "RJAA",
        elevation_m=41,
        toda_m=4000,
        lda_m=4000,
        notes="Sea-level, ISA conditions nominal",
        latitude_deg=35.7720,
        longitude_deg=140.3929,
    ),
    # --- Challenging / high-altitude / short-field airports ---
    Airport(
        "Nairobi Jomo Kenyatta (HKJK)",
        "HKJK",
        elevation_m=1624,
        toda_m=4117,
        lda_m=4117,
        isa_deviation_c=8,
        notes="Classic hot-and-high reference case",
        latitude_deg=-1.3192,
        longitude_deg=36.9278,
    ),
    Airport(
        "Quito Mariscal Sucre (SEQM)",
        "SEQM",
        elevation_m=2813,
        toda_m=3970,
        lda_m=3970,
        isa_deviation_c=5,
        notes="Very high-altitude Andean airport",
        latitude_deg=-0.1292,
        longitude_deg=-78.3575,
    ),
    Airport(
        "Bogotá El Dorado (SKBO)",
        "SKBO",
        elevation_m=2548,
        toda_m=3800,
        lda_m=3800,
        isa_deviation_c=5,
        notes="High plateau; MTOW restrictions common",
        latitude_deg=4.7016,
        longitude_deg=-74.1469,
    ),
    Airport(
        "Addis Ababa Bole (HAAB)",
        "HAAB",
        elevation_m=2334,
        toda_m=3800,
        lda_m=3800,
        isa_deviation_c=10,
        notes="African highland hub; hot-day ops",
        latitude_deg=8.9778,
        longitude_deg=38.7993,
    ),
    Airport(
        "Kathmandu Tribhuvan (VNKT)",
        "VNKT",
        elevation_m=1337,
        toda_m=3050,
        lda_m=3050,
        isa_deviation_c=5,
        notes="Short runway at moderate elevation; mountainous terrain",
        latitude_deg=27.6966,
        longitude_deg=85.3591,
    ),
    Airport(
        "Geneva (LSGG)",
        "LSGG",
        elevation_m=430,
        toda_m=3900,
        lda_m=3250,
        notes="Asymmetric TODA/LDA; noise-restricted departure",
        latitude_deg=46.2381,
        longitude_deg=6.1090,
    ),
    Airport(
        "Innsbruck (LOWI)",
        "LOWI",
        elevation_m=581,
        toda_m=2750,
        lda_m=2000,
        notes="Alpine approach; very short LDA",
        latitude_deg=47.2602,
        longitude_deg=11.3439,
    ),
    Airport(
        "La Paz El Alto (SLLP)",
        "SLLP",
        elevation_m=4061,
        toda_m=4000,
        lda_m=3800,
        isa_deviation_c=10,
        notes="Highest commercial airport in the world",
        latitude_deg=-16.5133,
        longitude_deg=-68.1925,
    ),
    Airport(
        "Gibraltar (LXGB)",
        "LXGB",
        elevation_m=4,
        toda_m=1836,
        lda_m=1507,
        notes="Very short runway; road crosses active runway",
        latitude_deg=36.1512,
        longitude_deg=-5.3496,
    ),
    Airport(
        "Lukla Tenzing-Hillary (VNLK)",
        "VNLK",
        elevation_m=2860,
        toda_m=527,
        lda_m=527,
        notes="Extreme mountain strip — reference edge case only",
        latitude_deg=27.6869,
        longitude_deg=86.7311,
    ),
]

AIRPORT_NAMES: List[str] = [a.name for a in AIRPORTS]


def get_airport(name_or_icao: str) -> Airport:
    """Look up an airport by display name or ICAO code."""
    for a in AIRPORTS:
        if a.name == name_or_icao or a.icao == name_or_icao:
            return a
    raise KeyError(f"Airport '{name_or_icao}' not found in database.")


def make_custom_airport(
    name: str,
    elevation_m: float,
    toda_m: float,
    lda_m: float,
    isa_deviation_c: float = 0.0,
    latitude_deg: float = 0.0,
    longitude_deg: float = 0.0,
) -> Airport:
    """Build a one-off custom airport entry."""
    return Airport(
        name=name,
        icao="CUST",
        elevation_m=elevation_m,
        toda_m=toda_m,
        lda_m=lda_m,
        isa_deviation_c=isa_deviation_c,
        notes="Custom entry",
        latitude_deg=latitude_deg,
        longitude_deg=longitude_deg,
    )
