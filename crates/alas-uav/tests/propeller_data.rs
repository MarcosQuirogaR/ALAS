// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! PER3 parser, local data directory, import, and polite-download policy.
//!
//! Every table used here is the SYNTHETIC text in `fixtures/prop_data`
//! (made-up numbers).  Nothing in this file touches the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use alas_uav::collector::Sleeper;
use alas_uav::propulsion_electric::parse_per3;
use alas_uav::propulsion_electric::prop_data::{
    import_prop_data, import_prop_folder, import_prop_zip, read_directory, store_per3_file,
};
use alas_uav::propulsion_electric::prop_fetch::{
    fetch_prop_tables, PropFetchError, PropFetchOptions, PropHttpGet, PropSelection,
    PROP_INDEX_URL, PROP_ROBOTS_URL, PROP_USER_AGENT,
};

const T7X4E: &str = include_str!("fixtures/prop_data/PER3_7x4E.dat");
const T9X45: &str = include_str!("fixtures/prop_data/PER3_9x45EP(F2B).dat");
const T12X6E: &str = include_str!("fixtures/prop_data/PER3_12x6E.dat");

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("alas-prop-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn per3_title_name_and_samples_become_a_bounded_map() {
    let table = parse_per3(T7X4E, "PER3_7x4E.dat").unwrap();
    assert_eq!(table.propeller_id(), "apc-7x4e");
    assert_eq!(table.model(), "7x4E");
    assert_eq!(table.source_file(), "PER3_7x4E.dat");
    assert!((table.diameter_m() - 7.0 * 0.0254).abs() < 1e-12);
    assert!((table.pitch_m() - 4.0 * 0.0254).abs() < 1e-12);
    // 3 curves of 4 rows at 0, 10, 20, 30 mph.
    assert_eq!(table.rpm_bounds_at_speed(0.0), Some((3000.0, 9000.0)));
    let top = table.rpm_bounds_at_speed(30.0 * 0.44704).unwrap();
    assert_eq!(top, (3000.0, 9000.0));
    assert!(table.rpm_bounds_at_speed(30.0 * 0.44704 + 0.01).is_none());
    // Static row of the 6000 rpm curve: Ct = 0.11, Cp = 0.045 (synthetic laws).
    let sample = table.coefficients_at(6000.0, 0.0).unwrap();
    assert!((sample.thrust_coefficient - 0.11).abs() < 1e-9);
    assert!((sample.power_coefficient - 0.045).abs() < 1e-9);
    // Between curves the interpolation is linear in rpm.
    let mid = table.coefficients_at(4500.0, 0.0).unwrap();
    assert!((mid.thrust_coefficient - 0.11).abs() < 1e-9);
    assert!(table.coefficients_at(2999.0, 0.0).is_none());
    assert!(table.coefficients_at(9001.0, 0.0).is_none());
    assert!(table.coefficients_at(f64::NAN, 0.0).is_none());
}

#[test]
fn a_file_variant_suffix_keeps_identifiers_unique() {
    let table = parse_per3(T9X45, "PER3_9x45EP(F2B).dat").unwrap();
    assert_eq!(table.propeller_id(), "apc-9x4-5ep-f2b");
    assert_eq!(table.model(), "9x4.5EP");
    let plain = parse_per3(T9X45, "PER3_9x45EP.dat").unwrap();
    assert_eq!(plain.propeller_id(), "apc-9x4-5ep");
}

#[test]
fn crlf_text_and_junk_rows_do_not_change_the_table() {
    let reference = parse_per3(T7X4E, "PER3_7x4E.dat").unwrap();
    let crlf = T7X4E.replace('\n', "\r\n");
    assert_eq!(parse_per3(&crlf, "PER3_7x4E.dat").unwrap(), reference);
    let noisy = T7X4E.replace(
        "(mph)",
        "(mph) text row\n  1.00 NaN 0.1 NaN 0.04\n  2.00 inf 0.1 0.05 inf\n  not numeric at all",
    );
    assert_eq!(parse_per3(&noisy, "PER3_7x4E.dat").unwrap(), reference);
}

#[test]
fn malformed_tables_are_typed_errors_not_panics() {
    for (label, text) in [
        ("empty", String::new()),
        ("no title dims", T7X4E.replace("7x4E", "propeller")),
        (
            "no curves",
            T7X4E.split("PROP RPM").next().unwrap().to_owned(),
        ),
        (
            "one curve",
            T7X4E
                .split("PROP RPM")
                .take(2)
                .collect::<Vec<_>>()
                .join("PROP RPM"),
        ),
        ("unordered speeds", T7X4E.replace("  10.00", "  -5.00")),
        ("duplicate rpm", T7X4E.replace("6000", "3000")),
    ] {
        assert!(parse_per3(&text, "PER3_x.dat").is_err(), "{label} accepted");
    }
}

#[test]
fn a_directory_loads_good_tables_and_reports_the_unusable_files() {
    let dir = scratch("load");
    for (name, text) in [
        ("PER3_7x4E.dat", T7X4E),
        ("PER3_9x45EP(F2B).dat", T9X45),
        ("per3_12x6e.DAT", T12X6E),
        ("PER3_broken.dat", "not a table"),
        ("notes.txt", "ignored"),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let loaded = read_directory(Some(dir.clone()));
    let ids: Vec<&str> = loaded.tables().iter().map(|t| t.propeller_id()).collect();
    assert_eq!(ids, ["apc-12x6e", "apc-7x4e", "apc-9x4-5ep-f2b"]);
    assert_eq!(loaded.skipped().len(), 1);
    assert_eq!(loaded.skipped()[0].0, "PER3_broken.dat");
    let missing = read_directory(Some(dir.join("absent")));
    assert!(missing.tables().is_empty() && missing.skipped().is_empty());
    assert!(read_directory(None).tables().is_empty());
}

#[test]
fn store_validates_before_writing_and_refuses_unsafe_names() {
    let dir = scratch("store");
    assert!(store_per3_file(&dir, "PER3_7x4E.dat", T7X4E.as_bytes()).is_ok());
    assert!(dir.join("PER3_7x4E.dat").is_file());
    assert!(!dir.join("PER3_7x4E.dat.part").exists());
    for bad in [
        "../PER3_a.dat",
        "PER3_a.txt",
        "sub/PER3_a.dat",
        "PER3_a b.dat",
        ".PER3_a.dat",
    ] {
        assert!(
            store_per3_file(&dir, bad, T7X4E.as_bytes()).is_err(),
            "{bad}"
        );
    }
    assert!(store_per3_file(&dir, "PER3_bad.dat", b"junk").is_err());
    assert!(!dir.join("PER3_bad.dat").exists());
}

#[test]
fn importing_a_folder_keeps_only_valid_tables() {
    let source = scratch("import-src");
    let dest = scratch("import-dst");
    std::fs::write(source.join("PER3_7x4E.dat"), T7X4E).unwrap();
    std::fs::write(source.join("PER3_bad.dat"), "junk").unwrap();
    std::fs::write(source.join("readme.txt"), "x").unwrap();
    let report = import_prop_folder(&source, &dest).unwrap();
    assert_eq!(report.imported, ["PER3_7x4E.dat"]);
    assert_eq!(report.rejected.len(), 1);
    let via_dispatch = import_prop_data(&source, &dest).unwrap();
    assert_eq!(via_dispatch.imported, ["PER3_7x4E.dat"]);
    assert!(import_prop_data(&source.join("readme.txt"), &dest).is_err());
}

/// Build a zip by hand: entries are (name, bytes, deflate?).
fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, deflate) in entries {
        let stored = if *deflate {
            miniz_oxide::deflate::compress_to_vec(data, 6)
        } else {
            data.to_vec()
        };
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        let mut local = Vec::new();
        local.extend(b"PK\x03\x04");
        local.extend(
            [20u16, 0, method, 0, 0]
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        local.extend(0u32.to_le_bytes());
        local.extend((stored.len() as u32).to_le_bytes());
        local.extend((data.len() as u32).to_le_bytes());
        local.extend((name.len() as u16).to_le_bytes());
        local.extend(0u16.to_le_bytes());
        local.extend(name.as_bytes());
        local.extend(&stored);
        out.extend(local);
        central.extend(b"PK\x01\x02");
        central.extend(
            [20u16, 20, 0, method, 0, 0]
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        central.extend(0u32.to_le_bytes());
        central.extend((stored.len() as u32).to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u16; 4].iter().flat_map(|v| v.to_le_bytes()));
        central.extend(0u32.to_le_bytes());
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let directory_offset = out.len() as u32;
    let directory_size = central.len() as u32;
    out.extend(central);
    out.extend(b"PK\x05\x06");
    out.extend(
        [0u16, 0, entries.len() as u16, entries.len() as u16]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    out.extend(directory_size.to_le_bytes());
    out.extend(directory_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[test]
fn importing_a_zip_reads_stored_and_deflated_entries_without_escaping_the_folder() {
    let root = scratch("zip");
    let dest = root.join("dest");
    let archive = zip(&[
        ("pack/PER3_7x4E.dat", T7X4E.as_bytes(), true),
        ("pack/PER3_9x45EP(F2B).dat", T9X45.as_bytes(), false),
        ("../PER3_escape.dat", T7X4E.as_bytes(), false),
        ("pack/PER3_bad.dat", b"junk", true),
        ("pack/readme.txt", b"hello", false),
        ("pack/", b"", false),
    ]);
    let archive_path = root.join("props.zip");
    std::fs::write(&archive_path, &archive).unwrap();
    let report = import_prop_zip(&archive_path, &dest).unwrap();
    let mut imported = report.imported.clone();
    imported.sort();
    assert_eq!(
        imported,
        ["PER3_7x4E.dat", "PER3_9x45EP(F2B).dat", "PER3_escape.dat"]
    );
    assert_eq!(report.rejected.len(), 1);
    // The traversal entry was flattened into dest; nothing landed beside it.
    assert!(dest.join("PER3_escape.dat").is_file());
    assert!(!root.join("PER3_escape.dat").exists());
    let reread = std::fs::read_to_string(dest.join("PER3_7x4E.dat")).unwrap();
    assert_eq!(reread, T7X4E);
    assert!(import_prop_data(&archive_path, &root.join("dest2")).is_ok());

    std::fs::write(
        root.join("junk.zip"),
        b"not a zip at all, long enough to scan",
    )
    .unwrap();
    assert!(import_prop_zip(&root.join("junk.zip"), &dest).is_err());
    assert!(import_prop_zip(&root.join("missing.zip"), &dest).is_err());
}

#[derive(Default)]
struct FakeWeb {
    pages: BTreeMap<String, Result<Vec<u8>, String>>,
    log: Vec<(String, String)>,
}

impl PropHttpGet for FakeWeb {
    fn get(&mut self, url: &str, user_agent: &str) -> Result<Vec<u8>, String> {
        self.log.push((url.to_owned(), user_agent.to_owned()));
        self.pages
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err(format!("404 {url}")))
    }
}

#[derive(Default)]
struct FakeClock(Vec<Duration>);

impl Sleeper for FakeClock {
    fn sleep(&mut self, duration: Duration) {
        self.0.push(duration);
    }
}

const ROBOTS_OK: &str = "User-agent: *\nDisallow: /wp-admin/\nAllow: /wp-admin/admin-ajax.php";

fn fake_web(robots: &str) -> FakeWeb {
    let index = r#"<a href="/files/PER2_STATIC-1.DAT">x</a>
        <a href="/files/PER3_7x4E.dat">a</a>
        <a href="https://www.apcprop.com/files/PER3_9x45EP(F2B).dat">b</a>
        <a href="/files/PER3_12x6E.dat">c</a>
        <a href="/files/PER3_7x4E.dat">dup</a>
        <a href="/files/../PER3_evil.dat">traversal</a>
        <a href="https://evil.example/files/PER3_far.dat">other host</a>"#;
    let mut pages = BTreeMap::new();
    pages.insert(PROP_ROBOTS_URL.to_owned(), Ok(robots.as_bytes().to_vec()));
    pages.insert(PROP_INDEX_URL.to_owned(), Ok(index.as_bytes().to_vec()));
    for (name, text) in [
        ("PER3_7x4E.dat", T7X4E),
        ("PER3_9x45EP(F2B).dat", T9X45),
        ("PER3_12x6E.dat", T12X6E),
    ] {
        pages.insert(
            format!("https://www.apcprop.com/files/{name}"),
            Ok(text.as_bytes().to_vec()),
        );
    }
    FakeWeb {
        pages,
        log: Vec::new(),
    }
}

fn options(selection: PropSelection) -> PropFetchOptions {
    PropFetchOptions {
        selection,
        interval: Duration::from_millis(1),
    }
}

fn run(
    web: &mut FakeWeb,
    clock: &mut FakeClock,
    dest: &Path,
    selection: PropSelection,
) -> Result<alas_uav::propulsion_electric::prop_fetch::PropFetchReport, PropFetchError> {
    fetch_prop_tables(web, clock, dest, &options(selection), &mut |_| {})
}

#[test]
fn the_download_is_polite_identifiable_and_resumable() {
    let dest = scratch("fetch");
    let (mut web, mut clock) = (fake_web(ROBOTS_OK), FakeClock::default());
    let report = run(
        &mut web,
        &mut clock,
        &dest,
        PropSelection::Models(vec![
            "7X4e".to_owned(),
            "12x6E".to_owned(),
            "nope".to_owned(),
        ]),
    )
    .unwrap();
    assert_eq!(report.downloaded, ["PER3_7x4E.dat", "PER3_12x6E.dat"]);
    assert_eq!(report.unknown_models, ["nope"]);
    // robots.txt first, then the index, then one request per table.
    let urls: Vec<&str> = web.log.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(urls[0], PROP_ROBOTS_URL);
    assert_eq!(urls[1], PROP_INDEX_URL);
    assert_eq!(urls.len(), 4);
    // Generic identity, no e-mail address; at least 5 s between all requests.
    assert!(web.log.iter().all(|(_, agent)| agent == PROP_USER_AGENT));
    assert!(!PROP_USER_AGENT.contains('@'));
    assert_eq!(clock.0.len(), urls.len() - 1);
    assert!(clock.0.iter().all(|d| *d >= Duration::from_secs(5)));
    // Re-running skips what is installed and makes no table request.
    let (mut web2, mut clock2) = (web_again(), FakeClock::default());
    let again = run(
        &mut web2,
        &mut clock2,
        &dest,
        PropSelection::Models(vec!["7x4E".to_owned()]),
    )
    .unwrap();
    assert!(again.downloaded.is_empty());
    assert_eq!(again.already_present, ["PER3_7x4E.dat"]);
    assert_eq!(web2.log.len(), 2);
}

fn web_again() -> FakeWeb {
    fake_web(ROBOTS_OK)
}

#[test]
fn only_same_host_files_are_ever_selected() {
    let dest = scratch("fetch-all");
    let (mut web, mut clock) = (fake_web(ROBOTS_OK), FakeClock::default());
    let report = run(&mut web, &mut clock, &dest, PropSelection::All).unwrap();
    assert_eq!(report.downloaded.len(), 3);
    assert!(web
        .log
        .iter()
        .all(|(url, _)| url.starts_with("https://www.apcprop.com/")));
    assert!(web.log.iter().all(|(url, _)| !url.contains("evil")));

    let electric_dest = scratch("fetch-electric");
    let (mut web, mut clock) = (web_again(), FakeClock::default());
    let electric = run(
        &mut web,
        &mut clock,
        &electric_dest,
        PropSelection::Electric,
    )
    .unwrap();
    assert_eq!(
        electric.downloaded.len(),
        3,
        "7x4E, 9x4.5EP, 12x6E all contain E"
    );
}

#[test]
fn robots_txt_is_obeyed_and_a_block_makes_no_further_request() {
    let dest = scratch("fetch-robots");
    let (mut web, mut clock) = (
        fake_web("User-agent: *\nDisallow: /files/"),
        FakeClock::default(),
    );
    let error = run(&mut web, &mut clock, &dest, PropSelection::All).unwrap_err();
    assert!(
        matches!(error, PropFetchError::RobotsDisallowed(_)),
        "{error}"
    );
    assert!(!web.log.iter().any(|(url, _)| url.contains("/files/PER3")));
    assert!(std::fs::read_dir(&dest).unwrap().next().is_none());

    let (mut web, mut clock) = (fake_web("User-agent: *\nDisallow: /"), FakeClock::default());
    let error = run(&mut web, &mut clock, &dest, PropSelection::All).unwrap_err();
    assert!(matches!(error, PropFetchError::RobotsDisallowed(_)));
    assert_eq!(web.log.len(), 1, "only robots.txt was requested");
}

#[test]
fn bad_downloads_are_rejected_and_repeated_failure_stops_the_run() {
    let dest = scratch("fetch-fail");
    let (mut web, mut clock) = (fake_web(ROBOTS_OK), FakeClock::default());
    web.pages.insert(
        "https://www.apcprop.com/files/PER3_7x4E.dat".to_owned(),
        Ok(b"<html>captcha</html>".to_vec()),
    );
    let report = run(&mut web, &mut clock, &dest, PropSelection::All).unwrap();
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.downloaded.len(), 2);
    assert!(
        !dest.join("PER3_7x4E.dat").exists(),
        "unparsable data is not kept"
    );

    let dest = scratch("fetch-stop");
    let (mut web, mut clock) = (fake_web(ROBOTS_OK), FakeClock::default());
    for name in ["PER3_7x4E.dat", "PER3_9x45EP(F2B).dat", "PER3_12x6E.dat"] {
        web.pages.insert(
            format!("https://www.apcprop.com/files/{name}"),
            Err("HTTP 429".to_owned()),
        );
    }
    let error = run(&mut web, &mut clock, &dest, PropSelection::All).unwrap_err();
    assert!(
        matches!(error, PropFetchError::TooManyFailures(_)),
        "{error}"
    );

    let (mut web, mut clock) = (FakeWeb::default(), FakeClock::default());
    assert!(matches!(
        run(&mut web, &mut clock, &dest, PropSelection::All),
        Err(PropFetchError::Transport(_))
    ));
    web.pages.insert(
        PROP_ROBOTS_URL.to_owned(),
        Ok(ROBOTS_OK.as_bytes().to_vec()),
    );
    web.pages
        .insert(PROP_INDEX_URL.to_owned(), Ok(b"<html></html>".to_vec()));
    assert!(matches!(
        run(&mut web, &mut clock, &dest, PropSelection::All),
        Err(PropFetchError::EmptyIndex)
    ));
}
