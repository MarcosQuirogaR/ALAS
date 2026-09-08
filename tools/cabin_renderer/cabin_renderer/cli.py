from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from .model import SceneError, available_stations, load_scene, recommended_scene_set
from .render import write_outputs, write_recommended_outputs


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Render alas.cabin-scene/v2 as deterministic SVG.")
    parser.add_argument("scene", type=Path)
    output = parser.add_mutually_exclusive_group(required=True)
    output.add_argument("--svg", type=Path)
    output.add_argument("--batch-dir", type=Path, help="Render every exported station into this directory.")
    output.add_argument("--recommended-dir", type=Path, help="Render a recommended board for every v2 JSON in SCENE directory.")
    parser.add_argument("--png", type=Path)
    parser.add_argument("--station", type=float, help="Exact exported station x coordinate in metres.")
    parser.add_argument("--deck", action="append", dest="decks", help="Deck ID to include; repeat as needed.")
    parser.add_argument("--recommended", action="store_true", help="Select one station shared by every passenger deck and the cargo hold.")
    parser.add_argument("--allow-invalid", action="store_true", help="Annotate findings and exit successfully.")
    args = parser.parse_args(argv)
    try:
        if args.recommended_dir:
            if not args.scene.is_dir() or args.station is not None or args.decks or args.png:
                parser.error("--recommended-dir requires SCENE to be a directory and cannot use station/deck/png")
            args.recommended_dir.mkdir(parents=True, exist_ok=True)
            records=[]
            for source in sorted(args.scene.glob("*_cabin_scene_v2.json")):
                try:
                    started=time.perf_counter(); passengers,cargo=recommended_scene_set(source)
                    svg_path=args.recommended_dir/f"{source.stem.replace('_cabin_scene_v2','')}_recommended.svg"
                    png_path=svg_path.with_suffix(".png")
                    digest,findings=write_recommended_outputs(passengers,cargo,svg_path,png_path)
                    records.append({"source":str(source),"svg":str(svg_path),"png":str(png_path),"sha256":digest,
                                    "unified_sections":[{"decks":p.metadata["selected_decks"],"x_m":p.station_m} for p in passengers],
                                    "findings":[f.__dict__ for f in findings],"elapsed_s":round(time.perf_counter()-started,6)})
                except (SceneError, KeyError, TypeError) as exc:
                    records.append({"source":str(source),"error":str(exc)})
            summary=args.recommended_dir/"summary.json"
            summary.write_text(json.dumps(records,indent=2)+"\n",encoding="utf-8")
            print(json.dumps({"summary":str(summary),"rendered":sum("sha256" in r for r in records),"failed":sum("error" in r for r in records)},indent=2))
            return 0 if not any("error" in r for r in records) else 2
        if args.batch_dir:
            if args.station is not None or args.png is not None or args.recommended:
                parser.error("--batch-dir cannot be combined with --station or --png")
            records = []
            for station in available_stations(args.scene):
                stem = f"station_{station:+.3f}m".replace("+", "p").replace("-", "m")
                svg_path, png_path = args.batch_dir / f"{stem}.svg", args.batch_dir / f"{stem}.png"
                digest, findings = write_outputs(load_scene(args.scene, station, set(args.decks) if args.decks else None), svg_path, png_path)
                records.append({"station_x_m": station, "svg": str(svg_path), "png": str(png_path), "sha256": digest,
                                "findings": [f.__dict__ for f in findings]})
            print(json.dumps(records, indent=2))
            return 0 if args.allow_invalid or not any(f["severity"] == "error" for r in records for f in r["findings"]) else 2
        if args.recommended:
            if args.station is not None or args.decks:
                parser.error("--recommended selects its own stations/decks")
            passengers, cargo = recommended_scene_set(args.scene)
            digest, findings = write_recommended_outputs(passengers, cargo, args.svg, args.png)
            print(json.dumps({"svg":str(args.svg), "unified_sections":[{"decks":p.metadata["selected_decks"],"x_m":p.station_m} for p in passengers],
                              "sha256":digest,"findings":[f.__dict__ for f in findings]}, indent=2))
            return 0 if args.allow_invalid or not any(f.severity == "error" for f in findings) else 2
        scene = load_scene(args.scene, args.station, set(args.decks) if args.decks else None)
        digest, findings = write_outputs(scene, args.svg, args.png)
    except (OSError, ValueError, RuntimeError, SceneError) as exc:
        parser.error(str(exc))
    print(json.dumps({"svg": str(args.svg), "station_x_m": scene.station_m,
                      "decks": scene.metadata["selected_decks"], "diagnostics": scene.metadata["diagnostics"],
                      "sha256": digest, "findings": [f.__dict__ for f in findings]}, indent=2))
    return 0 if args.allow_invalid or not any(f.severity == "error" for f in findings) else 2


if __name__ == "__main__":
    raise SystemExit(main())
