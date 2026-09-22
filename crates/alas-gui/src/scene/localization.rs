// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Display-bound localization for report scenes with generated text.

pub(super) fn localize_scene_text(text: &str) -> String {
    let exact = alas_i18n::t(Some(text), None);
    if exact != text {
        return exact.into_owned();
    }
    if let Some((heading, detail)) = text.split_once('\n') {
        let localized_heading = alas_i18n::t(Some(heading), None);
        let localized_detail = alas_i18n::t(Some(detail), None);
        if localized_heading != heading || localized_detail != detail {
            return format!("{localized_heading}\n{localized_detail}");
        }
    }
    for label in ["Physical CG", "Aero CG"] {
        if let Some(value) = text.strip_prefix(label) {
            return format!("{}{}", alas_i18n::t(Some(label), None), value);
        }
    }
    for (prefix, key) in [
        ("Airfoil Reynolds Sweep: ", "Airfoil Reynolds Sweep: "),
        ("MSES Mach Field (alpha = ", "MSES Mach Field (alpha = "),
        ("Tank capacity: ", "Tank capacity: "),
        ("Required fuel: ", "Required fuel: "),
        ("sufficient", "sufficient"),
        ("INSUFFICIENT", "INSUFFICIENT"),
    ] {
        if let Some(value) = text.strip_prefix(prefix) {
            let translated = alas_i18n::t(Some(key), None);
            if translated != key {
                return format!("{translated}{value}");
            }
        }
    }
    if text.contains("target ") || text.contains("[OK]") || text.contains("[OUT OF RANGE]") {
        return text
            .replace("target ", &alas_i18n::t(Some("target "), None))
            .replace("[OK]", &format!("[{}]", alas_i18n::t(Some("OK"), None)))
            .replace(
                "[OUT OF RANGE]",
                &format!("[{}]", alas_i18n::t(Some("OUT OF RANGE"), None)),
            );
    }
    if let Some(value) = text.strip_prefix("Mode ") {
        return format!("{} {value}", alas_i18n::t(Some("Mode"), None));
    }
    for (prefix, key) in [
        ("Wingbox planform", "Wingbox planform"),
        ("FEM vs Torenbeek wing mass", "FEM vs Torenbeek wing mass"),
        ("Lift curve", "Lift curve"),
        ("Spar x/c=", "Spar x/c="),
        ("Engine:", "Engine:"),
        ("Design point:", "Design point:"),
        ("Specific thrust", "Specific thrust"),
        ("Thermal efficiency", "Thermal efficiency"),
        ("Propulsive efficiency", "Propulsive efficiency"),
        ("Overall efficiency", "Overall efficiency"),
        (
            "Per-engine thrust, static (rated)",
            "Per-engine thrust, static (rated)",
        ),
        (
            "Per-engine thrust, this cruise pt",
            "Per-engine thrust, this cruise pt",
        ),
        ("Total installed thrust", "Total installed thrust"),
    ] {
        if let Some(value) = text.strip_prefix(prefix) {
            let translated = alas_i18n::t(Some(key), None);
            if translated != key {
                return format!("{translated}{value}");
            }
        }
    }
    let trimmed = text.trim_start();
    let indent = &text[..text.len() - trimmed.len()];
    if let Some((prefix, value)) = trimmed.split_once(": ") {
        let key = format!("{prefix}: ");
        let translated = alas_i18n::t(Some(&key), None);
        if translated != key {
            return format!("{indent}{translated}{value}");
        }
    }
    if let Some(value) = text.strip_prefix("Landing Gear Planform: ") {
        let translated = alas_i18n::t(Some("Landing Gear Planform: "), None);
        if translated != "Landing Gear Planform: " {
            return format!("{translated}{value}");
        }
    }
    if let Some(name) = text.strip_suffix(" (current)") {
        return format!("{} ({})", name, alas_i18n::t(Some("current"), None));
    }
    if let Some(cl) = text
        .strip_prefix("Drag vs Mach (illustrative, CL=")
        .and_then(|value| value.strip_suffix(')'))
    {
        return alas_i18n::t(Some("Drag vs Mach (illustrative, CL={cl})"), None)
            .replace("{cl}", cl);
    }
    if let Some((count, mass)) = text
        .strip_suffix(" t")
        .and_then(|value| value.split_once(" seats, "))
    {
        return alas_i18n::t(Some("{count} seats, {mass} t"), None)
            .replace("{count}", count)
            .replace("{mass}", mass);
    }
    if let Some((count, mass)) = text
        .strip_suffix(" t")
        .and_then(|value| value.split_once(" ULD, "))
    {
        return alas_i18n::t(Some("{count} ULD, {mass} t"), None)
            .replace("{count}", count)
            .replace("{mass}", mass);
    }
    localize_route_footer(text).unwrap_or_else(|| text.to_owned())
}

fn localize_route_footer(text: &str) -> Option<String> {
    let (waypoints, remainder) = text.split_once(" waypoints | ")?;
    let (distance_km, description) = remainder.split_once(" km | ")?;
    if let Some(profile) = description.strip_prefix("flown profile: ") {
        if let Some((masses, altitudes)) = profile.split_once(", ") {
            let (start_mass, end_mass) = masses.split_once(" kg to ")?;
            let end_mass = end_mass.strip_suffix(" kg")?;
            let (start_altitude, end_altitude) =
                altitudes.strip_suffix(" m")?.split_once(" to ")?;
            return Some(
                alas_i18n::t(Some("{waypoints} waypoints | {distance_km} km | flown profile: {start_mass} kg to {end_mass} kg, {start_altitude} to {end_altitude} m"), None)
                    .replace("{waypoints}", waypoints)
                    .replace("{distance_km}", distance_km)
                    .replace("{start_mass}", start_mass)
                    .replace("{end_mass}", end_mass)
                    .replace("{start_altitude}", start_altitude)
                    .replace("{end_altitude}", end_altitude),
            );
        }
    }
    if description == "lateral route only (mission telemetry unavailable)" {
        return Some(
            alas_i18n::t(Some("{waypoints} waypoints | {distance_km} km | lateral route only (mission telemetry unavailable)"), None)
                .replace("{waypoints}", waypoints)
                .replace("{distance_km}", distance_km),
        );
    }
    let (profile_note, orbit_note) = description.rsplit_once(" | ")?;
    if orbit_note != "orthographic globe; drag to orbit; wheel zoom in fullscreen" {
        return None;
    }
    Some(
        alas_i18n::t(Some("{waypoints} waypoints | {distance_km} km | {profile_note} | orthographic globe; drag to orbit; wheel zoom in fullscreen"), None)
            .replace("{waypoints}", waypoints)
            .replace("{distance_km}", distance_km)
            .replace("{profile_note}", &alas_i18n::t(Some(profile_note), None)),
    )
}
