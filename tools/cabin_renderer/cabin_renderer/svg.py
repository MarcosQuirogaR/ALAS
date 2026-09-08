from __future__ import annotations

import html
from shapely.geometry import GeometryCollection, LineString, MultiLineString, MultiPolygon, Point, Polygon
from shapely.geometry.base import BaseGeometry


def num(v: float) -> str:
    value = f"{v:.5f}".rstrip("0").rstrip(".")
    return "0" if value == "-0" else value


def path_data(geom: BaseGeometry, yflip: bool = True) -> str:
    def ring(coords) -> str:
        pts = [(x, -y if yflip else y) for x, y, *_ in coords]
        return "M " + " L ".join(f"{num(x)} {num(y)}" for x, y in pts) + " Z"
    if isinstance(geom, Polygon):
        return " ".join([ring(geom.exterior.coords), *[ring(r.coords) for r in geom.interiors]])
    if isinstance(geom, MultiPolygon):
        return " ".join(path_data(g, yflip) for g in geom.geoms)
    if isinstance(geom, LineString):
        pts = [(x, -y if yflip else y) for x, y, *_ in geom.coords]
        return "M " + " L ".join(f"{num(x)} {num(y)}" for x, y in pts)
    if isinstance(geom, MultiLineString | GeometryCollection):
        return " ".join(path_data(g, yflip) for g in geom.geoms)
    if isinstance(geom, Point):
        return f"M {num(geom.x)} {num(-geom.y)}"
    raise TypeError(f"unsupported SVG geometry {geom.geom_type}")


def element(tag: str, attrs: dict[str, str], content: str = "") -> str:
    values = " ".join(f'{key}="{html.escape(str(value), quote=True)}"' for key, value in sorted(attrs.items()))
    return f"<{tag} {values}>{content}</{tag}>" if content else f"<{tag} {values}/ >".replace("/ >", "/>")
