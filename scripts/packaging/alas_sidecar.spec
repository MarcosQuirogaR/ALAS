# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec for the ALAS FastAPI sidecar (alas/sidecar/server.py).

Do not invoke `pyinstaller` on this file directly -- run
`scripts/build_sidecar.py`, which resolves the repo root, calls PyInstaller
with this spec, and stages the result under desktop/sidecar_dist/ where the
Go shell's `//go:embed` picks it up. See desktop/sidecar.go for how the
frozen binary is located and run at app startup.

Built as a one-dir bundle (COLLECT), not one-file: one-file mode
self-extracts to a fresh temp directory on every launch (multi-second cold
start for a bundle this size, once scipy/aerosandbox/pyvista are in it) and
its self-extracting-archive shape is a well-known false-positive trigger for
antivirus heuristics. One-dir pays that extraction cost once, at build time,
not on every app launch.
"""

from pathlib import Path

from PyInstaller.utils.hooks import collect_all

REPO_ROOT = Path(SPECPATH).resolve().parent.parent  # noqa: F821 (SPECPATH is injected by PyInstaller)
# entrypoint.py (next to this spec), not server.py directly -- see that
# file's docstring for why server.py can't be frozen as the entry script.
ENTRY_SCRIPT = REPO_ROOT / "scripts" / "packaging" / "entrypoint.py"

BINARY_NAME = "alas-core"

# Packages with data files or import-time dynamism PyInstaller's static
# analysis can't see on its own (compiled extension modules, plugin-style
# lazy loaders, bundled non-.py assets). `collect_all` pulls in each
# package's submodules, data files, and binaries together so none of the
# three is accidentally left out.
COLLECT_ALL_PACKAGES = [
    "aerosandbox",  # airfoil/atmosphere data tables, NeuralFoil model hookup
    "neuralfoil",  # aerosandbox's NN airfoil surrogate -- ships its own weight files
    "casadi",  # aerosandbox's optimization backend -- a SWIG extension plus
    # dozens of solver-plugin DLLs (bonmin, ipopt, osqp, ...) loaded by
    # casadi's own C++ plugin loader at runtime, invisible to PyInstaller's
    # static import analysis
    "scipy",
    "matplotlib",
    # pyvista/vtkmodules deliberately NOT collected any more -- see `excludes`
    # below for why they're dead weight now. They have to be absent from BOTH
    # lists: `collect_all` adds a package as explicit datas/binaries/
    # hiddenimports, and those are honoured regardless of `excludes` (which only
    # prunes the module-import graph), so leaving them here silently kept
    # ~14 MB of pyvista/vtkmodules Python, .pyi stubs and example meshes in the
    # bundle even after they were excluded.
    "pyNastran",
]

datas = [
    # geometry/airfoils.py resolves this as Path(__file__).parent.parent /
    # "data" / "coord_seligFmt.zip" -- a real runtime dependency (the UIUC
    # airfoil coordinate database), not build-time only, so it must land at
    # the same alas/data/ path inside the frozen bundle.
    (str(REPO_ROOT / "alas" / "data" / "coord_seligFmt.zip"), "alas/data"),
    # The Earth texture is public domain (NASA Earth Observatory), so it ships:
    # the route map and 3-D globe render textured on first launch with no
    # download step.
    (str(REPO_ROOT / "alas" / "data" / "textures"), "alas/data/textures"),
    # The airway navdata is deliberately absent. It is GPLv3 and cannot be
    # redistributed under this project's licence, so it is downloaded on demand
    # into paths.user_data_root() -- never into the bundle, which is a per-build
    # extraction cache that each release replaces and would orphan it.
    # integration/assets.py fetches it; paths.resolve_data_path finds it.
]
binaries = []
hiddenimports = [
    # uvicorn.Config(..., loop="auto", http="auto", ws="auto") (the defaults
    # server.py uses) resolves the concrete implementation via a runtime
    # importlib.import_module() call PyInstaller's static analysis can't
    # follow -- list every candidate explicitly so the one actually chosen
    # on the target machine is always present in the bundle.
    "uvicorn.loops.auto",
    "uvicorn.loops.asyncio",
    "uvicorn.protocols.http.auto",
    "uvicorn.protocols.http.h11_impl",
    "uvicorn.protocols.websockets.auto",
    "uvicorn.protocols.websockets.websockets_impl",
    "uvicorn.lifespan.on",
    "uvicorn.lifespan.off",
    "uvicorn.logging",
]

for package in COLLECT_ALL_PACKAGES:
    pkg_datas, pkg_binaries, pkg_hiddenimports = collect_all(package)
    datas += pkg_datas
    binaries += pkg_binaries
    hiddenimports += pkg_hiddenimports

# Development/reference-only material that must never end up in a shipped
# binary, even transitively: GUI toolkits left over from the deleted
# alas/gui/ package, notebook/test/docs tooling that may be installed in
# the same environment via other extras, and pyNastran's Qt-based GUI
# submodule (the sidecar only uses pyNastran's BDF/OP2 readers and the
# headless Patran runner, see alas/integration/patran_runner.py).
excludes = [
    "tkinter",
    "PySide6",
    "PyQt5",
    "PyQt6",
    "pyNastran.gui",
    "IPython",
    "jupyter",
    "notebook",
    "pytest",
    "sphinx",
    # PyVista/VTK: ~330 MB installed (vtk.libs alone is ~265 MB) and by far the
    # largest single contributor to the bundle -- but nothing in the shipped app
    # can reach it any more. Its only consumer was the server-rendered PyVista
    # globe screenshot (reporting/route_globe.build_globe_plotter via
    # sidecar/figures_extra.figure_route_globe), and the Mission & Route tab now
    # draws a real interactive WebGL globe client-side instead; that figure is
    # deliberately NOT registered in EXTRA_FIGURES, so it has no HTTP route and
    # cannot be invoked. The one still-live import from that module,
    # sync_mass_to_route, is pure numpy, and route_globe.py imports pyvista/vtk
    # lazily *inside* build_globe_plotter -- so dropping them here cannot break
    # an import path that actually runs. A dev checkout is unaffected (this spec
    # only shapes the frozen bundle), so build_globe_plotter still works there
    # for a future static-report export.
    "pyvista",
    "vtk",
    "vtkmodules",
]

block_cipher = None

a = Analysis(  # noqa: F821
    [str(ENTRY_SCRIPT)],
    pathex=[str(REPO_ROOT)],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[str(REPO_ROOT / "scripts" / "packaging" / "rthook_casadi_dll_path.py")],
    excludes=excludes,
    noarchive=False,
    # -OO bytecode: strips docstrings and asserts from every bundled module.
    # Slightly smaller PYZ and makes decompiled output less readable -- but
    # be clear-eyed: PyInstaller ships *bytecode*, which tools like
    # decompyle3/pycdc can still largely reconstruct. Real anti-reverse-
    # engineering needs Nuitka (compile to C) or PyArmor (licensed
    # obfuscator); this is a mitigation, not protection.
    optimize=2,
    cipher=block_cipher,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)  # noqa: F821

exe = EXE(  # noqa: F821
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name=BINARY_NAME,
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=True,  # keep a real console subsystem; desktop/sidecar.go hides
    # the window on Windows via CREATE_NO_WINDOW when it spawns this
    # process, which is more reliable than PyInstaller's --windowed mode
    # (whose stdout/stderr handling is inconsistent when a parent process
    # pipes those streams instead of a real console being attached -- and
    # the ALAS_PORT handshake on stdout is load-bearing, see
    # alas/sidecar/server.py).
)

coll = COLLECT(  # noqa: F821
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=False,
    name=BINARY_NAME,
)
