# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Aircraft builder -- turns a (DesignVector, GeometryConfig) pair into an
AeroSandbox ``Airplane``.

This is the unified replacement for the two near-identical ``gen_geometry`` /
``define_ave_geometry`` functions in the reference scripts. All geometry decisions
read from the config objects; the only literals here are structural assembly
logic, not tunable values.
"""

from __future__ import annotations

import aerosandbox as asb
import aerosandbox.numpy as np

from ..config.design_variables import DesignVector
from ..config.geometry_config import GeometryConfig
from .airfoils import AirfoilLibrary, build_section


class AircraftBuilder:
    """Builds parametric aircraft from design variables and a geometry scaffold."""

    def __init__(self, geometry: GeometryConfig | None = None):
        self.geometry = geometry or GeometryConfig()
        # Apply the engine spec once at construction time -- not on every build()
        # call, which would repeat the DB lookup O(n_evaluations) times.
        self.geometry.engine.apply_engine_spec()

    # -- public API ----------------------------------------------------------
    def build(
        self, dv: DesignVector | None = None, include_engines: bool = True
    ) -> asb.Airplane:
        """Assemble the full aircraft for a given design vector.

        Passing ``dv=None`` builds the nominal reference aircraft.
        """
        dv = dv or DesignVector.default()
        g = self.geometry

        root_section = build_section(
            dv, AirfoilLibrary.get(g.wing.root_airfoil).coordinates
        )
        tip_airfoil = AirfoilLibrary.get(g.wing.tip_airfoil)
        tail_airfoil = AirfoilLibrary.get(g.empennage.tail_airfoil)

        wings = [
            self._build_main_wing(dv, root_section, tip_airfoil),
            self._build_hstab(dv, tail_airfoil),
            self._build_vstab(dv, tail_airfoil),
        ]
        fuselages = [self._build_fuselage(dv)]
        if include_engines:
            fuselages.extend(self._build_engines(dv))

        main_wing = wings[0]
        mac = main_wing.mean_aerodynamic_chord()
        # Initial CG seed (~quarter-MAC); refined later by the autobalance step.
        x_wing_global = g.wing.root_datum_x_m + dv.wing_x_shift_m
        sweep_rad = np.radians(dv.sweep_deg)
        y_break = g.wing.break_span_fraction * (dv.span_m / 2)
        x_cg_seed = x_wing_global + y_break * np.tan(sweep_rad) + 0.25 * mac

        return asb.Airplane(
            name="ALAS Aircraft",
            xyz_ref=[x_cg_seed, 0, 0],
            wings=wings,
            fuselages=fuselages,
            s_ref=main_wing.area(),
            c_ref=mac,
            b_ref=dv.span_m,
        )

    # -- components ----------------------------------------------------------
    def _build_main_wing(self, dv, root_section, tip_airfoil) -> asb.Wing:
        g = self.geometry.wing
        semi_span = dv.span_m / 2
        y_break = g.break_span_fraction * semi_span
        sweep_in = np.radians(dv.sweep_deg)
        sweep_out = np.radians(dv.sweep_deg - g.outboard_sweep_decrement_deg)
        dx_break = y_break * np.tan(sweep_in)
        dx_tip = dx_break + (semi_span - y_break) * np.tan(sweep_out)
        x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m

        return (
            asb.Wing(
                name="Main Wing",
                symmetric=True,
                xsecs=[
                    asb.WingXSec(
                        xyz_le=[0, 0, g.root_z_m],
                        chord=dv.root_chord_m,
                        twist=g.root_twist_deg,
                        airfoil=root_section,
                    ),
                    asb.WingXSec(
                        xyz_le=[dx_break, y_break, g.break_z_m],
                        chord=dv.break_chord_m,
                        twist=g.break_twist_deg,
                        airfoil=root_section,
                    ),
                    asb.WingXSec(
                        xyz_le=[dx_tip, semi_span, g.tip_z_m],
                        chord=dv.tip_chord_m,
                        twist=dv.tip_twist_deg,
                        airfoil=tip_airfoil,
                    ),
                ],
            )
            .translate([x_wing_global, 0, 0])
            .subdivide_sections(g.n_subdivisions)
        )

    def _build_hstab(self, dv, tail_airfoil) -> asb.Wing:
        g = self.geometry.empennage
        ts = dv.tail_scale
        x_hstab = (
            dv.fuselage_length_m - g.hstab_offset_from_tail_m
        ) + dv.tail_x_shift_m
        tip_le = g.hstab_tip_le_m
        return (
            asb.Wing(
                name="Horizontal Stabilizer",
                symmetric=True,
                xsecs=[
                    asb.WingXSec(
                        xyz_le=[0, 0, 0],
                        chord=g.hstab_root_chord_m * ts,
                        twist=g.hstab_root_twist_deg,
                        airfoil=tail_airfoil,
                    ),
                    asb.WingXSec(
                        xyz_le=[tip_le[0] * ts, tip_le[1] * ts, tip_le[2]],
                        chord=g.hstab_tip_chord_m * ts,
                        twist=g.hstab_tip_twist_deg,
                        airfoil=tail_airfoil,
                    ),
                ],
            )
            .translate([x_hstab, 0, g.hstab_z_m])
            .subdivide_sections(g.n_subdivisions)
        )

    def _build_vstab(self, dv, tail_airfoil) -> asb.Wing:
        g = self.geometry.empennage
        ts = dv.tail_scale
        x_vstab = (
            dv.fuselage_length_m - g.vstab_offset_from_tail_m
        ) + dv.tail_x_shift_m
        tip_le = g.vstab_tip_le_m
        return (
            asb.Wing(
                name="Vertical Stabilizer",
                symmetric=False,
                xsecs=[
                    asb.WingXSec(
                        xyz_le=[0, 0, 0],
                        chord=g.vstab_root_chord_m * ts,
                        airfoil=tail_airfoil,
                    ),
                    asb.WingXSec(
                        xyz_le=[tip_le[0] * ts, tip_le[1], tip_le[2] * ts],
                        chord=g.vstab_tip_chord_m * ts,
                        airfoil=tail_airfoil,
                    ),
                ],
            )
            .translate([x_vstab, 0, g.vstab_z_m])
            .subdivide_sections(g.n_subdivisions)
        )

    def _build_fuselage(self, dv) -> asb.Fuselage:
        g = self.geometry.fuselage
        radius = g.diameter_m / 2
        fus_len = dv.fuselage_length_m
        cabin_end = fus_len - g.tailcone_length_m

        # Check if we have an ovoid cross-section (e.g. A380 double decker)
        is_ovoid = g.height_m is not None and g.height_m != g.diameter_m

        def make_xsec(x_val, z_val, r_val):
            if is_ovoid:
                # Scale width and height proportionally with the equivalent circular profile radius
                local_width = r_val * 2
                local_height = r_val * 2 * (g.height_m / g.diameter_m)
                return asb.FuselageXSec(
                    xyz_c=[x_val, 0, z_val],
                    width=local_width,
                    height=local_height,
                    shape=2.0,  # Ellipse
                )
            else:
                return asb.FuselageXSec(
                    xyz_c=[x_val, 0, z_val],
                    radius=r_val,
                )

        # Mathematically parameterized airliner-like profiles (inspired by peterdsharpe/transport-aircraft)
        # Nose: 10 stations using sinspace to resolve the curved ellipsoid rounding
        x_nose = np.sinspace(0, 1, 10)
        stations_nose = [
            make_xsec(
                x_val=xi * g.cabin_start_x_m,
                z_val=g.cabin_z_m + (g.nose_z_m - g.cabin_z_m) * (1 - xi) ** 2,
                r_val=radius * (1 - (1 - xi) ** 2) ** 0.5,
            )
            for xi in x_nose[:-1]  # Exclude last point since it is the cabin start
        ]

        # Cabin: 2 stations (start and end)
        stations_cabin = [
            make_xsec(g.cabin_start_x_m, g.cabin_z_m, radius),
            make_xsec(cabin_end, g.cabin_z_m, radius),
        ]

        # Tail: 10 stations resolving gradual taper and upsweep
        x_tail = np.linspace(0, 1, 10)
        stations_tail = [
            make_xsec(
                x_val=cabin_end + xi * g.tailcone_length_m,
                z_val=g.cabin_z_m + (g.tail_z_m - g.cabin_z_m) * xi**1.5,
                r_val=radius * (1 - xi**1.5),
            )
            for xi in x_tail[1:]  # Exclude first point since it is the cabin end
        ]

        stations = stations_nose + stations_cabin + stations_tail

        return asb.Fuselage(
            name="Fuselage",
            xsecs=stations,
        )

    def _build_engines(self, dv) -> list:
        g = self.geometry.engine
        x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m
        sweep_rad = np.radians(dv.sweep_deg)
        nacelles = []

        semi_span = dv.span_m / 2.0
        y_break = self.geometry.wing.break_span_fraction * semi_span

        for y_pos in g.spanwise_positions_m:
            if y_pos == 0.0:
                # Centerline tail-mounted engine (e.g. trijet tail engine on DC-10)
                name = "Nacelle Center"
                nacelle_len = g.nacelle_profile[-1][0]
                x_inlet = dv.fuselage_length_m - nacelle_len
                # Place the tail engine on top of the tailcone at the base of the vstab
                z_nacelle = self.geometry.fuselage.tail_z_m + g.radius_scale_m + 0.2
            else:
                name = "Nacelle R" if y_pos > 0 else "Nacelle L"
                x_inlet = (
                    x_wing_global + abs(y_pos) * np.tan(sweep_rad)
                ) - g.inlet_x_offset_m

                # Interpolate local wing Z-height for dihedral-aware placement
                y_abs = abs(y_pos)
                if y_abs <= y_break:
                    z_wing = self.geometry.wing.root_z_m + (
                        self.geometry.wing.break_z_m - self.geometry.wing.root_z_m
                    ) * (y_abs / (y_break + 1e-9))
                else:
                    z_wing = self.geometry.wing.break_z_m + (
                        self.geometry.wing.tip_z_m - self.geometry.wing.break_z_m
                    ) * ((y_abs - y_break) / (semi_span - y_break + 1e-9))

                # Apply local Z shift relative to the wing LE.
                # z_m is a direct offset below the interpolated wing z at this
                # span station (negative = engine hangs below wing).
                z_nacelle = z_wing + g.z_m

            nacelles.append(
                asb.Fuselage(
                    name=name,
                    xsecs=[
                        asb.FuselageXSec(xyz_c=[x, 0, 0], radius=g.radius_scale_m * r)
                        for x, r in g.nacelle_profile
                    ],
                ).translate([x_inlet, y_pos, z_nacelle])
            )
        return nacelles
