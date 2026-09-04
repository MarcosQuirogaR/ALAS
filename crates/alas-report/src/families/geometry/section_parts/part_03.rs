// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Canvas width, in scene pixels.
const WIDTH: f64 = 900.0;

/// Canvas height, in scene pixels.
const HEIGHT: f64 = 790.0;

/// Drawing area the section is fitted into, as `[left, top, right, bottom]`.
///
/// Dimension lines are drawn outside it, the way they are on a general
/// arrangement drawing, so the margins are part of the figure rather than
/// slack.
const FRAME: [f64; 4] = [180.0, 82.0, 720.0, 606.0];

/// Canvas height at which the key and the notes begin.
const NOTES_TOP: f64 = 652.0;

/// Characters per line of the notes block.
///
/// Scene text is not laid out by a text engine, so the wrap is a character
/// count chosen for the notes font size and this canvas width.
const NOTE_WRAP_CHARS: usize = 168;

/// Metres to inches, for the second figure printed on every dimension.
const INCHES_PER_METRE: f64 = 39.370_078_740_157_48;

/// Placement of the section in the canvas.
#[derive(Clone, Copy)]
struct SectionMap {
    /// Canvas position of the aircraft centreline at the vertical datum.
    origin: [f64; 2],
    /// Pixels per metre, equal in both axes.
    scale: f64,
    /// Section height that maps to the vertical datum, in metres.
    z_datum: f64,
}

impl SectionMap {
    /// Fit a ring into the drawing frame with an equal scale in both axes.
    fn fit(ring: &[[f64; 2]]) -> Self {
        let extent = bounds(ring);
        let (span_y, span_z) = (
            (extent[2] - extent[0]).max(1e-3),
            (extent[3] - extent[1]).max(1e-3),
        );
        let scale = ((FRAME[2] - FRAME[0]) / span_y).min((FRAME[3] - FRAME[1]) / span_z);
        let z_datum = 0.5 * (extent[1] + extent[3]);
        Self {
            origin: [
                0.5 * (FRAME[0] + FRAME[2]) - 0.5 * (extent[0] + extent[2]) * scale,
                0.5 * (FRAME[1] + FRAME[3]),
            ],
            scale,
            z_datum,
        }
    }

    /// Map a section point to canvas pixels.
    fn at(self, y_m: f64, z_m: f64) -> [f64; 2] {
        [
            self.origin[0] + y_m * self.scale,
            self.origin[1] - (z_m - self.z_datum) * self.scale,
        ]
    }

    /// Map a ring to canvas pixels.
    fn ring(self, ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
        ring.iter().map(|p| self.at(p[0], p[1])).collect()
    }
}

/// Fills and strokes that stay semantic across themes.
///
/// Component colour carries meaning here -- class, ULD, structure, lining -- so
/// it does not follow the palette. What follows the palette is the page: the
/// background, the text, and the interior tone the drawing sits on.
struct SectionInk {
    /// Cabin interior fill.
    interior: Color,
    /// Pressure-shell fill.
    shell: Color,
    /// Structure between the outer mould line and the inner boundary.
    structure: Color,
    /// Deck floor fill.
    floor: Color,
    /// Hold interior fill.
    hold: Color,
    /// Outline used on drawn hardware.
    outline: Color,
    /// Primary text.
    text: Color,
    /// Secondary text and dimension lines.
    muted: Color,
}

impl SectionInk {
    /// Resolve the page-following tones for a theme.
    fn new(palette_name: &str) -> Self {
        let dark = palette_name.contains("dark") || palette_name.contains("grey");
        Self {
            interior: Color::from_hex(if dark { "#2b3138" } else { "#f7f4ed" }),
            shell: Color::from_hex(if dark { "#828b95" } else { "#c3c9cf" }),
            structure: Color::from_hex(if dark { "#5c646d" } else { "#9aa3ac" }),
            floor: Color::from_hex(if dark { "#78828c" } else { "#8b959f" }),
            hold: Color::from_hex(if dark { "#20262d" } else { "#39424c" }),
            outline: Color::from_hex(if dark { "#101418" } else { "#2c343c" }),
            text: Color::from_hex(if dark { "#f2f4f6" } else { "#101418" }),
            muted: Color::from_hex(if dark { "#aab3bc" } else { "#5b656f" }),
        }
    }
}

/// Add a filled and outlined polygon in canvas coordinates.
fn poly(scene: &mut Scene, points: Vec<[f64; 2]>, fill: Color, stroke: Color, width: f64) {
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(fill)),
        stroke: (width > 0.0).then(|| Stroke::new(stroke, width)),
    });
}

/// Add a text label.
#[allow(clippy::too_many_arguments)] // A label is a position, a font and an alignment; naming a struct for it would be longer at every call.
fn label(
    scene: &mut Scene,
    value: impl Into<String>,
    pos: [f64; 2],
    color: Color,
    size: f64,
    align: TextAlign,
    baseline: TextBaseline,
    bold: bool,
) {
    scene.add(SceneElement::Text {
        text: value.into(),
        pos,
        font_size: size,
        color,
        align,
        baseline,
        angle_deg: 0.0,
        bold,
    });
}

/// Seat fill by cabin class.
fn class_color(class: &str) -> Color {
    Color::from_hex(match class {
        "First" => "#8e6fc0",
        "Business" => "#3d8fd1",
        "Premium" => "#2bb2a4",
        _ => "#2fbf87",
    })
}

/// Draw one seat: legs, pan, reclined back, headrest and both armrests.
fn draw_seat(scene: &mut Scene, map: SectionMap, seat: &SeatSlice, color: Color, ink: &SectionInk) {
    let (floor, top) = (seat.floor_z_m, seat.floor_z_m + seat.height_m);
    let pan = floor + seat.height_m * 0.38;
    let half = seat.width_m * 0.5;
    let fill = if seat.occupied {
        color
    } else {
        Color::rgba(color.r, color.g, color.b, 90)
    };
    for leg in [-half * 0.55, half * 0.55] {
        scene.add(SceneElement::Line {
            p1: map.at(seat.y_m + leg, floor),
            p2: map.at(seat.y_m + leg, pan - 0.03),
            stroke: Stroke::new(ink.muted, 2.0),
        });
    }
    poly(
        scene,
        map.ring(&rectangle(seat.y_m, seat.width_m, pan - 0.05, pan + 0.05)),
        fill,
        ink.outline,
        1.1,
    );
    poly(
        scene,
        map.ring(&vec![
            [seat.y_m - half * 0.86, pan],
            [seat.y_m + half * 0.86, pan],
            [seat.y_m + half * 0.74, top - seat.height_m * 0.16],
            [seat.y_m - half * 0.74, top - seat.height_m * 0.16],
        ]),
        fill,
        ink.outline,
        1.1,
    );
    poly(
        scene,
        map.ring(&rectangle(
            seat.y_m,
            seat.width_m * 0.62,
            top - seat.height_m * 0.16,
            top,
        )),
        fill,
        ink.outline,
        1.1,
    );
    for arm in [-half, half] {
        poly(
            scene,
            map.ring(&rectangle(
                seat.y_m + arm,
                seat.width_m * 0.10,
                pan + 0.02,
                pan + 0.18,
            )),
            ink.muted,
            ink.outline,
            0.8,
        );
    }
}

/// Draw the standing scale figure.
fn draw_occupant(scene: &mut Scene, map: SectionMap, y_m: f64, floor_z_m: f64, ink: &SectionInk) {
    let grey = Color::from_hex("#98a1aa");
    poly(
        scene,
        map.ring(&occupant_ring(y_m, floor_z_m)),
        grey,
        ink.outline,
        0.9,
    );
    scene.add(SceneElement::Circle {
        center: map.at(y_m, floor_z_m + OCCUPANT_HEIGHT_M - 0.115),
        radius: 0.115 * map.scale,
        fill: Some(Fill::new(grey)),
        stroke: Some(Stroke::new(ink.outline, 0.9)),
    });
}

/// Draw one overhead run from its exported profile, plus a door seam.
fn draw_bin(scene: &mut Scene, map: SectionMap, bin: &BinSlice, ink: &SectionInk) {
    let fill = Color::from_hex(if bin.kind == "center" {
        "#d3cec3"
    } else {
        "#e2ded4"
    });
    poly(scene, map.ring(&bin.profile), fill, ink.outline, 1.2);
    let extent = bounds(&bin.profile);
    let seam = extent[1] + (extent[3] - extent[1]) * 0.34;
    scene.add(SceneElement::Line {
        p1: map.at(extent[0] + (extent[2] - extent[0]) * 0.08, seam),
        p2: map.at(extent[2] - (extent[2] - extent[0]) * 0.08, seam),
        stroke: Stroke::new(Color::from_hex("#8d8981"), 1.0),
    });
}

/// Draw a window aperture, rotated onto its exported outward normal.
///
/// A projected aperture is drawn dashed, because it belongs to the nearest
/// exported longitudinal position rather than to the plane being cut.
fn draw_window(
    scene: &mut Scene,
    map: SectionMap,
    window: &WindowAperture,
    projected: bool,
    ink: &SectionInk,
) {
    let normal = &window.outward_normal_yz;
    let magnitude = (normal.y * normal.y + normal.z * normal.z).sqrt();
    let (nz, ny) = if magnitude > 0.0 {
        (normal.z / magnitude, normal.y / magnitude)
    } else {
        (0.0, 1.0)
    };
    let (half_h, half_t) = (window.height_m * 0.5, 0.045);
    let center = [window.center_yz_m.y, window.center_yz_m.z];
    let corners: Ring = [
        [-half_t, -half_h],
        [half_t, -half_h],
        [half_t, half_h],
        [-half_t, half_h],
    ]
    .iter()
    .map(|p| {
        [
            center[0] + p[0] * ny - p[1] * nz,
            center[1] + p[0] * nz + p[1] * ny,
        ]
    })
    .collect();
    scene.add(SceneElement::Polygon {
        points: map.ring(&corners),
        fill: Some(Fill::new(Color::from_hex("#8fd3f4"))),
        stroke: Some(if projected {
            Stroke::dashed(ink.outline, 1.0, 3.0, 2.0)
        } else {
            Stroke::new(ink.outline, 1.0)
        }),
    });
}

/// Draw one cargo item and its identifying label.
fn draw_cargo(scene: &mut Scene, map: SectionMap, item: &CargoSlice, ink: &SectionInk) {
    let fill = if item.from_uld {
        Color::from_hex("#a8a4d8")
    } else {
        Color::from_hex("#9aa3ad")
    };
    poly(
        scene,
        map.ring(&item.ring),
        fill,
        Color::from_hex("#3f3b6b"),
        1.2,
    );
    let extent = bounds(&item.ring);
    let center = map.at(
        0.5 * (extent[0] + extent[2]),
        0.5 * (extent[1] + extent[3]),
    );
    label(
        scene,
        item.label.clone(),
        [center[0], center[1] - 6.0],
        ink.outline,
        10.0,
        TextAlign::Center,
        TextBaseline::Middle,
        true,
    );
    label(
        scene,
        format!("{:.0} kg", item.mass_kg),
        [center[0], center[1] + 7.0],
        ink.outline,
        8.5,
        TextAlign::Center,
        TextBaseline::Middle,
        false,
    );
}

/// Draw the pressure shell, the structure ring, the cabin liner and the hold.
fn draw_structure(scene: &mut Scene, map: SectionMap, slice: &SectionSlice<'_>, ink: &SectionInk) {
    poly(scene, map.ring(&slice.outer), ink.shell, ink.outline, 1.6);
    if let Some(inner) = slice.inner.as_ref() {
        poly(scene, map.ring(inner), ink.structure, ink.outline, 1.0);
    }
    if let Some(liner) = slice.liner.as_ref() {
        poly(
            scene,
            map.ring(liner),
            ink.interior,
            Color::from_hex("#b9c1c8"),
            1.2,
        );
    }
    if let Some(hold) = slice.hold.as_ref() {
        poly(
            scene,
            map.ring(hold),
            ink.hold,
            Color::from_hex("#98a2ac"),
            1.2,
        );
    }
}

/// Draw one deck: its floor slab, seats, occupant, overhead runs and name.
fn draw_deck(scene: &mut Scene, map: SectionMap, deck: &DeckSlice<'_>, ink: &SectionInk) {
    let half_width = deck.deck.usable_width_m * 0.5;
    poly(
        scene,
        map.ring(&rectangle(
            0.0,
            deck.deck.usable_width_m,
            deck.deck.floor_z_m - FLOOR_HALF_THICKNESS_M,
            deck.deck.floor_z_m + FLOOR_HALF_THICKNESS_M,
        )),
        ink.floor,
        ink.outline,
        1.0,
    );
    let color = deck
        .row
        .map_or_else(|| class_color("Economy"), |row| class_color(&row.class));
    for seat in &deck.seats {
        draw_seat(scene, map, seat, color, ink);
    }
    for &y_m in &deck.occupants {
        draw_occupant(scene, map, y_m, deck.deck.floor_z_m, ink);
    }
    for bin in &deck.bins {
        draw_bin(scene, map, bin, ink);
    }
    if deck.deck.passenger {
        label(
            scene,
            format!("{} deck", deck.deck.id),
            map.at(-half_width, deck.deck.floor_z_m + 0.10),
            ink.muted,
            9.0,
            TextAlign::Left,
            TextBaseline::Bottom,
            true,
        );
    }
}
