# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""NASTRAN static/modal/vibration solves: BDF case-control generation, an
opt-in subprocess runner, and OP2/F06 result reading.

Generalizes ``Reference Scripts/03_simulation.py`` (SOL 101/103/111 BDF
builders) and the NASTRAN-reading halves of ``04_postprocessing.py``/
``05_validation.py``. Mirrors :mod:`alas.physics.mses_analysis`'s
subprocess pattern exactly: resolve a user-configured executable path,
return a non-"ok" ``status`` (never raise) if it's missing, the run times
out, or the ``.f06`` reports a fatal error, so a normal pipeline run never
fails because NASTRAN wasn't installed or didn't converge -- the same
contract :class:`~alas.physics.mses_analysis.MSESPolarResult` and
:class:`~alas.integration.suave_bridge.MissionResult` already use.

Running a real solve (``StructuresConfig.run_nastran``) requires a real
NASTRAN install and cannot be exercised on a machine without one; the BDF
text this module writes is structurally verified (``pyNastran`` round-trip)
but the actual subprocess solve is the user's own machine's job.
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import warnings as _warnings
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional

import numpy as np

from . import _nastran_compat  # noqa: F401  (numpy 2.x shim, must import before pyNastran)
from pyNastran.bdf.bdf import BDF

from ..config.requirements import DesignRequirements
from ..config.structures_config import StructuresConfig
from ..geometry.wing_mesh_bdf import MeshNodeIndex
from ..paths import resolve_tool_exe
from ..proc import no_window_kwargs
from ..physics import structural_loads as loads

MONITOR_LABELS = ("root", "kink", "engine", "tip")


# ============================================================================
# Free-field card formatting (direct port of 03_simulation.py's helpers)
# ============================================================================


def _f(v: float) -> str:
    """Format a float for NASTRAN free-field (must contain a decimal point;
    a bare integer like '500' triggers USER FATAL 9994)."""
    if v == 0.0:
        return "0."
    s = f"{float(v):.8g}"
    if "e" in s.lower():
        s = f"{float(v):.6f}".rstrip("0")
        if s.endswith("."):
            s += "0"
    elif "." not in s:
        s += "."
    return s


def _force_lines(sid: int, forces: Dict[int, float], z_sign: float) -> List[str]:
    return [
        f"FORCE,{sid},{nid},0,{_f(abs(f) * z_sign)},0.,0.,1."
        for nid, f in forces.items()
    ]


def _elliptic_forces_by_y(
    nid_y: List[tuple], total_force_n: float, semi_span: float
) -> Dict[int, float]:
    """Distributes ``total_force_n`` over ``[(nid, y_m), ...]`` using the
    same half-ellipse shape as
    :func:`alas.physics.structural_loads.elliptic_distributed_load`,
    discretized onto the mesh's own front-spar node line (the line the
    reference scripts apply aero FORCE cards along)."""
    ys = np.array([y for _, y in nid_y], dtype=float)
    q = np.sqrt(np.clip(1.0 - (ys / max(semi_span, 1e-9)) ** 2, 0.0, 1.0))
    dy = np.abs(np.gradient(ys)) if len(ys) > 1 else np.ones(1)
    weights = q * dy
    s = weights.sum()
    if s < 1e-15:
        weights = np.ones_like(q)
        s = weights.sum()
    weights = weights / s * total_force_n
    return {nid: float(w) for (nid, _), w in zip(nid_y, weights)}


def _monitor_set(node_index: MeshNodeIndex) -> Dict[str, int]:
    engine_nid = (
        node_index.engine_nids[0] if node_index.engine_nids else node_index.kink_nid
    )
    return {
        "root": node_index.root_nid,
        "kink": node_index.kink_nid,
        "engine": engine_nid,
        "tip": node_index.tip_nid,
    }


def _node_y(model: BDF, nid: int) -> float:
    return float(model.nodes[nid].xyz[1])


# ============================================================================
# SOL 101 -- Linear static (pull-up / push-down / level, from structural_loads)
# ============================================================================


def build_sol101_bulk(
    model: BDF,
    node_index: MeshNodeIndex,
    req: DesignRequirements,
    cfg: StructuresConfig,
    mesh_include: str,
) -> str:
    front_upper = node_index.spar_upper_nids[0]
    semi_span = _node_y(model, node_index.tip_nid)
    nid_y = [(n, _node_y(model, n)) for n in front_upper]

    cases = loads.load_cases(req, cfg.additional_safety_factor)
    sid_of = {c.name: i + 1 for i, c in enumerate(cases)}

    out: List[str] = [
        "SOL 101",
        "CEND",
        "$",
        "TITLE = ALAS Wingbox -- Static Analysis",
        "ECHO = NONE",
        "$",
    ]
    for c in cases:
        sid = sid_of[c.name]
        out += [
            f"SUBCASE {sid}",
            f"  TITLE = {c.name} (n={c.load_factor:+.2f})",
            f"  LOAD = {sid}",
            "  SPC = 1",
            "  STRESS(VONMISES,CORNER) = ALL",
            "  DISPLACEMENT = ALL",
            "  SPCFORCE = ALL",
            "  OLOAD = ALL",
            "$",
        ]
    out += ["BEGIN BULK", "PARAM,COUPMASS,1", f"INCLUDE '{mesh_include}'", "$"]

    for c in cases:
        sid = sid_of[c.name]
        # Non-overlapping SID ranges: LOAD ids are 1..3 (few, small), GRAV ids
        # start at 100, FORCE ids at 200 -- guarantees no cross-type SID
        # collision regardless of how many load cases exist (an earlier
        # sid*10/sid*20 scheme collided: pull-up's force_sid=20 == push-
        # down's grav_sid=20, which would have silently merged the two
        # cards' load sets under one ID in the LOAD combination cards).
        grav_sid, force_sid = 100 + sid, 200 + sid
        g_mag = abs(c.load_factor) * req.gravity_m_s2
        # Aero lift acts in the direction of the (signed) case total force; the
        # structure's own inertial relief (GRAV) always opposes gravity, i.e.
        # acts opposite the load-factor's own sign convention -- a positive n
        # (pull-up) means the airframe accelerates upward, so GRAV must point
        # -Z (down) to represent that inertial reaction, matching 03_simulation.py.
        z_sign_aero = 1.0 if c.total_force_n >= 0 else -1.0
        z_sign_grav = -1.0 if c.load_factor >= 0 else 1.0
        out += [
            f"$ ---- {c.name}: n={c.load_factor:+.2f} ----",
            f"LOAD,{sid},1.0,1.0,{grav_sid},1.0,{force_sid}",
            f"GRAV,{grav_sid},0,{_f(g_mag)},0.,0.,{_f(z_sign_grav)}",
        ]
        forces = _elliptic_forces_by_y(nid_y, abs(c.total_force_n), semi_span)
        out += _force_lines(force_sid, forces, z_sign_aero)
        out.append("$")

    out.append("ENDDATA")
    return "\n".join(out)


# ============================================================================
# SOL 103 -- Normal modes
# ============================================================================


def build_sol103_bulk(cfg: StructuresConfig, mesh_include: str) -> str:
    return "\n".join(
        [
            "SOL 103",
            "CEND",
            "$",
            "TITLE = ALAS Wingbox -- Normal Modes",
            "ECHO = NONE",
            "$",
            "SUBCASE 1",
            "  TITLE = Normal modes extraction",
            "  SPC = 1",
            "  METHOD = 1",
            "  MEFFMASS(PRINT,SUMMARY) = YES",
            "  RESVEC = YES",
            "  DISPLACEMENT = ALL",
            "  SPCFORCE = ALL",
            "$",
            "BEGIN BULK",
            "PARAM,COUPMASS,1",
            f"INCLUDE '{mesh_include}'",
            "$",
            f"EIGRL,1,,,{cfg.n_modes}",
            "$",
            "ENDDATA",
        ]
    )


# ============================================================================
# SOL 111 -- Modal frequency response (sine sweep + random vibration)
# ============================================================================


def _dynamic_bulk(
    cfg: StructuresConfig, excitation_nid: int, include_random: bool
) -> List[str]:
    out = ["$ ---- Modal extraction ----", f"EIGRL,1,,,{cfg.n_modes}", "$"]
    n_steps = max(1, int(cfg.freq_sweep_max_hz / max(cfg.freq_step_hz, 1e-6)))
    out += [
        f"$ ---- Frequency range: {cfg.freq_step_hz} to {cfg.freq_sweep_max_hz} Hz ({n_steps} steps) ----",
        f"FREQ1,3,{_f(cfg.freq_step_hz)},{_f(cfg.freq_step_hz)},{n_steps}",
        "$",
        f"$ ---- Damping: {cfg.modal_damping_ratio * 100:.0f}% critical ----",
        "TABDMP1,4,CRIT",
        f"+,{_f(0.0)},{_f(cfg.modal_damping_ratio)},{_f(cfg.freq_sweep_max_hz)},{_f(cfg.modal_damping_ratio)},ENDT",
        "$",
        "$ ---- Unit-amplitude TABLED1 ----",
        "TABLED1,200,LINEAR,LINEAR",
        f"+,{_f(0.0)},{_f(1.0)},{_f(cfg.freq_sweep_max_hz)},{_f(1.0)},ENDT",
        "$",
        f"$ ---- DAREA: unit force (1 N) in Z at excitation node {excitation_nid} ----",
        f"DAREA,1000,{excitation_nid},3,1.",
        "$",
        "RLOAD1,100,1000,0.,0.,200,0.,LOAD",
        "$",
        "DLOAD,300,1.,1.,100",
        "$",
    ]
    if include_random:
        psd_si = cfg.psd_base_g2_per_hz * 9.81**2
        out += [
            f"$ ---- Random PSD: {cfg.psd_base_g2_per_hz} g^2/Hz = {psd_si:.4f} (m/s^2)^2/Hz ----",
            "TABRND1,500,LOG,LOG",
            f"+,{_f(0.0)},{_f(psd_si)},{_f(cfg.freq_sweep_max_hz)},{_f(psd_si)},ENDT",
            "$",
            "RANDPS,600,100,100,1.,0.,500",
            "$",
        ]
    return out


def build_sol111_sine_bulk(
    cfg: StructuresConfig, node_index: MeshNodeIndex, mesh_include: str
) -> str:
    monitors = _monitor_set(node_index)
    monitor_ids = ", ".join(str(n) for n in sorted(set(monitors.values())))
    excitation_nid = monitors["engine"]
    out = [
        "SOL 111",
        "CEND",
        "$",
        "TITLE = ALAS Wingbox -- Sine Sweep",
        "ECHO = NONE",
        "$",
        f"SET 9000 = {monitor_ids}",
        "$",
        "SUBCASE 1",
        "  TITLE = Modal frequency response -- unit harmonic force at excitation node",
        "  SPC = 1",
        "  METHOD = 1",
        "  MEFFMASS(PRINT,SUMMARY) = YES",
        "  RESVEC = YES",
        "  FREQUENCY = 3",
        "  SDAMP = 4",
        "  DLOAD = 300",
        "  DISPLACEMENT(PHASE,SORT2) = 9000",
        "  ACCELERATION(PHASE,SORT2) = 9000",
        "  STRESS(SORT2,PHASE) = ALL",
        "$",
        "BEGIN BULK",
        "PARAM,COUPMASS,1",
        f"INCLUDE '{mesh_include}'",
        "$",
    ]
    out += _dynamic_bulk(cfg, excitation_nid, include_random=False)
    out += ["ENDDATA"]
    return "\n".join(out)


def build_sol111_random_bulk(
    cfg: StructuresConfig, node_index: MeshNodeIndex, mesh_include: str
) -> str:
    monitors = _monitor_set(node_index)
    monitor_ids = ", ".join(str(n) for n in sorted(set(monitors.values())))
    excitation_nid = monitors["engine"]
    out = [
        "SOL 111",
        "CEND",
        "$",
        "TITLE = ALAS Wingbox -- Random Vibration",
        "ECHO = NONE",
        "$",
        f"SET 9000 = {monitor_ids}",
        "$",
        "SUBCASE 1",
        "  TITLE = Random response -- white noise excitation",
        "  SPC = 1",
        "  METHOD = 1",
        "  MEFFMASS(PRINT,SUMMARY) = YES",
        "  RESVEC = YES",
        "  FREQUENCY = 3",
        "  SDAMP = 4",
        "  DLOAD = 300",
        "  RANDOM = 600",
        "  DISPLACEMENT(SORT2,PSDF,CRMS) = 9000",
        "  ACCELERATION(SORT2,PSDF,CRMS) = 9000",
        "  STRESS(SORT2,CRMS) = ALL",
        "$",
        "BEGIN BULK",
        "PARAM,COUPMASS,1",
        f"INCLUDE '{mesh_include}'",
        "$",
    ]
    out += _dynamic_bulk(cfg, excitation_nid, include_random=True)
    out += ["ENDDATA"]
    return "\n".join(out)


# ============================================================================
# Subprocess runner + F06 fatal-message scan
# ============================================================================


@dataclass
class NastranRunOutcome:
    """Rich diagnostic for one SOL run -- unlike a bare bool, ``detail``
    always says *why* a run was judged not-ok (timeout, nonzero exit,
    missing .f06, a fatal message, or a subprocess that never launched),
    since an opaque "didn't converge" is useless for debugging a real
    install (which behavior can't be exercised on this development
    machine at all -- see the module docstring)."""

    ok: bool
    detail: str


def _tail(text: str, n_lines: int = 15) -> str:
    lines = text.strip().splitlines()
    return "\n".join(lines[-n_lines:]) if lines else "(empty)"


def _kill_process_tree(proc: subprocess.Popen) -> None:
    """Terminates ``proc`` and all its descendants.

    A plain ``Popen.kill()`` only signals the immediate child -- NASTRAN's
    ``nastran.exe`` is a front-end launcher that forks a second-level
    ``nastran.exe``/``analysis.exe`` solver process which survives its own
    parent's death (confirmed directly: killing just the top-level PID left
    an orphaned solver process still burning CPU/memory and holding a
    license seat). Neither Python's stdlib nor this project has ``psutil``,
    so on Windows shell out to ``taskkill``'s own recursive ``/T`` (tree)
    kill; on POSIX, rely on the process group started with
    ``start_new_session=True`` below and signal that whole group instead.
    """
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(proc.pid)],
            capture_output=True,
            text=True,
            **no_window_kwargs(),
        )
    else:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        except ProcessLookupError:
            pass


def _run_nastran(bdf_path: Path, exe_path: Path, timeout_s: float) -> NastranRunOutcome:
    """Invokes ``exe_path`` on ``bdf_path``. Never raises -- a timeout or
    subprocess error is treated the same as "didn't converge", but always
    with a concrete reason attached.

    ``sdirectory``/scratch-database location is passed explicitly rather
    than left to the install's own ``.rcf`` config default: at least one
    real MSC Nastran Student Edition install was found to ship a broken
    default (``sdirectory=e:memory=estimate``, pointing at a drive that
    doesn't exist on the machine it was installed on), which fails before
    any solve even starts. Overriding it with the BDF's own directory
    (guaranteed to exist, since the BDF was just written there) sidesteps
    that class of installer misconfiguration entirely.
    """
    # Absolute path: NASTRAN's own internal validation of this keyword doesn't
    # necessarily resolve a relative one against the same cwd the subprocess
    # was launched with, so a relative sdirectory can spuriously fail its
    # "this directory does not exist" check even when it does.
    cmd = [
        str(exe_path),
        bdf_path.name,
        "scr=yes",
        f"sdirectory={bdf_path.parent.resolve()}",
    ]
    popen_kwargs = (
        dict(no_window_kwargs()) if os.name == "nt" else {"start_new_session": True}
    )
    try:
        proc = subprocess.Popen(
            cmd,
            cwd=str(bdf_path.parent),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            **popen_kwargs,
        )
    except OSError as exc:
        return NastranRunOutcome(False, f"Failed to launch {exe_path}: {exc}")

    try:
        stdout, stderr = proc.communicate(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        _kill_process_tree(proc)
        proc.communicate()  # reap the process, discard whatever it had buffered
        return NastranRunOutcome(
            False,
            f"Timed out after {timeout_s:.0f}s running: {' '.join(cmd)} "
            "(solver process tree has been force-killed)",
        )

    if proc.returncode != 0:
        detail = (
            f"{exe_path.name} exited with code {proc.returncode} (cwd={bdf_path.parent}).\n"
            f"stdout (tail):\n{_tail(stdout)}\nstderr (tail):\n{_tail(stderr)}"
        )
        return NastranRunOutcome(False, detail)

    f06_path = bdf_path.with_suffix(".f06")

    if not f06_path.exists():
        return NastranRunOutcome(
            False,
            f"{exe_path.name} exited 0 but wrote no {f06_path.name} in {bdf_path.parent} -- if this "
            "executable is a GUI-mode launcher (e.g. a *w.exe variant) it may have opened a window and "
            "returned immediately instead of blocking until the solve finished, or it may write output "
            "to a different working directory than the one it was launched from. "
            f"stdout (tail):\n{_tail(stdout)}\nstderr (tail):\n{_tail(stderr)}",
        )

    with open(f06_path, encoding="utf-8", errors="replace") as fh:
        content = fh.read()
    fatal_lines = [
        ln.strip() for ln in content.splitlines() if "USER FATAL MESSAGE" in ln
    ]
    if fatal_lines:
        return NastranRunOutcome(
            False,
            f"{f06_path.name} reports {len(fatal_lines)} fatal message(s):\n"
            + "\n".join(fatal_lines[:5]),
        )
    return NastranRunOutcome(True, "ok")


# ============================================================================
# Result containers
# ============================================================================


@dataclass
class NastranStaticResult:
    status: str = "not_run"  # "ok" | "error" | "not_run"
    error: Optional[str] = None
    tip_deflection_m: Dict[str, float] = field(default_factory=dict)
    root_von_mises_max_pa: Dict[str, float] = field(default_factory=dict)


@dataclass
class NastranModesResult:
    status: str = "not_run"
    error: Optional[str] = None
    frequencies_hz: List[float] = field(default_factory=list)
    # Front-spar T3 (out-of-plane) displacement per mode, normalized peak=1,
    # sampled at mode_shape_y_m -- lets a caller overlay the real NASTRAN
    # shape against the Rayleigh trial shape it's supposedly estimating,
    # the same front-spar-line convention 05_validation.py's own mode-shape
    # panel used. Same length/order as frequencies_hz.
    mode_shapes: List[np.ndarray] = field(default_factory=list)
    mode_shape_y_m: Optional[np.ndarray] = None


@dataclass
class NastranVibrationResult:
    status: str = "not_run"
    error: Optional[str] = None
    frf_freq_hz: Optional[np.ndarray] = None
    frf_tip_abs_m_per_n: Optional[np.ndarray] = None
    peak_frf: float = 0.0
    peak_freq_hz: float = 0.0
    miles_rms_m: Dict[str, float] = field(default_factory=dict)
    nastran_rms_m: Dict[str, float] = field(default_factory=dict)


@dataclass
class NastranResults:
    static: NastranStaticResult = field(default_factory=NastranStaticResult)
    modes: NastranModesResult = field(default_factory=NastranModesResult)
    vibration: NastranVibrationResult = field(default_factory=NastranVibrationResult)


# ============================================================================
# OP2 readers
# ============================================================================


def _read_op2(path: Path):
    if not path.exists():
        return None
    from pyNastran.op2.op2 import OP2

    with _warnings.catch_warnings():
        _warnings.simplefilter("ignore")
        op2 = OP2(debug=False)
        op2.read_op2(str(path))
    return op2


def _cquad4_stress_table(op2, sid: int):
    """Return the cquad4_stress result object for ``sid``, or None. Tries
    both pyNastran's v1.3+ nested API and the legacy direct attribute (the
    reference scripts needed both across pyNastran versions)."""
    for getter in (
        lambda: op2.op2_results.stress.cquad4_stress,
        lambda: op2.cquad4_stress,
    ):
        try:
            raw = getter()
            if sid in raw:
                return raw[sid]
        except AttributeError:
            pass
    return None


def _read_static(op2, node_index: MeshNodeIndex, cases) -> NastranStaticResult:
    result = NastranStaticResult(status="ok")
    tip_nid = node_index.tip_nid
    for i, c in enumerate(cases):
        sid = i + 1
        if sid not in op2.displacements:
            continue
        d = op2.displacements[sid]
        nids = d.node_gridtype[:, 0].astype(int)
        hits = np.where(nids == tip_nid)[0]
        if len(hits):
            result.tip_deflection_m[c.name] = float(d.data[0, hits[0], 2])
        stress = _cquad4_stress_table(op2, sid)
        if stress is not None:
            try:
                # Column 7 is the von Mises stress component (CQUAD4 CORNER
                # output layout), matching the reference's own
                # ``_cquad4_vm`` -- not the last column in general.
                result.root_von_mises_max_pa[c.name] = float(
                    np.max(np.abs(stress.data[0, :, 7]))
                )
            except (IndexError, ValueError):
                pass
    return result


def _read_modes(op2, model: BDF, node_index: MeshNodeIndex) -> NastranModesResult:
    """Reads SOL 103 frequencies (filtering out the near-zero/rigid-body
    modes below 0.5 Hz, same threshold 05_validation.py's own
    ``_get_structural_freqs_nastran`` used) plus each surviving mode's
    front-spar T3 shape, so a caller can nearest-frequency-match each
    Rayleigh trial mode against the right NASTRAN mode instead of assuming
    raw index order lines them up -- with ``cfg.n_modes`` (default 30) real
    NASTRAN modes almost always including torsional/local-panel modes
    the 4-entry Rayleigh cantilever trial-shape table has no equivalent
    for, index-order pairing silently compares unrelated modes."""
    result = NastranModesResult(status="ok")
    ev = op2.eigenvectors.get(1)
    if ev is None:
        return result
    freqs = np.array([float(f) for f in ev.mode_cycles])
    keep = np.where(freqs > 0.5)[0]
    result.frequencies_hz = [float(f) for f in freqs[keep]]
    if len(keep) == 0:
        return result

    front_upper = set(node_index.spar_upper_nids[0])
    nids_ev = ev.node_gridtype[:, 0].astype(int)
    mask = np.array([nid in front_upper for nid in nids_ev])
    if not np.any(mask):
        return result

    y_vals = np.array([_node_y(model, int(nid)) for nid in nids_ev[mask]])
    order = np.argsort(y_vals)
    result.mode_shape_y_m = y_vals[order]
    for mi in keep:
        t3 = ev.data[mi, mask, 2].astype(float)[order]
        norm = float(np.max(np.abs(t3))) or 1.0
        result.mode_shapes.append(t3 / norm)
    return result


def _read_vibration(
    op2_sine,
    op2_random,
    modal_freqs: List[float],
    monitors: Dict[str, int],
    damping_ratio: float,
    psd_base_g2_per_hz: float,
) -> NastranVibrationResult:
    result = NastranVibrationResult(status="ok")
    tip_nid = monitors["tip"]
    g = 9.81
    psd_si = psd_base_g2_per_hz * g**2

    if op2_sine is not None and 1 in op2_sine.displacements:
        d = op2_sine.displacements[1]
        freqs = d.freqs.astype(float)
        nids = d.node_gridtype[:, 0].astype(int)
        hits = np.where(nids == tip_nid)[0]
        if len(hits):
            frf = np.abs(d.data[:, int(hits[0]), 2].astype(complex))
            result.frf_freq_hz = freqs
            result.frf_tip_abs_m_per_n = frf
            pk = int(np.argmax(frf))
            result.peak_frf = float(frf[pk])
            result.peak_freq_hz = float(freqs[pk])

        f1 = modal_freqs[0] if modal_freqs else None
        if f1:
            for label, nid in monitors.items():
                hits = np.where(nids == nid)[0]
                if not len(hits):
                    continue
                h_abs = np.abs(d.data[:, int(hits[0]), 2].astype(complex))
                h_pk = float(np.max(h_abs))
                f_pk = float(freqs[np.argmax(h_abs)])
                rms = h_pk * np.sqrt(np.pi * f_pk * psd_si / (4.0 * damping_ratio))
                result.miles_rms_m[label] = rms

    if op2_random is not None and 1 in op2_random.displacements:
        d = op2_random.displacements[1]
        freqs = d.freqs.astype(float)
        nids = d.node_gridtype[:, 0].astype(int)
        for label, nid in monitors.items():
            hits = np.where(nids == nid)[0]
            if not len(hits):
                continue
            h = d.data[:, int(hits[0]), 2].astype(complex)
            s_resp = np.abs(h) ** 2 * psd_si
            result.nastran_rms_m[label] = float(np.sqrt(np.trapezoid(s_resp, freqs)))

    return result


# ============================================================================
# Top-level orchestration
# ============================================================================


def run_nastran_analysis(
    model: BDF,
    node_index: MeshNodeIndex,
    cfg: StructuresConfig,
    req: DesignRequirements,
    work_dir: Path,
    repo_root: Path,
) -> NastranResults:
    """Writes the shared mesh into ``work_dir`` and each enabled solution's
    BDF into its own ``work_dir/<solution>/`` subfolder, runs
    ``nastran_exe_path`` on each (if ``cfg.run_nastran`` and the executable
    is found), and reads back whatever ``.op2`` files exist. Never raises:
    any failure at any stage leaves that solution's result at its default
    ``"not_run"``/``"error"`` status.

    Each solution gets its own subfolder (rather than all 4 solutions'
    ``.bdf``/``.f04``/``.f06``/``.log``/``.op2`` sharing one flat directory)
    because NASTRAN's own per-run scratch/log output -- and, when a run is
    force-killed after a timeout, leftover ``.rcf``/``.aeso``/``.becho``/
    ``.plt``/etc scratch fragments (see :func:`_kill_process_tree`) -- would
    otherwise all land in the same directory and become impossible to tell
    apart by solution. The mesh itself is written once at ``work_dir`` and
    referenced from each subfolder via a relative ``../wing_mesh.bdf``
    INCLUDE (confirmed NASTRAN resolves this correctly relative to cwd).
    """
    work_dir = Path(work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)
    mesh_path = work_dir / "wing_mesh.bdf"
    model.write_bdf(str(mesh_path), size=16, is_double=False)

    results = NastranResults()
    cases = loads.load_cases(req, cfg.additional_safety_factor)

    mesh_include = "../wing_mesh.bdf"
    bdf_specs = []
    if cfg.run_sol_static:
        bdf_specs.append(
            ("sol101", build_sol101_bulk(model, node_index, req, cfg, mesh_include))
        )
    if cfg.run_sol_modes:
        bdf_specs.append(("sol103", build_sol103_bulk(cfg, mesh_include)))
    if cfg.run_sol_vibration_sine:
        bdf_specs.append(
            ("sol111_sine", build_sol111_sine_bulk(cfg, node_index, mesh_include))
        )
    if cfg.run_sol_vibration_random:
        bdf_specs.append(
            ("sol111_random", build_sol111_random_bulk(cfg, node_index, mesh_include))
        )

    written: Dict[str, Path] = {}
    for name, content in bdf_specs:
        sub_dir = work_dir / name
        # Wipe any previous run's contents first: NASTRAN itself auto-
        # versions its own output (.f04/.f06/.log/.op2 -> .f04.1/.f04.2/...)
        # when it finds a same-named file already there from an earlier
        # run, so a stale subfolder left alone accumulates one extra set of
        # files per re-run indefinitely instead of reflecting only the
        # latest solve.
        shutil.rmtree(sub_dir, ignore_errors=True)
        sub_dir.mkdir(parents=True, exist_ok=True)
        p = sub_dir / f"wing_{name}.bdf"
        p.write_text(content, encoding="utf-8")
        written[name] = p

    if not cfg.run_nastran:
        return results

    exe_path = resolve_tool_exe(cfg.nastran_exe_path, Path(repo_root))
    if exe_path is None:
        error = (
            f"NASTRAN executable not found at {Path(cfg.nastran_exe_path)}"
            if str(cfg.nastran_exe_path).strip()
            else "NASTRAN executable not configured (Setup > External Tools); "
            "reporting analytical estimates only"
        )
        if "sol101" in written:
            results.static = NastranStaticResult(status="error", error=error)
        if "sol103" in written:
            results.modes = NastranModesResult(status="error", error=error)
        if "sol111_sine" in written or "sol111_random" in written:
            results.vibration = NastranVibrationResult(status="error", error=error)
        return results

    outcomes: Dict[str, NastranRunOutcome] = {}
    for name, path in written.items():
        outcomes[name] = _run_nastran(path, exe_path, cfg.timeout_s)

    if "sol101" in written:
        try:
            if outcomes["sol101"].ok:
                op2 = _read_op2(written["sol101"].with_suffix(".op2"))
                results.static = (
                    _read_static(op2, node_index, cases)
                    if op2
                    else NastranStaticResult(
                        status="error",
                        error="SOL 101 ran cleanly but no .op2 was produced",
                    )
                )
            else:
                results.static = NastranStaticResult(
                    status="error", error=outcomes["sol101"].detail
                )
        except Exception as exc:
            results.static = NastranStaticResult(
                status="error", error=f"SOL 101 result read failed: {exc}"
            )

    modal_freqs: List[float] = []
    if "sol103" in written:
        try:
            if outcomes["sol103"].ok:
                op2 = _read_op2(written["sol103"].with_suffix(".op2"))
                results.modes = (
                    _read_modes(op2, model, node_index)
                    if op2
                    else NastranModesResult(
                        status="error",
                        error="SOL 103 ran cleanly but no .op2 was produced",
                    )
                )
                modal_freqs = results.modes.frequencies_hz
            else:
                results.modes = NastranModesResult(
                    status="error", error=outcomes["sol103"].detail
                )
        except Exception as exc:
            results.modes = NastranModesResult(
                status="error", error=f"SOL 103 result read failed: {exc}"
            )

    if "sol111_sine" in written or "sol111_random" in written:
        try:
            sine_ok = outcomes.get("sol111_sine")
            random_ok = outcomes.get("sol111_random")
            op2_sine = (
                _read_op2(written["sol111_sine"].with_suffix(".op2"))
                if sine_ok and sine_ok.ok
                else None
            )
            op2_random = (
                _read_op2(written["sol111_random"].with_suffix(".op2"))
                if random_ok and random_ok.ok
                else None
            )
            if op2_sine is None and op2_random is None:
                details = []
                if sine_ok is not None:
                    details.append(f"SOL 111 sine: {sine_ok.detail}")
                if random_ok is not None:
                    details.append(f"SOL 111 random: {random_ok.detail}")
                results.vibration = NastranVibrationResult(
                    status="error", error="\n".join(details)
                )
            else:
                results.vibration = _read_vibration(
                    op2_sine,
                    op2_random,
                    modal_freqs,
                    _monitor_set(node_index),
                    cfg.modal_damping_ratio,
                    cfg.psd_base_g2_per_hz,
                )
        except Exception as exc:
            results.vibration = NastranVibrationResult(
                status="error", error=f"SOL 111 result read failed: {exc}"
            )

    return results
