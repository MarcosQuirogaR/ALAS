// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for [`alas_report::theme`]: palettes, resolution, and color constants.
//!
//! Validates bitwise agreement of color hex strings against reference Python definitions.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_report::theme::{
    get_palette, BASELINE_COLOR, DEFAULT_THEME, GHOST_COLOR, OPTIMIZED_COLOR, PALETTE_DARK,
    PALETTE_GREY, PALETTE_LIGHT,
};
use alas_testkit::{Comparison, Tier};
use serde_json::Value;

#[test]
fn theme_palettes_and_resolution_match_reference() {
    let fixture: Value = alas_testkit::load("report", "theme");
    let mut check = Comparison::new("report/theme", Tier::Exact);

    // Verify predefined palettes
    let light = &fixture["palettes"]["light"];
    check.exact(
        "palettes.light.bg",
        &PALETTE_LIGHT.bg,
        &light["bg"].as_str().unwrap(),
    );
    check.exact(
        "palettes.light.title",
        &PALETTE_LIGHT.title,
        &light["title"].as_str().unwrap(),
    );
    check.exact(
        "palettes.light.accent",
        &PALETTE_LIGHT.accent,
        &light["accent"].as_str().unwrap(),
    );

    let dark = &fixture["palettes"]["dark"];
    check.exact(
        "palettes.dark.bg",
        &PALETTE_DARK.bg,
        &dark["bg"].as_str().unwrap(),
    );
    check.exact(
        "palettes.dark.title",
        &PALETTE_DARK.title,
        &dark["title"].as_str().unwrap(),
    );

    let grey = &fixture["palettes"]["grey"];
    check.exact(
        "palettes.grey.bg",
        &PALETTE_GREY.bg,
        &grey["bg"].as_str().unwrap(),
    );

    // Verify resolved lookups
    check.exact(
        "resolved.none.name",
        &get_palette(None).name,
        &fixture["resolved"]["none"]["name"].as_str().unwrap(),
    );
    check.exact(
        "resolved.dark.name",
        &get_palette(Some("dark")).name,
        &fixture["resolved"]["dark"]["name"].as_str().unwrap(),
    );
    check.exact(
        "resolved.unknown.name",
        &get_palette(Some("nonexistent")).name,
        &fixture["resolved"]["unknown"]["name"].as_str().unwrap(),
    );

    // Verify constants
    check.exact(
        "constants.default_theme",
        &DEFAULT_THEME,
        &fixture["constants"]["default_theme"].as_str().unwrap(),
    );
    check.exact(
        "constants.baseline_color",
        &BASELINE_COLOR,
        &fixture["constants"]["baseline_color"].as_str().unwrap(),
    );
    check.exact(
        "constants.optimized_color",
        &OPTIMIZED_COLOR,
        &fixture["constants"]["optimized_color"].as_str().unwrap(),
    );
    check.exact(
        "constants.ghost_color",
        &GHOST_COLOR,
        &fixture["constants"]["ghost_color"].as_str().unwrap(),
    );

    check.finish();
}
