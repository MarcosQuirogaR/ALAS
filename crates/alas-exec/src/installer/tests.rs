// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installer tests against a mock HTTP server on the loopback interface.
//!
//! Nothing here touches the real network: archives are built in memory and
//! served from `127.0.0.1`, which only a test build may use over plain HTTP.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::json;

use super::engine::ToolResult;
use super::engine::{run_install, uninstall_tool, InstallContext};
use super::extract::{entry_name_is_safe, link_stays_inside};
use super::manifest::Manifest;
use super::plan::PlanItemState;
use super::plan::{install_plan, ConsentReceipt};
use super::registry::Registry;
use super::transfer::{validate_url, TransferOptions};
use crate::sha256::Sha256;

const PLATFORM: &str = "test-platform";
const OK_TEXT: &str = "alas-health-ok";

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "alas-installer-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tool_file_name() -> &'static str {
    if cfg!(windows) {
        "tool.bat"
    } else {
        "tool.sh"
    }
}

fn tool_script(output: &str) -> Vec<u8> {
    if cfg!(windows) {
        format!("@echo off\r\necho {output}\r\n").into_bytes()
    } else {
        format!("#!/bin/sh\necho {output}\n").into_bytes()
    }
}

/// Minimal ustar writer: `(name, content, mode)` regular files.
fn tar_bytes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content, mode) in entries {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(format!("{mode:07o}\0").as_bytes());
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        header[124..136].copy_from_slice(format!("{:011o}\0", content.len()).as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

fn good_archive(output: &str) -> Vec<u8> {
    let path = format!("pkg/{}", tool_file_name());
    tar_bytes(&[(path.as_str(), tool_script(output).as_slice(), 0o755)])
}

fn hash_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize_hex()
}

/// Serves one body; records the `Range` start of every request.
struct Mock {
    url: String,
    ranges: Arc<Mutex<Vec<Option<u64>>>>,
}

fn serve(body: Vec<u8>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&ranges);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle(stream, &body, &seen);
        }
    });
    Mock {
        url: format!("http://127.0.0.1:{port}/tool.tar"),
        ranges,
    }
}

fn handle(mut stream: TcpStream, body: &[u8], seen: &Mutex<Vec<Option<u64>>>) {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        request.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&request).to_lowercase();
    let start = text
        .lines()
        .find_map(|line| line.strip_prefix("range: bytes="))
        .and_then(|rest| rest.split('-').next())
        .and_then(|digits| digits.parse::<u64>().ok());
    if let Ok(mut guard) = seen.lock() {
        guard.push(start);
    }
    let total = body.len() as u64;
    let head = match start {
        Some(from) if from < total => format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{total}\r\nConnection: close\r\n\r\n",
            total - from,
            total - 1
        ),
        Some(_) => "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        None => format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"),
    };
    let _ = stream.write_all(head.as_bytes());
    if let Some(from) = start.filter(|&from| from < total) {
        let _ = stream.write_all(&body[from as usize..]);
    } else if start.is_none() {
        let _ = stream.write_all(body);
    }
}

fn manifest_for(url: &str, sha256: &str, min_bytes: u64, expect: &str) -> Manifest {
    let text = json!({
        "schema": 1,
        "tools": [{
            "id": "faketool", "name": "Fake tool", "licence": "test",
            "publisher_url": "https://example.invalid/", "kind": "automatic",
            "artifacts": { PLATFORM: {
                "version": "1.0", "url": url, "sha256": sha256,
                "min_bytes": min_bytes, "format": "tar",
                "directory": "FakeTool-1.0", "strip_single_root": true,
                "executable": tool_file_name(),
                "health": { "args": [], "expect_stdout_contains": expect, "timeout_seconds": 30 }
            }}
        }]
    })
    .to_string();
    Manifest::parse(&text).expect("manifest")
}

fn context(root: &Path) -> InstallContext {
    InstallContext {
        data_root: root.to_path_buf(),
        platform: PLATFORM.into(),
        options: TransferOptions {
            allow_loopback_http: true,
        },
        checks: super::build::SelfChecks::default(),
    }
}

fn run(manifest: &Manifest, ctx: &InstallContext) -> Vec<super::engine::ToolReport> {
    let plan = install_plan(manifest, ctx, None);
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = AtomicBool::new(false);
    run_install(manifest, ctx, &plan, &consent, &cancel, &mut |_| {}).expect("run")
}

#[test]
fn installs_verifies_registers_and_is_then_up_to_date() {
    let archive = good_archive(OK_TEXT);
    let mock = serve(archive.clone());
    let dir = TempDir::new("ok");
    let manifest = manifest_for(&mock.url, &hash_of(&archive), 100, OK_TEXT);
    let ctx = context(&dir.0);

    let reports = run(&manifest, &ctx);
    let ToolResult::Installed { executable, health } = &reports[0].result else {
        panic!("expected install, got {:?}", reports[0].result);
    };
    assert!(health.passed, "{}", health.detail);
    assert_eq!(
        *executable,
        ctx.tools_dir().join("FakeTool-1.0").join(tool_file_name())
    );
    let registry = Registry::load(&dir.0);
    assert!(registry.tools["faketool"].healthy);
    assert_eq!(
        registry.executable(&dir.0, "faketool").as_deref(),
        Some(executable.as_path())
    );
    assert!(!dir
        .0
        .join("downloads/tools")
        .read_dir()
        .expect("cache")
        .any(|_| true));

    let again = install_plan(&manifest, &ctx, None);
    assert_eq!(again.items[0].state, PlanItemState::AlreadyInstalled);
}

#[test]
fn an_interrupted_download_resumes_with_a_range_request() {
    let archive = good_archive(OK_TEXT);
    let mock = serve(archive.clone());
    let dir = TempDir::new("resume");
    let hash = hash_of(&archive);
    let manifest = manifest_for(&mock.url, &hash, 100, OK_TEXT);
    let ctx = context(&dir.0);
    let half = archive.len() / 2;
    let cache = dir.0.join("downloads/tools");
    fs::create_dir_all(&cache).expect("cache");
    fs::write(cache.join(format!("{hash}.download")), &archive[..half]).expect("seed part");

    let reports = run(&manifest, &ctx);
    assert!(
        matches!(reports[0].result, ToolResult::Installed { .. }),
        "{:?}",
        reports[0].result
    );
    let seen = mock.ranges.lock().expect("lock").clone();
    assert_eq!(seen, vec![Some(half as u64)]);
}

/// A server that never honours `Range` (codeload): always the whole body, 200.
fn serve_ignoring_ranges(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}/tool.tar")
}

#[test]
fn a_download_resumed_against_a_server_that_ignores_ranges_restarts_from_zero() {
    let archive = good_archive(OK_TEXT);
    let url = serve_ignoring_ranges(archive.clone());
    let dir = TempDir::new("noranges");
    let hash = hash_of(&archive);
    let manifest = manifest_for(&url, &hash, 100, OK_TEXT);
    let ctx = context(&dir.0);
    let cache = dir.0.join("downloads/tools");
    fs::create_dir_all(&cache).expect("cache");
    fs::write(
        cache.join(format!("{hash}.download")),
        &archive[..archive.len() / 2],
    )
    .expect("seed part");

    let reports = run(&manifest, &ctx);
    assert!(
        matches!(reports[0].result, ToolResult::Installed { .. }),
        "{:?}",
        reports[0].result
    );
}

#[test]
fn a_wrong_hash_discards_the_download_and_installs_nothing() {
    let archive = good_archive(OK_TEXT);
    let mock = serve(archive);
    let dir = TempDir::new("badhash");
    let manifest = manifest_for(&mock.url, &"0".repeat(64), 100, OK_TEXT);
    let ctx = context(&dir.0);

    let reports = run(&manifest, &ctx);
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(message.contains("SHA-256 mismatch"), "{message}");
    assert!(!ctx.tools_dir().join("FakeTool-1.0").exists());
    assert!(Registry::load(&dir.0).tools.is_empty());
    assert_eq!(
        fs::read_dir(dir.0.join("downloads/tools"))
            .expect("cache")
            .count(),
        0
    );
}

#[test]
fn a_path_traversal_entry_is_rejected_before_anything_is_written() {
    let evil = tar_bytes(&[("../evil.txt", b"x", 0o644), ("pkg/tool", b"x", 0o755)]);
    let mock = serve(evil.clone());
    let dir = TempDir::new("evil");
    let manifest = manifest_for(&mock.url, &hash_of(&evil), 10, OK_TEXT);
    let ctx = context(&dir.0);

    let reports = run(&manifest, &ctx);
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(message.contains("unsafe entry name"), "{message}");
    assert!(!dir.0.join("tools-staging/evil.txt").exists());
    assert!(!dir.0.join("evil.txt").exists());
}

#[test]
fn a_failed_health_check_restores_the_previous_install() {
    let archive = good_archive("something else");
    let mock = serve(archive.clone());
    let dir = TempDir::new("rollback");
    let manifest = manifest_for(&mock.url, &hash_of(&archive), 100, OK_TEXT);
    let ctx = context(&dir.0);
    let previous = ctx.tools_dir().join("FakeTool-1.0");
    fs::create_dir_all(&previous).expect("previous");
    fs::write(previous.join("marker.txt"), "keep").expect("marker");

    let reports = run(&manifest, &ctx);
    let ToolResult::Failed(message) = &reports[0].result else {
        panic!("expected failure, got {:?}", reports[0].result);
    };
    assert!(message.contains("health check failed"), "{message}");
    assert_eq!(
        fs::read_to_string(previous.join("marker.txt")).expect("restored"),
        "keep"
    );
    assert!(!previous.join(tool_file_name()).exists());
}

#[test]
fn consent_for_a_different_plan_is_refused() {
    let archive = good_archive(OK_TEXT);
    let dir = TempDir::new("consent");
    let ctx = context(&dir.0);
    let shown = manifest_for("http://127.0.0.1:1/a.tar", &hash_of(&archive), 100, OK_TEXT);
    let swapped = manifest_for("http://127.0.0.1:1/b.tar", &hash_of(&archive), 100, OK_TEXT);
    let plan = install_plan(&shown, &ctx, None);
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = AtomicBool::new(false);
    assert!(run_install(&swapped, &ctx, &plan, &consent, &cancel, &mut |_| {}).is_err());
    let other_plan = install_plan(&swapped, &ctx, None);
    assert!(run_install(&swapped, &ctx, &other_plan, &consent, &cancel, &mut |_| {}).is_err());
}

#[test]
fn only_https_or_test_loopback_sources_are_accepted() {
    let strict = TransferOptions::default();
    let loopback = TransferOptions {
        allow_loopback_http: true,
    };
    assert!(validate_url("https://example.com/a.zip", strict).is_ok());
    assert!(validate_url("http://example.com/a.zip", strict).is_err());
    assert!(validate_url("http://127.0.0.1:80/a.zip", strict).is_err());
    assert!(validate_url("http://127.0.0.1:8080/a.zip", loopback).is_ok());
    assert!(validate_url("http://127.0.0.1.evil.com/a.zip", loopback).is_err());
    assert!(validate_url("http://localhost@evil.com/a.zip", loopback).is_err());
    assert!(validate_url("ftp://127.0.0.1/a.zip", loopback).is_err());
}

#[test]
fn archive_names_and_links_must_stay_inside() {
    assert!(entry_name_is_safe("pkg/bin/tool.exe"));
    assert!(entry_name_is_safe("./pkg/a"));
    assert!(!entry_name_is_safe("../evil"));
    assert!(!entry_name_is_safe("pkg/../../evil"));
    assert!(!entry_name_is_safe("/etc/passwd"));
    assert!(!entry_name_is_safe("C:/Windows/x"));
    assert!(!entry_name_is_safe("pkg\\..\\evil"));
    let root = Path::new("stage");
    assert!(link_stays_inside(
        root,
        &root.join("lib/libx.so"),
        Path::new("libx.so.1")
    ));
    assert!(link_stays_inside(
        root,
        &root.join("lib/libx.so"),
        Path::new("../bin/x")
    ));
    assert!(!link_stays_inside(
        root,
        &root.join("lib/libx.so"),
        Path::new("../../outside")
    ));
    assert!(!link_stays_inside(
        root,
        &root.join("lib/libx.so"),
        Path::new("/etc/passwd")
    ));
}

#[test]
fn the_bundled_manifest_is_consistent_and_fails_closed() {
    let manifest = Manifest::bundled().expect("bundled manifest");
    let script = include_str!("../../../../tools/setup_openvsp_preview.ps1").to_lowercase();
    for tool in &manifest.tools {
        assert!(tool.publisher_url.starts_with("https://"), "{}", tool.id);
        match tool.kind {
            super::InstallKind::Automatic => {
                assert!(!tool.artifacts.is_empty(), "{}", tool.id);
                for artifact in tool.artifacts.values() {
                    assert!(artifact.url.starts_with("https://"), "{}", tool.id);
                    assert!(artifact.sha256.is_some(), "{} must be hash-pinned", tool.id);
                    assert!(artifact.health.is_some(), "{} needs a tiny case", tool.id);
                }
            }
            super::InstallKind::PendingReview => {
                assert!(
                    tool.artifacts.is_empty(),
                    "{} must not carry an unreviewed URL",
                    tool.id
                );
            }
            super::InstallKind::Guided => assert!(!tool.guided_steps.is_empty(), "{}", tool.id),
            super::InstallKind::BuildFromSource => {
                let build = tool.build.as_ref().expect("a build section");
                for (platform, artifact) in &tool.artifacts {
                    assert!(artifact.url.starts_with("https://"), "{}", tool.id);
                    assert!(
                        artifact.sha256.is_some(),
                        "{} source must be hash-pinned",
                        tool.id
                    );
                    assert!(
                        build.steps.contains_key(platform),
                        "{} needs build steps for {platform}",
                        tool.id
                    );
                    for part in build.toolchain.get(platform).into_iter().flatten() {
                        assert!(
                            part.url.starts_with("https://") && part.sha256.is_some(),
                            "{}",
                            tool.id
                        );
                    }
                }
            }
        }
    }
    // The OpenVSP pin is the one the reviewed setup script already carries.
    let openvsp = manifest.tool("openvsp").expect("openvsp");
    let pinned = openvsp.artifacts["windows-x86_64"]
        .sha256
        .as_deref()
        .expect("pin");
    assert!(script.contains(pinned));
    for guided in ["mses", "msc-nastran", "openfoam", "flowunsteady"] {
        assert_eq!(
            manifest.tool(guided).expect(guided).kind,
            super::InstallKind::Guided
        );
    }
}

#[test]
fn guided_and_pending_tools_never_download() {
    let manifest = Manifest::bundled().expect("bundled manifest");
    let dir = TempDir::new("guided");
    let mut ctx = InstallContext::new(&dir.0);
    ctx.platform = "windows-x86_64".into();
    let ids: Vec<String> = ["mses", "openfoam"].map(String::from).to_vec();
    let plan = install_plan(&manifest, &ctx, Some(&ids));
    assert_eq!(plan.downloads().count(), 0);
    assert_eq!(plan.total_download_bytes(), 0);
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = AtomicBool::new(false);
    let reports = run_install(&manifest, &ctx, &plan, &consent, &cancel, &mut |_| {}).expect("run");
    assert!(reports
        .iter()
        .all(|r| matches!(r.result, ToolResult::Guided(_))));
    assert!(!ctx.tools_dir().exists());
    let all = install_plan(&manifest, &ctx, None);
    let openvsp = all
        .items
        .iter()
        .find(|item| item.id == "openvsp")
        .expect("openvsp");
    assert_eq!(openvsp.state, PlanItemState::Install);
    for id in ["gmsh", "paraview"] {
        let item = all.items.iter().find(|item| item.id == id).expect(id);
        assert_eq!(item.state, PlanItemState::Install, "{id}");
    }
}

#[test]
fn uninstall_removes_the_directory_and_record_but_refuses_foreign_paths() {
    let archive = good_archive(OK_TEXT);
    let mock = serve(archive.clone());
    let dir = TempDir::new("uninstall");
    let manifest = manifest_for(&mock.url, &hash_of(&archive), 100, OK_TEXT);
    let ctx = context(&dir.0);
    run(&manifest, &ctx);
    assert!(uninstall_tool(&ctx, "faketool").expect("uninstall"));
    assert!(!ctx.tools_dir().join("FakeTool-1.0").exists());
    assert!(Registry::load(&dir.0).tools.is_empty());
    assert!(!uninstall_tool(&ctx, "faketool").expect("second"));

    let mut registry = Registry::default();
    registry.tools.insert(
        "evil".into(),
        super::registry::InstalledRecord {
            version: "1".into(),
            directory: "../outside".into(),
            executable: "x".into(),
            archive_sha256: String::new(),
            url: String::new(),
            installed_at: 0,
            healthy: true,
        },
    );
    registry.save(&dir.0).expect("save");
    assert!(uninstall_tool(&ctx, "evil").is_err());
}

#[test]
fn an_installed_openvsp_layout_is_found_by_the_existing_locator() {
    let dir = TempDir::new("locator");
    let install = dir.0.join("external tools/OpenVSP-3.51.2-win64");
    fs::create_dir_all(&install).expect("dir");
    fs::write(install.join("vspscript.exe"), b"x").expect("exe");
    let app = TempDir::new("app");
    let locator = crate::ToolLocator::new(&app.0, &dir.0);
    let found = locator.discover_openvsp(Path::new(""));
    assert_eq!(
        found,
        crate::ExecutableDiscovery::Ready(install.join("vspscript.exe"))
    );
}

#[cfg(windows)]
#[test]
fn zip_archives_extract_through_the_system_tar() {
    let source = TempDir::new("zipsrc");
    fs::create_dir_all(source.0.join("pkg")).expect("pkg");
    fs::write(
        source.0.join("pkg").join(tool_file_name()),
        tool_script(OK_TEXT),
    )
    .expect("tool");
    let zip = source.0.join("out.zip");
    let tar =
        Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot")).join("System32/tar.exe");
    let status = std::process::Command::new(tar)
        .args(["-a", "-cf"])
        .arg(&zip)
        .arg("-C")
        .arg(&source.0)
        .arg("pkg")
        .status()
        .expect("tar");
    assert!(status.success());
    let staging = TempDir::new("zipout");
    super::extract::extract_archive(&zip, super::ArchiveFormat::Zip, &staging.0).expect("extract");
    assert!(staging.0.join("pkg").join(tool_file_name()).is_file());
}

/// Maintainer check against the real, already downloaded OpenVSP archive:
/// `ALAS_TEST_OPENVSP_ZIP=<path> cargo test -p alas-exec -- --ignored real_openvsp`.
#[cfg(windows)]
#[test]
#[ignore = "needs the real OpenVSP zip (ALAS_TEST_OPENVSP_ZIP)"]
fn real_openvsp_archive_installs_and_passes_its_health_check() {
    let path = std::env::var("ALAS_TEST_OPENVSP_ZIP").expect("ALAS_TEST_OPENVSP_ZIP");
    let mock = serve(fs::read(path).expect("read zip"));
    let mut manifest = Manifest::bundled().expect("bundled");
    let tool = manifest
        .tools
        .iter_mut()
        .find(|t| t.id == "openvsp")
        .expect("openvsp");
    let artifact = tool.artifacts.remove("windows-x86_64").expect("artifact");
    let mut artifact = artifact;
    artifact.url = mock.url.clone();
    tool.artifacts.insert(PLATFORM.into(), artifact);
    let dir = TempDir::new("realvsp");
    let reports = run(&manifest, &context(&dir.0));
    let report = reports.iter().find(|r| r.id == "openvsp").expect("report");
    let ToolResult::Installed { executable, health } = &report.result else {
        panic!("{:?}", report.result);
    };
    assert!(health.passed, "{}", health.detail);
    assert!(executable.ends_with(Path::new("OpenVSP-3.51.2-win64").join("vspscript.exe")));
}

mod build_tests;

#[test]
fn a_dangling_link_is_checked_by_where_it_would_end_up() {
    let dir = TempDir::new("dangling");
    let root = dir.0.join("root");
    fs::create_dir_all(root.join("a")).expect("root");
    let base = fs::canonicalize(&root).expect("canonical root");
    let link = root.join("a").join("l");
    // The link itself does not exist: resolution starts from its real parent.
    assert!(super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("../b/c")
    ));
    assert!(!super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("../../outside")
    ));
    assert!(!super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("/etc")
    ));
}

#[test]
fn a_dangling_link_through_a_parent_directory_cannot_escape() {
    let dir = TempDir::new("dangling-up");
    let root = dir.0.join("root");
    fs::create_dir_all(root.join("a/b")).expect("tree");
    let base = fs::canonicalize(&root).expect("canonical root");
    let link = root.join("a").join("l");
    // Real directories: `..` means what it says, and anything past a missing name
    // that still contains `..` is refused.
    assert!(super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("b/../../x")
    ));
    assert!(!super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("../../x")
    ));
    assert!(!super::extract::link_resolves_inside(
        &base,
        &link,
        Path::new("missing/../../..")
    ));
}
