// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Seats of the row a plane cuts, ordered laterally.
fn seats_of_row<'a>(scene: &'a CabinScene, row: &'a SeatRow, floor_z_m: f64) -> Vec<SeatSlice> {
    let mut seats: Vec<SeatSlice> = scene
        .seats
        .iter()
        .filter(|seat| seat.row_id == row.id)
        .map(|seat| SeatSlice {
            y_m: seat.center_y_m,
            width_m: seat.width_m,
            floor_z_m,
            height_m: row.envelope.height_m,
            occupied: seat.occupied.unwrap_or(true),
        })
        .collect();
    seats.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
    seats
}

/// Overhead runs a plane cuts on one deck, with their exported profiles.
fn bins_of_deck(scene: &CabinScene, deck_id: &str, x_m: f64) -> Vec<BinSlice> {
    scene
        .overhead
        .runs
        .iter()
        .filter(|run| run.deck_id == deck_id && spans(&run.envelope, x_m))
        .map(|run| BinSlice {
            profile: run.profile_yz_m.iter().map(|p| [p.y, p.z]).collect(),
            kind: run.kind.clone(),
        })
        .collect()
}

/// Fit a cargo item's contour into its resolved envelope.
///
/// A registered ULD carries a normalized profile whose proportions are the
/// standard's, so it is mapped onto the envelope rather than replaced by a box:
/// an LD3's cut corner stays visible. The envelope itself is never resized to
/// make an item fit a hold, which is what makes an overflowing item a finding
/// instead of a smaller drawing.
fn cargo_ring(item: &CargoItem) -> (Ring, bool) {
    let envelope = &item.envelope;
    let Some(uld) = item.uld.as_ref() else {
        return (
            rectangle(
                envelope.center_y_m,
                envelope.width_m,
                envelope.center_z_m - envelope.height_m * 0.5,
                envelope.center_z_m + envelope.height_m * 0.5,
            ),
            false,
        );
    };
    // The resolved DeckItem retains the aircraft-frame lateral centre but not
    // the original loading orientation. Mirror the canonical contour for a
    // port-side item, matching the live 3D asset path; the source metadata
    // still controls whether a profile is mirrorable.
    let mirror_for_port = envelope.center_y_m < 0.0 && uld.mirrorable;
    let contour: Ring = uld
        .normalized_contour_yz
        .iter()
        .map(|p| [if mirror_for_port { -p.y } else { p.y }, p.z])
        .collect();
    if contour.len() < 3 {
        return (
            rectangle(
                envelope.center_y_m,
                envelope.width_m,
                envelope.center_z_m - envelope.height_m * 0.5,
                envelope.center_z_m + envelope.height_m * 0.5,
            ),
            false,
        );
    }
    let source = bounds(&contour);
    let (span_y, span_z) = (source[2] - source[0], source[3] - source[1]);
    if span_y <= 0.0 || span_z <= 0.0 {
        return (
            rectangle(
                envelope.center_y_m,
                envelope.width_m,
                envelope.center_z_m - envelope.height_m * 0.5,
                envelope.center_z_m + envelope.height_m * 0.5,
            ),
            false,
        );
    }
    let ring = contour
        .iter()
        .map(|p| {
            [
                envelope.center_y_m
                    + (p[0] - 0.5 * (source[0] + source[2])) / span_y * envelope.width_m,
                envelope.center_z_m
                    + (p[1] - 0.5 * (source[1] + source[3])) / span_z * envelope.height_m,
            ]
        })
        .collect();
    (ring, true)
}

/// Which aisles a standing figure fits in.
///
/// The figure is drawn at a fixed 1.75 m and is never squeezed: an aisle that
/// does not clear the liner, the seats and the overhead runs at full height
/// gets no occupant, and the caller records why.
fn occupant_positions(
    aisles: &[f64],
    floor_z_m: f64,
    liner: Option<&Ring>,
    seats: &[SeatSlice],
    bins: &[BinSlice],
) -> Vec<f64> {
    let Some(liner) = liner else {
        return Vec::new();
    };
    aisles.iter().copied().filter(|&y_m| {
        let figure = occupant_ring(y_m, floor_z_m);
        if !ring_within(&figure, liner) {
            return false;
        }
        let figure_bounds = bounds(&figure);
        let clear_of_seats = seats.iter().all(|seat| {
            !bounds_overlap(
                figure_bounds,
                bounds(&rectangle(
                    seat.y_m,
                    seat.width_m,
                    seat.floor_z_m,
                    seat.floor_z_m + seat.height_m,
                )),
                0.01,
            )
        });
        clear_of_seats
            && bins
                .iter()
                .all(|bin| !bounds_overlap(figure_bounds, bounds(&bin.profile), 0.01))
    })
    .collect()
}

/// The nearest nominal aperture on each side of each deck.
///
/// Used only when the drawn plane cuts none, so that a section still shows
/// where a window sits in the cross-section. The caller marks these projected.
fn nearest_windows(scene: &CabinScene, x_m: f64) -> Vec<&WindowAperture> {
    let mut chosen: Vec<&WindowAperture> = Vec::new();
    for deck in &scene.decks {
        for starboard in [false, true] {
            let nearest = scene
                .windows
                .apertures
                .iter()
                .filter(|window| window.deck_id == deck.id)
                .filter(|window| (window.center_yz_m.y > 0.0) == starboard)
                .min_by(|a, b| (a.x_m - x_m).abs().total_cmp(&(b.x_m - x_m).abs()));
            if let Some(window) = nearest {
                chosen.push(window);
            }
        }
    }
    chosen
}

/// Resolve one deck at a station.
fn slice_deck<'a>(
    scene: &'a CabinScene,
    deck: &'a ResolvedDeck,
    x_m: f64,
    liner: Option<&Ring>,
) -> DeckSlice<'a> {
    let row = scene
        .seat_rows
        .iter()
        .find(|row| row.deck_id == deck.id && spans(&row.envelope, x_m));
    let seats = row.map_or_else(Vec::new, |row| seats_of_row(scene, row, deck.floor_z_m));
    let bins = bins_of_deck(scene, &deck.id, x_m);
    let aisles = row.map_or_else(Vec::new, |row| aisle_centers(row, &seats));
    let occupants = occupant_positions(&aisles, deck.floor_z_m, liner, &seats, &bins);
    DeckSlice {
        deck,
        row,
        seats,
        bins,
        aisles,
        occupants,
    }
}

/// Clear distance between an overhead run's outboard face and the liner.
///
/// Measured at the height of the run's outboard top corner, on its own side.
/// A run that reaches or crosses the liner returns zero rather than a negative
/// gap; a crossing is a separate, louder finding.
fn bin_liner_gap(bin: &BinSlice, liner: &Ring) -> Option<f64> {
    let extent = bounds(&bin.profile);
    let outboard = if extent[2].abs() >= extent[0].abs() {
        extent[2]
    } else {
        extent[0]
    };
    let half = half_width_at(liner, extent[3], outboard)?;
    Some((half - outboard.abs()).max(0.0))
}

/// Report what the resolved geometry gets wrong, without changing it.
fn collect_findings(slice: &SectionSlice<'_>) -> Vec<String> {
    let mut findings = Vec::new();
    for deck in &slice.decks {
        let Some(liner) = slice.liner.as_ref() else {
            continue;
        };
        let escaping = deck
            .seats
            .iter()
            .filter(|seat| {
                !ring_within(
                    &rectangle(
                        seat.y_m,
                        seat.width_m,
                        seat.floor_z_m,
                        seat.floor_z_m + seat.height_m,
                    ),
                    liner,
                )
            })
            .count();
        if escaping > 0 {
            findings.push(format!(
                "{} solved seat envelope(s) on the {} deck cross the cabin liner",
                escaping, deck.deck.id
            ));
        }
        for bin in &deck.bins {
            if !ring_within(&bin.profile, liner) {
                findings.push(format!(
                    "an overhead run on the {} deck crosses the cabin liner",
                    deck.deck.id
                ));
                break;
            }
        }
        if let Some(gap) = deck
            .bins
            .iter()
            .filter_map(|bin| bin_liner_gap(bin, liner))
            .fold(None, |worst: Option<f64>, gap| {
                Some(worst.map_or(gap, |current: f64| current.max(gap)))
            })
        {
            if gap > BIN_ATTACHMENT_TOLERANCE_M {
                findings.push(format!(
                    "an overhead run on the {} deck stands {gap:.2} m clear of the liner with no exported attachment",
                    deck.deck.id
                ));
            }
        }
        if deck.deck.passenger && deck.row.is_none() {
            findings.push(format!(
                "no seat row is cut on the {} deck at this station",
                deck.deck.id
            ));
        }
        if deck.deck.passenger && !deck.aisles.is_empty() && deck.occupants.is_empty() {
            findings.push(format!(
                "no aisle on the {} deck clears an unscaled {:.2} m standing figure",
                deck.deck.id, OCCUPANT_HEIGHT_M
            ));
        }
    }
    if let Some(hold) = slice.hold.as_ref() {
        let escaping = slice
            .cargo
            .iter()
            .filter(|item| !ring_within(&item.ring, hold))
            .count();
        if escaping > 0 {
            findings.push(format!(
                "{escaping} cargo item(s) cross the hold liner at this station"
            ));
        }
    } else if !slice.cargo.is_empty() {
        findings.push("cargo is cut at a station with no hold contour".to_owned());
    }
    findings
}

/// Resolve everything one plane cuts, or nothing if the scene has no station
/// with an interior contour to cut.
fn slice_section(scene: &CabinScene) -> Option<SectionSlice<'_>> {
    let station = select_station(scene)?;
    let x_m = station.x_m;
    let liner = station
        .liner
        .as_ref()
        .or(station.inner.as_ref())
        .map(ring_of);
    let mut decks: Vec<&ResolvedDeck> = scene.decks.iter().collect();
    decks.sort_by(|a, b| b.floor_z_m.total_cmp(&a.floor_z_m));
    let decks: Vec<DeckSlice<'_>> = decks
        .into_iter()
        .map(|deck| slice_deck(scene, deck, x_m, liner.as_ref()))
        .collect();
    let cargo: Vec<CargoSlice> = scene
        .cargo
        .items
        .iter()
        .filter(|item| spans(&item.envelope, x_m))
        .map(|item| {
            let (ring, from_uld) = cargo_ring(item);
            CargoSlice {
                ring,
                label: item
                    .uld
                    .as_ref()
                    .map_or_else(|| "BULK".to_owned(), |uld| uld.code.clone()),
                mass_kg: item.mass_kg,
                from_uld,
            }
        })
        .collect();
    let cut: Vec<&WindowAperture> = scene
        .windows
        .apertures
        .iter()
        .filter(|window| (window.x_m - x_m).abs() <= window.width_m * 0.5)
        .collect();
    let windows_projected = cut.is_empty();
    let windows = if cut.is_empty() {
        nearest_windows(scene, x_m)
    } else {
        cut
    };
    let (decks_with_rows, cargo_flag, overhead_runs, _) = station_score(scene, x_m);
    let mut slice = SectionSlice {
        outer: ring_of(&station.outer),
        inner: station.inner.as_ref().map(ring_of),
        liner,
        hold: station.hold.as_ref().map(ring_of),
        decks,
        cargo,
        windows,
        windows_projected,
        choice: StationChoice {
            id: station.id.clone(),
            x_m,
            decks_with_rows,
            passenger_decks: scene.decks.iter().filter(|deck| deck.passenger).count(),
            cargo_items: cargo_flag,
            overhead_runs,
        },
        findings: Vec::new(),
    };
    slice.choice.cargo_items = slice.cargo.len();
    slice.findings = collect_findings(&slice);
    Some(slice)
}
