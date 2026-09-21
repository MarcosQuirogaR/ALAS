# Native OpenVSP previews on Windows

ALAS can render a native OpenVSP screenshot through an optional, app-local
Python runtime. This runtime is separate from the batch `vspscript.exe` used to
generate solver artifacts. A logged-in Windows desktop with working OpenGL is
required for graphical capture; a headless service session may be unable to
produce an image.

After a successful geometry export ALAS tries native capture automatically. The
capture has a 30-second process-tree deadline and briefly opens a native graphics
window. Successful captures replace the mesh fallback in Results. Failures are
reported as preview diagnostics and do not invalidate the aerodynamic export.
The screenshot keeps its native aspect ratio, including OpenVSP's automatic crop.

Fuselage export explicitly uses zero skin tangent strength and C0 continuity to
match ALAS's piecewise-linear loft between section envelopes. This prevents
OpenVSP's default spline from adding nose and tail bulges between stations.
Section locations, widths and heights are preserved; small facets from the
original section spacing may remain. A native regression samples the surface
after saving and reopening the model to verify the linear envelope.

The runtime is discovered beside the selected `vspscript.exe`, in
`preview-runtime/python.exe`. `ALAS_OPENVSP_PREVIEW_PYTHON` may override the Python
path when a compatible graphics installation is already available. The retained
`*.capture.py`, `*.preview.stdout.txt`, and `*.preview.stderr.txt` files make capture
reproducible and explain failures. The native capture updates only the separate
`*.cad_preview.vsp3` (display state, tessellation and analysis-mesh removal), never
the solver-facing model or mesh. Generate a fresh result after upgrading ALAS.

The **Explore in OpenVSP** button in the Results geometry figure opens the full
CAD model in the installed `vsp.exe`. It does not require the Python screenshot
runtime. The same button appears in the maximized figure. Its tooltip explains
when the current model or GUI is unavailable.

## Install the optional screenshot runtime

From the repository root in PowerShell:

```powershell
./tools/setup_openvsp_preview.ps1
```

The default destination is
`external tools/OpenVSP-3.51.2-win64/preview-runtime`. No administrator privileges,
system Python installation, `pip`, or changes to `PATH` are required. The script
uses the Windows x64 embedded Python distribution and the matching OpenVSP
Python bindings. NumPy is included because the bindings import it.

The script pins and verifies SHA-256 hashes for:

| Component | Version | Source |
| --- | --- | --- |
| Embedded CPython, Windows x64 | 3.13.7 | [Python distribution](https://www.python.org/ftp/python/3.13.7/python-3.13.7-embed-amd64.zip) |
| OpenVSP bindings, Python 3.13, Windows x64 | 3.51.2 | [OpenVSP distribution](https://openvsp.org/zips/old/windows/OpenVSP-3.51.2-win64-Python3.13.zip) |
| NumPy wheel, CPython 3.13, Windows x64 | 2.3.3 | [PyPI release metadata](https://pypi.org/pypi/numpy/2.3.3/json) |

Verified archives are cached under `.agent/openvsp-runtime`. To reuse downloaded
archives elsewhere, provide `-CacheDirectory` containing `python.zip`,
`openvsp-python.zip`, and `numpy.zip`; missing archives are downloaded. Cached
files are also verified, and a hash mismatch stops setup.

```powershell
./tools/setup_openvsp_preview.ps1 -CacheDirectory 'D:/Downloads/openvsp-cache'
```

Use `-Destination` to place the runtime in another OpenVSP installation's
`preview-runtime` directory. The script refuses an existing destination by
default. With `-Force`, it assembles the replacement first and preserves the
previous directory as `preview-runtime.backup-<unique-id>`. It does not delete
the previous runtime. A failed setup retains staging files for inspection.

## Runtime contents and licenses

The runtime root contains embedded Python, native OpenVSP DLLs, NumPy and its
wheel metadata, plus `python/openvsp` and `python/openvsp_config`. Its isolated
`python313._pth` lists those module locations; system Python packages are not
used. `runtime-manifest.json` records the pinned downloads and hashes.

Python's `LICENSE.txt`, OpenVSP's `LICENSE.OpenVSP.txt`, the binding licenses,
and NumPy's bundled license metadata are retained. Keep these files with any
redistributed runtime.

Successful setup establishes the runtime files; it does not establish that the
machine's graphics driver or desktop session can capture an image. If a native
screenshot cannot be generated, inspect the retained preview diagnostics and
use the mesh fallback or **Explore in OpenVSP** to examine the model directly.
