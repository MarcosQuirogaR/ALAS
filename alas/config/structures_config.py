# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Structural analysis (wingbox FEM) configuration.

Generalizes the reference ``Reference Scripts/00_sizing.py``..``05_validation.py``
pipeline -- originally hardcoded to one aircraft's fixed 2-spar wingbox,
fixed materials, and a single load case -- into a config any ALAS design
can use. The split mirrors :mod:`alas.config.mses_config`:

* Everything the *user* chooses beforehand (spar count/positions, materials,
  rib pattern, safety margin, NASTRAN path) lives here.
* Everything the reference scripts *derived* (cap dimensions, rib spacing,
  skin thickness bump) is computed by :mod:`alas.physics.
  structural_sizing` from these inputs -- not stored as a separate field.

Like MSES, running a real NASTRAN solve is opt-in (``run_nastran``, default
off) since a full static+modes+vibration run can take minutes and requires a
real licensed install; the BDF files and the analytical (no-NASTRAN)
deformation/stress/frequency estimates in :mod:`alas.physics.
structural_analysis` are always computed when ``enabled`` is true.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional, Tuple


@dataclass
class StructuresConfig:
    """Wingbox structural analysis settings, editable in Advanced Settings ->
    Structural Analysis."""

    enabled: bool = field(
        default=True,
        metadata={
            "label": "Enabled",
            "help": "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN "
            ".bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of "
            "a normal Run, populating the Structural Analysis Results tab. Does not affect the mass model, "
            "CG, or optimizer -- purely a downstream analysis, like MSES/Propulsion Analysis.",
        },
    )

    # -- Wingbox configuration (user-controlled) -----------------------------
    spar_chord_fractions: Tuple[float, ...] = field(
        default=(0.25, 0.70),
        metadata={
            "label": "Spar chord positions",
            "unit": "x/c",
            "help": "Chordwise position of each spar, as a fraction of local chord (0=leading edge, 1=trailing "
            "edge). One entry per spar, any order (sorted automatically). E.g. (0.25, 0.70) is a classic "
            "front/rear 2-spar box; add a third entry for a mid-spar.",
        },
    )
    te_rib_mode: str = field(
        default="all",
        metadata={
            "label": "Trailing-edge rib panel mode",
            "help": "Which ribs get trailing-edge panels (rear-spar to TE), preventing TE buckling: 'all', 'none', "
            "'alternate', 'inboard' (only inboard of the wing break), 'outboard', or 'step_N' (every Nth rib).",
        },
    )
    center_spar_enabled: bool = field(
        default=False,
        metadata={
            "label": "Add center spar (root-to-kink)",
            "help": "Adds an optional third spar running only from the root to the wing break/kink station, at "
            "center_spar_chord_fraction of local chord -- the partial-span reinforcement spar common on "
            "widebody wings (extra bending/shear capacity where root load is highest, without the mass of "
            "running it all the way to the tip). Off by default (classic 2-spar box). This spar carries "
            "no load and contributes no mass outboard of the kink -- it simply doesn't exist there.",
        },
    )
    center_spar_chord_fraction: float = field(
        default=0.50,
        metadata={
            "label": "Center spar chord position",
            "unit": "x/c",
            "help": "Chordwise position of the optional center spar (see center_spar_enabled), as a fraction of "
            "local chord. Only used when center_spar_enabled is True.",
        },
    )

    # -- Materials (user-selectable, see config.materials.MATERIAL_DATABASE) -
    skin_material: str = field(
        default="Al 7075-T6",
        metadata={
            "label": "Skin material",
            "help": "Material name from the built-in structural material database, used for the wing skin panels.",
        },
    )
    spar_web_material: str = field(
        default="Al 7075-T6",
        metadata={
            "label": "Spar web material",
            "help": "Material for the spar shear webs.",
        },
    )
    spar_cap_material: str = field(
        default="CFRP UD",
        metadata={
            "label": "Spar cap material",
            "help": "Material for the spar caps (the primary bending-load-carrying members).",
        },
    )
    rib_material: str = field(
        default="Al 7075-T6",
        metadata={
            "label": "Rib material",
            "help": "Material for the rib webs.",
        },
    )

    # -- Sizing assumptions (app-determined geometry, user-tunable knobs) ----
    additional_safety_factor: float = field(
        default=1.0,
        metadata={
            "label": "Additional safety factor",
            "unit": "-",
            "help": "Extra margin multiplied onto the design loads on top of the CS-25 ultimate load factors "
            "already used (DesignRequirements.ultimate_load_factor / limit_load_factor_neg). 1.0 = no "
            "extra margin beyond CS-25 ultimate.",
        },
    )
    t_skin_min_m: float = field(
        default=0.006,
        metadata={
            "label": "Skin gauge",
            "unit": "m",
            "help": "Wing skin thickness -- this is the value actually used (no shear-flow/buckling upsizing is "
            "modeled, see structural_sizing.py), so treat it as a practical starting assumption for this "
            "class of aircraft, not a bare absolute-minimum gauge. 6mm matches the reference sizing "
            "scripts' own baseline for a large long-range wing; a much thinner value (e.g. 2mm) understates "
            "real skin panel buckling resistance and inflates the auto-derived rib count substantially, "
            "since rib spacing scales with sqrt(t_skin).",
        },
    )
    t_web_min_m: float = field(
        default=0.002,
        metadata={
            "label": "Minimum spar web gauge",
            "unit": "m",
        },
    )
    t_rib_m: float = field(
        default=0.004,
        metadata={
            "label": "Rib web thickness",
            "unit": "m",
        },
    )
    t_te_strip_m: float = field(
        default=0.002,
        metadata={
            "label": "Trailing-edge strip thickness",
            "unit": "m",
        },
    )
    cap_taper_eta_lock: float = field(
        default=0.40,
        metadata={
            "label": "Cap taper lock station",
            "unit": "0-1 of semispan",
            "help": "Spanwise fraction below which spar caps keep their full root section (bending moment is "
            "highest inboard, so locking the section here preserves most of the tip-deflection stiffness). "
            "Above this station, caps taper linearly down to cap_taper_tip_fraction at the tip.",
        },
    )
    cap_taper_tip_fraction: float = field(
        default=0.20,
        metadata={
            "label": "Cap taper tip fraction",
            "unit": "-",
            "help": "Fraction of the locked-section cap flange width/thickness remaining at the wingtip.",
        },
    )
    rib_buckling_coeff: float = field(
        default=1.5,
        metadata={
            "label": "Rib spacing buckling coefficient",
            "unit": "-",
            "help": "Empirical panel-buckling coefficient (c) in the Euler skin-panel critical stress formula used "
            "to auto-derive rib spacing/count -- higher allows wider rib spacing for the same skin thickness.",
        },
    )
    rib_radius_of_gyration_m: float = field(
        default=0.030,
        metadata={
            "label": "Stiffened-panel radius of gyration",
            "unit": "m",
            "help": "Effective radius of gyration of a skin panel stiffened by a stringer, used by the same rib-"
            "spacing buckling formula.",
        },
    )
    num_ribs_override: Optional[int] = field(
        default=None,
        metadata={
            "label": "Rib count override",
            "help": "Force an exact number of ribs instead of the auto-derived panel-buckling spacing. Leave blank "
            "to let the app determine rib count/spacing.",
        },
    )
    spanwise_stations: int = field(
        default=200,
        metadata={
            "label": "Spanwise integration stations",
            "help": "Number of spanwise points used for load/moment/deflection integration (sizing and the "
            "analytical deformation/stress solver). Higher = smoother curves, slower.",
        },
    )
    mesh_chordwise_points: int = field(
        default=50,
        metadata={
            "label": "Mesh chordwise points per rib",
            "help": "Number of chordwise points sampled per rib cross-section in the FEM mesh.",
        },
    )

    # -- NASTRAN integration (mirrors MSESConfig) ------------------------------
    # Path + enable moved to Setup > External Tools (a single consolidated
    # page for every external-tool integration) -- hidden here so they're not
    # edited in two places, but still live on this dataclass so the rest of
    # the pipeline is unaffected.
    nastran_exe_path: str = field(
        default="",
        metadata={
            "label": "NASTRAN executable path",
            "help": "Path (repo-root-relative or absolute) to nastran.exe. Leave blank to only generate .bdf files "
            "and use the theoretical (analytical) deformation/stress/frequency estimates -- no NASTRAN "
            "install is required for that path. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )
    run_nastran: bool = field(
        default=True,
        metadata={
            "label": "Run NASTRAN",
            "help": "Actually invoke nastran_exe_path as a subprocess on the generated .bdf files. On by default: "
            "when the executable isn't configured/found, the app still writes valid .bdf files and shows the "
            "theoretical (analytical) results only, so leaving this on is always safe. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )
    run_sol_static: bool = field(
        default=True,
        metadata={
            "label": "Run static analysis (SOL 101)",
            "help": "Pull-up / push-down / 1g-level static load cases -> deformation and stress.",
        },
    )
    run_sol_modes: bool = field(
        default=True,
        metadata={
            "label": "Run normal modes (SOL 103)",
            "help": "Natural frequencies and mode shapes.",
        },
    )
    run_sol_vibration_sine: bool = field(
        default=True,
        metadata={
            "label": "Run sine sweep (SOL 111)",
            "help": "Modal frequency response to a harmonic engine-mounted excitation force.",
        },
    )
    run_sol_vibration_random: bool = field(
        default=True,
        metadata={
            "label": "Run random vibration (SOL 111)",
            "help": "Modal random-vibration response (PSD) to a white-noise engine-mounted excitation.",
        },
    )
    timeout_s: float = field(
        default=3600.0,
        metadata={
            "label": "NASTRAN timeout per solution",
            "unit": "s",
            "help": "Max time allowed for one NASTRAN solution (SOL 101/103/111 each run separately) before it's killed.",
        },
    )
    n_modes: int = field(
        default=30,
        metadata={
            "label": "Number of modes to extract",
            "help": "Max structural modes for SOL 103's EIGRL and the analytical Rayleigh-quotient estimate.",
        },
    )
    freq_sweep_max_hz: float = field(
        default=500.0,
        metadata={
            "label": "Frequency sweep upper limit",
            "unit": "Hz",
        },
    )
    freq_step_hz: float = field(
        default=1.0,
        metadata={
            "label": "Frequency sweep step",
            "unit": "Hz",
        },
    )
    modal_damping_ratio: float = field(
        default=0.02,
        metadata={
            "label": "Modal damping ratio",
            "unit": "fraction of critical",
            "help": "Structural damping assumed for the sine/random vibration response (2% is a common metallic-"
            "airframe assumption).",
        },
    )
    psd_base_g2_per_hz: float = field(
        default=0.01,
        metadata={
            "label": "Random vibration base PSD",
            "unit": "g^2/Hz",
            "help": "Flat white-noise acceleration power spectral density applied at the excitation point for the "
            "random-vibration case.",
        },
    )

    # -- Patran headless render (mirrors NASTRAN above) ------------------------
    # Path + enable moved to Setup > External Tools -- see the NASTRAN fields above.
    patran_exe_path: str = field(
        default="",
        metadata={
            "label": "Patran executable path",
            "help": "Path (repo-root-relative or absolute) to patran.exe. Leave blank to skip -- no Patran install "
            "is required for anything else in Structural Analysis. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )
    run_patran_export: bool = field(
        default=True,
        metadata={
            "label": "Render Patran deformation plots",
            "help": "After a successful NASTRAN SOL 101 static solve, batch-replay a Patran session per load case "
            "to export a deformation-plot PNG (same view MSC Patran's own interactive GUI shows). On by "
            "default; needs a real licensed Patran install and launches it as a subprocess per load case "
            "(a few seconds each) -- has no effect when patran_exe_path isn't configured. Requires "
            "run_nastran and run_sol_static to both be on. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )


def resolve_spar_geometry(
    cfg: "StructuresConfig",
) -> Tuple[Tuple[float, ...], Tuple[bool, ...]]:
    """Returns ``(spar_chord_fractions, spar_full_span)`` -- the actual spar
    list ``WingStructureGeometry`` should be built with, folding in the
    optional partial-span center spar (``center_spar_enabled``) so every
    call site (pipeline, GUI live preview, CLI) composes the same list the
    same way instead of each re-deriving it independently."""
    fracs = tuple(cfg.spar_chord_fractions)
    full_span = tuple(True for _ in fracs)
    if cfg.center_spar_enabled:
        fracs = fracs + (cfg.center_spar_chord_fraction,)
        full_span = full_span + (False,)
    return fracs, full_span
