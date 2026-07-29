# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Cross-field validation for ALASConfig.

A single field's own ``metadata={"min","max"}`` (see ``sidecar/schema.py``)
already bounds it in isolation. What's missing is *cross-field*
physical-constraint checks: a value can sit well inside its own min/max and
still combine with another field into a nonsensical design (a dive speed
slower than cruise speed; a tapered surface whose tip is wider than its
root). Rules here run against the fully-gathered
:class:`~alas.config.settings.ALASConfig`, not individual fields,
since a cross-field check inherently needs more than one dataclass's worth
of state.

Lives at the top level (sibling to ``pipeline.py``), not under
``sidecar/``, because it has no FastAPI dependency and is called directly by
both the CLI and the sidecar's ``POST /validate``
(``sidecar/routes_validate.py``).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List, Tuple

from .config.settings import ALASConfig


@dataclass(frozen=True)
class ValidationIssue:
    #: Dotted path from ALASConfig down to the offending field, e.g.
    #: ``"geometry.empennage.hstab_tip_chord_m"`` -- the first segment must
    #: match a MainWindow form attribute's config name (``"requirements"`` ->
    #: ``self.requirements_form``, etc.), the rest a chain of dataclass field
    #: names down to the leaf.
    field_path: str
    message: str
    #: "error" blocks the Run button; "warning" only highlights the field.
    severity: str = "error"


Rule = Callable[[ALASConfig], List[ValidationIssue]]


def _rule_dive_speed_exceeds_cruise(config: ALASConfig) -> List[ValidationIssue]:
    """The cruise design point must not sit outside the V-n flight envelope.

    Mirrors physics.performance.build_vn_diagram's exact VC/VD/v_cruise_op
    formulas (VD = req.dive_speed_m_s is an *equivalent* airspeed -- CS-25
    V-speeds are EAS, not TAS -- so the cruise point must also be converted
    via ``sqrt(rho/rho_sl)``, not compared as a raw TAS; comparing the two
    airspeed references directly, as an earlier version of this rule did,
    produced false positives on physically fine designs). Deliberately
    reimplements just these two lines rather than calling
    ``build_vn_diagram`` itself, which additionally needs a built
    ``asb.Airplane`` for its stall-speed terms -- too expensive to construct
    on every validation tick this runs from (a live preview's debounce),
    and unnecessary since the cruise-envelope check below never touches
    wing geometry.

    Severity follows the V-n diagram's own zone convention: past VD is the
    "cannot be exceeded" red zone (error); between VC and VD is the yellow
    caution zone (warning, not blocked).
    """
    import math

    import aerosandbox as asb

    req = config.requirements
    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    rho_sl = 1.225
    v_cruise_op = (
        req.cruise_mach * atmo.speed_of_sound() * math.sqrt(atmo.density() / rho_sl)
    )
    v_d = req.dive_speed_m_s
    v_c = v_d / 1.25

    if v_cruise_op > v_d:
        return [
            ValidationIssue(
                "requirements.dive_speed_m_s",
                f"Cruise design point ({v_cruise_op:.0f} m/s EAS at Mach {req.cruise_mach:.2f} / "
                f"{req.cruise_altitude_m:,.0f} m) exceeds the design dive speed VD ({v_d:.0f} m/s EAS) -- "
                "the aircraft would cruise outside its own structural flight envelope.",
                severity="error",
            )
        ]
    if v_cruise_op > v_c:
        return [
            ValidationIssue(
                "requirements.dive_speed_m_s",
                f"Cruise design point ({v_cruise_op:.0f} m/s EAS) is above VC ({v_c:.0f} m/s EAS = VD/1.25) -- "
                "the aircraft cruises in the V-n diagram's caution band, not normal operation.",
                severity="warning",
            )
        ]
    return []


def _rule_empennage_taper(config: ALASConfig) -> List[ValidationIssue]:
    """Tip chord must be smaller than root chord for a tapered surface --
    the canonical "does this basic shape make sense" geometric sanity check.
    """
    emp = config.geometry.empennage
    issues: List[ValidationIssue] = []
    if emp.hstab_tip_chord_m >= emp.hstab_root_chord_m:
        issues.append(
            ValidationIssue(
                "geometry.empennage.hstab_tip_chord_m",
                f"H-stab tip chord ({emp.hstab_tip_chord_m:.2f} m) must be smaller than its root chord "
                f"({emp.hstab_root_chord_m:.2f} m).",
                severity="error",
            )
        )
    if emp.vstab_tip_chord_m >= emp.vstab_root_chord_m:
        issues.append(
            ValidationIssue(
                "geometry.empennage.vstab_tip_chord_m",
                f"V-stab tip chord ({emp.vstab_tip_chord_m:.2f} m) must be smaller than its root chord "
                f"({emp.vstab_root_chord_m:.2f} m).",
                severity="error",
            )
        )
    return issues


_RULES: Tuple[Rule, ...] = (
    _rule_dive_speed_exceeds_cruise,
    _rule_empennage_taper,
)


def validate(config: ALASConfig) -> List[ValidationIssue]:
    """Run every registered rule, collecting every issue (a rule failing with
    an unrelated exception -- e.g. a field temporarily mid-edit in a way that
    breaks a unit conversion -- is swallowed rather than crashing the whole
    debounced preview refresh this typically runs from)."""
    issues: List[ValidationIssue] = []
    for rule in _RULES:
        try:
            issues.extend(rule(config))
        except Exception:
            continue
    return issues
