# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Generic wingbox rib/spar geometry generator.

Generalizes ``Reference Scripts/01_geometry.py`` (hardcoded to one aircraft's
span/chord/sweep/kink and a fixed SC(2)-0714/NACA2410 blended airfoil) to any
wing :class:`alas.geometry.aircraft_builder.AircraftBuilder` produces,
and to an arbitrary number of spars at arbitrary chord fractions:

* Planform (chord/x_le) and dihedral (z_le) come from the *exact same
  root -> break(kink) -> tip piecewise-linear formulas*
  ``AircraftBuilder._build_main_wing`` uses (copied, not reinvented), so the
  structural wingbox is always geometrically consistent with the actual
  aerodynamic wing the rest of the app analyses.
* Airfoil shape at each spanwise station is sampled directly from the
  *actual built* ``root_section``/``tip_airfoil`` ``asb.Airfoil`` objects
  (via their own ``upper_coordinates()``/``lower_coordinates()``) instead of
  a hardcoded blend -- so bumps/thickness/camber morphing the optimizer
  applies are reflected automatically.
* Each spar gets its own 3-point (root/break/tip) kinked reference line, the
  same treatment the reference reserved only for its rear spar (whose
  position naturally kinks at the wing break since the planform itself
  does). Generalizing this to *every* spar, not just picking one as
  "special", is what lets ``spar_chord_fractions`` be an arbitrary-length
  list instead of a fixed front/rear pair.

Coordinate convention matches AeroSandbox's airplane frame exactly: X =
chordwise (aft-positive), Y = spanwise (outboard-positive, root = 0), Z = up
(including dihedral). Twist (washout) is **not** applied to the FEM
cross-sections -- a documented simplification matching the reference
scripts' own fidelity level (a few degrees of twist has a second-order
effect on spanwise bending stiffness).

Ribs are cut perpendicular to the local leading edge (streamwise at the
root, matching the reference's ``get_rib_vector`` convention -- the root rib
is the SPC'd wall and must be a clean streamwise cut). Root-adjacent
"transition" ribs are truncated where their perpendicular cut would
otherwise extend past the wing root (Y < 0) -- this is the geometry that
demands the zipper-triangle skin bridging and RBE3 rivets in
:mod:`alas.geometry.wing_mesh_bdf`; see that module's docstring.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import List, Optional, Sequence, Tuple

import numpy as np
import aerosandbox as asb

from ..config.design_variables import DesignVector
from ..config.geometry_config import WingConfig


def _airfoil_surfaces(
    af: asb.Airfoil,
) -> Tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Return (x_upper, z_upper, x_lower, z_lower), each ascending in x,
    normalized to unit chord. AeroSandbox's own coordinate orientation isn't
    guaranteed LE->TE (see the same defensive sort in geometry/airfoils.py's
    ``apply_bumps``), so sort explicitly rather than assume."""
    upper = np.asarray(af.upper_coordinates())
    lower = np.asarray(af.lower_coordinates())
    xu, zu = upper[:, 0], upper[:, 1]
    xl, zl = lower[:, 0], lower[:, 1]
    iu = np.argsort(xu)
    il = np.argsort(xl)
    return xu[iu], zu[iu], xl[il], zl[il]


def _intersect_line_ray(A, B, P, D) -> Tuple[Optional[float], Optional[float]]:
    """Intersection of segment A->B (parametrized by t in [0,1]) with ray
    P + s*D (s >= 0 expected by caller). Returns (s, t) or (None, None) if
    parallel. Direct port of 01_geometry.py's ``intersect_line_ray``."""
    x_A, y_A = A
    x_B, y_B = B
    x_p, y_p = P
    dx, dy = D

    dx_seg = x_B - x_A
    dy_seg = y_B - y_A

    det = dx * dy_seg - dy * dx_seg
    if abs(det) < 1e-8:
        return None, None

    s = ((x_A - x_p) * dy_seg - (y_A - y_p) * dx_seg) / det

    if abs(dy_seg) > abs(dx_seg):
        t = (y_p + s * dy - y_A) / dy_seg
    else:
        t = (x_p + s * dx - x_A) / dx_seg

    return s, t


@dataclass
class RibStation:
    """One spanwise rib cross-section, ready for sizing/mesh consumption."""

    index: int
    eta: float
    y_station: float
    is_full: bool  # False = truncated ("transition") rib near the root
    frac_actual: float  # L_actual / L_nominal along the rib's own cut direction
    extrados: np.ndarray  # (n, 3) absolute XYZ, LE -> (truncated) TE
    intrados: np.ndarray  # (n, 3)
    j_spars: List[int]  # node index of each spar (order = sorted spar_chord_fractions);
    # -1 if that spar falls outside this rib's truncated reach
    rib_dir_xy: Tuple[float, float]  # unit chordwise direction in the XY plane


class WingStructureGeometry:
    """Computes generic rib/spar FEM geometry for one main-wing design.

    Construct once per analysis with the design's own ``DesignVector``,
    ``WingConfig``, and the *actual* root/tip airfoil objects
    (``AircraftBuilder`` already builds these -- pass the same ones so the
    wingbox always matches the analyzed aerodynamic shape), then call
    :meth:`get_rib_stations` / :meth:`get_spar_nodes`.
    """

    def __init__(
        self,
        dv: DesignVector,
        wing_cfg: WingConfig,
        root_section: asb.Airfoil,
        tip_airfoil: asb.Airfoil,
        spar_chord_fractions: Sequence[float],
        spar_full_span: Optional[Sequence[bool]] = None,
    ):
        self.dv = dv
        self.wing_cfg = wing_cfg
        self.semi_span = dv.span_m / 2.0
        self.break_eta = float(wing_cfg.break_span_fraction)
        self.y_break = self.break_eta * self.semi_span

        self.sweep_in = math.radians(dv.sweep_deg)
        self.sweep_out = math.radians(
            dv.sweep_deg - wing_cfg.outboard_sweep_decrement_deg
        )

        # Same dx_break/dx_tip formulas as AircraftBuilder._build_main_wing.
        self.dx_break = self.y_break * math.tan(self.sweep_in)
        self.dx_tip = self.dx_break + (self.semi_span - self.y_break) * math.tan(
            self.sweep_out
        )

        self.c_root = dv.root_chord_m
        self.c_break = dv.break_chord_m
        self.c_tip = dv.tip_chord_m

        self.z_root = wing_cfg.root_z_m
        self.z_break = wing_cfg.break_z_m
        self.z_tip = wing_cfg.tip_z_m

        if not spar_chord_fractions:
            raise ValueError("At least one spar_chord_fractions entry is required.")
        full_span_in = (
            spar_full_span
            if spar_full_span is not None
            else [True] * len(spar_chord_fractions)
        )
        # Sort fracs and full_span together (not two independent sorts) so a
        # partial-span spar's own full_span=False flag stays attached to its
        # own fraction regardless of the input order (e.g. a center spar
        # appended after two full-span spars still lands paired correctly
        # once sorted into the middle).
        paired = sorted(zip((float(f) for f in spar_chord_fractions), full_span_in))
        self.spar_fracs: List[float] = [p[0] for p in paired]
        self.spar_full_span: List[bool] = [bool(p[1]) for p in paired]

        self._root_xu, self._root_zu, self._root_xl, self._root_zl = _airfoil_surfaces(
            root_section
        )
        self._tip_xu, self._tip_zu, self._tip_xl, self._tip_zl = _airfoil_surfaces(
            tip_airfoil
        )

        # TE-line slopes dTE_x/dy on each panel (used by get_rib_lengths' root-plane
        # truncation check), matching 01_geometry.py's A_in/A_out.
        self.a_in = math.tan(self.sweep_in) + (self.c_break - self.c_root) / max(
            self.y_break, 1e-9
        )
        self.a_out = math.tan(self.sweep_out) + (self.c_tip - self.c_break) / max(
            self.semi_span - self.y_break, 1e-9
        )
        self.x_kink_te = self.dx_break + self.c_break

        # Precompute each spar's 3-point (root/break/tip) reference line --
        # generalizes 01_geometry.py's _get_spar_reference_points to N spars,
        # applying the kinked-line treatment to *every* spar (see module docstring).
        self._spar_ref_pts = self._compute_spar_reference_points()

    # -- planform / dihedral (piecewise-linear root -> break -> tip) --------
    def local_chord(self, eta: float) -> float:
        if eta <= self.break_eta:
            t = eta / max(self.break_eta, 1e-9)
            return self.c_root * (1 - t) + self.c_break * t
        t = (eta - self.break_eta) / max(1.0 - self.break_eta, 1e-9)
        return self.c_break * (1 - t) + self.c_tip * t

    def x_le(self, eta: float) -> float:
        if eta <= self.break_eta:
            t = eta / max(self.break_eta, 1e-9)
            return self.dx_break * t
        t = (eta - self.break_eta) / max(1.0 - self.break_eta, 1e-9)
        return self.dx_break + (self.dx_tip - self.dx_break) * t

    def z_le(self, eta: float) -> float:
        if eta <= self.break_eta:
            t = eta / max(self.break_eta, 1e-9)
            return self.z_root * (1 - t) + self.z_break * t
        t = (eta - self.break_eta) / max(1.0 - self.break_eta, 1e-9)
        return self.z_break * (1 - t) + self.z_tip * t

    def rib_vector(self, eta: float) -> Tuple[float, float]:
        """Unit chordwise direction in the XY plane: streamwise at the root
        (the SPC'd wall must be a clean streamwise cut), perpendicular to the
        local leading edge everywhere else."""
        if eta <= 1e-9:
            return 1.0, 0.0
        sweep = self.sweep_in if eta <= self.break_eta else self.sweep_out
        return math.cos(sweep), -math.sin(sweep)

    def le_direction(self, eta: float) -> Tuple[float, float]:
        """Unit leading-edge tangent direction in the XY plane, independent
        of :meth:`rib_vector` -- used as a self-consistency check (the
        mesh's realized rib cuts should come out perpendicular to this,
        except at the root rib, which is deliberately streamwise instead;
        see :meth:`rib_vector`)."""
        sweep = self.sweep_in if eta <= self.break_eta else self.sweep_out
        dx, dy = math.tan(sweep), 1.0
        norm = math.hypot(dx, dy)
        return dx / norm, dy / norm

    # -- airfoil surface at an arbitrary (eta, x/c) --------------------------
    def airfoil_zu_zl(self, eta: float, xc_frac: float) -> Tuple[float, float]:
        """Upper/lower surface height (fraction of local chord) at x/c.

        eta <= break_eta: the root section verbatim (matches AircraftBuilder
        using the same morphed ``root_section`` airfoil for both the root and
        break WingXSecs -- no interpolation needed inboard). eta > break_eta:
        linearly interpolated toward ``tip_airfoil`` (matches AeroSandbox's
        own linear interpolation between the break and tip WingXSecs).
        """
        xc = float(np.clip(xc_frac, 0.0, 1.0))
        zu_root = float(np.interp(xc, self._root_xu, self._root_zu))
        zl_root = float(np.interp(xc, self._root_xl, self._root_zl))
        if eta <= self.break_eta:
            return zu_root, zl_root
        zu_tip = float(np.interp(xc, self._tip_xu, self._tip_zu))
        zl_tip = float(np.interp(xc, self._tip_xl, self._tip_zl))
        blend = (eta - self.break_eta) / max(1.0 - self.break_eta, 1e-9)
        blend = float(np.clip(blend, 0.0, 1.0))
        return (
            zu_root * (1 - blend) + zu_tip * blend,
            zl_root * (1 - blend) + zl_tip * blend,
        )

    def spar_height(self, eta: float, xc_frac: float) -> float:
        """Free web height (extrados - intrados) at (eta, x/c), in metres."""
        zu, zl = self.airfoil_zu_zl(eta, xc_frac)
        return (zu - zl) * self.local_chord(eta)

    # -- rib length (truncation by the root plane Y=0) -----------------------
    def get_rib_lengths(
        self, y_le_val: float, x_le_val: float, aft_x: float, aft_y: float
    ) -> Tuple[float, float]:
        """Returns (L_nominal_to_TE, L_actual_after_root-plane_truncation), in
        metres along the rib's own cut direction. Direct generalization of
        01_geometry.py's ``get_rib_lengths``."""
        if abs(aft_y) < 1e-6:
            return self.local_chord(y_le_val / self.semi_span), self.local_chord(
                y_le_val / self.semi_span
            )

        s_te_in = None
        denom1 = aft_x - self.a_in * aft_y
        if abs(denom1) > 1e-6:
            s = (self.a_in * y_le_val + self.c_root - x_le_val) / denom1
            if s > 0:
                y_int = y_le_val + s * aft_y
                if 0 <= y_int <= self.y_break + 1e-4:
                    s_te_in = s

        s_te_out = None
        denom2 = aft_x - self.a_out * aft_y
        if abs(denom2) > 1e-6:
            s = (
                self.a_out * (y_le_val - self.y_break) + self.x_kink_te - x_le_val
            ) / denom2
            if s > 0:
                y_int = y_le_val + s * aft_y
                if self.y_break - 1e-4 <= y_int <= self.semi_span + 1e-4:
                    s_te_out = s

        if s_te_in is not None and s_te_out is not None:
            l_nominal = min(s_te_in, s_te_out)
        elif s_te_in is not None:
            l_nominal = s_te_in
        elif s_te_out is not None:
            l_nominal = s_te_out
        else:
            l_nominal = self.local_chord(y_le_val / self.semi_span)

        s_root = None
        if aft_y < 0:
            s = -y_le_val / aft_y
            if s > 0:
                s_root = s

        l_actual = min(l_nominal, s_root) if s_root is not None else l_nominal
        return l_nominal, l_actual

    # -- spar reference lines + intersections --------------------------------
    def _compute_spar_reference_points(self) -> List[dict]:
        """One 3-point (root/break/tip) reference line per spar, generalizing
        01_geometry.py's ``_get_spar_reference_points`` (which only did this
        for its rear spar) to every spar in ``spar_fracs``.

        A spar with ``spar_full_span[i] = False`` (the optional widebody-
        style partial-span center spar, see StructuresConfig.
        center_spar_enabled) gets no "tip" point at all -- it physically
        ends at the break/kink station, so there's no break->tip segment to
        define. :meth:`compute_spar_intersections` treats the missing key
        as "this spar doesn't exist outboard of the break."
        """
        x_le_root, aft_root = 0.0, self.rib_vector(0.0)
        l_root, _ = self.get_rib_lengths(0.0, x_le_root, *aft_root)

        x_le_break = self.x_le(self.break_eta)
        aft_break = self.rib_vector(self.break_eta)
        l_break, _ = self.get_rib_lengths(self.y_break, x_le_break, *aft_break)

        x_le_tip = self.x_le(1.0)
        aft_tip = self.rib_vector(1.0)
        l_tip, _ = self.get_rib_lengths(self.semi_span, x_le_tip, *aft_tip)

        refs = []
        for frac, full_span in zip(self.spar_fracs, self.spar_full_span):
            ref = {
                "root": (
                    x_le_root + frac * l_root * aft_root[0],
                    0.0 + frac * l_root * aft_root[1],
                ),
                "break": (
                    x_le_break + frac * l_break * aft_break[0],
                    self.y_break + frac * l_break * aft_break[1],
                ),
            }
            if full_span:
                ref["tip"] = (
                    x_le_tip + frac * l_tip * aft_tip[0],
                    self.semi_span + frac * l_tip * aft_tip[1],
                )
            refs.append(ref)
        return refs

    def compute_spar_intersections(
        self,
        y_station: float,
        x_le_val: float,
        aft_x: float,
        aft_y: float,
        l_nominal: float,
    ) -> List[Optional[float]]:
        """Distance (along the rib cut direction) from LE to each spar, in
        metres. Falls back to a straight %-chord estimate if the reference
        line doesn't intersect this rib's cut (same non-fatal fallback the
        reference used). Returns ``None`` for a partial-span spar at a
        station beyond its own break-station endpoint -- it doesn't exist
        there, not even as a fallback estimate."""
        p_rib = (x_le_val, y_station)
        d_rib = (aft_x, aft_y)
        out: List[Optional[float]] = []
        for frac, full_span, ref in zip(
            self.spar_fracs, self.spar_full_span, self._spar_ref_pts
        ):
            if not full_span and y_station > self.y_break + 1e-6:
                out.append(None)
                continue

            s_in, t_in = _intersect_line_ray(ref["root"], ref["break"], p_rib, d_rib)
            s_out = t_out = None
            if full_span:
                s_out, t_out = _intersect_line_ray(
                    ref["break"], ref["tip"], p_rib, d_rib
                )

            s_val = None
            if t_in is not None and -1e-5 <= t_in <= 1.0 + 1e-5:
                s_val = s_in
            elif t_out is not None and -1e-5 <= t_out <= 1.0 + 1e-5:
                s_val = s_out
            if s_val is None:
                if t_in is not None and t_out is not None:
                    dist_in = max(0.0, -t_in, t_in - 1.0)
                    dist_out = max(0.0, -t_out, t_out - 1.0)
                    s_val = s_in if dist_in < dist_out else s_out
                elif t_in is not None:
                    s_val = s_in
                elif t_out is not None:
                    s_val = s_out

            if s_val is None or s_val < 0.0:
                s_val = frac * l_nominal
            out.append(float(s_val))
        return out

    # -- full rib station generation ------------------------------------------
    def get_rib_stations(
        self, num_ribs: int, num_pts_chord: int = 50
    ) -> List[RibStation]:
        """Generates ``num_ribs`` spanwise rib stations, cosine-spaced
        chordwise with each spar's exact chord fraction injected so spar
        elements land on real mesh nodes -- direct generalization of
        01_geometry.py's ``get_all_rib_nodes_for_fem``.

        A rib whose perpendicular cut is truncated before reaching a given
        spar (``frac_spar > frac_actual``) gets ``j_spars[i] = -1`` for that
        spar -- the mesh builder must skip that spar segment for this rib,
        exactly as the reference's sentinel convention requires.
        """
        y_stations = np.linspace(0.0, self.semi_span, num_ribs)
        stations: List[RibStation] = []

        for i, y in enumerate(y_stations):
            eta = float(y / self.semi_span)
            x_le_val = self.x_le(eta)
            z_le_val = self.z_le(eta)
            aft_x, aft_y = self.rib_vector(eta)
            l_nominal, l_actual = self.get_rib_lengths(float(y), x_le_val, aft_x, aft_y)

            s_spars = self.compute_spar_intersections(
                float(y), x_le_val, aft_x, aft_y, l_nominal
            )

            beta = np.linspace(0.0, np.pi, num_pts_chord)
            xc_fracs = 0.5 * (1.0 - np.cos(beta))

            frac_actual = l_actual / max(l_nominal, 1e-9)
            # None means "this spar doesn't exist at this rib" (a partial-
            # span spar beyond its own break-station endpoint, see
            # compute_spar_intersections) -- excluded from both the
            # critical-point injection and the node lookup below, same as
            # any other spar this rib's truncated reach doesn't extend to.
            frac_spars = [
                (s / max(l_nominal, 1e-9)) if s is not None else None for s in s_spars
            ]

            critical_pts = [
                f for f in frac_spars if f is not None and f <= frac_actual + 1e-5
            ] + [frac_actual]
            for pt in critical_pts:
                idx = int(np.argmin(np.abs(xc_fracs - pt)))
                xc_fracs[idx] = float(pt)

            xc_fracs = np.unique(np.round(xc_fracs, 6))
            xc_fracs = xc_fracs[xc_fracs <= np.round(frac_actual, 6)]

            j_spars: List[int] = []
            for f_spar in frac_spars:
                if f_spar is None:
                    j_spars.append(-1)
                    continue
                hits = np.where(np.isclose(xc_fracs, np.round(f_spar, 6)))[0]
                j_spars.append(int(hits[0]) if len(hits) else -1)

            n = len(xc_fracs)
            ext_pts = np.zeros((n, 3))
            int_pts = np.zeros((n, 3))
            for j, frac in enumerate(xc_fracs):
                zu_norm, zl_norm = self.airfoil_zu_zl(eta, frac)
                if j == n - 1:
                    # Pinch the physical TE shut: upper=lower at the rib end
                    # (or at the root-plane cut, where "TE" is really a cut face).
                    zl_norm = zu_norm
                px = x_le_val + (frac * l_nominal) * aft_x
                py = y + (frac * l_nominal) * aft_y
                pz_chord = zu_norm * self.local_chord(eta)
                pz_chord_l = zl_norm * self.local_chord(eta)
                ext_pts[j] = [px, py, z_le_val + pz_chord]
                int_pts[j] = [px, py, z_le_val + pz_chord_l]

            stations.append(
                RibStation(
                    index=i,
                    eta=eta,
                    y_station=float(y),
                    is_full=True,  # classification (full vs transition) is refined by the mesh builder,
                    # which knows the skin-start rib -- see wing_mesh_bdf.py
                    frac_actual=float(frac_actual),
                    extrados=ext_pts,
                    intrados=int_pts,
                    j_spars=j_spars,
                    rib_dir_xy=(aft_x, aft_y),
                )
            )
        return stations
