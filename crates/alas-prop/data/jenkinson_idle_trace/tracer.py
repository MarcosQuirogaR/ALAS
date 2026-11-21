"""Trace the curves of a Jenkinson engine chart (rendered from the book PDF).

usage: tracer.py spec.json
spec: pdf, page, scale, ytick [[row, value],[row, value]], xtick [[col, value],[col, value]],
      rows [r0, r1] (plot rows), left_col, right_col (curve x extent),
      labels (top-to-bottom order at the left end), nodes (Mach values), overlay (png path),
      dark (gray threshold)
"""
import json
import os
import sys
import numpy as np
import pypdfium2 as pdfium
from PIL import Image, ImageDraw

spec = json.load(open(sys.argv[1]))
pdf = pdfium.PdfDocument(os.environ["JENKINSON_PDF"])
img = pdf[spec["page"] - 1].render(scale=spec["scale"]).to_pil().convert("RGB")
gray = np.array(img.convert("L"))
mask = gray < spec.get("dark", 128)
for bx0, by0, bx1, by1 in spec.get("erase", []):
    mask[by0:by1, bx0:bx1] = False
r0, r1 = spec["rows"]
left, right = spec["left_col"], spec["right_col"]

(yr_a, yv_a), (yr_b, yv_b) = spec["ytick"]
(xc_a, xv_a), (xc_b, xv_b) = spec["xtick"]


def val_y(row):
    return yv_a + (row - yr_a) * (yv_b - yv_a) / (yr_b - yr_a)


def row_y(val):
    return yr_a + (val - yv_a) * (yr_b - yr_a) / (yv_b - yv_a)


def val_x(col):
    return xv_a + (col - xc_a) * (xv_b - xv_a) / (xc_b - xc_a)


def col_x(val):
    return xc_a + (val - xv_a) * (xc_b - xc_a) / (xv_b - xv_a)


def clusters(x):
    column = mask[r0:r1, x]
    out = []
    start = None
    for i, v in enumerate(column):
        if v and start is None:
            start = i
        if (not v) and start is not None:
            out.append((start + r0, i - 1 + r0))
            start = None
    if start is not None:
        out.append((start + r0, r1 - 1))
    return out


labels = spec["labels"]
n = len(labels)
start_right = spec.get("start") == "right"
x = spec.get("init_col", right - 8 if start_right else left + 8)
cl = clusters(x)
print("init clusters at col", x, cl)
if spec.get("init_rows"):
    ys = [float(v) for v in spec["init_rows"]]
else:
    if len(cl) != n:
        print("WARNING: %d clusters for %d curves at init" % (len(cl), n))
    ys = [0.5 * (a + b) for a, b in cl]
    if len(ys) != n:
        sys.exit("cannot initialise; set init_rows")
paths = [{x: y} for y in ys]
slopes = [0.0] * n
for xx in (range(x - 1, left - 1, -1) if start_right else range(x + 1, right + 1)):
    cl = clusters(xx)
    preds = []
    for k in range(n):
        xs = sorted(paths[k].keys())
        xs = xs[:60] if start_right else xs[-60:]
        if len(xs) >= 12:
            sl = np.polyfit(xs, [paths[k][v] for v in xs], 1)[0]
        else:
            sl = slopes[k]
        slopes[k] = sl
        ref = xs[0] if start_right else xs[-1]
        preds.append(paths[k][ref] + sl * (xx - ref))
    owner = {}
    for k in range(n):
        best, bd = None, 1e9
        for ci, (a, b) in enumerate(cl):
            if a - 7 <= preds[k] <= b + 7:
                d = 0
            else:
                d = min(abs(preds[k] - a), abs(preds[k] - b))
            if d < bd:
                best, bd = ci, d
        if best is not None and bd < 16:
            owner.setdefault(best, []).append(k)
    for k in range(n):
        y = preds[k]
        for ci, ks in owner.items():
            if k in ks:
                a, b = cl[ci]
                thick = b - a + 1
                if len(ks) == 1 and thick <= spec.get("max_single", 14):
                    y = 0.5 * (a + b)
                elif len(ks) == 1:
                    y = 0.5 * (a + b) if abs(0.5 * (a + b) - preds[k]) < 6 else preds[k]
                # shared or thick: keep prediction
        paths[k][xx] = y

nodes = spec["nodes"]
print("labels (top-to-bottom at left):", labels)
print("nodes", nodes)
res = {}
for k, lab in enumerate(labels):
    row = []
    for m in nodes:
        c = int(round(col_x(m)))
        if c < min(paths[k]) or c > max(paths[k]):
            row.append(None)
        else:
            row.append(round(val_y(paths[k][c]), 4))
    res[lab] = row
    print(lab, row)
json.dump(res, open(spec["overlay"].replace(".png", ".json"), "w"))
draw = ImageDraw.Draw(img)
colors = [(255, 0, 0), (0, 160, 0), (0, 0, 255), (200, 0, 200), (0, 170, 170), (230, 120, 0), (120, 80, 0), (90, 90, 90)]
for k in range(n):
    pts = [(xx, paths[k][xx]) for xx in sorted(paths[k])]
    draw.line(pts, fill=colors[k % len(colors)], width=2)
for m in nodes:
    c = col_x(m)
    draw.line([(c, r0), (c, r1)], fill=(150, 150, 255), width=1)
for lab, row in res.items():
    pass
x0 = int(spec.get("crop", [0, 0, 0, 0])[0])
if spec.get("crop"):
    cx0, cy0, cx1, cy1 = spec["crop"]
    img = img.crop((cx0, cy0, cx1, cy1))
img.save(spec["overlay"])
print("overlay", spec["overlay"], img.size)
