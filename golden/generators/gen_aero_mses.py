# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::mses``: ``alas/physics/mses_analysis.py`` -- an MSES 2-D polar
sweep and a surface-pressure solve, both driven through AeroSandbox's own
``aerodynamics.aero_2D.MSES`` wrapper, which pipes menu keystrokes to Mark
Drela's real ``mset``/``mses``/``mplot`` binaries.

This is an external-solver fixture, unlike every other row so far: the numbers
come out of a compiled tool, not out of Python arithmetic. That is workable at
``exact`` because MSES is deterministic -- an identical mesh and identical
``mses.case`` deck produce byte-identical output, verified here by running each
case twice and refusing to write a fixture whose two runs disagreed. The Rust
port drives the same binaries with the same decks, so it reproduces the same
output; the parity test's ``exact`` comparison is of the orchestration and the
parsing, not of a re-derived float.

What the fixture records, per case:

* ``repaneled_coordinates`` -- the section AFTER ``airfoil.repanel(80)``, which
  is what actually reaches ``mset``. The mses row's ``exact`` claim is about
  the orchestration, so the fixture feeds the post-repanel coordinates the Rust
  side reads back: ``repanel`` is a cubic-spline resample and belongs to the
  ``alas-geom::asb::airfoil`` row (green at ``linalg``), and holding an
  external-solver deck to bit-reproducing a spline through a different
  platform's ``pow`` would be asserting the wrong thing here.
* ``airfoil_dat`` and ``mses_case`` -- the exact deck bytes AeroSandbox wrote,
  captured from a persistent-working-directory run. These let the parity test
  check the generated deck at ``exact`` WITHOUT the binaries present, which is
  the only check the row can make on a machine that does not have MSES.
* ``result`` -- the authoritative ``MSESPolarResult`` / ``MSESPressureResult``
  from the actual ``run_mses_polar`` / ``run_mses_pressure_distribution``.
* ``binaries`` -- an FNV-1a digest and size of each executable. The ``exact``
  numeric comparison only holds against the same MSES build, so the parity test
  compares these first and reports a mismatch as "different MSES build, numeric
  check skipped" rather than as forty wrong numbers.

Cases: two polar sweeps (a cambered ``naca2412`` and a symmetric ``naca0012``,
the latter reaching near-zero lift and moment and the sign handling around
them) and one pressure solve (``naca2412``, whose BL dump splits into upper and
lower surfaces and whose flowfield carries the Mach contour points). The
reinitialize-on-non-convergence branch and the no-convergence error message are
NOT fixture cases -- a deterministically non-converging point is exactly the
fragile edge an ``exact`` fixture must avoid -- and are covered by Rust unit
tests on synthetic solver output instead.
"""

from __future__ import annotations

import os
import shutil
import tempfile
from pathlib import Path

import numpy as np

import _framework

_framework.add_alas_to_path()

import aerosandbox as asb  # noqa: E402
from aerosandbox.aerodynamics.aero_2D import MSES  # noqa: E402

from alas.config.mses_config import MSESConfig  # noqa: E402
from alas.physics import mses_analysis  # noqa: E402

_FNV_OFFSET_BASIS_64 = 0xCBF29CE484222325
_FNV_PRIME_64 = 0x00000100000001B3
_MASK_64 = (1 << 64) - 1

# The repanel density both entry points hardcode at their MSES call sites.
_N_POINTS_PER_SIDE = 80


def _fnv1a64(data: bytes) -> str:
    """FNV-1a, 64-bit, as ``gen_geom_selig.py``/``gen_aero_neuralfoil.py`` do."""
    digest = _FNV_OFFSET_BASIS_64
    for byte in data:
        digest ^= byte
        digest = (digest * _FNV_PRIME_64) & _MASK_64
    return f"{digest:016x}"


def _locate_mses_dir() -> Path:
    """The folder holding mset/mses/mplot, from the environment or known spots."""
    candidates = []
    override = os.environ.get("ALAS_MSES_DIR")
    if override:
        candidates.append(Path(override))
    candidates.append(
        _framework.ALAS_ROOT / "desktop" / "build" / "bin" / "external tools" / "MSES"
    )
    for candidate in candidates:
        if (candidate / "mset.exe").exists() and (candidate / "mses.exe").exists():
            return candidate
    raise SystemExit(
        "MSES binaries not found. Set ALAS_MSES_DIR to the folder holding "
        "mset.exe/mses.exe/mplot.exe (MSES is licensed separately by MIT and "
        "is not bundled with either repository)."
    )


def _config(mses_dir: Path, *, halfwidth: float, n_points: int) -> MSESConfig:
    cfg = MSESConfig()
    cfg.mses_dir = str(mses_dir)  # absolute -> resolve_tool_dir honours it verbatim
    cfg.alpha_sweep_halfwidth_deg = halfwidth
    cfg.alpha_sweep_n_points = n_points
    return cfg


def _config_dict(cfg: MSESConfig) -> dict:
    """The fields the Rust `MsesConfig` needs to reproduce the run."""
    return {
        "n_crit": float(cfg.n_crit),
        "xtr_upper": float(cfg.xtr_upper),
        "xtr_lower": float(cfg.xtr_lower),
        "max_iterations": int(cfg.max_iterations),
        "mset_n": int(cfg.mset_n),
        "mset_e": float(cfg.mset_e),
        "timeout_mset_s": float(cfg.timeout_mset_s),
        "timeout_mses_s": float(cfg.timeout_mses_s),
        "alpha_sweep_halfwidth_deg": float(cfg.alpha_sweep_halfwidth_deg),
        "alpha_sweep_n_points": int(cfg.alpha_sweep_n_points),
    }


def _capture_decks(repaneled, cfg: MSESConfig, mses_dir: Path, alphas, mach, reynolds):
    """Run AeroSandbox's MSES in a persistent dir, return the deck bytes it wrote."""
    workdir = Path(tempfile.mkdtemp(prefix="alas_mses_gen_"))
    try:
        ms = MSES(
            airfoil=repaneled,
            n_crit=cfg.n_crit,
            xtr_upper=cfg.xtr_upper,
            xtr_lower=cfg.xtr_lower,
            max_iter=cfg.max_iterations,
            mset_command=str(mses_dir / "mset.exe"),
            mses_command=str(mses_dir / "mses.exe"),
            mplot_command=str(mses_dir / "mplot.exe"),
            verbosity=0,
            timeout_mset=cfg.timeout_mset_s,
            timeout_mses=cfg.timeout_mses_s,
            mset_n=cfg.mset_n,
            mset_e=cfg.mset_e,
            working_directory=str(workdir),
        )
        ms.run(alpha=alphas, Re=reynolds, mach=mach)
        airfoil_dat = (workdir / "airfoil.dat").read_text(encoding="ascii")
        mses_case = (workdir / "mses.case").read_text(encoding="ascii")
        return airfoil_dat, mses_case
    finally:
        shutil.rmtree(workdir, ignore_errors=True)


def _polar_result_dict(result) -> dict:
    return {
        "status": result.status,
        "error": result.error,
        "airfoil_name": result.airfoil_name,
        "mach": float(result.mach),
        "reynolds": float(result.reynolds),
        "alpha_deg": [float(v) for v in result.alpha_deg],
        "CL": [float(v) for v in result.CL],
        "CD": [float(v) for v in result.CD],
        "CM": [float(v) for v in result.CM],
        "CDv": [float(v) for v in result.CDv],
        "CDw": [float(v) for v in result.CDw],
        "xtr_top": [float(v) for v in result.xtr_top],
        "xtr_bot": [float(v) for v in result.xtr_bot],
    }


def _pressure_result_dict(result) -> dict:
    return {
        "status": result.status,
        "error": result.error,
        "alpha_deg": float(result.alpha_deg),
        "x_upper": [float(v) for v in result.x_upper],
        "cp_upper": [float(v) for v in result.cp_upper],
        "mach_upper": [float(v) for v in result.mach_upper],
        "x_lower": [float(v) for v in result.x_lower],
        "cp_lower": [float(v) for v in result.cp_lower],
        "mach_lower": [float(v) for v in result.mach_lower],
        "field_x": [float(v) for v in result.field_x],
        "field_y": [float(v) for v in result.field_y],
        "field_mach": [float(v) for v in result.field_mach],
        "airfoil_x": [float(v) for v in result.airfoil_x],
        "airfoil_y": [float(v) for v in result.airfoil_y],
    }


def _polar_case(name, airfoil_name, mach, reynolds, trim_alpha, cfg, mses_dir) -> dict:
    airfoil = asb.Airfoil(airfoil_name)
    repaneled = airfoil.repanel(n_points_per_side=_N_POINTS_PER_SIDE)
    half = max(0.5, cfg.alpha_sweep_halfwidth_deg)
    n = max(3, cfg.alpha_sweep_n_points)
    alphas = np.linspace(trim_alpha - half, trim_alpha + half, n)

    def run():
        return mses_analysis.run_mses_polar(
            airfoil=airfoil,
            mach=mach,
            reynolds=reynolds,
            trim_alpha_deg=trim_alpha,
            mses_config=cfg,
            repo_root=_framework.ALAS_ROOT,
        )

    first = _polar_result_dict(run())
    second = _polar_result_dict(run())
    if first["status"] != "ok":
        raise SystemExit(f"polar case {name} did not converge: {first['error']}")
    if first != second:
        raise SystemExit(f"polar case {name} was not deterministic across two runs")

    airfoil_dat, mses_case = _capture_decks(
        repaneled, cfg, mses_dir, alphas, mach, reynolds
    )
    return {
        "name": name,
        "airfoil_name": airfoil_name,
        "mach": float(mach),
        "reynolds": float(reynolds),
        "trim_alpha_deg": float(trim_alpha),
        "config": _config_dict(cfg),
        "alphas_requested": [float(v) for v in alphas],
        "repaneled_coordinates": [[float(x), float(y)] for x, y in repaneled.coordinates],
        "airfoil_dat": airfoil_dat,
        "mses_case": mses_case,
        "mses_case_alpha": float(alphas[-1]),
        "result": first,
    }


def _pressure_case(name, airfoil_name, mach, reynolds, alpha, cfg, mses_dir) -> dict:
    airfoil = asb.Airfoil(airfoil_name)
    repaneled = airfoil.repanel(n_points_per_side=_N_POINTS_PER_SIDE)

    def run():
        return mses_analysis.run_mses_pressure_distribution(
            airfoil=airfoil,
            mach=mach,
            reynolds=reynolds,
            alpha_deg=alpha,
            mses_config=cfg,
            repo_root=_framework.ALAS_ROOT,
        )

    first = _pressure_result_dict(run())
    second = _pressure_result_dict(run())
    if first["status"] != "ok":
        raise SystemExit(f"pressure case {name} did not converge: {first['error']}")
    if first != second:
        raise SystemExit(f"pressure case {name} was not deterministic across two runs")
    if first["alpha_deg"] != alpha:
        raise SystemExit(
            f"pressure case {name} converged at a retry offset, not the target "
            "alpha; pick a condition where the exact alpha converges"
        )

    airfoil_dat, mses_case = _capture_decks(
        repaneled, cfg, mses_dir, alpha, mach, reynolds
    )
    return {
        "name": name,
        "airfoil_name": airfoil_name,
        "mach": float(mach),
        "reynolds": float(reynolds),
        "alpha_deg": float(alpha),
        "config": _config_dict(cfg),
        "repaneled_coordinates": [[float(x), float(y)] for x, y in repaneled.coordinates],
        "airfoil_dat": airfoil_dat,
        "mses_case": mses_case,
        "result": first,
    }


def main() -> None:
    mses_dir = _locate_mses_dir()
    binaries = {}
    for exe in ("mset.exe", "mses.exe", "mplot.exe"):
        blob = (mses_dir / exe).read_bytes()
        binaries[exe] = {"size": len(blob), "fnv1a64": _fnv1a64(blob)}

    cfg = _config(mses_dir, halfwidth=1.0, n_points=3)

    polar_cases = [
        _polar_case("naca2412_m03", "naca2412", 0.3, 5.0e6, 2.0, cfg, mses_dir),
        _polar_case("naca0012_m04", "naca0012", 0.4, 3.0e6, 0.0, cfg, mses_dir),
    ]
    pressure_cases = [
        _pressure_case("naca2412_m03_a2", "naca2412", 0.3, 5.0e6, 2.0, cfg, mses_dir),
    ]

    if not any(any(c > 0.0 for c in case["result"]["CL"]) for case in polar_cases):
        raise SystemExit("no polar case produced a positive lift coefficient")
    if not pressure_cases[0]["result"]["x_upper"]:
        raise SystemExit("pressure case produced no upper-surface points")

    _framework.write(
        "aero",
        "mses",
        {
            "binaries": binaries,
            "polar": polar_cases,
            "pressure": pressure_cases,
        },
        description=(
            "alas.physics.mses_analysis: run_mses_polar (naca2412 and symmetric "
            "naca0012) and run_mses_pressure_distribution (naca2412), driven "
            "through AeroSandbox's MSES wrapper onto the real mset/mses/mplot "
            "binaries; records post-repanel coordinates, the exact deck bytes, "
            "the parsed results and per-binary FNV-1a digests"
        ),
    )


if __name__ == "__main__":
    main()
