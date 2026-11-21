// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every registered preset builds the same main wing, tail and fuselages (a wingtip device, wings[3], is not hashed).
//!
//! The pinned values are FNV-1a 64 hashes of the `Debug` text of the built
//! wings and fuselages (flight shape, engines included), with every
//! floating-point literal of the text rounded to six significant digits (MSVC
//! and glibc differ in the last bits of sin/cos/pow). They must change only
//! with an intended preset geometry change, never with the device code.
//! `ALAS_PRINT_GEOMETRY_HASHES=1` prints them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;

/// Rewrites every floating-point literal of a `Debug` text to six significant digits (values below
/// 1e-9 in magnitude to zero), so the hash does not depend on the last bits of `sin`/`cos`/`powf`:
/// MSVC and glibc differ by up to 1e-10 relative in the fitted-spline outputs.
fn rounded_floats(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let after_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        let starts_number = !after_word
            && (c.is_ascii_digit()
                || (c == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)));
        if !starts_number {
            let width = text[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&text[i..i + width]);
            i += width;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() {
            let d = bytes[j];
            let exponent_sign = (d == b'-' || d == b'+') && matches!(bytes[j - 1], b'e' | b'E');
            if d.is_ascii_digit() || d == b'.' || d == b'e' || d == b'E' || exponent_sign {
                j += 1;
            } else {
                break;
            }
        }
        let token = &text[i..j];
        match token.parse::<f64>() {
            Ok(value) if token.contains(['.', 'e', 'E']) => {
                if value.abs() < 1.0e-9 {
                    out.push_str("0.0");
                } else {
                    out.push_str(&format!("{value:.5e}"));
                }
            }
            _ => out.push_str(token),
        }
        i = j;
    }
    out
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

const PINNED: &[(&str, u64)] = &[
    ("AVE", 17188348303707019953),
    ("A340-300", 8943492922135040493),
    ("A380-800", 1234884715865935970),
    ("B787-9", 15285136624471078292),
    ("A320-200", 15429497400857214482),
    ("A220-300", 2050431395552750819),
    ("ATR72-600", 326942642925229747),
    ("DC-10", 553469525261035396),
    ("E195-E2", 13413440671489730707),
    ("C919", 14130148864216987288),
    ("B747-400", 12921993988550395705),
    ("A400M", 17318306471280094299),
    ("A350-900", 585306279258708281),
    ("LJ60", 13167169628244638114),
    ("C295", 13593942879527161542),
];

#[test]
fn presets_without_a_wingtip_device_are_bit_identical() {
    let print = std::env::var_os("ALAS_PRINT_GEOMETRY_HASHES").is_some();
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        // a preset with a real device (sharklet, winglet, fence) builds it as a fourth surface.
        // The tail fixtures (dorsal fillet, pod struts and fairing, ventral fins) are always built.
        let fixtures = plane
            .wings
            .iter()
            .skip(3)
            .filter(|wing| {
                [
                    "Dorsal Fin",
                    "Centre Pod Strut",
                    "Fin Aft Fairing",
                    "Ventral Fin",
                ]
                .iter()
                .any(|prefix| wing.name.starts_with(prefix))
            })
            .count();
        let surfaces = 3 + fixtures + usize::from(preset.geometry.wing.wingtip_device.is_some());
        assert_eq!(plane.wings.len(), surfaces, "{name}");
        let text = rounded_floats(&format!("{:?}{:?}", plane.wings, plane.fuselages));
        if let Some(dir) = std::env::var_os("ALAS_DUMP_GEOMETRY_TEXT") {
            std::fs::write(
                std::path::Path::new(&dir).join(format!("{name}.txt")),
                &text,
            )
            .unwrap();
        }
        let hash = fnv1a(&text);
        if print {
            println!("HASH {name} {hash}");
        }
        if !print {
            let key = name.to_string();
            let pinned = PINNED
                .iter()
                .find(|entry| entry.0 == key)
                .map(|entry| entry.1);
            assert_eq!(pinned, Some(hash), "{name}: geometry hash moved");
        }
    }
}
