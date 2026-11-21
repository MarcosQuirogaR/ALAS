// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Display-bound localization for report scenes with generated text.

/// A generated physical-feasibility finding as displayed: a template match,
/// else the catalog entry of the whole message (the text itself when none).
pub(crate) fn localize_finding_message(text: &str) -> String {
    localize_template(text).unwrap_or_else(|| alas_i18n::t(Some(text), None).into_owned())
}

/// The dynamic "no feasible design" line: fixed words translate, the
/// constraint identifiers, numbers and units pass through.
pub(crate) fn localize_infeasibility(text: &str) -> Option<String> {
    let rest = text.strip_prefix("No feasible design: ")?;
    let mut line = format!("{}{rest}", alas_i18n::t(Some("No feasible design: "), None));
    for fixed in [" against ", " candidates)", "; rejections: "] {
        let translated = alas_i18n::t(Some(fixed), None);
        if translated != fixed {
            line = line.replace(fixed, &translated);
        }
    }
    Some(line)
}

pub(super) fn localize_scene_text(text: &str) -> String {
    if let Some(line) = localize_infeasibility(text) {
        return line;
    }
    let exact = alas_i18n::t(Some(text), None);
    if exact != text {
        return exact.into_owned();
    }
    if let Some(localized) = localize_template(text) {
        return localized;
    }
    // The balance chart's limit-line labels, "tip-back 35.0 to 37.4 %MAC", and
    // their gate-table rows, "Not governing: rotation -12.0 %MAC".
    if let Some(rest) = text.strip_prefix("Not governing: ") {
        let lead = alas_i18n::t(Some("Not governing: "), None);
        if lead != "Not governing: " {
            return format!("{lead}{}", localize_scene_text(rest));
        }
    }
    if let Some(label) = localize_cg_line_label(text) {
        return label;
    }
    if let Some(localized) = localize_cabin_section(text) {
        return localized;
    }
    if let Some(localized) = localize_bracketed_unit(text) {
        return localized;
    }
    // A counted legend label, "Valid (391)": the name translates, the count
    // passes through.
    if let Some((label, count)) = text
        .strip_suffix(')')
        .and_then(|value| value.rsplit_once(" ("))
        .filter(|(_, count)| !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()))
    {
        let translated = alas_i18n::t(Some(label), None);
        if translated != label {
            return format!("{translated} ({count})");
        }
    }
    // The mass labels' role names, alone (the two-line label) or followed by
    // the mass.
    let start = text
        .find(|character: char| character.is_ascii_digit())
        .unwrap_or(text.len());
    if start > 0 {
        let labels: Vec<_> = text[..start].trim_end().split(" / ").collect();
        if labels.iter().all(|label| {
            matches!(
                *label,
                "Analyzed ZFW" | "Sized TOW" | "Analyzed TOW" | "Design LW"
            )
        }) {
            let roles = labels
                .iter()
                .map(|label| alas_i18n::t(Some(label), None).into_owned())
                .collect::<Vec<_>>()
                .join(" / ");
            return if text[start..].is_empty() {
                roles
            } else {
                format!("{roles} {}", &text[start..])
            };
        }
    }
    // A sheet title, "<aircraft> - <sheet name>": the aircraft name is a
    // proper name and passes through.
    if let Some((name, sheet)) = text.split_once(" \u{2013} ") {
        let translated = alas_i18n::t(Some(sheet), None);
        if translated != sheet {
            return format!("{name} \u{2013} {translated}");
        }
    }
    if let Some((heading, detail)) = text.split_once('\n') {
        let localized_heading = alas_i18n::t(Some(heading), None);
        let localized_detail = alas_i18n::t(Some(detail), None);
        if localized_heading != heading || localized_detail != detail {
            return format!("{localized_heading}\n{localized_detail}");
        }
    }
    // A freight label, "463L pallet · 2,100 kg": the item name, then the mass.
    if let Some((name, mass)) = text.split_once(" \u{b7} ") {
        if let (Some(code), Some(number)) = (name.strip_suffix(" pallet"), mass.strip_suffix(" kg"))
        {
            if number.chars().all(|c| c.is_ascii_digit() || c == ',') {
                let pallet = alas_i18n::t(Some("pallet"), None);
                return format!("{pallet} {code} \u{b7} {number} kg");
            }
        }
    }
    // The forward-limit flag: "forward limit -3 %MAC (ahead of LEMAC)".
    if let Some(rest) = text.strip_prefix("forward limit ") {
        if let Some(value) = rest.strip_suffix(" %MAC (ahead of LEMAC)") {
            let lead = alas_i18n::t(Some("forward limit "), None);
            let tail = alas_i18n::t(Some(" %MAC (ahead of LEMAC)"), None);
            return format!("{lead}{value}{tail}");
        }
    }
    if let Some(rest) = text.strip_prefix("Design point violates:\n") {
        let heading = alas_i18n::t(Some("Design point violates:"), None);
        let lines = rest
            .lines()
            .map(|line| {
                for key in ["cruise T/W ", "OEI climb T/W ", "take-off ", "landing W/S "] {
                    if let Some(tail) = line.strip_prefix(key) {
                        return format!("{}{tail}", alas_i18n::t(Some(key), None));
                    }
                }
                line.to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");
        return format!("{heading}\n{lines}");
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
        ("Trimmed cruise: ", "Trimmed cruise: "),
        ("Loaded takeoff mass = ", "Loaded takeoff mass = "),
        ("Takeoff mass = MTOW = ", "Takeoff mass = MTOW = "),
        (
            "Symmetric-log axes, linear within ",
            "Symmetric-log axes, linear within ",
        ),
        ("INSUFFICIENT", "INSUFFICIENT"),
        ("Run CLmax ", "Run CLmax "),
        (
            "Design CL is outside the swept polar: bars are the nearest sample, CL ",
            "Design CL is outside the swept polar: bars are the nearest sample, CL ",
        ),
        ("extrapolated α0L = ", "extrapolated α0L = "),
        ("TODR margin to TODA: ", "TODR margin to TODA: "),
        ("Lift split: ", "Lift split: "),
        ("Off L/D peak: design CL ", "Off L/D peak: design CL "),
        ("tip  b/2 = ", "tip  b/2 = "),
        ("CD wave off scale, up to ", "CD wave off scale, up to "),
        ("CD0 off scale, up to ", "CD0 off scale, up to "),
        (
            "No valid trimmed cruise state (",
            "No valid trimmed cruise state (",
        ),
    ] {
        if let Some(value) = text.strip_prefix(prefix) {
            let translated = alas_i18n::t(Some(key), None);
            if translated != key {
                return format!("{translated}{value}");
            }
        }
    }
    // The landing-gear summary line: fixed words around numbers.
    if text.starts_with("Tip-back: ") && text.contains("Tire margin nose/main: ") {
        let mut line = text.to_owned();
        for word in [
            "Tip-back:",
            "Tire margin nose/main:",
            "BELOW LIMIT",
            "BELOW DESIGN FACTOR",
            "OVERLOADED",
            "OK",
        ] {
            let translated = alas_i18n::t(Some(word), None);
            if translated != word {
                line = replace_whole_word(&line, word, &translated);
            }
        }
        // "; basis A / B": translate the rating-basis words as one unit.
        if let Some((head, tail)) = line.split_once("; basis ") {
            let words: Vec<String> = tail
                .split(" / ")
                .map(|word| {
                    let key = format!("basis {word}");
                    let translated = alas_i18n::t(Some(&key), None);
                    if translated == key {
                        word.to_owned()
                    } else {
                        translated.into_owned()
                    }
                })
                .collect();
            let label = alas_i18n::t(Some("; basis "), None);
            line = format!("{head}{label}{}", words.join(" / "));
        }
        return line;
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
            // A value that is itself a catalog phrase (an outcome or a
            // termination reason) is translated; numbers and units pass
            // through unchanged.
            let value = localize_infeasibility(value)
                .unwrap_or_else(|| alas_i18n::t(Some(value), None).into_owned());
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
    if let Some((count, mass)) = text
        .strip_suffix(" t")
        .and_then(|value| value.split_once(" 463L pallet, "))
    {
        return alas_i18n::t(Some("{count} 463L pallet, {mass} t"), None)
            .replace("{count}", count)
            .replace("{mass}", mass);
    }
    localize_tool_card(text)
        .or_else(|| localize_mode_pairing(text))
        .or_else(|| localize_local_mode(text))
        .or_else(|| localize_phrases(text))
        .or_else(|| localize_route_footer(text))
        .unwrap_or_else(|| text.to_owned())
}

/// Fixed phrases inside generated captions and notes, longest first. Each is a
/// catalog key; the numbers around it pass through.
const SCENE_PHRASES: &[&str] = &[
    " - route too short to reach the configured cruise altitude",
    " - route too short to reach the mission-spec cruise altitude",
    " - peak exceeds the configured cruise altitude",
    " - peak exceeds the mission-spec cruise altitude",
    " ft vs configured cruise ",
    " ft vs mission-spec cruise ",
    "Peak altitude ",
    ": corner in SFn (cycle limit or solver branch switch), not a trend.",
    "Dotted beyond BPR ",
    "Design markers (cycle solved at the engine's own BPR) sit ",
    " (SFn) and ",
    " (TSFC) from the sweep curves at the same BPR.",
    ", extrapolated: off the carpet grid",
    "Design point (SFn=",
    " (paired by frequency only: no mode shapes, unverified)",
    "Cruise (M",
];

/// Replace `word` by `with` where it stands alone, not inside a longer word.
fn replace_whole_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = &rest[at + word.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&rest[..at]);
        out.push_str(if before_ok && after_ok { with } else { word });
        rest = after;
    }
    out.push_str(rest);
    out
}

fn localize_phrases(text: &str) -> Option<String> {
    let mut out = text.to_owned();
    let mut changed = false;
    for phrase in SCENE_PHRASES {
        if out.contains(phrase) {
            let translated = alas_i18n::t(Some(phrase), None);
            if translated != *phrase {
                out = out.replace(phrase, &translated);
                changed = true;
            }
        }
    }
    changed.then_some(out)
}

/// Generated sentences with numbers or backend text inside. Each is a catalog
/// key whose `{name}` placeholders take the text found at that position.
const SCENE_TEMPLATES: &[&str] = &[
    "No feasible CG range: forward limit aft of aft limit ({lo} to {hi} kg)",
    "Worst at {mass} kg: forward {fwd} %MAC (limit set: {fwd_set}), aft {aft} %MAC (limit set: {aft_set})",
    "{n} grid point(s) masked (faint hollow markers): specific thrust fell more than 20 % below its running maximum along the OPR sweep (backward branch, not a valid operating point). SFn of the masked points: {lo} to {hi} m/s. {k} of them fall outside the plotted range and are not drawn.",
    "{n} grid point(s) masked (faint hollow markers): specific thrust fell more than 20 % below its running maximum along the OPR sweep (backward branch, not a valid operating point). SFn of the masked points: {lo} to {hi} m/s.",
    "Hatched: Re {lo} to {hi} ({n} cells per map) is not drawn, unreliable low-Re surrogate range. The colour scale uses only the drawn cells.",
    "{n} of {total} cruise points dropped: only points within 150 m of the highest cruise altitude flown are shown.",
    "No sine-sweep or random-vibration result exists for this design. Reported status: {status}",
    "Set its executable under Workspace > External Tools to render deformation plots. Reported status: {status}",
    "NASTRAN SOL 103 is not configured: {status}",
    "NASTRAN-95 SOL 103 is not configured: {status}",
    "TOW limited at {icao}: {limiting} limit the take-off to {limit_kg} kg of the {mission_kg} kg mission mass; leave {reduction_kg} kg of payload or fuel behind. An operating limitation, not a design violation: the sea-level ISA certification rows are reported separately",
    "{file}: file not found",
    "{file}: permission denied",
    "{file}: could not be read",
];

/// An axis label "Label [unit]": the label translates, the unit passes through.
fn localize_bracketed_unit(text: &str) -> Option<String> {
    let (label, unit) = text.strip_suffix(']')?.rsplit_once(" [")?;
    let key = label.trim_end();
    let translated = alas_i18n::t(Some(key), None);
    (translated != key).then(|| format!("{translated}{} [{unit}]", &label[key.len()..]))
}

/// Split `template` at its `{name}` placeholders and read the values out of
/// `text`; `None` when the fixed words around them do not match.
fn match_template<'a>(text: &'a str, template: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    let mut rest = text;
    let mut tail = template;
    let mut values = Vec::new();
    while let Some(open) = tail.find('{') {
        rest = rest.strip_prefix(&tail[..open])?;
        let close = open + tail[open..].find('}')?;
        let name = &tail[open + 1..close];
        tail = &tail[close + 1..];
        let next_literal = tail.find('{').map_or(tail, |at| &tail[..at]);
        let end = if next_literal.is_empty() {
            rest.len()
        } else {
            rest.find(next_literal)?
        };
        let value = &rest[..end];
        if value.is_empty() {
            return None;
        }
        values.push((name, value));
        rest = &rest[end..];
    }
    (rest == tail).then_some(values)
}

/// A placeholder value: a catalog phrase (a limit-set name, a status) is
/// translated, and a number or unknown text passes through.
fn localize_template_value(value: &str) -> String {
    let whole = alas_i18n::t(Some(value), None);
    if whole != value {
        return whole.into_owned();
    }
    value
        .split(" / ")
        .map(|part| alas_i18n::t(Some(part), None).into_owned())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn localize_template(text: &str) -> Option<String> {
    for template in SCENE_TEMPLATES {
        let Some(values) = match_template(text, template) else {
            continue;
        };
        let translated = alas_i18n::t(Some(template), None);
        if translated == *template {
            return None;
        }
        let mut out = translated.into_owned();
        for (name, value) in values {
            out = out.replace(&format!("{{{name}}}"), &localize_template_value(value));
        }
        return Some(out);
    }
    None
}

/// "<tool> not configured. <detail>": the neutral card for an optional tool.
fn localize_tool_card(text: &str) -> Option<String> {
    let (tool, detail) = text.split_once(" not configured. ")?;
    let marker = alas_i18n::t(Some(" not configured. "), None);
    let detail =
        localize_template(detail).unwrap_or_else(|| alas_i18n::t(Some(detail), None).into_owned());
    Some(format!("{tool}{marker}{detail}"))
}

/// "<solver> <f> Hz: local / chordwise, no beam counterpart (MAC x)" and
/// "<solver> <f> Hz: torsion-dominated, twist*c/2 / w = r (MAC x)".
fn localize_local_mode(text: &str) -> Option<String> {
    if let Some((head, rest)) = text.split_once(": local / chordwise, no beam counterpart (MAC ") {
        let mac = rest.strip_suffix(')')?;
        let template = alas_i18n::t(
            Some(": local / chordwise, no beam counterpart (MAC {mac})"),
            None,
        );
        return Some(format!("{head}{}", template.replace("{mac}", mac)));
    }
    let (head, rest) = text.split_once(": torsion-dominated, twist*c/2 / w = ")?;
    let (ratio, mac) = rest.split_once(" (MAC ")?;
    let mac = mac.strip_suffix(')')?;
    let template = alas_i18n::t(
        Some(": torsion-dominated, twist*c/2 / w = {r} (MAC {mac})"),
        None,
    );
    Some(format!(
        "{head}{}",
        template.replace("{r}", ratio).replace("{mac}", mac)
    ))
}

/// "<solver> (paired by MAC, all N >= X)" and "<solver> (paired by MAC, K of N below X)".
fn localize_mode_pairing(text: &str) -> Option<String> {
    let (solver, rest) = text.split_once(" (paired by MAC, ")?;
    let rest = rest.strip_suffix(')')?;
    if let Some(tail) = rest.strip_prefix("all ") {
        let (count, threshold) = tail.split_once(" >= ")?;
        let template = alas_i18n::t(Some(" (paired by MAC, all {n} >= {mac})"), None);
        return Some(format!(
            "{solver}{}",
            template.replace("{n}", count).replace("{mac}", threshold)
        ));
    }
    let (failing, tail) = rest.split_once(" of ")?;
    let (count, threshold) = tail.split_once(" below ")?;
    let template = alas_i18n::t(Some(" (paired by MAC, {failing} of {n} below {mac})"), None);
    Some(format!(
        "{solver}{}",
        template
            .replace("{failing}", failing)
            .replace("{n}", count)
            .replace("{mac}", threshold)
    ))
}

fn localize_route_footer(text: &str) -> Option<String> {
    let (waypoints, remainder) = text.split_once(" waypoints | ")?;
    let (distance_km, description) = remainder.split_once(" km | ")?;
    if let Some(profile) = description.strip_prefix("flown profile: ") {
        if let Some((masses, peak)) = profile.split_once(", peak altitude ") {
            // The flown profile's highest point: its first and last altitudes
            // are only the airport elevations.
            let (start_mass, end_mass) = masses.split_once(" kg to ")?;
            let end_mass = end_mass.strip_suffix(" kg")?;
            let peak = peak.strip_suffix(" m")?;
            return Some(
                alas_i18n::t(Some("{waypoints} waypoints | {distance_km} km | flown profile: {start_mass} kg to {end_mass} kg, peak altitude {peak_altitude} m"), None)
                    .replace("{waypoints}", waypoints)
                    .replace("{distance_km}", distance_km)
                    .replace("{start_mass}", start_mass)
                    .replace("{end_mass}", end_mass)
                    .replace("{peak_altitude}", peak),
            );
        }
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

/// The chart's limit-line names (`alas_report` `Mechanism::label`).
const CG_LINE_NAMES: [&str; 7] = [
    "nose-gear load",
    "rotation",
    "clean trim",
    "landing trim",
    "static margin",
    "min nose load",
    "tip-back",
];

/// A limit-line label, `"<name> <value> %MAC"` or
/// `"<name> <low> to <high> %MAC"`: the name and the word "to" translate, the
/// numbers pass through.
fn localize_cg_line_label(text: &str) -> Option<String> {
    let body = text.strip_suffix(" %MAC")?;
    CG_LINE_NAMES.into_iter().find_map(|name| {
        let translated = alas_i18n::t(Some(name), None);
        if translated == name {
            return None;
        }
        let numbers = body.strip_prefix(name)?.strip_prefix(' ')?;
        if !numbers
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | ' ' | 't' | 'o'))
        {
            return None;
        }
        let to = alas_i18n::t(Some(" to "), None);
        Some(format!(
            "{} {} %CMA",
            alas_i18n::t(Some(name), None),
            numbers.replace(" to ", &to)
        ))
    })
}

/// The cabin-section figure's generated lines: the catalog key is the English
/// template, with `{name}` where a number goes.
const CABIN_SECTION_TEMPLATES: [&str; 5] = [
    "WARNING: {escaping} of {seats} seats in this row do not fit inside the inner contour at the drawn floor height.",
    "WARNING: {escaping} of {freight} freight items at this station stand outside the inner contour at the floor.",
    "Representative station x = {station} m | outer section {width} x {height} m | lining {lining_width} x {lining_height} m",
    "Representative station x = {station} m | {class} row | outer section {width} x {height} m | lining {lining_width} x {lining_height} m",
    "Model floor z = {floor} m; lining keel z = {keel} m, crown z = {crown} m; chord at the floor {chord} m (at least {minimum} m needed)",
];

/// A piece of a template: fixed words or a named value.
enum TemplatePart<'a> {
    Words(&'a str),
    Value(&'a str),
}

fn template_parts(template: &str) -> Vec<TemplatePart<'_>> {
    let mut parts = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        if open > 0 {
            parts.push(TemplatePart::Words(&rest[..open]));
        }
        parts.push(TemplatePart::Value(&rest[open + 1..open + close]));
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        parts.push(TemplatePart::Words(rest));
    }
    parts
}

/// The values of `text` read against `template`, or `None` when it does not match.
fn match_cabin_template<'a>(text: &'a str, template: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    let parts = template_parts(template);
    let mut values = Vec::new();
    let mut rest = text;
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            TemplatePart::Words(words) => {
                rest = rest.strip_prefix(words)?;
            }
            TemplatePart::Value(name) => {
                let end = match parts.get(index + 1) {
                    Some(TemplatePart::Words(next)) => rest.find(next)?,
                    _ => rest.len(),
                };
                values.push((name, &rest[..end]));
                rest = &rest[end..];
            }
        }
        index += 1;
    }
    rest.is_empty().then_some(values)
}

/// The Spanish form of one of [`CABIN_SECTION_TEMPLATES`] with its values filled in.
fn localize_cabin_section(text: &str) -> Option<String> {
    for template in CABIN_SECTION_TEMPLATES {
        let Some(values) = match_cabin_template(text, template) else {
            continue;
        };
        let translated = alas_i18n::t(Some(template), None);
        if translated == template {
            continue;
        }
        let mut out = translated.into_owned();
        for (name, value) in values {
            let value = if name == "class" {
                alas_i18n::t(Some(value), None).into_owned()
            } else {
                value.to_owned()
            };
            out = out.replace(&format!("{{{name}}}"), &value);
        }
        return Some(out);
    }
    None
}
