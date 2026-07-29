# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""pyNastran BDF mesh builder for the generic wingbox.

Generalizes ``Reference Scripts/02_mesh.py`` to any :class:`alas.
geometry.wing_structure.WingStructureGeometry` / :class:`alas.physics.
structural_sizing.WingboxSizing` pair, with an arbitrary number of spars.

**This mesh is not simple -- read before touching the zipper/RBE3 logic.**
Ribs are cosine-sampled and root-adjacent ("transition") ribs are truncated
by the root plane (Y=0), so adjacent ribs generally do **not** have equal
chordwise node counts. Building skin panels naively rib-to-rib on such a
mesh leaves gaps or forces degenerate connectivity -- the failure mode that
originally caused extreme skin warping in the reference project. Two
mechanisms fix this, both ported faithfully rather than simplified:

1. **Zipper-triangle skin bridging** (:func:`_zipper_skin_strip`): when two
   adjacent ribs have different chordwise node counts, the shorter rib's
   last node becomes a shared pivot and the extra panels on the longer
   rib's side are closed with ``CTRIA3`` instead of ``CQUAD4`` -- the skin
   mesh never has a gap regardless of node-count mismatch.
2. **RBE3 rivets on transition ribs** (:func:`_add_transition_rbe3`): a
   transition rib's nodes are tied to the 3 nearest full-length skin nodes
   (within a spanwise search band) via ``RBE3`` -- this is what makes a
   physically shorter rib move and deform together with the skin around it
   instead of floating disconnected.

Five geometric health checks the reference project used to catch this bug
class are reproduced in :func:`_check_mesh_health` and returned as a
:class:`MeshHealthReport` (not just console prints): rib-LE perpendicularity,
``CQUAD4`` warping coefficient, ``CTRIA3`` degenerate-triangle detection,
spar XY-straightness, and a "no node at Y < 0" sanity check. A degenerate
triangle or a Y<0 node is a hard structural-mesh corruption (raises
``ValueError``, matching the reference's own severity); the others are
non-fatal warnings surfaced in the report.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Tuple

import numpy as np

from ..integration import _nastran_compat  # noqa: F401  (numpy 2.x shim, must import before pyNastran)
from pyNastran.bdf.bdf import BDF

from ..config.geometry_config import EngineConfig
from ..config.mass_config import MassModelConfig
from ..config.materials import MaterialSpec
from ..config.requirements import DesignRequirements
from ..config.structures_config import StructuresConfig
from ..physics import structural_loads as loads
from ..physics.structural_sizing import WingboxSizing, _cap_taper
from .wing_structure import RibStation, WingStructureGeometry

WARPING_THRESHOLD = 0.05
_Y_ROOT_EXCL = 0.01  # m: exclude root-SPC'd nodes from the RBE3 independent-node pool
_SEC_RIB_THICKNESS_FACTOR = 0.5  # secondary (non-full) ribs are thinner than main ribs


@dataclass
class MeshNodeIndex:
    """Node IDs :mod:`alas.integration.nastran_runner` needs to write
    load/monitor cards -- returned directly from the same Python objects
    that built the mesh, rather than the reference scripts' approach of a
    separate script re-parsing element PIDs out of a written .bdf file
    (fragile, and unnecessary here since everything runs in one process)."""

    root_nid: int
    tip_nid: int
    kink_nid: int
    spar_upper_nids: List[
        List[int]
    ]  # per spar, sorted root -> tip (skin_ribs intersected with structural region)
    spar_lower_nids: List[List[int]]
    engine_nids: List[int]  # one per wing-mounted engine (CONM2 attachment node)


@dataclass
class MeshHealthReport:
    n_nodes: int = 0
    n_elements: int = 0
    n_perp_warnings: int = 0
    n_warping_bad: int = 0
    warping_max: float = 0.0
    warping_mean: float = 0.0
    n_cquad4: int = 0
    n_ctria3: int = 0
    triangle_ratio: float = 0.0
    n_spar_straightness_warnings: int = 0
    spar_straightness_max_dev_m: Dict[float, float] = field(default_factory=dict)
    rbe3_count: int = 0
    warnings: List[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        """True iff the mesh is free of the specific defect this module's
        docstring is about: badly warped (non-planar) CQUAD4 skin panels.
        Perpendicularity/straightness are still reported (``warnings``,
        ``n_perp_warnings``, ``n_spar_straightness_warnings``) but don't
        gate ``ok`` -- exactly the reference scripts' own severity split
        (only the CTRIA3-degenerate and Y<0 checks ever raised there; every
        other check, including these two, was console-only). A nonzero
        spar-straightness deviation right at the root is an *expected*
        characteristic of this model, not a defect: the root rib's spar
        anchor is defined along the streamwise root chord (it's a clean
        streamwise SPC'd cut), while every other station's anchor follows
        the perpendicular-to-LE rib direction -- the two conventions aren't
        perfectly collinear at the root/first-structural-station transition,
        the same convention the reference scripts used for their own rear
        spar reference line.
        """
        return self.n_warping_bad == 0


def _te_rib_selected(
    mode: str, pos: int, y_station: float, y_break: float, n_inboard: int
) -> bool:
    """Which ribs get trailing-edge panels -- direct port of 02_mesh.py's
    ``_te_rib_selected``, generalized variable names only."""
    mode = mode.lower()
    if mode == "all":
        return True
    if mode == "none":
        return False
    if mode == "alternate":
        return pos % 2 == 0
    if mode == "inboard":
        return y_station <= y_break
    if mode == "outboard":
        return y_station > y_break
    if mode == "inboard_alternate":
        if y_station <= y_break:
            return True
        return (pos - n_inboard) % 2 == 0
    if mode.startswith("step_"):
        try:
            step = int(mode.split("_")[1])
            return pos % step == 0
        except (IndexError, ValueError):
            pass
    return True


def build_wing_mesh_bdf(
    wsg: WingStructureGeometry,
    sizing: WingboxSizing,
    cfg: StructuresConfig,
    engine_cfg: EngineConfig,
    mass_cfg: MassModelConfig,
    req: DesignRequirements,
    skin_mat: MaterialSpec,
    web_mat: MaterialSpec,
    cap_mat: MaterialSpec,
    rib_mat: MaterialSpec,
) -> Tuple[BDF, MeshHealthReport, MeshNodeIndex]:
    """Builds the FEM ``BDF`` model for the semi-wing wingbox.

    Ribs are generated fresh at the mesh's own resolution
    (``sizing.num_ribs`` stations, ``cfg.mesh_chordwise_points`` chordwise
    points) -- generally a different, finer grid than the sizing/analytical
    solver's own ``spanwise_stations`` integration grid -- so cap dimensions
    are re-derived here from the root value + taper law (the same
    :func:`alas.physics.structural_sizing._cap_taper`), not sampled
    from the sizing pass's array.
    """
    stations = wsg.get_rib_stations(sizing.num_ribs, cfg.mesh_chordwise_points)
    n_ribs = len(stations)
    n_spars = len(wsg.spar_fracs)
    warnings: List[str] = []

    def _nominal_chord(eta: float) -> float:
        return wsg.local_chord(eta)

    # -- PASO 1: classify ribs (skin / transition / aero-only) ---------------
    te_x_root = float(stations[0].extrados[-1][0])
    skin_start = None
    for i in range(1, n_ribs):
        if float(stations[i].extrados[-1][0]) >= te_x_root * 0.99:
            skin_start = i
            break
    skin_start = skin_start if skin_start is not None else 1

    aero_only = set()
    for i, st in enumerate(stations):
        ratio = float(np.linalg.norm(st.extrados[-1] - st.extrados[0])) / max(
            _nominal_chord(st.eta), 1e-9
        )
        if ratio <= 0.20:
            aero_only.add(i)
            warnings.append(
                f"Rib {i} (y={st.y_station:.2f} m) aero-only: {ratio:.1%} nominal chord -- excluded."
            )

    skin_ribs = [
        i for i in range(n_ribs) if i not in aero_only and (i == 0 or i >= skin_start)
    ]
    skin_ribs_set = set(skin_ribs)
    transition_ribs = [i for i in range(1, skin_start) if i not in aero_only]
    all_structural = sorted(set(skin_ribs) | set(transition_ribs))

    for i in range(n_ribs):
        stations[i].is_full = i not in transition_ribs

    # -- Materials / properties -----------------------------------------------
    model = BDF(debug=False)
    mid_skin, mid_web, mid_cap, mid_rib = 1, 2, 3, 4
    model.add_mat1(
        mid_skin, skin_mat.e_pa, skin_mat.g_pa, skin_mat.nu, rho=skin_mat.rho_kg_m3
    )
    model.add_mat1(
        mid_web, web_mat.e_pa, web_mat.g_pa, web_mat.nu, rho=web_mat.rho_kg_m3
    )
    model.add_mat1(
        mid_cap, cap_mat.e_pa, cap_mat.g_pa, cap_mat.nu, rho=cap_mat.rho_kg_m3
    )
    model.add_mat1(
        mid_rib, rib_mat.e_pa, rib_mat.g_pa, rib_mat.nu, rho=rib_mat.rho_kg_m3
    )

    pid_skin, pid_main_rib, pid_sec_rib, pid_te_strip = 1, 4, 5, 6
    pid_web_base = 100  # one PSHELL per spar web: 100+i
    pid_cap_base = (
        10000  # one PBARL family per spar per segment: (10000 + spar_i*1000) + seg_i
    )

    model.add_pshell(pid_skin, mid1=mid_skin, t=sizing.t_skin, mid2=mid_skin)
    model.add_pshell(pid_main_rib, mid1=mid_rib, t=cfg.t_rib_m, mid2=mid_rib)
    model.add_pshell(
        pid_sec_rib,
        mid1=mid_rib,
        t=cfg.t_rib_m * _SEC_RIB_THICKNESS_FACTOR,
        mid2=mid_rib,
    )
    model.add_pshell(pid_te_strip, mid1=mid_rib, t=cfg.t_te_strip_m, mid2=mid_rib)
    for i in range(n_spars):
        model.add_pshell(
            pid_web_base + i, mid1=mid_web, t=sizing.spars[i].t_web, mid2=mid_web
        )

    # -- Nodes -----------------------------------------------------------------
    node_id = 1
    coord_to_nid: Dict[Tuple[float, float, float], int] = {}
    node_map: Dict[
        Tuple[int, int, str], int
    ] = {}  # (rib_idx, pt_idx, "ext"/"int") -> nid

    def _coord_key(xyz) -> Tuple[float, float, float]:
        return (
            round(float(xyz[0]), 6),
            round(float(xyz[1]), 6),
            round(float(xyz[2]), 6),
        )

    def _add_grid(xyz) -> int:
        nonlocal node_id
        key = _coord_key(xyz)
        if key in coord_to_nid:
            return coord_to_nid[key]
        model.add_grid(node_id, [float(xyz[0]), float(xyz[1]), float(xyz[2])])
        coord_to_nid[key] = node_id
        node_id += 1
        return coord_to_nid[key]

    for i, st in enumerate(stations):
        for j, pt in enumerate(st.extrados):
            node_map[(i, j, "ext")] = _add_grid(pt)
        for j, pt in enumerate(st.intrados):
            node_map[(i, j, "int")] = _add_grid(pt)

    # -- Spar node lists (ALL structural ribs, not just skin_ribs -- the spar
    # is the continuous load path through the transition region; the skin
    # itself starts later and is rivet-connected via RBE3 instead) -----------
    spar_upper: List[List[int]] = []
    spar_lower: List[List[int]] = []
    for si in range(n_spars):
        up, lo = [], []
        for i in all_structural:
            j = stations[i].j_spars[si]
            if j >= 0:
                up.append(node_map[(i, j, "ext")])
                lo.append(node_map[(i, j, "int")])
        spar_upper.append(up)
        spar_lower.append(lo)

    # -- Elements ----------------------------------------------------------------
    eid = 1
    n_skin = n_fweb = n_rib_elems = n_te = n_cap = 0

    def _add_q(n1, n2, n3, n4, pid) -> None:
        """CQUAD4, or degenerates to CTRIA3; skips fully-degenerate panels.
        Direct port of 02_mesh.py's ``_add_q`` -- the core of the zipper
        bridging (see module docstring)."""
        nonlocal eid, n_skin
        nodes = [int(n1), int(n2), int(n3), int(n4)]
        unq = [nodes[0]]
        for nn in nodes[1:]:
            if nn != unq[-1]:
                unq.append(nn)
        if unq[0] == unq[-1] and len(unq) > 1:
            unq.pop()
        if len(unq) == 3:
            model.add_ctria3(eid, pid, unq)
            eid += 1
        elif len(unq) == 4:
            model.add_cquad4(eid, pid, unq)
            eid += 1

    def _add_tri(nodes, pid) -> None:
        """Adds an already-triangular CTRIA3 (the zipper's explicit pivot-fan
        closing triangles, as opposed to _add_q's degenerate-quad case)."""
        nonlocal eid, n_skin
        model.add_ctria3(eid, pid, [int(n) for n in nodes])
        eid += 1

    def _zipper_skin_strip(r_c: int, r_n: int, side: str) -> None:
        """Skin panels between two consecutive skin ribs, zipper-bridging any
        node-count mismatch with CTRIA3 fan triangles."""
        nonlocal n_skin
        attr = "extrados" if side == "ext" else "intrados"
        l_c = len(getattr(stations[r_c], attr))
        l_n = len(getattr(stations[r_n], attr))
        min_l = min(l_c, l_n)
        reversed_order = side == "int"  # keep outward normal on the intrados

        for j in range(min_l - 1):
            n1 = node_map[(r_c, j, side)]
            n2 = node_map[(r_n, j, side)]
            n3 = node_map[(r_n, j + 1, side)]
            n4 = node_map[(r_c, j + 1, side)]
            if reversed_order:
                _add_q(n1, n4, n3, n2, pid_skin)
            else:
                _add_q(n1, n2, n3, n4, pid_skin)
            n_skin += 1

        if l_c > l_n:
            pivot = node_map[(r_n, min_l - 1, side)]
            for j in range(min_l - 1, l_c - 1):
                a, b = node_map[(r_c, j, side)], node_map[(r_c, j + 1, side)]
                tri = [a, pivot, b] if not reversed_order else [a, b, pivot]
                _add_tri(tri, pid_skin)
                n_skin += 1
        elif l_n > l_c:
            pivot = node_map[(r_c, min_l - 1, side)]
            for j in range(min_l - 1, l_n - 1):
                a, b = node_map[(r_n, j, side)], node_map[(r_n, j + 1, side)]
                tri = [pivot, a, b] if not reversed_order else [b, a, pivot]
                _add_tri(tri, pid_skin)
                n_skin += 1

    for idx in range(len(skin_ribs) - 1):
        _zipper_skin_strip(skin_ribs[idx], skin_ribs[idx + 1], "ext")
        _zipper_skin_strip(skin_ribs[idx], skin_ribs[idx + 1], "int")

    # -- Spar webs (span the full structural region) ---------------------------
    for si in range(n_spars):
        up, lo = spar_upper[si], spar_lower[si]
        for i in range(len(up) - 1):
            _add_q(up[i], up[i + 1], lo[i + 1], lo[i], pid_web_base + si)
            n_fweb += 1

    # -- Rib panels --------------------------------------------------------------
    skin_ribs_main = [i for i in skin_ribs if i >= skin_start]
    skin_ribs_main_set = set(skin_ribs_main)
    y_break = wsg.y_break
    n_inboard_main = sum(1 for i in skin_ribs_main if stations[i].y_station <= y_break)

    for i in all_structural:
        st = stations[i]
        is_full = st.is_full
        j_last_spar = st.j_spars[-1] if st.j_spars else -1
        n_chord = len(st.extrados)
        pid_rib = pid_main_rib if is_full else pid_sec_rib

        if i in skin_ribs_main_set and j_last_spar != -1:
            j_end = j_last_spar
        else:
            j_end = (
                j_last_spar if (not is_full and j_last_spar != -1) else (n_chord - 1)
            )

        for j in range(1, j_end):  # j=0 (LE) skipped: ext==int node there
            n1 = node_map[(i, j, "ext")]
            n2 = node_map[(i, j + 1, "ext")]
            n3 = node_map[(i, j + 1, "int")]
            n4 = node_map[(i, j, "int")]
            _add_q(n1, n2, n3, n4, pid_rib)
            n_rib_elems += 1

    # -- Optional trailing-edge panels (configurable pattern) -------------------
    te_indices: List[int] = []
    for pos, i in enumerate(skin_ribs_main):
        st = stations[i]
        if not _te_rib_selected(
            cfg.te_rib_mode, pos, st.y_station, y_break, n_inboard_main
        ):
            continue
        j_last_spar = st.j_spars[-1] if st.j_spars else -1
        if j_last_spar < 0:
            continue
        n_chord = len(st.extrados)
        if j_last_spar >= n_chord - 1:
            continue
        for j in range(j_last_spar, n_chord - 1):
            n1, n2 = node_map[(i, j, "ext")], node_map[(i, j + 1, "ext")]
            n3, n4 = node_map[(i, j + 1, "int")], node_map[(i, j, "int")]
            _add_q(n1, n2, n3, n4, pid_sec_rib)
            n_te += 1
        te_indices.append(i)

    # -- TE closing strip between consecutive full skin ribs --------------------
    n_te_strip = 0
    for idx in range(len(skin_ribs) - 1):
        r_c, r_n = skin_ribs[idx], skin_ribs[idx + 1]
        if stations[r_c].is_full and stations[r_n].is_full:
            lc, ln = len(stations[r_c].extrados) - 1, len(stations[r_n].extrados) - 1
            n1, n2 = node_map[(r_c, lc, "ext")], node_map[(r_n, ln, "ext")]
            n3, n4 = node_map[(r_n, ln, "int")], node_map[(r_c, lc, "int")]
            _add_q(n1, n2, n3, n4, pid_te_strip)
            n_te_strip += 1

    # -- Tapered CBAR spar caps ---------------------------------------------------
    cap_orient = [1.0, 0.0, 0.0]
    coords_lookup = {
        nid: np.array(g.xyz, dtype=float) for nid, g in model.nodes.items()
    }
    for si in range(n_spars):
        up, lo = spar_upper[si], spar_lower[si]
        root_sizing = sizing.spars[si]
        bf_root, tf_root = float(root_sizing.w_cap[0]), float(root_sizing.t_cap[0])
        pid_cap_family = pid_cap_base + si * 1000

        for i in range(len(up) - 1):
            n1, n2, n1_lo = up[i], up[i + 1], lo[i]
            xyz1, xyz2, xyz1_lo = (
                coords_lookup[n1],
                coords_lookup[n2],
                coords_lookup[n1_lo],
            )
            y_mid = (xyz1[1] + xyz2[1]) / 2.0
            eta_mid = float(np.clip(y_mid / wsg.semi_span, 0.0, 1.0))
            h_local = max(float(np.linalg.norm(xyz1 - xyz1_lo)), 0.05)

            taper = float(
                _cap_taper(
                    np.array([eta_mid]),
                    cfg.cap_taper_eta_lock,
                    cfg.cap_taper_tip_fraction,
                )[0]
            )
            tf_loc = min(tf_root * taper, h_local / 3.0)
            bf_loc = max(bf_root * taper, tf_loc)

            pid_local = pid_cap_family + i
            model.add_pbarl(
                pid_local,
                mid_cap,
                "I",
                [h_local, bf_loc, bf_loc, 0.001, tf_loc, tf_loc],
            )
            model.add_cbar(eid, pid_local, [n1, n2], x=cap_orient, g0=None, offt="GGG")
            eid += 1
            n_cap += 1

    # -- Root SPC (6-DOF) ---------------------------------------------------------
    root = stations[0]
    spc_nodes: List[int] = []
    for j in range(len(root.extrados)):
        spc_nodes.append(node_map[(0, j, "ext")])
    for j in range(len(root.intrados)):
        nid = node_map[(0, j, "int")]
        if nid not in spc_nodes:
            spc_nodes.append(nid)
    model.add_spc1(1, "123456", spc_nodes)

    # -- Engine point masses (CONM2), one per wing-mounted engine ------------------
    engine_loads = loads.engine_point_loads_n(engine_cfg, mass_cfg, req)
    conm2_nids: List[int] = []
    for y_eng, m_eng in engine_loads:
        nearest_i = min(
            all_structural, key=lambda i: abs(stations[i].y_station - abs(y_eng))
        )
        st = stations[nearest_i]
        j_front = st.j_spars[0] if st.j_spars and st.j_spars[0] >= 0 else 0
        eng_nid = node_map[(nearest_i, j_front, "int")]
        chord_here = _nominal_chord(st.eta)
        offset = [
            -0.15 * chord_here,
            0.0,
            -1.0,
        ]  # generic: pylon hangs ahead of / below the front spar
        model.add_conm2(eid, eng_nid, m_eng, cid=0, X=offset)
        conm2_nids.append(eng_nid)
        eid += 1

    model.add_param("GRDPNT", [0])
    model.add_param("AUTOSPC", ["YES"])
    model.add_param("POST", [-1])

    # -- RBE3 rivets on transition ribs -------------------------------------------
    rbe3_count = _add_transition_rbe3(
        model,
        stations,
        node_map,
        skin_ribs_set,
        transition_ribs,
        wsg.semi_span,
        sizing.num_ribs,
        eid,
    )

    # -- Health checks --------------------------------------------------------------
    report = _check_mesh_health(
        model,
        stations,
        all_structural,
        spar_upper,
        wsg,
        n_cquad4=sum(1 for e in model.elements.values() if e.type == "CQUAD4"),
        n_ctria3=sum(1 for e in model.elements.values() if e.type == "CTRIA3"),
    )
    report.warnings = warnings + report.warnings
    report.rbe3_count = rbe3_count
    report.n_nodes = len(model.nodes)
    report.n_elements = len(model.elements)

    # -- Node index for the NASTRAN case-control/monitor cards --------------------
    front_spar_upper = spar_upper[0]
    node_index = MeshNodeIndex(
        root_nid=front_spar_upper[0],
        tip_nid=front_spar_upper[-1],
        kink_nid=min(
            front_spar_upper, key=lambda n: abs(coords_lookup[n][1] - wsg.y_break)
        ),
        spar_upper_nids=spar_upper,
        spar_lower_nids=spar_lower,
        engine_nids=conm2_nids,
    )

    return model, report, node_index


def _add_transition_rbe3(
    model: BDF,
    stations: List[RibStation],
    node_map: Dict[Tuple[int, int, str], int],
    skin_ribs_set: set,
    transition_ribs: List[int],
    semi_span: float,
    num_ribs: int,
    eid_start: int,
) -> int:
    """Ties each transition-rib node to its 3 nearest full-length skin nodes
    via RBE3 (weighted-average rigid element) -- direct port of
    02_mesh.py's transition-rib rivet logic. This is what makes a physically
    shorter (truncated) rib move and deform together with the skin around it
    instead of floating disconnected -- the fix for the warping failure mode
    this module's docstring describes."""
    skin_nids: List[int] = []
    skin_coords: List[np.ndarray] = []
    seen: set = set()
    for (rib_idx, j, side), nid in node_map.items():
        if rib_idx in skin_ribs_set and nid not in seen:
            xyz = np.array(model.nodes[nid].xyz, dtype=float)
            if xyz[1] < _Y_ROOT_EXCL:
                continue
            seen.add(nid)
            skin_nids.append(nid)
            skin_coords.append(xyz)

    if not skin_nids or not transition_ribs:
        return 0

    skin_nids_arr = np.array(skin_nids, dtype=int)
    skin_coords_arr = np.array(skin_coords, dtype=float)
    rib_spacing_est = semi_span / max(num_ribs, 1)
    y_band = 3.0 * rib_spacing_est

    eid = eid_start
    count = 0
    processed: set = set()
    for rib_idx in transition_ribs:
        st = stations[rib_idx]
        j_last_spar = st.j_spars[-1] if st.j_spars else -1
        j_end = (
            j_last_spar
            if (not st.is_full and j_last_spar != -1)
            else (len(st.extrados) - 1)
        )
        rib_y = st.y_station

        local_mask = np.abs(skin_coords_arr[:, 1] - rib_y) <= y_band
        if int(local_mask.sum()) < 3:
            local_mask = np.ones(len(skin_coords_arr), dtype=bool)
        local_coords = skin_coords_arr[local_mask]
        local_nids = skin_nids_arr[local_mask]

        for side in ("ext", "int"):
            for j in range(1, j_end + 1):
                nid_rib = node_map.get((rib_idx, j, side))
                if nid_rib is None or nid_rib in processed:
                    continue
                processed.add(nid_rib)
                xyz_rib = np.array(model.nodes[nid_rib].xyz, dtype=float)
                dists = np.linalg.norm(local_coords - xyz_rib, axis=1)
                n_cands = min(3, len(local_nids))
                closest = local_nids[np.argsort(dists)[:n_cands]].tolist()
                model.add_rbe3(eid, nid_rib, "123456", [1.0], ["123456"], [closest])
                eid += 1
                count += 1
    return count


def _check_mesh_health(
    model: BDF,
    stations: List[RibStation],
    all_structural: List[int],
    spar_upper: List[List[int]],
    wsg: WingStructureGeometry,
    n_cquad4: int,
    n_ctria3: int,
) -> MeshHealthReport:
    """The five geometric health checks that originally caught the skin-
    warping bug class -- ported from 02_mesh.py, not treated as optional."""
    report = MeshHealthReport(n_cquad4=n_cquad4, n_ctria3=n_ctria3)
    warnings: List[str] = []
    coords = {nid: np.array(g.xyz, dtype=float) for nid, g in model.nodes.items()}

    # 1) Rib-LE perpendicularity -- a self-consistency regression check: the
    # realized LE->TE vector of each rib's own extrados array (built from
    # WingStructureGeometry.rib_vector) should come out perpendicular to the
    # INDEPENDENTLY-formulated leading-edge tangent (le_direction) for every
    # rib except the root (deliberately streamwise, see rib_vector's own
    # docstring) -- this only fails if the two direction formulas have
    # drifted out of sync with each other or with how nodes were placed. ----
    n_perp_warn = 0
    for i in all_structural:
        st = stations[i]
        if st.eta <= 1e-9:
            continue  # root rib is deliberately streamwise, not perpendicular to the LE
        ext = st.extrados
        if len(ext) < 2:
            continue
        rib_vec = ext[-1][:2] - ext[0][:2]
        rib_len = float(np.linalg.norm(rib_vec))
        if rib_len < 1e-10:
            continue
        rib_dir_actual = rib_vec / rib_len
        le_dir = np.array(wsg.le_direction(st.eta))
        dot = float(np.dot(rib_dir_actual, le_dir))
        if abs(dot) < 0.05:
            continue
        n_perp_warn += 1
        warnings.append(
            f"Rib {i} (y={st.y_station:.2f} m) not perpendicular to local LE (dot={dot:+.3f})."
        )
    report.n_perp_warnings = n_perp_warn

    # 2) CQUAD4 warping coefficient ----------------------------------------------
    wc_list = []
    for elem in model.elements.values():
        if elem.type != "CQUAD4":
            continue
        n1, n2, n3, n4 = elem.node_ids
        p = np.stack([coords[n1], coords[n2], coords[n3], coords[n4]])
        d1, d2 = p[2] - p[0], p[3] - p[1]
        normal = np.cross(d1, d2)
        nn = np.linalg.norm(normal)
        if nn < 1e-12:
            continue
        normal = normal / nn
        h = np.abs((p - p.mean(axis=0)) @ normal).max()
        wc_list.append(h / (2.0 * (np.linalg.norm(d1) + np.linalg.norm(d2))))
    if wc_list:
        wc_arr = np.array(wc_list)
        report.warping_max = float(wc_arr.max())
        report.warping_mean = float(wc_arr.mean())
        report.n_warping_bad = int(np.sum(wc_arr > WARPING_THRESHOLD))
        if report.n_warping_bad:
            warnings.append(
                f"{report.n_warping_bad} CQUAD4 elements exceed the warping threshold "
                f"({WARPING_THRESHOLD:.0%}); max={report.warping_max:.3f}."
            )

    # 3) CTRIA3 degenerate-triangle detection (hard failure) ---------------------
    n_degenerate = 0
    for elem in model.elements.values():
        if elem.type != "CTRIA3":
            continue
        nids = elem.node_ids
        if len(set(nids)) < 3:
            n_degenerate += 1
            continue
        p1, p2, p3 = coords[nids[0]], coords[nids[1]], coords[nids[2]]
        area = 0.5 * float(np.linalg.norm(np.cross(p2 - p1, p3 - p1)))
        if area < 1e-9:
            n_degenerate += 1
    if n_ctria3 > 0:
        report.triangle_ratio = n_ctria3 / max(n_ctria3 + n_cquad4, 1)
    if n_degenerate > 0:
        raise ValueError(
            f"{n_degenerate} CTRIA3 elements are degenerate (zero area or duplicate nodes) -- "
            "the zipper-triangle skin-bridging logic in wing_mesh_bdf.py produced an invalid mesh."
        )

    # 4) Spar XY-straightness -------------------------------------------------------
    y_break = wsg.y_break
    n_spar_warn = 0
    straightness: Dict[float, float] = {}
    for si, frac in enumerate(wsg.spar_fracs):
        nids_upper = spar_upper[si]
        if len(nids_upper) < 3:
            continue
        pts = np.array([coords[n] for n in nids_upper])
        ys = pts[:, 1]
        ki = int(np.argmin(np.abs(ys - y_break)))
        ki = max(1, min(ki, len(pts) - 2))
        max_dev = 0.0
        for seg in (pts[: ki + 1], pts[ki:]):
            if len(seg) < 3:
                continue
            p0_xy, p1_xy = seg[0, :2], seg[-1, :2]
            ab = p1_xy - p0_xy
            ab_sq = float(np.dot(ab, ab))
            if ab_sq < 1e-20:
                continue
            for p in seg[1:-1]:
                ap = p[:2] - p0_xy
                t = float(np.dot(ap, ab)) / ab_sq
                closest = p0_xy + t * ab
                max_dev = max(max_dev, float(np.linalg.norm(p[:2] - closest)))
        straightness[frac] = max_dev
        if max_dev >= 1e-3:
            n_spar_warn += 1
            warnings.append(
                f"Spar x/c={frac:.2f} deviates {max_dev * 1000:.2f} mm from straight."
            )
    report.n_spar_straightness_warnings = n_spar_warn
    report.spar_straightness_max_dev_m = straightness

    # 5) No node at Y < 0 (hard failure) ---------------------------------------------
    below = [
        (nid, node.xyz[1]) for nid, node in model.nodes.items() if node.xyz[1] < -0.001
    ]
    if below:
        worst = min(v for _, v in below)
        raise ValueError(
            f"{len(below)} nodes have Y < 0 (worst={worst:.4f} m) -- the wing must not extend past the "
            "root. Check WingStructureGeometry.get_rib_lengths' root-plane truncation."
        )

    report.warnings = warnings
    return report
