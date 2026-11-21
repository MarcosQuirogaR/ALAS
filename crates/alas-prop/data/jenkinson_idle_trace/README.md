# Jenkinson descent charts: trace provenance

Source: L. R. Jenkinson, P. Simpkin, D. Rhodes, *Civil Jet Aircraft Design*, Arnold 1999, Chapter 9,
Figs 9.18-9.21 panels (d) descent thrust `FN/FN*` and (f) descent fuel flow `lb/hr/FN*`
(printed pp. 204/205, 207/208, 210/211, 213/214; PDF pages 219/220, 222/223, 225/226, 228/229).
The scripts read the path of your copy of the book from the `JENKINSON_PDF` environment variable.

## Reproduce
`python run_all.py <dir>` renders each page at 4x, calibrates the axes on two printed ticks, erases the
text boxes and traces every curve column by column; it writes `traces/<spec>.json` (values at Mach 0..0.9)
and `overlay_<spec>.png` (traced curves over the page). `python gen_tables.py ../../src/empirical_turbofan/flight_idle/charts.rs tables.json`
rebuilds the Rust tables from the page readings in that script; `python check_tables.py <dir>` prints the
deviation of the tables from the traces and draws the table points over the pages (`table_<spec>.png`).
Requires pypdfium2, numpy, Pillow. The overlays are not committed; rerun the scripts to regenerate them.

## Calibration (render pixels at the stated scale = printed values)
| spec | PDF page | scale | y ticks | x ticks |
|---|---|---|---|---|
| d13 | 228 | 4x | y: 1564px=0.0, 1793px=-0.04 | x: 562px=0.2, 1252px=0.8 |
| d3 | 219 | 4x | y: 1250px=0.1, 1963px=-0.15 | x: 715px=0.2, 1528px=0.9 |
| d65 | 222 | 4x | y: 1288px=0.04, 1744px=-0.04 | x: 568px=0.2, 1248px=0.8 |
| d8 | 225 | 4x | y: 1512px=0.04, 1970px=-0.04 | x: 716px=0.2, 1166px=0.6 |
| f13 | 229 | 4x | y: 1500px=0.012, 1934px=0.004 | x: 725px=0.2, 1368px=0.8 |
| f3 | 220 | 4x | y: 1335px=0.05, 1875px=0.03 | x: 570px=0.2, 1245px=0.8 |
| f65 | 223 | 4x | y: 1350px=0.02, 2030px=0.005 | x: 710px=0.2, 1393px=0.8 |
| f8 | 226 | 4x | y: 1320px=0.016, 1798px=0.008 | x: 575px=0.2, 1205px=0.8 |

(d3/d65/d8/d13 = thrust charts BPR 3/6.5/8/13, f* = fuel charts; `_b` = the same with the zero axis line erased.)
Chart notes: the BPR 8 height legend prints "(10 000 ft)" (read 1000 ft); the lowest y label of the BPR 6.5 and 8
fuel charts prints 0.050 / 0.004 and is 0.005 by tick spacing.

## Page readings not taken from a trace column
Left ends (Mach ~0, read at the axis): BPR 8: SL 0.0641, 10k 0.0553, 15k 0.0505, 25k 0.0408, 30k 0.0351, 40k 0.0284,
45k 0.0230. BPR 13: SL 0.0249, 10k 0.0218, 15k 0.0197, 25k 0.0152, 30k 0.0134, 40k 0.0090, 45k 0.0067.
BPR 3: 45k 0.0929, 40k 0.0890, 30k 0.0403, then SL 0.0354, 10k 0.0259, 20k 0.0124 (the three curves that meet at P).
Common crossing points P (Mach, FN/FN*): BPR 6.5 (0.20, 0.0098); BPR 13 (0.225, 0.0047); BPR 3 SL/10k/20k (0.265, 0.0);
BPR 8 (0.23, 0.0231) for the SL, 10,000 and 15,000 ft curves only. The Mach 0.2 node is the straight line from the left
end to P; the Mach 0.3 (and, for BPR 6.5 and 13, 0.4) node is the straight line from P to the traced Mach 0.5 value
(BPR 8: to the traced Mach 0.4 value).

**BPR 8, 25,000-45,000 ft (corrected in the second audit round).** These four curves do not pass through P: a column
read of the page (`columns.py`, output `columns_d8.json`) resolves them at Mach 0.2-0.3, where they fan out below the
SL/10k/15k bundle (one 19 px wide run at 0.0271, three curves). At Mach 0.2 the runs are 0.0220 (25k), 0.0197 (30k),
0.0164 (40k), 0.0134 (45k); at Mach 0.225 they are 0.0197, 0.0179, 0.0148, 0.0122; at Mach 0.3 the 25k curve is inside the
four-curve bundle at 0.0171 +/- 0.0017 (taken 0.0161 on its own trend from 0.0172 at Mach 0.275) and the others are 0.0146
(30k), 0.0123 (40k), 0.0101 (45k). The earlier construction (all seven curves through P, 0.0247/0.0238/0.0231 at Mach
0.2 and 0.0176 at Mach 0.3) overstated these nodes by 0.004-0.012. At Mach 0.4 the column read has a single-width run at 0.0066 that nothing in the earlier table matched; it
is the 45k curve (0.0101 at Mach 0.30, 0.0092 at 0.35, 0.0051 merged at 0.5), so the 45k node is 0.0066 and the 25k-40k
curves stay merged at 0.0096. From Mach 0.5 the 30-45k curves share the traced bundle value.

Where the page prints a flat segment (BPR 3 sea level, Mach 0.70 to its end at 0.82) the table holds it.
FN* is each engine's maximum sea-level static thrust (Table 9.4, printed p. 202: 15,100 / 95,000 / 99,300 / 97,979 lb for
BPR 3.0 / 6.5 / 8.0 / 13.0), so all four charts are on the same definition. The BPR 3 chart nevertheless prints larger net
drag than the other three (-0.109 against -0.039 to -0.059 at sea level and Mach 0.8); the text gives no reason, the
table reproduces the page, and that chart enters only through the interpolation below BPR 6.5.

No height-monotone projection is applied (the earlier version pooled adjacent heights at Mach >= 0.4): where the net
thrust is positive the gross thrust falls with density as well, so no height trend is implied; where it is negative the ram
drag falls with density and the traced values are already monotone (the test `the_traced_charts_obey_the_physical_invariants`
checks it on the negative cells only).

Deviation of the tables from the page: `check_tables.py` reports the worst deviation from the traced nodes at Mach >= 0.5
(0.0026, BPR 8, 25k) and, for the BPR 8 curves at Mach 0.2-0.3, from the column reads above (resolved curves only, Mach 0.2-0.4).
