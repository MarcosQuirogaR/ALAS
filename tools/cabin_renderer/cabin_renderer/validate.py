from __future__ import annotations

from dataclasses import dataclass

from shapely.ops import unary_union

from .model import Scene


@dataclass(frozen=True)
class Finding:
    severity: str
    code: str
    message: str
    component_ids: tuple[str, ...] = ()


def validate_scene(scene: Scene, tolerance_m: float = 1e-4) -> list[Finding]:
    findings: list[Finding] = []
    if not scene.outer.buffer(tolerance_m).covers(scene.liner):
        findings.append(Finding("error", "LINER_OUTSIDE_SHELL", "Cabin liner is not contained by outer shell."))
    for c in scene.components:
        boundary = scene.hold if c.kind == "uld" else (scene.outer if c.kind == "window" else scene.liner)
        if boundary is not None and not boundary.buffer(tolerance_m).covers(c.geometry):
            findings.append(Finding("error", "COMPONENT_OUTSIDE_CONTOUR", f"{c.id} exceeds its containing contour.", (c.id,)))

    occupants = scene.by_kind("occupant") + scene.by_kind("seat")
    bins = scene.by_kind("ohcp")
    clearance_obstacles = bins + scene.by_kind("psu") + scene.by_kind("visual_lining")
    for person in occupants:
        for bin_ in clearance_obstacles:
            if person.geometry.intersection(bin_.geometry).area > tolerance_m**2:
                findings.append(Finding("error", "SEAT_OHCP_INTERSECTION", f"{person.id} intersects overhead/lining {bin_.id}.", (person.id, bin_.id)))

    ulds = scene.by_kind("uld")
    for i, left in enumerate(ulds):
        for right in ulds[i + 1:]:
            if left.geometry.intersection(right.geometry).area > tolerance_m**2:
                findings.append(Finding("error", "ULD_OVERLAP", f"{left.id} overlaps {right.id}.", (left.id, right.id)))

    attachments = scene.by_kind("ohcp_attachment")
    attachment_union = unary_union([a.geometry for a in attachments]) if attachments else None
    for bin_ in bins:
        connected = attachment_union is not None and bin_.geometry.buffer(tolerance_m).intersects(attachment_union)
        connected = connected and scene.liner.boundary.buffer(tolerance_m * 10).intersects(attachment_union)
        if not connected:
            findings.append(Finding("error", "OHCP_NOT_ATTACHED", f"{bin_.id} has no continuous attachment path to liner.", (bin_.id,)))
    return findings
