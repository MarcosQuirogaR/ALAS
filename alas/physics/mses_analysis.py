# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
MSES 2-D airfoil polar analysis -- runs a real coupled viscous/inviscid Euler
+ integral-boundary-layer solve (Mark Drela, MIT) on the optimized design's
root airfoil section, for the Model Comparison tab.

Drives AeroSandbox's own ``aerodynamics.aero_2D.MSES`` wrapper (which pipes
menu keystrokes to the native ``mset``/``mses`` executables via stdin -- no
GUI/X11 window needed for the numerical solve itself, verified directly
against real Windows binaries). Unlike the SUAVE mission bridge, this runs
in-process: MSES is an
external compiled tool invoked as a subprocess, not a Python import, so there
is no numpy/scipy version conflict requiring an isolated venv.

Never raises out of :func:`run_mses_polar` -- like
:class:`alas.integration.suave_bridge.MissionResult`, failures are
reported via ``MSESPolarResult.status``/``error`` so a normal pipeline run
never fails because MSES couldn't converge on a particular (possibly
optimizer-degenerate) airfoil shape.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional

import aerosandbox as asb
import numpy as np

from ..proc import no_window_kwargs


@dataclass
class MSESPolarResult:
    """Result of an MSES alpha sweep on one airfoil section."""

    status: str = "not_run"  # "ok" | "error" | "not_run"
    error: Optional[str] = None

    airfoil_name: str = ""
    mach: float = 0.0
    reynolds: float = 0.0

    alpha_deg: List[float] = field(default_factory=list)
    CL: List[float] = field(default_factory=list)
    CD: List[float] = field(default_factory=list)
    CM: List[float] = field(default_factory=list)
    CDv: List[float] = field(default_factory=list)  # viscous drag
    CDw: List[float] = field(default_factory=list)  # wave drag
    xtr_top: List[float] = field(default_factory=list)  # transition location, upper
    xtr_bot: List[float] = field(default_factory=list)  # transition location, lower

    @property
    def l_over_d(self) -> List[float]:
        return [cl / cd if cd > 1e-9 else 0.0 for cl, cd in zip(self.CL, self.CD)]


def run_mses_polar(
    airfoil: asb.Airfoil,
    mach: float,
    reynolds: float,
    trim_alpha_deg: float,
    mses_config,
    repo_root: Path,
) -> MSESPolarResult:
    """Run an MSES alpha sweep on ``airfoil`` bracketing ``trim_alpha_deg``.

    Parameters
    ----------
    airfoil : the 2-D section to analyse (e.g. the optimized design's morphed
        root section from ``geometry.airfoils.build_section``).
    mach, reynolds : the cruise flow condition (chord-referenced Re).
    trim_alpha_deg : the aircraft's trimmed cruise angle of attack -- the
        sweep is centred here (a 2-D section's local alpha differs from the
        3-D trimmed body alpha, but this keeps the comparison anchored to the
        actual cruise operating point rather than an arbitrary fixed range).
    repo_root : resolves ``mses_config.mses_dir`` (repo-root-relative) to
        absolute executable paths.
    """
    result = MSESPolarResult(
        airfoil_name=getattr(airfoil, "name", "") or "optimized_root_section",
        mach=float(mach),
        reynolds=float(reynolds),
    )
    try:
        from aerosandbox.aerodynamics.aero_2D import MSES

        from ..paths import resolve_tool_dir

        # `repo_root` is retained for signature compatibility, but the tool dir
        # is now resolved via the frozen-build-aware resolver (see alas/
        # paths.py) so `external tools/MSES` is found next to the real app exe,
        # not inside the PyInstaller extraction cache.
        mses_dir = resolve_tool_dir(mses_config.mses_dir)
        mset_exe = mses_dir / "mset.exe"
        mses_exe = mses_dir / "mses.exe"
        mplot_exe = mses_dir / "mplot.exe"
        if not (mset_exe.exists() and mses_exe.exists()):
            result.status = "error"
            result.error = f"MSES executables not found under {mses_dir}"
            return result

        half = max(0.5, mses_config.alpha_sweep_halfwidth_deg)
        n = max(3, mses_config.alpha_sweep_n_points)
        alphas = np.linspace(trim_alpha_deg - half, trim_alpha_deg + half, n)

        import tempfile

        with tempfile.TemporaryDirectory(prefix="alas_mses_") as tmp:
            ms = MSES(
                airfoil=airfoil.repanel(n_points_per_side=80),
                n_crit=mses_config.n_crit,
                xtr_upper=mses_config.xtr_upper,
                xtr_lower=mses_config.xtr_lower,
                max_iter=mses_config.max_iterations,
                mset_command=str(mset_exe),
                mses_command=str(mses_exe),
                mplot_command=str(mplot_exe),
                verbosity=0,
                timeout_mset=mses_config.timeout_mset_s,
                timeout_mses=mses_config.timeout_mses_s,
                mset_n=mses_config.mset_n,
                mset_e=mses_config.mset_e,
                working_directory=tmp,
            )
            raw = ms.run(alpha=alphas, Re=reynolds, mach=mach)

        n_converged = len(np.ravel(raw.get("alpha", [])))
        if n_converged == 0:
            result.status = "error"
            result.error = "MSES did not converge at any swept alpha"
            return result

        result.alpha_deg = list(np.ravel(raw["alpha"]))
        result.CL = list(np.ravel(raw["CL"]))
        result.CD = list(np.ravel(raw["CD"]))
        result.CM = list(np.ravel(raw.get("CM", np.zeros(n_converged))))
        result.CDv = list(np.ravel(raw.get("CDv", np.zeros(n_converged))))
        result.CDw = list(np.ravel(raw.get("CDw", np.zeros(n_converged))))
        result.xtr_top = list(np.ravel(raw.get("xtr_top", np.zeros(n_converged))))
        result.xtr_bot = list(np.ravel(raw.get("xtr_bot", np.zeros(n_converged))))
        result.status = "ok"
        return result
    except KeyError as exc:
        # AeroSandbox's own MSES.run() unconditionally does
        # runs_output.pop("Ma") on its accumulated-results dict; if *none* of
        # the swept alphas converged, that dict is empty and the pop raises a
        # bare KeyError('Ma') before ms.run() ever returns -- so the n==0
        # check above (which handles a *partial* sweep) never gets a chance
        # to run. Reported here with a clear, honest message instead of the
        # cryptic raw "'Ma'" string (mirrors run_mses_pressure_distribution's
        # identical fix for the single-alpha case).
        result.status = "error"
        if str(exc).strip("'\"") == "Ma":
            result.error = "MSES did not converge at any swept alpha"
        else:
            result.error = f"KeyError: {exc}"
        return result
    except Exception as exc:
        result.status = "error"
        result.error = str(exc)
        return result


@dataclass
class MSESPressureResult:
    """Surface pressure (Cp) and local-Mach distribution at one alpha.

    Split into upper/lower surface branches (by sign of the section-local y
    coordinate) for the conventional Cp-vs-x/c plot; wake points (x/c beyond
    the trailing edge) are excluded.
    """

    status: str = "not_run"
    error: Optional[str] = None
    alpha_deg: float = 0.0

    x_upper: List[float] = field(default_factory=list)
    cp_upper: List[float] = field(default_factory=list)
    mach_upper: List[float] = field(default_factory=list)
    x_lower: List[float] = field(default_factory=list)
    cp_lower: List[float] = field(default_factory=list)
    mach_lower: List[float] = field(default_factory=list)

    field_x: List[float] = field(default_factory=list)
    field_y: List[float] = field(default_factory=list)
    field_mach: List[float] = field(default_factory=list)

    airfoil_x: List[float] = field(default_factory=list)
    airfoil_y: List[float] = field(default_factory=list)


def run_mses_pressure_distribution(
    airfoil: asb.Airfoil,
    mach: float,
    reynolds: float,
    alpha_deg: float,
    mses_config,
    repo_root: Path,
    alpha_retry_offsets_deg: Optional[List[float]] = None,
) -> MSESPressureResult:
    """Solve one MSES operating point and extract the surface Cp/Mach distribution.

    Reuses AeroSandbox's own ``MSES`` class for the mesh-generation + flow
    solve (its ``mset``/``mses`` keystroke templates are already correct and
    tested -- see :func:`run_mses_polar`), pointed at a *persistent* working
    directory so the converged case files survive after ``.run()`` returns.
    Then separately drives ``mplot``'s "Dump BLs" option (menu path
    ``12`` -> filename -> ``0`` -> ``0``, verified interactively against the
    real Windows binaries) to
    write the per-panel boundary-layer table (x, y, Cp, local Mach, and
    boundary-layer quantities) to a text file, which is parsed here. This is
    new code (AeroSandbox's own wrapper only returns the scalar polar, not
    the spatial distribution), not a reuse of an existing AeroSandbox method.

    A single fixed alpha has no adjacent sweep point to fall back on the way
    ``run_mses_polar``'s multi-alpha sweep does, so it is the more fragile of
    the two entry points here -- if the exact ``alpha_deg`` fails to converge
    (a fresh mesh/solve at a nearby angle sometimes succeeds where the exact
    target didn't, a known MSES sensitivity), this retries at each offset in
    ``alpha_retry_offsets_deg`` (default a small +/- bracket around the
    target) with a completely fresh working directory/mesh per attempt,
    before giving up. ``result.alpha_deg`` reports whichever angle actually
    converged, so a caller can tell the plot is a nearby approximation, not
    the exact requested point, if a retry was needed.
    """
    if alpha_retry_offsets_deg is None:
        alpha_retry_offsets_deg = [0.0, 0.5, -0.5, 1.0, -1.0]

    result = MSESPressureResult(alpha_deg=float(alpha_deg))
    try:
        from aerosandbox.aerodynamics.aero_2D import MSES
        import shutil
        import subprocess
        import tempfile

        from ..paths import resolve_tool_dir

        mses_dir = resolve_tool_dir(mses_config.mses_dir)
        mset_exe = mses_dir / "mset.exe"
        mses_exe = mses_dir / "mses.exe"
        mplot_exe = mses_dir / "mplot.exe"
        if not (mset_exe.exists() and mses_exe.exists() and mplot_exe.exists()):
            result.status = "error"
            result.error = f"MSES executables not found under {mses_dir}"
            return result

        repaneled = airfoil.repanel(n_points_per_side=80)
        attempt_errors: List[str] = []
        tmp = None
        raw = None
        converged_alpha = None
        try:
            for offset in alpha_retry_offsets_deg:
                candidate_alpha = float(alpha_deg) + offset
                tmp = tempfile.mkdtemp(prefix="alas_mses_cp_")
                try:
                    ms = MSES(
                        airfoil=repaneled,
                        n_crit=mses_config.n_crit,
                        xtr_upper=mses_config.xtr_upper,
                        xtr_lower=mses_config.xtr_lower,
                        max_iter=mses_config.max_iterations,
                        mset_command=str(mset_exe),
                        mses_command=str(mses_exe),
                        mplot_command=str(mplot_exe),
                        verbosity=0,
                        timeout_mset=mses_config.timeout_mset_s,
                        timeout_mses=mses_config.timeout_mses_s,
                        mset_n=mses_config.mset_n,
                        mset_e=mses_config.mset_e,
                        working_directory=tmp,  # persists after .run() -- needed for the mplot dump below
                    )
                    candidate_raw = ms.run(
                        alpha=candidate_alpha, Re=reynolds, mach=mach
                    )
                except Exception as exc:
                    attempt_errors.append(f"alpha={candidate_alpha:.2f}: {exc}")
                    shutil.rmtree(tmp, ignore_errors=True)
                    tmp = None
                    continue

                if len(np.ravel(candidate_raw.get("alpha", []))) == 0:
                    attempt_errors.append(
                        f"alpha={candidate_alpha:.2f}: did not converge"
                    )
                    shutil.rmtree(tmp, ignore_errors=True)
                    tmp = None
                    continue

                raw = candidate_raw
                converged_alpha = candidate_alpha
                break

            if raw is None:
                result.status = "error"
                tried = ", ".join(
                    f"{alpha_deg + o:.2f}" for o in alpha_retry_offsets_deg
                )
                result.error = (
                    f"MSES did not converge at alpha={alpha_deg:.2f} deg or any retry "
                    f"offset (tried: {tried} deg). Last error: "
                    f"{attempt_errors[-1] if attempt_errors else 'unknown'}"
                )
                return result

            result.alpha_deg = float(converged_alpha)
            dump_name = "bl_dump.txt"
            subprocess.run(
                f'"{mplot_exe}" case',
                input=f"12\n{dump_name}\n0\n0\n",
                cwd=tmp,
                capture_output=True,
                text=True,
                shell=True,
                timeout=mses_config.timeout_mses_s,
                **no_window_kwargs(),
            )
            dump_path = Path(tmp) / dump_name
            if not dump_path.exists():
                result.status = "error"
                result.error = "mplot did not produce a BL dump file"
                return result

            # Also dump the 2D flowfield (option 11) for Mach contours
            flowfield_name = "flowfield.txt"
            subprocess.run(
                f'"{mplot_exe}" case',
                input=f"11\n{flowfield_name}\n0\n0\n",
                cwd=tmp,
                capture_output=True,
                text=True,
                shell=True,
                timeout=mses_config.timeout_mses_s,
                **no_window_kwargs(),
            )
            flowfield_path = Path(tmp) / flowfield_name

            xs, ys, cps, mes = [], [], [], []
            with open(dump_path, "r", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split()
                    if len(parts) < 8:
                        continue
                    try:
                        x, y, _s, _b0, cp, _ue, _rho, me = (float(p) for p in parts[:8])
                    except ValueError:
                        continue
                    xs.append(x)
                    ys.append(y)
                    cps.append(cp)
                    mes.append(me)

            if not xs:
                result.status = "error"
                result.error = "BL dump file was empty or unparseable"
                return result

            upper, lower = [], []
            for x, y, cp, me in zip(xs, ys, cps, mes):
                if x < -0.01 or x > 1.02:
                    continue  # exclude wake points trailing downstream of the TE
                (upper if y >= 0 else lower).append((x, cp, me))

            # MSES's BL dump walks the surface in panel order (arc length),
            # not monotonic x -- connecting raw dump order with a line plot
            # draws spurious cross-panel diagonals wherever x briefly
            # reverses (e.g. near a rounded leading edge). Sorting each
            # surface by x/c gives the standard, artifact-free Cp(x)/Mach(x)
            # curve.
            upper.sort(key=lambda t: t[0])
            lower.sort(key=lambda t: t[0])
            if upper:
                result.x_upper, result.cp_upper, result.mach_upper = (
                    list(v) for v in zip(*upper)
                )
            if lower:
                result.x_lower, result.cp_lower, result.mach_lower = (
                    list(v) for v in zip(*lower)
                )

            if flowfield_path.exists():
                fx, fy, fm = [], [], []
                with open(flowfield_path, "r", errors="replace") as f:
                    for line in f:
                        line = line.strip()
                        if not line or line.startswith("#"):
                            continue
                        parts = line.split()
                        if len(parts) >= 8:
                            try:
                                fx.append(float(parts[0]))
                                fy.append(float(parts[1]))
                                fm.append(float(parts[7]))
                            except ValueError:
                                pass
                result.field_x = fx
                result.field_y = fy
                result.field_mach = fm

            # The exact panelled geometry MSES actually solved (post-repanel,
            # post-morph) -- NOT necessarily the same shape as any airfoil
            # preset name a caller might have on hand, so the contour plot's
            # outline always matches the flowfield it's drawn over.
            result.airfoil_x = list(repaneled.coordinates[:, 0])
            result.airfoil_y = list(repaneled.coordinates[:, 1])

            result.status = "ok"
            return result
        finally:
            # Clean up whichever attempt's directory ultimately "won" (failed
            # attempts already cleaned themselves up in the retry loop above).
            if tmp is not None:
                shutil.rmtree(tmp, ignore_errors=True)
    except KeyError as exc:
        # AeroSandbox's own MSES.run() unconditionally does
        # runs_output.pop("Ma") on its accumulated-results dict; if the
        # single alpha requested here didn't converge, that dict is empty
        # and the pop raises a bare KeyError('Ma') -- a real (if minor)
        # upstream library gap for the single-alpha case (a multi-alpha
        # sweep, as in run_mses_polar, doesn't hit this as long as at least
        # one point converges). Reported here with a clear, honest message
        # instead of the cryptic raw "'Ma'" string.
        result.status = "error"
        if str(exc).strip("'\"") == "Ma":
            result.error = f"MSES did not converge at alpha={alpha_deg:.2f} deg"
        else:
            result.error = f"KeyError: {exc}"
        return result
    except Exception as exc:
        result.status = "error"
        result.error = str(exc)
        return result
