# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
3D textured-globe route visualization -- a Python port of ``route_globe.m``.

The MATLAB script rendered a textured sphere with a route drawn as a
mass-colored line, synchronizing a SimBrief KML's waypoints against a SUAVE
CSV by cumulative distance. This module does the same with
:class:`~alas.routing.route.Route` (which may come from a manual KML
import, the open-navdata airway graph, or a great-circle fallback) and
:class:`~alas.integration.suave_bridge.MissionResult`, rendered with
PyVista instead of MATLAB's ``surface``/``patch`` (true UV texture-mapping
onto a sphere; the sidecar's ``figures_extra.py`` grabs an off-screen
screenshot of it to serve to the desktop app as a static image).

Unlike the MATLAB original -- which only had the KML's static altitude --
this also projects the SUAVE-simulated altitude profile onto the route, since
ALAS actually has a real climb/cruise/descent trace to use.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional, Tuple

import numpy as np

from ..integration.assets import default_texture_path
from ..integration.suave_bridge import MissionResult
from ..routing.route import Route

R_EARTH_KM = 6371.0
_PATH_VISIBILITY_OFFSET_KM = 30.0  # lift the route slightly above the globe surface


def sync_mass_to_route(
    route: Route, mission: MissionResult
) -> Tuple[np.ndarray, np.ndarray]:
    """Project the SUAVE mission's mass and altitude onto the route's waypoints.

    Mirrors ``route_globe.m``'s distance-based synchronisation (lines 19-45):
    integrate true airspeed over time to get cumulative flown distance,
    normalise it to the route's total length, drop non-increasing samples
    (aircraft stationary/very slow), then linearly interpolate.
    """
    dist_route = route.cumulative_distance_m
    total_route_m = float(dist_route[-1]) if len(dist_route) else 0.0

    time_s = np.asarray(mission.time_s, dtype=float)
    tas = np.asarray(mission.tas_m_s, dtype=float)
    mass = np.asarray(mission.mass_kg, dtype=float)
    altitude = np.asarray(mission.altitude_m, dtype=float)

    if len(time_s) < 2 or total_route_m <= 0:
        return np.full(len(route.waypoints), mass[-1] if len(mass) else 0.0), np.full(
            len(route.waypoints), altitude[-1] if len(altitude) else 0.0
        )

    # Trapezoidal integration of TAS over time (mirrors MATLAB's cumtrapz).
    dist_csv = np.concatenate(
        ([0.0], np.cumsum(np.diff(time_s) * (tas[:-1] + tas[1:]) / 2.0))
    )
    if dist_csv[-1] <= 0:
        dist_csv = np.linspace(0.0, total_route_m, len(time_s))
    else:
        dist_csv = dist_csv / dist_csv[-1] * total_route_m

    # Keep only strictly-increasing distance samples (drop stationary/slow
    # points) so np.interp has a valid monotonic x-axis.
    keep = np.concatenate(([True], np.diff(dist_csv) > 0))
    dist_unique, mass_unique, alt_unique = dist_csv[keep], mass[keep], altitude[keep]

    mass_at_route = np.interp(dist_route, dist_unique, mass_unique)
    altitude_at_route = np.interp(dist_route, dist_unique, alt_unique)
    return mass_at_route, altitude_at_route


def _route_to_xyz(route: Route, altitude_m: np.ndarray) -> np.ndarray:
    lat = np.radians([wp.lat for wp in route.waypoints])
    lon = np.radians([wp.lon for wp in route.waypoints])
    r = (
        R_EARTH_KM
        + np.asarray(altitude_m, dtype=float) / 1000.0
        + _PATH_VISIBILITY_OFFSET_KM
    )
    x = r * np.cos(lat) * np.cos(lon)
    y = r * np.cos(lat) * np.sin(lon)
    z = r * np.sin(lat)
    return np.column_stack([x, y, z])


def build_globe_plotter(
    route: Route,
    mass_profile: Optional[np.ndarray] = None,
    altitude_profile: Optional[np.ndarray] = None,
    texture_path: Optional[Path] = None,
    off_screen: bool = False,
    plotter=None,
):
    """Build (or populate) a PyVista plotter showing the route on a textured 3D globe.

    If ``plotter`` is given, meshes are added to it directly instead of
    creating a new ``pv.Plotter`` -- this lets the GUI pass its
    ``pyvistaqt.QtInteractor`` straight through (it exposes the same
    ``add_mesh``/``add_point_labels``/``add_text`` API as ``pv.Plotter``)
    rather than building a throwaway off-screen plotter and copying actors.
    Otherwise returns a fresh, un-shown ``pv.Plotter`` for callers to
    ``.show()`` / ``.screenshot()`` (static export into the design report).

    ``mass_profile`` is optional: before a SUAVE mission has been run, the
    route's lateral path can still be shown (great-circle / navdata-airway /
    KML) as a plain-colored line; once mission data is available, passing the
    mass profile colors the route by total aircraft mass with a scalar bar,
    matching ``route_globe.m``'s visualization.
    """
    import pyvista as pv  # lazy: keep the optional gui/pyvista dependency out of headless imports
    import vtk

    if altitude_profile is None:
        altitude_profile = np.zeros(len(route.waypoints))
    texture_path = texture_path or default_texture_path()

    if plotter is None:
        plotter = pv.Plotter(off_screen=off_screen)

    sphere_source = vtk.vtkTexturedSphereSource()
    sphere_source.SetRadius(R_EARTH_KM)
    sphere_source.SetThetaResolution(120)
    sphere_source.SetPhiResolution(120)
    sphere_source.Update()
    sphere = pv.wrap(sphere_source.GetOutput())

    # vtkTexturedSphereSource assigns u=0 to the point at our-longitude=0 deg
    # (verified: point (R,0,0) -> tcoord u=0), i.e. it assumes the texture's
    # LEFT edge is the Greenwich meridian. The downloaded Earth texture
    # (NASA "Land Shallow Topo", download_earth_texture.py) is the opposite,
    # standard equirectangular convention: Greenwich at the image's CENTER
    # column, +/-180 deg at the left/right edges (confirmed by sampling
    # known-land pixels -- London/Dubai/Sahara only land under that
    # assumption, not vtkTexturedSphereSource's default). Shifting u by 0.5
    # realigns the two; without this every point/label lands ~180 deg of
    # longitude away from its real position (e.g. a London route point
    # appearing over the Bering Sea near Alaska).
    tcoords = sphere.active_texture_coordinates
    tcoords[:, 0] = (tcoords[:, 0] + 0.5) % 1.0
    sphere.active_texture_coordinates = tcoords

    setup_hints = []
    if Path(texture_path).exists():
        plotter.add_mesh(
            sphere, texture=pv.read_texture(str(texture_path)), smooth_shading=True
        )
    else:
        plotter.add_mesh(sphere, color="lightblue", smooth_shading=True)
        setup_hints.append("Get the textured Earth from Setup > External Tools")
    if route.source == "great_circle":
        from ..routing.navdata_graph import default_navdata_dir, navdata_available

        if not navdata_available(default_navdata_dir()):
            setup_hints.append("Get airway routing data from Setup > External Tools")

    points = _route_to_xyz(route, altitude_profile)
    poly = pv.PolyData()
    poly.points = points
    poly.lines = np.hstack([[len(points)], np.arange(len(points))])
    # The scalar array must be set on `poly` *before* the tube filter runs,
    # otherwise tube() has nothing to carry over onto the new tube surface
    # and add_mesh(..., scalars=...) fails with a missing-array KeyError.
    if mass_profile is not None:
        poly["Total Mass (kg)"] = np.asarray(mass_profile, dtype=float)
    tube = poly.tube(radius=R_EARTH_KM * 0.003)
    if mass_profile is not None:
        plotter.add_mesh(
            tube,
            scalars="Total Mass (kg)",
            cmap="jet",
            scalar_bar_args={"title": "Total Mass (kg)", "color": "black"},
        )
    else:
        plotter.add_mesh(tube, color="orange")

    origin_label = route.waypoints[0].ident or "DEP"
    dest_label = route.waypoints[-1].ident or "ARR"
    plotter.add_point_labels(
        [points[0] * 1.02, points[-1] * 1.02],
        [origin_label, dest_label],
        text_color="white",
        bold=True,
        point_size=10,
        shape=None,
    )

    idx_mid = len(points) // 2
    mid_text = f"Cruise\nAlt: {altitude_profile[idx_mid]:,.0f} m"
    if mass_profile is not None:
        mid_text += f"\nMass: {mass_profile[idx_mid]:,.0f} kg"
    plotter.add_mesh(
        pv.Sphere(radius=R_EARTH_KM * 0.006, center=points[idx_mid]), color="black"
    )
    plotter.add_point_labels(
        [points[idx_mid] * 1.02],
        [mid_text],
        text_color="black",
        font_size=10,
        shape_color="white",
        shape_opacity=0.7,
    )

    plotter.add_text(
        f"{origin_label} -> {dest_label}  ({route.source})", color="black", font_size=12
    )
    if setup_hints:
        plotter.add_text(
            "\n".join(setup_hints), position="lower_left", color="gray", font_size=9
        )
    plotter.set_background("white")
    return plotter
