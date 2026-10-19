// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shipped examples end to end. The process example needs a Python
//! interpreter: set `ALAS_TEST_PYTHON` to its path to run it; without the
//! variable the test is skipped, so CI does not depend on Python.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use alas_addons::{discover, install_examples, run_addon, AddonSettings};

/// Raymer Eq. 15.25 evaluated independently (Python float64, constants
/// 1 lb = 0.45359237 kg, 1 ft = 0.3048 m) for the default inputs.
const EXPECTED_LB: f64 = 13_000.152_156_573_784;
const KG_PER_LB: f64 = 0.453_592_37;

fn examples_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("alas-addons-it-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    install_examples(&dir).unwrap();
    dir
}

fn find<'a>(found: &'a alas_addons::Discovery, id: &str) -> &'a alas_addons::InstalledAddon {
    found.addons.iter().find(|addon| addon.id == id).unwrap()
}

#[test]
fn the_rhai_example_reproduces_the_independent_evaluation() {
    let dir = examples_dir("rhai");
    let found = discover(&dir);
    let addon = find(&found, "raymer_wing_weight");
    let outcome = run_addon(
        addon,
        &BTreeMap::new(),
        &serde_json::json!({}),
        &AddonSettings::default(),
    )
    .unwrap();
    let lb = outcome.outputs["wing_mass_lb"];
    // The 1 ft2 = 10.7639 conversion differs from the independent value only
    // through rounding of the pi-free constants; the agreement is far below
    // the equation's own accuracy.
    assert!((lb - EXPECTED_LB).abs() / EXPECTED_LB < 1e-6, "{lb}");
    assert!((outcome.outputs["wing_mass_kg"] - lb * KG_PER_LB).abs() < 1e-6 * lb);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_process_addon_is_refused_until_enabled_and_round_trips_json() {
    let Some(python) = std::env::var_os("ALAS_TEST_PYTHON") else {
        eprintln!("skipped: ALAS_TEST_PYTHON is not set");
        return;
    };
    let dir = examples_dir("process");
    let found = discover(&dir);
    let addon = find(&found, "raymer_wing_weight_py");
    let mut settings = AddonSettings {
        python: Some(PathBuf::from(python).to_string_lossy().into_owned()),
        ..AddonSettings::default()
    };
    let design = serde_json::json!({"geometry": {"area_m2": 100.0}});

    let refused = run_addon(addon, &BTreeMap::new(), &design, &settings);
    assert!(refused.is_err(), "a process add-on ran while disabled");

    settings.set_enabled(&addon.id, &addon.manifest.version, true);
    let outcome = run_addon(addon, &BTreeMap::new(), &design, &settings).unwrap();
    let lb = outcome.outputs["wing_mass_lb"];
    assert!((lb - EXPECTED_LB).abs() / EXPECTED_LB < 1e-9, "{lb}");
    assert!(outcome.log.contains("protocol"));
    assert_eq!(outcome.messages.len(), 1);

    let rhai = run_addon(
        find(&found, "raymer_wing_weight"),
        &BTreeMap::new(),
        &design,
        &AddonSettings::default(),
    )
    .unwrap();
    assert!((rhai.outputs["wing_mass_lb"] - lb).abs() / lb < 1e-6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_failures_are_reported_not_hung() {
    let Some(python) = std::env::var_os("ALAS_TEST_PYTHON") else {
        eprintln!("skipped: ALAS_TEST_PYTHON is not set");
        return;
    };
    let python = PathBuf::from(python);
    let dir = std::env::temp_dir().join(format!("alas-addons-it-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cases = [
        ("hang", "import time\ntime.sleep(60)\n", 0.5, "timed out"),
        ("exit", "import sys\nsys.exit(3)\n", 5.0, "exit status"),
        ("junk", "print('not json')\n", 5.0, "not valid JSON"),
        (
            "missing",
            "print('{\"outputs\": {}}')\n",
            5.0,
            "missing output",
        ),
        (
            "nan",
            "print('{\"outputs\": {\"y\": null}}')\n",
            5.0,
            "not a number",
        ),
    ];
    for (id, body, timeout, _expect) in cases {
        let folder = dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("main.py"), body).unwrap();
        std::fs::write(
            folder.join("addon.toml"),
            format!(
                "name=\"{id}\"\nversion=\"1\"\nkind=\"process\"\nentry=\"main.py\"\ntimeout_s={timeout}\n[[outputs]]\nname=\"y\"\n"
            ),
        )
        .unwrap();
    }
    let found = discover(&dir);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    let mut settings = AddonSettings {
        python: Some(python.to_string_lossy().into_owned()),
        ..AddonSettings::default()
    };
    for (id, _, _, expect) in cases {
        let addon = find(&found, id);
        settings.set_enabled(&addon.id, "1", true);
        let started = std::time::Instant::now();
        let error = run_addon(addon, &BTreeMap::new(), &serde_json::json!({}), &settings)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expect), "{id}: {error}");
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_interpreter_is_a_clean_error() {
    let dir = examples_dir("nopython");
    let found = discover(&dir);
    let addon = find(&found, "raymer_wing_weight_py");
    let mut settings = AddonSettings {
        python: Some("definitely-not-a-python-interpreter".into()),
        ..AddonSettings::default()
    };
    settings.set_enabled(&addon.id, &addon.manifest.version, true);
    let error = run_addon(addon, &BTreeMap::new(), &serde_json::json!({}), &settings)
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot start"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_timeout_kills_the_grandchild_too() {
    let Some(python) = std::env::var_os("ALAS_TEST_PYTHON") else {
        eprintln!("skipped: ALAS_TEST_PYTHON is not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("alas-addons-it-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = dir.join("tree");
    std::fs::create_dir_all(&folder).unwrap();
    // The grandchild appends to a heartbeat file every 100 ms; the parent
    // starts it and then sleeps past the timeout.
    std::fs::write(
        folder.join("child.py"),
        "import sys, time\nwhile True:\n    open(sys.argv[1], 'a').write('x')\n    time.sleep(0.1)\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("main.py"),
        "import subprocess, sys, time\nsubprocess.Popen([sys.executable, 'child.py', sys.argv[0] + '.beat'])\ntime.sleep(60)\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("addon.toml"),
        "name=\"tree\"\nversion=\"1\"\nkind=\"process\"\nentry=\"main.py\"\ntimeout_s=2.0\n[[outputs]]\nname=\"y\"\n",
    )
    .unwrap();
    let found = discover(&dir);
    let addon = find(&found, "tree");
    let mut settings = AddonSettings {
        python: Some(PathBuf::from(python).to_string_lossy().into_owned()),
        ..AddonSettings::default()
    };
    settings.set_enabled("tree", "1", true);
    let error = run_addon(addon, &BTreeMap::new(), &serde_json::json!({}), &settings)
        .unwrap_err()
        .to_string();
    assert!(error.contains("timed out"), "{error}");

    let beat_file = folder.join("main.py.beat");
    let first = std::fs::metadata(&beat_file).map(|m| m.len()).unwrap_or(0);
    assert!(first > 0, "the grandchild never started");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let second = std::fs::metadata(&beat_file).map(|m| m.len()).unwrap();
    assert_eq!(first, second, "the grandchild survived the timeout");
    let _ = std::fs::remove_dir_all(&dir);
}
