"""Direct column readings of the BPR 8.0 thrust chart (Fig. 9.20(d)) at Mach 0.1-0.4.

usage: python columns.py columns_d8.json
The tracer merges the curves that cross inside the chart's bundle, so the Mach 0.2-0.3 nodes of the 25,000-45,000 ft
curves are read here instead: at each Mach column the dark runs (printed line centres) of the 4x render are converted
to FN/FN* with the calibration of specs/spec_d8.json. Each entry is [value, run width in px]; a run wider than the
single line width (about 6 px) holds several curves that cannot be told apart at that column.
"""
import glob
import json
import os
import sys
from pathlib import Path

import numpy as np
import pypdfium2 as pdfium

here = Path(__file__).resolve().parent
spec = json.loads((here / "specs" / "spec_d8.json").read_text())
page = pdfium.PdfDocument(os.environ["JENKINSON_PDF"])[spec["page"] - 1]
img = np.array(page.render(scale=spec["scale"]).to_pil().convert("L"))
(yr_a, yv_a), (yr_b, yv_b) = spec["ytick"]
(xc_a, xv_a), (xc_b, xv_b) = spec["xtick"]
px = lambda m: xc_a + (m - xv_a) * (xc_b - xc_a) / (xv_b - xv_a)
val = lambda y: yv_a + (y - yr_a) * (yv_b - yv_a) / (yr_b - yr_a)
out = {}
for mach in (0.10, 0.15, 0.20, 0.225, 0.25, 0.275, 0.30, 0.35, 0.40):
    x = int(round(px(mach)))
    dark = img[:, x - 1 : x + 2].min(axis=1) < 110
    runs, start = [], None
    for y in range(int(yr_a) - 200, int(yr_b) - 10):
        if dark[y] and start is None:
            start = y
        if not dark[y] and start is not None:
            centre = val(0.5 * (start + y - 1))
            if abs(centre) > 0.0012:  # the zero axis line
                runs.append([round(centre, 4), y - start])
            start = None
    out[f"{mach:.3f}"] = runs
Path(sys.argv[1]).write_text(json.dumps(out, indent=1))
print("ok")
