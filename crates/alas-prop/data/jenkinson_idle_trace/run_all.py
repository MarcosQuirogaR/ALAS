"""Re-run every trace spec; write raw node values (traces/<name>.json) and overlay PNGs.

usage: python run_all.py <overlay-dir>
Each spec holds the PDF page, the render scale, the axis calibration (two tick
pixels and their printed values per axis), the plot area, the initial curve rows
and the text boxes erased before tracing. Requires pypdfium2, numpy, Pillow.
"""
import json
import subprocess
import sys
from pathlib import Path

here = Path(__file__).resolve().parent
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for spec_path in sorted((here / "specs").glob("spec_*.json")):
    spec = json.loads(spec_path.read_text())
    name = spec_path.stem[len("spec_"):]
    spec["overlay"] = str(out / f"overlay_{name}.png")
    spec["nodes"] = [round(0.1 * k, 1) for k in range(0, 10)]
    tmp = out / f"spec_{name}.run.json"
    tmp.write_text(json.dumps(spec))
    subprocess.run([sys.executable, "-I", "-X", "utf8", str(here / "tracer.py"), str(tmp)], check=True, capture_output=True)
    (here / "traces" / f"{name}.json").write_text((out / f"overlay_{name}.json").read_text())
    print("traced", name)
