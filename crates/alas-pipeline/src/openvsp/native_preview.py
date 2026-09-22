"""Capture through OpenVSP's native graphics API in an isolated process.

The pinned facade owns the FLTK/OpenGL event loop; ALAS bounds the entire
process tree with a timeout. No solver analysis is invoked here.
"""
import math
from pathlib import Path
import sys
import time

# The pinned facade imports its server module, which interprets sys.argv as
# server port/configuration. Keep ALAS's path arguments out of that import.
arguments = sys.argv[1:]
sys.argv = sys.argv[:1]

import openvsp_config

openvsp_config._IGNORE_IMPORTS = True
openvsp_config.LOAD_GRAPHICS = True
openvsp_config.LOAD_FACADE = True
openvsp_config.FACADE_SERVER_TIMEOUT = 10
openvsp_config.FACADE_SERVER_ATTEMPTS = 1
openvsp_config.FACADE_PRINT_LEVEL = 0
import openvsp as vsp


def capture(model, output):
    vsp.StartGUI()
    vsp.ReadVSPFile(str(model))
    geoms = vsp.FindGeoms()
    if not geoms:
        raise RuntimeError("OpenVSP loaded no geometry")
    for geom in geoms:
        kind = vsp.GetGeomTypeName(geom)
        if kind in ("Mesh", "NGonMesh"):
            # Analysis-generated meshes are not editable CAD components.
            vsp.DeleteGeom(geom)
            continue
        vsp.SetSetFlag(geom, vsp.SET_SHOWN, True)
        vsp.SetGeomDrawType(geom, vsp.GEOM_DRAW_SHADE)
        vsp.SetGeomDisplayType(geom, vsp.DISPLAY_BEZIER)
        # Display tessellation only; preserve dimensions and section curves.
        vsp.SetParmVal(geom, "Tess_W", "Shape", 65)
    vsp.Update()
    for geom in vsp.FindGeoms():
        low, high = vsp.GetGeomBBoxMin(geom), vsp.GetGeomBBoxMax(geom)
        values = [low.x(), low.y(), low.z(), high.x(), high.y(), high.z()]
        if not all(math.isfinite(v) and abs(v) < 1.0e6 for v in values):
            raise RuntimeError("Invalid CAD bounds for " + vsp.GetGeomName(geom))
    vsp.SetViewAxis(False)
    vsp.SetShowBorders(False)
    vsp.SetBackground(0.12, 0.12, 0.12)
    vehicle = vsp.FindContainer("Vehicle", 0)
    for name, value in [("RotationX", 25), ("RotationY", -20), ("RotationZ", -15)]:
        vsp.SetParmVal(vsp.FindParm(vehicle, name, "AdjustView"), value)
    vsp.UpdateGUI()
    # Give the newly shown native window a draw cycle before fitting/capture.
    time.sleep(0.25)
    vsp.FitAllViews()
    vsp.UpdateGUI()
    # Keep the same native view available when the user opens the CAD file.
    vsp.WriteVSPFile(str(model), vsp.SET_ALL)
    vsp.ScreenGrab(str(output), 1600, 900, True, True)
    if not output.is_file() or output.stat().st_size < 1000:
        raise RuntimeError("OpenVSP did not produce a native screenshot")
    print("ALAS_NATIVE_CAPTURE_COMPLETE", flush=True)


if __name__ == "__main__":
    try:
        capture(Path(arguments[0]).resolve(), Path(arguments[1]).resolve())
    finally:
        try:
            vsp.StopGUI()
        finally:
            # Pinned OpenVSP facade exposes this cleanup method, not close().
            vsp._single._close_server()
