# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
ALAS command-line interface.

A thin front-end over :class:`DesignPipeline`. It loads the user's requirements
from a YAML config (or uses the built-in defaults), runs the pipeline, and
writes artifacts. The compute core stays UI-agnostic -- the desktop GUI
(``desktop/``, a Go/Wails shell + React frontend) talks to it through the
FastAPI sidecar in ``alas/sidecar/`` instead of this module.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from .config.settings import ALASConfig
from .pipeline import DesignPipeline


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="alas",
        description="Optimize and analyze an aircraft design from a set of requirements.",
    )
    p.add_argument(
        "-c",
        "--config",
        type=Path,
        default=None,
        help="Path to a YAML requirements/config file. Omit to use defaults.",
    )
    p.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("outputs"),
        help="Directory for exported artifacts (default: ./outputs).",
    )
    p.add_argument(
        "--no-optimize",
        action="store_true",
        help="Skip optimization and analyze the nominal design.",
    )
    p.add_argument(
        "--no-baseline",
        action="store_true",
        help="Do not analyze the baseline design for comparison.",
    )
    p.add_argument(
        "--no-mission",
        action="store_true",
        help="Skip SUAVE mission analysis even if config.mission.enabled is True.",
    )
    p.add_argument(
        "--no-parallel",
        action="store_true",
        help="Run pipeline stages strictly sequentially instead of "
        "concurrently (parallel is the default; use this for "
        "deterministic single-threaded debugging).",
    )
    p.add_argument(
        "--plots",
        action="store_true",
        help="Generate and save figures (requires matplotlib).",
    )
    p.add_argument(
        "--show",
        action="store_true",
        help="Display figures interactively (implies --plots).",
    )
    p.add_argument(
        "--seed",
        type=int,
        default=None,
        help="Override the optimizer random seed for reproducibility.",
    )
    p.add_argument("--quiet", action="store_true", help="Reduce console output.")
    p.add_argument(
        "--save-config",
        type=Path,
        default=None,
        help="Write the effective configuration to this YAML path and exit.",
    )
    return p


def load_config(args: argparse.Namespace) -> ALASConfig:
    config = ALASConfig.from_yaml(args.config) if args.config else ALASConfig()
    if args.seed is not None:
        config.optimizer.solver.seed = args.seed
    if args.no_mission:
        config.mission.enabled = False
    return config


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    config = load_config(args)

    if args.save_config is not None:
        config.to_yaml(args.save_config)
        print(f"Effective configuration written to {args.save_config}")
        return 0

    pipeline = DesignPipeline(config)
    result = pipeline.run(
        optimize=not args.no_optimize,
        compare_baseline=not args.no_baseline,
        output_dir=args.output,
        make_plots=args.plots or args.show,
        show_plots=args.show,
        verbose=not args.quiet,
        parallel=not args.no_parallel,
    )
    _print_mission_summary(result, quiet=args.quiet)
    _print_mses_summary(result, quiet=args.quiet)
    _print_structural_summary(result, quiet=args.quiet)
    return 0


def _print_mses_summary(result, quiet: bool) -> None:
    """Surface Stage 6's MSES 2-D polar/pressure-distribution output on the
    console -- the CLI equivalent of ``_print_mission_summary`` above.

    Without this, a headless run with ``config.mses.enabled`` (the GUI's
    default) never printed anything about MSES at all: ``pipeline.run()``'s
    internal ``report(...)`` progress messages (including the MSES skip
    reason on failure) only reach the CLI if a ``progress_callback`` is
    passed, and ``main()`` never passes one -- so a failed/non-converged
    MSES solve would go silent on the terminal, not just omitted from
    a plot, exactly like the mission-analysis gap this mirrors.
    """
    mses = result.mses_result
    pressure = result.mses_pressure
    if mses is None:
        return
    if mses.status != "ok":
        if not quiet:
            print(f"\n--- MSES analysis: {mses.status} ---")
            if mses.error:
                print(f"  {mses.error}")
        return

    print("\n--- MSES 2-D polar analysis ---")
    print(f"  airfoil          : {mses.airfoil_name}")
    print(f"  Mach / Re        : {mses.mach:.3f} / {mses.reynolds:.3e}")
    print(f"  converged points : {len(mses.alpha_deg)}")
    if pressure is not None and pressure.status != "ok":
        print(f"  pressure distribution: {pressure.status} ({pressure.error})")


def _print_structural_summary(result, quiet: bool) -> None:
    """Surface the wingbox structural analysis stage's output on the
    console -- CLI equivalent of ``_print_mission_summary``/
    ``_print_mses_summary`` above. Without this, a headless run with
    ``config.structures.enabled`` (the default) never printed anything
    about the sizing/analytical results or a real NASTRAN solve's
    success/failure.
    """
    sr = result.structural_result
    if sr is None:
        return
    if sr.status != "ok":
        if not quiet:
            print(f"\n--- Structural analysis: {sr.status} ---")
            if sr.error:
                print(f"  {sr.error}")
        return

    sizing = sr.sizing
    print("\n--- Wingbox structural analysis ---")
    print(
        f"  spars            : {len(sizing.spars)} @ x/c={[f'{f:.2f}' for f in sizing.spar_fracs]}"
    )
    print(
        f"  ribs             : {sizing.num_ribs} (spacing {sizing.rib_spacing_m:.2f} m)"
    )
    print(f"  sizing load case : {sizing.sizing_load_case}")
    print(
        f"  semi-wing mass   : {sizing.total_mass_kg:,.0f} kg  "
        f"(both wings: {2 * sizing.total_mass_kg:,.0f} kg, Torenbeek estimate: "
        f"{sr.torenbeek_wing_mass_kg:,.0f} kg)"
    )
    for name, lc in sr.analysis.load_cases.items():
        print(f"  [{name:10s}] tip deflection = {lc.tip_deflection_m:+.2f} m")
    if sr.mesh_health is not None:
        status = "OK" if sr.mesh_health.ok else "WARNINGS"
        print(
            f"  FEM mesh         : {sr.mesh_health.n_nodes} nodes, {sr.mesh_health.n_elements} elements [{status}]"
        )
    if sr.nastran is not None:
        for label, sub in (
            ("static", sr.nastran.static),
            ("modes", sr.nastran.modes),
            ("vibration", sr.nastran.vibration),
        ):
            if sub.status != "not_run":
                print(
                    f"  NASTRAN {label:10s}: {sub.status}"
                    + (f" ({sub.error})" if sub.error else "")
                )
    if sr.patran is not None:
        print(
            f"  Patran export    : {sr.patran.status}"
            + (f" ({sr.patran.error})" if sr.patran.error else "")
        )
        for name, png in sr.patran.png_paths.items():
            print(f"    {name:10s}: {png}")


def _print_mission_summary(result, quiet: bool) -> None:
    """Surface Stage 5's SUAVE mission/route output on the console.

    Mission analysis runs automatically as part of ``pipeline.run()`` (and
    ``--no-mission`` exists specifically to skip it), but until now the CLI
    never printed or exported anything about it -- only the GUI's Results
    view showed mission data. A multi-minute SUAVE subprocess would run with
    its output going nowhere for a CLI user.
    """
    mission = result.mission_result
    if mission is None:
        return
    if mission.status != "ok":
        if not quiet:
            print(f"\n--- Mission analysis: {mission.status} ---")
            if mission.error:
                print(f"  {mission.error}")
        return

    print("\n--- SUAVE mission analysis ---")
    if result.route is not None:
        print(f"  route source     : {result.route.source}")
        print(f"  route distance   : {result.route.total_distance_m / 1000:.0f} km")
    summary = mission.summary or {}
    for key, value in summary.items():
        print(f"  {key:16s} : {value}")
    if mission.csv_path is not None:
        print(f"  [file] mission flight data written: {mission.csv_path}")


if __name__ == "__main__":
    raise SystemExit(main())
