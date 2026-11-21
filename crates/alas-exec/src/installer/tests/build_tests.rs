// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `build_from_source` tools against mock servers: a toy "source" archive is
//! downloaded, built by shell commands, pruned, self-checked and installed.

use super::*;
use std::time::Duration;

const SOURCE_TEXT: &str = "alas-built-ok";

fn source_archive() -> Vec<u8> {
    tar_bytes(&[
        ("src/payload.txt", SOURCE_TEXT.as_bytes(), 0o644),
        ("src/junk/big.txt", b"not kept", 0o644),
        ("src/README.txt", b"readme", 0o644),
    ])
}

/// The toy build: write `build/bin/<tool>` as a runnable script.
fn toy_steps() -> serde_json::Value {
    let name = tool_file_name();
    if cfg!(windows) {
        let line = format!(
            "mkdir build\\bin && (echo @echo off& echo echo {SOURCE_TEXT}) > build\\bin\\{name}"
        );
        json!([{ "program": "cmd", "args": ["/C", line], "timeout_seconds": 60 }])
    } else {
        let line = format!(
            "mkdir -p build/bin && printf '#!/bin/sh\\necho {SOURCE_TEXT}\\n' > build/bin/{name} && chmod +x build/bin/{name}"
        );
        json!([{ "program": "sh", "args": ["-c", line], "timeout_seconds": 60 }])
    }
}

fn build_manifest(url: &str, sha256: &str, build: serde_json::Value) -> Manifest {
    let text = json!({
        "schema": 1,
        "tools": [{
            "id": "builttool", "name": "Built tool", "licence": "test",
            "publisher_url": "https://example.invalid/", "kind": "build_from_source",
            "artifacts": { PLATFORM: {
                "version": "rev1", "url": url, "sha256": sha256,
                "min_bytes": 100, "format": "tar",
                "directory": "BuiltTool", "strip_single_root": true,
                "executable": format!("build/bin/{}", tool_file_name())
            }},
            "build": build
        }]
    })
    .to_string();
    Manifest::parse(&text).expect("manifest")
}

fn plain_build() -> serde_json::Value {
    json!({
        "steps": { PLATFORM: toy_steps() },
        "keep": ["build/bin", "payload.txt"],
        "check": "toy-check"
    })
}

fn with_toy_check(root: &Path, result: Result<&'static str, &'static str>) -> InstallContext {
    context(root).with_check("toy-check", move |dir: &Path| {
        // The check sees the built tool in the staged tree.
        assert!(dir.join("build/bin").join(tool_file_name()).is_file());
        result.map(str::to_owned).map_err(str::to_owned)
    })
}

#[test]
fn a_source_archive_is_built_pruned_checked_and_installed() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-ok");
    let manifest = build_manifest(&mock.url, &hash_of(&archive), plain_build());
    let ctx = with_toy_check(&dir.0, Ok("beam theory matched"));

    let plan = install_plan(&manifest, &ctx, None);
    assert_eq!(plan.items[0].state, PlanItemState::Install);
    let reports = run(&manifest, &ctx);
    let ToolResult::Installed { executable, health } = &reports[0].result else {
        panic!("expected install, got {:?}", reports[0].result);
    };
    assert!(health.passed);
    assert_eq!(health.detail, "beam theory matched");
    let installed = ctx.tools_dir().join("BuiltTool");
    assert_eq!(
        *executable,
        installed.join("build/bin").join(tool_file_name())
    );
    assert!(installed.join("payload.txt").is_file());
    assert!(
        !installed.join("junk").exists(),
        "unlisted source is pruned"
    );
    assert!(!installed.join("README.txt").exists());
    let record = fs::read_to_string(installed.join("ALAS-BUILD.txt")).expect("build record");
    assert!(
        record.contains(&hash_of(&archive)) && record.contains("not redistributable"),
        "{record}"
    );
    assert_eq!(
        install_plan(&manifest, &ctx, None).items[0].state,
        PlanItemState::AlreadyInstalled
    );
}

#[test]
fn a_failing_self_check_discards_the_build_and_restores_the_previous_install() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-check");
    let manifest = build_manifest(&mock.url, &hash_of(&archive), plain_build());
    let ctx = with_toy_check(&dir.0, Err("deflection 31 % off beam theory"));
    let previous = ctx.tools_dir().join("BuiltTool");
    fs::create_dir_all(&previous).expect("previous");
    fs::write(previous.join("marker.txt"), "keep").expect("marker");

    let reports = run(&manifest, &ctx);
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(message.contains("31 % off beam theory"), "{message}");
    assert_eq!(
        fs::read_to_string(previous.join("marker.txt")).expect("kept"),
        "keep"
    );
    assert!(Registry::load(&dir.0).tools.is_empty());
}

#[test]
fn an_unregistered_self_check_fails_closed() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-nocheck");
    let manifest = build_manifest(&mock.url, &hash_of(&archive), plain_build());
    let reports = run(&manifest, &context(&dir.0));
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(
        message.contains("self-check") && message.contains("not available"),
        "{message}"
    );
    assert!(!context(&dir.0).tools_dir().join("BuiltTool").exists());
}

#[test]
fn a_failing_build_command_reports_its_output_and_installs_nothing() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-fail");
    let failing = if cfg!(windows) {
        json!([{ "program": "cmd", "args": ["/C", "echo compiler exploded 1>&2 & exit 3"] }])
    } else {
        json!([{ "program": "sh", "args": ["-c", "echo compiler exploded >&2; exit 3"] }])
    };
    let build = json!({ "steps": { PLATFORM: failing }, "keep": [], "check": "toy-check" });
    let manifest = build_manifest(&mock.url, &hash_of(&archive), build);
    let reports = run(&manifest, &with_toy_check(&dir.0, Ok("unused")));
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(
        message.contains("build failed") && message.contains("compiler exploded"),
        "{message}"
    );
    assert!(!dir.0.join("external tools/BuiltTool").exists());
}

#[test]
fn missing_system_compilers_are_a_guided_state_with_the_hint() {
    let archive = source_archive();
    let dir = TempDir::new("build-missing");
    let build = json!({
        "required_commands": { PLATFORM: ["alas-no-such-compiler-xyz"] },
        "missing_commands_hint": { PLATFORM: "Install it with your package manager." },
        "steps": { PLATFORM: toy_steps() }
    });
    let manifest = build_manifest("http://127.0.0.1:1/x.tar", &hash_of(&archive), build);
    let ctx = context(&dir.0);
    let plan = install_plan(&manifest, &ctx, None);
    assert_eq!(plan.items[0].state, PlanItemState::MissingSystemTools);
    assert!(plan.items[0]
        .steps
        .iter()
        .any(|s| s.contains("alas-no-such-compiler-xyz")));
    assert!(plan.items[0]
        .steps
        .iter()
        .any(|s| s.contains("package manager")));
    let reports = run(&manifest, &ctx);
    assert!(
        matches!(reports[0].result, ToolResult::Guided(_)),
        "{:?}",
        reports[0].result
    );
}

#[test]
fn a_pinned_toolchain_is_downloaded_once_put_on_path_and_reused() {
    let source = source_archive();
    let source_mock = serve(source.clone());
    let tool_name = if cfg!(windows) { "mycc.bat" } else { "mycc.sh" };
    let compiler = tar_bytes(&[(
        &format!("tc/bin/{tool_name}"),
        tool_script("compiled-by-toolchain").as_slice(),
        0o755,
    )]);
    let toolchain_mock = serve(compiler.clone());
    let dir = TempDir::new("build-toolchain");
    // The first step runs the toolchain's compiler, which only PATH can find;
    // the second writes the product the way the plain toy build does.
    let mut steps = vec![json!({ "program": tool_name, "args": [] })];
    steps.extend(toy_steps().as_array().cloned().unwrap_or_default());
    let build = json!({
        "toolchain": { PLATFORM: [{
            "version": "cc-1", "url": toolchain_mock.url, "sha256": hash_of(&compiler),
            "min_bytes": 100, "format": "tar", "directory": "ToyCC",
            "strip_single_root": true, "executable": format!("bin/{tool_name}")
        }] },
        "path_dirs": { PLATFORM: ["bin"] },
        "steps": { PLATFORM: steps },
        "keep": ["build/bin"],
        "check": "toy-check"
    });
    let manifest = build_manifest(&source_mock.url, &hash_of(&source), build);
    let ctx = with_toy_check(&dir.0, Ok("ok"));

    let plan = install_plan(&manifest, &ctx, None);
    assert!(plan.total_download_bytes() > 0);
    let reports = run(&manifest, &ctx);
    assert!(
        matches!(reports[0].result, ToolResult::Installed { .. }),
        "{:?}",
        reports[0].result
    );
    assert!(dir.0.join("toolchains/ToyCC/bin").join(tool_name).is_file());
    assert_eq!(toolchain_mock.ranges.lock().expect("lock").len(), 1);

    // Force a rebuild: the toolchain is reused, not fetched again.
    uninstall_tool(&ctx, "builttool").expect("uninstall");
    let again = run(&manifest, &ctx);
    assert!(
        matches!(again[0].result, ToolResult::Installed { .. }),
        "{:?}",
        again[0].result
    );
    assert_eq!(toolchain_mock.ranges.lock().expect("lock").len(), 1);
}

#[test]
fn a_build_manifest_without_a_build_section_is_refused() {
    let text = json!({ "schema": 1, "tools": [{
        "id": "x", "name": "x", "licence": "t", "publisher_url": "https://e.invalid/",
        "kind": "build_from_source"
    }]})
    .to_string();
    assert!(Manifest::parse(&text).is_err());
}

/// A build step that starts a grandchild, which appends to `heartbeat` once a
/// second for a minute.
fn grandchild_steps(heartbeat: &Path, timeout_seconds: u64) -> serde_json::Value {
    let beat = heartbeat.display().to_string();
    if cfg!(windows) {
        // cmd is the step; ping is its child and writes a line a second.
        let line = format!("ping -n 60 127.0.0.1 > {beat}");
        json!([{ "program": "cmd", "args": ["/C", line], "timeout_seconds": timeout_seconds }])
    } else {
        let line = format!(
            "(i=0; while [ $i -lt 60 ]; do echo x >> '{beat}'; i=$((i+1)); sleep 1; done) & wait"
        );
        json!([{ "program": "sh", "args": ["-c", line], "timeout_seconds": timeout_seconds }])
    }
}

fn heartbeat_len(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |meta| meta.len())
}

/// After the step was killed the grandchild must be gone: its heartbeat stops.
fn assert_grandchild_is_gone(heartbeat: &Path) {
    let first = heartbeat_len(heartbeat);
    assert!(first > 0, "the grandchild never ran");
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        first,
        heartbeat_len(heartbeat),
        "the grandchild is still running after the build was stopped"
    );
}

#[test]
fn a_build_step_past_its_deadline_is_killed_with_its_children() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-deadline");
    let heartbeat = dir.0.join("heartbeat.txt");
    let build = json!({
        "steps": { PLATFORM: grandchild_steps(&heartbeat, 2) },
        "keep": ["build/bin"], "check": "toy-check"
    });
    let manifest = build_manifest(&mock.url, &hash_of(&archive), build);
    let ctx = with_toy_check(&dir.0, Ok("unused"));
    let reports = run(&manifest, &ctx);
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(message.contains("timed out"), "{message}");
    assert_grandchild_is_gone(&heartbeat);
    assert!(!ctx.tools_dir().join("BuiltTool").exists());
}

#[test]
fn cancelling_a_build_kills_the_compiler_tree() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-cancel");
    let heartbeat = dir.0.join("heartbeat.txt");
    let build = json!({
        "steps": { PLATFORM: grandchild_steps(&heartbeat, 600) },
        "keep": ["build/bin"], "check": "toy-check"
    });
    let manifest = build_manifest(&mock.url, &hash_of(&archive), build);
    let ctx = with_toy_check(&dir.0, Ok("unused"));
    let plan = install_plan(&manifest, &ctx, None);
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let beat = heartbeat.clone();
    let canceller = thread::spawn(move || {
        // Cancel once the grandchild is demonstrably running.
        let start = std::time::Instant::now();
        while heartbeat_len(&beat) == 0 && start.elapsed() < Duration::from_secs(60) {
            thread::sleep(Duration::from_millis(200));
        }
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let reports = run_install(&manifest, &ctx, &plan, &consent, &cancel, &mut |_| {}).expect("run");
    canceller.join().expect("canceller");
    assert!(
        matches!(reports[0].result, ToolResult::Cancelled),
        "{:?}",
        reports[0].result
    );
    assert_grandchild_is_gone(&heartbeat);
}

#[test]
fn the_plan_lists_the_source_and_the_toolchain_with_sources_and_reuse() {
    let manifest = Manifest::bundled().expect("bundled manifest");
    let dir = TempDir::new("plan-downloads");
    let mut ctx = InstallContext::new(&dir.0);
    ctx.platform = "windows-x86_64".into();
    let ids = vec!["nastran95".to_owned()];
    let plan = install_plan(&manifest, &ctx, Some(&ids));
    let item = &plan.items[0];
    assert_eq!(item.state, PlanItemState::Install);
    let roles: Vec<&str> = item.downloads.iter().map(|d| d.role.as_str()).collect();
    assert_eq!(roles, ["Source", "Toolchain"]);
    assert!(item.downloads[0].url.contains("codeload.github.com"));
    assert!(item.downloads[1].url.contains("winlibs_mingw"));
    assert!(!item.downloads[1].licence.is_empty());
    assert!(item.downloads[1].describe().contains("winlibs_mingw"));
    assert_eq!(
        item.download_bytes,
        Some(item.downloads.iter().map(|d| d.bytes).sum())
    );
    // A verified earlier extraction of the toolchain is reused and not counted.
    let toolchain = &manifest
        .tool("nastran95")
        .unwrap()
        .build
        .as_ref()
        .unwrap()
        .toolchain["windows-x86_64"][0];
    let target = dir.0.join("toolchains").join(&toolchain.directory);
    fs::create_dir_all(target.join("bin")).expect("toolchain dir");
    fs::write(target.join(&toolchain.executable), b"x").expect("exe");
    fs::write(
        target.join(".alas-toolchain"),
        toolchain.sha256.as_deref().unwrap(),
    )
    .expect("marker");
    let again = install_plan(&manifest, &ctx, Some(&ids));
    assert!(again.items[0].downloads[1].reused);
    // The estimate drops the toolchain extraction when it is reused.
    assert_eq!(
        item.build_disk_bytes.unwrap() - again.items[0].build_disk_bytes.unwrap(),
        toolchain.extracted_bytes.unwrap()
    );
    assert_eq!(
        again.items[0].download_bytes,
        Some(again.items[0].downloads[0].bytes)
    );
}

#[test]
fn a_changed_build_recipe_reads_as_not_installed() {
    let archive = source_archive();
    let mock = serve(archive.clone());
    let dir = TempDir::new("build-identity");
    let manifest = build_manifest(&mock.url, &hash_of(&archive), plain_build());
    let ctx = with_toy_check(&dir.0, Ok("ok"));
    let reports = run(&manifest, &ctx);
    assert!(matches!(reports[0].result, ToolResult::Installed { .. }));
    assert_eq!(
        install_plan(&manifest, &ctx, None).items[0].state,
        PlanItemState::AlreadyInstalled
    );
    // Same revision, different keep list: a different build.
    let mut changed = plain_build();
    changed["keep"] = json!(["build/bin"]);
    let manifest = build_manifest(&mock.url, &hash_of(&archive), changed);
    assert_eq!(
        install_plan(&manifest, &ctx, None).items[0].state,
        PlanItemState::Install
    );
}
