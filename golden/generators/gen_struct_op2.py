# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::op2``: the native OP2 result reader.

This module has no Python counterpart to run. The reference does not read OP2
itself -- ``alas/integration/nastran_runner.py`` hands the file to pyNastran's
``OP2.read_op2`` and reads values off the result object. The Rust port reads the
binary directly, so the thing it must agree with is *pyNastran's reader*: given
the same file, recover the same numbers.

There is also no NASTRAN install here to produce a real OP2 to read. pyNastran
solves both problems at once: it can synthesize a result object and write a real
OP2 from it, so each case here is a file pyNastran wrote, and the recorded
values are what pyNastran's own reader gets back from that file. The claim the
parity test makes is exactly ``read_op2(bytes) == pyNastran.read_op2(bytes)``,
across the four tables the runner's readers ask for:

* SOL 101 real static displacements (``op2.displacements``) and CQUAD4 corner
  von Mises stress (``op2.op2_results.stress.cquad4_stress``), in one file, as a
  real SOL 101 run produces them together;
* SOL 103 eigenvectors with their eigenvalues and mode cycles
  (``op2.eigenvectors``), including a sub-0.5 Hz mode the runner would later
  filter -- the reader recovers every mode, filtering is the runner's job;
* SOL 111 complex frequency-response displacements (``op2.displacements`` again,
  but complex), one subcase over several frequencies.

Two pyNastran adjustments the writer needs, both making it emit the layout a
real MSC.Nastran run writes rather than working around it:

* ``OPHIG`` is the table a real run writes eigenvectors to, and pyNastran's
  reader routes that name to ``op2.eigenvectors``; its ``add_modal_case`` helper
  just lacks it from one lookup table, so it is registered with the eigenvector
  table code (7).
* pyNastran 1.4.1's CQUAD4-144 (corner) stress writer computes its record size
  from a ``num_wide`` of 70, but then writes 87 words per element (2 header +
  5 nodes x 17), which is the real MSC value and the size its own reader
  expects; the stale 70 is corrected to 87 so the writer runs.

The ``.op2`` bytes ride in the fixture as a hex string: the recorded numbers are
the reviewable part, the file is only the reader's input, and keeping it in the
JSON leaves the parity test hermetic and ``golden/`` free of binary blobs. Hex
rather than base64 so the test decodes it in a handful of lines and takes no
dependency for it.
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
import pyNastran.op2.tables.oes_stressStrain.real.oes_plates as _oes_plates  # noqa: E402

_table_object.table_name_to_table_code.setdefault("OPHIG", 7)
_oes_plates.ELEMENT_NAME_TO_NUM_WIDE["CQUAD4-144"] = 87

from pyNastran.op2.op2 import OP2  # noqa: E402
from pyNastran.op2.tables.oes_stressStrain.real.oes_plates import (  # noqa: E402
    RealPlateStressArray,
)
from pyNastran.op2.tables.oug.oug_displacements import (  # noqa: E402
    ComplexDisplacementArray,
    RealDisplacementArray,
)
from pyNastran.op2.tables.oug.oug_eigenvectors import RealEigenvectorArray  # noqa: E402

# A representative wing-root node line: the ids do not matter to the reader (it
# strips the device code and keeps the grid id), but real ids read better than
# 1..n and confirm the id arithmetic on numbers that are not all one digit.
NODE_IDS = [4, 10, 46, 110, 233]
GRIDTYPE = np.ones(len(NODE_IDS), dtype="int32")
NODE_GRIDTYPE = np.column_stack([np.array(NODE_IDS, dtype="int32"), GRIDTYPE])


def _vector_data(seed: float) -> np.ndarray:
    """One (nnodes, 6) block of plausible displacement magnitudes."""
    n = len(NODE_IDS)
    base = np.arange(1, n * 6 + 1, dtype="float32").reshape(n, 6)
    return (base * 1.0e-3 + seed).astype("float32")


def _write(op2: OP2) -> bytes:
    """Write the assembled OP2 to bytes via a temp path, return the bytes.

    pyNastran writes to a filename; the file is read straight back into memory
    so nothing lands on disk beside the fixture.
    """
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


def _vector_record(table) -> dict:
    return {
        "node_ids": [int(n) for n in table.node_gridtype[:, 0]],
        "data": [[float(v) for v in row] for row in table.data[0]],
    }


def build_sol101() -> dict:
    """Real static displacements and CQUAD4 corner stress, two load cases."""
    op2 = OP2()
    op2.set_as_msc()
    eids = [1001, 1002]
    corners = NODE_IDS[:4]
    element_node = []
    for eid in eids:
        for nid in [0] + corners:  # centroid then four corners
            for _fiber in range(2):  # bottom, top
                element_node.append([eid, nid])
    element_node = np.array(element_node)
    nrows = len(element_node)
    fiber = np.tile(np.array([-0.05, 0.05], dtype="float32"), nrows // 2)
    for sid in (1, 2):
        op2.displacements[sid] = RealDisplacementArray.add_static_case(
            "OUGV1", NODE_GRIDTYPE, _vector_data(sid).reshape(1, len(NODE_IDS), 6),
            isubcase=sid)
        sdata = (np.arange(1, nrows * 8 + 1, dtype="float32") * 1.0e6 + sid * 1.0e7)
        op2.op2_results.stress.cquad4_stress[sid] = RealPlateStressArray.add_static_case(
            "OES1X1", "CQUAD4-144", 5, element_node, fiber,
            sdata.reshape(1, nrows, 8), isubcase=sid)
    raw = _write(op2)
    back = _read(raw)
    displacements = {
        str(sid): _vector_record(back.displacements[sid])
        for sid in sorted(back.displacements)
    }
    stress = {}
    for sid in sorted(back.op2_results.stress.cquad4_stress):
        s = back.op2_results.stress.cquad4_stress[sid]
        stress[str(sid)] = {
            "element_ids": [int(e) for e in s.element_node[:, 0]],
            "node_ids": [int(n) for n in s.element_node[:, 1]],
            "data": [[float(v) for v in row] for row in s.data[0]],
        }
    return {
        "op2_hex": raw.hex(),
        "displacements": displacements,
        "cquad4_stress": stress,
    }


def build_sol103() -> dict:
    """Eigenvectors: one rigid-body-ish sub-0.5 Hz mode and three elastic ones."""
    op2 = OP2()
    op2.set_as_msc()
    eigenvalues = np.array([0.02, 120.0, 480.0, 1080.0], dtype="float64")
    modes = np.arange(1, len(eigenvalues) + 1, dtype="int32")
    cycles = (np.sqrt(eigenvalues) / (2.0 * np.pi)).astype("float32")
    n = len(NODE_IDS)
    data = np.stack([_vector_data(float(m)) for m in modes]).astype("float32")
    op2.eigenvectors[1] = RealEigenvectorArray.add_modal_case(
        "OPHIG", NODE_GRIDTYPE, data.reshape(len(modes), n, 6), isubcase=1,
        modes=modes, eigenvalues=eigenvalues, mode_cycles=cycles)
    raw = _write(op2)
    back = _read(raw)
    ev = back.eigenvectors[1]
    record = {
        "modes": [int(m) for m in ev.modes],
        "eigenvalues": [float(e) for e in ev.eigns],
        "mode_cycles": [float(c) for c in ev.mode_cycles],
        "node_ids": [int(n) for n in ev.node_gridtype[:, 0]],
        "data": [[[float(v) for v in row] for row in ev.data[mi]]
                 for mi in range(ev.data.shape[0])],
    }
    return {
        "op2_hex": raw.hex(),
        "eigenvectors": {"1": record},
    }


def build_sol111() -> dict:
    """Complex frequency-response displacements, one subcase over four freqs."""
    op2 = OP2()
    op2.set_as_msc()
    freqs = np.array([2.5, 5.0, 7.5, 10.0], dtype="float32")
    n = len(NODE_IDS)
    real = np.stack([_vector_data(float(f)) for f in freqs])
    imag = np.stack([_vector_data(float(f) + 100.0) for f in freqs])
    cdata = (real + 1j * imag).astype("complex64").reshape(len(freqs), n, 6)
    op2.displacements[1] = ComplexDisplacementArray.add_freq_case(
        "OUGV1", NODE_GRIDTYPE, cdata, isubcase=1, freqs=freqs)
    raw = _write(op2)
    back = _read(raw)
    d = back.displacements[1]
    record = {
        "freqs": [float(f) for f in d.freqs],
        "node_ids": [int(nid) for nid in d.node_gridtype[:, 0]],
        "real": [[[float(z.real) for z in row] for row in d.data[fi]]
                 for fi in range(d.data.shape[0])],
        "imag": [[[float(z.imag) for z in row] for row in d.data[fi]]
                 for fi in range(d.data.shape[0])],
    }
    return {
        "op2_hex": raw.hex(),
        "complex_displacements": {"1": record},
    }


def main() -> None:
    payload = {
        "sol101_static": build_sol101(),
        "sol103_modal": build_sol103(),
        "sol111_freq": build_sol111(),
    }
    _framework.write(
        "struct",
        "op2",
        payload,
        description=(
            "pyNastran-written OP2 files and the values pyNastran's own reader "
            "recovers from them, for the four tables alas/integration/"
            "nastran_runner.py reads: SOL 101 static displacements and CQUAD4 "
            "corner von Mises stress in one file, SOL 103 eigenvectors with "
            "eigenvalues and mode cycles (including a sub-0.5 Hz mode), and SOL "
            "111 complex frequency-response displacements. Each case carries the "
            "raw .op2 as a hex string and the recovered node/element ids, data "
            "arrays, cycles and frequencies."
        ),
    )


if __name__ == "__main__":
    main()
