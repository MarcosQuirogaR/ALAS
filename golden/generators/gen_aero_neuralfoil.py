# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::neuralfoil``: the airfoil surrogate itself.

NeuralFoil is a trained network, so unlike every other row in this port there
is no formula to re-derive -- the weights *are* the model. This generator
therefore does two jobs from one reading of the installed package, for the
reason ``gen_geom_selig.py`` gives for doing the same: it writes the embedded
parameter blobs the crate ships (``crates/alas-aero/data/nn-*.bin``), and it
writes the parity fixture (``golden/aero/neuralfoil.json``) that proves those
blobs and the arithmetic wrapped around them. Split across two scripts, the
blob and the fixture could disagree about what was exported.

**Which model sizes are exported.** NeuralFoil ships eight, from ``xxsmall``
to ``xxxlarge``. This program offers five: ``AirfoilSweepScreen.tsx``'s
``MODEL_SIZES`` is ``["small", "medium", "large", "xlarge", "xxlarge"]``, and
every default in ``alas`` -- ``airfoil_screening.py``'s ``model_size="large"``,
``routes_airfoil_sweep.py``'s request default, and the ``model_size="large"``
``neuralfoil.get_aero_from_coordinates`` applies when
``visualization.py:2013`` does not pass one -- is inside that set. Those five
are exported (2.18 MB of ``f32``); the other three are not reachable from any
input this program accepts. The five do not all have the same depth (4, 5, 5,
6 and 6 weight layers), which is the point of exporting more than one: a
reader that assumed a fixed layer count would agree on ``large`` and be wrong
on ``small``.

**Precision.** The stored parameters are ``float32`` and the inputs are
``float64``, so every ``w @ x`` NumPy evaluates is promoted to ``float64``
before the multiply: the *values* are ``f32``-precision, the *arithmetic* is
not. The blobs therefore store the ``f32`` bit patterns exactly, and the Rust
side widens them once at load and works in ``f64`` throughout, which is what
NumPy does. Storing them already widened would double the file for no
information.

**What the fixture records.**

``digests`` -- FNV-1a 64-bit over each exported blob's bytes, which is what
stops ``crates/alas-aero/data/`` drifting from the installed package. Same
scheme, and same reasoning, as ``golden/geom/selig.json``.

``network`` -- ``nf.get_aero_from_kulfan_parameters`` on its own: the raw
network, with none of the compressibility or post-stall machinery around it.
All 198 outputs are recorded per case, not just the six headline ones, because
the 192 boundary-layer channels are where the flip-and-average symmetry
actually shows: a port that got the un-flipping permutation wrong would
produce a plausible ``CL`` and a transposed boundary layer.

``kulfan_airfoil`` -- ``KulfanAirfoil.get_aero_from_neuralfoil``, which is the
network plus post-stall blending, the critical-Mach fit, the wave-drag
schedule and the Mach-tuck moment shift. The cases walk the Mach schedule's
four branches and both ends of the stall blend deliberately;
``_assert_branches`` refuses to write a fixture that misses one.

``airfoil`` -- ``Airfoil.get_aero_from_neuralfoil``, the entry point
``airfoil_screening.py:251`` calls, on sections this program actually screens,
including the morphed root section the default aircraft's builder produces.

``coordinates`` -- ``neuralfoil.get_aero_from_coordinates``, the entry point
``visualization.py:2013`` calls. It is *not* the same function with different
arguments: it reaches ``get_aero_from_airfoil``, which normalizes and calls
the raw network, and never touches the Mach or post-stall corrections. A port
that routed it through the ``KulfanAirfoil`` path would agree at ``mach=0``
and low alpha and disagree everywhere else.
"""

from __future__ import annotations

import struct
from pathlib import Path

import _framework
import numpy as np

_framework.add_alas_to_path()

import aerosandbox as asb  # noqa: E402
import neuralfoil as nf  # noqa: E402
from aerosandbox.geometry.airfoil.kulfan_airfoil import KulfanAirfoil  # noqa: E402
from alas.config import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.geometry.airfoils import AirfoilLibrary  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DATA_DIR = REPO_ROOT / "crates" / "alas-aero" / "data"

# The five sizes this program's own interface can ask for. See the module
# docstring; the other three NeuralFoil ships are unreachable from here.
MODEL_SIZES = ["small", "medium", "large", "xlarge", "xxlarge"]

NETWORK_MAGIC = b"ALASNFW1"
DISTRIBUTION_MAGIC = b"ALASNFD1"

N_BL_POINTS = 32
N_OUTPUTS = 6 + 6 * N_BL_POINTS


def _output_keys() -> list[str]:
    """Every key ``get_aero_from_kulfan_parameters`` returns, in output order."""
    keys = ["analysis_confidence", "CL", "CD", "CM", "Top_Xtr", "Bot_Xtr"]
    for surface in ("upper", "lower"):
        for quantity in ("theta", "H", "ue/vinf"):
            keys += [f"{surface}_bl_{quantity}_{i}" for i in range(N_BL_POINTS)]
    return keys


OUTPUT_KEYS = _output_keys()

# `KulfanAirfoil.get_aero_from_neuralfoil` adds these ten on top, and passes
# the 192 boundary-layer channels through unchanged.
WRAPPED_KEYS = [
    "analysis_confidence",
    "CL",
    "CD",
    "CM",
    "Cpmin",
    "Top_Xtr",
    "Bot_Xtr",
    "mach_crit",
    "mach_dd",
    "Cpmin_0",
]


_FNV_OFFSET_BASIS_64 = 0xCBF29CE484222325
_FNV_PRIME_64 = 0x100000001B3
_MASK_64 = (1 << 64) - 1


def _fnv1a64(data: bytes) -> str:
    """FNV-1a, 64-bit, as 16 lowercase hex digits.

    The same digest ``gen_geom_selig.py`` uses, for the same reason: this is
    proving an exported blob has not drifted from the package it came from,
    not defending against a crafted collision, and adding a cryptographic hash
    to compute it would be a dependency neither side of this port needs.
    """
    digest = _FNV_OFFSET_BASIS_64
    for byte in data:
        digest ^= byte
        digest = (digest * _FNV_PRIME_64) & _MASK_64
    return f"{digest:016x}"


def _layer_indices(params: dict[str, np.ndarray]) -> list[int]:
    """The sorted layer indices, read off the ``net.<i>.weight`` key names.

    The saved dictionaries index layers by their position in the original
    PyTorch ``Sequential``, so the activations occupy the odd indices and the
    weight layers are 0, 2, 4, ... -- which is why this is a sorted set of
    parsed integers upstream rather than ``range(n_layers)``.
    """
    return sorted({int(key.split(".")[1]) for key in params})


def _encode_network(params: dict[str, np.ndarray]) -> bytes:
    """One model's parameters as the crate's embedded blob.

    Layout, little-endian throughout: the magic, the layer count, then one
    ``(rows, cols)`` pair per layer, then each layer's weight matrix in
    row-major order followed by its bias vector -- all ``f32``, exactly the
    bits the ``.npz`` holds.
    """
    indices = _layer_indices(params)
    shapes = [params[f"net.{i}.weight"].shape for i in indices]

    out = bytearray(NETWORK_MAGIC)
    out += struct.pack("<I", len(indices))
    for rows, cols in shapes:
        out += struct.pack("<II", rows, cols)
    for i in indices:
        weight = np.ascontiguousarray(params[f"net.{i}.weight"], dtype=np.float32)
        bias = np.ascontiguousarray(params[f"net.{i}.bias"], dtype=np.float32)
        out += weight.astype("<f4").tobytes()
        out += bias.astype("<f4").tobytes()
    return bytes(out)


def _encode_distribution(distribution: dict[str, np.ndarray]) -> bytes:
    """The training-distribution statistics the confidence output leans on.

    Only two of the three arrays in the ``.npz`` are read at inference time:
    the mean (``f32``) and the inverse covariance (``f64``). The covariance
    itself is never used. Each is stored at its own upstream width, because
    the Mahalanobis distance is evaluated in ``f64`` and narrowing the inverse
    covariance would change the answer.
    """
    mean = np.ascontiguousarray(distribution["mean_inputs_scaled"], dtype=np.float32)
    inverse = np.ascontiguousarray(
        distribution["inv_cov_inputs_scaled"], dtype=np.float64
    )
    if inverse.shape != (mean.size, mean.size):
        raise SystemExit(f"inverse covariance is {inverse.shape}, expected square")

    out = bytearray(DISTRIBUTION_MAGIC)
    out += struct.pack("<I", mean.size)
    out += mean.astype("<f4").tobytes()
    out += inverse.astype("<f8").tobytes()
    return bytes(out)


def _write_blobs() -> dict[str, str]:
    """Write every exported blob and return its digest, keyed by file name."""
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    digests: dict[str, str] = {}

    for size in MODEL_SIZES:
        if size not in nf.main._allowable_model_sizes:
            raise SystemExit(f"the installed NeuralFoil has no {size!r} model")
        blob = _encode_network(nf.main._nn_parameters[size])
        path = DATA_DIR / f"nn-{size}.bin"
        path.write_bytes(blob)
        digests[path.name] = _fnv1a64(blob)
        print(f"wrote {path} ({len(blob) / 1e6:.3f} MB)")

    blob = _encode_distribution(nf.main._scaled_input_distribution)
    path = DATA_DIR / "nn-input-distribution.bin"
    path.write_bytes(blob)
    digests[path.name] = _fnv1a64(blob)
    print(f"wrote {path} ({len(blob) / 1e3:.1f} kB)")

    return digests


def _kulfan_of(airfoil: asb.Airfoil) -> KulfanAirfoil:
    """The eight-per-side Kulfan fit of a normalized section.

    Exactly what both reached call sites do before the network sees anything:
    normalize, then fit without redoing the normalization.
    """
    return airfoil.normalize().to_kulfan_airfoil(
        n_weights_per_side=8, normalize_coordinates=False
    )


def _kulfan_payload(kulfan: KulfanAirfoil) -> dict:
    return {
        "lower_weights": [float(w) for w in kulfan.lower_weights],
        "upper_weights": [float(w) for w in kulfan.upper_weights],
        "leading_edge_weight": float(kulfan.leading_edge_weight),
        "TE_thickness": float(kulfan.TE_thickness),
    }


def _scalar(value) -> float:
    return float(np.reshape(value, -1)[0])


def _morphed_root_section() -> asb.Airfoil:
    """The section ``airfoil_screening.py`` actually hands the surrogate.

    The sweep builds a wing with each candidate as the root airfoil and then
    reads ``plane.wings[0].xsecs[0].airfoil`` -- the *morphed* section, after
    the builder's blending and bump application, not the library entry the
    candidate named. Screening a section that the builder would have altered
    and then scoring the unaltered one is exactly the mistake this case exists
    to catch.
    """
    config = ALASConfig()
    plane = AircraftBuilder(config.geometry).build(include_engines=False)
    return plane.wings[0].xsecs[0].airfoil


def _sections() -> dict:
    """Every section any case below names, resolved once."""
    return {
        "naca2412": asb.Airfoil("naca2412"),
        "naca0012": asb.Airfoil("naca0012"),
        "sc2_0714": AirfoilLibrary.get("SC2-0714"),
        "e63": AirfoilLibrary.get("e63"),
        # Thin enough that the network reports about 0.19 analysis confidence
        # -- the Mahalanobis penalty on the confidence channel is doing real
        # work here, where on an ordinary section it barely moves it.
        "naca0006": asb.Airfoil("naca0006"),
        "naca2412_tilted": _tilted(asb.Airfoil("naca2412")),
        "morphed_root": _morphed_root_section(),
    }


def _tilted(airfoil: asb.Airfoil) -> asb.Airfoil:
    """A section deliberately taken out of the standard frame.

    The moment correction ``Airfoil.get_aero_from_neuralfoil`` applies is
    proportional to how far the section had to be moved to reach unit chord
    at zero incidence, and it is identically zero on a section already there
    -- which is every NACA section and most of the database. The few database
    entries that are meaningfully off-frame are multi-element high-lift decks
    (``30p-30n`` is the extreme: chord 1.51, incidence -2.9 degrees), and an
    eight-weight CST fit of a three-element deck diverges -- weights reaching
    1e7, and a network that overflows to infinity. That is faithful upstream
    behaviour and it is recorded in ``docs/PORTING.md``, but a fixture case
    whose every expected value is an infinity proves nothing, since infinities
    compare equal whatever produced them.

    So the strongly-off-frame case is authored here instead: an ordinary
    section, moved by a known amount, which keeps the fit sane and drives all
    three corrections well clear of their identity values. This is the same
    reasoning ``golden/route/inputs/`` records for the routing fixture.
    """
    moved = airfoil.rotate(np.radians(-4.0)).scale(1.3, 1.3)
    return moved.translate(0.02, -0.01)


# The raw network, with nothing wrapped around it. Every field a caller can
# set is exercised somewhere: the five reachable model sizes (whose depths are
# 4, 5, 5, 6 and 6 weight layers), both signs of alpha, both ends of the
# Reynolds range the visualization grid sweeps, and one case at a non-default
# transition setting -- nothing in `alas` overrides `n_crit`/`xtr_*`, but they
# are three of the twenty-five network inputs, and a port that dropped one
# would agree on every case that left it at its default.
NETWORK_CASES = {
    "large_nominal": dict(section="naca2412", alpha=3.0, re=1e6),
    "small_four_layers": dict(section="naca2412", alpha=3.0, re=1e6, size="small"),
    "medium_five_layers": dict(section="naca2412", alpha=3.0, re=1e6, size="medium"),
    "xlarge_six_layers": dict(section="naca2412", alpha=3.0, re=1e6, size="xlarge"),
    "xxlarge_six_layers": dict(section="naca2412", alpha=3.0, re=1e6, size="xxlarge"),
    "symmetric_zero_alpha": dict(section="naca0012", alpha=0.0, re=1e6),
    "negative_alpha": dict(section="naca2412", alpha=-6.0, re=1e6),
    "high_alpha": dict(section="naca2412", alpha=14.0, re=1e6),
    "low_reynolds": dict(section="naca2412", alpha=3.0, re=1e4),
    "transport_reynolds": dict(section="sc2_0714", alpha=1.5, re=2e7),
    "forced_transition": dict(
        section="naca2412",
        alpha=3.0,
        re=1e6,
        n_crit=5.0,
        xtr_upper=0.3,
        xtr_lower=0.6,
    ),
    "low_analysis_confidence": dict(section="naca0006", alpha=8.0, re=1e6),
}

# `KulfanAirfoil.get_aero_from_neuralfoil`: the network plus the Mach and
# post-stall machinery. The Mach values walk the wave-drag schedule's four
# branches and the alphas reach both ends of the stall blend.
KULFAN_CASES = {
    "incompressible": dict(section="naca2412", alpha=3.0, re=1e6, mach=0.0),
    "subcritical": dict(section="naca2412", alpha=3.0, re=1e6, mach=0.3),
    # naca2412 at these conditions reports mach_crit = 0.6449 and
    # mach_dd = 0.7128, so 0.68 lands inside the quartic rise and 0.90 inside
    # the cosine-Hermite patch that carries it up to Mach 1.1.
    "quartic_rise": dict(section="naca2412", alpha=1.0, re=2e7, mach=0.68),
    "drag_divergence": dict(section="naca2412", alpha=1.0, re=2e7, mach=0.90),
    "supersonic": dict(section="naca2412", alpha=1.0, re=2e7, mach=1.30),
    "transonic_patch_upper_end": dict(section="sc2_0714", alpha=1.0, re=2e7, mach=1.05),
    "separated_positive": dict(section="naca2412", alpha=25.0, re=1e6, mach=0.2),
    "separated_negative": dict(section="naca2412", alpha=-25.0, re=1e6, mach=0.2),
    "alpha_wraps_past_180": dict(section="naca2412", alpha=200.0, re=1e6, mach=0.2),
}

# `Airfoil.get_aero_from_neuralfoil`, the entry point `airfoil_screening.py`
# calls: normalization, the fit, the network, the corrections, and the moment
# correction that moves the answer back to the original section's frame.
AIRFOIL_CASES = {
    "morphed_root_cruise": dict(section="morphed_root", alpha=2.0, re=2e7, mach=0.78),
    "sc2_0714_cruise": dict(section="sc2_0714", alpha=1.5, re=2e7, mach=0.78),
    "e63_off_frame": dict(section="e63", alpha=4.0, re=5e5, mach=0.1),
    "tilted_off_frame": dict(section="naca2412_tilted", alpha=4.0, re=5e6, mach=0.2),
}

# `neuralfoil.get_aero_from_coordinates`, the entry point
# `visualization.py:2013` calls, at corners of the grid it sweeps
# (alpha in [-5, 12], Re in [1e4, 1e7]). This path never reaches the Mach or
# post-stall corrections.
COORDINATE_CASES = {
    "grid_low_corner": dict(section="naca2412", alpha=-5.0, re=1e4),
    "grid_high_corner": dict(section="naca2412", alpha=12.0, re=1e7),
    "grid_interior": dict(section="naca2412", alpha=6.0, re=1e6),
    "e63_off_frame": dict(section="e63", alpha=4.0, re=5e5),
}


def _network_payload(sections: dict, spec: dict) -> dict:
    kulfan = _kulfan_of(sections[spec["section"]])
    size = spec.get("size", "large")
    arguments = dict(
        alpha=spec["alpha"],
        Re=spec["re"],
        n_crit=spec.get("n_crit", 9.0),
        xtr_upper=spec.get("xtr_upper", 1.0),
        xtr_lower=spec.get("xtr_lower", 1.0),
    )
    aero = nf.get_aero_from_kulfan_parameters(
        kulfan_parameters=dict(
            lower_weights=kulfan.lower_weights,
            upper_weights=kulfan.upper_weights,
            leading_edge_weight=kulfan.leading_edge_weight,
            TE_thickness=kulfan.TE_thickness,
        ),
        model_size=size,
        **arguments,
    )
    return {
        "kulfan": _kulfan_payload(kulfan),
        "model_size": size,
        **{key: float(value) for key, value in arguments.items()},
        "outputs": {key: _scalar(aero[key]) for key in OUTPUT_KEYS},
    }


def _kulfan_airfoil_payload(sections: dict, spec: dict) -> dict:
    kulfan = _kulfan_of(sections[spec["section"]])
    size = spec.get("size", "large")
    aero = kulfan.get_aero_from_neuralfoil(
        alpha=spec["alpha"], Re=spec["re"], mach=spec["mach"], model_size=size
    )
    return {
        "kulfan": _kulfan_payload(kulfan),
        "model_size": size,
        "alpha": float(spec["alpha"]),
        "Re": float(spec["re"]),
        "mach": float(spec["mach"]),
        # `t/c` sets the whole supersonic end of the wave-drag schedule, and
        # it comes from the Kulfan surfaces analytically rather than from the
        # vertex list -- a different function from `Airfoil.max_thickness`
        # despite sharing its name through inheritance.
        "max_thickness": float(kulfan.max_thickness()),
        "outputs": {key: _scalar(aero[key]) for key in WRAPPED_KEYS},
        "boundary_layer": {key: _scalar(aero[key]) for key in OUTPUT_KEYS[6:]},
    }


def _airfoil_payload(sections: dict, spec: dict) -> dict:
    airfoil = sections[spec["section"]]
    size = spec.get("size", "large")
    aero = airfoil.get_aero_from_neuralfoil(
        alpha=spec["alpha"], Re=spec["re"], mach=spec["mach"], model_size=size
    )
    normalization = airfoil.normalize(return_dict=True)
    return {
        "coordinates": [[float(x), float(y)] for x, y in airfoil.coordinates],
        "model_size": size,
        "alpha": float(spec["alpha"]),
        "Re": float(spec["re"]),
        "mach": float(spec["mach"]),
        "rotation_angle": float(normalization["rotation_angle"]),
        "scale_factor": float(normalization["scale_factor"]),
        "outputs": {key: _scalar(aero[key]) for key in WRAPPED_KEYS},
    }


def _coordinate_payload(sections: dict, spec: dict) -> dict:
    airfoil = sections[spec["section"]]
    aero = nf.get_aero_from_coordinates(
        coordinates=airfoil.coordinates, alpha=spec["alpha"], Re=spec["re"]
    )
    return {
        "coordinates": [[float(x), float(y)] for x, y in airfoil.coordinates],
        "alpha": float(spec["alpha"]),
        "Re": float(spec["re"]),
        "outputs": {key: _scalar(aero[key]) for key in OUTPUT_KEYS},
    }


def _assert_wave_drag_branches(cases: dict) -> None:
    """Refuse a fixture that does not walk the whole wave-drag schedule.

    ``CD_wave`` is four nested ``np.where`` branches -- zero below the critical
    Mach number, a quartic rise to drag divergence, a cosine-Hermite patch up
    to Mach 1.1, and a blend above it. Each is a different formula, and each
    boundary is where a port that misread one shows up. The branch a case took
    is decided by its Mach number against the ``mach_crit`` and ``mach_dd``
    the case itself reports, so this classifies from the recorded output
    rather than from a threshold hardcoded here.
    """
    reached = set()
    for case in cases.values():
        mach = case["mach"]
        crit = case["outputs"]["mach_crit"]
        divergence = case["outputs"]["mach_dd"]
        if mach < crit:
            reached.add("subcritical")
        elif mach < divergence:
            reached.add("quartic")
        elif mach < 1.1:
            reached.add("patch")
        else:
            reached.add("supersonic")

    missing = {"subcritical", "quartic", "patch", "supersonic"} - reached
    if missing:
        raise SystemExit(
            f"no case reaches the {sorted(missing)} branch(es) of CD_wave; "
            "add a Mach number that does"
        )


def _assert_stall_branches(cases: dict) -> None:
    """Refuse a fixture that stays on one side of the post-stall blend.

    The blend is smooth, so there is no branch to reach -- but at |alpha| well
    inside 20 degrees the separated model contributes essentially nothing, and
    a fixture built only from attached cases would not distinguish a port that
    left the whole 360-degree extension out.
    """
    alphas = [abs(((case["alpha"] + 180.0) % 360.0) - 180.0) for case in cases.values()]
    if not any(alpha >= 22.0 for alpha in alphas):
        raise SystemExit("no case is separated enough to exercise the post-stall blend")
    if not any(alpha <= 10.0 for alpha in alphas):
        raise SystemExit("no case is attached; the blend would be untested at one end")


def _assert_off_frame(cases: dict) -> None:
    """Refuse a fixture in which every section was already normalized.

    The moment correction ``Airfoil.get_aero_from_neuralfoil`` applies is
    proportional to how far the section had to move. On a section already at
    unit chord and zero incidence it is identically zero, so a fixture of such
    sections cannot tell a port that applied it from one that did not.
    """
    if not any(
        case["rotation_angle"] != 0.0 or case["scale_factor"] != 1.0
        for case in cases.values()
    ):
        raise SystemExit(
            "every airfoil case is already in the standard frame; the moment "
            "correction would be zero throughout"
        )


def main() -> None:
    digests = _write_blobs()
    sections = _sections()

    network = {
        name: _network_payload(sections, spec) for name, spec in NETWORK_CASES.items()
    }
    kulfan_airfoil = {
        name: _kulfan_airfoil_payload(sections, spec)
        for name, spec in KULFAN_CASES.items()
    }
    airfoil = {
        name: _airfoil_payload(sections, spec) for name, spec in AIRFOIL_CASES.items()
    }
    coordinates = {
        name: _coordinate_payload(sections, spec)
        for name, spec in COORDINATE_CASES.items()
    }

    _assert_wave_drag_branches(kulfan_airfoil)
    _assert_stall_branches(kulfan_airfoil)
    _assert_off_frame(airfoil)

    _framework.write(
        "aero",
        "neuralfoil",
        {
            "digests": digests,
            "network": network,
            "kulfan_airfoil": kulfan_airfoil,
            "airfoil": airfoil,
            "coordinates": coordinates,
        },
        description=(
            "NeuralFoil's airfoil surrogate: FNV-1a digests of the five "
            "exported parameter blobs (proving crates/alas-aero/data/ against "
            "the installed package), the raw network's 198 outputs, "
            "KulfanAirfoil.get_aero_from_neuralfoil across the wave-drag "
            "schedule and the post-stall blend, and both reached entry "
            "points -- Airfoil.get_aero_from_neuralfoil and "
            "neuralfoil.get_aero_from_coordinates"
        ),
    )


if __name__ == "__main__":
    main()
