# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Airfoil library and parametric airfoil shaping.

This module is the single home for airfoil handling, which in the reference
scripts was duplicated across files. It provides:

* :class:`AirfoilLibrary` -- resolve airfoils by name (NACA series or a built-in
  reference section) or load a ``.dat`` file.
* :func:`apply_bumps` -- additive Hicks-Henne-style bump functions on the upper
  and lower surfaces (local shape control).
* :func:`morph_airfoil` -- global thickness/camber scaling via a
  thickness+camber decomposition.
* :func:`build_section` -- the high-level entry point that applies bumps then
  morphing to produce the wing's working section from a :class:`DesignVector`.

All shaping is parametric and driven by the design vector, so no airfoil shape
is hardcoded into the optimization loop.
"""

from __future__ import annotations

import threading
from pathlib import Path
from typing import Sequence

import aerosandbox as asb
import aerosandbox.numpy as np

from ..config.design_variables import DesignVector
from ..data.airfoil_data import NAMED_COORDINATES

# Default chordwise centres (x/c) of the four bump functions. These define the
# *scheme* (where local shape control acts), not a specific design; the design
# vector sets the bump amplitudes.
_BUMP_CENTERS = {
    "upper_front": 0.25,
    "upper_rear": 0.75,
    "lower_mid": 0.40,
    "lower_rear": 0.85,
}


class AirfoilLibrary:
    """Resolves airfoil names/files into AeroSandbox ``Airfoil`` objects."""

    _zip_airfoils = (
        None  # Cache map of {lowercase_name: (original_case_stem, zip_entry_name)}
    )
    _zip_path = Path(__file__).parent.parent / "data" / "coord_seligFmt.zip"
    # Guards first-time population of _zip_airfoils. Without this, two threads
    # racing into _init_zip() (e.g. the GUI's live 3D preview and a pipeline
    # run on a worker thread, or two concurrent FullAnalysis calls once the
    # pipeline parallelizes its post-optimization stages) could see the cache
    # as "already initialized" (non-None) while it is still empty/partially
    # populated, silently falling back to the wrong airfoil.
    _zip_lock = threading.Lock()

    @classmethod
    def _init_zip(cls):
        import zipfile

        if cls._zip_airfoils is not None:
            return
        with cls._zip_lock:
            if (
                cls._zip_airfoils is not None
            ):  # re-check: another thread may have won the race
                return
            zip_airfoils = {}
            if cls._zip_path.exists():
                try:
                    with zipfile.ZipFile(cls._zip_path, "r") as z:
                        for name in z.namelist():
                            if name.startswith("coord_seligFmt/") and name.endswith(
                                ".dat"
                            ):
                                stem = Path(name).stem
                                zip_airfoils[stem.lower()] = (stem, name)
                except Exception as e:
                    print(f"Warning: Failed to index airfoil zip archive: {e}")
            cls._zip_airfoils = zip_airfoils  # published only once fully populated

    @staticmethod
    def normalize_coordinates(coords: np.ndarray) -> np.ndarray:
        """Ensure coordinates are ordered: Upper TE -> LE -> Lower TE."""
        import numpy as std_np

        if len(coords) < 3:
            return coords

        # Find leading edge
        le_idx = int(std_np.argmin(coords[:, 0]))
        seg1 = coords[: le_idx + 1]
        seg2 = coords[le_idx:]

        s1_x, s1_y = seg1[:, 0], seg1[:, 1]
        s2_x, s2_y = seg2[:, 0], seg2[:, 1]

        idx1 = std_np.argsort(s1_x)
        idx2 = std_np.argsort(s2_x)

        y1_mid = float(std_np.interp(0.5, s1_x[idx1], s1_y[idx1]))
        y2_mid = float(std_np.interp(0.5, s2_x[idx2], s2_y[idx2]))

        if y1_mid >= y2_mid:
            upper_seg = seg1
            lower_seg = seg2
        else:
            upper_seg = seg2
            lower_seg = seg1

        upper_sorted = upper_seg[std_np.argsort(upper_seg[:, 0])[::-1]]
        lower_sorted = lower_seg[std_np.argsort(lower_seg[:, 0])]

        if std_np.linalg.norm(upper_sorted[-1] - lower_sorted[0]) < 1e-6:
            coords_new = std_np.vstack((upper_sorted, lower_sorted[1:]))
        else:
            coords_new = std_np.vstack((upper_sorted, lower_sorted))

        return coords_new

    @classmethod
    def get(cls, name: str) -> asb.Airfoil:
        """Return an airfoil by name.

        Recognises Selig zip database airfoils, built-in reference sections, and
        otherwise defers to AeroSandbox's name resolution.
        """
        import numpy as std_np

        cls._init_zip()
        name_lower = name.lower()

        # 1. Check zip file library
        if cls._zip_airfoils and name_lower in cls._zip_airfoils:
            import zipfile

            stem, zip_name = cls._zip_airfoils[name_lower]
            try:
                with zipfile.ZipFile(cls._zip_path, "r") as z:
                    with z.open(zip_name) as f:
                        lines = f.read().decode("utf-8").splitlines()
                        coords = []
                        for line in lines[1:]:
                            parts = line.strip().split()
                            if len(parts) >= 2:
                                try:
                                    coords.append([float(parts[0]), float(parts[1])])
                                except ValueError:
                                    continue
                        coords = std_np.array(coords)
                        coords = cls.normalize_coordinates(coords)
                        return asb.Airfoil(name=stem, coordinates=coords)
            except Exception as e:
                print(f"Warning: Failed to load '{name}' from zip: {e}")

        # 2. Check built-in named coords
        if name in NAMED_COORDINATES:
            coords = cls.normalize_coordinates(std_np.array(NAMED_COORDINATES[name]))
            return asb.Airfoil(name=name, coordinates=coords)

        # 3. Fallback to AeroSandbox default search (e.g. NACA)
        af = asb.Airfoil(name)
        if hasattr(af, "coordinates") and af.coordinates is not None:
            af.coordinates = cls.normalize_coordinates(af.coordinates)
        return af

    @classmethod
    def get_available_airfoils(cls) -> list[str]:
        """Return list of all indexed Selig zip + built-in airfoil names."""
        cls._init_zip()
        names = list(NAMED_COORDINATES.keys())
        if cls._zip_airfoils:
            names.extend([v[0] for v in cls._zip_airfoils.values()])
        return sorted(list(set(names)))

    @classmethod
    def from_dat(cls, path: str | Path) -> asb.Airfoil:
        """Load an airfoil from a standard Selig ``.dat`` file."""
        import numpy as std_np

        path = Path(path)
        coords = std_np.loadtxt(path, skiprows=1)
        coords = cls.normalize_coordinates(coords)
        return asb.Airfoil(name=path.stem, coordinates=coords)


def apply_bumps(
    coords: np.ndarray,
    bumps_upper: Sequence[float],
    bumps_lower: Sequence[float],
    n_points_per_side: int = 120,
) -> asb.Airfoil:
    """Add Hicks-Henne-style bumps to the upper and lower surfaces.

    Each bump is ``amp * sin(pi*x)^w * exp(-10*(x - x_center)^2)`` -- a localised
    perturbation that vanishes at the leading and trailing edges. ``bumps_upper``
    and ``bumps_lower`` are 2-element amplitude vectors for the front/rear and
    mid/rear control stations respectively.
    """
    af = asb.Airfoil("scratch", coordinates=coords).repanel(
        n_points_per_side=n_points_per_side
    )

    upper = af.upper_coordinates()
    lower = af.lower_coordinates()
    x_up, y_up = upper[:, 0], upper[:, 1]

    # AeroSandbox returns the upper surface LE->TE or TE->LE depending on version;
    # normalise to ascending-x for the bump math, then restore orientation.
    flip_up = x_up[0] > x_up[-1]
    if flip_up:
        x_up, y_up = x_up[::-1], y_up[::-1]
    x_lo, y_lo = lower[:, 0], lower[:, 1]

    def add_bump(x_arr, y_arr, amp, center_x, width=2.5):
        if amp == 0.0:
            return y_arr
        x_safe = np.clip(x_arr, 0.0, 1.0)
        perturbation = (
            amp
            * np.sin(np.pi * x_safe) ** width
            * np.exp(-10 * (x_safe - center_x) ** 2)
        )
        mask = (x_arr > 0.01) & (x_arr < 0.99)
        y_arr = np.where(mask, y_arr + perturbation, y_arr)
        return y_arr

    y_up = add_bump(x_up, y_up, bumps_upper[0], _BUMP_CENTERS["upper_front"])
    y_up = add_bump(x_up, y_up, bumps_upper[1], _BUMP_CENTERS["upper_rear"])
    y_lo = add_bump(x_lo, y_lo, bumps_lower[0], _BUMP_CENTERS["lower_mid"])
    y_lo = add_bump(x_lo, y_lo, bumps_lower[1], _BUMP_CENTERS["lower_rear"])

    upper_new = (
        np.column_stack((x_up[::-1], y_up[::-1]))
        if flip_up
        else np.column_stack((x_up, y_up))
    )
    lower_new = np.column_stack((x_lo, y_lo))

    if np.linalg.norm(upper_new[-1] - lower_new[0]) < 1e-6:
        coords_new = np.vstack((upper_new, lower_new[1:]))
    else:
        coords_new = np.vstack((upper_new, lower_new))
    return asb.Airfoil("bumped", coordinates=coords_new)


def morph_airfoil(
    coords: np.ndarray, thickness_scale: float, camber_scale: float, n_points: int = 150
) -> asb.Airfoil:
    """Scale an airfoil's thickness and camber independently.

    The section is decomposed into a mean camber line and a thickness
    distribution; each is scaled, then the surfaces are reassembled. This lets
    the optimizer thicken/thin (wing-box volume, wave drag) and load/unload
    (lift, pitching moment) the section as separate levers.
    """
    le_idx = np.argmin(coords[:, 0])
    upper = coords[: le_idx + 1]
    lower = coords[le_idx:]

    x_grid = np.linspace(0, 1, n_points)

    u_x, u_y = upper[:, 0], upper[:, 1]
    idx_u = np.argsort(u_x)
    y_upper = np.interp(x_grid, u_x[idx_u], u_y[idx_u])

    l_x, l_y = lower[:, 0], lower[:, 1]
    idx_l = np.argsort(l_x)
    y_lower = np.interp(x_grid, l_x[idx_l], l_y[idx_l])

    thickness = (y_upper - y_lower) * thickness_scale
    camber = ((y_upper + y_lower) / 2) * camber_scale

    new_y_u = camber + thickness / 2
    new_y_l = camber - thickness / 2

    c_upper = np.column_stack((x_grid[::-1], new_y_u[::-1]))
    c_lower = np.column_stack((x_grid[1:], new_y_l[1:]))
    final_coords = np.vstack((c_upper, c_lower))
    return asb.Airfoil("morphed", coordinates=final_coords)


def build_section(dv: DesignVector, base_coords: np.ndarray) -> asb.Airfoil:
    """Produce the working wing section from a design vector.

    Pipeline: base reference section -> local bumps -> global thickness/camber
    morphing. This is the section used at the wing root and break.
    """
    bumped = apply_bumps(
        base_coords,
        bumps_upper=[dv.bump_upper_front, dv.bump_upper_rear],
        bumps_lower=[dv.bump_lower_mid, dv.bump_lower_rear],
    )
    return morph_airfoil(
        bumped.coordinates,
        thickness_scale=dv.airfoil_thickness_scale,
        camber_scale=dv.airfoil_camber_scale,
    )
