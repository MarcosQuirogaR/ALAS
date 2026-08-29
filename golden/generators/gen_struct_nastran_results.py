# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::nastran``'s result readers, and the run report's text half.

``golden/struct/nastran.json`` already covers the decks this module writes.
What it cannot cover is the other direction -- what the module reads back --
because reading back needs a solve, and a solve needs a NASTRAN install this
machine does not have. That is the same wall ``alas-struct::op2`` hit, and it
comes off the same way: pyNastran can *write* a real OP2 from a synthesized
result object, so each scenario here is a file pyNastran wrote, and the
expectations are what ``nastran_runner``'s own readers -- ``_read_static``,
``_read_modes``, ``_read_vibration``, imported and called, not transcribed --
pull out of it.

That distinction is the whole point of the fixture. The readers are small, and
almost all of their content is the branches they take when something is
*missing*: a subcase the solve never wrote, a monitor grid absent from the
result, a mode below the rigid-body threshold, a stress table that is not
there. Every scenario below exists to reach one of those branches, because a
reader that returns the right numbers on a complete result and the wrong shape
on an incomplete one is exactly the failure a solve on a user's machine would
produce and this machine would never see.

Two helpers ride along for the run report, which is text rather than numbers:

* ``_tail`` is called directly, like the readers.
* the ``.f06`` fatal-message scan is *transcribed*, because upstream writes it
  inline inside ``_run_nastran`` -- a function that cannot be called without an
  executable to run. The transcription is two lines, and the part worth
  checking survives it: the expectations still come from Python's own
  ``str.splitlines`` and ``str.strip``, which is where a Rust translation can
  actually diverge (``splitlines`` breaks on form feeds, and an ``.f06`` is
  full of them).
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

# The numpy-2.x shim the reference imports before pyNastran; imported for the
# same reason here, and before any pyNastran import. (E402 below: the path and
# the shim have to be set up before the third-party imports can succeed.)
from alas.integration import _nastran_compat  # noqa: F401,E402

import numpy as np  # noqa: E402
import pyNastran.op2.result_objects.table_object as _table_object  # noqa: E402

# Same two writer adjustments gen_struct_op2.py documents: OPHIG is the table a
# real run writes eigenvectors to, and pyNastran's own writer lacks it from one
# lookup table.
_table_object.table_name_to_table_code.setdefault("OPHIG", 7)

from pyNastran.bdf.bdf import BDF  # noqa: E402
from pyNastran.op2.op2 import OP2  # noqa: E402
from pyNastran.op2.tables.oug.oug_displacements import (  # noqa: E402
    ComplexDisplacementArray,
    RealDisplacementArray,
)
from pyNastran.op2.tables.oug.oug_eigenvectors import RealEigenvectorArray  # noqa: E402
from pyNastran.op2.tables.oes_stressStrain.real.oes_plates import (  # noqa: E402
    RealPlateStressArray,
)
import pyNastran.op2.tables.oes_stressStrain.real.oes_plates as _oes_plates  # noqa: E402

_oes_plates.ELEMENT_NAME_TO_NUM_WIDE["CQUAD4-144"] = 87

from alas.geometry.wing_mesh_bdf import MeshNodeIndex  # noqa: E402
from alas.integration import nastran_runner as runner  # noqa: E402
from alas.physics.structural_loads import LoadCase  # noqa: E402

# A front-spar node line running root to tip, with the y stations the mode-shape
# reader sorts by. Deliberately *not* in ascending id order: the reader sorts by
# y, and an id-ordered line would let a translation that forgot to sort pass.
SPAR_NIDS = [4, 46, 110, 233, 517]
SPAR_Y = [0.0, 3.25, 7.5, 12.75, 17.0]
# The rear-spar line. Only the first spar drives the mode shape, so these grids
# appear in the result and must be filtered out of it.
REAR_NIDS = [5, 47, 111, 234, 518]


def _node_index(engine_nids=(110,)) -> MeshNodeIndex:
    return MeshNodeIndex(
        root_nid=SPAR_NIDS[0],
        tip_nid=SPAR_NIDS[-1],
        kink_nid=SPAR_NIDS[2],
        spar_upper_nids=[list(SPAR_NIDS), list(REAR_NIDS)],
        spar_lower_nids=[[n + 1000 for n in SPAR_NIDS], [n + 1000 for n in REAR_NIDS]],
        engine_nids=list(engine_nids),
    )


def _model() -> BDF:
    """A BDF carrying only what ``_read_modes`` asks it for: grid y stations."""
    model = BDF(debug=False)
    for nid, y in zip(SPAR_NIDS, SPAR_Y):
        model.add_grid(nid, [1.0, y, 0.0])
    for nid, y in zip(REAR_NIDS, SPAR_Y):
        model.add_grid(nid, [2.0, y, 0.0])
    return model


def _gridtype(nids) -> np.ndarray:
    ids = np.array(list(nids), dtype="int32")
    return np.column_stack([ids, np.ones(len(ids), dtype="int32")])


def _write(op2: OP2) -> bytes:
    """Write the assembled OP2 through a temp path and return its bytes."""
    import os
    import tempfile

    fd, path = tempfile.mkstemp(suffix=".op2")
    os.close(fd)
    try:
        op2.write_op2(path, nastran_format="msc")
        with open(path, "rb") as handle:
            return handle.read()
    finally:
        os.remove(path)


def _read(raw: bytes) -> OP2:
    import os
    import tempfile
    import warnings

    fd, path = tempfile.mkstemp(suffix=".op2")
    os.write(fd, raw)
    os.close(fd)
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            op2 = OP2(debug=False)
            op2.read_op2(path)
        return op2
    finally:
        os.remove(path)


def _vector(nids, seed: float) -> np.ndarray:
    """One (nnodes, 6) block of plausible displacement magnitudes."""
    n = len(nids)
    base = np.arange(1, n * 6 + 1, dtype="float32").reshape(n, 6)
    return (base * 1.0e-3 + seed).astype("float32")


# ---------------------------------------------------------------------------
# SOL 101 -- _read_static
# ---------------------------------------------------------------------------

CASE_NAMES = ["pull-up", "push-down", "level"]


def _static_op2(subcases, *, with_stress, nids) -> bytes:
    """A SOL 101 file carrying `subcases` displacement tables (and stress)."""
    op2 = OP2()
    op2.set_as_msc()
    gridtype = _gridtype(nids)
    element_node = []
    for eid in (1001, 1002):
        for nid in [0] + list(nids)[:4]:
            for _fiber in range(2):
                element_node.append([eid, nid])
    element_node = np.array(element_node)
    nrows = len(element_node)
    fiber = np.tile(np.array([-0.05, 0.05], dtype="float32"), nrows // 2)
    for sid in subcases:
        op2.displacements[sid] = RealDisplacementArray.add_static_case(
            "OUGV1", gridtype, _vector(nids, sid).reshape(1, len(nids), 6),
            isubcase=sid)
        if with_stress:
            # Column 7 is von Mises in the CQUAD4 corner layout, and it is not
            # the last column -- the signs here make a reader that took the
            # wrong column, or forgot the abs, produce a different maximum.
            sdata = (np.arange(1, nrows * 8 + 1, dtype="float32") * 1.0e6
                     + sid * 1.0e7)
            sdata = sdata.reshape(nrows, 8)
            sdata[:, 7] *= np.where(np.arange(nrows) % 3 == 0, -1.7, 0.4)
            op2.op2_results.stress.cquad4_stress[sid] = (
                RealPlateStressArray.add_static_case(
                    "OES1X1", "CQUAD4-144", 5, element_node, fiber,
                    sdata.reshape(1, nrows, 8).astype("float32"), isubcase=sid))
    return _write(op2)


def _static_record(label, subcases, *, with_stress, nids, node_index) -> dict:
    raw = _static_op2(subcases, with_stress=with_stress, nids=nids)
    cases = [LoadCase(name, 1.0, 1.0) for name in CASE_NAMES]
    result = runner._read_static(_read(raw), node_index, cases)
    return {
        "label": label,
        "op2_hex": raw.hex(),
        "case_names": CASE_NAMES,
        "node_index": _node_index_json(node_index),
        "expected": {
            "status": result.status,
            "tip_deflection_m": {k: float(v)
                                 for k, v in result.tip_deflection_m.items()},
            "root_von_mises_max_pa": {
                k: float(v) for k, v in result.root_von_mises_max_pa.items()},
        },
    }


def _node_index_json(index: MeshNodeIndex) -> dict:
    return {
        "root_nid": int(index.root_nid),
        "tip_nid": int(index.tip_nid),
        "kink_nid": int(index.kink_nid),
        "spar_upper_nids": [[int(n) for n in line]
                            for line in index.spar_upper_nids],
        "spar_lower_nids": [[int(n) for n in line]
                            for line in index.spar_lower_nids],
        "engine_nids": [int(n) for n in index.engine_nids],
    }


def build_static() -> list:
    index = _node_index()
    all_nids = SPAR_NIDS + REAR_NIDS
    return [
        _static_record("all three subcases, stress present", [1, 2, 3],
                       with_stress=True, nids=all_nids, node_index=index),
        # The solve wrote only the first subcase: the reader must skip the two
        # that are absent rather than index past the end of the table.
        _static_record("only subcase 1 solved", [1],
                       with_stress=True, nids=all_nids, node_index=index),
        # Displacements without stress: every case keeps a tip deflection and
        # none gets a von Mises entry.
        _static_record("no stress table", [1, 2, 3],
                       with_stress=False, nids=all_nids, node_index=index),
        # The tip grid is missing from the result. Upstream still records the
        # stress for that case, so the two dictionaries end up different sizes.
        _static_record("tip grid absent from the result", [1, 2, 3],
                       with_stress=True, nids=SPAR_NIDS[:-1] + REAR_NIDS,
                       node_index=index),
    ]


# ---------------------------------------------------------------------------
# SOL 103 -- _read_modes
# ---------------------------------------------------------------------------


def _modal_op2(cycles, nids) -> bytes:
    op2 = OP2()
    op2.set_as_msc()
    cycles = np.array(cycles, dtype="float32")
    eigenvalues = ((cycles.astype("float64") * 2.0 * np.pi) ** 2)
    modes = np.arange(1, len(cycles) + 1, dtype="int32")
    data = np.stack([_vector(nids, float(m)) for m in modes]).astype("float32")
    # Give the T3 column a shape with a real peak, and put the peak somewhere
    # other than the tip so a translation that normalized by the last value
    # rather than the largest one comes out different.
    for mi in range(len(modes)):
        column = np.sin(np.linspace(0.0, np.pi * (mi + 1), data.shape[1]))
        data[mi, :, 2] = (column * (mi + 1)).astype("float32")
    op2.eigenvectors[1] = RealEigenvectorArray.add_modal_case(
        "OPHIG", _gridtype(nids), data.reshape(len(modes), len(nids), 6),
        isubcase=1, modes=modes, eigenvalues=eigenvalues, mode_cycles=cycles)
    return _write(op2)


def _modes_record(label, cycles, nids, node_index) -> dict:
    raw = _modal_op2(cycles, nids)
    result = runner._read_modes(_read(raw), _model(), node_index)
    shape_y = result.mode_shape_y_m
    return {
        "label": label,
        "op2_hex": raw.hex(),
        "node_index": _node_index_json(node_index),
        "grid_y": {str(nid): float(y)
                   for nid, y in zip(SPAR_NIDS + REAR_NIDS, SPAR_Y + SPAR_Y)},
        "expected": {
            "status": result.status,
            "frequencies_hz": [float(f) for f in result.frequencies_hz],
            "mode_shape_y_m": (None if shape_y is None
                               else [float(y) for y in shape_y]),
            "mode_shapes": [[float(v) for v in shape]
                            for shape in result.mode_shapes],
        },
    }


def build_modes() -> list:
    index = _node_index()
    all_nids = SPAR_NIDS + REAR_NIDS
    return [
        # Two modes under the 0.5 Hz rigid-body threshold and three above it.
        _modes_record("two rigid-body modes filtered out",
                      [0.02, 0.31, 1.74, 4.90, 9.35], all_nids, index),
        # Nothing survives the filter: frequencies empty, and the reader stops
        # before it ever builds a shape, so mode_shape_y_m stays absent.
        _modes_record("every mode below the threshold",
                      [0.02, 0.11, 0.44], all_nids, index),
        # No front-spar grid is in the result. The frequencies still come back;
        # the shapes do not.
        _modes_record("front-spar grids absent from the result",
                      [1.74, 4.90], REAR_NIDS, index),
    ]


# ---------------------------------------------------------------------------
# SOL 111 -- _read_vibration
# ---------------------------------------------------------------------------

DAMPING_RATIO = 0.02
PSD_BASE = 0.04


def _freq_op2(freqs, nids) -> bytes:
    op2 = OP2()
    op2.set_as_msc()
    freqs = np.array(freqs, dtype="float32")
    real = np.stack([_vector(nids, float(f)) for f in freqs])
    imag = np.stack([_vector(nids, float(f) + 100.0) for f in freqs])
    # A resonant peak in T3, so peak_frf/peak_freq_hz land on an interior
    # frequency rather than on whichever end of the sweep is largest.
    for fi, f in enumerate(freqs):
        gain = 1.0 / ((1.0 - (float(f) / 6.0) ** 2) ** 2 + 0.01)
        real[fi, :, 2] = (real[fi, :, 2] * gain).astype("float32")
        imag[fi, :, 2] = (imag[fi, :, 2] * gain * 0.1).astype("float32")
    cdata = (real + 1j * imag).astype("complex64").reshape(len(freqs), len(nids), 6)
    op2.displacements[1] = ComplexDisplacementArray.add_freq_case(
        "OUGV1", _gridtype(nids), cdata, isubcase=1, freqs=freqs)
    return _write(op2)


def _vibration_record(label, *, sine_nids, random_nids, modal_freqs,
                      node_index) -> dict:
    freqs = [2.5, 5.0, 6.0, 7.5, 10.0, 14.0]
    sine_raw = None if sine_nids is None else _freq_op2(freqs, sine_nids)
    random_raw = None if random_nids is None else _freq_op2(freqs, random_nids)
    monitors = runner._monitor_set(node_index)
    result = runner._read_vibration(
        None if sine_raw is None else _read(sine_raw),
        None if random_raw is None else _read(random_raw),
        modal_freqs, monitors, DAMPING_RATIO, PSD_BASE)
    return {
        "label": label,
        "sine_op2_hex": None if sine_raw is None else sine_raw.hex(),
        "random_op2_hex": None if random_raw is None else random_raw.hex(),
        "modal_freqs": [float(f) for f in modal_freqs],
        "node_index": _node_index_json(node_index),
        "damping_ratio": DAMPING_RATIO,
        "psd_base_g2_per_hz": PSD_BASE,
        "expected": {
            "status": result.status,
            "frf_freq_hz": (None if result.frf_freq_hz is None
                            else [float(f) for f in result.frf_freq_hz]),
            "frf_tip_abs_m_per_n": (
                None if result.frf_tip_abs_m_per_n is None
                else [float(v) for v in result.frf_tip_abs_m_per_n]),
            "peak_frf": float(result.peak_frf),
            "peak_freq_hz": float(result.peak_freq_hz),
            "miles_rms_m": {k: float(v) for k, v in result.miles_rms_m.items()},
            "nastran_rms_m": {k: float(v)
                              for k, v in result.nastran_rms_m.items()},
        },
    }


def build_vibration() -> list:
    index = _node_index()
    all_nids = SPAR_NIDS + REAR_NIDS
    return [
        _vibration_record("both solves, all four monitors present",
                          sine_nids=all_nids, random_nids=all_nids,
                          modal_freqs=[1.74, 4.90, 9.35], node_index=index),
        # No modal frequencies: the FRF still comes back, Miles' rule does not,
        # because it has no first mode to scale by.
        _vibration_record("no modal frequencies",
                          sine_nids=all_nids, random_nids=all_nids,
                          modal_freqs=[], node_index=index),
        # Only the random solve ran: every sine-derived field stays at its
        # default and only the PSD integral is populated.
        _vibration_record("random only",
                          sine_nids=None, random_nids=all_nids,
                          modal_freqs=[1.74], node_index=index),
        # Only the sine solve ran.
        _vibration_record("sine only",
                          sine_nids=all_nids, random_nids=None,
                          modal_freqs=[1.74], node_index=index),
        # The tip grid is missing, so there is no FRF and no tip entry, but the
        # other three monitors still get their Miles and PSD numbers.
        _vibration_record("tip grid absent from the result",
                          sine_nids=SPAR_NIDS[:-1] + REAR_NIDS,
                          random_nids=SPAR_NIDS[:-1] + REAR_NIDS,
                          modal_freqs=[1.74], node_index=index),
        # An aircraft with no wing-mounted engine: _monitor_set aliases the
        # engine monitor onto the kink grid, so two labels report one grid.
        _vibration_record("no wing-mounted engine",
                          sine_nids=all_nids, random_nids=all_nids,
                          modal_freqs=[1.74],
                          node_index=_node_index(engine_nids=())),
    ]


# ---------------------------------------------------------------------------
# The run report's text half
# ---------------------------------------------------------------------------

F06_CLEAN = (
    "0                                                     N A S T R A N\n"
    "1    MSC.NASTRAN JOB CREATED ON 01-JAN-26 AT 00:00:00\n"
    "0                                   D I S P L A C E M E N T S\n"
    "  * * * END OF JOB * * *\n"
)

# A real .f06 is paginated with form feeds, which Python's splitlines breaks on
# and Rust's lines() does not. Both placements are here, and only the second one
# discriminates: a form feed that follows a newline is stripped off the front of
# the line either way, so a translation that missed it still reports the right
# text. A form feed that ends a line of its own -- the ordinary page eject, with
# the fatal message beginning the next page -- is the one that goes wrong, and
# a reader splitting on newlines alone quotes it with the whole previous line
# still attached.
F06_FATAL = (
    "0        THE FOLLOWING CARD WAS NOT RECOGNIZED\n"
    "\x0c   *** USER FATAL MESSAGE 9994 (IFP)   \n"
    "      THE FIELD IS NOT A REAL NUMBER.\n"
    "\r\n"
    " *** USER FATAL MESSAGE 307 (XSORT)\r\n"
    "0*** USER WARNING MESSAGE 4698\n"
)

F06_PAGINATED = (
    "0                      S U M M A R Y   O F   R E A L   E I G E N V A L U E S\n"
    "      PAGE     3\x0c   *** USER FATAL MESSAGE 5423 (GP4)\n"
    "      THE MODEL HAS NO STIFFNESS.\n"
    "      PAGE     4\x0c *** USER FATAL MESSAGE 3060 (SEKRRS)\n"
)

F06_MANY_FATALS = "".join(
    f"  *** USER FATAL MESSAGE {700 + i} (MODULE)  \n" for i in range(7)
)

TAIL_TEXTS = {
    "empty": "",
    "blank": "   \n\n  \t\n",
    "shorter than the window": "one\ntwo\nthree\n",
    "longer than the window": "".join(f"line {i}\n" for i in range(40)),
    "windows newlines": "alpha\r\nbeta\r\ngamma\r\n",
    "trailing blanks": "kept\n\n\n",
    "form feeds": F06_FATAL,
    "page ejects": F06_PAGINATED,
}


def build_text() -> dict:
    tails = [
        {"label": label, "text": text, "n_lines": n,
         "expected": runner._tail(text, n)}
        for label, text in TAIL_TEXTS.items()
        for n in (15, 3)
    ]
    # Transcribed from _run_nastran, which cannot be called without an
    # executable. What matters -- and what is genuinely Python's here -- is
    # splitlines and strip.
    scans = []
    for label, text in (
        ("clean", F06_CLEAN),
        ("fatal after a leading form feed", F06_FATAL),
        ("fatal after a page eject", F06_PAGINATED),
        ("more fatals than the report shows", F06_MANY_FATALS),
    ):
        lines = [ln.strip() for ln in text.splitlines()
                 if "USER FATAL MESSAGE" in ln]
        scans.append({"label": label, "text": text, "expected_lines": lines,
                      "expected_reported": lines[:5]})
    return {"tail": tails, "fatal_scan": scans}


def main() -> None:
    payload = {
        "static": build_static(),
        "modes": build_modes(),
        "vibration": build_vibration(),
        "text": build_text(),
    }
    _framework.write(
        "struct",
        "nastran_results",
        payload,
        description=(
            "What alas/integration/nastran_runner.py's result readers recover "
            "from a solve, on OP2 files pyNastran wrote: _read_static over four "
            "SOL 101 scenarios (complete, partly solved, no stress table, tip "
            "grid absent), _read_modes over three SOL 103 scenarios (rigid-body "
            "modes filtered, everything filtered, front-spar grids absent), and "
            "_read_vibration over six SOL 111 scenarios (both solves, no modal "
            "frequencies, either solve alone, tip grid absent, no wing-mounted "
            "engine). Also _tail over eight texts at two window sizes, and the "
            ".f06 USER FATAL MESSAGE scan over four .f06 bodies, two of them "
            "paginated with form feeds."
        ),
    )


if __name__ == "__main__":
    main()
