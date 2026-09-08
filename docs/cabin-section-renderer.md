# Cabin cross-section renderer

Decision recorded 2026-09-04. Evidence policy for the geometry itself is in
[`cabin-renderer-sources.md`](cabin-renderer-sources.md); this file records how
the drawing is built and why in this language.

## Decision

The 2-D cabin section is drawn in Rust, in `alas-report`, as an
`alas_report::scene::Scene`. There is no second language and no new dependency.

`crates/alas-report/src/families/geometry/section.rs` consumes the
`alas.cabin-scene/v2` scene the pipeline already exports and emits scene
primitives. Those primitives already have two backends — `alas-report::svg` for
export and `alas-viz::render` for the egui viewport — so one implementation
produces the in-app figure, the SVG, the PDF page and the headless raster.

Four reasons, in the order they decided it:

1. **The geometry is already solved in Rust.** Station contours, deck heights,
   seat positions, overhead envelopes and ULD contours come out of
   `alas-pipeline`. A renderer in another language re-parses that and drifts
   from it; a renderer in this one cannot disagree with the exporter about
   where a seat is.
2. **One executable.** `docs/ARCHITECTURE.md` commits to a single native binary
   with no interpreter and no sidecar. A Python renderer is a second runtime to
   install, and on the development machine the prototype's interpreter is not
   installed at all, so it cannot currently run.
3. **No geometry engine is needed.** The prototype used Shapely for containment
   and area. Containment of a rectangle in a convex section, polygon area and a
   half-width lookup are about eighty lines of Rust, and unused hold area is
   shaded with the SVG even-odd fill rule rather than a boolean difference —
   the printed numbers come from the exact areas.
4. **Speed makes it usable.** All eight presets render in about one second,
   because `CabinScene::from_parts` builds a scene from geometry and a payload
   layout without running an analysis first.

`tools/cabin_renderer/` (Python, Shapely, CairoSVG) keeps its value as the
prototype that established the rules below and as an independent oracle. It is
not the production path.

## How the figure is built

1. **Select one station.** A transverse section has exactly one longitudinal
   coordinate. Stations are ranked by seat rows cut on passenger decks, then
   cargo, a hold contour, overhead runs and seat count. Nothing is borrowed
   from a neighbouring frame to fill the picture.
2. **Slice.** Seats, overhead runs, cargo and windows whose exported envelopes
   span that station, plus the station's outer, inner, liner and hold contours.
3. **Validate before drawing.** Seat envelopes and overhead runs against the
   liner, cargo against the hold, the standing figure against everything, and
   the clear distance from each overhead run to the liner.
4. **Draw and state.** The figure prints the station it chose, what that
   station covered, the hold's cross-sectional area utilization, the fidelity
   of its inputs and every finding.

Nothing is resized to fit. A seat that does not fit keeps its solved width, a
ULD keeps the proportions of its standard contour, and the scale figure keeps
its 1.75 m; the mismatch becomes a finding instead of a smaller drawing.

## Running the gallery

```
cargo run -p alas-gui --example render_cabin_section_gallery
```

Writes `outputs/cabin_sections/`: for every registered preset the resolved
`alas.cabin-scene/v2` JSON and a light and dark section as SVG and PNG, plus
`index.html` as a contact sheet. An output directory can be given as the first
argument.

## What the current sections show

Every preset renders, including the A380's two passenger decks at one station.
The findings are about the inputs, not the drawing, and are the reason the
figure prints them:

- Outboard **seat envelopes cross the cabin liner** on most presets: the layout
  solver fits seats abreast without a liner-clearance check at seat height.
- **Overhead runs stand 1.2 m to 2.4 m clear of the liner** with no exported
  attachment geometry, and some cross it. The scene already declares
  `overhead.topology` missing.
- **ATR72-600** resolves a 1.17 m deck height and 1.25 m seats, so no aisle
  clears a standing figure and four seat envelopes leave the liner.
- **A220-300 and ATR72-600** put a cargo item across the hold liner.

## Limits

- `liner` is the inner envelope reused; there is no liner mould line input, so
  it carries `nominal_fallback`.
- Window apertures are nominal. When no aperture is cut by the drawn plane the
  nearest one is projected onto it and drawn dashed, and the figure says so.
- Vertex containment stands in for polygon containment. It is exact for the
  rectangles tested against the elliptical sections in use; a non-convex liner
  would need an edge-crossing test as well.
- These are engineering visualizations. They are not certified geometry and not
  suitable for loading or airworthiness decisions.
