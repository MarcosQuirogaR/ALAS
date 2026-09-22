// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Inspect the linked PE resource tree, including every original ICO payload.
//!
//! Set ALAS_RESOURCE_EXE to inspect a separately packaged executable instead.
//! PE layout: https://learn.microsoft.com/en-us/windows/win32/debug/pe-format

#![cfg(windows)]
// This file is a test binary, so a failing expect or unwrap is the assertion
// failing rather than a library invariant breaking, and what it reports about
// the inspected resource tree belongs on the console with the test output.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stderr)]

use std::{env, fs};

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

struct Resources {
    exe: Vec<u8>,
    sections: usize,
    section_count: usize,
    root: usize,
}

impl Resources {
    fn load() -> Self {
        let path =
            env::var_os("ALAS_RESOURCE_EXE").unwrap_or_else(|| env!("CARGO_BIN_EXE_ALAS").into());
        let exe = fs::read(&path).expect("built application is readable");
        assert_eq!(&exe[..2], b"MZ");
        let pe = u32_at(&exe, 0x3c) as usize;
        assert_eq!(&exe[pe..pe + 4], b"PE\0\0");
        let optional = pe + 24;
        let directories = optional
            + match u16_at(&exe, optional) {
                0x10b => 96,
                0x20b => 112,
                _ => panic!("unsupported PE optional header"),
            };
        let resource_rva = u32_at(&exe, directories + 16);
        assert_ne!(resource_rva, 0, "application has no Windows resources");
        let mut result = Self {
            sections: optional + u16_at(&exe, pe + 20) as usize,
            section_count: u16_at(&exe, pe + 6) as usize,
            exe,
            root: 0,
        };
        result.root = result.offset(resource_rva);
        eprintln!("Inspecting resources in {}", path.to_string_lossy());
        result
    }

    fn offset(&self, rva: u32) -> usize {
        for index in 0..self.section_count {
            let section = self.sections + 40 * index;
            let start = u32_at(&self.exe, section + 12);
            let size = u32_at(&self.exe, section + 16);
            if rva >= start && rva - start < size {
                return (u32_at(&self.exe, section + 20) + rva - start) as usize;
            }
        }
        panic!("resource RVA has no file-backed section");
    }

    fn entries(&self, directory: usize) -> Vec<(u32, u32)> {
        let count =
            u16_at(&self.exe, directory + 12) as usize + u16_at(&self.exe, directory + 14) as usize;
        (0..count)
            .map(|i| {
                let entry = directory + 16 + 8 * i;
                (u32_at(&self.exe, entry), u32_at(&self.exe, entry + 4))
            })
            .collect()
    }

    fn data(&self, kind: u32, id: u32) -> &[u8] {
        let mut directory = self.root;
        for key in [kind, id] {
            let (_, child) = self
                .entries(directory)
                .into_iter()
                .find(|(name, _)| *name == key)
                .expect("required resource type/name exists");
            assert_ne!(child & 0x8000_0000, 0);
            directory = self.root + (child & 0x7fff_ffff) as usize;
        }
        let languages = self.entries(directory);
        assert_eq!(languages.len(), 1, "one neutral-language resource expected");
        let data = self.root + languages[0].1 as usize;
        let start = self.offset(u32_at(&self.exe, data));
        let length = u32_at(&self.exe, data + 4) as usize;
        &self.exe[start..start + length]
    }
}

#[test]
fn embedded_icon_preserves_every_source_image() {
    let pe = Resources::load();
    let source = include_bytes!("../../../app_icon.ico");
    let group = pe.data(14, 1); // RT_GROUP_ICON
    assert_eq!(&group[..6], &source[..6]);
    let count = u16_at(source, 4) as usize;
    assert!(count > 1, "application icon must contain multiple sizes");
    let mut sizes = Vec::new();
    for i in 0..count {
        let ico_entry = 6 + i * 16;
        let group_entry = 6 + i * 14;
        assert_eq!(
            &group[group_entry..group_entry + 4],
            &source[ico_entry..ico_entry + 4],
            "icon dimensions and palette must be retained"
        );
        // RC normalizes the source ICO's unspecified (zero) plane count to one.
        assert_eq!(
            u16_at(group, group_entry + 4),
            u16_at(source, ico_entry + 4).max(1)
        );
        assert_eq!(
            &group[group_entry + 6..group_entry + 12],
            &source[ico_entry + 6..ico_entry + 12],
            "icon colour depth and payload length must be retained"
        );
        let offset = u32_at(source, ico_entry + 12) as usize;
        let length = u32_at(source, ico_entry + 8) as usize;
        let id = u16_at(group, group_entry + 12) as u32;
        assert_eq!(pe.data(3, id), &source[offset..offset + length]); // RT_ICON
        let dimension = |value| if value == 0 { 256 } else { value as u16 };
        sizes.push((
            dimension(source[ico_entry]),
            dimension(source[ico_entry + 1]),
        ));
    }
    eprintln!("Embedded icon sizes (px), byte-identical payloads: {sizes:?}");
}

#[test]
fn version_resource_identifies_the_product_and_repository_metadata() {
    let pe = Resources::load();
    let version = pe.data(16, 1); // RT_VERSION
    assert_eq!(u16_at(version, 0) as usize, version.len());
    // VERSIONINFO consists of aligned UTF-16 keys/values and a fixed structure.
    let words: Vec<u16> = version.chunks_exact(2).map(|b| u16_at(b, 0)).collect();
    let text = String::from_utf16_lossy(&words);
    for (key, value) in [
        ("ProductName", "ALAS"),
        ("FileDescription", "ALAS"),
        ("OriginalFilename", "ALAS.exe"),
        ("CompanyName", env!("CARGO_PKG_AUTHORS")),
        ("LegalCopyright", env!("CARGO_PKG_AUTHORS")),
        ("Comments", env!("CARGO_PKG_LICENSE")),
        ("FileVersion", env!("CARGO_PKG_VERSION")),
        ("ProductVersion", env!("CARGO_PKG_VERSION")),
    ] {
        let after_key = text.split_once(&format!("{key}\0")).expect(key).1;
        assert!(
            after_key
                .trim_start_matches('\0')
                .starts_with(&format!("{value}\0")),
            "wrong {key}"
        );
        eprintln!("{key}: {value}");
    }
    let signature = 0xfeef04bdu32.to_le_bytes();
    let fixed = version
        .windows(4)
        .position(|b| b == signature)
        .expect("VS_FIXEDFILEINFO");
    let major: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();
    let minor: u32 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap();
    let patch: u32 = env!("CARGO_PKG_VERSION_PATCH").parse().unwrap();
    for offset in [8, 16] {
        assert_eq!(u32_at(version, fixed + offset), major << 16 | minor);
        assert_eq!(u32_at(version, fixed + offset + 4), patch << 16);
    }
}
