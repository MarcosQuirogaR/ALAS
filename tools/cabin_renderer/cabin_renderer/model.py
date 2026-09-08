from __future__ import annotations

import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from shapely.affinity import rotate, scale, translate
from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import nearest_points, unary_union
from shapely.geometry.base import BaseGeometry
from shapely.validation import explain_validity


class SceneError(ValueError):
    pass


def _finite(value: Any, path: str = "$") -> None:
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return
    if isinstance(value, (int, float)):
        if not math.isfinite(value):
            raise SceneError(f"{path}: non-finite number")
    elif isinstance(value, list):
        for i, item in enumerate(value):
            _finite(item, f"{path}[{i}]")
    elif isinstance(value, dict):
        for key, item in value.items():
            _finite(item, f"{path}.{key}")
    else:
        raise SceneError(f"{path}: unsupported JSON value")


def _polygon(points: Any, path: str) -> Polygon:
    if not isinstance(points, list) or len(points) < 3:
        raise SceneError(f"{path}: at least three y/z points required")
    try:
        geom = Polygon([(float(p["y"]), float(p["z"])) for p in points])
    except (KeyError, TypeError, ValueError) as exc:
        raise SceneError(f"{path}: invalid y/z point: {exc}") from exc
    if geom.is_empty or not geom.is_valid:
        raise SceneError(f"{path}: {explain_validity(geom)}")
    return geom


def _box(envelope: dict[str, Any]) -> Polygon:
    y, z = float(envelope["center_y_m"]), float(envelope["center_z_m"])
    w, h = float(envelope["width_m"]), float(envelope["height_m"])
    return Polygon([(y-w/2,z-h/2),(y+w/2,z-h/2),(y+w/2,z+h/2),(y-w/2,z+h/2)])


def _at_station(envelope: dict[str, Any], x: float, tol: float = 1e-6) -> bool:
    return abs(float(envelope["center_x_m"]) - x) <= float(envelope["length_m"]) / 2 + tol


@dataclass(frozen=True)
class Component:
    id: str
    kind: str
    geometry: BaseGeometry
    data: dict[str, Any]


@dataclass(frozen=True)
class Scene:
    title: str
    station_m: float
    outer: Polygon
    liner: Polygon
    hold: Polygon | None
    components: tuple[Component, ...]
    metadata: dict[str, Any]

    def by_kind(self, kind: str) -> tuple[Component, ...]:
        return tuple(c for c in self.components if c.kind == kind)


def available_stations(source: str | Path | dict[str, Any]) -> tuple[float, ...]:
    raw = json.loads(Path(source).read_text(encoding="utf-8")) if not isinstance(source, dict) else source
    return tuple(float(s["x_m"]) for s in raw.get("stations", []))


def recommended_scene_set(source: str | Path | dict[str, Any]) -> tuple[list[Scene], Scene | None]:
    raw = json.loads(Path(source).read_text(encoding="utf-8")) if not isinstance(source, dict) else source
    if raw.get("schema_version") != "alas.cabin-scene/v2":
        raise SceneError("$.schema_version: expected 'alas.cabin-scene/v2'")
    stations = available_stations(raw)
    passenger_decks = sorted(d["id"] for d in raw.get("decks", []) if d.get("passenger"))
    if not passenger_decks:
        raise SceneError("no passenger decks available for recommended rendering")
    cargo_decks = {d["id"] for d in raw.get("decks", []) if not d.get("passenger")}
    def score(x: float, envelopes: list[dict[str, Any]]) -> int:
        return sum(_at_station(e, x) for e in envelopes)
    windows = raw.get("windows", {}).get("apertures", [])
    cargo_env = [i["envelope"] for i in raw.get("cargo", {}).get("items", [])]
    # A transverse section has exactly one physical x coordinate.  Refuse to
    # compose attractive but physically impossible panels from different x's.
    def deck_has_row(deck_id: str, x: float) -> bool:
        return any(r["deck_id"] == deck_id and _at_station(r["envelope"], x)
                   for r in raw.get("seat_rows", []))
    candidates = [x for x in stations if all(deck_has_row(d, x) for d in passenger_decks)]
    if cargo_env and cargo_decks:
        candidates = [x for x in candidates if any(_at_station(e, x) for e in cargo_env)]
    if not candidates:
        raise SceneError("no exported station intersects every passenger deck and resolved cargo; a unified transverse section cannot be composed")
    all_env = [r["envelope"] for r in raw.get("seat_rows", [])]
    all_env += [r["envelope"] for r in raw.get("overhead", {}).get("runs", [])]
    all_env += cargo_env
    def unified_score(x: float) -> tuple[int, int, float]:
        window_hits = sum(abs(float(w["x_m"])-x) <= float(w["width_m"])/2 for w in windows)
        return score(x, all_env), window_hits, -abs(x-sum(stations)/len(stations))
    x = max(candidates, key=unified_score)
    unified = load_scene(raw, x, set(passenger_decks) | cargo_decks)
    unified.metadata["section_invariant"] = {"station_x_m": x, "all_components_same_station": True}
    return [unified], None


def recommended_scenes(source: str | Path | dict[str, Any]) -> tuple[Scene, Scene | None]:
    """Backward-compatible single-panel API; prefer recommended_scene_set."""
    passengers, cargo = recommended_scene_set(source)
    return passengers[0], cargo


def load_scene(source: str | Path | dict[str, Any], station_x_m: float | None = None,
               deck_ids: set[str] | None = None) -> Scene:
    raw = json.loads(Path(source).read_text(encoding="utf-8")) if not isinstance(source, dict) else source
    if not isinstance(raw, dict):
        raise SceneError("$: object required")
    _finite(raw)
    if raw.get("schema_version") != "alas.cabin-scene/v2":
        raise SceneError("$.schema_version: expected 'alas.cabin-scene/v2'")
    if raw.get("units") != {"length": "m", "mass": "kg", "angle": "rad"}:
        raise SceneError("$.units: renderer requires m/kg/rad")
    stations = raw.get("stations")
    if not isinstance(stations, list) or not stations:
        raise SceneError("$.stations: non-empty array required")
    target = float(station_x_m) if station_x_m is not None else float(stations[len(stations)//2]["x_m"])
    station = min(stations, key=lambda s: abs(float(s["x_m"]) - target))
    if station_x_m is not None and abs(float(station["x_m"]) - target) > 1e-6:
        raise SceneError(f"station {target:g} m unavailable; choose one of {available_stations(raw)}")
    x = float(station["x_m"])
    outer = _polygon(station["outer"]["points_yz_m"], "$.stations[].outer.points_yz_m")
    liner_src = station.get("liner") or station.get("inner")
    if not liner_src:
        raise SceneError(f"station {x:g} m has neither liner nor inner contour")
    liner = _polygon(liner_src["points_yz_m"], "$.stations[].liner.points_yz_m")
    hold = _polygon(station["hold"]["points_yz_m"], "$.stations[].hold.points_yz_m") if station.get("hold") else None
    decks_all = {d["id"]: d for d in raw.get("decks", [])}
    valid_decks = set(decks_all)
    selected = valid_decks if deck_ids is None else set(deck_ids)
    unknown = selected - valid_decks
    if unknown:
        raise SceneError(f"unknown deck(s): {', '.join(sorted(unknown))}; available: {', '.join(sorted(valid_decks))}")
    components: list[Component] = []
    ids: set[str] = set()
    def add(ident: str, kind: str, geometry: BaseGeometry, data: dict[str, Any]) -> None:
        if not ident or ident in ids:
            raise SceneError(f"duplicate/empty component id: {ident!r}")
        ids.add(ident)
        components.append(Component(ident, kind, geometry, data))
    center_fallback_diagnostics: list[dict[str, str]] = []
    for deck_id in sorted(selected):
        deck = decks_all[deck_id]
        z, width = float(deck["floor_z_m"]), float(deck["usable_width_m"])
        floor = Polygon([(-width/2,z-.045),(width/2,z-.045),(width/2,z+.045),(-width/2,z+.045)])
        add(f"floor-{deck_id}", "floor", floor, {**deck, "label": deck_id})
    rows = {r["id"]: r for r in raw.get("seat_rows", [])}
    for seat in raw.get("seats", []):
        row = rows.get(seat["row_id"])
        if seat["deck_id"] not in selected or row is None or not _at_station(row["envelope"], x):
            continue
        w = float(seat["width_m"]); h = float(row["envelope"]["height_m"])
        e = {"center_y_m": seat["center_y_m"], "center_z_m": seat["center_z_m"], "width_m": w, "height_m": h}
        add(seat["id"], "seat", _box(e), seat)
    window_source = raw.get("windows", {}).get("apertures", [])
    windows_here = [w for w in window_source if w["deck_id"] in selected and abs(float(w["x_m"])-x) <= float(w["width_m"])/2]
    for window in windows_here:
        if window["deck_id"] not in selected or abs(float(window["x_m"])-x) > float(window["width_m"])/2:
            continue
        center, normal = window["center_yz_m"], window["outward_normal_yz"]
        # Longitudinal aperture width is not visible in a transverse slice; show a
        # thin radial pane and preserve the exported tangent-direction height.
        pane_t = 0.06
        geom = Polygon([(-pane_t/2,-window["height_m"]/2),(pane_t/2,-window["height_m"]/2),(pane_t/2,window["height_m"]/2),(-pane_t/2,window["height_m"]/2)])
        angle = math.degrees(math.atan2(float(normal["z"]), float(normal["y"])))
        data=dict(window)
        if abs(float(window["x_m"])-x) > float(window["width_m"])/2:
            data["fidelity"]="visualization_only_projected_nearest_nominal_window"
        add(window["id"], "window", translate(rotate(geom, angle), float(center["y"]), float(center["z"])), data)
    topology = {t["run_id"]: t for t in raw.get("overhead", {}).get("topology", [])}
    missing_attachment_runs: list[Component] = []
    for run in raw.get("overhead", {}).get("runs", []):
        if run["deck_id"] not in selected or not _at_station(run["envelope"], x): continue
        add(run["id"], "ohcp", _polygon(run["profile_yz_m"], f"overhead.{run['id']}.profile_yz_m"), run)
        top = topology.get(run["id"], {})
        if top.get("attachment_ids"):
            # Exporter currently has IDs but no attachment geometry; never invent it.
            pass
        else:
            missing_attachment_runs.append(components[-1])
    # Visualization-only center OHSC fallback. It is eligible only for a row
    # with at least two aisles and a real center seat block. Dimensions are
    # explicit variant parameters so supplier/solver geometry can replace it.
    center_variants = {
        "regular": {"width_m": .72, "height_m": .34},
        "large": {"width_m": .88, "height_m": .40},
        "extra_large": {"width_m": 1.02, "height_m": .46},
    }
    for deck_id in sorted(selected):
        eligible = [r for r in rows.values() if r["deck_id"] == deck_id and _at_station(r["envelope"], x)
                    and len(r.get("blocks", [])) >= 3 and len(r.get("blocks", [])) - 1 >= 2
                    and int(r.get("blocks", [0, 0, 0])[1]) > 0]
        exported_center = any(c.kind == "ohcp" and c.data.get("deck_id") == deck_id
                              and abs(c.geometry.centroid.x) < .3 for c in components)
        if not eligible or exported_center: continue
        variant = "regular"
        params = center_variants[variant]
        ceiling = float(decks_all[deck_id]["ceiling_z_m"])
        w, h = params["width_m"], params["height_m"]
        profile = Polygon([(-w/2, ceiling-h), (w/2, ceiling-h),
                           (w*.42, ceiling-.04), (-w*.42, ceiling-.04)])
        data = {"deck_id": deck_id, "kind": "center_hinge", "variant": variant,
                "variant_parameters": {k: dict(v) for k, v in center_variants.items()},
                "fidelity": "visualization_only_topology_fallback",
                "source": "renderer fallback informed by DLR OHSC variant taxonomy; dimensions provisional"}
        add(f"visual-center-ohsc-{deck_id}", "ohcp", profile, data)
        missing_attachment_runs.append(components[-1])
        center_fallback_diagnostics.append({"field": f"overhead.center.{deck_id}",
            "reason": f"{variant} center OHSC is a topology-only visualization fallback; DLR publishes no dimensioned profile",
            "required_source": "resolved supplier center-bin profile and attachment geometry"})
    # Geometry-only lining fallback: these parts are never represented as solver
    # truth and remain tagged so they cannot silently become engineering inputs.
    for bin_ in missing_attachment_runs:
        top_point = max(list(bin_.geometry.exterior.coords)[:-1], key=lambda p: p[1])
        liner_point = nearest_points(Point(top_point), liner.boundary)[1]
        link = LineString([top_point, (liner_point.x, liner_point.y)])
        add(f"visual-attachment-{bin_.id}", "ohcp_attachment", link,
            {"fidelity":"visualization_only", "source":"renderer continuity fallback", "run_id":bin_.id})
        miny,minz,maxy,_ = bin_.geometry.bounds
        psu_z = minz - .055
        psu = Polygon([(miny+.04,psu_z-.025),(maxy-.04,psu_z-.025),(maxy-.04,psu_z+.025),(miny+.04,psu_z+.025)])
        add(f"visual-psu-{bin_.id}", "psu", psu,
            {"fidelity":"visualization_only", "source":"renderer PSU fallback", "run_id":bin_.id})
    by_deck_bins: dict[str, list[Component]] = {}
    for c in components:
        if c.kind == "ohcp": by_deck_bins.setdefault(str(c.data["deck_id"]), []).append(c)
    for deck_id, bins in by_deck_bins.items():
        if len(bins) < 2: continue
        left, right = min(bins, key=lambda b:b.geometry.centroid.x), max(bins, key=lambda b:b.geometry.centroid.x)
        y0,y1 = left.geometry.bounds[2], right.geometry.bounds[0]
        if y1 > y0:
            z = min(float(decks_all[deck_id]["ceiling_z_m"]), max(left.geometry.bounds[3],right.geometry.bounds[3])+.08)
            ceiling = Polygon([(y0,z-.04),(y1,z-.04),(y1,z+.04),(y0,z+.04)])
            add(f"visual-ceiling-{deck_id}", "visual_lining", ceiling,
                {"fidelity":"visualization_only", "source":"renderer continuous ceiling fallback"})
    cargo_by_slot = {i["slot_id"]: i for i in raw.get("cargo", {}).get("items", [])}
    for slot in raw.get("cargo", {}).get("slots", []):
        if slot["deck_id"] not in selected or not _at_station(slot["envelope"], x): continue
        item = cargo_by_slot.get(slot["id"])
        if not item: continue
        uld = item.get("uld")
        geom = _box(item["envelope"])
        data = dict(item)
        if uld and uld.get("normalized_contour_yz"):
            base = _polygon(uld["normalized_contour_yz"], f"cargo.{item['id']}.uld.normalized_contour_yz")
            miny,minz,maxy,maxz = base.bounds
            centered = translate(base, -(miny+maxy)/2, -(minz+maxz)/2)
            geom = translate(scale(centered, float(item["envelope"]["width_m"])/(maxy-miny), float(item["envelope"]["height_m"])/(maxz-minz), origin=(0,0)), float(item["envelope"]["center_y_m"]), float(item["envelope"]["center_z_m"]))
            data["standard"] = f"{uld['code']} · {uld['name']}"
        add(item["id"], "uld" if uld else "cargo", geom, data)
    # Put one scale human in the widest exported aisle only if its clearance
    # envelope is genuinely valid. No person is squeezed or rescaled to fit.
    occupant_skipped: list[str] = []
    for deck_id in sorted(selected):
        if not decks_all[deck_id].get("passenger"): continue
        seats_here = sorted([c for c in components if c.kind == "seat" and c.data["deck_id"] == deck_id], key=lambda c:c.geometry.centroid.x)
        if len(seats_here) < 2: continue
        gaps = [(a.geometry.bounds[2], b.geometry.bounds[0]) for a,b in zip(seats_here,seats_here[1:]) if b.geometry.bounds[0]-a.geometry.bounds[2] >= .44]
        if not gaps: continue
        lo,hi = max(gaps, key=lambda g:g[1]-g[0]); cy=(lo+hi)/2
        floor_z=float(decks_all[deck_id]["floor_z_m"])
        silhouette = Polygon([(cy-.18,floor_z),(cy-.12,floor_z+.82),(cy-.20,floor_z+1.38),(cy-.12,floor_z+1.64),(cy,floor_z+1.72),(cy+.12,floor_z+1.64),(cy+.20,floor_z+1.38),(cy+.12,floor_z+.82),(cy+.18,floor_z)])
        obstacles = [c.geometry for c in components if c.kind in {"ohcp","psu","visual_lining"}]
        if liner.covers(silhouette) and not any(silhouette.intersection(o).area > 1e-6 for o in obstacles):
            add(f"visual-occupant-{deck_id}", "occupant", silhouette,
                {"deck_id":deck_id,"fidelity":"visualization_only","source":"renderer scale/clearance figure"})
        else:
            occupant_skipped.append(deck_id)
    diagnostics = list(raw.get("missing_inputs", []))
    diagnostics += center_fallback_diagnostics
    diagnostics += [{"field":f"decks.{d}.occupant", "reason":"no unscaled 1.72 m silhouette clears liner and overhead", "required_source":"resolved occupant/clearance policy"} for d in occupant_skipped]
    diagnostics += [{"field": "overhead.topology", "reason": m, "required_source": "supplier installation geometry"} for m in raw.get("overhead", {}).get("missing", [])]
    if raw.get("windows", {}).get("status") != "authoritative":
        diagnostics.append({"field":"windows", "reason":"window apertures are non-authoritative", "required_source":"aircraft window schedule"})
    if hold is None:
        diagnostics.append({"field":"stations[].hold", "reason":f"station {x:g} m has no hold contour", "required_source":"station-indexed hold liner"})
    # Cross-sectional utilization is deliberately independent of the existing
    # longitudinal slot/fill model.  Intersections are clipped for the occupied
    # diagnostic area; overflow remains a validation error.
    hold_loads = [c.geometry for c in components if c.kind in {"uld", "cargo"}]
    occupied = unary_union(hold_loads).intersection(hold) if hold is not None and hold_loads else None
    usable_area = 0.0 if hold is None else hold.area
    occupied_area = 0.0 if occupied is None else occupied.area
    unused = None if hold is None else hold.difference(occupied if occupied is not None else Polygon())
    passenger_floors = [float(d["floor_z_m"]) for d in decks_all.values() if d.get("passenger")]
    main_floor_z = min(passenger_floors) if passenger_floors else 0.0
    miny, minz, maxy, _ = liner.bounds
    gross_underfloor = liner.intersection(box(miny - 1, minz - 1, maxy + 1, main_floor_z))
    gross_occupied = unary_union(hold_loads).intersection(gross_underfloor) if hold_loads else Polygon()
    gross_unused = gross_underfloor.difference(gross_occupied)
    longitudinal = [float(c.data.get("fill_fraction")) for c in components
                    if c.kind in {"uld", "cargo"} and c.data.get("fill_fraction") is not None]
    cargo_metrics = {"usable_hold_area_m2": usable_area, "occupied_union_area_m2": occupied_area,
                     "transverse_area_utilization": None if usable_area <= 0 else occupied_area / usable_area,
                     "gross_underfloor_area_m2": gross_underfloor.area,
                     "gross_underfloor_occupied_area_m2": gross_occupied.area,
                     "gross_underfloor_area_utilization": None if gross_underfloor.area <= 0 else gross_occupied.area / gross_underfloor.area,
                     "gross_underfloor_unused_geometry": gross_unused,
                     "longitudinal_fill_fraction": None if not longitudinal else sum(longitudinal) / len(longitudinal),
                     "unused_hold_geometry": unused,
                     "area_method": "union(load polygons) intersect usable hold / usable hold"}
    return Scene(
        f"{raw.get('provenance', {}).get('aircraft_preset') or 'Aircraft'} cabin at x={x:g} m",
        x, outer, liner, hold, tuple(components),
        {"provenance": raw.get("provenance", {}), "diagnostics": diagnostics,
         "selected_decks": sorted(selected), "available_decks": sorted(valid_decks),
         "cargo_metrics": cargo_metrics},
    )
