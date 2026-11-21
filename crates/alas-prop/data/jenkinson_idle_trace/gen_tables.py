"""Assemble the Jenkinson descent tables (charts.rs) from the page readings.

Readings (Jenkinson, Simpkin, Rhodes, Civil Jet Aircraft Design, 1999):
  thrust FN/FN*: Figs 9.18(d) p.204 (BPR 3.0), 9.19(d) p.207 (6.5), 9.20(d) p.210 (8.0), 9.21(d) p.213 (13.0)
  fuel lb/hr/FN*: Figs 9.18(f) p.205, 9.19(f) p.208, 9.20(f) p.211, 9.21(f) p.214
Each literal below is either a traced node (traces/<spec>.json, Mach >= 0.4, run_all.py) or a
construction from traced points, stated in README.md:
  * M0.3 (and M0.4 for BPR 6.5 / 13) of the curves that meet at P (BPR 8: second kink, M0.3 only): straight line from the chart's common crossing point P
    (BPR 6.5 (0.20, 0.0098); BPR 13 (0.225, 0.0047); BPR 3 curves SL/10k/20k (0.265, 0.0)) to the traced M0.5 value;
  * M0.2: the straight line from the curve's left end (M ~ 0, read in README.md) to P (BPR 8: P = (0.23, 0.0231)
    for the SL, 10,000 and 15,000 ft curves only);
  * BPR 8, 25,000-45,000 ft, M0.2 and M0.3: direct column readings (columns.py, columns_d8.json); these curves do not
    pass through P (they fan out below it, 0.0220/0.0197/0.0164/0.0134 at M0.2) and are resolved up to M0.3; at M0.4 the
    45,000 ft curve is the single-width run at 0.0066 (the others stay merged at 0.0096 with the 25k curve);
  * no height-monotone projection is applied: the traced values are stored as read. (An earlier version pooled
    adjacent heights at Mach >= 0.4; that is not physical where the net thrust is positive, because gross thrust
    falls with density as well. The ram-drag argument holds only where the net thrust is negative, and there the
    traced values need no pooling: the test the_traced_charts_obey_the_physical_invariants checks it.)
usage: python gen_tables.py charts.rs tables.json
"""
import json
import sys

M_D = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]
H = lambda ft: ft  # feet

# thrust: chart -> rows (feet, [M0.2 .. M0.8]); raw readings
raw = {
    3.0: {
        0: [0.0087, -0.0069, -0.0266, -0.0518, -0.0813, -0.1088, -0.1088],
        10000: [0.0064, -0.0044, -0.0169, -0.0331, -0.0518, -0.0744, -0.0992],
        20000: [0.0030, -0.0012, -0.0045, -0.0126, -0.0211, -0.0308, -0.0411],
        30000: [0.0264, 0.0194, 0.0143, 0.0087, -0.0005, -0.0033, -0.0094],
        40000: [0.0710, 0.0620, 0.0549, 0.0485, 0.0420, 0.0360, 0.0302],
        45000: [0.0756, 0.0669, 0.0602, 0.0542, 0.0485, 0.0434, 0.0395],
    },
    6.5: {
        0: [0.0098, 0.0014, -0.0071, -0.0155, -0.0229, -0.0311, -0.0394],
        10000: [0.0098, 0.0039, -0.0019, -0.0078, -0.0135, -0.0192, -0.0247],
        15000: [0.0098, 0.0054, 0.0010, -0.0034, -0.0090, -0.0139, -0.0185],
        25000: [0.0098, 0.0067, 0.0035, 0.0004, -0.0026, -0.0048, -0.0071],
        36000: [0.0098, 0.0066, 0.0034, 0.0002, -0.0009, -0.0025, -0.0041],
        40000: [0.0098, 0.0070, 0.0041, 0.0013, 0.0013, 0.0003, -0.0021],
    },
    8.0: {
        0: [0.0284, 0.0170, 0.0083, -0.0047, -0.0163, -0.0286, -0.0412],
        10000: [0.0273, 0.0185, 0.0118, 0.0032, -0.0048, -0.0128, -0.0209],
        15000: [0.0267, 0.0182, 0.0111, 0.0052, 0.0006, -0.0067, -0.0128],
        25000: [0.0220, 0.0161, 0.0095, 0.0086, 0.0044, 0.0003, -0.0030],
        30000: [0.0197, 0.0146, 0.0096, 0.0051, 0.0041, 0.0031, 0.0010],
        40000: [0.0164, 0.0123, 0.0096, 0.0051, 0.0041, 0.0031, 0.0010],
        45000: [0.0134, 0.0101, 0.0066, 0.0051, 0.0041, 0.0031, 0.0010],
    },
    13.0: {
        0: [0.0069, -0.0035, -0.0145, -0.0255, -0.0366, -0.0479, -0.0591],
        10000: [0.0066, -0.0011, -0.0089, -0.0167, -0.0244, -0.0321, -0.0397],
        15000: [0.0064, -0.0002, -0.0068, -0.0134, -0.0197, -0.0259, -0.0321],
        25000: [0.0059, 0.0011, -0.0036, -0.0084, -0.0125, -0.0163, -0.0202],
        30000: [0.0057, 0.0015, -0.0027, -0.0070, -0.0102, -0.0133, -0.0163],
        40000: [0.0052, 0.0026, -0.0002, -0.0030, -0.0055, -0.0079, -0.0100],
        45000: [0.0049, 0.0027, -0.0001, -0.0028, -0.0045, -0.0061, -0.0076],
    },
}
thrust = {}
for bpr, rows in raw.items():
    heights = sorted(rows)
    cols = [[rows[h][j] for h in heights] for j in range(len(M_D))]
    thrust[bpr] = {h: [round(cols[j][i], 4) for j in range(len(M_D))] for i, h in enumerate(heights)}

fuel = json.load(open("fuel_readings.json"))  # {bpr: {feet: [M0.1 .. M0.9]}}

def fmt_row(feet, vals):
    f = "0.0" if feet == 0 else "{:_.1f}".format(float(feet))
    return "            (%s, [%s])," % (f, ", ".join("%.4f" % v for v in vals))

def emit(name, n, table):
    lines = ["pub(super) const %s: [Chart<%d>; 4] = [" % (name, n)]
    for bpr in sorted(table, key=float):
        lines += ["    Chart {", "        bpr: %.1f," % float(bpr), "        rows: &["]
        for feet in sorted(table[bpr], key=float):
            lines.append(fmt_row(float(feet), table[bpr][feet]))
        lines += ["        ],", "    },"]
    lines.append("];")
    return "\n".join(lines)

header = """// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The traced Jenkinson descent charts (data only; see the parent module for
//! the sources and the reading method). GENERATED by
//! `crates/alas-prop/data/jenkinson_idle_trace/gen_tables.py` from the page
//! readings and traces kept in that directory: edit them, not this file.

use super::Chart;

"""
text = header + emit("DESCENT_THRUST_CHARTS", 7, {str(k): {str(h): v for h, v in rows.items()} for k, rows in thrust.items()}) + "\n\n" \
    + emit("DESCENT_FUEL_CHARTS", 9, {str(k): {str(h): v for h, v in rows.items()} for k, rows in fuel.items()}) + "\n"
open(sys.argv[1], "w", encoding="utf-8", newline="\n").write(text)
json.dump({"thrust_mach": M_D, "thrust": {str(k): {str(h): v for h, v in r.items()} for k, r in thrust.items()},
           "fuel_mach": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9], "fuel": fuel}, open(sys.argv[2], "w"), indent=1)
print("ok")
