"""Compare tables.json with the raw traces and draw the table points over the book pages.

usage: python check_tables.py <overlay-dir>
Prints, per chart and curve, the largest deviation of the table from the traced node values at the
Mach nodes where a trace exists (Mach >= 0.5 for the thrust charts, every node for the fuel charts);
writes <overlay-dir>/table_<spec>.png with a red circle per table point.
"""
import json
import os
import sys
from pathlib import Path

import numpy as np
import pypdfium2 as pdfium
from PIL import ImageDraw

here = Path(__file__).resolve().parent
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
tables = json.loads((here / "tables.json").read_text())

BPR = {"d3": "3.0", "d65": "6.5", "d8": "8.0", "d13": "13.0", "f3": "3.0", "f65": "6.5", "f8": "8.0", "f13": "13.0"}


def feet_of(label):
    if label == "bundle":
        return [30000, 40000, 45000]
    return [0 if label == "SL" else int(label) * 1000]


worst = {}
for name, bpr in BPR.items():
    kind = "thrust" if name.startswith("d") else "fuel"
    spec = json.loads((here / "specs" / f"spec_{name}.json").read_text())
    trace = json.loads((here / "traces" / f"{name}.json").read_text())
    nodes_t = tables[f"{kind}_mach"] if kind == "fuel" else tables["thrust_mach"]
    rows = tables[kind][bpr]
    pdf = pdfium.PdfDocument(os.environ["JENKINSON_PDF"])
    img = pdf[spec["page"] - 1].render(scale=spec["scale"]).to_pil().convert("RGB")
    draw = ImageDraw.Draw(img)
    (yr_a, yv_a), (yr_b, yv_b) = spec["ytick"]
    (xc_a, xv_a), (xc_b, xv_b) = spec["xtick"]
    px = lambda m: xc_a + (m - xv_a) * (xc_b - xc_a) / (xv_b - xv_a)
    py = lambda v: yr_a + (v - yv_a) * (yr_b - yr_a) / (yv_b - yv_a)
    trace_nodes = [round(0.1 * k, 1) for k in range(0, 10)]
    for label, vals in trace.items():
        for feet in feet_of(label):
            row = rows.get(str(feet))
            if row is None:
                continue
            for m, v in zip(nodes_t, row):
                x, y = px(m), py(v)
                draw.ellipse([x - 7, y - 7, x + 7, y + 7], outline=(255, 0, 0), width=3)
            lo = 0.5 if kind == "thrust" else 0.1
            dev = 0.0
            for m, v in zip(nodes_t, row):
                if m < lo or round(m, 1) not in trace_nodes:
                    continue
                t = vals[trace_nodes.index(round(m, 1))]
                if t is None:
                    continue
                dev = max(dev, abs(v - t) / (abs(t) if kind == "fuel" else 1.0))
            worst[(name, label, feet)] = dev
    if "crop" in spec:
        img = img.crop(tuple(spec["crop"]))
    img.save(out / f"table_{name}.png")
for key, dev in sorted(worst.items()):
    print(key, "thrust abs dev %.4f" % dev if key[0].startswith("d") else "fuel rel dev %.3f" % dev)

# BPR 8 at Mach 0.2-0.3: the curves the column read (columns.py) resolves, in order from the top of the resolved runs.
cols = json.loads((here / "columns_d8.json").read_text())
resolved = {"0.200": {25000: 1, 30000: 2, 40000: 3, 45000: 4}, "0.300": {30000: 1, 40000: 2, 45000: 3}, "0.400": {45000: 3}}
nodes = tables["thrust_mach"]
worst_cols = 0.0
for key, curves in resolved.items():
    j = nodes.index(float(key))
    for feet, run in curves.items():
        dev = abs(tables["thrust"]["8.0"][str(feet)][j] - cols[key][run][0])
        worst_cols = max(worst_cols, dev)
print("BPR 8 Mach 0.2-0.4 resolved curves, worst abs dev from the column read %.4f" % worst_cols)
