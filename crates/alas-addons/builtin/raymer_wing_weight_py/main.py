# Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Eq. 15.25 (p. 574).
#
# ALAS process add-on protocol, version 1:
#   stdin : {"protocol": 1, "inputs": {name: number}, "design": {...}}
#   stdout: {"outputs": {name: number}, "messages": [text]}
#   stderr: free text, kept in the add-on log
import json
import math
import sys

KG_PER_LB = 0.45359237
FT2_PER_M2 = 1.0 / 0.3048**2

request = json.load(sys.stdin)
x = request["inputs"]

wdg = x["mtow_kg"] / KG_PER_LB
nz = 1.5 * x["limit_load_factor"]
sw = x["wing_area_m2"] * FT2_PER_M2
scsw = x["control_surface_area_m2"] * FT2_PER_M2
sweep = math.radians(x["sweep_quarter_chord_deg"])

lb = (
    0.0051
    * (wdg * nz) ** 0.557
    * sw**0.649
    * x["aspect_ratio"] ** 0.5
    * x["thickness_ratio_root"] ** -0.4
    * (1.0 + x["taper_ratio"]) ** 0.1
    / math.cos(sweep)
    * scsw**0.1
)

print("computed with protocol", request.get("protocol"), file=sys.stderr)
json.dump(
    {
        "outputs": {"wing_mass_kg": lb * KG_PER_LB, "wing_mass_lb": lb},
        "messages": ["Raymer Eq. 15.25, 6th edition, p. 574"],
    },
    sys.stdout,
)
