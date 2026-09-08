from __future__ import annotations

import hashlib
from pathlib import Path

from .model import Component, Scene
from .svg import element, num, path_data
from .validate import Finding, validate_scene


STYLES = {
    "seat": ("#275a86", "#132c44"), "occupant": ("#d6b08a", "#593e31"),
    "ohcp": ("#e3ddd1", "#5c6268"), "ohcp_attachment": ("none", "#73808b"),
    "window": ("#7dd3fc", "#d9f2ff"), "floor": ("#4b5563", "#cbd5e1"),
    "uld": ("#d8a43b", "#5b3b08"), "cargo": ("#b8872e", "#5b3b08"),
    "psu": ("#eee7d8", "#777066"), "visual_lining": ("#ece7dc", "#9e988e"),
}


def _component(c: Component) -> str:
    fill, stroke = STYLES.get(c.kind, ("#9ca3af", "#374151"))
    attrs = {"class": f"component {c.kind}", "d": path_data(c.geometry), "data-id": c.id,
             "fill": fill, "fill-rule": "evenodd", "stroke": stroke, "stroke-width": "0.018"}
    shape = element("path", attrs)
    minx,miny,maxx,maxy = c.geometry.bounds
    if c.kind == "seat":
        w,h=maxx-minx,maxy-miny; leg=.035
        back = element("rect", {"fill":"#3975a8","stroke":"#132c44","stroke-width":".014","rx":num(w*.10),"x":num(minx+w*.13),"y":num(-maxy),"width":num(w*.74),"height":num(h*.62)})
        head = element("rect", {"fill":"#4c88ba","stroke":"#132c44","stroke-width":".012","rx":num(w*.12),"x":num(minx+w*.20),"y":num(-maxy),"width":num(w*.60),"height":num(h*.18)})
        cushion = element("path", {"d":f"M {num(minx+w*.08)} {num(-(miny+h*.42))} Q {num((minx+maxx)/2)} {num(-(miny+h*.34))} {num(maxx-w*.08)} {num(-(miny+h*.42))} L {num(maxx-w*.16)} {num(-(miny+h*.25))} L {num(minx+w*.16)} {num(-(miny+h*.25))} Z","fill":"#4c88ba","stroke":"#132c44","stroke-width":".014"})
        frame = "".join(element("line", {"x1":num(x),"x2":num(x+(-.025 if x>(minx+maxx)/2 else .025)),"y1":num(-(miny+h*.28)),"y2":num(-miny),"stroke":"#9aa7b1","stroke-width":num(leg)}) for x in (minx+w*.25,maxx-w*.25))
        arms = "".join(element("line", {"x1":num(x),"x2":num(x),"y1":num(-(miny+h*.48)),"y2":num(-(miny+h*.34)),"stroke":"#172536","stroke-width":".025"}) for x in (minx+w*.08,maxx-w*.08))
        shape = back + head + cushion + frame + arms
    elif c.kind == "floor":
        shape += "".join(element("circle", {"cx":num(x),"cy":num(-(maxy-.01)),"fill":"#aab5bf","r":".018"}) for x in [minx+(maxx-minx)*i/8 for i in range(1,8)])
        label = c.data.get("label", "deck")
        shape += element("text", {"class":"deck-label","x":num(minx+.05),"y":num(-maxy-.06)}, str(label))
    elif c.kind == "ohcp":
        seamx=(minx+maxx)/2
        shape += element("path", {"d":f"M {num(minx)} {num(-miny)} Q {num(seamx)} {num(-(miny-.045))} {num(maxx)} {num(-miny)}","fill":"none","stroke":"#8d8981","stroke-width":".012"})
        shape += element("circle", {"cx":num(seamx),"cy":num(-miny-.035),"fill":"#7b7e80","r":".018"})
    elif c.kind == "window":
        shape = element("path", {**attrs,"fill":"#4faed8","stroke":"#e8f8ff","stroke-width":".045"}) + element("path", {**attrs,"fill":"none","stroke":"#173e54","stroke-width":".012"})
    if c.kind not in {"uld","cargo"}:
        fidelity=str(c.data.get("fidelity",""))
        return element("g", {"aria-label": c.id, "data-fidelity":fidelity, "id": f"component-{c.id}"}, shape)
    p = c.geometry.representative_point()
    code = str(c.data.get("standard", c.data.get("model", "BULK")))
    rollers = "".join(element("circle", {"cx":num(minx+(maxx-minx)*i/6),"cy":num(-miny+.035),"fill":"#cbd2d8","stroke":"#505a61","stroke-width":".008","r":".026"}) for i in range(1,6))
    ribs = "".join(element("line", {"x1":num(minx+(maxx-minx)*i/5),"x2":num(minx+(maxx-minx)*i/5),"y1":num(-miny),"y2":num(-maxy),"stroke":"#8b671d","stroke-width":".008","opacity":".6"}) for i in range(1,5))
    label = element("text", {"class": "uld-label", "font-family": "Arial, sans-serif", "font-size": "0.13",
                             "font-weight": "700", "x": num(p.x), "y": num(-p.y), "text-anchor": "middle"}, code)
    fill = c.data.get("fill_fraction")
    detail = "" if fill is None else element("text", {"class":"uld-detail","x":num(p.x),"y":num(-p.y+.18),"text-anchor":"middle"}, f"{float(fill)*100:.0f}% exported load proxy")
    return element("g", {"aria-label": f"{c.id} {code}", "id": f"component-{c.id}"}, shape + ribs + rollers + label + detail)


def render_svg(scene: Scene, annotate: bool = True, chrome: bool = True) -> tuple[str, list[Finding]]:
    findings = validate_scene(scene)
    minx, miny, maxx, maxy = scene.outer.bounds
    pad = max(maxx - minx, maxy - miny) * 0.08
    vb = f"{num(minx-pad)} {num(-maxy-pad)} {num(maxx-minx+2*pad)} {num(maxy-miny+2*pad)}"
    css = ".component{vector-effect:non-scaling-stroke}.uld-label{fill:#211805;paint-order:stroke;stroke:#fff;stroke-width:.012}.uld-detail,.deck-label{font:.09px Arial,sans-serif;fill:#dfe7ed}.deck-label{font-weight:700;fill:#5b6570}.warning{font:600 .12px sans-serif;fill:#ffdf70}"
    shell = element("path", {"d": path_data(scene.outer), "fill": "#788694", "stroke": "#202b36", "stroke-width": ".035"})
    liner = element("path", {"d": path_data(scene.liner), "fill": "#f5f0e7", "stroke": "#ced5da", "stroke-width": ".025"})
    liner_shadow = element("path", {"d":path_data(scene.liner.boundary),"fill":"none","stroke":"#aab6bf","stroke-width":".055","opacity":".45"})
    hold = "" if scene.hold is None else element("path", {"d": path_data(scene.hold), "fill": "#303942", "stroke": "#9aa7b1", "stroke-width": ".018"})
    cargo_metrics = scene.metadata.get("cargo_metrics", {})
    unused_geom = cargo_metrics.get("unused_hold_geometry")
    gross_unused_geom = cargo_metrics.get("gross_underfloor_unused_geometry")
    gross_unused = "" if gross_unused_geom is None or gross_unused_geom.is_empty else element("path", {
        "aria-label":"Unused gross underfloor cross-sectional area", "d":path_data(gross_unused_geom),
        "fill":"#ec4899", "fill-opacity":".13", "stroke":"none"})
    unused = "" if unused_geom is None or unused_geom.is_empty else element("path", {
        "aria-label":"Unused usable hold cross-sectional area", "d":path_data(unused_geom),
        "fill":"#d9468d", "fill-opacity":".38", "stroke":"#f472b6", "stroke-width":".012"})
    components = "".join(_component(c) for c in sorted(scene.components, key=lambda x: (x.kind, x.id)))
    warnings = ""
    diagnostics = scene.metadata.get("diagnostics", [])
    if annotate and (findings or diagnostics):
        lines = [f"{f.severity.upper()}: {f.code}: {f.message}" for f in findings]
        lines += [f"MISSING: {d.get('field')}: {d.get('reason')} | needs {d.get('required_source')}" for d in diagnostics]
        warnings = element("metadata", {"id": "geometry-findings"}, "\n".join(lines))
    title = "" if not chrome else element("text", {"fill":"#17212b","font-family":"Arial, sans-serif","font-size":num((maxx-minx)*.055),"font-weight":"700","text-anchor":"middle","x":num((minx+maxx)/2),"y":num(-maxy-pad*.35)}, scene.title)
    badge = "" if not chrome or not findings else element("text", {"fill":"#8a4b00","font-family":"Arial, sans-serif","font-size":num((maxx-minx)*.027),"font-weight":"700","text-anchor":"middle","x":num((minx+maxx)/2),"y":num(-miny+pad*.55)}, f"! {len(findings)} geometry finding(s) · see metadata")
    metric_label = ""
    utilization = cargo_metrics.get("transverse_area_utilization")
    longitudinal = cargo_metrics.get("longitudinal_fill_fraction")
    gross = cargo_metrics.get("gross_underfloor_area_utilization")
    if chrome and utilization is not None:
        longitudinal_text = "unavailable" if longitudinal is None else f"{longitudinal*100:.1f}%"
        metric_label = element("text", {"fill":"#9f295f","font-family":"Arial, sans-serif",
            "font-size":num((maxx-minx)*.025),"font-weight":"700","text-anchor":"middle",
            "x":num((minx+maxx)/2),"y":num(-miny+pad*.30)},
            f"Hold-liner area used {utilization*100:.1f}% · gross underfloor area used {gross*100:.1f}% · exported ULD fill proxy {longitudinal_text}")
    body = element("style", {}, css) + warnings + title + badge + metric_label + element("g", {"id": "structure"}, shell + liner + liner_shadow + gross_unused + hold + unused) + element("g", {"id": "components"}, components)
    svg = element("svg", {"aria-label": scene.title, "data-schema": "alas.cabin-scene/v2", "role": "img", "viewBox": vb, "xmlns": "http://www.w3.org/2000/svg"}, body)
    return "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" + svg + "\n", findings


def write_outputs(scene: Scene, svg_path: Path, png_path: Path | None = None, annotate: bool = True) -> tuple[str, list[Finding]]:
    svg, findings = render_svg(scene, annotate)
    svg_path.parent.mkdir(parents=True, exist_ok=True)
    svg_path.write_text(svg, encoding="utf-8", newline="\n")
    if png_path:
        try:
            import cairosvg
        except ImportError as exc:
            raise RuntimeError("PNG output requires CairoSVG") from exc
        cairosvg.svg2png(bytestring=svg.encode(), write_to=str(png_path), output_width=1800)
    return hashlib.sha256(svg.encode()).hexdigest(), findings


def render_recommended_svg(passengers: Scene | list[Scene], cargo: Scene | None) -> tuple[str, list[Finding]]:
    panels = [passengers] if isinstance(passengers, Scene) else passengers
    if len(panels) != 1 or cargo is not None:
        raise ValueError("recommended rendering requires one unified scene and no independent cargo panel")
    scene = panels[0]
    invariant = scene.metadata.get("section_invariant", {})
    if not invariant.get("all_components_same_station"):
        raise ValueError("scene does not declare the one-shell station invariant")
    return render_svg(scene, annotate=True, chrome=True)


def write_recommended_outputs(passenger: Scene | list[Scene], cargo: Scene | None, svg_path: Path,
                              png_path: Path | None = None) -> tuple[str, list[Finding]]:
    svg, findings = render_recommended_svg(passenger, cargo)
    svg_path.parent.mkdir(parents=True, exist_ok=True)
    svg_path.write_text(svg, encoding="utf-8", newline="\n")
    if png_path:
        import cairosvg
        cairosvg.svg2png(bytestring=svg.encode(), write_to=str(png_path), output_width=2000)
    return hashlib.sha256(svg.encode()).hexdigest(), findings
