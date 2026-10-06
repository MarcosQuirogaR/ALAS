"""Model-versus-published validation figures for the ALAS website and docs.

Error convention everywhere: (model - published) / published, in percent.
Positive means the model is above the published value.

Two stages
----------
1. extract (needs a model run; writes site/scripts/validation_data.json):

     cargo run --profile test -p alas-pipeline --example model_reference_dump \
         -- out/validation/MODEL.json
     node tools/aircraft_parity.cjs --model out/validation/MODEL.json \
         --contract golden/aircraft/real_aircraft_parity.json \
         --out out/validation/parity --report out/validation/parity.html
     uv run --with numpy python site/scripts/validation_figures.py extract \
         out/validation/MODEL.json out/validation/parity/AIRCRAFT_PARITY.json

2. plot (needs only validation_data.json; writes the PNG pairs, the landing
   copies and manifest.json):

     uv run --with matplotlib --with numpy python site/scripts/validation_figures.py

Data provenance
---------------
* Model values: the registered preset design vectors run through the current
  native pipeline (no optimisation, no per-aircraft tuning multiplier).
* OEW references: the source registry crates/alas-config/src/oew_reference
  (values copied into OEW_REFERENCES below with their source class).
* Range, field-length and L/D references: golden/aircraft/real_aircraft_parity.json
  as evaluated by tools/aircraft_parity.cjs. These are chart readings or
  estimates (secondary), kept as diagnostics by that harness.
* Scored/diagnostic counts and the registered-input split come straight from
  the harness summary.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SITE = HERE.parent
DATA = HERE / "validation_data.json"
DOCS_ASSETS = SITE / "docs-site" / "docs" / "assets"
LANDING = SITE / "public" / "validation"

PRESET_ORDER = [
    "A220-300", "A320-200", "A340-300", "A380-800", "B787-9", "B747-400",
    "DC-10", "ATR72-600", "E195-E2", "C919", "A400M", "AVE",
]

# class: M = manufacturer document, O = operator record or derived value,
# S = secondary compilation or aggregator.
# value_kg, uncertainty_kg (as stated in the registry), class, source note.
OEW_REFERENCES = {
    "A220-300": (37149.0, 500.0, "M", "Airbus A220 Aircraft Recovery Publication, planning OEW (140-seat cabin)"),
    "A320-200": (41052.0, 1500.0, "O", "operator sheet, 77 t MTOW, 180 seats, wingtip fences"),
    "A340-300": (131215.0, 2000.0, "M", "Airbus ACAP jacking-dimensions figure, configuration not stated"),
    "A380-800": (277000.0, 8000.0, "S", "aggregator value, typical 3-class cabin; published values span 270-285 t"),
    "B787-9": (128850.0, 3000.0, "O", "superseded Boeing ACAP page (typical 290 seats), attribution not re-verified"),
    "B747-400": (178755.0, 2000.0, "M", "Boeing ACAP specification OEW, 3-class 400 seats"),
    "DC-10": (120914.0, 2000.0, "M", "McDonnell Douglas ACAP, Series 30 passenger column"),
    "ATR72-600": (13450.0, 440.0, "M", "ATR factsheet, typical in-service operational empty weight"),
    "E195-E2": (35700.0, 500.0, "O", "EASA MZFW minus Embraer maximum payload (derived)"),
    "C919": (45700.0, 2000.0, "S", "public specification table (secondary, inclusion list not stated)"),
    "A400M": (78600.0, 2100.0, "O", "operator page, empty-weight definition not stated"),
}
CLASS_LABEL = {
    "M": "Manufacturer document",
    "O": "Operator record or derived",
    "S": "Secondary or aggregator",
}
# Presets whose residual is understood or whose reference is weak.
FLAGS = {
    "B747-400": "known OEW over-prediction",
    "C919": "secondary-source reference",
    "A400M": "military airlifter, operator-page reference",
    "A380-800": "aggregator reference",
}

RANGE_IDS = [
    ("performance.range_max_payload_nmi_chart", "Range at max payload"),
    ("performance.range_mtow_max_fuel_nmi_chart", "Range at MTOW, max fuel"),
    ("performance.ferry_range_nmi_chart", "Ferry range"),
]
FIELD_IDS = [
    ("performance.tofl_isa_sl_mtow_m_chart", "Take-off field length, MTOW"),
    ("performance.lfl_isa_sl_mlw_m_chart", "Landing field length, MLW"),
]
LD_ID = "aero.l_over_d_cruise"
SUMMARY_COLUMNS = [
    ("oew", "OEW"),
    ("performance.range_max_payload_nmi_chart", "Range\nmax payload"),
    ("performance.range_mtow_max_fuel_nmi_chart", "Range\nMTOW, fuel"),
    ("performance.ferry_range_nmi_chart", "Ferry\nrange"),
    ("performance.range_advertised_payload_nmi", "Range, advert.\npayload"),
    ("performance.tofl_isa_sl_mtow_m_chart", "TOFL\nMTOW"),
    ("performance.lfl_isa_sl_mlw_m_chart", "LFL\nMLW"),
    (LD_ID, "Cruise\nL/D"),
]


LABEL_OFFSETS = {"A220-300": (-6, -13), "A320-200": (-6, 8), "C919": (8, -12), "E195-E2": (-6, -13),
                 "A340-300": (-6, -14), "B787-9": (-6, -14), "DC-10": (-6, 8), "B747-400": (-6, 8),
                 "A380-800": (-8, -14), "A400M": (8, -8), "ATR72-600": (8, -4)}
LABEL_HA = {"A220-300": "right", "A320-200": "right", "E195-E2": "right", "A340-300": "right",
            "B787-9": "right", "DC-10": "right", "B747-400": "right", "A380-800": "right"}


def fmt(v):
    return f"{v:+.2f}" if abs(v) < 0.1 else f"{v:+.1f}"


def err_pct(model: float, ref: float) -> float:
    return 100.0 * (model - ref) / ref


# ---------------------------------------------------------------- extract

ID_RANGE_ADV = "performance.range_advertised_payload_nmi"
LB_KG = 0.45359237

R_MAXP = "performance.range_max_payload_nmi_chart"
R_MTOW = "performance.range_mtow_max_fuel_nmi_chart"
R_FERRY = "performance.ferry_range_nmi_chart"
F_TOFL = "performance.tofl_isa_sl_mtow_m_chart"
F_LFL = "performance.lfl_isa_sl_mlw_m_chart"

# Manufacturer-document values (extracted from the PDFs by the coordinator) and other
# references that are not in, or that supersede, the parity contract. An entry with the
# same (preset, id) as a contract row replaces it.
# Each entry: (preset, id, published, unit, uncertainty, quality, mark, source, condition)
# quality: chart = manufacturer planning chart reading; table = manufacturer table;
#   brochure = manufacturer brochure table, ISA and reserves unstated; secondary = public
#   compilation; extrapolated = continuation of a printed chart line beyond its axis.
# mark: "" = manufacturer primary with stated weights; "dagger" = advertised, conditions
#   unstated or secondary; "extrap" = extrapolated, shown hatched and excluded from statistics.
EXTRA_REFERENCES = [
    # A220-300, Airbus A220 ACP Issue 001 DM 13AAB (PW1521G-3, 149,000 lb MTOW)
    ("A220-300", R_MAXP, 1991.0, "nmi", 10.0, "chart", "",
     "Airbus A220 ACP Issue 001, DM 13AAB Fig.1 (ZFW vs range, ISA)",
     "ZFW 123,000 lb, MTOW 149,000 lb, ISA, still air; reserves not stated"),
    ("A220-300", R_MTOW, 3365.0, "nmi", 20.0, "chart", "",
     "Airbus A220 ACP Issue 001, DM 13AAB Fig.1",
     "MTOW 149,000 lb meets max fuel 17,727 kg (ZFW 110.2 klb), ISA"),
    ("A220-300", R_FERRY, 4140.0, "nmi", 100.0, "extrapolated", "extrap",
     "Airbus A220 ACP Issue 001 Fig.1, fuel line extrapolated to OEW",
     "chart stops at ZFW 96 klb; value is a linear extrapolation, not printed"),
    ("A220-300", F_TOFL, 2755.0, "m", 20.0, "chart", "",
     "Airbus A220 ACP Issue 001, DM 13AAB Fig.2 (take-off field length, ISA, PW1521G)",
     "SL ISA; printed 2,719 m at 148.2 klb, last segment extrapolated 0.8 klb to the 149,000 lb MTOW; flaps not stated"),
    ("A220-300", F_LFL, 1512.0, "m", 3.0, "chart", "",
     "Airbus A220 ACP Issue 001, DM 13AAB Fig.6 (landing field length, dry)",
     "MLW 129,500 lb, SL, dry runway; ISA and factoring not stated"),
    # B787-9, Boeing D6-58333 Rev O (raster charts)
    ("B787-9", R_MAXP, 5320.0, "nmi", 50.0, "chart", "",
     "Boeing 787 ACAP D6-58333 Rev O sec 3.2.2 p3-3 (payload/range, M0.85)",
     "ZFW 400,000 lb; printed 5,280 nmi at 560,000 lb, adjusted to 561,500 lb; raster reading"),
    ("B787-9", R_MTOW, 8215.0, "nmi", 30.0, "chart", "",
     "Boeing 787 ACAP D6-58333 Rev O sec 3.2.2 p3-3",
     "560,000 lb brake release meets fuel capacity 223,646 lb (ZFW 336.4 klb); raster reading"),
    ("B787-9", R_FERRY, 9330.0, "nmi", 40.0, "extrapolated", "extrap",
     "Boeing 787 ACAP D6-58333 Rev O sec 3.2.2, fuel line read at the model OEW",
     "chart has no zero-payload point; depends on the model OEW (128,392 kg)"),
    # B747-400, Boeing D6-58326-1 Rev F (CF6-80C2B1F; weights match the preset)
    ("B747-400", R_MAXP, 5715.0, "nmi", 20.0, "chart", "",
     "Boeing 747-400 ACAP D6-58326-1 Rev F sec 3.2.1 p3-2 (payload/range, M0.85, CF6-80C2B1F)",
     "ZFW 542,500 lb, brake release 875,000 lb, std day; FAR international reserves, 10 % trip allowance, 200 nmi alternate, 30 min hold"),
    ("B747-400", R_MTOW, 7051.0, "nmi", 25.0, "chart", "",
     "Boeing 747-400 ACAP D6-58326-1 Rev F sec 3.2.1 p3-2",
     "MTOW 875,000 lb meets fuel limit 382,336 lb (ZFW 490.2 klb); same reserves"),
    ("B747-400", R_FERRY, 8190.0, "nmi", 100.0, "extrapolated", "extrap",
     "Boeing 747-400 ACAP D6-58326-1 Rev F sec 3.2.1, max-fuel line extrapolated",
     "chart axis stops at ZFW 400 klb; extrapolated to OEW 394 klb, not printed"),
    ("B747-400", F_TOFL, 3219.0, "m", 6.0, "chart", "",
     "Boeing 747-400 ACAP D6-58326-1 Rev F sec 3.3.1 p3-13 (take-off field length, CF6-80C2B1)",
     "875,000 lb, SL, standard day, flaps 20, zero wind and slope"),
    ("B747-400", F_LFL, 2052.0, "m", 5.0, "chart", "",
     "Boeing 747-400 ACAP D6-58326-1 Rev F sec 3.4.2 p3-36 (landing field length, flaps 30)",
     "630,000 lb, SL, dry runway, zero wind; flaps 25 chart gives 2,230 m"),
    # E195-E2, Embraer specification sheet (weights match the preset)
    ("E195-E2", F_TOFL, 1775.0, "m", None, "table", "",
     "Embraer E195-E2 specification sheet p.2 (take-off field length)",
     "MTOW 62,500 kg, ISA, SL"),
    ("E195-E2", F_LFL, 1290.0, "m", None, "table", "",
     "Embraer E195-E2 specification sheet p.2 (landing field length)",
     "MLW 54,000 kg, ISA, SL; factoring not stated"),
    # C919, secondary
    ("C919", F_TOFL, 2052.0, "m", None, "secondary", "dagger",
     "public specification table citing COMAC (C919-100 STD, take-off at MTOW, ISA)",
     "secondary compilation; flap setting and runway conditions not stated"),
    # A400M, Airbus Defence and Space brochure (ISA and reserves not stated)
    ("A400M", R_MAXP, 1780.0, "nmi", None, "brochure", "dagger",
     "Airbus A400M brochure p.13 (range with 37 t, 3,300 km)",
     "MTOW 141,000 kg; ISA and reserves not stated"),
    ("A400M", R_FERRY, 4800.0, "nmi", None, "brochure", "dagger",
     "Airbus A400M brochure p.13 (ferry range 8,900 km)",
     "internal fuel 50,800 kg stated (preset usable 48,880 kg); ISA and reserves not stated"),
]

# Range at an advertised payload: (preset, payload_kg, nmi, quality, source, condition)
ADVERTISED_POINTS = {
    "ATR72-600": (None, 758.0, "ATR 72-600 factsheet (PW127M/N, 2020) p.2, range with 72 passengers",
                  "payload = model planning cabin (72 seats); reserves not stated"),
    "E195-E2": (None, 3000.0, "Embraer E195-E2 specification sheet, April 2025, p.1, range full PAX",
                "LRC, typical reserves, 100 nmi alternate; payload = model planning cabin (132 seats)"),
    "C919": (None, 2200.0, "COMAC basic range as quoted in the public specification table (secondary)",
             "standard-range variant; cabin and reserves not stated; payload = 158 seats at the model per-seat mass"),
    "A400M": (20000.0, 3400.0, "Airbus Defence and Space A400M brochure TMMA0026/01/2025 p.24 (20 t, 6,300 km)",
              "reserves, altitude and speed not stated"),
}

CANONICAL = {
    "performance.tofl_isa_sl_mtow_m": "performance.tofl_isa_sl_mtow_m_chart",
    "performance.lfl_isa_sl_mlw_m": "performance.lfl_isa_sl_mlw_m_chart",
}
MODEL_FIELD = {
    "performance.tofl_isa_sl_mtow_m_chart": ("performance", "takeoff_field_length_isa_sl_mtow_m"),
    "performance.lfl_isa_sl_mlw_m_chart": ("performance", "landing_field_length_isa_sl_mlw_m"),
    LD_ID: ("aerodynamics", "l_over_d_cruise"),
}


def corner_value(model, preset, rid):
    c = model[preset]["payload_range"]["corners"]
    idx = {"performance.range_max_payload_nmi_chart": 1,
           "performance.range_mtow_max_fuel_nmi_chart": 2,
           "performance.ferry_range_nmi_chart": 3}[rid]
    return c[idx]["range_nm"]


def range_at_payload(model, preset, payload_kg):
    """Still-air range on the model's own payload-range line (corners B, C, D)."""
    c = model[preset]["payload_range"]["corners"][1:]
    pts = [(k["payload_kg"], k["range_nm"]) for k in c]  # B, C, D: payload decreasing
    if payload_kg >= pts[0][0]:
        return pts[0][1]
    for (p0, r0), (p1, r1) in zip(pts, pts[1:]):
        if p1 <= payload_kg <= p0:
            return r1 + (r0 - r1) * (payload_kg - p1) / (p0 - p1)
    return pts[-1][1]


def model_value(model, preset, rid):
    if rid in MODEL_FIELD:
        a, b = MODEL_FIELD[rid]
        return model[preset][a][b]
    return corner_value(model, preset, rid)


def extract(model_path: Path, parity_path: Path) -> None:
    model = json.loads(model_path.read_text(encoding="utf-8"))
    parity = json.loads(parity_path.read_text(encoding="utf-8"))
    out = {
        "version": "ALAS 1.3.2 (pre-release working tree, 2026-10-06)",
        "error_convention": "(model - published) / published",
        "oew": {},
        "rows": [],
        "summary": parity["summary"],
        "model_mtow_kg": {k: v["mass"]["mtow_kg"] for k, v in model.items()},
        "model_mlw_kg": {k: v["mass"]["mlw_kg"] for k, v in model.items()},
    }
    for preset, (ref, unc, cls, note) in OEW_REFERENCES.items():
        m = model[preset]["mass"]["oew_kg"]
        out["oew"][preset] = {
            "model_kg": m, "published_kg": ref, "uncertainty_kg": unc,
            "class": cls, "source": note, "error_pct": err_pct(m, ref),
        }
    wanted = {i for i, _ in RANGE_IDS + FIELD_IDS} | {LD_ID} | set(CANONICAL)
    for r in parity["rows"]:
        if r["id"] in wanted and r["source_value"] is not None and r["model_value"] is not None:
            rid = CANONICAL.get(r["id"], r["id"])
            mval = r["model_value"]
            note = ""
            ev = {"primary": "brochure"}.get(r["evidence"], r["evidence"])
            if ev == "secondary" and rid != LD_ID:
                ev = "chart"
            weak = False
            out["rows"].append({
                "preset": r["preset"], "id": rid, "unit": r["unit"],
                "model": mval, "published": r["source_value"],
                "error_pct": err_pct(mval, r["source_value"]),
                "evidence": ev, "condition": r["condition"], "weak": weak, "mark": "", "note": note,
                "source": "golden/aircraft/real_aircraft_parity.json",
            })
    superseded = {(e[0], e[1]) for e in EXTRA_REFERENCES}
    out["rows"] = [r for r in out["rows"] if (r["preset"], r["id"]) not in superseded]
    for preset, rid, val, unit, unc, quality, mark, source, cond in EXTRA_REFERENCES:
        mval = model_value(model, preset, rid)
        out["rows"].append({
            "preset": preset, "id": rid, "unit": unit, "model": mval, "published": val,
            "uncertainty": unc, "error_pct": err_pct(mval, val), "evidence": quality,
            "condition": cond, "weak": mark != "", "mark": mark, "note": "", "source": source,
        })
    for preset, (payload, nmi, source, cond) in ADVERTISED_POINTS.items():
        cab = model[preset]["cabin_payload"]
        if payload is None:
            seats = 158 if preset == "C919" else cab["seats_modelled"]
            per_seat = cab["payload_total_kg"] / cab["seats_modelled"]
            payload = seats * per_seat
        mval = range_at_payload(model, preset, payload)
        out["rows"].append({
            "preset": preset, "id": ID_RANGE_ADV, "unit": "nmi", "model": mval, "published": nmi,
            "error_pct": err_pct(mval, nmi), "evidence": "advertised", "condition": cond,
            "weak": True, "mark": "dagger", "note": f"payload {payload:.0f} kg; model range interpolated on its own corner line",
            "source": source, "payload_kg": payload,
        })
    # conditions under which the model values are produced (identical for all presets)
    out["model_conditions"] = {
        "takeoff": model["A320-200"]["performance"]["takeoff_field_length_basis"],
        "landing": model["A320-200"]["performance"]["landing_field_length_basis"],
        "environment": model["A320-200"]["performance"]["condition"],
        "payload_range": model["A320-200"]["payload_range"]["method"],
    }
    # columns with no reference, per preset
    cols = [c for c, _ in SUMMARY_COLUMNS]
    got = {(r["preset"], r["id"]) for r in out["rows"]} | {(p, "oew") for p in out["oew"]}
    out["missing"] = {p: [c for c in cols if (p, c) not in got]
                      for p in PRESET_ORDER if p != "AVE"}
    # scored-comparison split per preset: registered input vs model output
    split = {}
    for r in parity["rows"]:
        p = r["preset"]
        s = split.setdefault(p, {"input_within": 0, "input_out": 0, "output_within": 0,
                                 "output_out": 0, "diagnostic": 0, "no_reference": 0})
        kind = "input" if r.get("model_provenance_note") else "output"
        if r["status"] == "within_tolerance":
            s[f"{kind}_within"] += 1
        elif r["status"] == "out_of_tolerance":
            s[f"{kind}_out"] += 1
        elif r["status"] == "diagnostic":
            s["diagnostic"] += 1
        else:
            s["no_reference"] += 1
    out["split"] = split
    DATA.write_text(json.dumps(out, indent=1), encoding="utf-8")
    print("wrote", DATA)


# ------------------------------------------------------------------- plot

THEMES = {
    "dark": dict(bg="#1e1e1e", fg="#e6e6e6", grid="#3c3c3c", dim="#9aa0a6",
                 band5="#3fb95030", band10="#d2992230",
                 M="#4aa3df", O="#f0a040", S="#b48ad9", pos="#e8776b", neg="#4aa3df",
                 ok="#2d4a33", mid="#5a4a22", bad="#5c2b27"),
}


def style(plt, t):
    plt.rcParams.update({
        "figure.facecolor": t["bg"], "axes.facecolor": t["bg"], "savefig.facecolor": t["bg"],
        "axes.edgecolor": t["grid"], "axes.labelcolor": t["fg"], "text.color": t["fg"],
        "xtick.color": t["fg"], "ytick.color": t["fg"], "axes.titlecolor": t["fg"],
        "grid.color": t["grid"], "grid.linewidth": 0.6, "font.size": 11,
        "axes.titlesize": 13, "axes.labelsize": 11.5, "legend.fontsize": 10,
        "legend.facecolor": t["bg"], "legend.edgecolor": t["grid"],
        "axes.spines.top": False, "axes.spines.right": False,
    })


def bands(ax, t, orient="x", limit=(-100, 100)):
    span = ax.axvspan if orient == "x" else ax.axhspan
    span(-10, 10, color=t["band10"], lw=0, zorder=0)
    span(-5, 5, color=t["band5"], lw=0, zorder=0)


def save(fig, name, theme, dpi=160):
    DOCS_ASSETS.mkdir(parents=True, exist_ok=True)
    path = DOCS_ASSETS / f"validation-{name}-{theme}.png"
    fig.savefig(path, dpi=dpi)
    return path


def fig_oew(plt, np, d, t):
    from matplotlib.patches import Patch
    rows = sorted(d["oew"].items(), key=lambda kv: kv[1]["error_pct"])
    fig, (a1, a2) = plt.subplots(1, 2, figsize=(11, 5.4), gridspec_kw={"width_ratios": [1, 1.15]})
    # parity
    lo, hi = 12, 300
    xs = np.array([lo, hi])
    a1.fill_between(xs, xs * 0.90, xs * 1.10, color=t["band10"], lw=0)
    a1.fill_between(xs, xs * 0.95, xs * 1.05, color=t["band5"], lw=0)
    a1.plot(xs, xs, color=t["dim"], lw=1)
    for p, v in d["oew"].items():
        x, y = v["published_kg"] / 1e3, v["model_kg"] / 1e3
        a1.errorbar(x, y, xerr=v["uncertainty_kg"] / 1e3, fmt="o", ms=6, color=t[v["class"]],
                    ecolor=t[v["class"]], elinewidth=1, capsize=0)
        if abs(v["error_pct"]) < 5.0:
            continue  # within the band: labelled in the signed-error panel
        a1.annotate(p, (x, y), textcoords="offset points", xytext=LABEL_OFFSETS.get(p, (6, 4)),
                    fontsize=8.5, color=t["fg"], ha=LABEL_HA.get(p, "left"))
    a1.set_xscale("log"); a1.set_yscale("log")
    a1.set_xlim(lo, hi); a1.set_ylim(lo, hi)
    a1.set_xticks([20, 50, 100, 200]); a1.set_xticklabels(["20", "50", "100", "200"])
    a1.set_yticks([20, 50, 100, 200]); a1.set_yticklabels(["20", "50", "100", "200"])
    a1.minorticks_off()
    a1.set_xlabel("Published operating empty mass [t]")
    a1.set_ylabel("ALAS operating empty mass [t]")
    a1.set_title("Model vs published (outliers labelled)", loc="left")
    a1.grid(True, which="major")
    # signed error
    names = [k for k, _ in rows]
    errs = [v["error_pct"] for _, v in rows]
    y = np.arange(len(rows))
    a2.axvspan(-10, 10, color=t["band10"], lw=0, zorder=0)
    a2.axvspan(-5, 5, color=t["band5"], lw=0, zorder=0)
    a2.barh(y, errs, color=[t[v["class"]] for _, v in rows], height=0.62, zorder=2)
    for i, (k, v) in enumerate(rows):
        u = 100 * v["uncertainty_kg"] / v["published_kg"]
        a2.plot([v["error_pct"] - u, v["error_pct"] + u], [i, i], color=t["fg"], lw=0.8, alpha=0.55, zorder=3)
        e = v["error_pct"]
        edge = max(e, 0) + u if e >= 0 else min(e, 0) - u
        a2.text(edge + (0.5 if e >= 0 else -0.5), i, fmt(e) + "%",
                va="center", ha="left" if e >= 0 else "right", fontsize=9.5, zorder=4)
    a2.axvline(0, color=t["fg"], lw=0.8, zorder=3)
    a2.set_yticks(y)
    a2.set_yticklabels([n + ("*" if n in FLAGS else "") for n in names])
    a2.set_xlim(-22, 24)
    a2.set_xlabel("Signed error (model - published) / published [%]")
    a2.set_title("Signed error", loc="left")
    a2.grid(True, axis="x")
    handles = [Patch(color=t[c], label=CLASS_LABEL[c]) for c in "MOS"]
    handles.append(Patch(color=t["band5"], label="+/-5 %"))
    handles.append(Patch(color=t["band10"], label="+/-10 %"))
    fig.legend(handles=handles, loc="lower center", ncol=5, frameon=False, fontsize=9.5)
    fig.text(0.5, 0.075, "Whiskers: stated uncertainty of the published value.  * known residual or weak reference "
             "(B747-400 over-predicted; C919, A380, A400M references are secondary).",
             ha="center", fontsize=8.8, color=t["dim"])
    fig.suptitle("Operating empty mass, 11 aircraft", x=0.02, ha="left", fontsize=14.5)
    fig.tight_layout(rect=(0, 0.11, 1, 0.97))
    return fig


def grouped(plt, np, d, t, ids, title, ylab, colors, lim, figsize=(10.5, 5.2)):
    rows = d["rows"]
    presets = [p for p in PRESET_ORDER if any(r["preset"] == p and r["id"] in {i for i, _ in ids} for r in rows)]
    fig, ax = plt.subplots(figsize=figsize)
    n = len(ids)
    w = 0.84 / n
    x = np.arange(len(presets))
    ax.axhspan(-10, 10, color=t["band10"], lw=0, zorder=0)
    ax.axhspan(-5, 5, color=t["band5"], lw=0, zorder=0)
    top = lim[1]
    for j, (i, label) in enumerate(ids):
        vals, weak, hatch = [], [], []
        for p in presets:
            m = [r for r in rows if r["preset"] == p and r["id"] == i]
            vals.append(m[0]["error_pct"] if m else np.nan)
            weak.append(m[0].get("mark", "") if m else "")
        pos = x - 0.42 + w * (j + 0.5)
        bars = ax.bar(pos, vals, w * 0.92, color=colors[j], label=label, zorder=2)
        for bar, wk in zip(bars, weak):
            if wk == "extrap":
                bar.set_hatch("///")
                bar.set_edgecolor(t["bg"])
                bar.set_alpha(0.6)
        for px, v, wk in zip(pos, vals, weak):
            if not np.isfinite(v):
                continue
            txt = f"{v:+.0f}" + {"dagger": "†", "extrap": "~"}.get(wk, "")
            if v > top:
                ax.text(px, top - 0.8, txt + " (off scale)", ha="center", va="top", fontsize=8, rotation=90,
                        color=t["bg"], zorder=4)
            else:
                ax.text(px, v + (0.8 if v >= 0 else -0.8), txt, ha="center",
                        va="bottom" if v >= 0 else "top", fontsize=8, zorder=4)
    ax.axhline(0, color=t["fg"], lw=0.8, zorder=3)
    ax.set_xticks(x); ax.set_xticklabels(presets)
    ax.set_ylim(*lim)
    ax.set_ylabel(ylab)
    ax.set_title(title, loc="left", fontsize=13.5)
    ax.grid(True, axis="y")
    ax.legend(loc="upper left", ncol=n, frameon=False, fontsize=9.5)
    return fig, ax


def fig_range(plt, np, d, t):
    ids = RANGE_IDS + [(ID_RANGE_ADV, "Range at advertised payload")]
    fig, ax = grouped(plt, np, d, t, ids, "Range vs published values",
                      "Signed error (model - published) / published [%]",
                      [t["M"], t["O"], t["S"], "#6fcf97"], (-20, 32), figsize=(13, 5.4))
    fig.text(0.01, 0.01, "Corners: readings from manufacturer payload-range charts or tables. Model: reserve-inclusive plan (EASA basic scheme, 200 nmi alternate), still air.\n"
             "Chart reserve rules and weight variants are not always stated.  † advertised or brochure value (ISA, reserves or payload unstated).\n"
             "~ hatched: extrapolated, not printed, excluded from statistics.  Advertised point = model range interpolated at the published payload.",
             fontsize=8.4, color=t["dim"], ha="left")
    fig.tight_layout(rect=(0, 0.11, 1, 1))
    return fig


def fig_field(plt, np, d, t):
    fig, ax = grouped(plt, np, d, t, FIELD_IDS, "ISA sea-level field lengths vs published values",
                      "Signed error (model - published) / published [%]", [t["M"], t["O"]], (-24, 62),
                      figsize=(11.5, 5.2))
    fig.text(0.01, 0.01, "Model: dry level runway, zero wind, ISA sea level; take-off at MTOW (Raymer correlation), landing at MLW (unfactored distance / 0.6).\n"
             "Published: manufacturer chart readings and tables; flap setting, ISA and factoring are not stated on every one.  † secondary value (C919).",
             fontsize=8.4, color=t["dim"], ha="left")
    fig.tight_layout(rect=(0, 0.08, 1, 1))
    return fig


def fig_summary(plt, np, d, t):
    lookup = {(r["preset"], r["id"]): r for r in d["rows"]}
    for p, v in d["oew"].items():
        lookup[(p, "oew")] = {"error_pct": v["error_pct"], "mark": "dagger" if v["class"] == "S" else ""}
    presets = [p for p in PRESET_ORDER if p != "AVE"]
    cols = SUMMARY_COLUMNS
    fig, ax = plt.subplots(figsize=(12.5, 6.4))
    ax.set_xlim(0, len(cols)); ax.set_ylim(len(presets), 0)
    for i, p in enumerate(presets):
        for j, (cid, _) in enumerate(cols):
            r = lookup.get((p, cid))
            if r is None:
                ax.text(j + 0.5, i + 0.5, "n/a", ha="center", va="center", color=t["dim"], fontsize=9)
                continue
            v = r["error_pct"]
            a = abs(v)
            c = t["ok"] if a <= 5 else t["mid"] if a <= 10 else t["bad"]
            mk = r.get("mark", "")
            ax.add_patch(plt.Rectangle((j, i), 1, 1, facecolor=c, edgecolor=t["bg"] if mk != "extrap" else t["dim"],
                                       lw=2 if mk != "extrap" else 1, hatch="///" if mk == "extrap" else None))
            ax.text(j + 0.5, i + 0.5, fmt(v) + {"dagger": "†", "extrap": "~"}.get(mk, ""), ha="center", va="center", fontsize=10.5)
    ax.set_xticks(np.arange(len(cols)) + 0.5)
    ax.set_xticklabels([c[1] for c in cols], fontsize=10)
    ax.xaxis.tick_top()
    ax.set_yticks(np.arange(len(presets)) + 0.5)
    ax.set_yticklabels([p + ("*" if p in FLAGS else "") for p in presets])
    ax.tick_params(length=0)
    for s in ax.spines.values():
        s.set_visible(False)
    from matplotlib.patches import Patch
    fig.legend(handles=[Patch(color=t["ok"], label="|error| <= 5 %"), Patch(color=t["mid"], label="5-10 %"),
                        Patch(color=t["bad"], label="> 10 %")], loc="lower center", ncol=3, frameon=False)
    fig.text(0.5, 0.075, "Signed error (model - published) / published [%]; n/a = no reference found.  * known residual or weak OEW reference.  "
             "† secondary, brochure or advertised value.  ~ hatched: extrapolated, excluded from statistics.\n"
             "OEW: manufacturer, operator or secondary records. Ranges, field lengths: chart readings and published tables. "
             "Cruise L/D: published estimates. All diagnostic.",
             ha="center", fontsize=8.4, color=t["dim"])
    fig.suptitle("Summary: error by quantity and aircraft", x=0.02, ha="left", fontsize=14.5)
    fig.tight_layout(rect=(0, 0.12, 1, 0.96))
    return fig


def fig_scope(plt, np, d, t):
    split = d["split"]
    presets = [p for p in PRESET_ORDER if p in split and p != "AVE" and split[p]["input_within"] + split[p]["input_out"] + split[p]["output_within"] + split[p]["output_out"] > 0]
    keys = [("input_within", "Registered input reproduced (data entry check)", t["dim"]),
            ("input_out", "Registered input, out of tolerance", t["pos"]),
            ("output_within", "Model output within tolerance", t["ok"] if False else "#3fa34d"),
            ("output_out", "Model output out of tolerance", "#d9534f")]
    fig, ax = plt.subplots(figsize=(10.5, 4.9))
    y = np.arange(len(presets))
    left = np.zeros(len(presets))
    for k, label, c in keys:
        v = np.array([split[p][k] for p in presets], dtype=float)
        ax.barh(y, v, left=left, color=c, label=label, height=0.62)
        for i, (l, w) in enumerate(zip(left, v)):
            if w >= 3:
                ax.text(l + w / 2, i, f"{int(w)}", ha="center", va="center", fontsize=9, color="#ffffff" if c != t["dim"] else t["bg"])
        left += v
    ax.set_yticks(y); ax.set_yticklabels(presets); ax.invert_yaxis()
    ax.set_xlabel("Scored rows (primary source, matching conditions) [count]")
    ax.legend(loc="lower right", frameon=False, fontsize=9.5)
    ax.set_axisbelow(True)
    ax.xaxis.set_major_locator(plt.MaxNLocator(integer=True))
    ax.grid(True, axis="x")
    ax.set_title("Scored comparisons: registered inputs vs model outputs", loc="left", fontsize=13.5)
    n_diag = sum(split[p]["diagnostic"] for p in split)
    fig.text(0.01, 0.01, f"{n_diag} further rows (secondary sources or non-matching conditions) are diagnostics and are not scored.", fontsize=8.6, color=t["dim"])
    fig.tight_layout(rect=(0, 0.04, 1, 1))
    return fig


def build():
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import numpy as np
    d = json.loads(DATA.read_text(encoding="utf-8"))
    makers = {"oew": fig_oew, "summary": fig_summary, "payload-range": fig_range,
              "field": fig_field, "scope": fig_scope}
    for theme, t in THEMES.items():
        style(plt, t)
        for name, fn in makers.items():
            fig = fn(plt, np, d, t)
            save(fig, name, theme)
            plt.close(fig)
    LANDING.mkdir(parents=True, exist_ok=True)
    import shutil
    for name in makers:
        shutil.copyfile(DOCS_ASSETS / f"validation-{name}-dark.png", LANDING / f"validation-{name}.png")
    write_manifest(d)
    print("figures written to", DOCS_ASSETS, "and", LANDING)


def quantity_stats(rows):
    rows = [r for r in rows if r.get("mark", "") != "extrap"]
    e = [r["error_pct"] for r in rows]
    if not e:
        return {"n": 0}
    a = [abs(x) for x in e]
    return {"n": len(e), "mean_abs_pct": round(sum(a) / len(a), 1), "max_abs_pct": round(max(a), 1),
            "mean_signed_pct": round(sum(e) / len(e), 1), "positive": sum(1 for x in e if x > 0)}


def stats(d):
    e = [v["error_pct"] for v in d["oew"].values()]
    a = sorted(abs(x) for x in e)
    n = len(a)
    med = a[n // 2] if n % 2 else 0.5 * (a[n // 2 - 1] + a[n // 2])
    by = lambda ids: [r for r in d["rows"] if r["id"] in ids and r.get("mark", "") != "extrap"]
    ranges = by({i for i, _ in RANGE_IDS})
    adv = by({ID_RANGE_ADV})
    tofl = by({FIELD_IDS[0][0]})
    lfl = by({FIELD_IDS[1][0]})
    ld = by({LD_ID})
    s = d["summary"]
    prov = s["by_provenance"]
    per_q = {
        "oew": quantity_stats([{"error_pct": x} for x in e]),
        "range_max_payload": quantity_stats(by({RANGE_IDS[0][0]})),
        "range_mtow_max_fuel": quantity_stats(by({RANGE_IDS[1][0]})),
        "ferry_range": quantity_stats(by({RANGE_IDS[2][0]})),
        "range_advertised_payload": quantity_stats(adv),
        "tofl": quantity_stats(tofl),
        "lfl": quantity_stats(lfl),
        "cruise_ld": quantity_stats(ld),
    }
    cells = len(PRESET_ORDER) - 1
    filled = sum(len(SUMMARY_COLUMNS) - len(m) for m in d["missing"].values())
    return {
        "presets": 12,
        "presets_with_published_oew": n,
        "oew_mean_abs_error_pct": round(sum(abs(x) for x in e) / n, 1),
        "oew_median_abs_error_pct": round(med, 1),
        "oew_within_5_pct": sum(1 for x in a if x <= 5),
        "oew_within_10_pct": sum(1 for x in a if x <= 10),
        "oew_max_abs_error_pct": round(a[-1], 1),
        "range_mean_abs_error_pct": quantity_stats(ranges)["mean_abs_pct"],
        "range_comparisons": len(ranges),
        "range_positive": sum(1 for r in ranges if r["error_pct"] > 0),
        "tofl_mean_abs_error_pct": quantity_stats(tofl)["mean_abs_pct"],
        "lfl_mean_abs_error_pct": quantity_stats(lfl)["mean_abs_pct"],
        "cruise_ld_mean_abs_error_pct": quantity_stats(ld)["mean_abs_pct"],
        "per_quantity": per_q,
        "matrix_cells_total": cells * (len(SUMMARY_COLUMNS)),
        "matrix_cells_with_reference": filled,
        "scored_comparisons": s["scored_comparisons"],
        "scored_registered_inputs": sum(prov["reference_input"].values()),
        "scored_model_outputs": sum(prov["independent"].values()),
        "scored_model_outputs_within_tolerance": prov["independent"]["within_tolerance"],
        "error_convention": "(model - published) / published",
        "version": d["version"],
    }


def write_manifest(d):
    s = stats(d)
    q = s["per_quantity"]
    manifest = {
        "summary": s,
        "figures": [
            {"file": "validation-oew.png",
             "title": "Operating empty mass vs published",
             "caption": f"Across {s['presets_with_published_oew']} aircraft the mean absolute OEW error is {s['oew_mean_abs_error_pct']} % "
                        f"(median {s['oew_median_abs_error_pct']} %); {s['oew_within_5_pct']} of {s['presets_with_published_oew']} are within 5 %. "
                        f"The B747-400 ({d['oew']['B747-400']['error_pct']:+.1f} %) is a known over-prediction; C919 and A400M references are secondary or unspecified.",
             "alt": "Parity plot and signed-error bars of modelled versus published operating empty mass for 11 aircraft."},
            {"file": "validation-summary.png",
             "title": "Error by quantity and aircraft",
             "caption": "Signed errors for OEW, range (three payload-range corners and an advertised-payload point), take-off and landing field length "
                        "and cruise L/D on the 11 real aircraft. Green is within 5 %, red beyond 10 %. n/a means no reference was found.",
             "alt": "Heat table of signed percentage errors by aircraft and quantity."},
            {"file": "validation-payload-range.png",
             "title": "Range vs published values",
             "caption": f"Range corners differ from published values by {q['range_max_payload']['mean_abs_pct']} % (max payload), "
                        f"{q['range_mtow_max_fuel']['mean_abs_pct']} % (MTOW, max fuel) and {q['ferry_range']['mean_abs_pct']} % (ferry) on average, "
                        "mean absolute. Most errors are positive. The C919 advertised range is a weak reference.",
             "alt": "Grouped bars of range error at maximum payload, at MTOW, at ferry condition and at an advertised payload."},
            {"file": "validation-field.png",
             "title": "Field lengths vs published values",
             "caption": f"Take-off field length is {q['tofl']['mean_abs_pct']} % and landing field length {q['lfl']['mean_abs_pct']} % "
                        "from published values on average. Landing is over-predicted on every aircraft and is the weakest result.",
             "alt": "Grouped bars of take-off and landing field length error for nine aircraft."},
            {"file": "validation-scope.png",
             "title": "What the comparisons actually test",
             "caption": f"Of {s['scored_comparisons']} scored contract rows, {s['scored_registered_inputs']} compare a registered input "
                        f"(data entry) and only {s['scored_model_outputs']} are independent model outputs, "
                        f"{s['scored_model_outputs_within_tolerance']} of them within tolerance.",
             "alt": "Stacked bars of contract rows by status per aircraft, separating registered inputs from model outputs."},
        ],
    }
    (LANDING / "manifest.json").write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    print(json.dumps(s, indent=1))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "extract":
        extract(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        build()
