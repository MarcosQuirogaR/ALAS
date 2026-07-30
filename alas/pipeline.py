# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
ALAS pipeline orchestrator.

Implements the canonical workflow:

    user requirements  ->  optimize design  ->  full analysis  ->  export

This is the single entry point every front-end (CLI, GUI) calls. It is
UI-agnostic: it takes a fully-populated :class:`ALASConfig`, runs the
stages, and returns a structured :class:`PipelineResult`. Side effects (files,
plots) are opt-in via arguments, and an optional ``progress_callback`` lets a
front-end stream status without the pipeline knowing what a front-end is.
"""

from __future__ import annotations

import concurrent.futures
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Callable, Dict, List, Optional, Tuple

import aerosandbox as asb

from .analysis.full_analysis import AnalysisReport, FullAnalysis
from .config.design_variables import DesignVector
from .config.settings import ALASConfig
from .optimization.optimizer import DesignOptimizer, OptimizationResult
from .reporting import design_report

if TYPE_CHECKING:
    from .integration.suave_bridge import MissionResult
    from .routing.route import Route
    from .physics.mses_analysis import MSESPolarResult, MSESPressureResult
    from .geometry.wing_structure import WingStructureGeometry
    from .geometry.wing_mesh_bdf import MeshHealthReport
    from .physics.structural_sizing import WingboxSizing
    from .physics.structural_analysis import StructuralAnalysisReport
    from .integration.nastran_runner import NastranResults
    from .integration.patran_runner import PatranExportResult

ProgressCallback = Callable[[str], None]


@dataclass
class StructuralAnalysisResult:
    """Wingbox structural analysis for the chosen design -- downstream/
    informational only (see :mod:`alas.physics.structural_sizing`'s
    module docstring): never feeds back into the mass model, CG, or
    optimizer. ``status``/``error`` follow the same non-fatal contract as
    :class:`~alas.integration.suave_bridge.MissionResult` and
    :class:`~alas.physics.mses_analysis.MSESPolarResult`.
    """

    status: str = (
        "not_run"  # "ok" | "error" | "not_run" (config.structures.enabled is False)
    )
    error: Optional[str] = None
    wsg: Optional["WingStructureGeometry"] = None
    sizing: Optional["WingboxSizing"] = None
    mesh_health: Optional["MeshHealthReport"] = None
    analysis: Optional["StructuralAnalysisReport"] = None
    nastran: Optional["NastranResults"] = None
    patran: Optional["PatranExportResult"] = None
    # Torenbeek's own predicted full-wing mass (physics.mass's component_masses["Wing"])
    # for this same design, for the FEM-vs-Torenbeek accuracy comparison figure --
    # read-only display, never used to correct or override the mass model itself.
    torenbeek_wing_mass_kg: float = float("nan")


@dataclass
class BaselineReport:
    """Up-front weight & balance + stability of the *initial* (baseline) design.

    Computed before optimization so the user can sanity-check that a preset
    (e.g. the A320) sits at a sensible CG and static margin -- i.e. that the mass
    model and payload distribution are correct. Deliberately cheap: it uses the
    detailed payload layout and a two-point static-margin estimate, with no full
    alpha sweep. Field names mirror :class:`AnalysisReport` where the shared
    visualization factories (mass breakdown, CG envelope, mass distribution)
    read them, so those figures work unchanged.

    ``status``/``error`` mirror :class:`~alas.integration.suave_bridge.
    MissionResult`'s pattern: a failure (bad preset, degenerate geometry, a
    VLM/solver exception) is reported as ``status="error"`` with ``error``
    explaining why, rather than the caller getting back a bare ``None`` with
    no way to know what went wrong.
    """

    design: DesignVector
    airplane: Optional[asb.Airplane] = None
    component_masses: Dict[str, float] = field(default_factory=dict)
    mass_coordinates: Dict[str, List[float]] = field(default_factory=dict)
    physical_cg: List[float] = field(default_factory=list)
    static_margin: float = float("nan")
    mac: float = 0.0
    x_neutral_point: float = 0.0
    cg_pct_mac: float = 0.0
    np_pct_mac: float = 0.0
    payload_layout: object = None  # PayloadLayout
    status: str = "ok"  # "ok" | "error"
    error: Optional[str] = None


@dataclass
class PipelineResult:
    """Everything the pipeline produces in one run."""

    config: ALASConfig
    optimized_design: DesignVector
    optimized_report: Optional[AnalysisReport] = None
    optimization_result: Optional[OptimizationResult] = None
    baseline_report: Optional[AnalysisReport] = None
    baseline_analysis: Optional[BaselineReport] = None
    # SUAVE mission analysis for the chosen design over the selected route
    # (config.departure_airport/arrival_airport). Both stay None when
    # config.mission.enabled is False, no optimized_report exists yet (the
    # baseline-only path), or the SUAVE environment isn't configured --
    # mission_result.status then explains why (see integration.suave_bridge).
    route: Optional["Route"] = None
    mission_result: Optional["MissionResult"] = None
    # MSES 2-D polar + surface Cp/Mach distribution on the optimized design's
    # root airfoil section, for the Model Comparison and Aerodynamics tabs.
    # None when config.mses.enabled is False, no optimized_report exists yet
    # (baseline-only path), or MSES isn't configured -- status then explains
    # why (see physics.mses_analysis).
    mses_result: Optional["MSESPolarResult"] = None
    mses_pressure: Optional["MSESPressureResult"] = None
    # Wingbox structural analysis (sizing + analytical deformation/stress/
    # frequency estimate + optional real NASTRAN solve) for the optimized
    # design. None when config.structures.enabled is False or no
    # optimized_report exists yet (the baseline-only path).
    structural_result: Optional["StructuralAnalysisResult"] = None


class DesignPipeline:
    """Orchestrates the optimize -> analyze -> report workflow."""

    def __init__(self, config: Optional[ALASConfig] = None):
        self.config = config or ALASConfig()

    def run(
        self,
        optimize: bool = True,
        compare_baseline: bool = True,
        output_dir: Optional[str | Path] = "outputs",
        make_plots: bool = False,
        show_plots: bool = False,
        verbose: bool = True,
        bounds: Optional[List[Tuple[float, float]]] = None,
        initial_design: Optional[DesignVector] = None,
        progress_callback: Optional[ProgressCallback] = None,
        parallel: bool = True,
    ) -> PipelineResult:
        """Run the pipeline. See the module docstring for the stage order.

        ``parallel`` (default ``True``) runs independent post-optimization
        stages concurrently on background threads instead of one after
        another, so a run's total wall-clock time is closer to the slowest
        stage rather than the sum of all of them:

        * Stage 2's two full analyses (the optimized design and, if
          ``compare_baseline``, the nominal design for comparison) are
          independent VLM sweeps and run on two threads.
        * Once Stage 2 finishes, export (Stage 3, disk I/O), the SUAVE
          mission analysis (Stage 5, a subprocess that can take minutes) run
          on background threads while plotting (Stage 4, which stays on the
          calling thread since matplotlib figure creation should not be
          spread across threads) proceeds concurrently.

        This is purely a scheduling change -- every stage's inputs/outputs
        are unchanged, and each stage's exceptions still propagate to the
        caller exactly as they would sequentially. Set ``parallel=False`` to
        force the old strictly-sequential behaviour (e.g. for deterministic
        single-threaded debugging).
        """
        report = progress_callback or (lambda _msg: None)
        if initial_design is not None:
            nominal = initial_design
        elif self.config.preset:
            from .config.presets import get_preset

            nominal = get_preset(self.config.preset).design_vector
        else:
            nominal = DesignVector.default()

        # --- Stage 0: baseline W&B + stability (always, before optimizing) ---
        # Lets the user validate that the initial/preset design has a sensible
        # CG and static margin before any optimization runs.
        report("Analysing baseline weight & balance and stability...")
        baseline_analysis = self._baseline_analysis(nominal)

        # --- Stage 1: find the design ---------------------------------------
        opt_result: Optional[OptimizationResult] = None
        if optimize:
            report("Optimizing design space...")
            opt_result = DesignOptimizer(self.config).run(
                verbose=verbose,
                bounds=bounds,
                progress_callback=progress_callback,
                initial_design=nominal,
            )
            design = opt_result.best_design
            report(
                f"Optimization complete ({opt_result.history.n_valid} valid evaluations)."
            )
        else:
            if verbose:
                print("--- Skipping optimization: analysing the initial design ---")
            design = nominal

        # --- Stage 2: full analysis of the chosen design (+ baseline compare) ---
        # The optimized design's analysis and the nominal-design comparison
        # analysis are independent VLM sweeps over different designs -- when
        # parallel, run them concurrently instead of one after another.
        analyzer = FullAnalysis(self.config)
        need_baseline_cmp = compare_baseline and optimize
        report("Running full AeroSandbox analysis of the selected design...")
        if parallel and need_baseline_cmp:
            if verbose:
                print("\n--- Analysing initial design for comparison (in parallel) ---")
            with concurrent.futures.ThreadPoolExecutor(
                max_workers=2, thread_name_prefix="alas-analysis"
            ) as ex:
                fut_optimized = ex.submit(
                    analyzer.run, design, include_engines=True, verbose=verbose
                )
                fut_baseline = ex.submit(
                    analyzer.run, nominal, include_engines=True, verbose=verbose
                )
                optimized_report = fut_optimized.result()
                baseline_report = fut_baseline.result()
        else:
            optimized_report = analyzer.run(
                design, include_engines=True, verbose=verbose
            )
            baseline_report = None
            if need_baseline_cmp:
                report("Analysing initial design for comparison...")
                if verbose:
                    print("\n--- Analysing initial design for comparison ---")
                baseline_report = analyzer.run(
                    nominal, include_engines=True, verbose=verbose
                )

        if verbose:
            design_report.print_summary(optimized_report, self.config)

        # --- Stages 3/4/5/6: export, plot, SUAVE mission analysis, MSES ------
        # These four are mutually independent once optimized_report/
        # baseline_report exist: export is disk I/O, mission analysis is a
        # subprocess call that can take minutes, MSES is a ~10-20s subprocess
        # sweep, and plotting only touches matplotlib. When parallel, export/
        # mission/MSES run on background threads while plotting proceeds on
        # this (the calling) thread -- matplotlib figure creation is kept off
        # worker threads throughout ALAS, so plotting is never itself
        # backgrounded.
        route = None
        mission_result = None
        mses_result = None
        mses_pressure = None
        structural_result = None
        run_mission = self.config.mission.enabled and optimized_report is not None
        run_mses = self.config.mses.enabled and optimized_report is not None
        run_structures = self.config.structures.enabled and optimized_report is not None

        if parallel and (
            output_dir is not None or run_mission or run_mses or run_structures
        ):
            with concurrent.futures.ThreadPoolExecutor(
                max_workers=4, thread_name_prefix="alas-post"
            ) as ex:
                export_future = None
                mission_future = None
                mses_future = None
                structures_future = None
                if output_dir is not None:
                    report("Exporting design data...")
                    export_future = ex.submit(
                        self._export, optimized_report, output_dir
                    )
                if run_mission:
                    report("Running SUAVE mission analysis...")
                    mission_future = ex.submit(
                        self._run_mission_analysis, optimized_report
                    )
                if run_mses:
                    report("Running MSES 2-D polar analysis...")
                    mses_future = ex.submit(self._run_mses_analysis, optimized_report)
                if run_structures:
                    report("Running wingbox structural analysis...")
                    structures_future = ex.submit(
                        self._run_structural_analysis, optimized_report
                    )

                if make_plots:
                    self._plot(
                        opt_result,
                        optimized_report,
                        baseline_report,
                        output_dir,
                        show_plots,
                    )

                if export_future is not None:
                    export_future.result()
                if mission_future is not None:
                    route, mission_result = mission_future.result()
                if mses_future is not None:
                    mses_result, mses_pressure = mses_future.result()
                if structures_future is not None:
                    structural_result = structures_future.result()
        else:
            if output_dir is not None:
                report("Exporting design data...")
                self._export(optimized_report, output_dir)
            if make_plots:
                self._plot(
                    opt_result,
                    optimized_report,
                    baseline_report,
                    output_dir,
                    show_plots,
                )
            if run_mission:
                report("Running SUAVE mission analysis...")
                route, mission_result = self._run_mission_analysis(optimized_report)
            if run_mses:
                report("Running MSES 2-D polar analysis...")
                mses_result, mses_pressure = self._run_mses_analysis(optimized_report)
            if run_structures:
                report("Running wingbox structural analysis...")
                structural_result = self._run_structural_analysis(optimized_report)

        if mission_result is not None:
            if mission_result.status == "ok":
                report("Mission analysis complete.")
                # run_mission deletes its own scratch dir before returning
                # (see suave_bridge.MissionResult.csv_path) -- outputs/ is
                # the flight-data CSV's durable home when exporting at all.
                if output_dir is not None and mission_result.csv_text:
                    csv_out = Path(output_dir) / "flight_data.csv"
                    csv_out.parent.mkdir(parents=True, exist_ok=True)
                    csv_out.write_text(mission_result.csv_text, encoding="utf-8")
                    mission_result.csv_path = csv_out
            else:
                report(f"Mission analysis skipped: {mission_result.status}")
        if mses_result is not None:
            if mses_result.status == "ok":
                report("MSES analysis complete.")
            else:
                report(
                    f"MSES analysis skipped: {mses_result.status} ({mses_result.error})"
                )
        if structural_result is not None:
            if structural_result.status == "ok":
                report("Structural analysis complete.")
            else:
                report(
                    f"Structural analysis skipped: {structural_result.status} ({structural_result.error})"
                )

        report("Done.")
        return PipelineResult(
            config=self.config,
            optimized_design=design,
            optimized_report=optimized_report,
            route=route,
            mission_result=mission_result,
            mses_result=mses_result,
            mses_pressure=mses_pressure,
            structural_result=structural_result,
            optimization_result=opt_result,
            baseline_report=baseline_report,
            baseline_analysis=baseline_analysis,
        )

    # -- baseline (run-once W&B + stability of the initial design) -----------
    def analyze_baseline(
        self,
        initial_design: Optional[DesignVector] = None,
        progress_callback: Optional[ProgressCallback] = None,
    ) -> PipelineResult:
        """Run *only* the baseline W&B + stability pass (no optimizer, no sweep).

        Backs the GUI "Analyze baseline" button so a preset can be validated
        instantly. Returns a :class:`PipelineResult` carrying only
        ``baseline_analysis`` (``optimized_report`` is ``None``).
        """
        report = progress_callback or (lambda _msg: None)
        if initial_design is not None:
            nominal = initial_design
        elif self.config.preset:
            from .config.presets import get_preset

            nominal = get_preset(self.config.preset).design_vector
        else:
            nominal = DesignVector.default()

        report("Analysing baseline weight & balance and stability...")
        baseline_analysis = self._baseline_analysis(nominal)
        report("Done.")
        return PipelineResult(
            config=self.config,
            optimized_design=nominal,
            optimized_report=None,
            optimization_result=None,
            baseline_report=None,
            baseline_analysis=baseline_analysis,
        )

    def _baseline_analysis(self, design: DesignVector) -> BaselineReport:
        """Build the initial design, balance it, and compute W&B + static margin."""
        from .geometry.aircraft_builder import AircraftBuilder
        from .physics.mass import run_mass_analysis
        from .physics.payload import build_payload_layout, oew_and_cg
        from .physics.stability import neutral_point

        req = self.config.requirements
        try:
            builder = AircraftBuilder(self.config.geometry)
            plane = builder.build(design, include_engines=True)

            # Weight and balance FIRST: the physical CG anchors the stability reference.
            masses, coords, _cg = run_mass_analysis(
                plane, req, self.config.geometry, self.config.mass_model
            )
            payload_layout = None
            try:
                oew, x_oew = oew_and_cg(masses, coords)
                payload_layout = build_payload_layout(plane, self.config, oew, x_oew)
                masses, coords, _cg = run_mass_analysis(
                    plane,
                    req,
                    self.config.geometry,
                    self.config.mass_model,
                    payload_layout=payload_layout,
                )
            except Exception:
                payload_layout = None

            cg = _cg
            # Anchor aerodynamic moment reference to the actual physical CG
            plane.xyz_ref[0] = float(cg[0])

            # Neutral point (wing+tail VLM + tail efficiency + fuselage), about the CG.
            x_np, sm, _cl_alpha = neutral_point(plane, self.config.analysis)
            mac = float(plane.c_ref)
            wing = next(
                (w for w in plane.wings if w.name == "Main Wing"), plane.wings[0]
            )
            x_lemac = float(wing.aerodynamic_center()[0]) - 0.25 * mac

            def pct(x):
                return (x - x_lemac) / max(mac, 1e-6) * 100.0

            return BaselineReport(
                design=design,
                airplane=plane,
                component_masses=masses,
                mass_coordinates=coords,
                physical_cg=cg,
                static_margin=sm,
                mac=mac,
                x_neutral_point=x_np,
                cg_pct_mac=pct(cg[0]) if cg else 0.0,
                np_pct_mac=pct(x_np),
                payload_layout=payload_layout,
            )
        except Exception as exc:
            return BaselineReport(design=design, status="error", error=str(exc))

    def _run_mission_analysis(
        self, report: AnalysisReport
    ) -> Tuple[Optional["Route"], Optional["MissionResult"]]:
        """Build the route and run a SUAVE mission for the chosen design.

        Never raises: an unresolvable airport, missing SUAVE environment, or
        subprocess failure all come back as a ``MissionResult`` with a
        non-"ok" ``status`` (or, for an unresolvable airport, ``(None,
        None)``) rather than failing the whole pipeline run.
        """
        from .config.airports import get_airport
        from .integration import suave_bridge, suave_mission, suave_vehicle
        from .routing.route import Route

        cfg = self.config
        try:
            origin = get_airport(cfg.departure_airport)
            dest = get_airport(cfg.arrival_airport)
        except KeyError:
            return None, None

        from .paths import resolve_data_path, resolve_tool_dir

        mcfg = cfg.mission
        # The navdata is downloaded rather than shipped, so it cannot be
        # resolved against the bundle: frozen, that is a per-build extraction
        # cache which a new release replaces. resolve_data_path searches the
        # install dir, the per-user data dir the downloader writes to, the
        # bundle and the repo root, and honours an absolute override.
        routes_dir = resolve_data_path(mcfg.routes_dir)
        navdata_dir = resolve_data_path(mcfg.navdata_dir)
        route = Route.for_airports(
            origin,
            dest,
            routes_dir=routes_dir,
            navdata_dir=navdata_dir,
            great_circle_points=mcfg.great_circle_points,
            simbrief_username=mcfg.simbrief_username,
            simbrief_timeout_s=mcfg.simbrief_timeout_s,
            simbrief_overrides_airports=mcfg.simbrief_overrides_airports,
        )
        # A SimBrief OFP may have overridden the configured pair (see
        # Route.for_airports). Size the mission against the airports actually
        # flown -- otherwise SUAVE would be handed London->Dubai field data for
        # what is really a Madrid->Coruna route, and every downstream distance,
        # fuel and field-performance number would describe a flight that never
        # happened.
        if route.origin_airport is not None:
            origin = route.origin_airport
        if route.dest_airport is not None:
            dest = route.dest_airport
        if route.origin_airport is not None or route.dest_airport is not None:
            import logging as _logging

            # Logged (not progress_callback'd): that callback is a local of
            # run(), not reachable from this method. The sidecar drains this
            # logger to the desktop app's run log anyway -- same path
            # routing.simbrief_route already reports through.
            _logging.getLogger("alas.routing").info(
                "Mission sized against SimBrief OFP endpoints %s->%s.",
                origin.icao,
                dest.icao,
            )

        vehicle_request = suave_vehicle.build_vehicle_request(report, cfg)
        mission_request = suave_mission.build_mission_request(
            cfg, origin, dest, route.total_distance_m
        )
        # The SUAVE venv + runner are externally-provisioned tools. Precedence:
        # (1) something already on disk at the configured/next-to-exe location
        # -- a user's own provisioned copy always wins (find_tool_dir); (2) a
        # bundled-and-extracted copy (ALAS_SUAVE_VENV_DIR/_RUNNER_DIR, set
        # by the Go launcher only when scripts/build_suave_env.py's runtime was
        # embedded and successfully extracted to its OS-temp cache -- see
        # desktop/suave_runtime.go); (3) resolve_tool_dir's existing best-guess
        # (dev checkout, or a sensible path for the eventual error message).
        # This is what lets a packaged ALAS.exe run mission analysis with
        # zero manual setup while still letting a user override it by simply
        # dropping their own .suave-venv next to the exe.
        import logging as _logging
        import os as _os
        from .paths import SUAVE_RUNNER_DIR_ENV, SUAVE_VENV_DIR_ENV, find_tool_dir

        log = _logging.getLogger("alas.mission")

        # Candidates are VALIDATED, not merely "does the directory exist".
        # The old version took the first existing directory, so an empty or
        # half-populated `.suave-venv`/`external tools/suave_runner` next to the
        # app silently shadowed a perfectly good bundled runtime and produced
        # "not_configured"; likewise a stale env var pointing at an extraction
        # that failed. Each tier is now tried in order and skipped unless it
        # actually contains the interpreter / runner script.
        venv_tiers = [
            ("configured/next-to-app", find_tool_dir(mcfg.suave_venv_dir)),
            (
                "bundled (launcher env)",
                Path(_os.environ[SUAVE_VENV_DIR_ENV])
                if _os.environ.get(SUAVE_VENV_DIR_ENV)
                else None,
            ),
        ]
        runner_tiers = [
            ("configured/next-to-app", find_tool_dir(mcfg.suave_runner_dir)),
            (
                "bundled (launcher env)",
                Path(_os.environ[SUAVE_RUNNER_DIR_ENV])
                if _os.environ.get(SUAVE_RUNNER_DIR_ENV)
                else None,
            ),
        ]
        venv_dir = next(
            (
                p
                for _src, p in venv_tiers
                if p is not None and suave_bridge.venv_is_valid(p)
            ),
            None,
        )
        runner_dir = next(
            (
                p
                for _src, p in runner_tiers
                if p is not None and suave_bridge.runner_is_valid(p)
            ),
            None,
        )

        # Last resort: any runtime the launcher extracted on a previous run. If
        # this build's extraction failed (AV, disk pressure, a killed first
        # launch) the env var is absent, yet a usable pinned runtime is often
        # still sitting in the cache -- reusing it beats refusing to run.
        if venv_dir is None or runner_dir is None:
            for (
                cached_venv,
                cached_runner,
            ) in suave_bridge.discover_extracted_runtimes():
                venv_dir = venv_dir or cached_venv
                runner_dir = runner_dir or cached_runner
                log.info(
                    "SUAVE: falling back to a previously extracted runtime at %s",
                    cached_venv.parent,
                )
                break

        # Nothing valid: keep the best guess so the error names a path the user
        # recognises rather than an empty string.
        venv_dir = venv_dir or resolve_tool_dir(mcfg.suave_venv_dir)
        runner_dir = runner_dir or resolve_tool_dir(mcfg.suave_runner_dir)
        log.info(
            "SUAVE resolution: venv=%s (valid=%s), runner=%s (valid=%s), env venv=%r runner=%r",
            venv_dir,
            suave_bridge.venv_is_valid(venv_dir),
            runner_dir,
            suave_bridge.runner_is_valid(runner_dir),
            _os.environ.get(SUAVE_VENV_DIR_ENV),
            _os.environ.get(SUAVE_RUNNER_DIR_ENV),
        )
        mission_result = suave_bridge.run_mission(
            vehicle_request,
            mission_request,
            venv_dir=venv_dir,
            runner_dir=runner_dir,
            timeout_s=mcfg.timeout_s,
        )
        return route, mission_result

    def _run_mses_analysis(
        self, report: AnalysisReport
    ) -> Tuple["MSESPolarResult", "MSESPressureResult"]:
        """Run an MSES 2-D polar sweep AND a surface Cp/Mach distribution dump
        on the optimized design's root airfoil section, at the trimmed cruise
        operating point.

        Never raises: any failure (missing executables, mesh-generation
        failure on a degenerate section, non-convergence, or -- the gap this
        docstring's claim didn't actually cover before -- an exception in
        this method's OWN preamble, e.g. an unresolvable root-airfoil name)
        comes back as a result with a non-"ok" ``status`` rather than failing
        the whole pipeline run -- same contract as ``_run_mission_analysis``.
        The two ``run_mses_*`` calls already guarantee this for themselves
        (see their own module docstring); the outer try/except here is what
        makes the guarantee hold for the setup code around them too, since a
        raised exception here would otherwise propagate out of the
        ThreadPoolExecutor future's ``.result()`` call in ``run()`` and fail
        the entire run, not just skip MSES.
        """
        import math
        from .geometry.airfoils import AirfoilLibrary, build_section
        from .physics.mses_analysis import (
            MSESPolarResult,
            MSESPressureResult,
            run_mses_polar,
            run_mses_pressure_distribution,
        )

        cfg = self.config
        req = cfg.requirements
        dv = report.design

        try:
            base_airfoil = AirfoilLibrary.get(cfg.geometry.wing.root_airfoil)
            root_section = build_section(dv, base_airfoil.coordinates)

            # MSES analyses a 2-D section; a swept wing's effective SECTION Mach
            # is lower than the freestream Mach by simple sweep theory
            # (M_eff = M_inf * cos(sweep)) -- feeding the raw freestream Mach
            # directly into an unswept 2-D solve produces a far more severe
            # transonic condition than the real swept wing section experiences
            # (confirmed directly: at the raw M0.85, MSES converged only 1/5
            # points with unphysical CL/CD; at the swept-corrected M~0.72, it
            # converged cleanly with a sane polar).
            m_effective = req.cruise_mach * math.cos(math.radians(dv.sweep_deg))

            atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
            v = req.cruise_mach * atmo.speed_of_sound()
            reynolds = float(
                atmo.density() * v * dv.root_chord_m / atmo.dynamic_viscosity()
            )

            trim_alpha = (
                report.trimmed_design_point.alpha_deg
                if report.trimmed_design_point is not None
                else report.design_point.alpha_deg
            )

            # repo_root kept for signature compatibility; mses_analysis now
            # resolves the executables via alas.paths.resolve_tool_dir
            # (frozen-build-aware), so this value is only a sensible fallback.
            from .paths import app_root

            repo_root = app_root()
            polar = run_mses_polar(
                root_section,
                m_effective,
                reynolds,
                trim_alpha_deg=trim_alpha,
                mses_config=cfg.mses,
                repo_root=repo_root,
            )
            pressure = run_mses_pressure_distribution(
                root_section,
                m_effective,
                reynolds,
                alpha_deg=trim_alpha,
                mses_config=cfg.mses,
                repo_root=repo_root,
            )
            return polar, pressure
        except Exception as exc:
            error = f"MSES setup failed before any solve was attempted: {exc}"
            return MSESPolarResult(status="error", error=error), MSESPressureResult(
                status="error", error=error
            )

    def _run_structural_analysis(
        self, report: AnalysisReport
    ) -> StructuralAnalysisResult:
        """Sizes a generic wingbox for the optimized design's main wing,
        computes the analytical (no-NASTRAN) deformation/stress/frequency
        estimate, writes the NASTRAN .bdf mesh, and -- if
        ``config.structures.run_nastran`` -- runs a real NASTRAN solve.

        Downstream/informational only: this never touches ``physics.mass``,
        the CG solve, or the optimizer (see ``physics.structural_sizing``'s
        module docstring). Never raises: any failure (unresolvable material
        name, degenerate geometry, mesh-health hard failure, NASTRAN not
        configured) comes back as a non-"ok" ``status`` -- same contract as
        ``_run_mses_analysis``/``_run_mission_analysis``.
        """
        from .config.materials import get_material
        from .config.structures_config import resolve_spar_geometry
        from .geometry.airfoils import AirfoilLibrary, build_section
        from .geometry.wing_structure import WingStructureGeometry
        from .geometry.wing_mesh_bdf import build_wing_mesh_bdf
        from .physics.structural_sizing import size_wingbox
        from .physics.structural_analysis import analyze_structure
        from .physics import structural_loads as loads
        from .integration.nastran_runner import run_nastran_analysis
        from .integration.patran_runner import run_patran_export

        cfg = self.config
        scfg = cfg.structures
        dv = report.design

        try:
            skin_mat = get_material(scfg.skin_material)
            web_mat = get_material(scfg.spar_web_material)
            cap_mat = get_material(scfg.spar_cap_material)
            rib_mat = get_material(scfg.rib_material)

            root_section = build_section(
                dv, AirfoilLibrary.get(cfg.geometry.wing.root_airfoil).coordinates
            )
            tip_airfoil = AirfoilLibrary.get(cfg.geometry.wing.tip_airfoil)
            spar_fracs, spar_full_span = resolve_spar_geometry(scfg)
            wsg = WingStructureGeometry(
                dv,
                cfg.geometry.wing,
                root_section,
                tip_airfoil,
                spar_fracs,
                spar_full_span,
            )

            sizing = size_wingbox(
                wsg, scfg, cfg.requirements, skin_mat, web_mat, cap_mat, rib_mat
            )
            analysis = analyze_structure(
                wsg,
                sizing,
                scfg,
                cfg.requirements,
                cfg.geometry.engine,
                cfg.mass_model,
                skin_mat,
                web_mat,
                cap_mat,
            )
            model, mesh_health, node_index = build_wing_mesh_bdf(
                wsg,
                sizing,
                scfg,
                cfg.geometry.engine,
                cfg.mass_model,
                cfg.requirements,
                skin_mat,
                web_mat,
                cap_mat,
                rib_mat,
            )

            from .paths import app_root

            repo_root = app_root()
            work_dir = repo_root / "outputs" / "bdf"
            nastran = run_nastran_analysis(
                model, node_index, scfg, cfg.requirements, work_dir, repo_root
            )

            patran = None
            if scfg.run_patran_export and nastran.static.status == "ok":
                case_names = [
                    c.name
                    for c in loads.load_cases(
                        cfg.requirements, scfg.additional_safety_factor
                    )
                ]
                patran = run_patran_export(
                    work_dir,
                    repo_root,
                    scfg.patran_exe_path,
                    case_names,
                    scfg.timeout_s,
                )

            torenbeek_wing_mass = report.component_masses.get("Wing", float("nan"))
            return StructuralAnalysisResult(
                status="ok",
                wsg=wsg,
                sizing=sizing,
                mesh_health=mesh_health,
                analysis=analysis,
                nastran=nastran,
                patran=patran,
                torenbeek_wing_mass_kg=torenbeek_wing_mass,
            )
        except Exception as exc:
            return StructuralAnalysisResult(status="error", error=str(exc))

    # -- helpers -------------------------------------------------------------
    def _export(self, report: AnalysisReport, output_dir: str | Path) -> None:
        out = Path(output_dir)
        out.mkdir(parents=True, exist_ok=True)
        design_report.export_json(report, self.config, out / "design_data.json")
        design_report.export_airfoil_dat(
            report, self.config, out / "optimized_airfoil.dat"
        )

    def _plot(
        self, opt_result, optimized_report, baseline_report, output_dir, show
    ) -> None:
        from .geometry.aircraft_builder import AircraftBuilder
        from .reporting import (
            visualization as viz,
        )  # lazy: only import matplotlib if plotting

        out = Path(output_dir) if output_dir else None

        def render(factory, filename, figsize, *args):
            fig = factory(
                *args, fig=(viz.new_managed_figure(figsize) if show else None)
            )
            if fig is None:
                return
            if out:
                viz.save_figure(fig, out / filename)

        if opt_result is not None:
            render(
                viz.figure_optimization_history,
                "optimization_history.png",
                (10, 6),
                opt_result.history,
            )
            render(
                viz.figure_design_evolution,
                "design_evolution.png",
                (12, 8),
                opt_result.history,
                AircraftBuilder(self.config.geometry),
            )
        render(viz.figure_aero_panel, "aero_panel.png", (12, 9), optimized_report)
        render(viz.figure_geometry, "geometry.png", (12, 9), optimized_report.airplane)

        # New airfoil and wireframe and span loading plots
        render(
            viz.figure_airfoil_comparison,
            "airfoil_comparison.png",
            (10, 5),
            self.config.geometry.wing.root_airfoil,
            optimized_report.design,
        )
        render(
            viz.figure_airfoil_evolution,
            "airfoil_evolution.png",
            (10, 5),
            optimized_report.airplane,
        )
        render(
            viz.figure_wireframe_wing,
            "wireframe_wing.png",
            (10, 6),
            optimized_report.airplane,
        )
        render(
            viz.figure_wireframe_fuselage,
            "wireframe_fuselage.png",
            (10, 6),
            optimized_report.airplane,
        )
        render(
            viz.figure_wireframe_empennage,
            "wireframe_empennage.png",
            (10, 6),
            optimized_report.airplane,
        )
        render(
            viz.figure_span_loading,
            "span_loading.png",
            (10, 6),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_vlm_flow, "vlm_flow.png", (10, 8), optimized_report, self.config
        )
        render(
            viz.figure_vn_diagram,
            "vn_diagram.png",
            (11, 7.5),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_dynamic_modes,
            "dynamic_modes.png",
            (11.5, 6),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_control_surfaces,
            "control_surfaces.png",
            (13.5, 7.5),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_stability_side_view,
            "stability_side_view.png",
            (13, 7),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_stability_metrics,
            "stability_metrics.png",
            (13, 5.5),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_cg_envelope,
            "cg_envelope.png",
            (9, 7),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_landing_gear_planform,
            "landing_gear_planform.png",
            (9, 11),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_mass_distribution,
            "mass_distribution.png",
            (12, 7),
            optimized_report,
        )
        render(
            viz.figure_mass_breakdown, "mass_breakdown.png", (11, 6), optimized_report
        )
        render(
            viz.figure_fuel_volume_check,
            "fuel_volume_check.png",
            (10, 3.2),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_payload_range,
            "payload_range.png",
            (10, 6.5),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_airfoil_reynolds,
            "airfoil_reynolds.png",
            (10, 6),
            optimized_report.airplane.wings[0].xsecs[0].airfoil,
        )
        render(
            viz.figure_propulsion_cycle_summary,
            "propulsion_cycle_summary.png",
            (11, 6),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_propulsion_carpet_plot,
            "propulsion_carpet_plot.png",
            (10, 7),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_propulsion_efficiency_decomposition,
            "propulsion_efficiency.png",
            (10, 6),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_propulsion_bpr_sensitivity,
            "propulsion_bpr_sensitivity.png",
            (10, 6),
            optimized_report,
            self.config,
        )
        render(
            viz.figure_propulsion_altitude_sweep,
            "propulsion_altitude_sweep.png",
            (12, 6),
            optimized_report,
            self.config,
        )

        if baseline_report is not None:
            render(
                viz.figure_polar_comparison,
                "polar_comparison.png",
                (13, 6),
                baseline_report,
                optimized_report,
            )
            render(
                viz.figure_planform_comparison,
                "planform_comparison.png",
                (11, 8),
                baseline_report.airplane,
                optimized_report.airplane,
            )

        if show:
            viz.show_all()
